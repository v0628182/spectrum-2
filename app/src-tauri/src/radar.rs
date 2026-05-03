use crate::audio_output::{self, RenderHandle};
use crate::models::RadarSnapshotDto;
use crossbeam_channel::{bounded, Receiver, Sender};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ── Constants from analysis.rs ──
const PERCEPTUAL_GAMMA: f32 = 0.50;
const NOISE_FLOOR: f32 = 0.005;
const RADAR_GATE: f32 = 0.04;

// ── Speaker mask bits (WASAPI) ──
const SPEAKER_FRONT_LEFT: u32 = 0x1;
const SPEAKER_FRONT_RIGHT: u32 = 0x2;
const SPEAKER_FRONT_CENTER: u32 = 0x4;
const SPEAKER_LOW_FREQUENCY: u32 = 0x8;
const SPEAKER_BACK_LEFT: u32 = 0x10;
const SPEAKER_BACK_RIGHT: u32 = 0x20;
const SPEAKER_FRONT_LEFT_OF_CENTER: u32 = 0x40;
const SPEAKER_FRONT_RIGHT_OF_CENTER: u32 = 0x80;
const SPEAKER_BACK_CENTER: u32 = 0x100;
const SPEAKER_SIDE_LEFT: u32 = 0x200;
const SPEAKER_SIDE_RIGHT: u32 = 0x400;

type AudioPacket = (Vec<f32>, u16, u32);

const WAVE_FORMAT_PCM: u16 = 0x0001;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

#[derive(Debug, Clone, Copy)]
enum WasapiSampleFormat {
    Float32,
    Pcm16,
    Pcm24,
    Pcm32,
}

/// Thread join handles collected for graceful shutdown.
struct ThreadHandles {
    capture: Option<std::thread::JoinHandle<()>>,
    analysis: Option<std::thread::JoinHandle<()>>,
    render: Option<std::thread::JoinHandle<()>>,
    overlay: Option<std::thread::JoinHandle<()>>,
}

#[derive(Clone)]
pub struct RadarService {
    snapshot: Arc<RwLock<RadarSnapshotDto>>,
    overlay_enabled: Arc<AtomicBool>,
    mini_radar_enabled: Arc<AtomicBool>,
    mini_radar_position: Arc<AtomicU8>,
    render_handle: Arc<Mutex<Option<RenderHandle>>>,
    /// Global shutdown flag — set to true when the app is closing
    shutdown: Arc<AtomicBool>,
    /// Thread handles for join on drop (wrapped in Mutex for interior mutability)
    handles: Arc<Mutex<ThreadHandles>>,
}

