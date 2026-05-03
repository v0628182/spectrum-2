use crate::models::InstallerStateDto;
use crate::native_control;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Default)]
pub struct InstallerManager {
    inner: Arc<Mutex<InstallerRuntimeState>>,
}

#[derive(Debug, Clone)]
struct InstallerRuntimeState {
    completed: bool,
    exit_code: Option<i32>,
    finished_at: Option<String>,
    running: bool,
    started_at: Option<String>,
    stderr: Vec<String>,
    stdout: Vec<String>,
    success: Option<bool>,
}

impl Default for InstallerRuntimeState {
    fn default() -> Self {
        Self {
            completed: false,
            exit_code: None,
            finished_at: None,
            running: false,
            started_at: None,
            stderr: Vec::new(),
            stdout: Vec::new(),
            success: None,
        }
    }
}

#[derive(Clone, Copy)]
struct InstallerStep {
    detail: &'static str,
    id: u32,
    title: &'static str,
}

const INSTALL_STEPS: [InstallerStep; 6] = [
    InstallerStep {
        id: 1,
        title: "Setting up audio drivers",
        detail: "Configuring Hi-Fi Cable and low-level audio routing.",
    },
    InstallerStep {
        id: 2,
        title: "Tuning performance",
        detail: "Renaming endpoints and applying 48 kHz / 24-bit defaults.",
    },
    InstallerStep {
        id: 3,
        title: "Loading sound profiles",
        detail: "Deploying runtime assets and profile bundles.",
    },
    InstallerStep {
        id: 4,
        title: "Configuring spatial audio",
        detail: "Registering Device Selector, MJUCjr and endpoint bindings.",
    },
    InstallerStep {
        id: 5,
        title: "Verifying setup",
        detail: "Running strict status and verify checks against the installed stack.",
    },
    InstallerStep {
        id: 6,
        title: "Wrapping up",
        detail: "Finalizing the desktop client and leaving the audio chain ready.",
    },
];

impl InstallerManager {
    pub fn start(&self) -> anyhow::Result<()> {
        {
            let mut state = self.inner.lock().expect("installer state poisoned");
            if state.running {
                return Ok(());
            }

            *state = InstallerRuntimeState {
                running: true,
                started_at: Some(now_string()),
                ..InstallerRuntimeState::default()
            };
        }

        clear_known_logs();
        let state_ref = Arc::clone(&self.inner);
        thread::Builder::new()
            .name("vanysound-installer".to_string())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_installer_process(Arc::clone(&state_ref));
                }));
                if let Err(panic) = result {
                    let msg = panic
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                        .unwrap_or_else(|| "unknown panic".into());
                    tracing::error!("Installer thread PANIC: {}", msg);
                    let mut guard = state_ref.lock().unwrap_or_else(|e| e.into_inner());
                    guard.running = false;
                    guard.completed = true;
                    guard.success = Some(false);
                    guard.stderr.push(format!("Internal error: {}", msg));
                }
            })
            .expect("failed to spawn installer thread");
        Ok(())
    }

    pub fn snapshot(&self) -> InstallerStateDto {
        build_snapshot(&self.inner)
    }
}

