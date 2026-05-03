
use aes::Aes256;
use anyhow::{anyhow, bail, Context, Result};
use cbc::{Decryptor, Encryptor};
use cipher::block_padding::Pkcs7;
use cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io::{ErrorKind, Write};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use winreg::enums::*;
use winreg::{RegKey, RegValue};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::Storage::FileSystem::{
    SetFileAttributesW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_SYSTEM, FILE_FLAGS_AND_ATTRIBUTES,
};
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, OpenProcessToken, SetEvent,
};

const EVENT_NAME: &str = "Global\\VanySoundProfileChanged";
const REGISTRY_PATH: &str = r"SOFTWARE\VanySound";
const RENDER_DEVICES_REGISTRY_PATH: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render";
const CHILD_APO_REGISTRY_PATH: &str = r"SOFTWARE\EqualizerAPO\Child APOs";
const HELPER_VERSION: &str = "control-plane-v4";
const HIFI_ENDPOINT_GUID_VALUE_NAME: &str = "HiFiEndpointGuid";
const HIFI_ENDPOINT_NAME_VALUE_NAME: &str = "HiFiEndpointName";
const MATERIALIZED_REVISION_VALUE_NAME: &str = "MaterializedRevision";
const MATERIALIZED_SLOT_VALUE_NAME: &str = "MaterializedSlot";
const LAST_CONFIRMED_REVISION_VALUE_NAME: &str = "LastConfirmedRevision";
const LEGACY_ACTIVE_FILE_NAME: &str = "sys_active.cfg";
const LEGACY_AUX_FILE_NAME: &str = "sys_aux.cfg";
const MATERIALIZED_SUBDIR_NAME: &str = "cache";
const SYSTEM_SID: &str = "*S-1-5-18";
const LOCAL_SERVICE_SID: &str = "*S-1-5-19";
const ADMINISTRATORS_SID: &str = "*S-1-5-32-544";
const FX_VALUE_LFX: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},1";
const FX_VALUE_GFX: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},2";
const FX_VALUE_POST_MIX: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},3";
const FX_VALUE_SFX: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},5";
const FX_VALUE_MFX: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6";
const FX_VALUE_EFX: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},7";
const EQUALIZER_APO_PRE_MIX_GUID: &str = "{EACD2258-FCAC-4FF4-B36D-419E924A6D79}";
const EQUALIZER_APO_POST_MIX_GUID: &str = "{EC1CC9CE-FAED-4822-828A-82A81A6F018F}";
const EQUALIZER_APO_POST_MIX_INSTALL_GUID: &str = "{5860E1C5-F95C-4a7a-8EC8-8AEF24F379A1}";
const EQUALIZER_APO_CHILD_APO_GUID: &str = "{62dc1a93-ae24-464c-a43e-452f824c4250}";
const EQUALIZER_APO_CHILD_PROC_GUID: &str = "{637c490d-eee3-4c0a-973f-371958802da2}";
const FX_VALUE_INSTALL_BLOB1: &str = "{fc52a749-4be9-4510-896e-966ba6525980},3";
const FX_VALUE_INSTALL_BLOB2: &str = "{9c00eeed-edce-4cd8-ae08-cb05e8ef57a0},3";
const FX_VALUE_INSTALL_DWORD: &str = "{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},5";
const DISABLE_AUTO_ADJUST_VALUE_NAME: &str = "DisableAutomaticAdjustment";
const ALLOW_SILENT_BUFFER_VALUE_NAME: &str = "AllowSilentBufferModification";
const PRE_MIX_CHILD_VALUE_NAME: &str = "PreMixChild";
const POST_MIX_CHILD_VALUE_NAME: &str = "PostMixChild";
const VERSION_VALUE_NAME: &str = "Version";
const CHILD_BACKUP_PREFIX: &str = "VanyBackup::";
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
const MANAGE_DEVICE_SELECTOR_ENABLED: bool = true;

static DEVICE_SELECTOR_BACKUP_VALUE_NAMES: [&str; 5] =
    [FX_VALUE_LFX, FX_VALUE_GFX, FX_VALUE_SFX, FX_VALUE_MFX, FX_VALUE_EFX];
static PREFERRED_HIFI_ENDPOINT_NEEDLES: [&str; 6] = [
    "echo plus hi-fi",
    "vb-audio hi-fi cable",
    "hi-fi cable",
    "hifi cable",
    "echo plus",
    "hi-fi",
];
static SECONDARY_HIFI_ENDPOINT_NEEDLES: [&str; 2] = ["vanysound.com", "vanysound"];
static EXCLUDED_ENDPOINT_NEEDLES: [&str; 7] =
    ["voicemeeter", "vaio", "microphone", "mic", "stream", "chat", "aux"];
static DEVICE_SELECTOR_MANAGED_VALUE_NAMES: [&str; 9] = [
    FX_VALUE_LFX,
    FX_VALUE_GFX,
    FX_VALUE_POST_MIX,
    FX_VALUE_SFX,
    FX_VALUE_MFX,
    FX_VALUE_EFX,
    FX_VALUE_INSTALL_BLOB1,
    FX_VALUE_INSTALL_BLOB2,
    FX_VALUE_INSTALL_DWORD,
];
const INSTALL_BLOB1_BYTES: [u8; 12] = [0x0b, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00];
const INSTALL_BLOB2_BYTES: [u8; 12] = [0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00];

#[derive(Clone)]
struct ProfilePayload {
    id: u32,
    name: String,
    config: String,
    strategy: String,
}

#[derive(Clone)]
struct EndpointInfo {
    guid: String,
    name: String,
}

#[derive(Clone)]
struct FileSnapshot {
    exists: bool,
    content: String,
}

#[derive(Clone)]
struct RawRegistryValue {
    bytes: Vec<u8>,
    vtype: RegType,
}

#[derive(Clone, Default)]
struct RegistryValueSnapshot {
    exists: bool,
    value: Option<RawRegistryValue>,
}

#[derive(Clone, Default)]
struct RegistryKeySnapshot {
    exists: bool,
    values: HashMap<String, RegistryValueSnapshot>,
}

#[derive(Clone, Default)]
struct SelectorSnapshot {
    fx: RegistryKeySnapshot,
    child: RegistryKeySnapshot,
}

#[derive(Clone)]
struct TransactionSnapshot {
    active_profile: u32,
    config_dir: PathBuf,
    target_endpoint_guid: Option<String>,
    materialized_revision: Option<String>,
    last_confirmed_revision: Option<String>,
    materialized_slot: String,
    config: FileSnapshot,
    legacy_active: FileSnapshot,
    legacy_aux: FileSnapshot,
    active_a: FileSnapshot,
    aux_a: FileSnapshot,
    active_b: FileSnapshot,
    aux_b: FileSnapshot,
    selector: Option<SelectorSnapshot>,
}

fn main() {
    if let Err(err) = real_main() {
        eprintln!("ERROR: {err}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if matches!(args.first().map(String::as_str), Some("--silent")) {
        args.remove(0);
    }

    let exit_code = match args.first().map(String::as_str) {
        Some("pack") if args.len() == 3 => command_pack(Path::new(&args[1]), Path::new(&args[2]))?,
        Some("deploy") if args.len() == 2 => command_deploy(Path::new(&args[1]))?,
        Some("switch") if args.len() == 2 => {
            let profile_id: u32 = args[1].parse().context("Profile must be numeric.")?;
            command_switch_internal(profile_id)?
        }
        Some("clear") if args.len() == 1 => command_clear_internal()?,
        Some("status") if args.len() == 1 => command_status_internal()?,
        Some("verify") if args.len() == 1 => command_verify_internal()?,
        Some("repair-device-selector") if args.len() == 1 => command_repair_device_selector()?,
        Some("serve") if args.len() == 1 => {
            println!("RESULT=ERROR");
            println!("MESSAGE=serve is not implemented in the native switch helper.");
            1
        }
        _ => {
            print_usage();
            1
        }
    };

    if exit_code != 0 {
        std::process::exit(exit_code);
    }
    Ok(())
}

fn print_usage() {
    println!("Usage:");
    println!("  vanysound-app.exe pack <profilesDir> <outputFile>");
    println!("  vanysound-app.exe deploy <bundleFile>");
    println!("  vanysound-app.exe switch <1-4>");
    println!("  vanysound-app.exe clear");
    println!("  vanysound-app.exe status");
    println!("  vanysound-app.exe verify");
    println!("  vanysound-app.exe repair-device-selector");
}

fn log_line(message: &str) {
    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(
            file,
            "[{}] {}",
            chrono_like_now(),
            message
        );
    }
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{now}")
}

fn common_data_root() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("VanySound")
}

fn user_data_root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .unwrap_or_else(common_data_root)
        .join("VanySound")
}

fn legacy_bundle_path() -> PathBuf {
    common_data_root().join("profiles.bin")
}

fn default_bundle_path() -> PathBuf {
    user_data_root().join("profiles.bin")
}

fn bundle_is_usable(path: &Path) -> bool {
    path.is_file() && fs::File::open(path).is_ok()
}

fn configured_bundle_path() -> Option<PathBuf> {
    load_registry_string_value("BundlePath")
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
}

