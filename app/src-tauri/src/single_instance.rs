use anyhow::{anyhow, bail, Result};

#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::iter;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
#[cfg(windows)]
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject};

const APP_INSTANCE_MUTEX_NAME: &str = "Global\\VanySound.App4.Instance";
const RUNTIME_OPERATION_MUTEX_NAME: &str = "Global\\VanySound.App4.Runtime";
const RUNTIME_MUTEX_TIMEOUT_MS: u32 = 8_000;

pub struct InstanceGuard {
    #[cfg(windows)]
    handle: HANDLE,
}

pub struct NamedMutexGuard {
    #[cfg(windows)]
    handle: HANDLE,
}

pub fn acquire_instance_guard() -> Result<InstanceGuard> {
    #[cfg(not(windows))]
    {
        Ok(InstanceGuard {})
    }

    #[cfg(windows)]
    unsafe {
        let name = to_wide(APP_INSTANCE_MUTEX_NAME);
        let handle = CreateMutexW(None, true, PCWSTR(name.as_ptr()))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(handle);
            bail!("VanySound is already running.");
        }

        Ok(InstanceGuard { handle })
    }
}

pub fn acquire_runtime_operation_guard() -> Result<NamedMutexGuard> {
    #[cfg(not(windows))]
    {
        Ok(NamedMutexGuard {})
    }

    #[cfg(windows)]
    unsafe {
        let name = to_wide(RUNTIME_OPERATION_MUTEX_NAME);
        let handle = CreateMutexW(None, false, PCWSTR(name.as_ptr()))?;
        let wait_result = WaitForSingleObject(handle, RUNTIME_MUTEX_TIMEOUT_MS);

        if wait_result == WAIT_OBJECT_0 || wait_result == WAIT_ABANDONED {
            return Ok(NamedMutexGuard { handle });
        }

        let _ = CloseHandle(handle);
        if wait_result == WAIT_TIMEOUT {
            bail!("Otra operacion de audio sigue en progreso. Intenta de nuevo en unos segundos.");
        }

        Err(anyhow!(
            "No se pudo adquirir el candado global de audio (wait={}).",
            wait_result.0
        ))
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            let _ = ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}

impl Drop for NamedMutexGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            let _ = ReleaseMutex(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(windows)]
fn to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(iter::once(0))
        .collect()
}