fn run_installer_process(state: Arc<Mutex<InstallerRuntimeState>>) {
    let result = (|| -> anyhow::Result<i32> {
        push_stream_line(
            &state,
            true,
            "[init] Extracting embedded installer...".into(),
        );

        // Locate the app's resource directory for binary assets
        let resource_dir = resolve_resource_dir();
        push_stream_line(
            &state,
            true,
            format!(
                "[init] Resource dir: {}",
                resource_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<none>".into())
            ),
        );

        // Extract embedded scripts to temp working directory
        let work_dir = crate::embedded_scripts::extract_embedded_scripts(
            resource_dir.as_deref().unwrap_or(Path::new("")),
        )?;
        let script_path = work_dir.join("install_echosuite.ps1");
        if !script_path.is_file() {
            anyhow::bail!("Failed to extract embedded installer script");
        }

        push_stream_line(
            &state,
            true,
            format!("[init] Scripts extracted to: {}", work_dir.display()),
        );

        // Clear old log files BEFORE launching
        clear_known_logs();
        let stderr_capture = std::env::temp_dir().join("vanysound_ps_stderr.log");
        let _ = std::fs::remove_file(&stderr_capture);

        // Build PS args with -File (simpler than -Command, no quoting headaches)
        let ps_script = normalize_powershell_script_path(&script_path);
        let mut ps_args = vec![
            "-NoProfile".to_string(),
            "-NonInteractive".to_string(),
            "-ExecutionPolicy".to_string(),
            "Bypass".to_string(),
            "-WindowStyle".to_string(),
            "Hidden".to_string(),
            "-File".to_string(),
            ps_script.display().to_string(),
            "-SkipSelfElevation".to_string(),
            "-ConsoleLog".to_string(),
            "-ForceRepair".to_string(),
        ];

        // Add desktop source args
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(exe_dir) = current_exe.parent() {
                ps_args.push("-DesktopSource".to_string());
                ps_args.push(exe_dir.display().to_string());
            }
            if let Some(exe_name) = current_exe.file_name() {
                ps_args.push("-DesktopExeName".to_string());
                ps_args.push(exe_name.to_string_lossy().to_string());
            }
        }

        let is_admin = crate::elevation::is_elevated();
        push_stream_line(
            &state,
            true,
            format!("[init] Elevated: {} | Launching installer...", is_admin),
        );

        let result = if is_admin {
            run_installer_direct(&state, &ps_args)
        } else {
            run_installer_elevated(&state, &ps_args)
        };

        // After process exits, capture any stderr file content
        if let Ok(stderr_content) = std::fs::read_to_string(&stderr_capture) {
            for line in stderr_content.lines() {
                if !line.trim().is_empty() {
                    push_stream_line(&state, false, format!("[stderr-file] {}", line.trim()));
                }
            }
        }

        result
    })();

    // Finalize state
    let mut guard = state.lock().expect("installer state poisoned");
    guard.running = false;
    guard.completed = true;
    guard.finished_at = Some(now_string());

    match result {
        Ok(code) => {
            let logged_outcome = install_terminal_outcome_detected();
            let resolved_success = reconcile_installer_success(code == 0, logged_outcome);
            guard.exit_code = Some(code);
            guard.success = Some(resolved_success);
            tracing::info!(exit_code = code, resolved_success, "installer finished");

            if resolved_success {
                write_install_receipt();
            }
        }
        Err(err) => {
            guard.exit_code = Some(1);
            guard.success = Some(false);
            guard.stderr.push(err.to_string());
            tracing::error!(error = %err, "installer process failed");
        }
    }

    crate::embedded_scripts::cleanup_embedded_scripts();
}

/// Direct launch — stdout/stderr captured to files (more reliable than pipes on Windows).
fn run_installer_direct(
    state: &Arc<Mutex<InstallerRuntimeState>>,
    ps_args: &[String],
) -> anyhow::Result<i32> {
    let stdout_path = std::env::temp_dir().join("vanysound_ps_stdout.log");
    let stderr_path = std::env::temp_dir().join("vanysound_ps_stderr.log");
    let _ = std::fs::remove_file(&stdout_path);
    let _ = std::fs::remove_file(&stderr_path);

    let stdout_file = std::fs::File::create(&stdout_path)?;
    let stderr_file = std::fs::File::create(&stderr_path)?;

    let mut command = Command::new("powershell.exe");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    for arg in ps_args {
        command.arg(arg);
    }
    command
        .stdin(Stdio::null())
        .stdout(stdout_file)
        .stderr(stderr_file);

    let mut child = command.spawn()?;

    // Tail ALL output sources: PS log files + our stdout/stderr capture files
    let log_tailer = start_log_tailer_with_ps_output(
        Arc::clone(state),
        stdout_path.clone(),
        stderr_path.clone(),
    );

    let exit_code = wait_for_process(&mut child, state)?;

    // Give the tailer time to read remaining data
    thread::sleep(Duration::from_millis(1000));
    log_tailer.store(true, std::sync::atomic::Ordering::Relaxed);
    thread::sleep(Duration::from_millis(500));

    // Final read of stderr for any late data
    if let Ok(content) = std::fs::read_to_string(&stderr_path) {
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        if !lines.is_empty() {
            for line in &lines {
                push_stream_line(state, false, format!("[ps-stderr] {}", line.trim()));
            }
        }
    }

    push_stream_line(
        state,
        true,
        format!("[result] PowerShell exited with code {}", exit_code),
    );

    Ok(exit_code)
}

