use crate::elevation;
use crate::logging;
use crate::models::{
    ControlStatusDto, DiagnosticsExportDto, LogSnapshotDto, RadarSnapshotDto, RuntimeSnapshotDto,
};
use crate::native_control;
use crate::single_instance;
use crate::AppState;
use tauri::State;

use std::sync::atomic::{AtomicBool, Ordering};

static AUTO_REPAIR_ATTEMPTED: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn refresh_runtime(state: State<'_, AppState>) -> Result<RuntimeSnapshotDto, String> {
    tracing::info!("refresh_runtime command received");
    let _lock = acquire_runtime_lock(&state)?;
    let snapshot = build_runtime_snapshot();
    log_runtime_snapshot("refresh_runtime", &snapshot);

    if should_auto_repair(&snapshot) && !AUTO_REPAIR_ATTEMPTED.swap(true, Ordering::SeqCst) {
        let profile_id = snapshot
            .status
            .as_ref()
            .and_then(|s| s.active_profile)
            .unwrap_or(0);
        tracing::warn!(
            profile_id,
            health = snapshot.install_health.as_deref().unwrap_or("unknown"),
            "refresh_runtime: auto-repairing stale materialization (one-shot)"
        );
        let repaired = attempt_auto_repair(profile_id);
        if repaired {
            let fresh = build_runtime_snapshot();
            log_runtime_snapshot("refresh_runtime:auto_repaired", &fresh);
            return Ok(fresh);
        }
    }

    Ok(snapshot)
}

fn should_auto_repair(snapshot: &RuntimeSnapshotDto) -> bool {
    let health_broken = snapshot
        .install_health
        .as_deref()
        .is_some_and(|h| h == "ApplyFailed" || h == "NeedsRepair");
    let has_active_profile = snapshot
        .status
        .as_ref()
        .and_then(|s| s.active_profile)
        .is_some_and(|p| p > 0);
    let endpoint_missing = snapshot
        .install_health
        .as_deref()
        .is_some_and(|h| h == "EndpointMissing");
    (health_broken && has_active_profile && snapshot.installed)
        || (endpoint_missing && snapshot.installed)
}

fn attempt_auto_repair(profile_id: u32) -> bool {
    // Always try to repair the device selector first — handles renamed/missing endpoints
    match native_control::repair_device_selector() {
        Ok(_) => tracing::info!("auto-repair: device selector repaired"),
        Err(err) => {
            tracing::debug!(error = %err, "auto-repair: device selector repair failed (may need elevation)");
            // Try elevated repair
            let _ = elevation::run_self_elevated(&["repair-device-selector"]);
        }
    }

    if profile_id == 0 {
        return false;
    }
    match native_control::switch_profile(profile_id) {
        Ok(_) => {
            crate::dsp_core::set_enabled(false);
            tracing::info!(profile_id, "auto-repair switch succeeded");
            true
        }
        Err(err) if elevation::should_retry_elevated(&err) => {
            tracing::warn!(profile_id, error = %err, "auto-repair retrying elevated");
            let profile_text = profile_id.to_string();
            match elevation::run_self_elevated(&["switch", profile_text.as_str()]) {
                Ok(_) => {
                    crate::dsp_core::set_enabled(false);
                    tracing::info!(profile_id, "auto-repair elevated switch succeeded");
                    true
                }
                Err(elev_err) => {
                    tracing::error!(profile_id, error = %elev_err, "auto-repair elevated failed");
                    false
                }
            }
        }
        Err(err) => {
            tracing::error!(profile_id, error = %err, "auto-repair switch failed");
            false
        }
    }
}

#[tauri::command]
pub fn switch_profile(
    state: State<'_, AppState>,
    profile_id: u32,
) -> Result<RuntimeSnapshotDto, String> {
    if !(1..=4).contains(&profile_id) {
        return Err("Profile id invalido. Debe estar entre 1 y 4.".to_string());
    }

    tracing::info!(profile_id, "switch_profile command received");
    let _lock = acquire_runtime_lock(&state)?;
    ensure_runtime_ready()?;

    match native_control::switch_profile(profile_id) {
        Ok(_) => {
            crate::dsp_core::set_enabled(false);
            let snapshot = build_runtime_snapshot();
            log_runtime_snapshot("switch_profile", &snapshot);
            Ok(snapshot)
        }
        Err(err) if elevation::should_retry_elevated(&err) => {
            tracing::warn!(profile_id, error = %err, "switch_profile retrying elevated");
            let profile_text = profile_id.to_string();
            elevation::run_self_elevated(&["switch", profile_text.as_str()])
                .map_err(|error| error.to_string())?;
            crate::dsp_core::set_enabled(false);
            let snapshot = build_runtime_snapshot();
            log_runtime_snapshot("switch_profile:elevated", &snapshot);
            Ok(snapshot)
        }
        Err(err) => {
            tracing::error!(profile_id, error = %err, "switch_profile failed");
            Err(err.to_string())
        }
    }
}

