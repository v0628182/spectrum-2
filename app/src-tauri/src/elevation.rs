use anyhow::{anyhow, Result};
#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::iter;
#[cfg(windows)]
use std::mem::size_of;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, GetLastError};
#[cfg(windows)]
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
#[cfg(windows)]
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

const NATIVE_SENTINEL: &str = "__vanysound_native__";

pub fn native_sentinel() -> &'static str {
    NATIVE_SENTINEL
}

/// Returns true if the current process is running with admin privileges.
pub fn is_elevated() -> bool {
    #[cfg(not(windows))]
    {
        false
    }

    #[cfg(windows)]
    {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::Security::{
            GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
        };
        use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        unsafe {
            let mut token = windows::Win32::Foundation::HANDLE::default();
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
}

/// If not elevated, relaunch self as admin and exit.
/// Returns true if we ARE elevated and should continue.
/// Returns false if we relaunched (caller should exit).
pub fn ensure_elevated() -> bool {
    if is_elevated() {
        return true;
    }

    #[cfg(windows)]
    {
        tracing::info!("App not elevated — relaunching as admin");
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(_) => return true, // can't relaunch, continue anyway
        };

        let args: Vec<String> = std::env::args().skip(1).collect();
        let args_str = args
            .iter()
            .map(|a| quote_windows_arg(a))
            .collect::<Vec<_>>()
            .join(" ");

        let exe_wide = to_wide(&exe.display().to_string());
        let verb_wide = to_wide("runas");
        let params_wide = to_wide(&args_str);

        let mut info = SHELLEXECUTEINFOW::default();
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.lpVerb = PCWSTR(verb_wide.as_ptr());
        info.lpFile = PCWSTR(exe_wide.as_ptr());
        info.lpParameters = PCWSTR(params_wide.as_ptr());
        info.nShow = 1; // SW_SHOWNORMAL

        let launched = unsafe { ShellExecuteExW(&mut info).is_ok() };
        if launched {
            // Elevated instance launched — exit this non-elevated one
            std::process::exit(0);
        } else {
            // User clicked "No" on UAC or it failed — continue without elevation
            tracing::warn!(
                "User declined elevation or ShellExecuteEx failed; continuing without admin"
            );
            return true;
        }
    }

    #[cfg(not(windows))]
    true
}

pub fn should_retry_elevated(err: &anyhow::Error) -> bool {
    let text = err.to_string();
    let normalized = text.to_ascii_lowercase();

    normalized.contains("access to the path is denied")
        || normalized.contains("access to the registry key")
        || normalized.contains("acceso denegado")
        || normalized.contains("os error 5")
        || normalized.contains("permiso denegado")
        || normalized.contains("requires elevation")
        || normalized.contains("the requested operation requires elevation")
        || normalized.contains("not elevated")
        || normalized.contains("device selector could not be enabled")
        || normalized.contains("device selector is not enabled")
        || normalized.contains("endpoint registration could not be enabled")
        || normalized.contains("endpoint registration is not active")
        || normalized.contains("registration is not active on target endpoint")
        || normalized.contains("repair-device-selector requires elevation")
        || normalized.contains("non-zero exit code 2")
}

pub fn run_self_elevated(args: &[&str]) -> Result<()> {
    #[cfg(not(windows))]
    {
        let _ = args;
        Err(anyhow!("La elevacion solo esta implementada para Windows."))
    }

    #[cfg(windows)]
    {
        tracing::info!(args = ?args, "attempting self elevation");
        let exe = std::env::current_exe()?;
        let exe_wide = to_wide(&exe.display().to_string());
        let verb_wide = to_wide("runas");
        let parameter_text = build_parameter_string(args);
        let parameter_wide = to_wide(&parameter_text);

        let mut info = SHELLEXECUTEINFOW::default();
        info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = PCWSTR(verb_wide.as_ptr());
        info.lpFile = PCWSTR(exe_wide.as_ptr());
        info.lpParameters = PCWSTR(parameter_wide.as_ptr());
        info.nShow = SW_HIDE.0;

        let launched = unsafe { ShellExecuteExW(&mut info).is_ok() };
        if !launched {
            let error = unsafe { GetLastError().0 };
            tracing::error!(args = ?args, shell_error = error, "self elevation launch failed");
            return Err(anyhow!("Auto-elevacion fallida (ShellExecuteExW={error})."));
        }

        if !info.hProcess.is_invalid() {
            unsafe {
                WaitForSingleObject(info.hProcess, INFINITE);
            }

            let mut exit_code = 0u32;
            let exit_ok = unsafe { GetExitCodeProcess(info.hProcess, &mut exit_code).is_ok() };
            unsafe {
                let _ = CloseHandle(info.hProcess);
            }

            if !exit_ok {
                tracing::error!(args = ?args, "self elevation finished but exit code could not be read");
                return Err(anyhow!(
                    "Auto-elevacion fallida: no se pudo leer el ExitCode del proceso elevado."
                ));
            }

            if exit_code != 0 {
                tracing::error!(args = ?args, exit_code, "self elevation finished with non-zero exit");
                return Err(anyhow!(
                    "El proceso elevado termino con codigo {}.",
                    exit_code
                ));
            }
        }

        tracing::info!(args = ?args, "self elevation completed successfully");
        Ok(())
    }
}

#[cfg(windows)]
fn build_parameter_string(args: &[&str]) -> String {
    let mut parameters = Vec::with_capacity(args.len() + 1);
    parameters.push(quote_windows_arg(NATIVE_SENTINEL));
    parameters.extend(args.iter().map(|arg| quote_windows_arg(arg)));
    parameters.join(" ")
}

#[cfg(windows)]
fn quote_windows_arg(value: &str) -> String {
    if value.is_empty()
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || ch == '"' || ch == '\\')
    {
        let mut quoted = String::from("\"");
        let mut backslashes = 0usize;
        for ch in value.chars() {
            match ch {
                '\\' => backslashes += 1,
                '"' => {
                    quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                    quoted.push('"');
                    backslashes = 0;
                }
                _ => {
                    if backslashes > 0 {
                        quoted.push_str(&"\\".repeat(backslashes));
                        backslashes = 0;
                    }
                    quoted.push(ch);
                }
            }
        }

        if backslashes > 0 {
            quoted.push_str(&"\\".repeat(backslashes * 2));
        }
        quoted.push('"');
        quoted
    } else {
        value.to_string()
    }
}

#[cfg(windows)]
fn to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(iter::once(0))
        .collect()
}