/// Log tailer that reads PS script log files AND our stdout/stderr capture files.
fn start_log_tailer_with_ps_output(
    state: Arc<Mutex<InstallerRuntimeState>>,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_flag = stop.clone();

    thread::spawn(move || {
        let log_files = [
            ("ps-out", stdout_path),
            ("ps-err", stderr_path),
            ("master", std::env::temp_dir().join("echosuite_master.log")),
            ("hifi", std::env::temp_dir().join("hificable_install.log")),
            ("apo", std::env::temp_dir().join("echosuite_apo.log")),
            (
                "loudness",
                std::env::temp_dir().join("echosuite_loudness.log"),
            ),
        ];

        let mut offsets: Vec<usize> = vec![0; log_files.len()];

        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            for (i, (label, path)) in log_files.iter().enumerate() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    let lines: Vec<&str> = content.lines().collect();
                    if lines.len() > offsets[i] {
                        for line in &lines[offsets[i]..] {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                push_stream_line(&state, true, format!("[{label}] {trimmed}"));
                            }
                        }
                        offsets[i] = lines.len();
                    }
                }
            }
            thread::sleep(Duration::from_millis(400));
        }

        // Final sweep
        for (i, (label, path)) in log_files.iter().enumerate() {
            if let Ok(content) = std::fs::read_to_string(path) {
                let lines: Vec<&str> = content.lines().collect();
                if lines.len() > offsets[i] {
                    for line in &lines[offsets[i]..] {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            push_stream_line(&state, true, format!("[{label}] {trimmed}"));
                        }
                    }
                }
            }
        }
    });

    stop_flag
}