impl RadarService {
    pub fn start() -> Self {
        let snapshot = Arc::new(RwLock::new(RadarSnapshotDto::default()));
        let snapshot_writer = Arc::clone(&snapshot);
        let snapshot_err = Arc::clone(&snapshot);
        let shutdown = Arc::new(AtomicBool::new(false));

        // Two channels: one for analysis (radar vis), one for render (audio output)
        let (analysis_tx, analysis_rx) = bounded::<AudioPacket>(4);
        let (render_tx, render_rx) = bounded::<AudioPacket>(16);

        // Spawn the audio render pipeline (outputs to user's headphones)
        let (render_handle, render_join) = audio_output::spawn_render_thread(render_rx);

        // ── Capture thread with exponential backoff ──
        let shutdown_capture = Arc::clone(&shutdown);
        let capture_join = thread::Builder::new()
            .name("radar-capture".into())
            .spawn(move || {
                tracing::info!("radar-capture thread started");
                let max_retries = 50;
                let mut retries = 0;

                loop {
                    if shutdown_capture.load(Ordering::Relaxed) {
                        tracing::info!("radar-capture: shutdown signal, exiting");
                        break;
                    }

                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        capture_loop_raw(analysis_tx.clone(), render_tx.clone())
                    }));

                    match result {
                        Ok(Ok(())) => {
                            tracing::info!("radar-capture loop exited cleanly");
                            break;
                        }
                        Ok(Err(e)) => {
                            retries += 1;
                            tracing::warn!(
                                retry = retries,
                                max = max_retries,
                                error = %e,
                                "radar-capture session failed, restarting..."
                            );
                            if retries >= max_retries {
                                tracing::error!("radar-capture max retries exceeded, giving up");
                                if let Ok(mut guard) = snapshot_err.write() {
                                    guard.last_error = Some(format!("Max retries: {}", e));
                                    guard.capture_active = false;
                                }
                                break;
                            }
                            // Exponential backoff: 2s, 4s, 8s, ... capped at 30s
                            let backoff_secs = (2u64 << retries.min(4)).min(30);
                            let backoff = Duration::from_secs(backoff_secs);
                            tracing::info!("radar-capture: backing off for {:?}", backoff);
                            // Sleep in 500ms increments to check shutdown
                            let mut slept = Duration::ZERO;
                            while slept < backoff {
                                if shutdown_capture.load(Ordering::Relaxed) {
                                    return;
                                }
                                thread::sleep(Duration::from_millis(500));
                                slept += Duration::from_millis(500);
                            }
                        }
                        Err(panic) => {
                            let msg = panic
                                .downcast_ref::<String>()
                                .cloned()
                                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                                .unwrap_or_else(|| "unknown panic".into());
                            tracing::error!("Radar capture PANIC: {}", msg);
                            if let Ok(mut guard) = snapshot_err.write() {
                                guard.last_error = Some(format!("panic: {}", msg));
                                guard.capture_active = false;
                            }
                            break;
                        }
                    }
                }
            })
            .expect("Failed to spawn radar capture thread");

        let analysis_join = thread::Builder::new()
            .name("radar-analysis".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    analysis_loop(analysis_rx, snapshot_writer);
                }));
                if let Err(panic) = result {
                    let msg = panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "unknown panic".into());
                    tracing::error!("Radar analysis thread PANIC: {}", msg);
                }
            })
            .expect("Failed to spawn radar analysis thread");

        // Spawn the fullscreen overlay (reads from same snapshot)
        let overlay_enabled = Arc::new(AtomicBool::new(false));
        let overlay_join = crate::overlay::spawn_overlay(
            Arc::clone(&snapshot),
            Arc::clone(&overlay_enabled),
            Arc::clone(&shutdown),
        );

        // ── Feature flag: flip to `true` to re-enable the mini radar overlay ──
        const MINI_RADAR_ENABLED: bool = false;

        let mini_radar_enabled = Arc::new(AtomicBool::new(false));
        let mini_radar_position = Arc::new(AtomicU8::new(crate::mini_radar::POS_BOTTOM_RIGHT));
        if MINI_RADAR_ENABLED {
            crate::mini_radar::spawn_mini_radar(
                Arc::clone(&snapshot),
                Arc::clone(&mini_radar_enabled),
                Arc::clone(&mini_radar_position),
            );
        }

        Self {
            snapshot,
            overlay_enabled,
            mini_radar_enabled,
            mini_radar_position,
            render_handle: Arc::new(Mutex::new(Some(render_handle))),
            shutdown,
            handles: Arc::new(Mutex::new(ThreadHandles {
                capture: Some(capture_join),
                analysis: Some(analysis_join),
                render: Some(render_join),
                overlay: Some(overlay_join),
            })),
        }
    }

    /// Signal all threads to shut down and wait for them to finish.
    pub fn shutdown(&self) {
        tracing::info!("RadarService::shutdown — signaling all threads");
        self.shutdown.store(true, Ordering::SeqCst);

        // Signal the render thread to stop via its own flag
        if let Ok(guard) = self.render_handle.lock() {
            if let Some(handle) = guard.as_ref() {
                handle.stop();
            }
        }

        // Join threads with a timeout
        if let Ok(mut handles) = self.handles.lock() {
            let thread_names = ["capture", "analysis", "render", "overlay"];
            let joins: [Option<std::thread::JoinHandle<()>>; 4] = [
                handles.capture.take(),
                handles.analysis.take(),
                handles.render.take(),
                handles.overlay.take(),
            ];
            for (name, join) in thread_names.iter().zip(joins.into_iter()) {
                if let Some(handle) = join {
                    tracing::info!("RadarService: joining {} thread...", name);
                    match handle.join() {
                        Ok(()) => tracing::info!("RadarService: {} thread joined OK", name),
                        Err(_) => tracing::error!("RadarService: {} thread panicked on join", name),
                    }
                }
            }
        }
        tracing::info!("RadarService::shutdown complete");
    }

    /// Signal the render thread to restart with a new output device.
    pub fn restart_render(&self) {
        if let Ok(guard) = self.render_handle.lock() {
            if let Some(handle) = guard.as_ref() {
                handle.restart();
            }
        }
    }

    pub fn snapshot(&self) -> RadarSnapshotDto {
        self.snapshot
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    pub fn set_overlay_enabled(&self, enabled: bool) {
        self.overlay_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn set_mini_radar_enabled(&self, enabled: bool) {
        self.mini_radar_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn set_mini_radar_position(&self, position: u8) {
        self.mini_radar_position
            .store(position.min(3), Ordering::Relaxed);
    }
}

impl Drop for RadarService {
    fn drop(&mut self) {
        tracing::info!("RadarService::drop — initiating graceful shutdown");
        self.shutdown();
    }
}

// ═══════════════════════════════════════════════════════════════
//  RAW WASAPI CAPTURE — bypasses wasapi crate for initialization
// ═══════════════════════════════════════════════════════════════

fn capture_loop_raw(
    analysis_tx: Sender<AudioPacket>,
    render_tx: Sender<AudioPacket>,
) -> anyhow::Result<()> {
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::*;

    unsafe {
        let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
        tracing::info!("Radar COM init: {:?}", hr);

        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| anyhow::anyhow!("CoCreateInstance: {}", e))?;

        // ── Find the HiFi Cable device (multi-strategy) ──
        // Strategy 1: Registry GUID lookup
        // Strategy 2: Name-based search (Hi-Fi Cable, VB-Audio, CABLE Output)
        // Strategy 3: Fall back to default render device (last resort)
        let device = find_hifi_cable_device(&enumerator).ok_or_else(|| {
            anyhow::anyhow!(
                "HiFi Cable/VanySound render endpoint not found. Run Repair Core Architecture."
            )
        })?;

        let dev_id = device
            .GetId()
            .map_err(|e| anyhow::anyhow!("GetId: {}", e))?;
        tracing::info!("Radar capturing from device: {:?}", dev_id);

        let audio_client: IAudioClient = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| anyhow::anyhow!("Activate: {}", e))?;

        let mix_fmt_result = audio_client.GetMixFormat();
        let (use_fallback, mix_fmt_ptr) = match mix_fmt_result {
            Ok(ptr) => (false, ptr),
            Err(e) => {
                tracing::warn!(
                    "GetMixFormat failed: {} (0x{:08X}); using fallback 48kHz/f32/stereo",
                    e.message(),
                    e.code().0 as u32
                );
                let fallback = build_fallback_format();
                let boxed = Box::new(fallback);
                let ptr = Box::into_raw(boxed) as *mut WAVEFORMATEX;
                (true, ptr)
            }
        };

        let mix_fmt = &*mix_fmt_ptr;
        let channels = mix_fmt.nChannels;
        let bits = mix_fmt.wBitsPerSample;
        let blockalign = mix_fmt.nBlockAlign;
        let sample_format = detect_sample_format(mix_fmt_ptr)?;

        let channel_mask = if mix_fmt.wFormatTag == 0xFFFE && mix_fmt.cbSize >= 22 {
            let ext = &*(mix_fmt_ptr as *const _ as *const WAVEFORMATEXTENSIBLE);
            ext.dwChannelMask as u32
        } else if channels == 2 {
            0x3
        } else {
            0
        };

        let mut def_period = 0i64;
        audio_client
            .GetDevicePeriod(Some(&mut def_period), None)
            .map_err(|e| anyhow::anyhow!("GetDevicePeriod: {}", e))?;

        tracing::info!(
            "Radar format: {}ch, {}bit, ba={}, sample={:?}, mask=0x{:X}, period={}, fallback={}",
            channels,
            bits,
            blockalign,
            sample_format,
            channel_mask,
            def_period,
            use_fallback
        );

        let stream_flags = if use_fallback {
            AUDCLNT_STREAMFLAGS_LOOPBACK
                | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY
        } else {
            AUDCLNT_STREAMFLAGS_LOOPBACK
        };

        audio_client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                stream_flags,
                def_period,
                0,
                mix_fmt_ptr,
                None,
            )
            .map_err(|e| {
                anyhow::anyhow!(
                    "WASAPI Init failed: {} (0x{:08X})",
                    e.message(),
                    e.code().0 as u32
                )
            })?;

        tracing::info!("Radar WASAPI loopback init OK — capturing HiFi Cable audio only");
        let result = run_raw_capture(
            audio_client,
            analysis_tx,
            render_tx,
            channels,
            bits,
            blockalign,
            channel_mask,
            sample_format,
        );

        // Free the mix format if it was allocated by GetMixFormat (not our fallback box)
        if !use_fallback {
            windows::Win32::System::Com::CoTaskMemFree(Some(
                mix_fmt_ptr as *const _ as *const std::ffi::c_void,
            ));
        }
        // Balance CoInitializeEx
        CoUninitialize();

        result
    }
}

