use crossbeam_channel::Sender;
use std::collections::VecDeque;
use std::thread::{self, JoinHandle};
use tracing::{debug, error, info, warn};
use wasapi::*;

pub type AudioPacket = (Vec<f32>, u16, u32);

/// Inicia la captura de audio en loopback y envia chunks interleaved
/// junto con el numero de canales y el channel mask de WASAPI.
pub fn start_capture(tx: Sender<AudioPacket>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("audio-capture".to_string())
        .spawn(move || {
            if let Err(e) = capture_loop(tx) {
                error!("Audio capture error: {}", e);
            }
        })
        .expect("Failed to spawn audio capture thread")
}

fn capture_loop(tx: Sender<AudioPacket>) -> anyhow::Result<()> {
    initialize_mta().ok().map_err(|e| anyhow::anyhow!("COM init failed: {:?}", e))?;
    info!("COM initialized (MTA)");

    let enumerator = DeviceEnumerator::new()
        .map_err(|e| anyhow::anyhow!("DeviceEnumerator: {:?}", e))?;
    let device = enumerator
        .get_default_device(&Direction::Render)
        .map_err(|e| anyhow::anyhow!("get_default_device: {:?}", e))?;
    let dev_name = device
        .get_friendlyname()
        .map_err(|e| anyhow::anyhow!("get_friendlyname: {:?}", e))?;
    info!("Default output device: {}", dev_name);

    let mut audio_client = device
        .get_iaudioclient()
        .map_err(|e| anyhow::anyhow!("get_iaudioclient: {:?}", e))?;

    let mix_format = audio_client
        .get_mixformat()
        .map_err(|e| anyhow::anyhow!("get_mixformat: {:?}", e))?;
    let channels = mix_format.get_nchannels();
    let sample_rate = mix_format.get_samplespersec();
    let bits = mix_format.get_bitspersample();
    let blockalign = mix_format.get_blockalign();
    let channel_mask = mix_format.get_dwchannelmask();
    info!(
        "Mix format: {}ch, {}Hz, {}bit, blockalign={}, mask=0x{:X}",
        channels, sample_rate, bits, blockalign, channel_mask
    );

    let (def_period, min_period) = audio_client
        .get_device_period()
        .map_err(|e| anyhow::anyhow!("get_device_period: {:?}", e))?;
    debug!("Device period: default={}, min={}", def_period, min_period);

    let mode = StreamMode::EventsShared {
        autoconvert: true,
        buffer_duration_hns: def_period,
    };
    audio_client
        .initialize_client(&mix_format, &Direction::Capture, &mode)
        .map_err(|e| anyhow::anyhow!("initialize_client: {:?}", e))?;
    info!("Audio client initialized in loopback capture mode");

    let h_event = audio_client
        .set_get_eventhandle()
        .map_err(|e| anyhow::anyhow!("set_get_eventhandle: {:?}", e))?;

    let capture_client = audio_client
        .get_audiocaptureclient()
        .map_err(|e| anyhow::anyhow!("get_audiocaptureclient: {:?}", e))?;

    audio_client
        .start_stream()
        .map_err(|e| anyhow::anyhow!("start_stream: {:?}", e))?;
    info!("Loopback capture started");

    let mut sample_queue: VecDeque<u8> = VecDeque::with_capacity(blockalign as usize * 4096);

    let chunk_frames: usize = 1024;
    let chunk_bytes = chunk_frames * blockalign as usize;
    let bytes_per_sample = (bits / 8) as usize;

    loop {
        if h_event.wait_for_event(2_000_000).is_err() {
            let silence = vec![0.0f32; chunk_frames * channels as usize];
            let _ = tx.try_send((silence, channels, channel_mask));
            continue;
        }

        match capture_client.read_from_device_to_deque(&mut sample_queue) {
            Ok(_buffer_info) => {}
            Err(e) => {
                warn!("Error reading from device: {}", e);
                continue;
            }
        }

        while sample_queue.len() >= chunk_bytes {
            let mut chunk_raw = vec![0u8; chunk_bytes];
            for byte in &mut chunk_raw {
                *byte = sample_queue.pop_front().unwrap();
            }

            let samples: Vec<f32> = if bits == 32 {
                chunk_raw
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect()
            } else if bits == 16 {
                chunk_raw
                    .chunks_exact(2)
                    .map(|b| {
                        let val = i16::from_le_bytes([b[0], b[1]]);
                        val as f32 / 32768.0
                    })
                    .collect()
            } else if bits == 24 {
                chunk_raw
                    .chunks_exact(bytes_per_sample)
                    .map(|b| {
                        let val = (b[0] as i32) | ((b[1] as i32) << 8) | ((b[2] as i32) << 16);
                        let val = if val & 0x800000 != 0 {
                            val | !0xFFFFFF
                        } else {
                            val
                        };
                        val as f32 / 8_388_608.0
                    })
                    .collect()
            } else {
                warn!("Unsupported bit depth: {}", bits);
                continue;
            };

            if tx.try_send((samples, channels, channel_mask)).is_err() {
                debug!("Audio channel full, dropping chunk");
            }
        }
    }
}
