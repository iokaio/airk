// SPDX-License-Identifier: Apache-2.0
//! Stdio clients live only as long as their launching client. Never applied to the service.
use anyhow::Result;

/// EOF is the normal shutdown path. This additionally handles an abruptly lost owner
/// even when another inherited pipe handle keeps stdin open. Only this process exits;
/// Windows closes its descendant job handles. No process-name sweep or idle timeout.
pub fn watch() -> Result<()> {
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if STARTED.get().is_some() {
        return Ok(());
    }
    let owner = Owner::capture()?;
    std::thread::Builder::new()
        .name("airk-client-owner".into())
        .spawn(move || {
            loop {
                if owner.exited() {
                    std::process::exit(0);
                }
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        })?;
    let _ = STARTED.set(());
    Ok(())
}

#[cfg(unix)]
struct Owner(libc::pid_t);
#[cfg(unix)]
impl Owner {
    fn capture() -> Result<Self> {
        // SAFETY: getppid has no pointers. Reparenting changes this identity even
        // if the old PID is reused; an exec handoff preserves the parent relation.
        let parent = unsafe { libc::getppid() };
        // PID namespaces and OS supervisors may expose no ordinary parent.
        // Those transports retain EOF/container shutdown rather than failing MCP startup.
        Ok(Self(parent))
    }
    fn exited(&self) -> bool {
        self.0 > 1 && unsafe { libc::getppid() != self.0 }
    }
}

#[cfg(windows)]
struct Owner(Vec<std::os::windows::io::OwnedHandle>);
#[cfg(windows)]
impl Owner {
    fn capture() -> Result<Self> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::{
            Foundation::INVALID_HANDLE_VALUE,
            System::{
                Diagnostics::ToolHelp::{
                    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
                    TH32CS_SNAPPROCESS,
                },
                Threading::{
                    GetCurrentProcess, GetProcessTimes, OpenProcess,
                    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
                },
            },
        };
        // SAFETY: snapshot captures process metadata only; the owned handle is closed on every path.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        anyhow::ensure!(
            snapshot != INVALID_HANDLE_VALUE,
            "cannot observe MCP client owner: {}",
            std::io::Error::last_os_error()
        );
        let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot) };
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        let mut found = unsafe { Process32FirstW(snapshot.as_raw_handle(), &mut entry) };
        let mut parents = std::collections::BTreeMap::new();
        while found != 0 {
            anyhow::ensure!(parents.len() < 65536, "process snapshot exceeds bound");
            parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
            found = unsafe { Process32NextW(snapshot.as_raw_handle(), &mut entry) };
        }
        let created = |process| -> Result<u64> {
            let mut birth = unsafe { std::mem::zeroed() };
            let mut exit = unsafe { std::mem::zeroed() };
            let mut kernel = unsafe { std::mem::zeroed() };
            let mut user = unsafe { std::mem::zeroed() };
            anyhow::ensure!(
                unsafe { GetProcessTimes(process, &mut birth, &mut exit, &mut kernel, &mut user) }
                    != 0,
                "MCP owner creation identity unavailable"
            );
            Ok((u64::from(birth.dwHighDateTime) << 32) | u64::from(birth.dwLowDateTime))
        };
        let mut pid = std::process::id();
        let mut child_birth = created(unsafe { GetCurrentProcess() })?;
        let mut handles = Vec::new();
        for _ in 0..4 {
            let parent = *parents
                .get(&pid)
                .ok_or_else(|| anyhow::anyhow!("MCP owner ancestry unavailable"))?;
            anyhow::ensure!(parent != 0, "MCP client owner unavailable");
            let raw = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    0,
                    parent,
                )
            };
            anyhow::ensure!(
                !raw.is_null(),
                "MCP client owner exited or is inaccessible: {}",
                std::io::Error::last_os_error()
            );
            let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
            let birth = created(handle.as_raw_handle())?;
            anyhow::ensure!(birth <= child_birth, "MCP owner PID was reused");
            child_birth = birth;
            let mut image = vec![0u16; 32768];
            let mut length = image.len() as u32;
            anyhow::ensure!(
                unsafe {
                    windows_sys::Win32::System::Threading::QueryFullProcessImageNameW(
                        handle.as_raw_handle(),
                        0,
                        image.as_mut_ptr(),
                        &mut length,
                    )
                } != 0,
                "MCP owner image unavailable"
            );
            let image = String::from_utf16_lossy(&image[..length as usize]);
            let launcher = std::path::Path::new(&image)
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("airk-launcher.exe"));
            handles.push(handle);
            if !launcher {
                break;
            }
            pid = parent;
        }
        // Handles bind the real client and preceding wrappers, including an older
        // universal launcher. Never signal these processes; exit only ourselves.
        let owner = Self(handles);
        anyhow::ensure!(!owner.exited(), "MCP client owner already exited");
        Ok(owner)
    }
    fn exited(&self) -> bool {
        use std::os::windows::io::AsRawHandle;
        self.0.iter().any(|handle| unsafe {
            windows_sys::Win32::System::Threading::WaitForSingleObject(handle.as_raw_handle(), 0)
                == 0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    #[test]
    fn owner_fixture() -> Result<()> {
        let Some(root) = std::env::var_os("AIRK_CLIENT_OWNER_FIXTURE") else {
            return Ok(());
        };
        let root = std::path::PathBuf::from(root);
        match std::env::var("AIRK_CLIENT_OWNER_ROLE")?.as_str() {
            "worker" => {
                let owner = Owner::capture()?;
                fs::write(root.join("ready"), std::process::id().to_string())?;
                while !owner.exited() {
                    std::thread::sleep(Duration::from_millis(20));
                }
                fs::write(root.join("exited"), b"owner gone")?;
            }
            role => {
                let mut child = Command::new(if role == "wrapper" || cfg!(unix) {
                    std::env::current_exe()?
                } else {
                    root.join(if cfg!(windows) {
                        "airk-launcher.exe"
                    } else {
                        "airk-launcher"
                    })
                })
                .args([
                    "--exact",
                    "client_owner::tests::owner_fixture",
                    "--nocapture",
                ])
                .env(
                    "AIRK_CLIENT_OWNER_ROLE",
                    if role == "wrapper" || cfg!(unix) {
                        "worker"
                    } else {
                        "wrapper"
                    },
                )
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
                // The wrapper deliberately has no watcher, modelling older installed launchers.
                if role == "wrapper" {
                    child.wait()?;
                } else {
                    std::thread::sleep(Duration::from_secs(30));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn lost_client_exits_through_old_wrapper_while_another_client_stays_alive() -> Result<()> {
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        fn wait(path: &std::path::Path) -> Result<()> {
            let deadline = Instant::now() + Duration::from_secs(8);
            while !path.exists() {
                anyhow::ensure!(
                    Instant::now() < deadline,
                    "owner fixture deadline: {}",
                    path.display()
                );
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(())
        }
        let first = tempfile::tempdir()?;
        let second = tempfile::tempdir()?;
        let launch = |root: &std::path::Path| -> Result<Child> {
            fs::copy(std::env::current_exe()?, root.join("airk-launcher.exe"))?;
            Ok(Child(
                Command::new(std::env::current_exe()?)
                    .args([
                        "--exact",
                        "client_owner::tests::owner_fixture",
                        "--nocapture",
                    ])
                    .env("AIRK_CLIENT_OWNER_FIXTURE", root)
                    .env("AIRK_CLIENT_OWNER_ROLE", "client")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?,
            ))
        };
        let mut first_client = launch(first.path())?;
        let mut second_client = launch(second.path())?;
        wait(&first.path().join("ready"))?;
        wait(&second.path().join("ready"))?;
        assert!(!first.path().join("exited").exists());
        first_client.0.kill()?;
        first_client.0.wait()?;
        wait(&first.path().join("exited"))?;
        assert!(second_client.0.try_wait()?.is_none());
        assert!(!second.path().join("exited").exists());
        second_client.0.kill()?;
        second_client.0.wait()?;
        wait(&second.path().join("exited"))?;
        Ok(())
    }
}