fn available_bundle_path() -> Option<PathBuf> {
    for candidate in [
        configured_bundle_path(),
        Some(default_bundle_path()),
        Some(legacy_bundle_path()),
    ]
    .into_iter()
    .flatten()
    {
        if bundle_is_usable(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn bundle_available() -> bool {
    available_bundle_path().is_some()
}

fn bundle_path() -> PathBuf {
    if let Some(bundle) = available_bundle_path() {
        return bundle;
    }

    if let Some(configured) = configured_bundle_path() {
        return configured;
    }

    default_bundle_path()
}

fn log_path() -> PathBuf {
    common_data_root().join("logs").join("control.log")
}

fn helper_path() -> String {
    std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("vanysound-app.exe"))
        .display()
        .to_string()
}

fn profile_names() -> HashMap<u32, &'static str> {
    HashMap::from([
        (1, "Gaming / Footsteps"),
        (2, "Misa"),
        (3, "Loudness EQ"),
        (4, "Full EQ"),
    ])
}

fn open_hklm() -> RegKey {
    RegKey::predef(HKEY_LOCAL_MACHINE)
}

fn open_hkcu() -> RegKey {
    RegKey::predef(HKEY_CURRENT_USER)
}

fn save_registry_string(name: &str, value: &str) -> Result<()> {
    let hkcu = open_hkcu();
    let (user_key, _) = hkcu
        .create_subkey(REGISTRY_PATH)
        .with_context(|| format!("Unable to open HKCU\\{} for {}.", REGISTRY_PATH, name))?;
    user_key
        .set_value(name, &value)
        .with_context(|| format!("Unable to write {} in HKCU\\{}.", name, REGISTRY_PATH))?;

    let hklm = open_hklm();
    if let Ok((machine_key, _)) = hklm.create_subkey(REGISTRY_PATH) {
        let _ = machine_key.set_value(name, &value);
    }
    Ok(())
}

fn save_registry_dword(name: &str, value: u32) -> Result<()> {
    let hkcu = open_hkcu();
    let (user_key, _) = hkcu
        .create_subkey(REGISTRY_PATH)
        .with_context(|| format!("Unable to open HKCU\\{} for {}.", REGISTRY_PATH, name))?;
    user_key
        .set_value(name, &value)
        .with_context(|| format!("Unable to write {} in HKCU\\{}.", name, REGISTRY_PATH))?;

    let hklm = open_hklm();
    if let Ok((machine_key, _)) = hklm.create_subkey(REGISTRY_PATH) {
        let _ = machine_key.set_value(name, &value);
    }
    Ok(())
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

fn load_registry_dword_value(name: &str) -> Option<u32> {
    let hkcu = open_hkcu();
    if let Ok(key) = hkcu.open_subkey(REGISTRY_PATH) {
        if let Ok(value) = key.get_value::<u32, _>(name) {
            return Some(value);
        }
    }

    let hklm = open_hklm();
    let key = hklm.open_subkey(REGISTRY_PATH).ok()?;
    key.get_value::<u32, _>(name).ok()
}

fn delete_registry_value(name: &str) {
    let hkcu = open_hkcu();
    if let Ok(key) = hkcu.open_subkey_with_flags(REGISTRY_PATH, KEY_SET_VALUE) {
        let _ = key.delete_value(name);
    }

    let hklm = open_hklm();
    if let Ok(key) = hklm.open_subkey_with_flags(REGISTRY_PATH, KEY_SET_VALUE) {
        let _ = key.delete_value(name);
    }
}

fn use_embedded_engine() -> bool {
    load_registry_string_value("EngineVersion")
        .map(|value| value.to_ascii_lowercase().starts_with("embedded-engine-"))
        .unwrap_or(false)
}

fn load_active_profile() -> u32 {
    load_registry_dword_value("ActiveProfile").unwrap_or(1u32)
}

fn resolve_config_dir() -> PathBuf {
    if let Some(configured) = load_registry_string_value("ConfigDir") {
        if !configured.trim().is_empty() {
            return PathBuf::from(configured);
        }
    }

    let hklm = open_hklm();
    if let Ok(key) = hklm.open_subkey(r"SOFTWARE\EqualizerAPO") {
        if let Ok(install_path) = key.get_value::<String, _>("InstallPath") {
            if !install_path.trim().is_empty() {
                return PathBuf::from(install_path).join("config");
            }
        }
    }

    PathBuf::from(r"C:\Program Files\EqualizerAPO\config")
}

fn resolve_engine_root() -> PathBuf {
    resolve_config_dir()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\EqualizerAPO"))
}

fn read_u32_le(bytes: &[u8], cursor: &mut usize) -> Result<u32> {
    if *cursor + 4 > bytes.len() {
        bail!("Unexpected end of bundle.");
    }
    let value = u32::from_le_bytes(bytes[*cursor..*cursor + 4].try_into().unwrap());
    *cursor += 4;
    Ok(value)
}

fn read_len_prefixed_utf8(bytes: &[u8], cursor: &mut usize) -> Result<String> {
    let len = read_u32_le(bytes, cursor)? as usize;
    if *cursor + len > bytes.len() {
        bail!("Unexpected end of bundle.");
    }
    let text = String::from_utf8(bytes[*cursor..*cursor + len].to_vec()).context("Invalid UTF-8 in bundle.")?;
    *cursor += len;
    Ok(text)
}

fn bundle_key() -> [u8; 32] {
    Sha256::digest(b"VanySoundControl-Profiles-v1").into()
}

fn encrypt(plaintext: &[u8]) -> Result<Vec<u8>> {
    let mut iv = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut iv);
    let key = bundle_key();
    let mut buffer = plaintext.to_vec();
    buffer.resize(plaintext.len() + 16, 0);
    let cipher = Encryptor::<Aes256>::new((&key).into(), (&iv).into())
        .encrypt_padded_mut::<Pkcs7>(&mut buffer, plaintext.len())
        .map_err(|_| anyhow!("Could not encrypt bundle."))?
        .to_vec();
    let mut result = iv.to_vec();
    result.extend_from_slice(&cipher);
    Ok(result)
}

fn decrypt(ciphertext: &[u8]) -> Result<Vec<u8>> {
    if ciphertext.len() < 17 {
        bail!("Encrypted bundle too short.");
    }
    let key = bundle_key();
    let (iv, rest) = ciphertext.split_at(16);
    let mut buffer = rest.to_vec();
    let plaintext = Decryptor::<Aes256>::new((&key).into(), iv.into())
        .decrypt_padded_mut::<Pkcs7>(&mut buffer)
        .map_err(|_| anyhow!("Could not decrypt bundle."))?;
    Ok(plaintext.to_vec())
}

fn pack_profiles(profiles: &HashMap<u32, ProfilePayload>) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"EAPF");
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&(profiles.len() as u32).to_le_bytes());
    let mut ids: Vec<u32> = profiles.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let profile = &profiles[&id];
        bytes.extend_from_slice(&profile.id.to_le_bytes());
        push_utf8(&mut bytes, &profile.name);
        push_utf8(&mut bytes, &profile.config);
        push_utf8(&mut bytes, &profile.strategy);
    }
    bytes
}

fn push_utf8(bytes: &mut Vec<u8>, text: &str) {
    let encoded = text.as_bytes();
    bytes.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
    bytes.extend_from_slice(encoded);
}

fn unpack_profiles(bytes: &[u8]) -> Result<HashMap<u32, ProfilePayload>> {
    let mut cursor = 0usize;
    if bytes.len() < 12 || &bytes[0..4] != b"EAPF" {
        bail!("Unsupported profile bundle.");
    }
    cursor += 4;
    let version = read_u32_le(bytes, &mut cursor)?;
    if version != 1 {
        bail!("Unsupported profile bundle version.");
    }
    let count = read_u32_le(bytes, &mut cursor)? as usize;
    let mut profiles = HashMap::new();
    for _ in 0..count {
        let id = read_u32_le(bytes, &mut cursor)?;
        let name = read_len_prefixed_utf8(bytes, &mut cursor)?;
        let config = read_len_prefixed_utf8(bytes, &mut cursor)?;
        let strategy = read_len_prefixed_utf8(bytes, &mut cursor)?;
        profiles.insert(
            id,
            ProfilePayload {
                id,
                name,
                config,
                strategy,
            },
        );
    }
    Ok(profiles)
}

fn load_bundle(bundle_file: &Path) -> Result<HashMap<u32, ProfilePayload>> {
    let encrypted = fs::read(bundle_file).with_context(|| format!("Could not read {}", bundle_file.display()))?;
    unpack_profiles(&decrypt(&encrypted)?)
}

fn compute_file_sha256(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn materialized_payload_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(MATERIALIZED_SUBDIR_NAME)
}

fn normalize_materialized_slot(value: &str) -> &'static str {
    if value.trim().eq_ignore_ascii_case("b") {
        "b"
    } else {
        "a"
    }
}

fn load_materialized_slot() -> String {
    load_registry_string_value(MATERIALIZED_SLOT_VALUE_NAME)
        .map(|value| normalize_materialized_slot(&value).to_string())
        .unwrap_or_else(|| "a".to_string())
}

fn alternate_materialized_slot(current_slot: &str) -> &'static str {
    if normalize_materialized_slot(current_slot) == "a" {
        "b"
    } else {
        "a"
    }
}

fn materialized_main_file_name_for_slot(slot: &str) -> String {
    format!("boot_{}.dat", normalize_materialized_slot(slot))
}

fn materialized_aux_file_name_for_slot(slot: &str) -> String {
    format!("stage_{}.dat", normalize_materialized_slot(slot))
}

fn materialized_active_path_for_slot(config_dir: &Path, slot: &str) -> PathBuf {
    materialized_payload_dir(config_dir).join(materialized_main_file_name_for_slot(slot))
}

fn materialized_aux_path_for_slot(config_dir: &Path, slot: &str) -> PathBuf {
    materialized_payload_dir(config_dir).join(materialized_aux_file_name_for_slot(slot))
}

fn materialized_active_path(config_dir: &Path) -> PathBuf {
    materialized_active_path_for_slot(config_dir, &load_materialized_slot())
}

fn materialized_aux_path(config_dir: &Path) -> PathBuf {
    materialized_aux_path_for_slot(config_dir, &load_materialized_slot())
}

fn materialized_main_include_path_for_slot(slot: &str) -> String {
    format!(r"{}\{}", MATERIALIZED_SUBDIR_NAME, materialized_main_file_name_for_slot(slot))
}

fn materialized_main_include_path() -> String {
    materialized_main_include_path_for_slot(&load_materialized_slot())
}

fn generate_materialized_revision() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{millis:x}")
}

fn load_materialized_revision() -> String {
    load_registry_string_value(MATERIALIZED_REVISION_VALUE_NAME)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "base".to_string())
}

fn load_last_confirmed_revision() -> Option<String> {
    load_registry_string_value(LAST_CONFIRMED_REVISION_VALUE_NAME)
        .filter(|value| !value.trim().is_empty())
}

fn save_last_confirmed_revision(value: &str) -> Result<()> {
    save_registry_string(LAST_CONFIRMED_REVISION_VALUE_NAME, value)
}

fn clear_last_confirmed_revision() {
    delete_registry_value(LAST_CONFIRMED_REVISION_VALUE_NAME);
}

fn append_normalized_lines(lines: &mut Vec<String>, text: &str) {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        lines.push(trimmed.to_string());
    }
}

fn resolve_vst_library_path(line: &str) -> String {
    let lower = line.to_ascii_lowercase();
    if !lower.starts_with("vstplugin:") {
        return line.to_string();
    }
    let after_keyword = &line["VSTPlugin:".len()..];
    let library_idx = after_keyword.to_ascii_lowercase().find("library ");
    let library_idx = match library_idx {
        Some(idx) => idx,
        None => return line.to_string(),
    };
    let after_library = &after_keyword[library_idx + "library ".len()..];
    let dll_end = after_library.find(' ').unwrap_or(after_library.len());
    let dll_name = after_library[..dll_end].trim();
    if dll_name.contains('\\') || dll_name.contains('/') || dll_name.contains(':') {
        return line.to_string();
    }
    let vst_dir = resolve_engine_root().join("VSTPlugins");
    let absolute_dll = vst_dir.join(dll_name);
    if !absolute_dll.is_file() {
        log_line(&format!(
            "WARN resolve_vst_library_path: DLL not found at {}, keeping relative",
            absolute_dll.display()
        ));
        return line.to_string();
    }
    let prefix = &line[..("VSTPlugin:".len() + library_idx + "library ".len())];
    let suffix = &after_library[dll_end..];
    format!("{}{}{}", prefix, absolute_dll.display(), suffix)
}

fn normalize_config(text: &str, inline_strategy_text: Option<&str>) -> String {
    let mut lines = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.to_ascii_lowercase().starts_with("include:") {
            if let Some(strategy_text) = inline_strategy_text {
                append_normalized_lines(&mut lines, strategy_text);
            }
            continue;
        }
        lines.push(trimmed.to_string());
    }
    let mut result = lines.join("\r\n");
    result.push_str("\r\n");
    result
}

fn render_main_config_for_slot(slot: &str) -> String {
    let revision = load_materialized_revision();
    format!(
        "# Windows Audio Subsystem - Driver Configuration\r\n# DO NOT EDIT - Managed by VanySoundControl\r\n# REV: {}\r\nInclude: {}\r\n",
        revision,
        materialized_main_include_path_for_slot(&slot),
    )
}

fn render_main_config() -> String {
    render_main_config_for_slot(&load_materialized_slot())
}

fn resolve_target_cable_name() -> String {
    load_registry_string_value(HIFI_ENDPOINT_NAME_VALUE_NAME)
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "VanySound".to_string())
}

fn scope_config_to_target_device(config: &str) -> String {
    if config
        .lines()
        .any(|line| line.trim_start().to_ascii_lowercase().starts_with("device:"))
    {
        return config.to_string();
    }

    format!("Device: {}\r\n{}", resolve_target_cable_name(), config)
}

fn render_profile_config(profile: &ProfilePayload) -> String {
    let normalized = normalize_config(
        &profile.config,
        (!profile.strategy.is_empty()).then_some(profile.strategy.as_str()),
    );
    scope_config_to_target_device(&normalized)
}

fn render_inline_main_config(profile: &ProfilePayload) -> String {
    let revision = load_materialized_revision();
    let scoped = render_profile_config(profile);
    format!(
        "# Windows Audio Subsystem - Driver Configuration\r\n# DO NOT EDIT - Managed by VanySoundControl\r\n# REV: {}\r\n{}",
        revision,
        scoped,
    )
}