#[tauri::command]
pub fn clear_profile(state: State<'_, AppState>) -> Result<RuntimeSnapshotDto, String> {
    tracing::info!("clear_profile command received");
    let _lock = acquire_runtime_lock(&state)?;

    match native_control::clear_profile() {
        Ok(_) => {
            crate::dsp_core::set_enabled(false);
            let snapshot = build_runtime_snapshot();
            log_runtime_snapshot("clear_profile", &snapshot);
            Ok(snapshot)
        }
        Err(err) if elevation::should_retry_elevated(&err) => {
            tracing::warn!(error = %err, "clear_profile retrying elevated");
            elevation::run_self_elevated(&["clear"]).map_err(|error| error.to_string())?;
            crate::dsp_core::set_enabled(false);
            let snapshot = build_runtime_snapshot();
            log_runtime_snapshot("clear_profile:elevated", &snapshot);
            Ok(snapshot)
        }
        Err(err) => {
            tracing::error!(error = %err, "clear_profile failed");
            Err(err.to_string())
        }
    }
}

#[tauri::command]
pub fn repair_device_selector(state: State<'_, AppState>) -> Result<RuntimeSnapshotDto, String> {
    tracing::info!("repair_device_selector command received");
    let _lock = acquire_runtime_lock(&state)?;

    match native_control::repair_device_selector() {
        Ok(_) => {
            let snapshot = build_runtime_snapshot();
            log_runtime_snapshot("repair_device_selector", &snapshot);
            Ok(snapshot)
        }
        Err(err) if elevation::should_retry_elevated(&err) => {
            tracing::warn!(error = %err, "repair_device_selector retrying elevated");
            elevation::run_self_elevated(&["repair-device-selector"])
                .map_err(|error| error.to_string())?;
            let snapshot = build_runtime_snapshot();
            log_runtime_snapshot("repair_device_selector:elevated", &snapshot);
            Ok(snapshot)
        }
        Err(err) => {
            tracing::error!(error = %err, "repair_device_selector failed");
            Err(err.to_string())
        }
    }
}

#[tauri::command]
pub fn set_spatial_mode(
    state: State<'_, AppState>,
    mode: String,
) -> Result<RuntimeSnapshotDto, String> {
    let mode = native_control::SpatialMode::from_input(&mode).map_err(|error| error.to_string())?;
    tracing::info!(mode = mode.as_str(), "set_spatial_mode command received");
    let _lock = acquire_runtime_lock(&state)?;

    match native_control::set_spatial_mode(mode) {
        Ok(_) => {
            let snapshot = build_runtime_snapshot();
            log_runtime_snapshot("set_spatial_mode", &snapshot);
            Ok(snapshot)
        }
        Err(err) if elevation::should_retry_elevated(&err) => {
            tracing::warn!(mode = mode.as_str(), error = %err, "set_spatial_mode retrying elevated");
            elevation::run_self_elevated(&["set-spatial-mode", mode.as_str()])
                .map_err(|error| error.to_string())?;
            let snapshot = build_runtime_snapshot();
            log_runtime_snapshot("set_spatial_mode:elevated", &snapshot);
            Ok(snapshot)
        }
        Err(err) => {
            tracing::error!(mode = mode.as_str(), error = %err, "set_spatial_mode failed");
            Err(err.to_string())
        }
    }
}

fn acquire_runtime_lock<'a>(
    state: &'a State<'a, AppState>,
) -> Result<RuntimeOperationGuard<'a>, String> {
    let local_guard = state
        .runtime_lock
        .lock()
        .map_err(|_| "Runtime lock is poisoned.".to_string())?;
    let global_guard =
        single_instance::acquire_runtime_operation_guard().map_err(|error| error.to_string())?;

    Ok(RuntimeOperationGuard {
        _local_guard: local_guard,
        _global_guard: global_guard,
    })
}

struct RuntimeOperationGuard<'a> {
    _local_guard: std::sync::MutexGuard<'a, ()>,
    _global_guard: single_instance::NamedMutexGuard,
}

