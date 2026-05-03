use crate::logging;
use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use winreg::enums::*;
use winreg::{RegKey, RegValue};

const REGISTRY_PATH: &str = r"SOFTWARE\VanySound";
const RENDER_DEVICES_REGISTRY_PATH: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render";
const HIFI_ENDPOINT_GUID_VALUE_NAME: &str = "HiFiEndpointGuid";
const BUNDLE_SHA256_VALUE_NAME: &str = "BundleSha256";
const MJUCJR_DLL_NAME: &str = "MJUCjr.dll";
const PKEY_AUDIOENDPOINT_PHYSICAL_SPEAKERS: &str = "{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},3";
const PKEY_AUDIOENDPOINT_FULL_RANGE_SPEAKERS: &str = "{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},6";
const PKEY_AUDIOENGINE_DEVICE_FORMAT: &str = "{f19f064d-082c-4e27-bc73-6882a1bb8e4c},0";
const PROPERTY_STORE_VT_BLOB: u32 = 65;
const SERIALIZED_WAVEFORMAT_OFFSET: usize = 8;
const WAVEFORMATEX_SIZE: usize = 18;
const WAVEFORMAT_EXTENSIBLE: u16 = 0xFFFE;
const WAVEFORMATEXTENSIBLE_CB_SIZE: u16 = 22;
const CHANNEL_MASK_OFFSET: usize = SERIALIZED_WAVEFORMAT_OFFSET + 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialMode {
    Stereo,
    Surround51,
    Surround71,
}