fn render_embedded_decoy_config() -> String {
    "# Windows Audio Subsystem - Driver Configuration\r\n# VanySound embedded engine active\r\n# No operational filters are stored here.\r\n".to_string()
}

fn render_cleared_config() -> String {
    let revision = load_materialized_revision();
    format!(
        "# Windows Audio Subsystem - Driver Configuration\r\n# VanySound switcher closed\r\n# REV: {}\r\n# No active profile is currently materialized.\r\n",
        revision
    )
}

fn should_force_detach(previous_profile: u32, target_profile: u32) -> bool {
    previous_profile != 0 && (previous_profile != target_profile || target_profile == 2)
}

fn should_force_recommit(previous_profile: u32, target_profile: u32) -> bool {
    let _ = previous_profile;
    target_profile != 0
}

fn is_sharing_violation(err: &std::io::Error) -> bool {
    err.raw_os_error() == Some(32)
}

fn write_with_sharing_retry(path: &Path, bytes: &[u8]) -> Result<()> {
    const MAX_RETRIES: u32 = 8;
    const BASE_DELAY_MS: u64 = 200;
    let mut last_error = None;
    for attempt in 0..MAX_RETRIES {
        match fs::write(path, bytes) {
            Ok(()) => return Ok(()),
            Err(err) if is_sharing_violation(&err) => {
                let delay = BASE_DELAY_MS * (attempt as u64 + 1);
                log_line(&format!(
                    "SHARING_RETRY write {} attempt={}/{} delay={}ms",
                    path.display(), attempt + 1, MAX_RETRIES, delay
                ));
                thread::sleep(Duration::from_millis(delay));
                last_error = Some(err);
            }
            Err(err) => return Err(err.into()),
        }
    }
    Err(anyhow!(
        "File sharing violation persisted after {} retries for {}: {}",
        MAX_RETRIES,
        path.display(),
        last_error.map(|e| e.to_string()).unwrap_or_default()
    ))
}

fn write_utf8_config_file(path: &Path, text: &str) -> Result<()> {
    let mut bytes = Vec::with_capacity(UTF8_BOM.len() + text.len());
    bytes.extend_from_slice(&UTF8_BOM);
    bytes.extend_from_slice(text.as_bytes());
    write_with_sharing_retry(path, &bytes)
}

fn normalize_text_for_compare(text: &str) -> String {
    text.trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

fn content_preview(text: &str) -> String {
    let normalized = normalize_text_for_compare(text)
        .replace('\n', "\\n")
        .replace('\t', "\\t");
    let mut preview = normalized.chars().take(120).collect::<String>();
    if normalized.chars().count() > 120 {
        preview.push_str("...");
    }
    preview
}

fn build_content_fingerprint(expected: &str, actual: &str) -> String {
    format!(
        "expected_len={} actual_len={} expected_sha={} actual_sha={} expected_head=\"{}\" actual_head=\"{}\"",
        normalize_text_for_compare(expected).len(),
        normalize_text_for_compare(actual).len(),
        format!("{:x}", Sha256::digest(normalize_text_for_compare(expected).as_bytes())),
        format!("{:x}", Sha256::digest(normalize_text_for_compare(actual).as_bytes())),
        content_preview(expected),
        content_preview(actual)
    )
}

fn cleared_config_texts_match(actual: &str) -> bool {
    let normalized = normalize_text_for_compare(actual);
    let lines = normalized
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    if lines.is_empty() {
        return false;
    }

    let has_header = lines
        .iter()
        .any(|line| *line == "# Windows Audio Subsystem - Driver Configuration");
    let has_closed_marker = lines
        .iter()
        .any(|line| *line == "# VanySound switcher closed" || *line == "# EchoAudio switcher closed");
    let has_no_profile_marker = lines
        .iter()
        .any(|line| *line == "# No active profile is currently materialized.");

    if !(has_header && has_closed_marker && has_no_profile_marker) {
        return false;
    }

    !lines.iter().any(|line| {
        let lower = line.to_ascii_lowercase();
        [
            "include:",
            "vstplugin:",
            "filter:",
            "graphiceq:",
            "loudnesscorrection:",
            "preamp:",
            "convolution:",
            "copy:",
        ]
        .iter()
        .any(|needle| lower.starts_with(needle))
    })
}

fn config_texts_match(expected: &str, actual: &str) -> bool {
    let normalized_expected = normalize_text_for_compare(expected);
    let normalized_actual = normalize_text_for_compare(actual);

    normalized_expected == normalized_actual
        || (normalized_expected.contains("# VanySound switcher closed")
            && cleared_config_texts_match(actual))
        || (normalized_expected.contains("# EchoAudio switcher closed")
            && cleared_config_texts_match(actual))
}

fn describe_disk_file(path: &Path) -> String {
    if !path.is_file() {
        return format!("{} {{exists=false}}", path.display());
    }

    let size = fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    let sha = compute_file_sha256(path).unwrap_or_else(|_| "<sha-error>".to_string());
    format!(
        "{} {{exists=true size={} sha256={}}}",
        path.display(),
        size,
        sha
    )
}

fn log_runtime_snapshot_line(context: &str, config_dir: &Path, endpoint: Option<&EndpointInfo>) {
    let config_path = config_dir.join("config.txt");
    let active_slot = load_materialized_slot();
    let active_path = materialized_active_path_for_slot(config_dir, &active_slot);
    let aux_path = materialized_aux_path_for_slot(config_dir, &active_slot);
    let selector_active = endpoint
        .as_ref()
        .map(|ep| is_device_selector_enabled(&ep.guid))
        .unwrap_or(false);

    log_line(&format!(
        "{context} snapshot | activeProfile={} slot={} bundle={} config={} active={} aux={} endpointGuid={} endpointName={} selectorActive={}",
        load_active_profile(),
        active_slot,
        describe_disk_file(&bundle_path()),
        describe_disk_file(&config_path),
        describe_disk_file(&active_path),
        describe_disk_file(&aux_path),
        endpoint.as_ref().map(|ep| ep.guid.as_str()).unwrap_or(""),
        endpoint.as_ref().map(|ep| ep.name.as_str()).unwrap_or(""),
        selector_active
    ));
}

fn set_path_attributes(path: &Path, attributes: u32) {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let _ = SetFileAttributesW(
            PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(attributes),
        );
    }
}

fn set_directory_attributes(path: &Path, hidden: bool) {
    if !path.is_dir() {
        return;
    }
    let attributes = if hidden {
        FILE_ATTRIBUTE_DIRECTORY.0 | FILE_ATTRIBUTE_HIDDEN.0 | FILE_ATTRIBUTE_SYSTEM.0
    } else {
        FILE_ATTRIBUTE_DIRECTORY.0
    };
    set_path_attributes(path, attributes);
}

fn run_icacls(path: &Path, args: &[&str]) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let system_root =
        std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string());
    let icacls_path = PathBuf::from(system_root).join("System32").join("icacls.exe");

    const MAX_RETRIES: u32 = 4;
    const BASE_DELAY_MS: u64 = 300;
    let mut last_stderr = String::new();
    let mut last_stdout = String::new();
    let mut last_code: Option<i32> = None;

    for attempt in 0..MAX_RETRIES {
        let output = {
            use std::os::windows::process::CommandExt;
            Command::new(&icacls_path)
                .arg(path)
                .args(args)
                .creation_flags(0x0800_0000)
                .output()
                .with_context(|| {
                    format!(
                        "No se pudo ejecutar {} para {}",
                        icacls_path.display(),
                        path.display()
                    )
                })?
        };

        if output.status.success() {
            return Ok(());
        }

        last_stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        last_stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        last_code = output.status.code();

        let is_sharing = last_stderr.contains("being used by another process")
            || last_stderr.contains("utilizado por otro proceso")
            || last_stdout.contains("being used by another process")
            || last_stdout.contains("utilizado por otro proceso");

        if is_sharing && attempt + 1 < MAX_RETRIES {
            let delay = BASE_DELAY_MS * (attempt as u64 + 1);
            log_line(&format!(
                "SHARING_RETRY icacls {} attempt={}/{} delay={}ms",
                path.display(), attempt + 1, MAX_RETRIES, delay
            ));
            thread::sleep(Duration::from_millis(delay));
            continue;
        }
        break;
    }

    bail!(
        "icacls fallo para {} (code={:?}) stdout='{}' stderr='{}'",
        path.display(),
        last_code,
        last_stdout,
        last_stderr
    );
}

fn relax_materialized_payload_acl(config_dir: &Path) -> Result<()> {
    let materialized_dir = materialized_payload_dir(config_dir);
    if materialized_dir.exists() {
        run_icacls(&materialized_dir, &["/reset", "/T", "/C"])?;
    }
    Ok(())
}

fn harden_materialized_payload_acl(config_dir: &Path) -> Result<()> {
    let materialized_dir = materialized_payload_dir(config_dir);
    if materialized_dir.exists() {
        let grant_system = format!("{SYSTEM_SID}:(OI)(CI)(F)");
        let grant_local_service = format!("{LOCAL_SERVICE_SID}:(OI)(CI)(RX)");
        let grant_admins = format!("{ADMINISTRATORS_SID}:(OI)(CI)(F)");
        run_icacls(
            &materialized_dir,
            &[
                "/inheritance:r",
                "/grant:r",
                &grant_system,
                &grant_local_service,
                &grant_admins,
                "/T",
                "/C",
            ],
        )?;
    }
    Ok(())
}

fn set_managed_file_attributes(config_dir: &Path, attributes: u32) {
    for file_name in ["config.txt", LEGACY_ACTIVE_FILE_NAME, LEGACY_AUX_FILE_NAME] {
        let path = config_dir.join(file_name);
        if path.is_file() {
            set_path_attributes(&path, attributes);
        }
    }

    for path in [
        materialized_active_path_for_slot(config_dir, "a"),
        materialized_aux_path_for_slot(config_dir, "a"),
        materialized_active_path_for_slot(config_dir, "b"),
        materialized_aux_path_for_slot(config_dir, "b"),
    ] {
        if path.is_file() {
            set_path_attributes(&path, attributes);
        }
    }
}

fn ensure_writable(config_dir: &Path) -> Result<()> {
    fs::create_dir_all(config_dir)?;
    relax_materialized_payload_acl(config_dir)?;
    let materialized_dir = materialized_payload_dir(config_dir);
    if materialized_dir.is_dir() {
        set_directory_attributes(&materialized_dir, false);
    }
    set_managed_file_attributes(config_dir, FILE_ATTRIBUTE_NORMAL.0);
    Ok(())
}

fn needs_materialized_payload_read_unlock(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    matches!(
        fs::read_to_string(path),
        Err(error) if error.kind() == ErrorKind::PermissionDenied
    )
}

fn rehide_config_dir(config_dir: &Path) {
    set_path_attributes(
        config_dir,
        FILE_ATTRIBUTE_DIRECTORY.0 | FILE_ATTRIBUTE_HIDDEN.0 | FILE_ATTRIBUTE_SYSTEM.0,
    );
    set_managed_file_attributes(
        config_dir,
        FILE_ATTRIBUTE_HIDDEN.0 | FILE_ATTRIBUTE_SYSTEM.0 | FILE_ATTRIBUTE_READONLY.0,
    );
    let materialized_dir = materialized_payload_dir(config_dir);
    set_directory_attributes(&materialized_dir, true);
}

fn ensure_materialized_payload_dir(config_dir: &Path) -> Result<PathBuf> {
    let path = materialized_payload_dir(config_dir);
    fs::create_dir_all(&path)?;
    set_directory_attributes(&path, false);
    Ok(path)
}

