use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ControlStatusDto {
    pub active_profile: Option<u32>,
    pub active_revision: Option<String>,
    pub bundle_present: Option<bool>,
    pub bundle_sha256: Option<String>,
    pub config_dir: Option<String>,
    pub config_mode: Option<String>,
    pub device_selector_active: Option<bool>,
    pub engine_dir: Option<String>,
    pub helper_log_path: Option<String>,
    pub helper_path: Option<String>,
    pub helper_version: Option<String>,
    pub install_health: Option<String>,
    pub last_confirmed_revision: Option<String>,
    pub plugin_path: Option<String>,
    pub plugin_present: Option<bool>,
    pub spatial_channel_mask: Option<u32>,
    pub spatial_mode: Option<String>,
    pub target_endpoint_guid: Option<String>,
    pub target_endpoint_name: Option<String>,
    pub verify_reason: Option<String>,
    pub verify_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSnapshotDto {
    pub helper_available: bool,
    pub install_health: Option<String>,
    pub installed: bool,
    pub needs_installation: bool,
    pub receipt_status: Option<String>,
    pub receipt_version: Option<String>,
    pub runtime_error: Option<String>,
    pub status: Option<ControlStatusDto>,
    pub verify: Option<ControlStatusDto>,
    pub verify_ok: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsExportDto {
    pub archive_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallerStateDto {
    pub completed: bool,
    pub current_step: u32,
    pub detail: String,
    pub exit_code: Option<i32>,
    pub finished_at: Option<String>,
    pub headline: String,
    pub is_installed: bool,
    pub log_lines: Vec<String>,
    pub progress: u32,
    pub running: bool,
    pub started_at: Option<String>,
    pub success: Option<bool>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFileDto {
    pub key: String,
    pub path: String,
    pub tail: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSnapshotDto {
    pub combined_tail: Vec<String>,
    pub files: Vec<LogFileDto>,
    pub log_dir: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RadarSnapshotDto {
    pub ambience: f32,
    pub capture_active: bool,
    pub center: f32,
    pub channel_mask: u32,
    pub channels: u16,
    pub far_left: f32,
    pub far_right: f32,
    pub last_error: Option<String>,
    pub last_update_ms: Option<u64>,
    pub left: f32,
    pub pan: f32,
    pub right: f32,
    pub spectrum: Vec<f32>,
    pub spectrum_peak_hz: f32,
    pub volume: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDeviceDto {
    pub id: String,
    pub name: String,
    pub is_active: bool,
    pub is_default: bool,
}