impl SpatialMode {
    pub fn from_input(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "stereo" => Ok(Self::Stereo),
            "5.1" => Ok(Self::Surround51),
            "7.1" => Ok(Self::Surround71),
            other => Err(anyhow!(
                "Modo espacial invalido: {other}. Usa stereo, 5.1 o 7.1."
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stereo => "stereo",
            Self::Surround51 => "5.1",
            Self::Surround71 => "7.1",
        }
    }

    fn channel_count(self) -> u16 {
        match self {
            Self::Stereo => 2,
            Self::Surround51 => 6,
            Self::Surround71 => 8,
        }
    }

    fn channel_mask(self) -> u32 {
        match self {
            Self::Stereo => 0x0003,
            Self::Surround51 => 0x060F,
            Self::Surround71 => 0x063F,
        }
    }

    fn from_channel_mask(mask: u32) -> Option<Self> {
        match mask {
            0x0003 => Some(Self::Stereo),
            0x060F => Some(Self::Surround51),
            0x063F => Some(Self::Surround71),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ControlStatus {
    pub active_profile: Option<u32>,
    pub active_revision: Option<String>,
    pub bundle_present: Option<bool>,
    pub bundle_sha256: Option<String>,
    pub config_dir: Option<PathBuf>,
    pub config_mode: Option<String>,
    pub device_selector_active: Option<bool>,
    pub engine_dir: Option<PathBuf>,
    pub helper_log_path: Option<PathBuf>,
    pub helper_path: Option<PathBuf>,
    pub helper_version: Option<String>,
    pub install_health: Option<String>,
    pub last_confirmed_revision: Option<String>,
    pub plugin_path: Option<PathBuf>,
    pub plugin_present: Option<bool>,
    pub spatial_channel_mask: Option<u32>,
    pub spatial_mode: Option<String>,
    pub target_endpoint_guid: Option<String>,
    pub target_endpoint_name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct VerifyStatus {
    pub active_profile: Option<u32>,
    pub active_revision: Option<String>,
    pub bundle_sha256: Option<String>,
    pub config_mode: Option<String>,
    pub device_selector_active: Option<bool>,
    pub helper_log_path: Option<PathBuf>,
    pub helper_path: Option<PathBuf>,
    pub helper_version: Option<String>,
    pub install_health: Option<String>,
    pub last_confirmed_revision: Option<String>,
    pub plugin_path: Option<PathBuf>,
    pub plugin_present: Option<bool>,
    pub spatial_channel_mask: Option<u32>,
    pub spatial_mode: Option<String>,
    pub target_endpoint_guid: Option<String>,
    pub target_endpoint_name: Option<String>,
    pub verify_reason: Option<String>,
    pub verify_state: Option<String>,
}

#[allow(dead_code)]
mod embedded {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/generated/vany_sound_control_embedded.rs"
    ));

    pub fn expected_helper_version() -> &'static str {
        HELPER_VERSION
    }

    pub fn status_struct() -> Result<super::ControlStatus> {
        let active_profile = load_active_profile();
        let endpoint = load_stored_target_endpoint();
        let selector_active = endpoint
            .as_ref()
            .map(|ep| is_device_selector_enabled(&ep.guid))
            .unwrap_or(false);

        Ok(super::ControlStatus {
            active_profile: (active_profile > 0).then_some(active_profile),
            active_revision: load_registry_string_value(MATERIALIZED_REVISION_VALUE_NAME),
            bundle_present: Some(bundle_available()),
            bundle_sha256: load_registry_string_value(super::BUNDLE_SHA256_VALUE_NAME),
            config_dir: Some(resolve_config_dir()),
            config_mode: Some(config_mode_for_profile(active_profile)),
            device_selector_active: Some(selector_active),
            engine_dir: Some(resolve_engine_root()),
            helper_log_path: Some(log_path()),
            helper_path: std::env::current_exe().ok(),
            helper_version: Some(HELPER_VERSION.to_string()),
            install_health: None,
            last_confirmed_revision: load_registry_string_value(LAST_CONFIRMED_REVISION_VALUE_NAME),
            plugin_path: None,
            plugin_present: None,
            spatial_channel_mask: None,
            spatial_mode: None,
            target_endpoint_guid: endpoint.as_ref().map(|ep| ep.guid.clone()),
            target_endpoint_name: endpoint.as_ref().map(|ep| ep.name.clone()),
        })
    }

    pub fn verify_struct() -> Result<super::VerifyStatus> {
        if !bundle_available() {
            bail!("Bundle not deployed.");
        }

        let active_profile = load_active_profile();
        let endpoint = load_stored_target_endpoint();
        let mut selector_active = endpoint
            .as_ref()
            .map(|ep| is_device_selector_enabled(&ep.guid))
            .unwrap_or(false);
        let config_mode = config_mode_for_profile(active_profile);

        let verify_state = if active_profile == 0 {
            let config_dir = resolve_config_dir();
            let config_path = config_dir.join("config.txt");
            let active_path = config_dir.join("sys_active.cfg");
            let aux_path = config_dir.join("sys_aux.cfg");
            if !config_path.is_file() {
                bail!("Cleared config.txt is missing.");
            }
            if active_path.is_file() || aux_path.is_file() {
                bail!("Active profile artifacts still exist after clear.");
            }

            let actual = std::fs::read_to_string(config_path).unwrap_or_default();
            let expected = render_cleared_config();
            if !config_texts_match(&expected, &actual) {
                bail!(
                    "Cleared config does not match expected decoy. {{{}}}",
                    build_content_fingerprint(&expected, &actual)
                );
            }

            "cleared".to_string()
        } else {
            let profiles = load_bundle(&bundle_path())?;
            let profile = require_profile(&profiles, active_profile)?;

            if !use_embedded_engine() {
                let Some(endpoint) = endpoint.as_ref() else {
                    bail!("HiFiEndpointGuid is not configured.");
                };

                if let Some(error) = validate_materialized_profile(
                    &profile,
                    &resolve_config_dir(),
                    &endpoint.guid,
                    &load_materialized_slot(),
                ) {
                    bail!(error);
                }

                selector_active = is_device_selector_enabled(&endpoint.guid);
            }

            "matched".to_string()
        };

        Ok(super::VerifyStatus {
            active_profile: (active_profile > 0).then_some(active_profile),
            active_revision: load_registry_string_value(MATERIALIZED_REVISION_VALUE_NAME),
            bundle_sha256: load_registry_string_value(super::BUNDLE_SHA256_VALUE_NAME),
            config_mode: Some(config_mode),
            device_selector_active: Some(selector_active),
            helper_log_path: Some(log_path()),
            helper_path: std::env::current_exe().ok(),
            helper_version: Some(HELPER_VERSION.to_string()),
            install_health: None,
            last_confirmed_revision: load_registry_string_value(LAST_CONFIRMED_REVISION_VALUE_NAME),
            plugin_path: None,
            plugin_present: None,
            spatial_channel_mask: None,
            spatial_mode: None,
            target_endpoint_guid: endpoint.as_ref().map(|ep| ep.guid.clone()),
            target_endpoint_name: endpoint.as_ref().map(|ep| ep.name.clone()),
            verify_reason: None,
            verify_state: Some(verify_state),
        })
    }

    pub fn switch_profile_internal(profile_id: u32) -> Result<super::ControlStatus> {
        let exit_code = command_switch_internal(profile_id)?;
        if exit_code != 0 {
            bail!("switch returned non-zero exit code {exit_code}");
        }
        status_struct()
    }

    pub fn pack_bundle_internal(profiles_dir: &Path, output_file: &Path) -> Result<()> {
        let exit_code = command_pack(profiles_dir, output_file)?;
        if exit_code != 0 {
            bail!("pack returned non-zero exit code {exit_code}");
        }
        Ok(())
    }

    pub fn deploy_bundle_internal(bundle_file: &Path) -> Result<()> {
        let exit_code = command_deploy(bundle_file)?;
        if exit_code != 0 {
            bail!("deploy returned non-zero exit code {exit_code}");
        }
        Ok(())
    }

    pub fn clear_profile_internal() -> Result<super::ControlStatus> {
        let exit_code = command_clear_internal()?;
        if exit_code != 0 {
            bail!("clear returned non-zero exit code {exit_code}");
        }
        status_struct()
    }

    pub fn repair_device_selector_internal() -> Result<super::ControlStatus> {
        let exit_code = command_repair_device_selector()?;
        if exit_code != 0 {
            bail!("repair-device-selector returned non-zero exit code {exit_code}");
        }
        status_struct()
    }

    pub fn bootstrap_runtime_internal() -> Result<super::ControlStatus> {
        super::ensure_plugin_installed(super::resolve_engine_dir_from_registry().as_ref())?;
        let mut status = status_struct()?;
        let bundle_present = status.bundle_present == Some(true);
        let profiles_available = resolve_workspace_profiles_root().is_some();
        let packaged_bundle_available = resolve_packaged_profiles_bundle().is_some();
        let bundle_outdated = if bundle_present && profiles_available {
            workspace_bundle_is_outdated()?
        } else if bundle_present && packaged_bundle_available {
            packaged_bundle_is_outdated()?
        } else {
            false
        };

        if !bundle_present {
            if profiles_available {
                pack_and_deploy_workspace_profiles()?;
                status = status_struct()?;
            } else if let Some(bundle) = resolve_packaged_profiles_bundle() {
                deploy_profiles_bundle(&bundle)?;
                status = status_struct()?;
            }
        } else if bundle_outdated {
            if profiles_available {
                pack_and_deploy_workspace_profiles()?;
            } else if let Some(bundle) = resolve_packaged_profiles_bundle() {
                deploy_profiles_bundle(&bundle)?;
            }
            status = status_struct()?;
        }

        let endpoint_missing = status
            .target_endpoint_guid
            .as_deref()
            .map(|value| value.trim().is_empty())
            .unwrap_or(true);
        if endpoint_missing {
            status = repair_device_selector_internal()?;
        }

        Ok(status)
    }

    fn stable_profile_fingerprint(profiles: &HashMap<u32, ProfilePayload>) -> String {
        let mut ids: Vec<u32> = profiles.keys().copied().collect();
        ids.sort_unstable();

        let mut hasher = Sha256::new();
        for id in ids {
            if let Some(profile) = profiles.get(&id) {
                let normalized = normalize_config(
                    &profile.config,
                    (!profile.strategy.is_empty()).then_some(profile.strategy.as_str()),
                );
                hasher.update(id.to_le_bytes());
                hasher.update(profile.name.as_bytes());
                hasher.update([0]);
                hasher.update(normalized.as_bytes());
                hasher.update([0xFF]);
            }
        }

        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    }

    fn load_workspace_profiles() -> Result<HashMap<u32, ProfilePayload>> {
        let profiles_root = resolve_workspace_profiles_root().ok_or_else(|| {
            anyhow!("No se encontro equalizerAPO\\Perfiles para comparar el bundle.")
        })?;
        let names = profile_names();
        let mut profiles = HashMap::new();

        for (id, name) in names {
            let dir = profiles_root.join(id.to_string());
            let config_path = dir.join("config.txt");
            if !config_path.is_file() {
                bail!("Missing config.txt for profile {id}");
            }
            let strategy_path = dir.join("strategy.txt");
            profiles.insert(
                id,
                ProfilePayload {
                    id,
                    name: name.to_string(),
                    config: std::fs::read_to_string(&config_path)?,
                    strategy: std::fs::read_to_string(&strategy_path).unwrap_or_default(),
                },
            );
        }

        Ok(profiles)
    }

    fn workspace_bundle_is_outdated() -> Result<bool> {
        if !bundle_available() {
            return Ok(true);
        }

        let workspace_profiles = load_workspace_profiles()?;
        let deployed_profiles = match load_bundle(&bundle_path()) {
            Ok(profiles) => profiles,
            Err(_) => return Ok(true),
        };

        let workspace_fingerprint = stable_profile_fingerprint(&workspace_profiles);
        let deployed_fingerprint = stable_profile_fingerprint(&deployed_profiles);

        Ok(workspace_fingerprint != deployed_fingerprint)
    }

    fn packaged_bundle_fingerprint(bundle: &Path) -> Result<String> {
        let profiles = load_bundle(bundle)?;
        Ok(stable_profile_fingerprint(&profiles))
    }

    fn packaged_bundle_is_outdated() -> Result<bool> {
        if !bundle_available() {
            return Ok(true);
        }

        let Some(packaged_bundle) = resolve_packaged_profiles_bundle() else {
            return Ok(false);
        };

        let packaged_fingerprint = packaged_bundle_fingerprint(&packaged_bundle)?;
        let deployed_fingerprint = match load_bundle(&bundle_path()) {
            Ok(profiles) => stable_profile_fingerprint(&profiles),
            Err(_) => return Ok(true),
        };

        Ok(packaged_fingerprint != deployed_fingerprint)
    }

    fn deploy_profiles_bundle(bundle: &Path) -> Result<()> {
        let deploy_code = command_deploy(bundle)?;
        if deploy_code != 0 {
            bail!("deploy returned non-zero exit code {deploy_code}");
        }
        Ok(())
    }

    pub fn pack_and_deploy_workspace_profiles() -> Result<()> {
        let profiles_root = resolve_workspace_profiles_root().ok_or_else(|| {
            anyhow!("No se encontro equalizerAPO\\Perfiles para generar el bundle.")
        })?;

        let bundle_path = std::env::temp_dir().join("VanySound").join("profiles.bin");
        if let Some(parent) = bundle_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let pack_code = command_pack(&profiles_root, &bundle_path)?;
        if pack_code != 0 {
            bail!("pack returned non-zero exit code {pack_code}");
        }

        deploy_profiles_bundle(&bundle_path)?;

        Ok(())
    }

    fn resolve_packaged_profiles_bundle() -> Option<PathBuf> {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut candidates = Vec::new();

        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(root) = current_exe.parent() {
                candidates.push(root.join("instalacion").join("profiles.bin"));
                candidates.push(
                    root.join("resources")
                        .join("instalacion")
                        .join("profiles.bin"),
                );
                candidates.push(root.join("_up_").join("instalacion").join("profiles.bin"));
            }
        }

        if let Some(root) = manifest_dir.parent() {
            candidates.push(root.join("instalacion").join("profiles.bin"));
        }

        for candidate in candidates {
            let candidate = candidate.canonicalize().unwrap_or(candidate);
            if candidate.is_file() {
                return Some(candidate);
            }
        }

        resolve_repo_root()
            .map(|root| root.join("instalacion").join("profiles.bin"))
            .map(|path| path.canonicalize().unwrap_or(path))
            .filter(|path| path.is_file())
    }

    pub fn resolve_workspace_profiles_root() -> Option<PathBuf> {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut candidates = Vec::new();

        if let Some(root) = manifest_dir.parent() {
            candidates.push(root.join("equalizerAPO").join("Perfiles"));
        }

        for candidate in candidates {
            let candidate = candidate.canonicalize().unwrap_or(candidate);
            if candidate.is_dir() {
                return Some(candidate);
            }
        }

        let candidates = resolve_repo_root().map(|root| {
            [
                root.join("equalizerAPO").join("Perfiles"),
                root.join("switch").join("equalizerAPO").join("Perfiles"),
            ]
        })?;

        candidates
            .into_iter()
            .map(|path| path.canonicalize().unwrap_or(path))
            .find(|path| path.is_dir())
    }

    fn resolve_repo_root() -> Option<PathBuf> {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        manifest_dir.ancestors().find_map(|ancestor| {
            let root = ancestor.to_path_buf();
            let profiles = root.join("equalizerAPO").join("Perfiles");
            let installer = root.join("instalacion").join("install_echosuite.ps1");
            let vanysound = root.join("vanysound").join("src").join("analysis.rs");

            if profiles.is_dir() && installer.is_file() && vanysound.is_file() {
                Some(root)
            } else {
                None
            }
        })
    }

    fn config_mode_for_profile(active_profile: u32) -> String {
        if use_embedded_engine() {
            if active_profile == 0 {
                "embedded-cleared".to_string()
            } else {
                "embedded".to_string()
            }
        } else if active_profile == 0 {
            "cleared".to_string()
        } else {
            "materialized".to_string()
        }
    }
}

pub fn expected_helper_version() -> &'static str {
    embedded::expected_helper_version()
}