/// Elevated launch via ShellExecuteExW — no visible window, no broken pipes.
/// Output is captured by tailing the log files the PS scripts write to.
fn run_installer_elevated(
    state: &Arc<Mutex<InstallerRuntimeState>>,
    ps_args: &[String],
) -> anyhow::Result<i32> {
    push_stream_line(
        state,
        true,
        "[elevated] Requesting admin privileges...".into(),
    );

    #[cfg(not(windows))]
    {
        let _ = ps_args;
        anyhow::bail!("Elevated install only supported on Windows");
    }

    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::iter;
        use std::mem::size_of;
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::{CloseHandle, GetLastError};
        use windows::Win32::System::Threading::{
            GetExitCodeProcess, WaitForSingleObject, INFINITE,
        };
        use windows::Win32::UI::Shell::{
            ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
        };
        use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

        fn to_wide(s: &str) -> Vec<u16> {
            OsStr::new(s).encode_wide().chain(iter::once(0)).collect()
        }

        // Build the parameter string for powershell.exe
        let params = ps_args
            .iter()
            .map(|a| {
                if a.contains(' ') || a.contains('"') {
                    format!("\"{}\"", a.replace('"', "\\\""))
                } else {
                    a.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        let file_wide = to_wide("powershell.exe");
        let verb_wide = to_wide("runas");
        let params_wide = to_wide(&params);

        let mut info = SHELLEXECUTEINFOW::default();
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = PCWSTR(verb_wide.as_ptr());
        info.lpFile = PCWSTR(file_wide.as_ptr());
        info.lpParameters = PCWSTR(params_wide.as_ptr());
        info.nShow = SW_HIDE.0;

        let launched = unsafe { ShellExecuteExW(&mut info).is_ok() };
        if !launched {
            let err = unsafe { GetLastError().0 };
            anyhow::bail!(
                "ShellExecuteExW failed (error {}). User may have declined UAC.",
                err
            );
        }

        push_stream_line(
            state,
            true,
            "[elevated] Admin process launched. Monitoring logs...".into(),
        );

        // Start log tailer — this is our ONLY source of output in elevated mode
        let stop_flag = start_log_tailer(Arc::clone(state));

        // Wait for the elevated process to exit
        if !info.hProcess.is_invalid() {
            unsafe {
                WaitForSingleObject(info.hProcess, INFINITE);
            }

            let mut exit_code = 0u32;
            let exit_ok = unsafe { GetExitCodeProcess(info.hProcess, &mut exit_code).is_ok() };
            unsafe {
                let _ = CloseHandle(info.hProcess);
            }

            stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            // Give tailer one last read
            thread::sleep(Duration::from_millis(500));

            if !exit_ok {
                anyhow::bail!("Elevated process finished but exit code could not be read");
            }

            push_stream_line(
                state,
                true,
                format!("[elevated] Process exited with code {}", exit_code),
            );

            return Ok(exit_code as i32);
        }

        stop_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        anyhow::bail!("Elevated process handle was invalid");
    }
}

/// Starts a background thread that tails PS log files and pushes new lines to the UI state.
/// Returns an AtomicBool flag to signal the thread to stop.
fn start_log_tailer(
    state: Arc<Mutex<InstallerRuntimeState>>,
) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_flag = stop.clone();

    thread::spawn(move || {
        let log_files = [
            ("master", std::env::temp_dir().join("echosuite_master.log")),
            ("hifi", std::env::temp_dir().join("hificable_install.log")),
            ("apo", std::env::temp_dir().join("echosuite_apo.log")),
            (
                "loudness",
                std::env::temp_dir().join("echosuite_loudness.log"),
            ),
        ];

        // Track how many lines we've already read from each file
        let mut offsets: Vec<usize> = vec![0; log_files.len()];

        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            for (i, (label, path)) in log_files.iter().enumerate() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    let lines: Vec<&str> = content.lines().collect();
                    let new_start = offsets[i];
                    if lines.len() > new_start {
                        for line in &lines[new_start..] {
                            let trimmed = line.trim();
                            if !trimmed.is_empty() {
                                push_stream_line(&state, true, format!("[{label}] {trimmed}"));
                            }
                        }
                        offsets[i] = lines.len();
                    }
                }
            }
            thread::sleep(Duration::from_millis(500));
        }

        // One final read to catch any remaining lines
        for (i, (label, path)) in log_files.iter().enumerate() {
            if let Ok(content) = std::fs::read_to_string(path) {
                let lines: Vec<&str> = content.lines().collect();
                if lines.len() > offsets[i] {
                    for line in &lines[offsets[i]..] {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            push_stream_line(&state, true, format!("[{label}] {trimmed}"));
                        }
                    }
                }
            }
        }
    });

    stop_flag
}

/// Waits for a child process with timeout and terminal-outcome detection.
fn wait_for_process(
    child: &mut std::process::Child,
    _state: &Arc<Mutex<InstallerRuntimeState>>,
) -> anyhow::Result<i32> {
    let global_deadline = Instant::now() + Duration::from_secs(300);

    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.code().unwrap_or(1));
        }

        if Instant::now() > global_deadline {
            tracing::error!("installer exceeded 300s timeout; killing");
            let _ = child.kill();
            let _ = child.wait();
            return Ok(1);
        }

        thread::sleep(Duration::from_millis(400));
    }
}

fn push_stream_line(state: &Arc<Mutex<InstallerRuntimeState>>, stdout: bool, line: String) {
    let mut guard = state.lock().expect("installer state poisoned");
    let target = if stdout {
        &mut guard.stdout
    } else {
        &mut guard.stderr
    };
    target.push(line);
    if target.len() > 64 {
        let drain = target.len() - 64;
        target.drain(0..drain);
    }
}

