mod analysis;
mod audio;
mod eq_control;
mod overlay;

use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("EchoAudio Radar starting...");

    // DPI awareness: sin esto, GetSystemMetrics devuelve coordenadas
    // virtualizadas en pantallas con scaling >100% y el overlay no
    // cubre toda la pantalla.
    unsafe {
        use windows::Win32::UI::HiDpi::{
            SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let _eq = eq_control::init_default_profile();

    let (tx, rx) = crossbeam_channel::bounded::<audio::AudioPacket>(4);

    let _audio_handle = audio::start_capture(tx);
    tracing::info!("Audio capture thread spawned");

    overlay::run(rx)
}