pub fn status() -> Result<ControlStatus> {
    let mut status = embedded::status_struct()?;
    enrich_control_status(&mut status);
    if let Ok(mut verify) = embedded::verify_struct() {
        enrich_verify_status(&mut verify);
        status.install_health = verify.install_health.clone();
        if status.last_confirmed_revision.is_none() {
            status.last_confirmed_revision = verify.last_confirmed_revision.clone();
        }
    }
    Ok(status)
}

pub fn verify_status() -> Result<VerifyStatus> {
    let mut verify = match embedded::verify_struct() {
        Ok(verify) => verify,
        Err(error) => {
            let status = embedded::status_struct().unwrap_or_default();
            VerifyStatus {
                active_profile: status.active_profile,
                active_revision: status.active_revision,
                bundle_sha256: status.bundle_sha256,
                config_mode: status.config_mode,
                device_selector_active: status.device_selector_active,
                helper_log_path: status.helper_log_path,
                helper_path: status.helper_path,
                helper_version: status.helper_version,
                install_health: status.install_health,
                last_confirmed_revision: status.last_confirmed_revision,
                plugin_path: status.plugin_path,
                plugin_present: status.plugin_present,
                spatial_channel_mask: status.spatial_channel_mask,
                spatial_mode: status.spatial_mode,
                target_endpoint_guid: status.target_endpoint_guid,
                target_endpoint_name: status.target_endpoint_name,
                verify_reason: Some(error.to_string()),
                verify_state: Some("failed".to_string()),
            }
        }
    };
    enrich_verify_status(&mut verify);
    Ok(verify)
}

pub fn switch_profile(profile_id: u32) -> Result<ControlStatus> {
    match embedded::switch_profile_internal(profile_id) {
        Ok(mut status) => {
            enrich_control_status(&mut status);
            Ok(status)
        }
        Err(first_error) => {
            let err_text = first_error.to_string().to_ascii_lowercase();
            let is_endpoint_issue = err_text.contains("hifiendpointguid")
                || err_text.contains("endpoint")
                || err_text.contains("device selector")
                || err_text.contains("not configured")
                || err_text.contains("not active")
                || err_text.contains("registration");

            if !is_endpoint_issue {
                return Err(first_error);
            }

            tracing::warn!(
                error = %first_error,
                "switch_profile failed with endpoint issue — auto-repairing device selector"
            );

            // Auto-repair: detect + register the HiFi Cable endpoint
            if let Err(repair_err) = repair_device_selector() {
                tracing::error!(error = %repair_err, "auto-repair device selector failed");
                return Err(first_error.context(format!("Auto-repair also failed: {repair_err}")));
            }

            tracing::info!("device selector repaired, retrying switch_profile");
            let mut status = embedded::switch_profile_internal(profile_id)?;
            enrich_control_status(&mut status);
            Ok(status)
        }
    }
}

pub fn clear_profile() -> Result<ControlStatus> {
    let mut status = embedded::clear_profile_internal()?;
    enrich_control_status(&mut status);
    Ok(status)
}

pub fn repair_device_selector() -> Result<ControlStatus> {
    let mut status = embedded::repair_device_selector_internal()?;
    enrich_control_status(&mut status);
    Ok(status)
}

pub fn bootstrap_runtime() -> Result<ControlStatus> {
    let mut status = embedded::bootstrap_runtime_internal()?;
    enrich_control_status(&mut status);
    refresh_audio_service_if_version_changed();
    Ok(status)
}

/// Restart Windows Audio Service once per app version upgrade.
///
/// After an NSIS update + reboot, EqualizerAPO's file watcher on config.txt
/// can desync (Hidden+System dir attributes confuse ReadDirectoryChangesW
/// during early boot). Restarting `audiosrv` forces EqualizerAPO to
/// reinitialize its APO chain and file monitoring.
fn refresh_audio_service_if_version_changed() {
    let current_version = env!("CARGO_PKG_VERSION");
    let registry_value_name = "LastAudioRefreshVersion";

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let stored_version: Option<String> = hkcu
        .open_subkey(REGISTRY_PATH)
        .ok()
        .and_then(|key| key.get_value::<String, _>(registry_value_name).ok());

    if stored_version.as_deref() == Some(current_version) {
        return;
    }

    tracing::info!(
        current_version,
        stored_version = stored_version.as_deref().unwrap_or("<none>"),
        "version change detected — restarting audio service for EqualizerAPO sync"
    );

    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let restart_result = Command::new("net")
        .args(["stop", "audiosrv"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .and_then(|_| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            Command::new("net")
                .args(["start", "audiosrv"])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
        });

    match restart_result {
        Ok(output) if output.status.success() => {
            tracing::info!("audio service restarted successfully");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!(%stderr, "audio service restart returned non-zero (non-fatal)");
        }
        Err(error) => {
            tracing::warn!(%error, "audio service restart failed (non-fatal)");
        }
    }

    // Persist version regardless of restart outcome to avoid retry loops
    if let Ok((key, _)) = hkcu.create_subkey(REGISTRY_PATH) {
        let _ = key.set_value(registry_value_name, &current_version);
    }
}

pub fn set_spatial_mode(mode: SpatialMode) -> Result<ControlStatus> {
    let endpoint_guid = resolve_or_repair_target_endpoint_guid()?;
    apply_spatial_mode_to_endpoint(&endpoint_guid, mode)?;

    let status = status()?;
    if status.spatial_mode.as_deref() != Some(mode.as_str())
        || status.spatial_channel_mask != Some(mode.channel_mask())
    {
        bail!(
            "La verificacion del modo espacial fallo. Esperado={} mask=0x{:04X}.",
            mode.as_str(),
            mode.channel_mask()
        );
    }

    Ok(status)
}

#[allow(dead_code)]
pub fn pack_and_deploy_workspace_profiles() -> Result<()> {
    embedded::pack_and_deploy_workspace_profiles()
}

#[allow(dead_code)]
pub fn workspace_profiles_root() -> Option<PathBuf> {
    embedded::resolve_workspace_profiles_root()
}

pub fn is_windows_supported() -> bool {
    cfg!(target_os = "windows")
}

/// Enumerate all active render endpoints via WASAPI COM API.
/// Returns proper Windows-native friendly names (e.g. "LG ULTRAGEAR (NVIDIA High Definition Audio)")
/// instead of the fragile registry PROPVARIANT parsing that loses endpoint-specific names.
pub fn list_audio_outputs() -> Result<Vec<crate::models::AudioDeviceDto>> {
    let raw_target_guid = crate::audio_output::read_output_endpoint_guid().unwrap_or_default();
    let current_target_guid = raw_target_guid.to_ascii_lowercase();

    tracing::debug!(
        registry_guid = %raw_target_guid,
        "list_audio_outputs: current OutputEndpointGuid from registry"
    );

    let devices = unsafe { enumerate_render_devices_com(&current_target_guid)? };
    Ok(devices)
}

/// COM-based device enumeration — uses IMMDeviceEnumerator + IPropertyStore
/// for reliable, Windows-native device names.
unsafe fn enumerate_render_devices_com(
    current_target_guid: &str,
) -> Result<Vec<crate::models::AudioDeviceDto>> {
    use windows::core::GUID;
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::*;
    use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

    let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
        .context("CoCreateInstance for IMMDeviceEnumerator")?;

    let collection = enumerator
        .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
        .context("EnumAudioEndpoints")?;

    let count = collection.GetCount().context("GetCount")?;

    // PKEY_Device_FriendlyName = {a45c254e-df1c-4efd-8020-67d146a850e0}, 14
    let pkey_friendly = PROPERTYKEY {
        fmtid: GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0),
        pid: 14,
    };

    let target_bare = current_target_guid
        .trim_start_matches('{')
        .trim_end_matches('}');

    let mut devices = Vec::with_capacity(count as usize);

    for i in 0..count {
        let device = match collection.Item(i) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Get device ID (GUID string)
        let dev_id_pwstr = match device.GetId() {
            Ok(id) => id,
            Err(_) => continue,
        };
        let dev_id = dev_id_pwstr.to_string().unwrap_or_default();

        // Get friendly name via IPropertyStore (exact same name Windows shows)
        let friendly_name = get_com_device_friendly_name(&device, &pkey_friendly)
            .unwrap_or_else(|| "Unknown Audio Device".to_string());

        // Check if this is the currently selected output device
        let device_bare = dev_id.to_ascii_lowercase();
        let device_bare = device_bare.trim_start_matches('{').trim_end_matches('}');

        let is_active = !target_bare.is_empty()
            && (device_bare == target_bare
                || device_bare.contains(target_bare)
                || target_bare.contains(device_bare));

        // Use the subkey-style GUID (just the {GUID} part) as the device ID
        // Extract GUID from the full endpoint ID (format: {0.0.0.00000000}.{GUID})
        let device_guid = extract_endpoint_guid(&dev_id).unwrap_or(dev_id.clone());

        devices.push(crate::models::AudioDeviceDto {
            id: device_guid,
            name: friendly_name,
            is_active,
            is_default: false,
        });
    }

    // Sort: active first, then alphabetical
    devices.sort_by(|a, b| b.is_active.cmp(&a.is_active).then(a.name.cmp(&b.name)));

    Ok(devices)
}