fn build_snapshot(state: &Arc<Mutex<InstallerRuntimeState>>) -> InstallerStateDto {
    let current = state.lock().expect("installer state poisoned").clone();
    let logs = collect_log_bundle(&current);
    let installed = runtime_installed();
    let derived_outcome = derive_install_outcome(&logs, installed);
    let mut completed = current.completed || derived_outcome.is_some();
    let mut success = resolve_snapshot_success(current.success, derived_outcome);
    if installed && !current.running && success.is_none() {
        completed = true;
        success = Some(true);
    }
    let latest_line = find_latest_meaningful_line(&logs, &current)
        .unwrap_or_else(|| "Waiting for the installer to report progress.".to_string());
    let (step, progress) = derive_progress(&logs, completed, success);
    let completed_with_attention = completed && success == Some(true) && !installed;

    InstallerStateDto {
        completed,
        current_step: step.id,
        detail: if completed && success == Some(true) {
            if completed_with_attention {
                "The installer finished, but this PC still needs one more runtime validation pass. You can continue and use Settings if a repair is needed.".to_string()
            } else {
                "Everything is configured. Your audio chain is ready.".to_string()
            }
        } else {
            latest_line
        },
        exit_code: current.exit_code,
        finished_at: current.finished_at,
        headline: if completed && success == Some(true) {
            if completed_with_attention {
                "INSTALL COMPLETE".to_string()
            } else {
                "ALL SET".to_string()
            }
        } else if completed && success == Some(false) {
            "INSTALLER NEEDS ATTENTION".to_string()
        } else {
            step.title.to_ascii_uppercase()
        },
        is_installed: installed,
        log_lines: logs.merged_tail,
        progress,
        running: if completed { false } else { current.running },
        started_at: current.started_at,
        success,
        summary: if completed_with_attention {
            "Installation finished. Final validation can continue from inside the desktop panel."
                .to_string()
        } else if completed && success == Some(false) {
            "The installer exited with errors. Check the latest logs.".to_string()
        } else {
            step.detail.to_string()
        },
    }
}

fn reconcile_installer_success(exit_success: bool, logged_outcome: Option<bool>) -> bool {
    match logged_outcome {
        Some(true) => true,
        Some(false) => false,
        None => exit_success,
    }
}

fn resolve_snapshot_success(
    current_success: Option<bool>,
    derived_outcome: Option<bool>,
) -> Option<bool> {
    match derived_outcome {
        Some(outcome) => Some(outcome),
        None => current_success,
    }
}

fn runtime_installed() -> bool {
    let Ok(status) = native_control::status() else {
        return false;
    };
    let verify = native_control::verify_status().ok();
    runtime_is_deployed(&status, verify.as_ref())
}

/// Deletes stale log files from a previous installer run.
fn clear_known_logs() {
    let log_files = [
        std::env::temp_dir().join("echosuite_master.log"),
        std::env::temp_dir().join("hificable_install.log"),
        std::env::temp_dir().join("echosuite_apo.log"),
        std::env::temp_dir().join("echosuite_loudness.log"),
    ];
    for path in &log_files {
        let _ = std::fs::remove_file(path);
    }
}

struct LogBundle {
    merged_tail: Vec<String>,
    per_file: Vec<(String, Vec<String>)>,
}

fn collect_log_bundle(state: &InstallerRuntimeState) -> LogBundle {
    let keys = [
        ("master", std::env::temp_dir().join("echosuite_master.log")),
        ("hifi", std::env::temp_dir().join("hificable_install.log")),
        ("apo", std::env::temp_dir().join("echosuite_apo.log")),
        (
            "loudness",
            std::env::temp_dir().join("echosuite_loudness.log"),
        ),
    ];

    let mut per_file = Vec::new();
    let mut merged = Vec::new();

    // Include app-level stdout first (diagnostic lines, progress)
    for line in state.stdout.iter().rev().take(30).rev() {
        merged.push(line.clone());
    }

    for (key, path) in keys {
        let lines = read_log_file(&path);
        for line in lines.iter().rev().take(40).rev() {
            merged.push(format!("[{key}] {}", trim_log_line(line)));
        }
        per_file.push((key.to_string(), lines));
    }

    for line in state.stderr.iter().rev().take(20).rev() {
        merged.push(format!("[stderr] {line}"));
    }

    if merged.len() > 60 {
        merged = merged.split_off(merged.len() - 60);
    }

    LogBundle {
        merged_tail: merged,
        per_file,
    }
}