fn purge_legacy_materialized_files(config_dir: &Path) {
    for file_name in [LEGACY_ACTIVE_FILE_NAME, LEGACY_AUX_FILE_NAME] {
        let path = config_dir.join(file_name);
        if path.is_file() {
            set_path_attributes(&path, FILE_ATTRIBUTE_NORMAL.0);
            let _ = fs::remove_file(path);
        }
    }
    for file_name in [
        "sys_p1.cfg",
        "sys_p2.cfg",
        "sys_p3.cfg",
        "sys_p4.cfg",
        "boot.dat",
        "stage.dat",
        "echo_profile1.txt",
        "echo_profile2.txt",
        "echo_profile3.txt",
        "echo_profile4.txt",
    ] {
        let path = config_dir.join(file_name);
        if path.is_file() {
            set_path_attributes(&path, FILE_ATTRIBUTE_NORMAL.0);
            let _ = fs::remove_file(path);
        }
    }
    for path in [
        materialized_payload_dir(config_dir).join("boot.dat"),
        materialized_payload_dir(config_dir).join("stage.dat"),
    ] {
        if path.is_file() {
            set_path_attributes(&path, FILE_ATTRIBUTE_NORMAL.0);
            let _ = fs::remove_file(path);
        }
    }
}

fn clear_materialized_payload(config_dir: &Path) {
    for path in [
        materialized_active_path_for_slot(config_dir, "a"),
        materialized_aux_path_for_slot(config_dir, "a"),
        materialized_active_path_for_slot(config_dir, "b"),
        materialized_aux_path_for_slot(config_dir, "b"),
    ] {
        let _ = fs::remove_file(path);
    }
    purge_legacy_materialized_files(config_dir);
    let materialized_dir = materialized_payload_dir(config_dir);
    let _ = fs::remove_dir(&materialized_dir);
}

fn signal_profile_changed() {
    let name: Vec<u16> = OsStr::new(EVENT_NAME).encode_wide().chain(Some(0)).collect();
    unsafe {
        if let Ok(handle) = CreateEventW(None, false, false, PCWSTR(name.as_ptr())) {
            if !handle.is_invalid() {
                let _ = SetEvent(handle);
                let _ = CloseHandle(handle);
            }
        }
    }
}

fn decode_registry_text(value: &RegValue) -> Option<String> {
    match value.vtype {
        REG_SZ | REG_EXPAND_SZ => decode_utf16_string(&value.bytes),
        REG_BINARY => {
            if value.bytes.len() >= 10 {
                let vt = u16::from_le_bytes([value.bytes[0], value.bytes[1]]);
                if vt == 31 {
                    return decode_utf16_string(&value.bytes[8..]);
                }
            }
            decode_utf16_string(&value.bytes)
        }
        _ => None,
    }
}

fn decode_utf16_string(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 2 {
        return None;
    }
    let u16s: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .take_while(|ch| *ch != 0)
        .collect();
    let text = String::from_utf16_lossy(&u16s).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn get_hifi_endpoint_match_score(text: &str) -> i32 {
    let normalized = text.trim().to_ascii_lowercase();
    if normalized.is_empty() || EXCLUDED_ENDPOINT_NEEDLES.iter().any(|needle| normalized.contains(needle)) {
        return 0;
    }
    for (idx, needle) in PREFERRED_HIFI_ENDPOINT_NEEDLES.iter().enumerate() {
        if normalized.contains(needle) {
            return 100 - idx as i32;
        }
    }
    for (idx, needle) in SECONDARY_HIFI_ENDPOINT_NEEDLES.iter().enumerate() {
        if normalized.contains(needle) {
            return 20 - idx as i32;
        }
    }
    0
}

fn resolve_endpoint_display_name(endpoint_guid: &str) -> String {
    let hklm = open_hklm();
    let path = format!(r"{}\{}\Properties", RENDER_DEVICES_REGISTRY_PATH, endpoint_guid);
    let Ok(key) = hklm.open_subkey(path) else {
        return endpoint_guid.to_string();
    };
    let mut best_score = 0;
    let mut best_text = None;
    for entry in key.enum_values().flatten() {
        if let Some(decoded) = decode_registry_text(&entry.1) {
            let score = get_hifi_endpoint_match_score(&decoded);
            if score > best_score {
                best_score = score;
                best_text = Some(decoded);
            }
        }
    }
    best_text.unwrap_or_else(|| endpoint_guid.to_string())
}

fn create_endpoint_info(endpoint_guid: &str) -> EndpointInfo {
    EndpointInfo {
        guid: endpoint_guid.to_string(),
        name: resolve_endpoint_display_name(endpoint_guid),
    }
}

fn detect_hifi_render_endpoints() -> Vec<EndpointInfo> {
    let hklm = open_hklm();
    let Ok(root) = hklm.open_subkey(RENDER_DEVICES_REGISTRY_PATH) else {
        return Vec::new();
    };
    let mut best: HashMap<String, i32> = HashMap::new();
    for endpoint_guid in root.enum_keys().flatten() {
        let path = format!(r"{}\{}\Properties", RENDER_DEVICES_REGISTRY_PATH, endpoint_guid);
        let Ok(props) = hklm.open_subkey(path) else {
            continue;
        };
        for entry in props.enum_values().flatten() {
            if let Some(decoded) = decode_registry_text(&entry.1) {
                let score = get_hifi_endpoint_match_score(&decoded);
                if score > 0 {
                    let current = best.entry(endpoint_guid.clone()).or_insert(0);
                    if score > *current {
                        *current = score;
                    }
                }
            }
        }
    }
    let Some(best_score) = best.values().copied().max() else {
        return Vec::new();
    };
    best.into_iter()
        .filter(|(_, score)| *score == best_score)
        .map(|(guid, _)| create_endpoint_info(&guid))
        .collect()
}

fn load_stored_target_endpoint() -> Option<EndpointInfo> {
    let endpoint_guid = load_registry_string_value(HIFI_ENDPOINT_GUID_VALUE_NAME)?;
    let hklm = open_hklm();
    let path = format!(r"{}\{}\Properties", RENDER_DEVICES_REGISTRY_PATH, endpoint_guid);
    if hklm.open_subkey(path).is_err() {
        return None;
    }
    let endpoint_name = load_registry_string_value(HIFI_ENDPOINT_NAME_VALUE_NAME)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| resolve_endpoint_display_name(&endpoint_guid));
    Some(EndpointInfo {
        guid: endpoint_guid,
        name: endpoint_name,
    })
}

fn require_stored_target_endpoint() -> Result<EndpointInfo> {
    load_stored_target_endpoint().ok_or_else(|| {
        anyhow!("HiFiEndpointGuid is not configured. Run repair-device-selector or reinstall.")
    })
}

fn detect_best_hifi_render_endpoint() -> Option<EndpointInfo> {
    detect_hifi_render_endpoints().into_iter().next()
}

fn save_target_endpoint(endpoint: &EndpointInfo) -> Result<()> {
    if endpoint.guid.trim().is_empty() {
        bail!("Target endpoint is missing.");
    }
    save_registry_string(HIFI_ENDPOINT_GUID_VALUE_NAME, &endpoint.guid)?;
    save_registry_string(
        HIFI_ENDPOINT_NAME_VALUE_NAME,
        if endpoint.name.trim().is_empty() {
            &endpoint.guid
        } else {
            &endpoint.name
        },
    )?;
    Ok(())
}

fn reg_value_string(value: &str) -> RegValue {
    let mut bytes: Vec<u8> = OsStr::new(value).encode_wide().flat_map(|c| c.to_le_bytes()).collect();
    bytes.extend_from_slice(&0u16.to_le_bytes());
    RegValue { bytes, vtype: REG_SZ }
}

fn reg_value_dword(value: u32) -> RegValue {
    RegValue {
        bytes: value.to_le_bytes().to_vec(),
        vtype: REG_DWORD,
    }
}

fn reg_value_binary(bytes: &[u8]) -> RegValue {
    RegValue {
        bytes: bytes.to_vec(),
        vtype: REG_BINARY,
    }
}

fn read_registry_string(key: &RegKey, value_name: &str) -> Option<String> {
    key.get_raw_value(value_name).ok().and_then(|value| decode_registry_text(&value).or_else(|| {
        if value.vtype == REG_DWORD && value.bytes.len() >= 4 {
            Some(u32::from_le_bytes(value.bytes[0..4].try_into().unwrap()).to_string())
        } else {
            None
        }
    }))
}

fn capture_registry_key_snapshot(path: &str, value_names: &[String]) -> RegistryKeySnapshot {
    let hklm = open_hklm();
    let mut snapshot = RegistryKeySnapshot::default();
    let Ok(key) = hklm.open_subkey(path) else {
        snapshot.exists = false;
        for name in value_names {
            snapshot
                .values
                .insert(name.clone(), RegistryValueSnapshot::default());
        }
        return snapshot;
    };

    snapshot.exists = true;
    for name in value_names {
        let value_snapshot = if let Ok(value) = key.get_raw_value(name) {
            RegistryValueSnapshot {
                exists: true,
                value: Some(RawRegistryValue {
                    bytes: value.bytes,
                    vtype: value.vtype,
                }),
            }
        } else {
            RegistryValueSnapshot::default()
        };
        snapshot.values.insert(name.clone(), value_snapshot);
    }
    snapshot
}

fn restore_registry_key_snapshot(path: &str, snapshot: &RegistryKeySnapshot, allow_delete_key: bool) -> Result<()> {
    let hklm = open_hklm();
    if !snapshot.exists {
        if allow_delete_key {
            let _ = hklm.delete_subkey_all(path);
            return Ok(());
        }
    }

    let (key, _) = hklm.create_subkey(path)?;
    for (name, value_snapshot) in &snapshot.values {
        if value_snapshot.exists {
            if let Some(value) = &value_snapshot.value {
                key.set_raw_value(
                    name,
                    &RegValue {
                        bytes: value.bytes.clone(),
                        vtype: value.vtype.clone(),
                    },
                )?;
            }
        } else {
            let _ = key.delete_value(name);
        }
    }
    Ok(())
}

fn capture_file_snapshot(path: &Path) -> FileSnapshot {
    FileSnapshot {
        exists: path.is_file(),
        content: fs::read_to_string(path).unwrap_or_default(),
    }
}

fn restore_file_snapshot(path: &Path, snapshot: &FileSnapshot) -> Result<()> {
    if !snapshot.exists {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, &snapshot.content)?;
    Ok(())
}

fn compute_device_selector_backup_value(fx_key: &RegKey, value_name: &str) -> String {
    let current = read_registry_string(fx_key, value_name).unwrap_or_else(|| "!VALUE".to_string());
    if current.eq_ignore_ascii_case(EQUALIZER_APO_PRE_MIX_GUID)
        || current.eq_ignore_ascii_case(EQUALIZER_APO_POST_MIX_GUID)
    {
        "!VALUE".to_string()
    } else {
        current
    }
}

fn get_child_backup_value_name(value_name: &str) -> String {
    format!("{CHILD_BACKUP_PREFIX}{value_name}")
}

fn open_endpoint_fx_key(endpoint_guid: &str, writable: bool) -> Result<RegKey> {
    let hklm = open_hklm();
    let path = format!(r"{}\{}\FxProperties", RENDER_DEVICES_REGISTRY_PATH, endpoint_guid);
    if writable {
        // First attempt: standard open
        match hklm.open_subkey_with_flags(&path, KEY_READ | KEY_WRITE) {
            Ok(key) => Ok(key),
            Err(_first_err) => {
                // MMDevices FxProperties is owned by TrustedInstaller.
                // Take ownership + grant admin FullControl, then retry.
                log_line(&format!(
                    "FxProperties access denied for {endpoint_guid}, attempting ACL takeover"
                ));
                ensure_registry_key_writable(&path)?;
                hklm.open_subkey_with_flags(&path, KEY_READ | KEY_WRITE)
                    .context("FxProperties still not writable after ACL takeover")
            }
        }
    } else {
        Ok(hklm.open_subkey(path)?)
    }
}