/// Get friendly name from IMMDevice via IPropertyStore (COM).
/// Returns the exact name Windows Sound Settings shows.
unsafe fn get_com_device_friendly_name(
    device: &windows::Win32::Media::Audio::IMMDevice,
    pkey: &windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY,
) -> Option<String> {
    let store = device
        .OpenPropertyStore(windows::Win32::System::Com::STGM(0))
        .ok()?;
    let val = store.GetValue(pkey).ok()?;
    let name = format!("{}", val);
    if name.is_empty() || name == "VT_EMPTY" {
        return None;
    }
    Some(name)
}

/// Extract the {GUID} portion from a full Windows endpoint ID.
/// Input:  "{0.0.0.00000000}.{AFEEB5A2-5E03-4D0F-BF4C-E66859CA7F3B}"
/// Output: "{AFEEB5A2-5E03-4D0F-BF4C-E66859CA7F3B}"
fn extract_endpoint_guid(endpoint_id: &str) -> Option<String> {
    // Find the last '{' and extract from there
    let last_brace = endpoint_id.rfind('{')?;
    let guid_part = &endpoint_id[last_brace..];
    if guid_part.contains('}') {
        Some(guid_part.to_string())
    } else {
        None
    }
}

/// Read the friendly name from a device's Properties subkey.
fn read_device_friendly_name(device_key: &RegKey) -> String {
    // PKEY_Device_FriendlyName = {a45c254e-df1c-4efd-8020-67d146a850e0},14
    const PKEY_FRIENDLY_NAME: &str = "{a45c254e-df1c-4efd-8020-67d146a850e0},14";
    // PKEY_DeviceInterface_FriendlyName = {b3f8fa53-0004-438e-9003-51a46e139bfc},6
    const PKEY_DEVICE_INTERFACE_NAME: &str = "{b3f8fa53-0004-438e-9003-51a46e139bfc},6";
    // PKEY_Device_DeviceDesc = {a45c254e-df1c-4efd-8020-67d146a850e0},2
    const PKEY_DEVICE_DESC: &str = "{a45c254e-df1c-4efd-8020-67d146a850e0},2";

    let props_key = match device_key.open_subkey("Properties") {
        Ok(key) => key,
        Err(_) => return "Unknown Audio Device".to_string(),
    };

    // Try properties in priority order
    for prop_name in [
        PKEY_FRIENDLY_NAME,
        PKEY_DEVICE_INTERFACE_NAME,
        PKEY_DEVICE_DESC,
    ] {
        if let Ok(raw) = props_key.get_raw_value(prop_name) {
            if let Some(name) = extract_best_string_from_propvar(&raw.bytes) {
                if name.len() >= 3 {
                    return name;
                }
            }
        }
    }

    "Unknown Audio Device".to_string()
}

/// Extract a string from a Windows PROPVARIANT-serialized registry blob.
///
/// Windows property store values in REG_BINARY use a variable-length header
/// before the UTF-16LE string data. The header size varies between 4, 8, and
/// 12 bytes depending on the VARTYPE and Windows version.
///
/// Strategy: try every even offset from 0..24 and return the LONGEST valid
/// UTF-16 string found. The real device name is always the longest candidate.
fn extract_best_string_from_propvar(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 4 {
        return None;
    }

    let max_offset = bytes.len().min(24);
    let mut best: Option<String> = None;

    for offset in (0..max_offset).step_by(2) {
        if offset + 2 > bytes.len() {
            break;
        }
        let slice = &bytes[offset..];
        let decoded: String = slice
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|&c| c != 0)
            .filter_map(|c| char::from_u32(c as u32))
            .collect();

        // Valid candidate: non-empty, all printable, at least 2 chars
        if decoded.len() >= 2
            && decoded.chars().all(|c| !c.is_control())
            && decoded.chars().any(|c| c.is_alphabetic())
        {
            let is_longer = best
                .as_ref()
                .map(|b| decoded.len() > b.len())
                .unwrap_or(true);
            if is_longer {
                best = Some(decoded);
            }
        }
    }

    best
}

/// Set the audio output device by persisting its GUID to the registry (HKCU).
/// No elevation needed since HKCU is writable by the current user.
pub fn set_audio_output(device_id: &str) -> Result<()> {
    crate::audio_output::write_output_endpoint_guid(device_id)
}

fn try_set_audio_output(device_id: &str) -> Result<()> {
    // IMPORTANT: load_registry_string_value reads HKCU first, so we MUST
    // write to HKCU for the change to be visible immediately.
    let hkcu = open_hkcu();

    // Read the current value first for diagnostics
    let old_value = load_registry_string_value(HIFI_ENDPOINT_GUID_VALUE_NAME)
        .unwrap_or_else(|| "<empty>".to_string());
    tracing::info!(
        old_value = %old_value,
        new_value = %device_id,
        "Attempting to write HiFiEndpointGuid to HKCU"
    );

    let key = hkcu
        .open_subkey_with_flags(REGISTRY_PATH, KEY_READ | KEY_WRITE)
        .or_else(|_| hkcu.create_subkey(REGISTRY_PATH).map(|(key, _)| key))
        .context("Cannot open/create HKCU\\SOFTWARE\\VanySound for writing")?;

    key.set_value(HIFI_ENDPOINT_GUID_VALUE_NAME, &device_id)
        .context("Failed to write HiFiEndpointGuid to HKCU")?;

    // Verify the write actually persisted
    let readback: String = key
        .get_value(HIFI_ENDPOINT_GUID_VALUE_NAME)
        .unwrap_or_else(|_| "<read-failed>".to_string());

    if readback.to_ascii_lowercase() != device_id.to_ascii_lowercase() {
        bail!(
            "Registry write verification failed: wrote '{}' but read back '{}' (old was '{}')",
            device_id,
            readback,
            old_value
        );
    }

    tracing::info!(
        device_id,
        readback = %readback,
        "Audio output device verified in HKCU registry"
    );
    Ok(())
}

pub fn export_diagnostics_bundle() -> Result<PathBuf> {
    let export_root = logging::log_root().join("exports");
    std::fs::create_dir_all(&export_root)?;

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let export_dir = export_root.join(format!("vanysound-diagnostics-{stamp}"));
    std::fs::create_dir_all(&export_dir)?;

    let status = status().ok();
    let verify = verify_status().ok();
    let debug_report = logging::debug_report();
    let log_snapshot = logging::log_snapshot();

    std::fs::write(export_dir.join("debug-report.txt"), debug_report)?;
    std::fs::write(
        export_dir.join("status.txt"),
        status
            .as_ref()
            .map(render_status_report)
            .unwrap_or_else(|| "STATUS_UNAVAILABLE=true\n".to_string()),
    )?;
    std::fs::write(
        export_dir.join("verify.txt"),
        verify
            .as_ref()
            .map(render_verify_report)
            .unwrap_or_else(|| "VERIFY_UNAVAILABLE=true\n".to_string()),
    )?;
    std::fs::write(
        export_dir.join("log-snapshot.txt"),
        render_log_snapshot_report(&log_snapshot),
    )?;
    std::fs::write(
        export_dir.join("system.txt"),
        render_system_report(status.as_ref(), verify.as_ref()),
    )?;

    for file in &log_snapshot.files {
        let source = PathBuf::from(&file.path);
        if source.is_file() {
            let target = export_dir.join(
                source
                    .file_name()
                    .map(|value| value.to_owned())
                    .unwrap_or_else(|| "unknown.log".into()),
            );
            let _ = std::fs::copy(source, target);
        }
    }

    let archive_path = export_root.join(format!("vanysound-diagnostics-{stamp}.zip"));
    let _ = std::fs::remove_file(&archive_path);
    let export_glob = export_dir.join("*");
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW_FLAG: u32 = 0x0800_0000;
    let compress_status = Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(format!(
            "Compress-Archive -Path '{}' -DestinationPath '{}' -Force",
            export_glob.display(),
            archive_path.display()
        ))
        .creation_flags(CREATE_NO_WINDOW_FLAG)
        .status();

    if matches!(compress_status, Ok(status) if status.success()) && archive_path.is_file() {
        return Ok(archive_path);
    }

    Ok(export_dir)
}

