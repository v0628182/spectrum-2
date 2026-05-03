//! Audio output render pipeline.
//!
//! Takes captured audio from the radar loopback and plays it through the
//! user-selected output device via WASAPI Shared Mode.
//!
//! The output device GUID is stored in `HKCU\SOFTWARE\VanySound\OutputEndpointGuid`.
//! When the user changes the device, the render thread is restarted.

use crossbeam_channel::Receiver;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::anyhow;

/// Audio packet: (f32 samples interleaved, channel count, channel mask)
pub type AudioPacket = (Vec<f32>, u16, u32);

const WAVE_FORMAT_PCM: u16 = 0x0001;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

#[derive(Debug, Clone, Copy)]
enum OutputSampleFormat {
    Float32,
    Pcm16,
    Pcm24,
    Pcm32,
}

const OUTPUT_GUID_REGISTRY_KEY: &str = "OutputEndpointGuid";
const REGISTRY_PATH: &str = r"SOFTWARE\VanySound";

// ─── Public API ────────────────────────────────────────────────

/// Spawn the render thread. Returns a handle for control + a JoinHandle for joining.
pub fn spawn_render_thread(
    rx: Receiver<AudioPacket>,
) -> (RenderHandle, std::thread::JoinHandle<()>) {
    let active = Arc::new(AtomicBool::new(true));
    let restart_signal = Arc::new(AtomicBool::new(false));

    let handle = RenderHandle {
        active: Arc::clone(&active),
        restart_signal: Arc::clone(&restart_signal),
    };

    let active_flag = Arc::clone(&active);
    let restart_flag = Arc::clone(&restart_signal);

    let join_handle = thread::Builder::new()
        .name("audio-render".into())
        .spawn(move || {
            render_supervisor(rx, active_flag, restart_flag);
        })
        .expect("Failed to spawn audio render thread");

    (handle, join_handle)
}

/// Handle to control the render thread from outside.
#[derive(Clone)]
pub struct RenderHandle {
    active: Arc<AtomicBool>,
    restart_signal: Arc<AtomicBool>,
}

impl RenderHandle {
    /// Signal the render thread to restart with a new output device.
    pub fn restart(&self) {
        self.restart_signal.store(true, Ordering::SeqCst);
    }

    /// Signal the render thread to stop permanently.
    pub fn stop(&self) {
        self.active.store(false, Ordering::SeqCst);
    }
}

// ─── Supervisor Loop ───────────────────────────────────────────
// Restarts the render session whenever restart_signal is set.

fn render_supervisor(
    rx: Receiver<AudioPacket>,
    active: Arc<AtomicBool>,
    restart_signal: Arc<AtomicBool>,
) {
    tracing::info!("audio-render supervisor started");

    // COM init ONCE per thread — not per session
    unsafe {
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_MULTITHREADED,
        );
    }

    while active.load(Ordering::SeqCst) {
        let output_guid = read_output_endpoint_guid();
        match &output_guid {
            Some(guid) => tracing::info!(guid = %guid, "audio-render: starting render session"),
            None => tracing::info!("audio-render: no output device configured, waiting..."),
        }

        restart_signal.store(false, Ordering::SeqCst);

        if let Some(guid) = output_guid {
            match render_session(&rx, &guid, &active, &restart_signal) {
                Ok(()) => tracing::info!("audio-render session ended gracefully"),
                Err(err) => tracing::error!(error = %err, "audio-render session error"),
            }
        } else {
            wait_for_signal_or_timeout(&active, &restart_signal, Duration::from_secs(2));
        }

        drain_channel(&rx);
    }

    // COM uninit ONCE per thread
    unsafe {
        windows::Win32::System::Com::CoUninitialize();
    }
    tracing::info!("audio-render supervisor exiting");
}

fn wait_for_signal_or_timeout(active: &AtomicBool, restart: &AtomicBool, timeout: Duration) {
    let start = std::time::Instant::now();
    while active.load(Ordering::SeqCst)
        && !restart.load(Ordering::SeqCst)
        && start.elapsed() < timeout
    {
        thread::sleep(Duration::from_millis(50));
    }
}

fn drain_channel(rx: &Receiver<AudioPacket>) {
    while rx.try_recv().is_ok() {}
}

// ─── WASAPI Render Session ─────────────────────────────────────