/// Take ownership of an HKLM registry key and grant Administrators FullControl.
/// This is required for MMDevices\...\FxProperties keys that are owned by
/// TrustedInstaller and have restrictive ACLs by default.
///
/// Uses PowerShell .NET interop for ACL manipulation since the process is
/// already running elevated when this code path is reached.
fn ensure_registry_key_writable(subkey_path: &str) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let ps_script = build_acl_takeover_script(subkey_path);

    log_line(&format!(
        "ensure_registry_key_writable: invoking PowerShell for {subkey_path}"
    ));

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps_script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .context("Failed to spawn PowerShell for ACL takeover")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() || !stdout.contains("ACL_OK") {
        log_line(&format!(
            "ensure_registry_key_writable FAILED: exit={:?} stdout={} stderr={}",
            output.status.code(), stdout.trim(), stderr.trim()
        ));
        bail!("ACL takeover failed for {}: {}", subkey_path, stderr.trim());
    }

    log_line(&format!("ACL takeover completed for {subkey_path}"));
    Ok(())
}

/// Build the PowerShell script that enables SeTakeOwnershipPrivilege,
/// takes ownership from TrustedInstaller, and grants Administrators FullControl.
fn build_acl_takeover_script(subkey_path: &str) -> String {
    let mut s = String::new();
    s.push_str("$ErrorActionPreference='Stop'; ");
    s.push_str("Add-Type '");
    s.push_str("using System; using System.Runtime.InteropServices; ");
    s.push_str("public class TknP { ");
    s.push_str("[DllImport(\"advapi32.dll\",SetLastError=true)]static extern bool AdjustTokenPrivileges(IntPtr h,bool d,ref TP n,int b,IntPtr p,IntPtr r); ");
    s.push_str("[DllImport(\"advapi32.dll\",SetLastError=true)]static extern bool OpenProcessToken(IntPtr p,uint a,out IntPtr t); ");
    s.push_str("[DllImport(\"advapi32.dll\",SetLastError=true)]static extern bool LookupPrivilegeValue(string s,string n,out L l); ");
    s.push_str("[DllImport(\"kernel32.dll\")]static extern IntPtr GetCurrentProcess(); ");
    s.push_str("struct TP { public int C; public L Ld; public int A; } ");
    s.push_str("struct L { public uint Lo; public int Hi; } ");
    s.push_str("public static void E(string priv){ IntPtr t; OpenProcessToken(GetCurrentProcess(),0x28,out t); TP tp=new TP(); tp.C=1; LookupPrivilegeValue(null,priv,out tp.Ld); tp.A=2; AdjustTokenPrivileges(t,false,ref tp,0,IntPtr.Zero,IntPtr.Zero); } ");
    s.push_str("}'; ");
    s.push_str("[TknP]::E('SeTakeOwnershipPrivilege'); [TknP]::E('SeRestorePrivilege'); ");
    s.push_str(&format!("$p='{}'; ", subkey_path));
    s.push_str("$sid=New-Object System.Security.Principal.SecurityIdentifier('S-1-5-32-544'); ");
    s.push_str("$k=[Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($p,'ReadWriteSubTree','TakeOwnership'); ");
    s.push_str("if($k){$a=$k.GetAccessControl('Owner');$a.SetOwner($sid);$k.SetAccessControl($a);$k.Close()}; ");
    s.push_str("$k2=[Microsoft.Win32.Registry]::LocalMachine.OpenSubKey($p,'ReadWriteSubTree','ChangePermissions'); ");
    s.push_str("if($k2){$a2=$k2.GetAccessControl();$r=New-Object System.Security.AccessControl.RegistryAccessRule($sid,'FullControl','ContainerInherit,ObjectInherit','None','Allow');$a2.AddAccessRule($r);$k2.SetAccessControl($a2);$k2.Close()}; ");
    s.push_str("Write-Output 'ACL_OK'");
    s
}


fn open_child_apo_key(endpoint_guid: &str, writable: bool) -> Result<RegKey> {
    let hklm = open_hklm();
    let path = format!(r"{}\{}", CHILD_APO_REGISTRY_PATH, endpoint_guid);
    if writable {
        Ok(hklm.create_subkey(path)?.0)
    } else {
        Ok(hklm.open_subkey(path)?)
    }
}

fn enable_device_selector_state_for_endpoint(endpoint_guid: &str) -> Result<()> {
    let fx_key = open_endpoint_fx_key(endpoint_guid, true)?;
    let child_key = open_child_apo_key(endpoint_guid, true)?;

    for value_name in DEVICE_SELECTOR_BACKUP_VALUE_NAMES {
        let backup_name = get_child_backup_value_name(value_name);
        if child_key.get_raw_value(&backup_name).is_err() {
            child_key.set_raw_value(&backup_name, &reg_value_string(&compute_device_selector_backup_value(&fx_key, value_name)))?;
        }
    }

    child_key.set_raw_value(FX_VALUE_LFX, &reg_value_string(EQUALIZER_APO_CHILD_APO_GUID))?;
    child_key.set_raw_value(FX_VALUE_GFX, &reg_value_string(EQUALIZER_APO_CHILD_PROC_GUID))?;
    child_key.set_raw_value(FX_VALUE_SFX, &reg_value_string(EQUALIZER_APO_CHILD_APO_GUID))?;
    child_key.set_raw_value(FX_VALUE_MFX, &reg_value_string(EQUALIZER_APO_CHILD_PROC_GUID))?;
    let _ = child_key.delete_value(FX_VALUE_EFX);
    child_key.set_raw_value(PRE_MIX_CHILD_VALUE_NAME, &reg_value_string(EQUALIZER_APO_CHILD_APO_GUID))?;
    child_key.set_raw_value(POST_MIX_CHILD_VALUE_NAME, &reg_value_string(EQUALIZER_APO_CHILD_PROC_GUID))?;
    child_key.set_raw_value(ALLOW_SILENT_BUFFER_VALUE_NAME, &reg_value_string("false"))?;
    child_key.set_raw_value(VERSION_VALUE_NAME, &reg_value_string("2"))?;
    let _ = child_key.delete_value(DISABLE_AUTO_ADJUST_VALUE_NAME);

    fx_key.set_raw_value(FX_VALUE_LFX, &reg_value_string(EQUALIZER_APO_PRE_MIX_GUID))?;
    fx_key.set_raw_value(FX_VALUE_GFX, &reg_value_string(EQUALIZER_APO_POST_MIX_GUID))?;
    fx_key.set_raw_value(FX_VALUE_POST_MIX, &reg_value_string(EQUALIZER_APO_POST_MIX_INSTALL_GUID))?;
    fx_key.set_raw_value(FX_VALUE_INSTALL_BLOB1, &reg_value_binary(&INSTALL_BLOB1_BYTES))?;
    fx_key.set_raw_value(FX_VALUE_INSTALL_BLOB2, &reg_value_binary(&INSTALL_BLOB2_BYTES))?;
    fx_key.set_raw_value(FX_VALUE_INSTALL_DWORD, &reg_value_dword(0))?;
    let _ = fx_key.delete_value(FX_VALUE_SFX);
    let _ = fx_key.delete_value(FX_VALUE_MFX);
    let _ = fx_key.delete_value(FX_VALUE_EFX);
    Ok(())
}

fn restore_or_delete_fx_value(fx_key: &RegKey, child_key: &RegKey, value_name: &str) -> Result<()> {
    let backup = read_registry_string(child_key, &get_child_backup_value_name(value_name)).unwrap_or_else(|| "!VALUE".to_string());
    if backup.eq_ignore_ascii_case("!KEY") || backup.eq_ignore_ascii_case("!VALUE") || backup.trim().is_empty() {
        let _ = fx_key.delete_value(value_name);
        return Ok(());
    }
    fx_key.set_raw_value(value_name, &reg_value_string(&backup))?;
    Ok(())
}

fn disable_device_selector_state_for_endpoint(endpoint_guid: &str) -> Result<()> {
    let fx_key = open_endpoint_fx_key(endpoint_guid, true)?;
    let child_key = open_child_apo_key(endpoint_guid, true)?;
    for value_name in DEVICE_SELECTOR_BACKUP_VALUE_NAMES {
        restore_or_delete_fx_value(&fx_key, &child_key, value_name)?;
    }
    let _ = fx_key.delete_value(FX_VALUE_POST_MIX);
    let _ = fx_key.delete_value(FX_VALUE_INSTALL_BLOB1);
    let _ = fx_key.delete_value(FX_VALUE_INSTALL_BLOB2);
    let _ = fx_key.delete_value(FX_VALUE_INSTALL_DWORD);
    let hklm = open_hklm();
    let _ = hklm.delete_subkey_all(format!(r"{}\{}", CHILD_APO_REGISTRY_PATH, endpoint_guid));
    Ok(())
}

fn child_snapshot_value_names() -> Vec<String> {
    let mut names = vec![
        FX_VALUE_LFX.to_string(),
        FX_VALUE_GFX.to_string(),
        FX_VALUE_SFX.to_string(),
        FX_VALUE_MFX.to_string(),
        FX_VALUE_EFX.to_string(),
        PRE_MIX_CHILD_VALUE_NAME.to_string(),
        POST_MIX_CHILD_VALUE_NAME.to_string(),
        ALLOW_SILENT_BUFFER_VALUE_NAME.to_string(),
        VERSION_VALUE_NAME.to_string(),
        DISABLE_AUTO_ADJUST_VALUE_NAME.to_string(),
    ];
    names.extend(DEVICE_SELECTOR_BACKUP_VALUE_NAMES.iter().map(|name| get_child_backup_value_name(name)));
    names
}

fn capture_selector_snapshot(endpoint_guid: &str) -> SelectorSnapshot {
    SelectorSnapshot {
        fx: capture_registry_key_snapshot(
            &format!(r"{}\{}\FxProperties", RENDER_DEVICES_REGISTRY_PATH, endpoint_guid),
            &DEVICE_SELECTOR_MANAGED_VALUE_NAMES.iter().map(|name| (*name).to_string()).collect::<Vec<_>>(),
        ),
        child: capture_registry_key_snapshot(
            &format!(r"{}\{}", CHILD_APO_REGISTRY_PATH, endpoint_guid),
            &child_snapshot_value_names(),
        ),
    }
}

fn restore_selector_snapshot(endpoint_guid: &str, snapshot: &SelectorSnapshot) -> Result<()> {
    restore_registry_key_snapshot(
        &format!(r"{}\{}\FxProperties", RENDER_DEVICES_REGISTRY_PATH, endpoint_guid),
        &snapshot.fx,
        false,
    )?;
    restore_registry_key_snapshot(
        &format!(r"{}\{}", CHILD_APO_REGISTRY_PATH, endpoint_guid),
        &snapshot.child,
        true,
    )?;
    Ok(())
}

fn set_device_selector_state(endpoint_guid: &str, enabled: bool) -> bool {
    if !MANAGE_DEVICE_SELECTOR_ENABLED {
        log_line(&format!(
            "DEVICE SELECTOR bypass endpoint={} enabled={} managed=false",
            endpoint_guid, enabled
        ));
        return true;
    }

    let result = if enabled {
        enable_device_selector_state_for_endpoint(endpoint_guid)
    } else {
        disable_device_selector_state_for_endpoint(endpoint_guid)
    };
    if let Err(err) = result {
        log_line(&format!("DEVICE SELECTOR WARN ({endpoint_guid}): {err}"));
        false
    } else {
        true
    }
}

