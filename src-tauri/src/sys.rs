/// Everything the executor and scanners need from Windows, behind a trait so tests can fake it.
pub trait SystemOps: Send + Sync {
    /// (total bytes, item count) across all drives' Recycle Bins.
    fn recycle_bin_info(&self) -> (u64, u64);
    fn empty_recycle_bin(&self) -> Result<(), String>;
    /// Run a program with a backend-built argument list. Ok(stdout) on exit code 0,
    /// otherwise Err(stderr, or the exit code when stderr is empty).
    fn run(&self, program: &str, args: &[&str]) -> Result<String, String>;
}

/// Test double: canned command output keyed by "program arg1 arg2", and a log of what ran.
#[cfg(test)]
pub(crate) mod fake {
    use super::SystemOps;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct FakeSys {
        pub bin: (u64, u64),
        pub responses: HashMap<String, Result<String, String>>,
        pub calls: Mutex<Vec<String>>,
        pub emptied: AtomicBool,
    }

    impl FakeSys {
        pub fn with_bin(bytes: u64, items: u64) -> Self {
            Self { bin: (bytes, items), ..Default::default() }
        }
        pub fn respond(mut self, command: &str, output: Result<&str, &str>) -> Self {
            self.responses.insert(command.to_string(), output.map(String::from).map_err(String::from));
            self
        }
        pub fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
        pub fn was_emptied(&self) -> bool {
            self.emptied.load(Ordering::SeqCst)
        }
    }

    impl SystemOps for FakeSys {
        fn recycle_bin_info(&self) -> (u64, u64) {
            self.bin
        }
        fn empty_recycle_bin(&self) -> Result<(), String> {
            self.emptied.store(true, Ordering::SeqCst);
            Ok(())
        }
        fn run(&self, program: &str, args: &[&str]) -> Result<String, String> {
            let key = std::iter::once(program).chain(args.iter().copied()).collect::<Vec<_>>().join(" ");
            self.calls.lock().unwrap().push(key.clone());
            self.responses.get(&key).cloned().unwrap_or_else(|| Err("not installed".into()))
        }
    }
}

use serde::Serialize;

pub struct WindowsSys;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveUsage {
    pub root: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows_sys::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHQueryRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHQUERYRBINFO,
};

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

impl SystemOps for WindowsSys {
    fn recycle_bin_info(&self) -> (u64, u64) {
        let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, i64Size: 0, i64NumItems: 0 };
        // Null root = all drives.
        let hr = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
        if hr < 0 {
            return (0, 0);
        }
        (info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64)
    }

    fn empty_recycle_bin(&self) -> Result<(), String> {
        let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
        let hr = unsafe { SHEmptyRecycleBinW(std::ptr::null_mut(), std::ptr::null(), flags) };
        if hr < 0 {
            Err(format!("Windows could not empty the Recycle Bin (HRESULT 0x{:08X})", hr as u32))
        } else {
            Ok(())
        }
    }

    fn run(&self, program: &str, args: &[&str]) -> Result<String, String> {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let out = std::process::Command::new(program)
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("{program}: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Err(if stderr.is_empty() { format!("{program} exited with {}", out.status) } else { stderr })
        }
    }
}

pub fn drive_usage(root: &str) -> Option<DriveUsage> {
    let path = wide(root);
    let (mut available, mut total, mut free) = (0u64, 0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(path.as_ptr(), &mut available, &mut total, &mut free) };
    (ok != 0).then(|| DriveUsage { root: root.to_string(), total_bytes: total, free_bytes: free })
}

pub fn is_elevated() -> bool {
    ::is_elevated::is_elevated()
}

/// Starts a new elevated copy of this exe through UAC. The caller exits the current process on Ok.
pub fn relaunch_as_admin() -> Result<(), String> {
    // Debug builds load the UI from the `tauri dev` server, which shuts down when this process
    // exits, so the elevated copy would open a blank window.
    if cfg!(debug_assertions) {
        return Err("In development, close the app and run `pnpm tauri dev` from an administrator terminal instead.".into());
    }
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.to_string_lossy().replace('\'', "''");
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &format!("Start-Process -FilePath '{exe}' -Verb RunAs")])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("Administrator access was not granted.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_drive_usage_is_plausible() {
        let d = drive_usage("C:\\").expect("C: exists");
        assert!(d.total_bytes > 0);
        assert!(d.free_bytes <= d.total_bytes);
    }

    #[test]
    fn run_returns_stdout_or_error() {
        assert_eq!(WindowsSys.run("cmd", &["/c", "echo hi"]).unwrap().trim(), "hi");
        assert!(WindowsSys.run("cmd", &["/c", "exit 3"]).is_err());
        assert!(WindowsSys.run("definitely-not-a-program-xyz", &[]).is_err());
    }

    #[test]
    fn missing_drive_returns_none() {
        assert!(drive_usage("Q:\\definitely-not-a-drive\\").is_none());
    }

    #[test]
    fn recycle_bin_query_does_not_panic() {
        let (_bytes, _items) = WindowsSys.recycle_bin_info();
    }

    #[test]
    fn relaunch_is_refused_in_debug_builds() {
        // Debug builds load the UI from the `tauri dev` server, which stops when this process exits,
        // so an elevated relaunch would open a blank window. Also keeps tests from raising UAC.
        let err = relaunch_as_admin().unwrap_err();
        assert!(err.contains("administrator terminal"), "{err}");
    }

    #[test]
    fn elevation_check_returns() {
        let _ = is_elevated();
    }
}