fn render_session(
    rx: &Receiver<AudioPacket>,
    output_guid: &str,
    active: &AtomicBool,
    restart_signal: &AtomicBool,
) -> anyhow::Result<()> {
    use windows::Win32::Media::Audio::*;

    unsafe {
        // COM is already initialized by the supervisor — no CoInitializeEx here

        let enumerator: IMMDeviceEnumerator = windows::Win32::System::Com::CoCreateInstance(
            &MMDeviceEnumerator,
            None,
            windows::Win32::System::Com::CLSCTX_ALL,
        )
        .map_err(|e| anyhow!("CoCreateInstance: {}", e))?;

        let device = find_device_by_guid(&enumerator, output_guid).ok_or_else(|| {
            anyhow!(
                "Output device not found for GUID '{}'. Is it plugged in?",
                output_guid
            )
        })?;

        let dev_id = device.GetId().map_err(|e| anyhow!("GetId: {}", e))?;
        tracing::info!("audio-render device: {:?}", dev_id);

        let audio_client: IAudioClient = device
            .Activate(windows::Win32::System::Com::CLSCTX_ALL, None)
            .map_err(|e| anyhow!("Activate IAudioClient: {}", e))?;

        let mix_fmt_ptr = audio_client
            .GetMixFormat()
            .map_err(|e| anyhow!("GetMixFormat: {}", e))?;
        let mix_fmt = &*mix_fmt_ptr;

        let out_channels = mix_fmt.nChannels;
        let out_bits = mix_fmt.wBitsPerSample;
        let out_blockalign = mix_fmt.nBlockAlign;
        let out_sample_rate = mix_fmt.nSamplesPerSec;
        let output_format = detect_output_sample_format(mix_fmt_ptr)?;

        tracing::info!(
            "audio-render format: {}ch, {}bit, rate={}, ba={}, sample={:?}",
            out_channels,
            out_bits,
            out_sample_rate,
            out_blockalign,
            output_format
        );

        let stream_flags =
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;

        let buffer_duration = 400_000i64; // 40ms in 100ns units

        audio_client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                stream_flags,
                buffer_duration,
                0,
                mix_fmt_ptr,
                None,
            )
            .map_err(|e| {
                anyhow!(
                    "WASAPI render init failed: {} (0x{:08X})",
                    e.message(),
                    e.code().0 as u32
                )
            })?;

        let buffer_size = audio_client
            .GetBufferSize()
            .map_err(|e| anyhow!("GetBufferSize: {}", e))?;

        let render_client: IAudioRenderClient = audio_client
            .GetService()
            .map_err(|e| anyhow!("GetService IAudioRenderClient: {}", e))?;

        audio_client
            .Start()
            .map_err(|e| anyhow!("Start render: {}", e))?;

        tracing::info!(
            "audio-render WASAPI started (buf={} frames, {}ch, {}Hz)",
            buffer_size,
            out_channels,
            out_sample_rate
        );

        let result = run_render_loop(
            &audio_client,
            &render_client,
            rx,
            buffer_size,
            out_channels,
            out_bits,
            out_blockalign,
            output_format,
            active,
            restart_signal,
        );

        let _ = audio_client.Stop();

        // Free the mix format allocated by GetMixFormat
        windows::Win32::System::Com::CoTaskMemFree(Some(
            mix_fmt_ptr as *const _ as *const std::ffi::c_void,
        ));

        tracing::info!("audio-render WASAPI stopped");

        result
    }
}