/// Multi-strategy search for the HiFi Cable virtual audio device.
/// 1. Try registry GUID (fastest, most precise)
/// 2. Enumerate all render devices and match by known name patterns
unsafe fn find_hifi_cable_device(
    enumerator: &windows::Win32::Media::Audio::IMMDeviceEnumerator,
) -> Option<windows::Win32::Media::Audio::IMMDevice> {
    use windows::Win32::Media::Audio::*;

    // ── Strategy 1: Registry GUID ──
    if let Some(guid) = read_hifi_endpoint_guid() {
        tracing::info!("Radar looking for HiFi Cable by registry GUID: {}", guid);
        if let Some(dev) = find_device_by_guid(enumerator, &guid) {
            tracing::info!("Radar found HiFi Cable via registry GUID");
            return Some(dev);
        }
        tracing::warn!("Registry GUID found but device not matched, trying name search...");
    }

    // ── Strategy 2: Search by device name patterns ──
    let name_patterns = [
        "hi-fi cable",
        "hifi cable",
        "cable output",
        "vb-audio virtual cable",
        "vb-audio hi-fi",
        "vanysound",
    ];

    tracing::info!("Radar searching for HiFi Cable by name patterns...");
    let collection = enumerator
        .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
        .ok()?;
    let count = collection.GetCount().ok()?;

    for i in 0..count {
        let device = match collection.Item(i) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Get the friendly name via IPropertyStore
        if let Some(name) = get_device_friendly_name(&device) {
            let name_lower = name.to_ascii_lowercase();
            tracing::debug!("Radar device [{}]: \"{}\"", i, name);

            for pattern in &name_patterns {
                if name_lower.contains(pattern) {
                    tracing::info!(
                        "Radar found HiFi Cable by name: \"{}\" (matched '{}')",
                        name,
                        pattern
                    );
                    return Some(device);
                }
            }
        }
    }

    tracing::warn!("Radar could not find HiFi Cable device by any strategy");
    None
}