fn is_device_selector_enabled(endpoint_guid: &str) -> bool {
    if !MANAGE_DEVICE_SELECTOR_ENABLED {
        let _ = endpoint_guid;
        return false;
    }

    let Ok(fx_key) = open_endpoint_fx_key(endpoint_guid, false) else {
        return false;
    };
    let Ok(child_key) = open_child_apo_key(endpoint_guid, false) else {
        return false;
    };

    let lfx = read_registry_string(&fx_key, FX_VALUE_LFX).unwrap_or_default();
    let gfx = read_registry_string(&fx_key, FX_VALUE_GFX).unwrap_or_default();
    let post_mix = read_registry_string(&fx_key, FX_VALUE_POST_MIX).unwrap_or_default();
    if !lfx.eq_ignore_ascii_case(EQUALIZER_APO_PRE_MIX_GUID)
        || !gfx.eq_ignore_ascii_case(EQUALIZER_APO_POST_MIX_GUID)
        || !post_mix.eq_ignore_ascii_case(EQUALIZER_APO_POST_MIX_INSTALL_GUID)
    {
        return false;
    }
    if fx_key.get_raw_value(FX_VALUE_SFX).is_ok()
        || fx_key.get_raw_value(FX_VALUE_MFX).is_ok()
        || fx_key.get_raw_value(FX_VALUE_EFX).is_ok()
    {
        return false;
    }
    let blob1 = fx_key.get_raw_value(FX_VALUE_INSTALL_BLOB1).ok();
    let blob2 = fx_key.get_raw_value(FX_VALUE_INSTALL_BLOB2).ok();
    let dword = fx_key.get_raw_value(FX_VALUE_INSTALL_DWORD).ok();
    if blob1.as_ref().map(|v| v.bytes.as_slice()) != Some(INSTALL_BLOB1_BYTES.as_slice())
        || blob2.as_ref().map(|v| v.bytes.as_slice()) != Some(INSTALL_BLOB2_BYTES.as_slice())
    {
        return false;
    }
    let Some(dword) = dword else {
        return false;
    };
    if dword.bytes.len() < 4 || u32::from_le_bytes(dword.bytes[0..4].try_into().unwrap()) != 0 {
        return false;
    }

    let child_lfx = read_registry_string(&child_key, FX_VALUE_LFX).unwrap_or_default();
    let child_gfx = read_registry_string(&child_key, FX_VALUE_GFX).unwrap_or_default();
    let child_sfx = read_registry_string(&child_key, FX_VALUE_SFX).unwrap_or_default();
    let child_mfx = read_registry_string(&child_key, FX_VALUE_MFX).unwrap_or_default();
    let pre_mix_child = read_registry_string(&child_key, PRE_MIX_CHILD_VALUE_NAME).unwrap_or_default();
    let post_mix_child = read_registry_string(&child_key, POST_MIX_CHILD_VALUE_NAME).unwrap_or_default();
    let allow_silent = read_registry_string(&child_key, ALLOW_SILENT_BUFFER_VALUE_NAME).unwrap_or_else(|| "false".to_string());
    let auto_adjust_enabled = child_key.get_raw_value(DISABLE_AUTO_ADJUST_VALUE_NAME).is_err();

    child_lfx.eq_ignore_ascii_case(EQUALIZER_APO_CHILD_APO_GUID)
        && child_gfx.eq_ignore_ascii_case(EQUALIZER_APO_CHILD_PROC_GUID)
        && child_sfx.eq_ignore_ascii_case(EQUALIZER_APO_CHILD_APO_GUID)
        && child_mfx.eq_ignore_ascii_case(EQUALIZER_APO_CHILD_PROC_GUID)
        && pre_mix_child.eq_ignore_ascii_case(EQUALIZER_APO_CHILD_APO_GUID)
        && post_mix_child.eq_ignore_ascii_case(EQUALIZER_APO_CHILD_PROC_GUID)
        && allow_silent.eq_ignore_ascii_case("false")
        && auto_adjust_enabled
}

fn capture_transaction_snapshot(config_dir: &Path, endpoint_guid: Option<&str>) -> TransactionSnapshot {
    let materialized_slot = load_materialized_slot();
    TransactionSnapshot {
        active_profile: load_active_profile(),
        config_dir: config_dir.to_path_buf(),
        target_endpoint_guid: endpoint_guid.map(ToOwned::to_owned),
        materialized_revision: load_registry_string_value(MATERIALIZED_REVISION_VALUE_NAME),
        last_confirmed_revision: load_last_confirmed_revision(),
        materialized_slot: materialized_slot.clone(),
        config: capture_file_snapshot(&config_dir.join("config.txt")),
        legacy_active: capture_file_snapshot(&config_dir.join(LEGACY_ACTIVE_FILE_NAME)),
        legacy_aux: capture_file_snapshot(&config_dir.join(LEGACY_AUX_FILE_NAME)),
        active_a: capture_file_snapshot(&materialized_active_path_for_slot(config_dir, "a")),
        aux_a: capture_file_snapshot(&materialized_aux_path_for_slot(config_dir, "a")),
        active_b: capture_file_snapshot(&materialized_active_path_for_slot(config_dir, "b")),
        aux_b: capture_file_snapshot(&materialized_aux_path_for_slot(config_dir, "b")),
        selector: endpoint_guid.map(capture_selector_snapshot),
    }
}

fn restore_transaction_snapshot(snapshot: &TransactionSnapshot) -> Result<()> {
    ensure_writable(&snapshot.config_dir)?;
    restore_file_snapshot(&snapshot.config_dir.join("config.txt"), &snapshot.config)?;
    restore_file_snapshot(
        &snapshot.config_dir.join(LEGACY_ACTIVE_FILE_NAME),
        &snapshot.legacy_active,
    )?;
    restore_file_snapshot(
        &snapshot.config_dir.join(LEGACY_AUX_FILE_NAME),
        &snapshot.legacy_aux,
    )?;
    restore_file_snapshot(
        &materialized_active_path_for_slot(&snapshot.config_dir, "a"),
        &snapshot.active_a,
    )?;
    restore_file_snapshot(
        &materialized_aux_path_for_slot(&snapshot.config_dir, "a"),
        &snapshot.aux_a,
    )?;
    restore_file_snapshot(
        &materialized_active_path_for_slot(&snapshot.config_dir, "b"),
        &snapshot.active_b,
    )?;
    restore_file_snapshot(
        &materialized_aux_path_for_slot(&snapshot.config_dir, "b"),
        &snapshot.aux_b,
    )?;
    if let (Some(endpoint_guid), Some(selector)) = (&snapshot.target_endpoint_guid, &snapshot.selector) {
        restore_selector_snapshot(endpoint_guid, selector)?;
    }
    save_registry_dword("ActiveProfile", snapshot.active_profile)?;
    if let Some(revision) = &snapshot.materialized_revision {
        save_registry_string(MATERIALIZED_REVISION_VALUE_NAME, revision)?;
    } else {
        delete_registry_value(MATERIALIZED_REVISION_VALUE_NAME);
    }
    if let Some(revision) = &snapshot.last_confirmed_revision {
        save_registry_string(LAST_CONFIRMED_REVISION_VALUE_NAME, revision)?;
    } else {
        clear_last_confirmed_revision();
    }
    save_registry_string(MATERIALIZED_SLOT_VALUE_NAME, &snapshot.materialized_slot)?;
    harden_materialized_payload_acl(&snapshot.config_dir)?;
    rehide_config_dir(&snapshot.config_dir);
    Ok(())
}

fn require_profile(profiles: &HashMap<u32, ProfilePayload>, id: u32) -> Result<ProfilePayload> {
    profiles
        .get(&id)
        .cloned()
        .ok_or_else(|| anyhow!("Profile not found: {id}"))
}

fn write_helper_state_lines(endpoint: Option<&EndpointInfo>, selector_active: bool) {
    println!("HELPER_VERSION={HELPER_VERSION}");
    println!("HELPER_PATH={}", helper_path());
    println!(
        "TARGET_ENDPOINT_GUID={}",
        endpoint.map(|ep| ep.guid.as_str()).unwrap_or("")
    );
    println!(
        "TARGET_ENDPOINT_NAME={}",
        endpoint.map(|ep| ep.name.as_str()).unwrap_or("")
    );
    println!(
        "DEVICE_SELECTOR_ACTIVE={}",
        if selector_active { "true" } else { "false" }
    );
}

fn validate_materialized_profile(
    profile: &ProfilePayload,
    config_dir: &Path,
    endpoint_guid: &str,
    materialized_slot: &str,
) -> Option<String> {
    if endpoint_guid.trim().is_empty() {
        return Some(
            "HiFiEndpointGuid is not configured. Run repair-device-selector or reinstall."
                .to_string(),
        );
    }
    if !is_device_selector_enabled(endpoint_guid) {
        log_line(&format!(
            "VALIDATE WARN: Device selector not active for {} — non-fatal, config.txt path is sufficient",
            endpoint_guid
        ));
    }
    let config_path = config_dir.join("config.txt");
    let normalized_slot = normalize_materialized_slot(materialized_slot).to_string();
    let active_path = materialized_active_path_for_slot(config_dir, &normalized_slot);
    let aux_path = materialized_aux_path_for_slot(config_dir, &normalized_slot);
    let needs_unlock = needs_materialized_payload_read_unlock(&active_path);
    if needs_unlock {
        if let Err(error) = ensure_writable(config_dir) {
            return Some(format!(
                "Materialized payload ACL could not be reopened for verification: {error}"
            ));
        }
    }

    let result = (|| {
        if !config_path.is_file() || !active_path.is_file() {
            return Some(format!(
                "Active config files are missing. slot={} config={} active={}",
                normalized_slot,
                describe_disk_file(&config_path),
                describe_disk_file(&active_path)
            ));
        }
        let legacy_active_path = config_dir.join(LEGACY_ACTIVE_FILE_NAME);
        let legacy_aux_path = config_dir.join(LEGACY_AUX_FILE_NAME);
        if legacy_active_path.is_file() || legacy_aux_path.is_file() {
            return Some(format!(
                "Legacy visible config artifacts are still present. active={} aux={}",
                describe_disk_file(&legacy_active_path),
                describe_disk_file(&legacy_aux_path)
            ));
        }
        let expected_main_include = render_main_config_for_slot(&normalized_slot);
        let expected_main_inline = render_inline_main_config(profile);
        let expected_active = render_profile_config(profile);
        let actual_main = fs::read_to_string(&config_path).unwrap_or_default();
        let actual_active = fs::read_to_string(&active_path).unwrap_or_default();
        let main_ok = config_texts_match(&expected_main_include, &actual_main)
            || config_texts_match(&expected_main_inline, &actual_main);
        if !main_ok || !config_texts_match(&expected_active, &actual_active)
        {
            return Some(format!(
                "Active profile materialization does not match bundle. config.txt {{{}}} {} {{{}}}",
                build_content_fingerprint(&expected_main_inline, &actual_main),
                active_path.display(),
                build_content_fingerprint(&expected_active, &actual_active)
            ));
        }
        if aux_path.is_file() {
            return Some(format!(
                "Unexpected concealed strategy payload for active profile. aux={}",
                describe_disk_file(&aux_path)
            ));
        }
        None
    })();

    if needs_unlock {
        if let Err(error) = harden_materialized_payload_acl(config_dir) {
            log_line(&format!("VERIFY WARN payload ACL reharden failed: {error}"));
        }
        rehide_config_dir(config_dir);
    }

    result
}

fn confirm_materialized_profile(
    profile: &ProfilePayload,
    config_dir: &Path,
    endpoint_guid: &str,
    materialized_slot: &str,
) -> Result<String> {
    let normalized_slot = normalize_materialized_slot(materialized_slot).to_string();
    ensure_writable(config_dir)?;
    let mut last_error = None;
    let result = (|| {
        for _ in 0..4 {
            if let Some(error) =
                validate_materialized_profile(profile, config_dir, endpoint_guid, &normalized_slot)
            {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(120));
                continue;
            }

            let active_revision = load_materialized_revision();
            save_last_confirmed_revision(&active_revision)?;
            return Ok(active_revision);
        }
        bail!(last_error.unwrap_or_else(|| "Materialized profile confirmation failed.".to_string()))
    })();

    harden_materialized_payload_acl(config_dir)?;
    rehide_config_dir(config_dir);
    result
}