/// Main render loop: reads packets from the channel and writes to the
/// WASAPI render buffer.
unsafe fn run_render_loop(
    audio_client: &windows::Win32::Media::Audio::IAudioClient,
    render_client: &windows::Win32::Media::Audio::IAudioRenderClient,
    rx: &Receiver<AudioPacket>,
    buffer_size: u32,
    out_channels: u16,
    _out_bits: u16,
    out_blockalign: u16,
    output_format: OutputSampleFormat,
    active: &AtomicBool,
    restart_signal: &AtomicBool,
) -> anyhow::Result<()> {
    // Accumulator for incoming samples (interleaved f32)
    let mut sample_buf: Vec<f32> = Vec::with_capacity(4096);

    loop {
        if !active.load(Ordering::SeqCst) || restart_signal.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Receive audio packets (non-blocking drain)
        while let Ok((samples, in_channels, _mask)) = rx.try_recv() {
            // Channel remix if needed (e.g., 8ch capture → 2ch output)
            let remixed = remix_channels(&samples, in_channels, out_channels);
            sample_buf.extend_from_slice(&remixed);
        }

        // Cap buffer to prevent unbounded growth when output can't keep up
        // ~500ms @ 48kHz stereo = 48000 samples. Drop oldest if exceeded.
        const MAX_SAMPLE_BUF: usize = 48000;
        if sample_buf.len() > MAX_SAMPLE_BUF {
            let excess = sample_buf.len() - MAX_SAMPLE_BUF;
            sample_buf.drain(..excess);
        }

        // How much space does the render buffer have?
        let padding = audio_client
            .GetCurrentPadding()
            .map_err(|e| anyhow!("GetCurrentPadding: {}", e))?;
        let available_frames = buffer_size.saturating_sub(padding) as usize;

        if available_frames > 0 && !sample_buf.is_empty() {
            let samples_per_frame = out_channels as usize;
            let frames_to_write = (sample_buf.len() / samples_per_frame).min(available_frames);

            if frames_to_write > 0 {
                let render_buf = render_client
                    .GetBuffer(frames_to_write as u32)
                    .map_err(|e| anyhow!("GetBuffer: {}", e))?;

                let total_samples = frames_to_write * samples_per_frame;

                write_render_samples(
                    render_buf,
                    &sample_buf[..total_samples],
                    frames_to_write,
                    out_channels,
                    out_blockalign,
                    output_format,
                );

                render_client
                    .ReleaseBuffer(frames_to_write as u32, 0)
                    .map_err(|e| anyhow!("ReleaseBuffer: {}", e))?;

                // Remove written samples from the accumulator
                sample_buf.drain(..total_samples);
            }
        }

        // Sleep to match roughly the device period (~10ms)
        thread::sleep(Duration::from_millis(5));
    }
}

// ─── Channel Remixing ──────────────────────────────────────────

unsafe fn detect_output_sample_format(
    mix_fmt_ptr: *const windows::Win32::Media::Audio::WAVEFORMATEX,
) -> anyhow::Result<OutputSampleFormat> {
    use windows::core::GUID;
    use windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE;

    const SUBTYPE_PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
    const SUBTYPE_IEEE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

    let fmt = &*mix_fmt_ptr;
    let format_tag = fmt.wFormatTag;
    let bits_per_sample = fmt.wBitsPerSample;
    let channels_raw = fmt.nChannels;
    let block_align = fmt.nBlockAlign;
    let cb_size = fmt.cbSize;
    let subtype = if format_tag == WAVE_FORMAT_EXTENSIBLE && cb_size >= 22 {
        Some((*(mix_fmt_ptr as *const WAVEFORMATEXTENSIBLE)).SubFormat)
    } else {
        None
    };

    let channels = channels_raw as usize;
    let bytes_per_frame = block_align as usize;
    if channels == 0 || bytes_per_frame % channels != 0 {
        anyhow::bail!(
            "Invalid WASAPI render format: channels={}, blockAlign={}",
            channels_raw,
            block_align
        );
    }

    let sample_format = match (format_tag, bits_per_sample, subtype) {
        (WAVE_FORMAT_IEEE_FLOAT, 32, _) => OutputSampleFormat::Float32,
        (WAVE_FORMAT_PCM, 16, _) => OutputSampleFormat::Pcm16,
        (WAVE_FORMAT_PCM, 24, _) => OutputSampleFormat::Pcm24,
        (WAVE_FORMAT_PCM, 32, _) => OutputSampleFormat::Pcm32,
        (WAVE_FORMAT_EXTENSIBLE, 32, Some(sub)) if sub == SUBTYPE_IEEE_FLOAT => {
            OutputSampleFormat::Float32
        }
        (WAVE_FORMAT_EXTENSIBLE, 16, Some(sub)) if sub == SUBTYPE_PCM => OutputSampleFormat::Pcm16,
        (WAVE_FORMAT_EXTENSIBLE, 24, Some(sub)) if sub == SUBTYPE_PCM => OutputSampleFormat::Pcm24,
        (WAVE_FORMAT_EXTENSIBLE, 32, Some(sub)) if sub == SUBTYPE_PCM => OutputSampleFormat::Pcm32,
        _ => anyhow::bail!(
            "Unsupported WASAPI render format: tag=0x{:04X}, bits={}",
            format_tag,
            bits_per_sample
        ),
    };

    let bytes_per_sample = bytes_per_frame / channels;
    let expected_min_bytes = match sample_format {
        OutputSampleFormat::Float32 | OutputSampleFormat::Pcm32 => 4,
        OutputSampleFormat::Pcm24 => 3,
        OutputSampleFormat::Pcm16 => 2,
    };
    if bytes_per_sample < expected_min_bytes {
        anyhow::bail!(
            "Invalid WASAPI render sample width: sample={:?}, bytesPerSample={}",
            sample_format,
            bytes_per_sample
        );
    }

    Ok(sample_format)
}