fn find_latest_meaningful_line(logs: &LogBundle, state: &InstallerRuntimeState) -> Option<String> {
    // Check stdout first (most recent diagnostic info)
    for line in state.stdout.iter().rev().take(8) {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    // Then stderr
    for line in state.stderr.iter().rev().take(8) {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    // Then log files
    for (_, lines) in &logs.per_file {
        for line in lines.iter().rev().take(8) {
            let trimmed = trim_log_line(line);
            if !trimmed.trim().is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

fn derive_install_outcome(logs: &LogBundle, _installed: bool) -> Option<bool> {
    let master_lines = logs
        .per_file
        .iter()
        .find_map(|(key, lines)| (key == "master").then_some(lines.as_slice()))
        .unwrap_or(&[]);

    master_log_outcome(master_lines)
}

fn runtime_is_deployed(
    status: &native_control::ControlStatus,
    verify: Option<&native_control::VerifyStatus>,
) -> bool {
    let install_health = verify
        .and_then(|current| current.install_health.as_deref())
        .or(status.install_health.as_deref());

    match install_health {
        Some("Healthy" | "NeedsRepair" | "ApplyFailed" | "EndpointMissing" | "PluginMissing") => {
            true
        }
        Some("NeedsInstall" | "VersionMismatch") => false,
        _ => runtime_assets_present(status, verify),
    }
}

fn runtime_assets_present(
    status: &native_control::ControlStatus,
    verify: Option<&native_control::VerifyStatus>,
) -> bool {
    let bundle_present = status.bundle_present == Some(true)
        || status.bundle_sha256.as_deref().is_some_and(has_text)
        || verify
            .and_then(|current| current.bundle_sha256.as_deref())
            .is_some_and(has_text);

    let helper_present = status.helper_version.as_deref().is_some_and(has_text)
        || status
            .helper_path
            .as_ref()
            .map(|path| path.is_file())
            .unwrap_or(false)
        || verify
            .and_then(|current| current.helper_version.as_deref())
            .is_some_and(has_text)
        || verify
            .and_then(|current| current.helper_path.as_ref())
            .map(|path| path.is_file())
            .unwrap_or(false);

    bundle_present && helper_present
}

fn derive_progress(
    logs: &LogBundle,
    completed: bool,
    success: Option<bool>,
) -> (InstallerStep, u32) {
    let mut master = String::new();
    let mut hifi = String::new();
    let mut apo = String::new();
    let mut loudness = String::new();

    for (key, lines) in &logs.per_file {
        let text = lines.join("\n");
        match key.as_str() {
            "master" => master = text,
            "hifi" => hifi = text,
            "apo" => apo = text,
            "loudness" => loudness = text,
            _ => {}
        }
    }

    let mut step = INSTALL_STEPS[0];
    let mut progress = if completed { 0 } else { 8 };

    if !hifi.is_empty() {
        step = INSTALL_STEPS[0];
        progress = progress.max(22);
    }
    if hifi.contains("48k") || hifi.contains("24-bit") {
        step = INSTALL_STEPS[1];
        progress = progress.max(38);
    }
    if !apo.is_empty() {
        step = INSTALL_STEPS[2];
        progress = progress.max(60);
    }
    if apo.contains("MJUCjr") || apo.contains("Device Selector") || apo.contains("profiles.bin") {
        step = INSTALL_STEPS[3];
        progress = progress.max(78);
    }
    if !loudness.is_empty() {
        step = INSTALL_STEPS[4];
        progress = progress.max(88);
    }
    if master.contains("Verificando instalacion final") || master.contains("verify") {
        step = INSTALL_STEPS[4];
        progress = progress.max(94);
    }
    if completed && success == Some(true) {
        step = INSTALL_STEPS[5];
        progress = 100;
    }
    if completed && success == Some(false) {
        progress = progress.max(96);
    }

    (step, progress)
}

/// Finds the directory containing binary resources (drivers, APO installer, etc.)
/// These are the non-script files that the installer needs at runtime.
fn resolve_resource_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            // NSIS per-machine: C:\Program Files\VanySound\instalacion\
            candidates.push(exe_dir.join("instalacion"));
            // Tauri resources: exe_dir\_up_\instalacion\
            candidates.push(exe_dir.join("_up_").join("instalacion"));
            // Generic resources
            candidates.push(exe_dir.join("resources").join("instalacion"));
        }
    }

    // Dev mode: relative to CARGO_MANIFEST_DIR
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(root) = manifest_dir.parent() {
        candidates.push(root.join("instalacion"));
    }

    candidates.into_iter().find(|p| p.is_dir())
}

fn append_desktop_source_args(command: &mut Command, script_path: &Path) {
    let uses_release_installer = script_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case("install_echosuite.ps1"))
        .unwrap_or(false);
    if !uses_release_installer {
        return;
    }

    let Ok(current_exe) = std::env::current_exe() else {
        return;
    };
    if let Some(exe_dir) = current_exe.parent() {
        command.arg("-DesktopSource").arg(exe_dir);
    }
    if let Some(exe_name) = current_exe.file_name() {
        command.arg("-DesktopExeName").arg(exe_name);
    }
}

fn normalize_powershell_script_path(path: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let raw = path.to_string_lossy();
        if let Some(stripped) = raw.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
        if let Some(stripped) = raw.strip_prefix(r"\??\") {
            return PathBuf::from(stripped);
        }
    }

    path.to_path_buf()
}