#[tauri::command]
pub fn run_installer(
    state: State<'_, AppState>,
) -> Result<crate::models::InstallerStateDto, String> {
    state.installer.start().map_err(|error| error.to_string())?;
    Ok(state.installer.snapshot())
}

#[tauri::command]
pub fn get_installer_state(
    state: State<'_, AppState>,
) -> Result<crate::models::InstallerStateDto, String> {
    Ok(state.installer.snapshot())
}

#[tauri::command]
pub fn append_frontend_log(level: String, message: String) -> Result<bool, String> {
    logging::append_frontend_log(&level, &message);
    Ok(true)
}

#[tauri::command]
pub fn get_log_snapshot() -> Result<LogSnapshotDto, String> {
    Ok(logging::log_snapshot())
}

#[tauri::command]
pub fn get_debug_report() -> Result<String, String> {
    Ok(logging::debug_report())
}

#[tauri::command]
pub fn export_diagnostics(state: State<'_, AppState>) -> Result<DiagnosticsExportDto, String> {
    tracing::info!("export_diagnostics command received");
    let _lock = acquire_runtime_lock(&state)?;
    let archive_path =
        native_control::export_diagnostics_bundle().map_err(|error| error.to_string())?;
    Ok(DiagnosticsExportDto {
        archive_path: archive_path.display().to_string(),
    })
}

#[tauri::command]
pub fn get_radar_snapshot(state: State<'_, AppState>) -> Result<RadarSnapshotDto, String> {
    Ok(state.radar.snapshot())
}

#[tauri::command]
pub fn set_overlay_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state.radar.set_overlay_enabled(enabled);
    Ok(())
}

#[tauri::command]
pub fn set_mini_radar_enabled(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state.radar.set_mini_radar_enabled(enabled);
    Ok(())
}

#[tauri::command]
pub fn set_mini_radar_position(state: State<'_, AppState>, position: u8) -> Result<(), String> {
    state.radar.set_mini_radar_position(position);
    Ok(())
}

#[tauri::command]
pub fn get_log_root() -> Result<String, String> {
    Ok(logging::log_root().display().to_string())
}