unsafe fn write_render_samples(
    render_buf: *mut u8,
    samples: &[f32],
    frames: usize,
    channels: u16,
    blockalign: u16,
    output_format: OutputSampleFormat,
) {
    if matches!(output_format, OutputSampleFormat::Float32) {
        let out_slice = std::slice::from_raw_parts_mut(render_buf as *mut f32, samples.len());
        out_slice.copy_from_slice(samples);
        return;
    }

    let channels = channels as usize;
    let blockalign = blockalign as usize;
    let bytes_per_sample = blockalign / channels;
    let out_bytes = std::slice::from_raw_parts_mut(render_buf, frames * blockalign);

    for (idx, sample) in samples.iter().enumerate() {
        let frame = idx / channels;
        let channel = idx % channels;
        let offset = frame * blockalign + channel * bytes_per_sample;

        match output_format {
            OutputSampleFormat::Float32 => unreachable!(),
            OutputSampleFormat::Pcm16 => {
                let bytes = f32_to_i16(*sample).to_le_bytes();
                out_bytes[offset..offset + 2].copy_from_slice(&bytes);
            }
            OutputSampleFormat::Pcm24 => {
                let value = f32_to_i24(*sample);
                out_bytes[offset] = (value & 0xFF) as u8;
                out_bytes[offset + 1] = ((value >> 8) & 0xFF) as u8;
                out_bytes[offset + 2] = ((value >> 16) & 0xFF) as u8;
                if bytes_per_sample > 3 {
                    out_bytes[offset + 3] = if value < 0 { 0xFF } else { 0x00 };
                }
            }
            OutputSampleFormat::Pcm32 => {
                let bytes = f32_to_i32(*sample).to_le_bytes();
                out_bytes[offset..offset + 4].copy_from_slice(&bytes);
            }
        }
    }
}

fn f32_to_i16(sample: f32) -> i16 {
    let sample = sample.clamp(-1.0, 1.0);
    if sample <= -1.0 {
        i16::MIN
    } else {
        (sample * i16::MAX as f32).round() as i16
    }
}

fn f32_to_i24(sample: f32) -> i32 {
    let sample = sample.clamp(-1.0, 1.0);
    if sample <= -1.0 {
        -8_388_608
    } else {
        (sample * 8_388_607.0).round() as i32
    }
}

fn f32_to_i32(sample: f32) -> i32 {
    let sample = sample.clamp(-1.0, 1.0);
    if sample <= -1.0 {
        i32::MIN
    } else {
        (sample * 2_147_483_647.0).round() as i32
    }
}