fn open_hklm() -> RegKey {
    RegKey::predef(HKEY_LOCAL_MACHINE)
}

fn open_hkcu() -> RegKey {
    RegKey::predef(HKEY_CURRENT_USER)
}

fn load_registry_string_value(name: &str) -> Option<String> {
    let hkcu = open_hkcu();
    if let Ok(key) = hkcu.open_subkey(REGISTRY_PATH) {
        if let Ok(value) = key.get_value::<String, _>(name) {
            return Some(value);
        }
    }

    let hklm = open_hklm();
    let key = hklm.open_subkey(REGISTRY_PATH).ok()?;
    key.get_value::<String, _>(name).ok()
}

fn load_registry_string_from_path(path: &str, name: &str) -> Option<String> {
    let hklm = open_hklm();
    let key = hklm.open_subkey(path).ok()?;
    key.get_value::<String, _>(name).ok()
}

fn normalize_optional_registry_string(value: Option<String>) -> Option<String> {
    value.and_then(|current| {
        let trimmed = current.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn resolve_plugin_path(engine_dir: Option<&PathBuf>) -> Option<PathBuf> {
    engine_dir.map(|root| root.join("VSTPlugins").join(MJUCJR_DLL_NAME))
}

fn resolve_packaged_resource_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            roots.push(exe_dir.to_path_buf());
            roots.push(exe_dir.join("resources"));
            roots.push(exe_dir.join("_up_"));
        }
    }

    roots
}

fn resolve_packaged_plugin_path() -> Option<PathBuf> {
    for root in resolve_packaged_resource_roots() {
        for candidate in [
            root.join("assets").join("VSTPlugins").join(MJUCJR_DLL_NAME),
            root.join("setup-prueba-otra-pc")
                .join("VanySound")
                .join("assets")
                .join("VSTPlugins")
                .join(MJUCJR_DLL_NAME),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

fn ensure_plugin_installed(engine_dir: Option<&PathBuf>) -> Result<()> {
    let Some(destination) = resolve_plugin_path(engine_dir) else {
        return Ok(());
    };
    let Some(source) = resolve_packaged_plugin_path() else {
        return Ok(());
    };

    let destination_matches = if destination.is_file() {
        std::fs::read(&source).ok() == std::fs::read(&destination).ok()
    } else {
        false
    };

    if destination_matches {
        return Ok(());
    }

    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::copy(&source, &destination).with_context(|| {
        format!(
            "No se pudo copiar {} a {}",
            source.display(),
            destination.display()
        )
    })?;

    Ok(())
}

fn resolve_engine_dir_from_registry() -> Option<PathBuf> {
    if let Some(engine_path) =
        load_registry_string_value("EnginePath").filter(|value| !value.trim().is_empty())
    {
        return Some(PathBuf::from(engine_path));
    }

    load_registry_string_value("ConfigDir")
        .filter(|value| !value.trim().is_empty())
        .and_then(|value| PathBuf::from(value).parent().map(Path::to_path_buf))
}

fn derive_install_health(
    helper_version: Option<&str>,
    bundle_present: Option<bool>,
    plugin_present: Option<bool>,
    target_endpoint_guid: Option<&str>,
    active_profile: Option<u32>,
    active_revision: Option<&str>,
    last_confirmed_revision: Option<&str>,
    verify_state: Option<&str>,
    verify_reason: Option<&str>,
) -> String {
    if !helper_version_is_supported(helper_version, verify_state) {
        return "VersionMismatch".to_string();
    }

    if bundle_present != Some(true) {
        return "NeedsInstall".to_string();
    }

    if plugin_present == Some(false) {
        return "PluginMissing".to_string();
    }

    if target_endpoint_guid
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        return "EndpointMissing".to_string();
    }

    if active_profile.unwrap_or(0) > 0
        && active_revision.is_some()
        && last_confirmed_revision.is_some()
        && active_revision != last_confirmed_revision
    {
        return "ApplyFailed".to_string();
    }

    if matches!(verify_state, Some("matched") | Some("cleared") | Some("ok")) {
        return "Healthy".to_string();
    }

    if verify_reason
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
    {
        return "NeedsRepair".to_string();
    }

    if active_profile.unwrap_or(0) > 0 {
        return "ApplyFailed".to_string();
    }

    "NeedsRepair".to_string()
}

fn helper_version_is_supported(helper_version: Option<&str>, verify_state: Option<&str>) -> bool {
    let Some(helper_version) = helper_version
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };

    if helper_version == expected_helper_version() {
        return true;
    }

    if !helper_version.starts_with("control-plane-v") {
        return false;
    }

    let legacy_version = helper_version
        .trim_start_matches("control-plane-v")
        .parse::<u32>()
        .ok();

    match legacy_version {
        Some(version) if version >= 2 => {
            matches!(verify_state, Some("matched") | Some("cleared") | Some("ok"))
        }
        _ => false,
    }
}

fn open_endpoint_properties_key(endpoint_guid: &str, writable: bool) -> Result<RegKey> {
    let hklm = open_hklm();
    let path = format!(
        r"{}\{}\Properties",
        RENDER_DEVICES_REGISTRY_PATH, endpoint_guid
    );
    if writable {
        Ok(hklm.open_subkey_with_flags(path, KEY_READ | KEY_WRITE)?)
    } else {
        Ok(hklm.open_subkey(path)?)
    }
}

fn read_registry_dword(key: &RegKey, name: &str) -> Option<u32> {
    let raw = key.get_raw_value(name).ok()?;
    if raw.vtype != REG_DWORD || raw.bytes.len() < 4 {
        return None;
    }

    Some(u32::from_le_bytes(raw.bytes[0..4].try_into().ok()?))
}

fn read_device_format_channel_mask(value: &RegValue) -> Option<u32> {
    if value.vtype != REG_BINARY {
        return None;
    }

    let bytes = &value.bytes;
    if bytes.len() < CHANNEL_MASK_OFFSET + 4 {
        return None;
    }

    Some(u32::from_le_bytes(
        bytes[CHANNEL_MASK_OFFSET..CHANNEL_MASK_OFFSET + 4]
            .try_into()
            .ok()?,
    ))
}

fn read_spatial_state(endpoint_guid: &str) -> Result<(Option<SpatialMode>, Option<u32>)> {
    let properties = open_endpoint_properties_key(endpoint_guid, false)?;
    let device_format_mask = properties
        .get_raw_value(PKEY_AUDIOENGINE_DEVICE_FORMAT)
        .ok()
        .and_then(|value| read_device_format_channel_mask(&value));
    let physical_mask = read_registry_dword(&properties, PKEY_AUDIOENDPOINT_PHYSICAL_SPEAKERS);
    let full_range_mask = read_registry_dword(&properties, PKEY_AUDIOENDPOINT_FULL_RANGE_SPEAKERS);
    let channel_mask = device_format_mask.or(physical_mask).or(full_range_mask);
    Ok((
        channel_mask.and_then(SpatialMode::from_channel_mask),
        channel_mask,
    ))
}

fn enrich_control_status(status: &mut ControlStatus) {
    status.active_revision = normalize_optional_registry_string(status.active_revision.take());
    status.last_confirmed_revision =
        normalize_optional_registry_string(status.last_confirmed_revision.take());
    status.bundle_sha256 = normalize_optional_registry_string(status.bundle_sha256.take());

    let plugin_path = resolve_plugin_path(status.engine_dir.as_ref());
    let plugin_present = plugin_path.as_ref().map(|path| path.is_file());
    status.plugin_path = plugin_path;
    status.plugin_present = plugin_present;

    if let Some(endpoint_guid) = status.target_endpoint_guid.as_deref() {
        if let Ok((mode, channel_mask)) = read_spatial_state(endpoint_guid) {
            status.spatial_mode = mode.map(|current| current.as_str().to_string());
            status.spatial_channel_mask = channel_mask;
        }
    } else {
        status.spatial_mode = None;
        status.spatial_channel_mask = None;
    }

    status.install_health = Some(derive_install_health(
        status.helper_version.as_deref(),
        status.bundle_present,
        status.plugin_present,
        status.target_endpoint_guid.as_deref(),
        status.active_profile,
        status.active_revision.as_deref(),
        status.last_confirmed_revision.as_deref(),
        None,
        None,
    ));
}

fn enrich_verify_status(verify: &mut VerifyStatus) {
    verify.active_revision = normalize_optional_registry_string(verify.active_revision.take());
    verify.last_confirmed_revision =
        normalize_optional_registry_string(verify.last_confirmed_revision.take());
    verify.bundle_sha256 = normalize_optional_registry_string(verify.bundle_sha256.take());
    verify.verify_reason = normalize_optional_registry_string(verify.verify_reason.take());

    if matches!(
        verify.verify_state.as_deref(),
        Some("matched") | Some("cleared") | Some("ok")
    ) && verify.last_confirmed_revision.is_none()
    {
        verify.last_confirmed_revision = verify.active_revision.clone();
    }

    let plugin_path = resolve_plugin_path(resolve_engine_dir_from_registry().as_ref());
    let plugin_present = plugin_path.as_ref().map(|path| path.is_file());
    verify.plugin_path = plugin_path;
    verify.plugin_present = plugin_present;

    if let Some(endpoint_guid) = verify.target_endpoint_guid.as_deref() {
        if let Ok((mode, channel_mask)) = read_spatial_state(endpoint_guid) {
            verify.spatial_mode = mode.map(|current| current.as_str().to_string());
            verify.spatial_channel_mask = channel_mask;
        }
    } else {
        verify.spatial_mode = None;
        verify.spatial_channel_mask = None;
    }

    verify.install_health = Some(derive_install_health(
        verify.helper_version.as_deref(),
        Some(verify.bundle_sha256.is_some()),
        verify.plugin_present,
        verify.target_endpoint_guid.as_deref(),
        verify.active_profile,
        verify.active_revision.as_deref(),
        verify.last_confirmed_revision.as_deref(),
        verify.verify_state.as_deref(),
        verify.verify_reason.as_deref(),
    ));
}

fn resolve_or_repair_target_endpoint_guid() -> Result<String> {
    if let Some(endpoint_guid) = load_registry_string_value(HIFI_ENDPOINT_GUID_VALUE_NAME)
        .filter(|value| !value.trim().is_empty())
    {
        if open_endpoint_properties_key(&endpoint_guid, false).is_ok() {
            return Ok(endpoint_guid);
        }
    }

    let repaired = repair_device_selector()?;
    repaired
        .target_endpoint_guid
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("HiFiEndpointGuid no quedo configurado despues de reparar."))
}