/// Search all render endpoints for a device whose ID contains the given GUID
unsafe fn find_device_by_guid(
    enumerator: &windows::Win32::Media::Audio::IMMDeviceEnumerator,
    guid: &str,
) -> Option<windows::Win32::Media::Audio::IMMDevice> {
    use windows::Win32::Media::Audio::*;

    let collection = enumerator
        .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
        .ok()?;
    let count = collection.GetCount().ok()?;

    for i in 0..count {
        let device = match collection.Item(i) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let dev_id = match device.GetId() {
            Ok(id) => id.to_string().unwrap_or_default(),
            Err(_) => continue,
        };
        if dev_id
            .to_ascii_lowercase()
            .contains(&guid.to_ascii_lowercase())
        {
            return Some(device);
        }
    }
    None
}

/// Get the friendly display name of an audio device.
/// Uses the device ID as a heuristic and also tries IPropertyStore.
unsafe fn get_device_friendly_name(
    device: &windows::Win32::Media::Audio::IMMDevice,
) -> Option<String> {
    use windows::core::GUID;

    // PKEY_Device_FriendlyName = {a45c254e-df1c-4efd-8020-67d146a850e0}, 14
    let fmtid = GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0);
    let pkey = windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY { fmtid, pid: 14 };

    // OpenPropertyStore(STGM_READ = 0)
    let store = device
        .OpenPropertyStore(windows::Win32::System::Com::STGM(0))
        .ok()?;
    let val = store.GetValue(&pkey).ok()?;

    // In windows 0.58, PROPVARIANT is opaque. Use Display trait to get string.
    let name = format!("{}", val);
    if name.is_empty() || name == "VT_EMPTY" {
        return None;
    }
    Some(name)
}

/// Read HiFiEndpointGuid — HKCU first (user-level, where app writes), then HKLM fallback.
fn read_hifi_endpoint_guid() -> Option<String> {
    use winreg::enums::*;
    use winreg::RegKey;

    // HKCU first — this is where set_audio_output and repair writes the GUID
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey(r"SOFTWARE\VanySound") {
        if let Ok(guid) = key.get_value::<String, _>("HiFiEndpointGuid") {
            if !guid.trim().is_empty() {
                tracing::info!("read_hifi_endpoint_guid: found in HKCU: {}", guid);
                return Some(guid);
            }
        }
    }

    // HKLM fallback
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey(r"SOFTWARE\VanySound") {
        if let Ok(guid) = key.get_value::<String, _>("HiFiEndpointGuid") {
            if !guid.trim().is_empty() {
                tracing::info!("read_hifi_endpoint_guid: found in HKLM: {}", guid);
                return Some(guid);
            }
        }
    }

    tracing::warn!("read_hifi_endpoint_guid: not found in HKCU or HKLM");
    None
}

/// Build a standard 48kHz / 32-bit float / stereo WAVEFORMATEXTENSIBLE
/// used as fallback when GetMixFormat fails (e.g. broken APO chain).
fn build_fallback_format() -> windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE {
    use windows::core::GUID;
    use windows::Win32::Media::Audio::*;

    // KSDATAFORMAT_SUBTYPE_IEEE_FLOAT = {00000003-0000-0010-8000-00aa00389b71}
    const SUBTYPE_IEEE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

    let channels: u16 = 2;
    let sample_rate: u32 = 48000;
    let bits_per_sample: u16 = 32;
    let block_align = channels * (bits_per_sample / 8);
    let avg_bytes = sample_rate * block_align as u32;

    WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: 0xFFFE, // WAVE_FORMAT_EXTENSIBLE
            nChannels: channels,
            nSamplesPerSec: sample_rate,
            nAvgBytesPerSec: avg_bytes,
            nBlockAlign: block_align,
            wBitsPerSample: bits_per_sample,
            cbSize: 22,
        },
        Samples: WAVEFORMATEXTENSIBLE_0 {
            wValidBitsPerSample: bits_per_sample,
        },
        dwChannelMask: (SPEAKER_FRONT_LEFT | SPEAKER_FRONT_RIGHT) as u32,
        SubFormat: SUBTYPE_IEEE_FLOAT,
    }
}

