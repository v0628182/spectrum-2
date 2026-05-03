mod audio_output;
mod commands;
mod dsp_core;
mod elevation;
mod embedded_scripts;
mod install;
mod integrity;
mod logging;
mod mini_radar;
mod models;
mod native_control;
mod overlay;
mod radar;
mod single_instance;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct AppState {
    installer: install::InstallerManager,
    radar: radar::RadarService,
    runtime_lock: Mutex<()>,
}

pub fn maybe_run_embedded_cli() -> Option<i32> {
    logging::init_logging();

    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some(elevation::native_sentinel()) {
        return None;
    }

    let cli_args = args.into_iter().skip(2).collect::<Vec<_>>();
    let exit_code = match native_control::command_from_cli(&cli_args) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("ERROR: {err}");
            tracing::error!(error = %err, "embedded native CLI failed");
            1
        }
    };

    Some(exit_code)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    logging::init_logging();

    // Request admin privileges once at startup — eliminates per-operation UAC
    elevation::ensure_elevated();

    let _instance_guard = match single_instance::acquire_instance_guard() {
        Ok(guard) => guard,
        Err(error) => {
            tracing::warn!(error = %error, "second instance rejected");
            return;
        }
    };

    // Global shutdown flag shared across all background threads
    let shutdown = Arc::new(AtomicBool::new(false));

    let state = AppState {
        installer: install::InstallerManager::default(),
        radar: radar::RadarService::start(),
        runtime_lock: Mutex::new(()),
    };

    // Online-only gate: monitor connectivity in background
    let shutdown_connectivity = Arc::clone(&shutdown);
    let (_connectivity_flag, connectivity_join) =
        integrity::spawn_connectivity_monitor(shutdown_connectivity);

    tracing::info!("Starting VanySound Tauri shell");

    let run_result = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::append_frontend_log,
            commands::apply_dsp_config,
            commands::clear_profile,
            commands::export_diagnostics,
            commands::get_debug_report,
            commands::get_installer_state,
            commands::get_log_root,
            commands::get_log_snapshot,
            commands::get_radar_snapshot,
            commands::list_audio_outputs,
            commands::refresh_runtime,
            commands::repair_device_selector,
            commands::run_installer,
            commands::set_audio_output,
            commands::set_mini_radar_enabled,
            commands::set_mini_radar_position,
            commands::set_overlay_enabled,
            commands::set_spatial_mode,
            commands::switch_profile,
            commands::reboot_system
        ])
        .run(tauri::generate_context!());

    // ── Graceful shutdown: signal all threads and join them ──
    tracing::info!("Tauri shell returned — initiating graceful shutdown");
    shutdown.store(true, Ordering::SeqCst);

    // Join connectivity monitor
    tracing::info!("Joining connectivity-monitor thread...");
    match connectivity_join.join() {
        Ok(()) => tracing::info!("connectivity-monitor joined OK"),
        Err(_) => tracing::error!("connectivity-monitor panicked on join"),
    }

    // RadarService::Drop will handle the rest (capture, analysis, render, overlay)

    match run_result {
        Ok(()) => {
            tracing::info!("VanySound Tauri shell exited normally (window closed by user)");
        }
        Err(e) => {
            tracing::error!("VanySound Tauri shell CRASHED: {}", e);
            // Write sync log as fallback in case tracing doesn't flush
            logging::append_line(
                &logging::log_root().join("crash.log"),
                "FATAL",
                &format!("Tauri shell crashed: {}", e),
            );
        }
    }
}