#[tauri::command]
pub fn list_audio_outputs() -> Result<Vec<crate::models::AudioDeviceDto>, String> {
    tracing::info!("list_audio_outputs command received");
    native_control::list_audio_outputs().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_audio_output(
    state: State<'_, AppState>,
    device_id: String,
) -> Result<RuntimeSnapshotDto, String> {
    tracing::info!(device_id = %device_id, "set_audio_output command received");
    let _lock = acquire_runtime_lock(&state)?;
    native_control::set_audio_output(&device_id).map_err(|error| error.to_string())?;
    // Signal the render thread to reconnect to the new output device
    state.radar.restart_render();
    tracing::info!("Render thread restart signaled for new output device");
    let snapshot = build_runtime_snapshot();
    log_runtime_snapshot("set_audio_output", &snapshot);
    Ok(snapshot)
}

fn ensure_runtime_ready() -> Result<(), String> {
    match native_control::bootstrap_runtime() {
        Ok(status) => {
            tracing::info!(
                config_mode = ?status.config_mode,
                active_profile = ?status.active_profile,
                selector_active = ?status.device_selector_active,
                endpoint = ?status.target_endpoint_guid,
                "runtime ready"
            );
            Ok(())
        }
        Err(err) if elevation::should_retry_elevated(&err) => {
            tracing::warn!(error = %err, "bootstrap_runtime retrying elevated");
            elevation::run_self_elevated(&["bootstrap-runtime"])
                .map_err(|error| error.to_string())?;
            Ok(())
        }
        Err(err) => {
            tracing::error!(error = %err, "bootstrap_runtime failed");
            Err(err.to_string())
        }
    }
}

pub fn build_runtime_snapshot() -> RuntimeSnapshotDto {
    let helper_available = native_control::is_windows_supported();
    let mut errors = Vec::new();

    let status = match native_control::status() {
        Ok(status) => Some(map_status(status)),
        Err(err) => {
            errors.push(err.to_string());
            None
        }
    };

    let verify = match native_control::verify_status() {
        Ok(verify) => Some(map_verify(verify)),
        Err(err) => {
            errors.push(err.to_string());
            None
        }
    };

    let runtime_error = (!errors.is_empty()).then(|| errors.join(" | "));
    let verify_state_ok = verify
        .as_ref()
        .map(|current| {
            matches!(
                current.verify_state.as_deref(),
                Some("matched") | Some("cleared") | Some("ok")
            )
        })
        .unwrap_or(false);
    let revision_confirmed = match (
        status.as_ref().and_then(|current| current.active_profile),
        status
            .as_ref()
            .and_then(|current| current.active_revision.as_deref()),
        status
            .as_ref()
            .and_then(|current| current.last_confirmed_revision.as_deref()),
    ) {
        (Some(active_profile), _, _) if active_profile == 0 => true,
        (_, Some(active_revision), Some(last_confirmed_revision)) => {
            active_revision == last_confirmed_revision
        }
        _ => false,
    };
    let verify_ok = verify_state_ok && revision_confirmed;
    let install_health = verify
        .as_ref()
        .and_then(|current| current.install_health.clone())
        .or_else(|| {
            status
                .as_ref()
                .and_then(|current| current.install_health.clone())
        });
    let installed = runtime_is_deployed(
        helper_available,
        install_health.as_deref(),
        status.as_ref(),
        verify.as_ref(),
    );

    let receipt_status = crate::install::validate_install_receipt();
    let receipt_version = crate::install::read_install_receipt().map(|receipt| receipt.version);

    RuntimeSnapshotDto {
        helper_available,
        install_health,
        installed,
        needs_installation: !installed,
        receipt_status: Some(receipt_status.as_str().to_string()),
        receipt_version,
        runtime_error,
        status,
        verify,
        verify_ok,
    }
}

fn runtime_is_deployed(
    helper_available: bool,
    install_health: Option<&str>,
    status: Option<&ControlStatusDto>,
    verify: Option<&ControlStatusDto>,
) -> bool {
    if !helper_available {
        return false;
    }

    match install_health {
        Some("Healthy" | "NeedsRepair" | "ApplyFailed" | "EndpointMissing" | "PluginMissing") => {
            true
        }
        Some("NeedsInstall" | "VersionMismatch") => false,
        _ => runtime_assets_present(status, verify),
    }
}

fn runtime_assets_present(
    status: Option<&ControlStatusDto>,
    verify: Option<&ControlStatusDto>,
) -> bool {
    let bundle_present = status
        .and_then(|current| current.bundle_present)
        .unwrap_or(false)
        || status
            .and_then(|current| current.bundle_sha256.as_deref())
            .is_some_and(has_text)
        || verify
            .and_then(|current| current.bundle_sha256.as_deref())
            .is_some_and(has_text);

    let helper_present = status
        .and_then(|current| current.helper_version.as_deref())
        .is_some_and(has_text)
        || status
            .and_then(|current| current.helper_path.as_deref())
            .is_some_and(has_text)
        || verify
            .and_then(|current| current.helper_version.as_deref())
            .is_some_and(has_text)
        || verify
            .and_then(|current| current.helper_path.as_deref())
            .is_some_and(has_text);

    bundle_present && helper_present
}

fn has_text(value: &str) -> bool {
    !value.trim().is_empty()
}

fn map_status(status: native_control::ControlStatus) -> ControlStatusDto {
    ControlStatusDto {
        active_profile: status.active_profile,
        active_revision: status.active_revision,
        bundle_present: status.bundle_present,
        bundle_sha256: status.bundle_sha256,
        config_dir: status.config_dir.map(|path| path.display().to_string()),
        config_mode: status.config_mode,
        device_selector_active: status.device_selector_active,
        engine_dir: status.engine_dir.map(|path| path.display().to_string()),
        helper_log_path: status
            .helper_log_path
            .map(|path| path.display().to_string()),
        helper_path: status.helper_path.map(|path| path.display().to_string()),
        helper_version: status.helper_version,
        install_health: status.install_health,
        last_confirmed_revision: status.last_confirmed_revision,
        plugin_path: status.plugin_path.map(|path| path.display().to_string()),
        plugin_present: status.plugin_present,
        spatial_channel_mask: status.spatial_channel_mask,
        spatial_mode: status.spatial_mode,
        target_endpoint_guid: status.target_endpoint_guid,
        target_endpoint_name: status.target_endpoint_name,
        verify_reason: None,
        verify_state: None,
    }
}

fn map_verify(verify: native_control::VerifyStatus) -> ControlStatusDto {
    ControlStatusDto {
        active_profile: verify.active_profile,
        active_revision: verify.active_revision,
        bundle_present: None,
        bundle_sha256: verify.bundle_sha256,
        config_dir: None,
        config_mode: verify.config_mode,
        device_selector_active: verify.device_selector_active,
        engine_dir: None,
        helper_log_path: verify
            .helper_log_path
            .map(|path| path.display().to_string()),
        helper_path: verify.helper_path.map(|path| path.display().to_string()),
        helper_version: verify.helper_version,
        install_health: verify.install_health,
        last_confirmed_revision: verify.last_confirmed_revision,
        plugin_path: verify.plugin_path.map(|path| path.display().to_string()),
        plugin_present: verify.plugin_present,
        spatial_channel_mask: verify.spatial_channel_mask,
        spatial_mode: verify.spatial_mode,
        target_endpoint_guid: verify.target_endpoint_guid,
        target_endpoint_name: verify.target_endpoint_name,
        verify_reason: verify.verify_reason,
        verify_state: verify.verify_state,
    }
}

fn log_runtime_snapshot(context: &str, snapshot: &RuntimeSnapshotDto) {
    tracing::info!(
        context,
        helper_available = snapshot.helper_available,
        installed = snapshot.installed,
        verify_ok = snapshot.verify_ok,
        runtime_error = %snapshot.runtime_error.as_deref().unwrap_or(""),
        status = %summarize_control_status(snapshot.status.as_ref()),
        verify = %summarize_control_status(snapshot.verify.as_ref()),
        "runtime snapshot"
    );
}

fn summarize_control_status(status: Option<&ControlStatusDto>) -> String {
    let Some(status) = status else {
        return "<none>".to_string();
    };

    format!(
        "active_profile={:?} active_revision={:?} last_confirmed_revision={:?} config_mode={:?} selector={:?} helper_version={:?} install_health={:?} endpoint_guid={:?} endpoint_name={:?} spatial_mode={:?} spatial_mask={:?} verify_state={:?} verify_reason={:?}",
        status.active_profile,
        status.active_revision,
        status.last_confirmed_revision,
        status.config_mode,
        status.device_selector_active,
        status.helper_version,
        status.install_health,
        status.target_endpoint_guid,
        status.target_endpoint_name,
        status.spatial_mode,
        status.spatial_channel_mask,
        status.verify_state,
        status.verify_reason
    )
}

#[tauri::command]
pub fn reboot_system() -> Result<(), String> {
    tracing::info!("reboot_system command received — initiating Windows reboot");
    std::process::Command::new("shutdown")
        .args([
            "/r",
            "/t",
            "3",
            "/c",
            "VanySound setup complete — rebooting now.",
        ])
        .spawn()
        .map_err(|e| format!("Failed to initiate reboot: {}", e))?;
    Ok(())
}

/// DSP Engine Control — updates the FirstEdition native realtime engine.
///
/// Equalizer APO is kept neutral/scoped to the cable; realtime processing happens
/// only on the captured cable stream before it is rendered to the selected output.
#[tauri::command]
pub fn apply_dsp_config(
    state: State<'_, AppState>,
    params: std::collections::HashMap<String, f64>,
) -> Result<RuntimeSnapshotDto, String> {
    tracing::info!(
        "apply_dsp_config command received with {} params",
        params.len()
    );
    let _lock = acquire_runtime_lock(&state)?;

    // Handle bypass mode — clear all processing.
    if params.get("_bypass").copied().unwrap_or(0.0) > 0.5 {
        tracing::info!("apply_dsp_config: BYPASS mode — clearing all processing");
        crate::dsp_core::set_enabled(false);
        native_control::apply_raw_config("Preamp: 0 dB\n").map_err(|e| e.to_string())?;
        let snapshot = build_runtime_snapshot();
        log_runtime_snapshot("apply_dsp_config:bypass", &snapshot);
        return Ok(snapshot);
    }

    if crate::dsp_core::needs_eqapo_neutralization() {
        native_control::apply_raw_config("Preamp: 0 dB\n").map_err(|e| {
            crate::dsp_core::mark_eqapo_dirty();
            e.to_string()
        })?;
    }

    let engine_params = crate::dsp_core::EngineParams::from_map(&params);
    crate::dsp_core::apply_params(engine_params).map_err(|e| {
        crate::dsp_core::mark_eqapo_dirty();
        e.to_string()
    })?;

    if let Some(scores) = crate::dsp_core::scores() {
        tracing::info!(
            footstep = scores.footstep,
            action = scores.action,
            protection = scores.protection,
            confidence = scores.confidence,
            "apply_dsp_config: FirstEdition realtime engine updated"
        );
    } else {
        tracing::info!("apply_dsp_config: FirstEdition realtime engine updated");
    }

    let snapshot = build_runtime_snapshot();
    log_runtime_snapshot("apply_dsp_config", &snapshot);
    Ok(snapshot)
}