fn install_terminal_outcome_detected() -> Option<bool> {
    let master_log = std::env::temp_dir().join("echosuite_master.log");
    let lines = read_log_file(&master_log);
    master_log_outcome(&lines)
}

fn master_log_outcome(lines: &[String]) -> Option<bool> {
    let master = lines.join("\n");
    if master.contains("Instalacion FALLIDA") {
        return Some(false);
    }

    if master.contains("Instalacion completada") {
        return Some(true);
    }

    None
}

fn trim_log_line(line: &str) -> String {
    line.trim()
        .trim_start_matches(|ch| ch == '[' || ch == ']')
        .to_string()
}

fn has_text(value: &str) -> bool {
    !value.trim().is_empty()
}

fn read_log_file(path: &Path) -> Vec<String> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut lines: Vec<String> = content
        .lines()
        .map(|line| line.trim_end().to_string())
        .collect();
    lines.retain(|line| !line.trim().is_empty());
    lines
}

fn now_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

// ═══════════════════════════════════════════════════════════════
// INSTALL RECEIPT — Persistent filesystem sentinel
// Written after successful install, read on every app launch.
// Path: C:\ProgramData\VanySound\install_receipt.json
// ═══════════════════════════════════════════════════════════════

const CURRENT_APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallReceipt {
    #[serde(rename = "v")]
    pub version: String,
    #[serde(rename = "ts")]
    pub installed_at: String,
    #[serde(rename = "c")]
    pub components: ReceiptComponents,
    #[serde(rename = "k")]
    pub machine_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptComponents {
    #[serde(rename = "hc")]
    pub hifi_cable: bool,
    #[serde(rename = "eq")]
    pub equalizer_apo: bool,
    #[serde(rename = "p")]
    pub profiles: bool,
    #[serde(rename = "ds")]
    pub device_selector: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReceiptStatus {
    /// Receipt exists and version matches current app (major.minor)
    Valid,
    /// Receipt exists but version major.minor doesn't match
    VersionMismatch,
    /// Receipt file doesn't exist
    Missing,
    /// Receipt file exists but is corrupted / unreadable
    Corrupted,
}

impl ReceiptStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::VersionMismatch => "version_mismatch",
            Self::Missing => "missing",
            Self::Corrupted => "corrupted",
        }
    }
}