fn rewrite_device_format_value(value: &RegValue, mode: SpatialMode) -> Result<RegValue> {
    if value.vtype != REG_BINARY {
        bail!("PKEY_AudioEngine_DeviceFormat no es REG_BINARY.");
    }

    let mut bytes = value.bytes.clone();
    if bytes.len() < SERIALIZED_WAVEFORMAT_OFFSET + WAVEFORMATEX_SIZE {
        bail!("PKEY_AudioEngine_DeviceFormat es demasiado corto.");
    }

    let vt = u32::from_le_bytes(
        bytes[0..4]
            .try_into()
            .map_err(|_| anyhow!("No se pudo leer el encabezado del formato."))?,
    );
    if vt != PROPERTY_STORE_VT_BLOB {
        bail!("PKEY_AudioEngine_DeviceFormat no contiene un VT_BLOB valido.");
    }

    let format_tag = u16::from_le_bytes(
        bytes[SERIALIZED_WAVEFORMAT_OFFSET..SERIALIZED_WAVEFORMAT_OFFSET + 2]
            .try_into()
            .unwrap(),
    );
    if format_tag != WAVEFORMAT_EXTENSIBLE {
        bail!("El formato del endpoint Hi-Fi no es WAVE_FORMAT_EXTENSIBLE.");
    }

    let sample_rate = u32::from_le_bytes(
        bytes[SERIALIZED_WAVEFORMAT_OFFSET + 4..SERIALIZED_WAVEFORMAT_OFFSET + 8]
            .try_into()
            .unwrap(),
    );
    let bits_per_sample = u16::from_le_bytes(
        bytes[SERIALIZED_WAVEFORMAT_OFFSET + 14..SERIALIZED_WAVEFORMAT_OFFSET + 16]
            .try_into()
            .unwrap(),
    );
    let cb_size = u16::from_le_bytes(
        bytes[SERIALIZED_WAVEFORMAT_OFFSET + 16..SERIALIZED_WAVEFORMAT_OFFSET + 18]
            .try_into()
            .unwrap(),
    );
    if cb_size < WAVEFORMATEXTENSIBLE_CB_SIZE || bytes.len() < CHANNEL_MASK_OFFSET + 4 {
        bail!("El blob del formato del endpoint no tiene espacio para dwChannelMask.");
    }

    let channels = mode.channel_count();
    let bytes_per_sample = u32::from(bits_per_sample).div_ceil(8);
    let block_align = u16::try_from(u32::from(channels) * bytes_per_sample)
        .map_err(|_| anyhow!("No se pudo calcular el block align del endpoint."))?;
    let avg_bytes_per_sec = sample_rate
        .checked_mul(u32::from(block_align))
        .ok_or_else(|| anyhow!("No se pudo calcular el avg bytes/sec del endpoint."))?;

    bytes[SERIALIZED_WAVEFORMAT_OFFSET + 2..SERIALIZED_WAVEFORMAT_OFFSET + 4]
        .copy_from_slice(&channels.to_le_bytes());
    bytes[SERIALIZED_WAVEFORMAT_OFFSET + 8..SERIALIZED_WAVEFORMAT_OFFSET + 12]
        .copy_from_slice(&avg_bytes_per_sec.to_le_bytes());
    bytes[SERIALIZED_WAVEFORMAT_OFFSET + 12..SERIALIZED_WAVEFORMAT_OFFSET + 14]
        .copy_from_slice(&block_align.to_le_bytes());
    bytes[CHANNEL_MASK_OFFSET..CHANNEL_MASK_OFFSET + 4]
        .copy_from_slice(&mode.channel_mask().to_le_bytes());

    Ok(RegValue {
        bytes,
        vtype: REG_BINARY,
    })
}

fn apply_spatial_mode_to_endpoint(endpoint_guid: &str, mode: SpatialMode) -> Result<()> {
    let properties = open_endpoint_properties_key(endpoint_guid, true)?;
    let current_device_format = properties
        .get_raw_value(PKEY_AUDIOENGINE_DEVICE_FORMAT)
        .with_context(|| {
            format!(
                "No se encontro PKEY_AudioEngine_DeviceFormat para el endpoint {}.",
                endpoint_guid
            )
        })?;
    let updated_device_format = rewrite_device_format_value(&current_device_format, mode)?;
    let channel_mask = mode.channel_mask();

    properties.set_raw_value(PKEY_AUDIOENGINE_DEVICE_FORMAT, &updated_device_format)?;
    properties.set_value(PKEY_AUDIOENDPOINT_PHYSICAL_SPEAKERS, &channel_mask)?;
    properties.set_value(PKEY_AUDIOENDPOINT_FULL_RANGE_SPEAKERS, &channel_mask)?;

    let (actual_mode, actual_mask) = read_spatial_state(endpoint_guid)?;
    if actual_mode != Some(mode) || actual_mask != Some(channel_mask) {
        bail!(
            "No se pudo verificar el modo espacial {} en el endpoint {}.",
            mode.as_str(),
            endpoint_guid
        );
    }
    Ok(())
}

pub fn command_from_cli(args: &[String]) -> Result<i32> {
    match args.first().map(String::as_str) {
        Some("status") => {
            let current = status()?;
            print_status_like_helper(&current);
            Ok(0)
        }
        Some("verify") => {
            let verify = verify_status()?;
            print_verify_like_helper(&verify);
            Ok(0)
        }
        Some("bootstrap-runtime") => {
            let current = bootstrap_runtime()?;
            print_status_like_helper(&current);
            Ok(0)
        }
        Some("pack") => {
            let profiles_dir = Path::new(
                args.get(1)
                    .context("pack requiere el directorio de perfiles.")?,
            );
            let output_file = Path::new(args.get(2).context("pack requiere la ruta de salida.")?);
            embedded::pack_bundle_internal(profiles_dir, output_file)?;
            println!("RESULT=OK");
            println!("BUNDLE_PATH={}", output_file.display());
            Ok(0)
        }
        Some("deploy") => {
            let bundle_file =
                Path::new(args.get(1).context("deploy requiere la ruta del bundle.")?);
            embedded::deploy_bundle_internal(bundle_file)?;
            let current = status()?;
            print_status_like_helper(&current);
            Ok(0)
        }
        Some("repair-device-selector") => {
            let current = repair_device_selector()?;
            print_status_like_helper(&current);
            Ok(0)
        }
        Some("clear") => {
            let current = clear_profile()?;
            print_status_like_helper(&current);
            Ok(0)
        }
        Some("switch") => {
            let profile_id: u32 = args
                .get(1)
                .context("switch requiere un profile id.")?
                .parse()
                .context("Profile id invalido.")?;
            let current = switch_profile(profile_id)?;
            print_status_like_helper(&current);
            Ok(0)
        }
        Some("set-spatial-mode") => {
            let mode = SpatialMode::from_input(
                args.get(1)
                    .context("set-spatial-mode requiere un modo espacial.")?,
            )?;
            let current = set_spatial_mode(mode)?;
            print_status_like_helper(&current);
            Ok(0)
        }
        Some("export-diagnostics") => {
            let archive_path = export_diagnostics_bundle()?;
            println!("RESULT=OK");
            println!("ARCHIVE_PATH={}", archive_path.display());
            Ok(0)
        }
        Some("set-audio-output") => {
            let device_id = args
                .get(1)
                .context("set-audio-output requires a device GUID.")?;
            try_set_audio_output(device_id)?;
            println!("RESULT=OK");
            Ok(0)
        }
        Some(other) => Err(anyhow!("Comando CLI desconocido: {other}")),
        None => Err(anyhow!("No se recibio comando CLI.")),
    }
}