unsafe fn detect_sample_format(
    mix_fmt_ptr: *const windows::Win32::Media::Audio::WAVEFORMATEX,
) -> anyhow::Result<WasapiSampleFormat> {
    use windows::core::GUID;
    use windows::Win32::Media::Audio::WAVEFORMATEXTENSIBLE;

    const SUBTYPE_PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
    const SUBTYPE_IEEE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

    let fmt = &*mix_fmt_ptr;
    let format_tag = fmt.wFormatTag;
    let bits_per_sample = fmt.wBitsPerSample;
    let cb_size = fmt.cbSize;
    let subtype = if format_tag == WAVE_FORMAT_EXTENSIBLE && cb_size >= 22 {
        Some((*(mix_fmt_ptr as *const WAVEFORMATEXTENSIBLE)).SubFormat)
    } else {
        None
    };

    match (format_tag, bits_per_sample, subtype) {
        (WAVE_FORMAT_IEEE_FLOAT, 32, _) => Ok(WasapiSampleFormat::Float32),
        (WAVE_FORMAT_PCM, 16, _) => Ok(WasapiSampleFormat::Pcm16),
        (WAVE_FORMAT_PCM, 24, _) => Ok(WasapiSampleFormat::Pcm24),
        (WAVE_FORMAT_PCM, 32, _) => Ok(WasapiSampleFormat::Pcm32),
        (WAVE_FORMAT_EXTENSIBLE, 32, Some(sub)) if sub == SUBTYPE_IEEE_FLOAT => {
            Ok(WasapiSampleFormat::Float32)
        }
        (WAVE_FORMAT_EXTENSIBLE, 16, Some(sub)) if sub == SUBTYPE_PCM => {
            Ok(WasapiSampleFormat::Pcm16)
        }
        (WAVE_FORMAT_EXTENSIBLE, 24, Some(sub)) if sub == SUBTYPE_PCM => {
            Ok(WasapiSampleFormat::Pcm24)
        }
        (WAVE_FORMAT_EXTENSIBLE, 32, Some(sub)) if sub == SUBTYPE_PCM => {
            Ok(WasapiSampleFormat::Pcm32)
        }
        _ => anyhow::bail!(
            "Unsupported WASAPI capture format: tag=0x{:04X}, bits={}",
            format_tag,
            bits_per_sample
        ),
    }
}

/// Get device identifier string from IMMDevice
unsafe fn get_device_name(device: &windows::Win32::Media::Audio::IMMDevice) -> Option<String> {
    let id = device.GetId().ok()?;
    let id_str = id.to_string().ok()?;
    Some(id_str)
}

fn decode_samples(raw: &[u8], format: WasapiSampleFormat, bytes_per_sample: usize) -> Vec<f32> {
    match format {
        WasapiSampleFormat::Float32 => raw
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect(),
        WasapiSampleFormat::Pcm16 => raw
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect(),
        WasapiSampleFormat::Pcm24 => raw
            .chunks_exact(bytes_per_sample)
            .filter(|b| b.len() >= 3)
            .map(|b| {
                let v = (b[0] as i32) | ((b[1] as i32) << 8) | ((b[2] as i32) << 16);
                let v = if v & 0x800000 != 0 { v | !0xFFFFFF } else { v };
                v as f32 / 8_388_608.0
            })
            .collect(),
        WasapiSampleFormat::Pcm32 => raw
            .chunks_exact(4)
            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0)
            .collect(),
    }
}