/// Remix interleaved samples from in_channels to out_channels.
/// Handles common cases: downmix (8→2, 6→2) and upmix (2→6, 2→8).
fn remix_channels(samples: &[f32], in_ch: u16, out_ch: u16) -> Vec<f32> {
    if in_ch == out_ch {
        return samples.to_vec();
    }

    let in_ch = in_ch as usize;
    let out_ch = out_ch as usize;
    let frame_count = samples.len() / in_ch;
    let mut output = Vec::with_capacity(frame_count * out_ch);

    for frame_idx in 0..frame_count {
        let frame_start = frame_idx * in_ch;
        let frame = &samples[frame_start..frame_start + in_ch];

        if out_ch < in_ch {
            // Downmix: take first out_ch channels (simple)
            // For proper downmix from 7.1→stereo, mix L/R with center and surrounds
            if out_ch == 2 && in_ch >= 6 {
                // ITU-R BS.775 compliant 7.1/5.1 → stereo downmix
                // All coefficients relative to 0 dBFS:
                //   FL/FR: 1/√2 (-3 dB)  — direct channels
                //   C:     1/√2 (-3 dB)  — center shared equally
                //   LFE:   omitted       — subwoofer content, not for headphones
                //   SL/SR: 1/2  (-6 dB)  — rear surround
                //   BL/BR: 1/(2√2) ≈ 0.354 (-9 dB) — side/back
                const FRONT: f32 = 0.707; // 1/√2
                const CENTER: f32 = 0.707; // 1/√2
                const SURROUND: f32 = 0.500; // 1/2
                const BACK: f32 = 0.354; // 1/(2√2)

                let fl = frame[0] * FRONT;
                let fr = frame[1] * FRONT;
                let c = frame.get(2).copied().unwrap_or(0.0) * CENTER;
                // LFE (ch3) intentionally omitted — headphone output, not subwoofer
                let sl = frame.get(4).copied().unwrap_or(0.0) * SURROUND;
                let sr = frame.get(5).copied().unwrap_or(0.0) * SURROUND;
                let bl = frame.get(6).copied().unwrap_or(0.0) * BACK;
                let br = frame.get(7).copied().unwrap_or(0.0) * BACK;

                // Sum and soft-clamp to [-1, 1] as safety net
                output.push((fl + c + sl + bl).clamp(-1.0, 1.0));
                output.push((fr + c + sr + br).clamp(-1.0, 1.0));
            } else {
                output.extend_from_slice(&frame[..out_ch]);
            }
        } else {
            // Upmix: copy available channels, zero-pad the rest
            output.extend_from_slice(frame);
            output.extend(std::iter::repeat(0.0f32).take(out_ch - in_ch));
        }
    }

    output
}

// ─── Device Lookup ─────────────────────────────────────────────

unsafe fn find_device_by_guid(
    enumerator: &windows::Win32::Media::Audio::IMMDeviceEnumerator,
    guid: &str,
) -> Option<windows::Win32::Media::Audio::IMMDevice> {
    use windows::Win32::Media::Audio::*;

    let collection = enumerator
        .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
        .ok()?;
    let count = collection.GetCount().ok()?;
    let target = guid.to_ascii_lowercase();
    let target_bare = target.trim_start_matches('{').trim_end_matches('}');

    for i in 0..count {
        let device = match collection.Item(i) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let dev_id = match device.GetId() {
            Ok(id) => id.to_string().unwrap_or_default().to_ascii_lowercase(),
            Err(_) => continue,
        };

        if dev_id.contains(target_bare) {
            tracing::info!(
                "audio-render found output device: {} (matched '{}')",
                dev_id,
                target_bare
            );
            return Some(device);
        }
    }

    tracing::warn!("audio-render: no device matched GUID '{}'", guid);
    None
}

// ─── Registry Helpers ──────────────────────────────────────────

/// Read the output endpoint GUID from HKCU (preferred) or HKLM (fallback).
pub fn read_output_endpoint_guid() -> Option<String> {
    use winreg::enums::*;
    use winreg::RegKey;

    // HKCU first (user-level setting)
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey(REGISTRY_PATH) {
        if let Ok(guid) = key.get_value::<String, _>(OUTPUT_GUID_REGISTRY_KEY) {
            if !guid.trim().is_empty() {
                return Some(guid);
            }
        }
    }

    // HKLM fallback
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey(REGISTRY_PATH) {
        if let Ok(guid) = key.get_value::<String, _>(OUTPUT_GUID_REGISTRY_KEY) {
            if !guid.trim().is_empty() {
                return Some(guid);
            }
        }
    }

    None
}

/// Write the output endpoint GUID to HKCU.
pub fn write_output_endpoint_guid(guid: &str) -> anyhow::Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey_with_flags(REGISTRY_PATH, KEY_READ | KEY_WRITE)
        .or_else(|_| hkcu.create_subkey(REGISTRY_PATH).map(|(k, _)| k))
        .map_err(|e| anyhow!("Cannot open HKCU\\SOFTWARE\\VanySound: {}", e))?;

    key.set_value(OUTPUT_GUID_REGISTRY_KEY, &guid)
        .map_err(|e| anyhow!("Failed to write OutputEndpointGuid: {}", e))?;

    // Verify
    let readback: String = key.get_value(OUTPUT_GUID_REGISTRY_KEY).unwrap_or_default();
    if readback.to_ascii_lowercase() != guid.to_ascii_lowercase() {
        anyhow::bail!(
            "Write verification failed: wrote '{}' but read '{}'",
            guid,
            readback
        );
    }

    tracing::info!(guid, "OutputEndpointGuid written and verified");
    Ok(())
}