fn print_status_like_helper(status: &ControlStatus) {
    println!("RESULT=OK");
    println!(
        "ACTIVE_PROFILE={}",
        status
            .active_profile
            .map(|value| value.to_string())
            .unwrap_or_else(|| "0".to_string())
    );
    println!(
        "CONFIG_DIR={}",
        status
            .config_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    );
    println!(
        "ENGINE_DIR={}",
        status
            .engine_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    );
    println!(
        "BUNDLE_PRESENT={}",
        status
            .bundle_present
            .unwrap_or(false)
            .to_string()
            .to_ascii_lowercase()
    );
    println!(
        "BUNDLE_SHA256={}",
        status.bundle_sha256.as_deref().unwrap_or_default()
    );
    println!(
        "CONFIG_MODE={}",
        status.config_mode.as_deref().unwrap_or_default()
    );
    println!(
        "ACTIVE_REVISION={}",
        status.active_revision.as_deref().unwrap_or_default()
    );
    println!(
        "LAST_CONFIRMED_REVISION={}",
        status
            .last_confirmed_revision
            .as_deref()
            .unwrap_or_default()
    );
    println!(
        "DEVICE_SELECTOR_ACTIVE={}",
        status
            .device_selector_active
            .unwrap_or(false)
            .to_string()
            .to_ascii_lowercase()
    );
    println!(
        "HELPER_VERSION={}",
        status.helper_version.as_deref().unwrap_or_default()
    );
    println!(
        "INSTALL_HEALTH={}",
        status.install_health.as_deref().unwrap_or_default()
    );
    println!(
        "PLUGIN_PRESENT={}",
        status
            .plugin_present
            .unwrap_or(false)
            .to_string()
            .to_ascii_lowercase()
    );
    println!(
        "PLUGIN_PATH={}",
        status
            .plugin_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    );
    println!(
        "SPATIAL_MODE={}",
        status.spatial_mode.as_deref().unwrap_or_default()
    );
    println!(
        "SPATIAL_CHANNEL_MASK={}",
        status
            .spatial_channel_mask
            .map(|value| value.to_string())
            .unwrap_or_default()
    );
    println!(
        "HELPER_PATH={}",
        status
            .helper_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    );
    println!(
        "HELPER_LOG_PATH={}",
        status
            .helper_log_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    );
    println!(
        "TARGET_ENDPOINT_GUID={}",
        status.target_endpoint_guid.as_deref().unwrap_or_default()
    );
    println!(
        "TARGET_ENDPOINT_NAME={}",
        status.target_endpoint_name.as_deref().unwrap_or_default()
    );
}

fn print_verify_like_helper(verify: &VerifyStatus) {
    println!("RESULT=OK");
    println!(
        "ACTIVE_PROFILE={}",
        verify
            .active_profile
            .map(|value| value.to_string())
            .unwrap_or_else(|| "0".to_string())
    );
    println!(
        "VERIFY={}",
        verify.verify_state.as_deref().unwrap_or_default()
    );
    println!(
        "VERIFY_REASON={}",
        verify.verify_reason.as_deref().unwrap_or_default()
    );
    println!(
        "CONFIG_MODE={}",
        verify.config_mode.as_deref().unwrap_or_default()
    );
    println!(
        "ACTIVE_REVISION={}",
        verify.active_revision.as_deref().unwrap_or_default()
    );
    println!(
        "LAST_CONFIRMED_REVISION={}",
        verify
            .last_confirmed_revision
            .as_deref()
            .unwrap_or_default()
    );
    println!(
        "BUNDLE_SHA256={}",
        verify.bundle_sha256.as_deref().unwrap_or_default()
    );
    println!(
        "DEVICE_SELECTOR_ACTIVE={}",
        verify
            .device_selector_active
            .unwrap_or(false)
            .to_string()
            .to_ascii_lowercase()
    );
    println!(
        "HELPER_VERSION={}",
        verify.helper_version.as_deref().unwrap_or_default()
    );
    println!(
        "INSTALL_HEALTH={}",
        verify.install_health.as_deref().unwrap_or_default()
    );
    println!(
        "PLUGIN_PRESENT={}",
        verify
            .plugin_present
            .unwrap_or(false)
            .to_string()
            .to_ascii_lowercase()
    );
    println!(
        "PLUGIN_PATH={}",
        verify
            .plugin_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    );
    println!(
        "SPATIAL_MODE={}",
        verify.spatial_mode.as_deref().unwrap_or_default()
    );
    println!(
        "SPATIAL_CHANNEL_MASK={}",
        verify
            .spatial_channel_mask
            .map(|value| value.to_string())
            .unwrap_or_default()
    );
    println!(
        "HELPER_PATH={}",
        verify
            .helper_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    );
    println!(
        "HELPER_LOG_PATH={}",
        verify
            .helper_log_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    );
    println!(
        "TARGET_ENDPOINT_GUID={}",
        verify.target_endpoint_guid.as_deref().unwrap_or_default()
    );
    println!(
        "TARGET_ENDPOINT_NAME={}",
        verify.target_endpoint_name.as_deref().unwrap_or_default()
    );
}

fn render_status_report(status: &ControlStatus) -> String {
    [
        format!(
            "ACTIVE_PROFILE={}",
            status
                .active_profile
                .map(|value| value.to_string())
                .unwrap_or_else(|| "0".to_string())
        ),
        format!(
            "ACTIVE_REVISION={}",
            status.active_revision.as_deref().unwrap_or_default()
        ),
        format!(
            "LAST_CONFIRMED_REVISION={}",
            status
                .last_confirmed_revision
                .as_deref()
                .unwrap_or_default()
        ),
        format!(
            "BUNDLE_PRESENT={}",
            status
                .bundle_present
                .unwrap_or(false)
                .to_string()
                .to_ascii_lowercase()
        ),
        format!(
            "BUNDLE_SHA256={}",
            status.bundle_sha256.as_deref().unwrap_or_default()
        ),
        format!(
            "CONFIG_MODE={}",
            status.config_mode.as_deref().unwrap_or_default()
        ),
        format!(
            "CONFIG_DIR={}",
            status
                .config_dir
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default()
        ),
        format!(
            "ENGINE_DIR={}",
            status
                .engine_dir
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default()
        ),
        format!(
            "HELPER_VERSION={}",
            status.helper_version.as_deref().unwrap_or_default()
        ),
        format!(
            "INSTALL_HEALTH={}",
            status.install_health.as_deref().unwrap_or_default()
        ),
        format!(
            "TARGET_ENDPOINT_GUID={}",
            status.target_endpoint_guid.as_deref().unwrap_or_default()
        ),
        format!(
            "TARGET_ENDPOINT_NAME={}",
            status.target_endpoint_name.as_deref().unwrap_or_default()
        ),
        format!(
            "PLUGIN_PRESENT={}",
            status
                .plugin_present
                .unwrap_or(false)
                .to_string()
                .to_ascii_lowercase()
        ),
        format!(
            "PLUGIN_PATH={}",
            status
                .plugin_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default()
        ),
    ]
    .join("\n")
}

fn render_verify_report(verify: &VerifyStatus) -> String {
    [
        format!(
            "ACTIVE_PROFILE={}",
            verify
                .active_profile
                .map(|value| value.to_string())
                .unwrap_or_else(|| "0".to_string())
        ),
        format!(
            "VERIFY={}",
            verify.verify_state.as_deref().unwrap_or_default()
        ),
        format!(
            "VERIFY_REASON={}",
            verify.verify_reason.as_deref().unwrap_or_default()
        ),
        format!(
            "ACTIVE_REVISION={}",
            verify.active_revision.as_deref().unwrap_or_default()
        ),
        format!(
            "LAST_CONFIRMED_REVISION={}",
            verify
                .last_confirmed_revision
                .as_deref()
                .unwrap_or_default()
        ),
        format!(
            "BUNDLE_SHA256={}",
            verify.bundle_sha256.as_deref().unwrap_or_default()
        ),
        format!(
            "INSTALL_HEALTH={}",
            verify.install_health.as_deref().unwrap_or_default()
        ),
        format!(
            "TARGET_ENDPOINT_GUID={}",
            verify.target_endpoint_guid.as_deref().unwrap_or_default()
        ),
        format!(
            "TARGET_ENDPOINT_NAME={}",
            verify.target_endpoint_name.as_deref().unwrap_or_default()
        ),
        format!(
            "PLUGIN_PRESENT={}",
            verify
                .plugin_present
                .unwrap_or(false)
                .to_string()
                .to_ascii_lowercase()
        ),
        format!(
            "PLUGIN_PATH={}",
            verify
                .plugin_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default()
        ),
    ]
    .join("\n")
}