unsafe fn run_raw_capture(
    audio_client: windows::Win32::Media::Audio::IAudioClient,
    analysis_tx: Sender<AudioPacket>,
    render_tx: Sender<AudioPacket>,
    channels: u16,
    bits: u16,
    blockalign: u16,
    channel_mask: u32,
    sample_format: WasapiSampleFormat,
) -> anyhow::Result<()> {
    use windows::Win32::Media::Audio::*;

    let capture_client: IAudioCaptureClient = audio_client
        .GetService()
        .map_err(|e| anyhow::anyhow!("GetService IAudioCaptureClient: {}", e))?;

    audio_client
        .Start()
        .map_err(|e| anyhow::anyhow!("Start: {}", e))?;

    tracing::info!(
        "Radar loopback capture started (raw WASAPI, {}ch {}bit)",
        channels,
        bits
    );

    let chunk_frames: usize = 1024;
    let chunk_bytes = chunk_frames * blockalign as usize;
    let mut sample_queue: VecDeque<u8> = VecDeque::with_capacity(chunk_bytes * 4);
    // Cap queue at ~500ms of audio to prevent OOM if output can't keep up
    let max_queue_bytes = chunk_bytes * 48; // ~48 chunks ≈ 500ms @ 48kHz

    let mut total_frames: u64 = 0;
    let mut total_sends: u64 = 0;
    let mut log_counter: u32 = 0;

    // Stale-stream detection: if no data for 15s, log idle state but keep listening.
    // The device may just be silent (menu screen, loading, alt-tab). NOT a fatal error.
    let mut last_data_time = std::time::Instant::now();
    let mut idle_logged = false;
    const STALE_TIMEOUT: Duration = Duration::from_secs(15);

    loop {
        thread::sleep(Duration::from_millis(10));

        let mut got_data = false;

        loop {
            let mut buffer_ptr: *mut u8 = std::ptr::null_mut();
            let mut num_frames = 0u32;
            let mut flags = 0u32;

            let hr =
                capture_client.GetBuffer(&mut buffer_ptr, &mut num_frames, &mut flags, None, None);

            match hr {
                Ok(()) => {
                    if num_frames > 0 && !buffer_ptr.is_null() {
                        let byte_count = num_frames as usize * blockalign as usize;
                        let data = std::slice::from_raw_parts(buffer_ptr, byte_count);

                        if flags & 0x2 != 0 {
                            sample_queue.extend(std::iter::repeat(0u8).take(byte_count));
                        } else {
                            sample_queue.extend(data);
                        }
                        // Enforce cap: drop oldest bytes if queue exceeds limit
                        while sample_queue.len() > max_queue_bytes {
                            let _ = sample_queue.pop_front();
                        }
                        total_frames += num_frames as u64;
                        got_data = true;

                        // Log first data reception
                        if total_frames == num_frames as u64 {
                            tracing::info!(
                                "Radar FIRST DATA: {} frames, flags=0x{:X}, queue={}",
                                num_frames,
                                flags,
                                sample_queue.len()
                            );
                        }
                    }
                    let _ = capture_client.ReleaseBuffer(num_frames);
                    if num_frames == 0 {
                        break;
                    }
                }
                Err(e) => {
                    // AUDCLNT_E_DEVICE_INVALIDATED (0x88890004) or similar
                    let code = e.code().0 as u32;
                    if code == 0x88890004 || code == 0x88890026 {
                        tracing::warn!(
                            "Radar capture device invalidated (0x{:08X}), restarting...",
                            code
                        );
                        let _ = audio_client.Stop();
                        return Err(anyhow::anyhow!("Device invalidated: 0x{:08X}", code));
                    }
                    break;
                }
            }
        }

        if got_data {
            last_data_time = std::time::Instant::now();
            if idle_logged {
                tracing::info!("Radar capture resumed after idle period");
                idle_logged = false;
            }
        } else if last_data_time.elapsed() > STALE_TIMEOUT && !idle_logged {
            tracing::warn!(
                "Radar capture idle for {:?} — device silent, staying connected",
                last_data_time.elapsed()
            );
            idle_logged = true;
        }

        // Drain queue into audio packets
        let bps = (bits / 8) as usize;
        while sample_queue.len() >= chunk_bytes {
            let mut raw = vec![0u8; chunk_bytes];
            for b in &mut raw {
                *b = sample_queue.pop_front().unwrap();
            }

            let samples = decode_samples(&raw, sample_format, bps);

            // Send to analysis (non-blocking, OK to drop if behind)
            let _ = analysis_tx.try_send((samples.clone(), channels, channel_mask));
            let mut render_samples = samples;
            crate::dsp_core::process_interleaved_in_place(&mut render_samples, channels);

            // Send to render (non-blocking, OK to drop if behind)
            if render_tx
                .try_send((render_samples, channels, channel_mask))
                .is_ok()
            {
                total_sends += 1;
            }
        }

        // Periodic logging every ~5 seconds (500 iterations of 10ms sleep)
        log_counter += 1;
        if log_counter % 500 == 0 {
            tracing::info!(
                "Radar stats: frames={}, sends={}, queue={}",
                total_frames,
                total_sends,
                sample_queue.len()
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  ANALYSIS
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, Default)]
struct RadarState {
    far_left: f32,
    left: f32,
    center: f32,
    right: f32,
    far_right: f32,
    ambience: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct SpeakerEnergies {
    front_left: f32,
    front_right: f32,
    front_center: f32,
    low_frequency: f32,
    back_left: f32,
    back_right: f32,
    front_left_of_center: f32,
    front_right_of_center: f32,
    back_center: f32,
    side_left: f32,
    side_right: f32,
    unknown_left: f32,
    unknown_right: f32,
}

#[derive(Debug, Clone, Copy)]
enum SpeakerPosition {
    FrontLeft,
    FrontRight,
    FrontCenter,
    LowFrequency,
    BackLeft,
    BackRight,
    FrontLeftOfCenter,
    FrontRightOfCenter,
    BackCenter,
    SideLeft,
    SideRight,
    UnknownLeft,
    UnknownRight,
}

fn analysis_loop(rx: Receiver<AudioPacket>, snapshot: Arc<RwLock<RadarSnapshotDto>>) {
    let mut prev = RadarState::default();
    let attack = 0.55_f32;
    let release = 0.12_f32;
    let mut recv_count: u64 = 0;

    while let Ok((samples, channels, mask)) = rx.recv() {
        recv_count += 1;
        if recv_count == 1 {
            tracing::info!(
                "Radar analysis: FIRST packet received, {} samples, {}ch, mask=0x{:X}",
                samples.len(),
                channels,
                mask
            );
        }

        let raw = analyze_radar(&samples, channels, mask);
        let smoothed = smooth_radar(&raw, &prev, attack, release);
        prev = smoothed;

        let (pan, volume) = stereo_quick(&samples, channels);
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        if let Ok(mut g) = snapshot.write() {
            g.capture_active = true;
            g.channels = channels;
            g.channel_mask = mask;
            g.far_left = smoothed.far_left;
            g.left = smoothed.left;
            g.center = smoothed.center;
            g.right = smoothed.right;
            g.far_right = smoothed.far_right;
            g.ambience = smoothed.ambience;
            g.pan = pan;
            g.volume = volume;
            g.last_update_ms = Some(now_ms);
            g.last_error = None;
        }

        if recv_count % 100 == 0 {
            tracing::info!(
                "Radar analysis: {} packets processed, vol={:.4}, center={:.4}",
                recv_count,
                volume,
                smoothed.center
            );
        }
    }
    tracing::warn!("Radar analysis loop exited");
}

fn stereo_quick(samples: &[f32], channels: u16) -> (f32, f32) {
    if samples.is_empty() || channels == 0 {
        return (0.0, 0.0);
    }
    let ch = channels as usize;
    let (mut sl, mut sr, mut n) = (0.0f32, 0.0f32, 0usize);
    for frame in samples.chunks_exact(ch) {
        let l = frame[0];
        let r = if ch >= 2 { frame[1] } else { l };
        sl += l * l;
        sr += r * r;
        n += 1;
    }
    if n == 0 {
        return (0.0, 0.0);
    }
    let rl = (sl / n as f32).sqrt();
    let rr = (sr / n as f32).sqrt();
    let pan = (rr - rl) / (rr + rl + 1e-10);
    (pan, rl.max(rr).min(1.0))
}

fn analyze_radar(samples: &[f32], channels: u16, mask: u32) -> RadarState {
    if samples.is_empty() || channels == 0 {
        return RadarState::default();
    }
    let ch = channels as usize;
    let fc = samples.len() / ch;
    if fc == 0 {
        return RadarState::default();
    }

    let positions = speaker_positions(ch, mask);
    let mut sums = vec![0.0f32; ch];
    for frame in samples.chunks_exact(ch) {
        for (i, s) in frame.iter().enumerate() {
            sums[i] += s * s;
        }
    }

    let mut e = SpeakerEnergies::default();
    for (i, sum) in sums.iter().enumerate() {
        let rms = (sum / fc as f32).sqrt();
        e.add(
            positions
                .get(i)
                .copied()
                .unwrap_or(SpeakerPosition::UnknownRight),
            rms,
        );
    }
    radar_from_energies(&e)
}

fn radar_from_energies(e: &SpeakerEnergies) -> RadarState {
    let a = 3.5;
    let fl = compress(e.front_left + e.unknown_left * 0.6) * a;
    let fr = compress(e.front_right + e.unknown_right * 0.6) * a;
    let fc = compress(e.front_center) * a;
    let lfe = compress(e.low_frequency) * a;
    let bl = compress(e.back_left) * a;
    let br = compress(e.back_right) * a;
    let flc = compress(e.front_left_of_center) * a;
    let frc = compress(e.front_right_of_center) * a;
    let bc = compress(e.back_center) * a;
    let sl = compress(e.side_left) * a;
    let sr = compress(e.side_right) * a;

    RadarState {
        far_left: gate(
            (sl * 0.95 + bl * 0.82 + fl * 0.18 + flc * 0.74 + bc * 0.16).clamp(0.0, 1.0),
        ),
        left: gate(
            (fl * 1.0 + flc * 0.90 + sl * 0.26 + bl * 0.20 + fc * 0.12 + bc * 0.06).clamp(0.0, 1.0),
        ),
        center: gate(
            (fc * 1.0 + (fl + fr) * 0.24 + (sl + sr) * 0.10 + bc * 0.18 + lfe * 0.05)
                .clamp(0.0, 1.0),
        ),
        right: gate(
            (fr * 1.0 + frc * 0.90 + sr * 0.26 + br * 0.20 + fc * 0.12 + bc * 0.06).clamp(0.0, 1.0),
        ),
        far_right: gate(
            (sr * 0.95 + br * 0.82 + fr * 0.18 + frc * 0.74 + bc * 0.16).clamp(0.0, 1.0),
        ),
        ambience: gate(
            (lfe * 0.32 + (sl + sr) * 0.08 + (bl + br) * 0.10 + (fl + fr) * 0.03 + fc * 0.10)
                .clamp(0.0, 1.0),
        ),
    }
}

fn smooth_radar(cur: &RadarState, prev: &RadarState, atk: f32, rel: f32) -> RadarState {
    RadarState {
        far_left: blend(prev.far_left, cur.far_left, atk, rel),
        left: blend(prev.left, cur.left, atk, rel),
        center: blend(prev.center, cur.center, atk, rel),
        right: blend(prev.right, cur.right, atk, rel),
        far_right: blend(prev.far_right, cur.far_right, atk, rel),
        ambience: blend(prev.ambience, cur.ambience, atk, rel),
    }
}

fn blend(prev: f32, cur: f32, atk: f32, rel: f32) -> f32 {
    let a = if cur > prev { atk } else { rel };
    a * cur + (1.0 - a) * prev
}

fn compress(x: f32) -> f32 {
    if x <= NOISE_FLOOR {
        0.0
    } else {
        x.powf(PERCEPTUAL_GAMMA)
    }
}
fn gate(x: f32) -> f32 {
    if x < RADAR_GATE {
        0.0
    } else {
        x
    }
}

fn speaker_positions(channels: usize, mask: u32) -> Vec<SpeakerPosition> {
    if mask != 0 {
        let bits = [
            (SPEAKER_FRONT_LEFT, SpeakerPosition::FrontLeft),
            (SPEAKER_FRONT_RIGHT, SpeakerPosition::FrontRight),
            (SPEAKER_FRONT_CENTER, SpeakerPosition::FrontCenter),
            (SPEAKER_LOW_FREQUENCY, SpeakerPosition::LowFrequency),
            (SPEAKER_BACK_LEFT, SpeakerPosition::BackLeft),
            (SPEAKER_BACK_RIGHT, SpeakerPosition::BackRight),
            (
                SPEAKER_FRONT_LEFT_OF_CENTER,
                SpeakerPosition::FrontLeftOfCenter,
            ),
            (
                SPEAKER_FRONT_RIGHT_OF_CENTER,
                SpeakerPosition::FrontRightOfCenter,
            ),
            (SPEAKER_BACK_CENTER, SpeakerPosition::BackCenter),
            (SPEAKER_SIDE_LEFT, SpeakerPosition::SideLeft),
            (SPEAKER_SIDE_RIGHT, SpeakerPosition::SideRight),
        ];
        let mut pos = Vec::with_capacity(channels);
        for (bit, sp) in bits {
            if mask & bit != 0 {
                pos.push(sp);
                if pos.len() == channels {
                    return pos;
                }
            }
        }
        while pos.len() < channels {
            pos.push(if pos.len() % 2 == 0 {
                SpeakerPosition::UnknownLeft
            } else {
                SpeakerPosition::UnknownRight
            });
        }
        return pos;
    }
    fallback_positions(channels)
}

fn fallback_positions(ch: usize) -> Vec<SpeakerPosition> {
    match ch {
        1 => vec![SpeakerPosition::FrontCenter],
        2 => vec![SpeakerPosition::FrontLeft, SpeakerPosition::FrontRight],
        6 => vec![
            SpeakerPosition::FrontLeft,
            SpeakerPosition::FrontRight,
            SpeakerPosition::FrontCenter,
            SpeakerPosition::LowFrequency,
            SpeakerPosition::SideLeft,
            SpeakerPosition::SideRight,
        ],
        8 => vec![
            SpeakerPosition::FrontLeft,
            SpeakerPosition::FrontRight,
            SpeakerPosition::FrontCenter,
            SpeakerPosition::LowFrequency,
            SpeakerPosition::BackLeft,
            SpeakerPosition::BackRight,
            SpeakerPosition::SideLeft,
            SpeakerPosition::SideRight,
        ],
        _ => (0..ch)
            .map(|i| {
                if i % 2 == 0 {
                    SpeakerPosition::UnknownLeft
                } else {
                    SpeakerPosition::UnknownRight
                }
            })
            .collect(),
    }
}

impl SpeakerEnergies {
    fn add(&mut self, pos: SpeakerPosition, val: f32) {
        match pos {
            SpeakerPosition::FrontLeft => self.front_left = self.front_left.max(val),
            SpeakerPosition::FrontRight => self.front_right = self.front_right.max(val),
            SpeakerPosition::FrontCenter => self.front_center = self.front_center.max(val),
            SpeakerPosition::LowFrequency => self.low_frequency = self.low_frequency.max(val),
            SpeakerPosition::BackLeft => self.back_left = self.back_left.max(val),
            SpeakerPosition::BackRight => self.back_right = self.back_right.max(val),
            SpeakerPosition::FrontLeftOfCenter => {
                self.front_left_of_center = self.front_left_of_center.max(val)
            }
            SpeakerPosition::FrontRightOfCenter => {
                self.front_right_of_center = self.front_right_of_center.max(val)
            }
            SpeakerPosition::BackCenter => self.back_center = self.back_center.max(val),
            SpeakerPosition::SideLeft => self.side_left = self.side_left.max(val),
            SpeakerPosition::SideRight => self.side_right = self.side_right.max(val),
            SpeakerPosition::UnknownLeft => self.unknown_left = self.unknown_left.max(val),
            SpeakerPosition::UnknownRight => self.unknown_right = self.unknown_right.max(val),
        }
    }
}