fn command_pack(profiles_dir: &Path, output_file: &Path) -> Result<i32> {
    if !profiles_dir.is_dir() {
        bail!("Profiles directory not found: {}", profiles_dir.display());
    }
    let names = profile_names();
    let mut profiles = HashMap::new();
    for (id, name) in names {
        let dir = profiles_dir.join(id.to_string());
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
                config: fs::read_to_string(&config_path)?,
                strategy: fs::read_to_string(&strategy_path).unwrap_or_default(),
            },
        );
    }
    let packed = pack_profiles(&profiles);
    let encrypted = encrypt(&packed)?;
    fs::write(output_file, encrypted)?;
    println!("RESULT=OK");
    println!("BUNDLE_PATH={}", output_file.display());
    println!("PROFILE_COUNT={}", profiles.len());
    println!("BUNDLE_SHA256={}", compute_file_sha256(output_file)?);
    log_line(&format!("PACK -> {}", output_file.display()));
    Ok(0)
}

fn command_deploy(bundle_file: &Path) -> Result<i32> {
    if !bundle_file.is_file() {
        bail!("Bundle not found: {}", bundle_file.display());
    }
    let profiles = load_bundle(bundle_file)?;
    let data_dir = common_data_root();
    let deployed_bundle = default_bundle_path();
    let deployed_root = deployed_bundle
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(user_data_root);
    let staged_bundle = deployed_root.join("profiles.next.bin");
    log_line(&format!(
        "DEPLOY prepare bundle={} dataDir={} targetBundle={} legacyBundle={}",
        bundle_file.display(),
        data_dir.display(),
        deployed_bundle.display(),
        legacy_bundle_path().display()
    ));
    fs::create_dir_all(&data_dir)
        .with_context(|| format!("Unable to create data dir {}.", data_dir.display()))?;
    log_line(&format!("DEPLOY dataDir ready {}", data_dir.display()));
    fs::create_dir_all(&deployed_root)
        .with_context(|| format!("Unable to create bundle dir {}.", deployed_root.display()))?;
    log_line(&format!("DEPLOY bundleDir ready {}", deployed_root.display()));
    if staged_bundle.exists() {
        set_path_attributes(&staged_bundle, FILE_ATTRIBUTE_NORMAL.0);
        let _ = fs::remove_file(&staged_bundle);
    }
    fs::copy(bundle_file, &staged_bundle).with_context(|| {
        format!(
            "Unable to copy bundle {} -> {}.",
            bundle_file.display(),
            staged_bundle.display()
        )
    })?;
    log_line(&format!("DEPLOY bundle staged {}", staged_bundle.display()));
    if deployed_bundle.exists() {
        set_path_attributes(&deployed_bundle, FILE_ATTRIBUTE_NORMAL.0);
        let _ = fs::remove_file(&deployed_bundle);
    }
    fs::rename(&staged_bundle, &deployed_bundle).or_else(|rename_error| {
        fs::copy(&staged_bundle, &deployed_bundle).with_context(|| {
            format!(
                "Unable to finalize bundle {} -> {} after rename failed: {rename_error}",
                staged_bundle.display(),
                deployed_bundle.display()
            )
        })?;
        let _ = fs::remove_file(&staged_bundle);
        Ok::<(), anyhow::Error>(())
    })?;
    log_line(&format!("DEPLOY bundle copied {}", deployed_bundle.display()));
    let config_dir = resolve_config_dir();
    let config_dir_string = config_dir.display().to_string();
    log_line(&format!(
        "DEPLOY state configDir={} registry=HKCU(primary)+HKLM(mirror)",
        config_dir_string
    ));
    save_registry_string("BundlePath", &deployed_bundle.display().to_string())
        .context("Unable to persist BundlePath.")?;
    save_registry_string("ConfigDir", &config_dir_string)
        .context("Unable to persist ConfigDir.")?;
    let bundle_sha256 = compute_file_sha256(&deployed_bundle)?;
    log_line(&format!("DEPLOY state bundleSha256={bundle_sha256}"));
    save_registry_string("BundleSha256", &bundle_sha256)
        .context("Unable to persist BundleSha256.")?;
    if !use_embedded_engine() {
        log_line("DEPLOY state engineVersion=control-plane-v1");
        save_registry_string("EngineVersion", "control-plane-v1")
            .context("Unable to persist EngineVersion.")?;
    }
    let engine_path = resolve_engine_root().display().to_string();
    log_line(&format!("DEPLOY state enginePath={engine_path}"));
    save_registry_string("EnginePath", &engine_path).context("Unable to persist EnginePath.")?;
    if !profiles.contains_key(&1) {
        bail!("Bundle missing profile 1.");
    }
    let requested_profile = load_active_profile();
    let deploy_profile = if profiles.contains_key(&requested_profile) {
        requested_profile
    } else {
        1
    };
    if deploy_profile != requested_profile {
        log_line(&format!(
            "DEPLOY fallback requestedProfile={} resolvedProfile={} bundle={}",
            requested_profile,
            deploy_profile,
            bundle_file.display()
        ));
    } else {
        log_line(&format!(
            "DEPLOY start requestedProfile={} bundle={}",
            requested_profile,
            bundle_file.display()
        ));
    }
    let mut result = command_switch_internal(deploy_profile)?;
    if result != 0 && deploy_profile != 1 {
        log_line(&format!(
            "DEPLOY retry requestedProfile={} resolvedProfile=1 previousExit={}",
            requested_profile,
            result
        ));
        result = command_switch_internal(1)?;
    }
    let endpoint = load_stored_target_endpoint();
    let selector_active = endpoint
        .as_ref()
        .map(|ep| is_device_selector_enabled(&ep.guid))
        .unwrap_or(false);
    println!("RESULT={}", if result == 0 { "OK" } else { "ERROR" });
    println!("BUNDLE_PATH={}", bundle_path().display());
    println!("CONFIG_DIR={}", config_dir.display());
    write_helper_state_lines(endpoint.as_ref(), selector_active);
    log_line(&format!("DEPLOY -> {}", bundle_path().display()));
    Ok(result)
}

fn command_repair_device_selector() -> Result<i32> {
    if !is_elevated() {
        bail!("repair-device-selector requires elevation.");
    }
    let endpoint = detect_best_hifi_render_endpoint()
        .ok_or_else(|| anyhow!("No Hi-Fi Cable render endpoint could be detected."))?;
    let config_dir = resolve_config_dir();
    save_target_endpoint(&endpoint)?;
    let should_enable = true;
    let selector_applied = set_device_selector_state(&endpoint.guid, should_enable);
    let selector_active = is_device_selector_enabled(&endpoint.guid);
    if !selector_applied || !selector_active {
        bail!(
            "Equalizer APO registration is not active on target endpoint {}.",
            endpoint.guid
        );
    }
    log_runtime_snapshot_line("REPAIR DEVICE SELECTOR after refresh", &config_dir, Some(&endpoint));
    println!("RESULT=OK");
    println!("ACTIVE_PROFILE={}", load_active_profile());
    println!(
        "CONFIG_MODE={}",
        if use_embedded_engine() {
            "embedded"
        } else if load_active_profile() == 0 {
            "cleared"
        } else {
            "materialized"
        }
    );
    println!(
        "REPAIRED={}",
        if selector_applied && (!should_enable || selector_active) {
            "true"
        } else {
            "false"
        }
    );
    write_helper_state_lines(Some(&endpoint), selector_active);
    log_line(&format!(
        "REPAIR DEVICE SELECTOR -> {} | name={} | enabled={} | applied={} | active={}",
        endpoint.guid, endpoint.name, should_enable, selector_applied, selector_active
    ));
    Ok(0)
}

fn command_switch_internal(profile_id: u32) -> Result<i32> {
    let names = profile_names();
    if !names.contains_key(&profile_id) {
        bail!("Profile must be 1-4.");
    }
    if !bundle_available() {
        bail!("Bundle not deployed at {}", bundle_path().display());
    }

    let profiles = load_bundle(&bundle_path())?;
    let profile = require_profile(&profiles, profile_id)?;
    let config_dir = resolve_config_dir();
    let endpoint = if use_embedded_engine() {
        load_stored_target_endpoint()
    } else {
        Some(require_stored_target_endpoint()?)
    };
    let snapshot = capture_transaction_snapshot(&config_dir, endpoint.as_ref().map(|ep| ep.guid.as_str()));
    log_line(&format!("SWITCH start profileId={profile_id}"));
    log_runtime_snapshot_line("SWITCH before materialize", &config_dir, endpoint.as_ref());

    let result: Result<()> = (|| {
        let previous_profile = snapshot.active_profile;
        ensure_writable(&config_dir)?;
        let current_slot = load_materialized_slot();
        let target_slot = alternate_materialized_slot(&current_slot).to_string();
        if !use_embedded_engine() {
            if should_force_detach(previous_profile, profile_id) {
                log_line(&format!(
                    "SWITCH preclear previousProfile={} targetProfile={} slot={}",
                    previous_profile, profile_id, current_slot
                ));
                save_registry_dword("ActiveProfile", 0)?;
                save_registry_string(MATERIALIZED_REVISION_VALUE_NAME, &generate_materialized_revision())?;
                clear_materialized_payload(&config_dir);
                let config_path = config_dir.join("config.txt");
                write_utf8_config_file(&config_path, &render_cleared_config())?;
                log_line(&format!("SWITCH preclear wrote config {}", describe_disk_file(&config_path)));
                signal_profile_changed();
                thread::sleep(Duration::from_millis(320));
            }

            save_registry_dword("ActiveProfile", profile_id)?;
            clear_last_confirmed_revision();
            save_registry_string(MATERIALIZED_REVISION_VALUE_NAME, &generate_materialized_revision())?;
            let _ = ensure_materialized_payload_dir(&config_dir)?;
            purge_legacy_materialized_files(&config_dir);
            let active_path = materialized_active_path_for_slot(&config_dir, &target_slot);
            write_utf8_config_file(
                &active_path,
                &render_profile_config(&profile),
            )?;
            log_line(&format!("SWITCH wrote active {}", describe_disk_file(&active_path)));
            for aux_path in [
                materialized_aux_path_for_slot(&config_dir, "a"),
                materialized_aux_path_for_slot(&config_dir, "b"),
            ] {
                let _ = fs::remove_file(&aux_path);
            }
            save_registry_string(MATERIALIZED_SLOT_VALUE_NAME, &target_slot)?;
            let config_path = config_dir.join("config.txt");
            write_utf8_config_file(&config_path, &render_inline_main_config(&profile))?;
            log_line(&format!("SWITCH wrote config {}", describe_disk_file(&config_path)));
        } else {
            save_registry_dword("ActiveProfile", profile_id)?;
            clear_last_confirmed_revision();
            save_registry_string(MATERIALIZED_REVISION_VALUE_NAME, &generate_materialized_revision())?;
            clear_materialized_payload(&config_dir);
            write_utf8_config_file(&config_dir.join("config.txt"), &render_embedded_decoy_config())?;
        }

        let mut selector_active = endpoint
            .as_ref()
            .map(|ep| is_device_selector_enabled(&ep.guid))
            .unwrap_or(false);
        if !use_embedded_engine() {
            let endpoint = endpoint.as_ref().expect("endpoint required");
            if !set_device_selector_state(&endpoint.guid, true) {
                log_line(&format!(
                    "DEVICE SELECTOR enable failed (non-fatal) for {} — continuing with config.txt path",
                    endpoint.guid
                ));
            }
            selector_active = is_device_selector_enabled(&endpoint.guid);
            if !selector_active {
                log_line(&format!(
                    "DEVICE SELECTOR not active (non-fatal) for {} — EqualizerAPO will process via config.txt",
                    endpoint.guid
                ));
            }
            if let Some(error) =
                validate_materialized_profile(&profile, &config_dir, &endpoint.guid, &target_slot)
            {
                bail!(error);
            }
        }

        harden_materialized_payload_acl(&config_dir)?;
        rehide_config_dir(&config_dir);
        signal_profile_changed();
        thread::sleep(Duration::from_millis(320));

        let mut confirmation_error = None;
        if !use_embedded_engine() {
            let endpoint = endpoint.as_ref().expect("endpoint required");
            if let Err(error) =
                confirm_materialized_profile(&profile, &config_dir, &endpoint.guid, &target_slot)
            {
                log_line(&format!(
                    "SWITCH confirmation failed previousProfile={} targetProfile={} reason={}",
                    previous_profile, profile_id, error
                ));
                confirmation_error = Some(error.to_string());
            }
        } else {
            save_last_confirmed_revision(&load_materialized_revision())?;
        }

        if !use_embedded_engine() && (confirmation_error.is_some() || should_force_recommit(previous_profile, profile_id)) {
            ensure_writable(&config_dir)?;
            save_registry_string(MATERIALIZED_REVISION_VALUE_NAME, &generate_materialized_revision())?;
            let config_path = config_dir.join("config.txt");
            write_utf8_config_file(&config_path, &render_inline_main_config(&profile))?;
            log_line(&format!(
                "SWITCH recommit previousProfile={} targetProfile={} config={}",
                previous_profile,
                profile_id,
                describe_disk_file(&config_path)
            ));
            harden_materialized_payload_acl(&config_dir)?;
            rehide_config_dir(&config_dir);
            signal_profile_changed();
            thread::sleep(Duration::from_millis(420));

            let endpoint = endpoint.as_ref().expect("endpoint required");
            if let Err(error) =
                confirm_materialized_profile(&profile, &config_dir, &endpoint.guid, &target_slot)
            {
                bail!(
                    "Profile confirmation failed after retry. first_error={} second_error={}",
                    confirmation_error.unwrap_or_else(|| "<none>".to_string()),
                    error
                );
            }
        } else if let Some(error) = confirmation_error {
            bail!("Profile confirmation failed: {error}");
        }

        log_runtime_snapshot_line("SWITCH after materialize", &config_dir, endpoint.as_ref());
        println!("RESULT=OK");
        println!("ACTIVE_PROFILE={profile_id}");
        println!("PROFILE_NAME={}", profile.name);
        println!("CONFIG_DIR={}", config_dir.display());
        println!("CONFIG_MODE={}", if use_embedded_engine() { "embedded" } else { "materialized" });
        write_helper_state_lines(endpoint.as_ref(), selector_active);
        log_line(&format!(
            "SWITCH -> {} | endpoint={} | helper={}",
            profile_id,
            endpoint.as_ref().map(|ep| ep.guid.as_str()).unwrap_or(""),
            helper_path()
        ));
        Ok(())
    })();

    if let Err(err) = result {
        let rollback_error = restore_transaction_snapshot(&snapshot).err();
        if let Some(ref rollback_error) = rollback_error {
            log_line(&format!(
                "ROLLBACK SWITCH RESTORE ERROR -> {} | endpoint={} | restore_reason={}",
                profile_id,
                endpoint.as_ref().map(|ep| ep.guid.as_str()).unwrap_or(""),
                rollback_error
            ));
        }
        log_line(&format!(
            "ROLLBACK SWITCH -> {} | endpoint={} | reason={}{}",
            profile_id,
            endpoint.as_ref().map(|ep| ep.guid.as_str()).unwrap_or(""),
            err,
            rollback_error
                .as_ref()
                .map(|rollback_error| format!(" | rollback={rollback_error}"))
                .unwrap_or_default()
        ));
        eprintln!("ERROR: {err}");
        if let Some(rollback_error) = rollback_error {
            eprintln!("ROLLBACK_ERROR: {rollback_error}");
        }
        return Ok(2);
    }
    Ok(0)
}