fn receipt_dir() -> PathBuf {
    std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join("VanySound")
}

fn receipt_path() -> PathBuf {
    receipt_dir().join("vs.dat")
}

/// Generates a deterministic machine identifier from hostname.
fn generate_machine_id() -> String {
    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    // Simple hash — not cryptographic, just a stable ID
    let mut hash: u64 = 5381;
    for byte in hostname.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(u64::from(byte));
    }
    format!("{:016x}", hash)
}

/// Extracts major.minor from a semver string (e.g. "1.0.7" → "1.0")
fn major_minor(version: &str) -> &str {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() >= 2 {
        let end = version
            .find('.')
            .and_then(|first_dot| {
                version[first_dot + 1..]
                    .find('.')
                    .map(|p| first_dot + 1 + p)
            })
            .unwrap_or(version.len());
        &version[..end]
    } else {
        version
    }
}

/// Writes the install receipt after a successful installation.
fn write_install_receipt() {
    let dir = receipt_dir();
    if let Err(err) = std::fs::create_dir_all(&dir) {
        tracing::error!(error = %err, "failed to create receipt directory");
        return;
    }

    let receipt = InstallReceipt {
        version: CURRENT_APP_VERSION.to_string(),
        installed_at: chrono_iso_now(),
        components: detect_installed_components(),
        machine_id: generate_machine_id(),
    };

    let path = receipt_path();
    match serde_json::to_string_pretty(&receipt) {
        Ok(json) => match std::fs::write(&path, json) {
            Ok(()) => tracing::info!(
                path = %path.display(),
                version = %receipt.version,
                "install receipt written successfully"
            ),
            Err(err) => tracing::error!(error = %err, "failed to write install receipt"),
        },
        Err(err) => tracing::error!(error = %err, "failed to serialize install receipt"),
    }
}

/// Reads the install receipt from disk.
pub fn read_install_receipt() -> Option<InstallReceipt> {
    let path = receipt_path();
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Validates the install receipt against the current app version.
pub fn validate_install_receipt() -> ReceiptStatus {
    let path = receipt_path();
    if !path.exists() {
        return ReceiptStatus::Missing;
    }

    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => return ReceiptStatus::Corrupted,
    };

    let receipt: InstallReceipt = match serde_json::from_str(&content) {
        Ok(receipt) => receipt,
        Err(_) => return ReceiptStatus::Corrupted,
    };

    // Exact version match — any change forces re-setup
    if receipt.version == CURRENT_APP_VERSION {
        ReceiptStatus::Valid
    } else {
        ReceiptStatus::VersionMismatch
    }
}

/// Detects which components are currently installed.
fn detect_installed_components() -> ReceiptComponents {
    let status = native_control::status().ok();
    let verify = native_control::verify_status().ok();

    let has_bundle = status
        .as_ref()
        .and_then(|s| s.bundle_present)
        .unwrap_or(false)
        || status
            .as_ref()
            .and_then(|s| s.bundle_sha256.as_deref())
            .is_some_and(has_text);

    let has_helper = status
        .as_ref()
        .and_then(|s| s.helper_version.as_deref())
        .is_some_and(has_text)
        || verify
            .as_ref()
            .and_then(|v| v.helper_version.as_deref())
            .is_some_and(has_text);

    let has_selector = status
        .as_ref()
        .and_then(|s| s.device_selector_active)
        .unwrap_or(false);

    ReceiptComponents {
        hifi_cable: has_bundle,
        equalizer_apo: has_helper,
        profiles: has_bundle,
        device_selector: has_selector,
    }
}

/// ISO 8601 timestamp without pulling in the chrono crate.
fn chrono_iso_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Approximate UTC: good enough for a receipt timestamp
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    // Rough date from days since epoch (not accounting for leap seconds)
    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Converts days since Unix epoch to (year, month, day). Civil calendar.
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from Howard Hinnant's date library
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}
