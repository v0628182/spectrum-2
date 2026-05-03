//! Embedded PowerShell scripts — compiled into the binary, never shipped as files.
//!
//! At install time, these are written to a temp directory, executed, and deleted.
//! This protects the installation logic from being visible to end users.

use std::fs;
use std::path::{Path, PathBuf};

/// All embedded scripts that make up the installer suite.
struct EmbeddedScript {
    filename: &'static str,
    content: &'static str,
}

const SCRIPTS: &[EmbeddedScript] = &[
    EmbeddedScript {
        filename: "install_echosuite.ps1",
        content: include_str!("../../instalacion/install_echosuite.ps1"),
    },
    EmbeddedScript {
        filename: "install_equalizerapo.ps1",
        content: include_str!("../../instalacion/install_equalizerapo.ps1"),
    },
    EmbeddedScript {
        filename: "install_hificable.ps1",
        content: include_str!("../../instalacion/install_hificable.ps1"),
    },
    EmbeddedScript {
        filename: "enable_loudness.ps1",
        content: include_str!("../../instalacion/enable_loudness.ps1"),
    },
    EmbeddedScript {
        filename: "set_hifi_device_selector.ps1",
        content: include_str!("../../instalacion/set_hifi_device_selector.ps1"),
    },
    EmbeddedScript {
        filename: "apply_echoplus_now.ps1",
        content: include_str!("../../instalacion/apply_echoplus_now.ps1"),
    },
];

/// Extracts all embedded scripts to a temporary working directory.
/// Returns the path to the working dir containing scripts + symlinks to resources.
///
/// The caller MUST call `cleanup_embedded_scripts()` after execution.
pub fn extract_embedded_scripts(resource_dir: &Path) -> anyhow::Result<PathBuf> {
    let work_dir = std::env::temp_dir().join("vanysound_install_runtime");

    // Clean any stale previous run
    if work_dir.exists() {
        let _ = fs::remove_dir_all(&work_dir);
    }
    fs::create_dir_all(&work_dir)?;

    // Write all embedded scripts
    for script in SCRIPTS {
        let target = work_dir.join(script.filename);
        fs::write(&target, script.content)?;
        tracing::info!(
            script = script.filename,
            path = %target.display(),
            "extracted embedded script"
        );
    }

    // Link binary resources from the app's resource directory into the work dir.
    // The PS scripts use $scriptDir to find these files, so they need to be
    // in the same directory (or a subdirectory) as the scripts.
    link_resource_tree(resource_dir, &work_dir)?;

    Ok(work_dir)
}

/// Removes the temporary script directory after installation.
pub fn cleanup_embedded_scripts() {
    let work_dir = std::env::temp_dir().join("vanysound_install_runtime");
    if work_dir.exists() {
        // Only delete the .ps1 scripts, keep other files intact
        // (in case external installers left logs there)
        for entry in fs::read_dir(&work_dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("ps1") {
                let _ = fs::remove_file(&path);
            }
        }
        // Try to remove the dir (will fail if non-empty, which is fine)
        let _ = fs::remove_dir_all(&work_dir);
    }
}

/// Returns the path to the master installer script in the work directory.
pub fn master_script_path() -> PathBuf {
    std::env::temp_dir()
        .join("vanysound_install_runtime")
        .join("install_echosuite.ps1")
}

/// Copies or hardlinks binary resources from the app's resource dir into the work dir.
/// Handles subdirectories (driver/, equalizerapo/).
fn link_resource_tree(source: &Path, dest: &Path) -> anyhow::Result<()> {
    if !source.exists() {
        tracing::warn!(
            source = %source.display(),
            "resource directory does not exist, skipping resource linking"
        );
        return Ok(());
    }

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip .ps1 files — we have our own embedded versions
        if name_str.ends_with(".ps1") {
            continue;
        }
        // Skip .exe helper files that are now embedded in the native app
        if name_str.eq_ignore_ascii_case("VanySoundControl.exe") {
            continue;
        }
        // NOTE: EchoTools.exe is NOT skipped — it's needed for registry ACL adjustment
        // Skip build-only artifacts
        if name_str.eq_ignore_ascii_case("nsis")
            || name_str.ends_with(".log")
            || name_str.ends_with(".nsi")
        {
            continue;
        }

        let dest_path = dest.join(&name);

        if file_type.is_dir() {
            fs::create_dir_all(&dest_path)?;
            link_resource_tree(&entry.path(), &dest_path)?;
        } else {
            // Skip if dest already exists (e.g., from embedded scripts)
            if dest_path.exists() {
                continue;
            }
            // Try hardlink first (instant, no disk space), fall back to copy
            match fs::hard_link(entry.path(), &dest_path) {
                Ok(_) => {}
                Err(_) => {
                    if let Err(copy_err) = fs::copy(entry.path(), &dest_path) {
                        tracing::warn!(
                            resource = %name_str,
                            error = %copy_err,
                            "failed to link/copy resource (non-fatal)"
                        );
                        continue;
                    }
                }
            }
            tracing::debug!(
                resource = %name_str,
                "linked resource to work dir"
            );
        }
    }

    Ok(())
}