fn command_clear_internal() -> Result<i32> {
    if !bundle_available() {
        bail!("Bundle not deployed at {}", bundle_path().display());
    }
    let config_dir = resolve_config_dir();
    let endpoint = load_stored_target_endpoint();
    let snapshot = capture_transaction_snapshot(&config_dir, endpoint.as_ref().map(|ep| ep.guid.as_str()));
    log_line("CLEAR start");
    log_runtime_snapshot_line("CLEAR before", &config_dir, endpoint.as_ref());
    let result: Result<()> = (|| {
        save_registry_dword("ActiveProfile", 0)?;
        clear_last_confirmed_revision();
        save_registry_string(MATERIALIZED_REVISION_VALUE_NAME, &generate_materialized_revision())?;
        ensure_writable(&config_dir)?;
        clear_materialized_payload(&config_dir);
        let config_path = config_dir.join("config.txt");
        write_utf8_config_file(&config_path, &render_cleared_config())?;
        log_line(&format!("CLEAR wrote config {}", describe_disk_file(&config_path)));
        let selector_active = endpoint
            .as_ref()
            .map(|ep| is_device_selector_enabled(&ep.guid))
            .unwrap_or(false);
        harden_materialized_payload_acl(&config_dir)?;
        rehide_config_dir(&config_dir);
        signal_profile_changed();
        thread::sleep(Duration::from_millis(250));
        save_last_confirmed_revision(&load_materialized_revision())?;
        log_runtime_snapshot_line("CLEAR after", &config_dir, endpoint.as_ref());
        println!("RESULT=OK");
        println!("ACTIVE_PROFILE=0");
        println!("PROFILE_NAME=Cleared");
        println!("CONFIG_DIR={}", config_dir.display());
        println!(
            "CONFIG_MODE={}",
            if use_embedded_engine() {
                "embedded-cleared"
            } else {
                "cleared"
            }
        );
        write_helper_state_lines(endpoint.as_ref(), selector_active);
        log_line(&format!(
            "CLEAR | endpoint={} | helper={}",
            endpoint.as_ref().map(|ep| ep.guid.as_str()).unwrap_or(""),
            helper_path()
        ));
        Ok(())
    })();
    if let Err(err) = result {
        let rollback_error = restore_transaction_snapshot(&snapshot).err();
        if let Some(ref rollback_error) = rollback_error {
            log_line(&format!(
                "ROLLBACK CLEAR RESTORE ERROR | endpoint={} | restore_reason={}",
                endpoint.as_ref().map(|ep| ep.guid.as_str()).unwrap_or(""),
                rollback_error
            ));
        }
        log_line(&format!(
            "ROLLBACK CLEAR | endpoint={} | reason={}{}",
            endpoint.as_ref().map(|ep| ep.guid.as_str()).unwrap_or(""),
            err,
            rollback_error
                .as_ref()
                .map(|rollback_error| format!(" | rollback={rollback_error}"))
                .unwrap_or_default()
        ));
        eprintln!("ERROR: {err}");
        if let Some(rollback_error) = rollback_error {
            eprintln!("ROLLBACK_ERROR: {rollback_error}");
        }
        return Ok(2);
    }
    Ok(0)
}

fn command_status_internal() -> Result<i32> {
    let active_profile = load_active_profile();
    let config_dir = resolve_config_dir();
    let endpoint = load_stored_target_endpoint();
    let selector_active = endpoint
        .as_ref()
        .map(|ep| is_device_selector_enabled(&ep.guid))
        .unwrap_or(false);
    log_line("STATUS start");
    log_runtime_snapshot_line("STATUS", &config_dir, endpoint.as_ref());
    println!("RESULT=OK");
    println!("ACTIVE_PROFILE={active_profile}");
    println!("CONFIG_DIR={}", config_dir.display());
    println!("ENGINE_DIR={}", resolve_engine_root().display());
    println!("BUNDLE_PATH={}", bundle_path().display());
    println!("BUNDLE_PRESENT={}", if bundle_available() { "true" } else { "false" });
    println!(
        "CONFIG_MODE={}",
        if use_embedded_engine() {
            if active_profile == 0 {
                "embedded-cleared"
            } else {
                "embedded"
            }
        } else if active_profile == 0 {
            "cleared"
        } else {
            "materialized"
        }
    );
    write_helper_state_lines(endpoint.as_ref(), selector_active);
    Ok(0)
}

fn command_verify_internal() -> Result<i32> {
    if !bundle_available() {
        bail!("Bundle not deployed.");
    }
    let active_profile = load_active_profile();
    let profiles = load_bundle(&bundle_path())?;
    let config_dir = resolve_config_dir();
    let endpoint = load_stored_target_endpoint();
    let mut selector_active = endpoint
        .as_ref()
        .map(|ep| is_device_selector_enabled(&ep.guid))
        .unwrap_or(false);
    log_line("VERIFY start");
    log_runtime_snapshot_line("VERIFY before", &config_dir, endpoint.as_ref());

    if active_profile == 0 {
        let config_path = config_dir.join("config.txt");
        let legacy_active_path = config_dir.join(LEGACY_ACTIVE_FILE_NAME);
        let legacy_aux_path = config_dir.join(LEGACY_AUX_FILE_NAME);
        let active_path = materialized_active_path(&config_dir);
        let aux_path = materialized_aux_path(&config_dir);
        if !config_path.is_file() {
            log_line("VERIFY cleared mismatch: config.txt missing");
            eprintln!("ERROR: Cleared config.txt is missing.");
            return Ok(1);
        }
        if legacy_active_path.is_file()
            || legacy_aux_path.is_file()
            || active_path.is_file()
            || aux_path.is_file()
        {
            log_line("VERIFY cleared mismatch: active artifacts still exist");
            eprintln!("ERROR: Active profile artifacts still exist after clear.");
            return Ok(2);
        }
        let actual = fs::read_to_string(config_path).unwrap_or_default();
        let expected = render_cleared_config();
        if !config_texts_match(&expected, &actual) {
            let fingerprint = build_content_fingerprint(&expected, &actual);
            log_line(&format!(
                "VERIFY cleared mismatch fingerprint={{{}}}",
                fingerprint
            ));
            eprintln!(
                "ERROR: Cleared config does not match expected decoy. {{{}}}",
                fingerprint
            );
            return Ok(2);
        }
        println!("RESULT=OK");
        println!("ACTIVE_PROFILE=0");
        println!("VERIFY=cleared");
        println!(
            "CONFIG_MODE={}",
            if use_embedded_engine() {
                "embedded-cleared"
            } else {
                "cleared"
            }
        );
        write_helper_state_lines(endpoint.as_ref(), selector_active);
        log_runtime_snapshot_line("VERIFY cleared ok", &config_dir, endpoint.as_ref());
        return Ok(0);
    }

    let profile = require_profile(&profiles, active_profile)?;
    if !use_embedded_engine() {
        let Some(endpoint) = endpoint.as_ref() else {
            eprintln!("ERROR: HiFiEndpointGuid is not configured.");
            return Ok(2);
        };
        if let Some(error) = validate_materialized_profile(
            &profile,
            &config_dir,
            &endpoint.guid,
            &load_materialized_slot(),
        ) {
            log_line(&format!(
                "VERIFY materialized mismatch profile={} error={}",
                active_profile, error
            ));
            eprintln!("ERROR: {error}");
            return Ok(2);
        }
        selector_active = is_device_selector_enabled(&endpoint.guid);
    }

    println!("RESULT=OK");
    println!("ACTIVE_PROFILE={active_profile}");
    println!("VERIFY=matched");
    println!("CONFIG_MODE={}", if use_embedded_engine() { "embedded" } else { "materialized" });
    write_helper_state_lines(endpoint.as_ref(), selector_active);
    log_runtime_snapshot_line("VERIFY matched ok", &config_dir, endpoint.as_ref());
    Ok(0)
}

fn is_elevated() -> bool {
    unsafe {
        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION::default();
        let mut size = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok && elevation.TokenIsElevated != 0
    }
}