fn render_log_snapshot_report(snapshot: &crate::models::LogSnapshotDto) -> String {
    let mut text = String::new();
    text.push_str(&format!("LOG_DIR={}\n", snapshot.log_dir));
    text.push_str("COMBINED_TAIL_BEGIN\n");
    for line in &snapshot.combined_tail {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str("COMBINED_TAIL_END\n");
    for file in &snapshot.files {
        text.push_str(&format!("\nFILE={} PATH={}\n", file.key, file.path));
        for line in &file.tail {
            text.push_str(line);
            text.push('\n');
        }
    }
    text
}

fn render_system_report(status: Option<&ControlStatus>, verify: Option<&VerifyStatus>) -> String {
    let windows_version = {
        use std::os::windows::process::CommandExt;
        Command::new("cmd")
            .args(["/c", "ver"])
            .creation_flags(0x0800_0000)
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .unwrap_or_else(|| "unknown".to_string())
            .trim()
            .to_string()
    };
    let apo_version = load_registry_string_from_path(r"SOFTWARE\EqualizerAPO", "Version")
        .or_else(|| load_registry_string_from_path(r"SOFTWARE\EqualizerAPO", "InstallPath"))
        .unwrap_or_else(|| "unknown".to_string());

    [
        format!("WINDOWS_VERSION={windows_version}"),
        format!("APO_VERSION={apo_version}"),
        format!(
            "HELPER_VERSION={}",
            status
                .and_then(|current| current.helper_version.as_deref())
                .or_else(|| verify.and_then(|current| current.helper_version.as_deref()))
                .unwrap_or_default()
        ),
        format!(
            "BUNDLE_SHA256={}",
            status
                .and_then(|current| current.bundle_sha256.as_deref())
                .or_else(|| verify.and_then(|current| current.bundle_sha256.as_deref()))
                .unwrap_or_default()
        ),
        format!(
            "TARGET_ENDPOINT_GUID={}",
            status
                .and_then(|current| current.target_endpoint_guid.as_deref())
                .or_else(|| verify.and_then(|current| current.target_endpoint_guid.as_deref()))
                .unwrap_or_default()
        ),
        format!(
            "TARGET_ENDPOINT_NAME={}",
            status
                .and_then(|current| current.target_endpoint_name.as_deref())
                .or_else(|| verify.and_then(|current| current.target_endpoint_name.as_deref()))
                .unwrap_or_default()
        ),
    ]
    .join("\n")
}

/// Write DSP engine config to a SEPARATE file (dsp_engine.txt) and ensure
/// config.txt includes it behind a top-level Device: selector so ONLY the
/// cable endpoint evaluates the include.
///
/// Self-healing: every call re-checks and re-patches config.txt if the
/// Include was lost (e.g., after clearProfile/switchProfile overwrites it).
pub fn apply_raw_config(config_text: &str) -> anyhow::Result<()> {
    let config_dir = resolve_eqapo_config_dir();
    let dsp_file = config_dir.join("dsp_engine.txt");
    let config_txt_path = config_dir.join("config.txt");

    tracing::info!(
        dsp_file = %dsp_file.display(),
        config_len = config_text.len(),
        "apply_raw_config: writing DSP engine config"
    );

    unlock_config_dir(&config_dir);

    // ── Step 1: Write dsp_engine.txt WITH Device: selector inside ──
    let device_name = resolve_target_cable_name().unwrap_or_else(|| "VanySound".to_string());
    let scoped_config = format!("Device: {}\r\n{}\r\n", device_name, config_text);

    let utf8_bom: [u8; 3] = [0xEF, 0xBB, 0xBF];
    let mut dsp_bytes = Vec::with_capacity(utf8_bom.len() + scoped_config.len());
    dsp_bytes.extend_from_slice(&utf8_bom);
    dsp_bytes.extend_from_slice(scoped_config.as_bytes());

    let mut last_error = None;
    for attempt in 0..5 {
        match std::fs::write(&dsp_file, &dsp_bytes) {
            Ok(()) => {
                tracing::info!(attempt, device = %device_name, "dsp_engine.txt written (cable-only)");
                last_error = None;
                break;
            }
            Err(e) => {
                tracing::warn!(attempt, error = %e, "dsp_engine.txt write failed");
                last_error = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
    if let Some(e) = last_error {
        return Err(anyhow::anyhow!("Failed to write dsp_engine.txt: {}", e));
    }

    // ── Step 2: Self-healing Include injection into config.txt ──
    write_scoped_dsp_main_config(&config_txt_path, &utf8_bom, &device_name)?;

    Ok(())
}

/// Rewrite config.txt so the Include itself is device-scoped. A Device:
/// directive inside an included file is not enough on every APO path because
/// the Include command may still be evaluated globally.
fn write_scoped_dsp_main_config(
    config_txt_path: &Path,
    bom: &[u8; 3],
    device_name: &str,
) -> anyhow::Result<()> {
    let main_config = format!(
        "# Windows Audio Subsystem - Driver Configuration\r\n\
         # VanySound DSP engine active\r\n\
         # Cable-only route\r\n\
         Device: {}\r\n\
         Include: dsp_engine.txt\r\n",
        device_name
    );

    let mut new_content = Vec::new();
    new_content.extend_from_slice(bom);
    new_content.extend_from_slice(main_config.as_bytes());

    for attempt in 0..5 {
        match std::fs::write(config_txt_path, &new_content) {
            Ok(()) => {
                tracing::info!(
                    attempt,
                    device = %device_name,
                    "write_scoped_dsp_main_config: config.txt rewritten for cable-only DSP"
                );
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(attempt, error = %e, "write_scoped_dsp_main_config: write failed");
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }

    Err(anyhow::anyhow!("Failed to write scoped DSP config.txt"))
}

/// Remove hidden/system attributes and grant write access to the config directory.
fn unlock_config_dir(config_dir: &Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    // Remove hidden + system attributes from directory
    let _ = Command::new("attrib")
        .args(["-H", "-S"])
        .arg(config_dir.as_os_str())
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    // Remove hidden + system attributes from config.txt
    let config_path = config_dir.join("config.txt");
    if config_path.exists() {
        let _ = Command::new("attrib")
            .args(["-H", "-S", "-R"])
            .arg(config_path.as_os_str())
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }

    // Grant current user full control via icacls
    let _ = Command::new("icacls")
        .arg(config_dir.as_os_str())
        .args(["/grant", "Everyone:(OI)(CI)F", "/T", "/Q"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

/// Resolve the EqualizerAPO config directory from registry or default path.
fn resolve_eqapo_config_dir() -> PathBuf {
    // Try HKCU first (VanySound stores its ConfigDir here)
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey(REGISTRY_PATH) {
        if let Ok(dir) = key.get_value::<String, _>("ConfigDir") {
            if !dir.trim().is_empty() {
                return PathBuf::from(dir);
            }
        }
    }

    // Try EqualizerAPO install path from HKLM
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey(r"SOFTWARE\EqualizerAPO") {
        if let Ok(install_path) = key.get_value::<String, _>("InstallPath") {
            if !install_path.trim().is_empty() {
                return PathBuf::from(install_path).join("config");
            }
        }
    }

    PathBuf::from(r"C:\Program Files\EqualizerAPO\config")
}

/// Read the target cable endpoint GUID from registry (HiFiEndpointGuid).
/// Returns the GUID string like "{99f3b2ab-07ec-49f5-84bd-8a45eb0300a9}".
fn resolve_target_cable_guid() -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey(REGISTRY_PATH) {
        if let Ok(guid) = key.get_value::<String, _>(HIFI_ENDPOINT_GUID_VALUE_NAME) {
            if !guid.trim().is_empty() {
                return Some(guid);
            }
        }
    }

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey(REGISTRY_PATH) {
        if let Ok(guid) = key.get_value::<String, _>(HIFI_ENDPOINT_GUID_VALUE_NAME) {
            if !guid.trim().is_empty() {
                return Some(guid);
            }
        }
    }

    None
}

/// Read the target cable endpoint NAME from registry (HiFiEndpointName).
/// Returns the friendly name like "VanySound".
fn resolve_target_cable_name() -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey(REGISTRY_PATH) {
        if let Ok(name) = key.get_value::<String, _>("HiFiEndpointName") {
            if !name.trim().is_empty() {
                return Some(name);
            }
        }
    }

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) = hklm.open_subkey(REGISTRY_PATH) {
        if let Ok(name) = key.get_value::<String, _>("HiFiEndpointName") {
            if !name.trim().is_empty() {
                return Some(name);
            }
        }
    }

    None
}
