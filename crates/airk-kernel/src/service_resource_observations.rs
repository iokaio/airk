// SPDX-License-Identifier: Apache-2.0
//! Best-effort observations of owned child PIDs, never process-name discovery.
#[cfg(windows)]
pub fn rss(pid: u32) -> Option<u64> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::{
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
            Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ},
        },
    };
    // SAFETY: read-only process handle; sized output buffer; handle closed on every path.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut memory: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        let size = std::mem::size_of_val(&memory) as u32;
        memory.cb = size;
        let ok = GetProcessMemoryInfo(handle, &mut memory, size) != 0;
        CloseHandle(handle);
        ok.then_some(memory.WorkingSetSize as u64)
    }
}
#[cfg(target_os = "linux")]
pub fn rss(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmRSS:")?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()?
            .checked_mul(1024)
    })
}
#[cfg(target_os = "macos")]
pub fn rss(pid: u32) -> Option<u64> {
    // SAFETY: libproc flavor 2 writes exactly a rusage_info_v2; no process mutation.
    unsafe {
        let mut usage: libc::rusage_info_v2 = std::mem::zeroed();
        (libc::proc_pid_rusage(
            i32::try_from(pid).ok()?,
            2,
            (&mut usage as *mut libc::rusage_info_v2).cast(),
        ) == 0)
            .then_some(usage.ri_resident_size)
    }
}
#[cfg(windows)]
pub fn available_memory() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: initialized correctly sized OS output buffer.
    unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of_val(&status) as u32;
        (GlobalMemoryStatusEx(&mut status) != 0).then_some(status.ullAvailPhys)
    }
}
#[cfg(target_os = "linux")]
pub fn available_memory() -> Option<u64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let host = info.lines().find_map(|line| {
        line.strip_prefix("MemAvailable:")?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()?
            .checked_mul(1024)
    })?;
    let groups = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    Some(constrained_available(host, &groups, |path| {
        std::fs::read_to_string(path).ok()
    }))
}
#[cfg(target_os = "macos")]
pub fn available_memory() -> Option<u64> {
    // A capacity value is not an observation of free memory. Keep unavailable until
    // a native pressure adapter supplies it; RSS admission remains active.
    None
}
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn rss(_pid: u32) -> Option<u64> {
    None
}
#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn available_memory() -> Option<u64> {
    None
}

#[cfg(any(target_os = "linux", test))]
fn constrained_available(
    host: u64,
    groups: &str,
    mut read: impl FnMut(&std::path::Path) -> Option<String>,
) -> u64 {
    use std::path::{Component, Path};
    let mut available = host;
    for line in groups.lines() {
        let mut parts = line.splitn(3, ':');
        let (Some(_), Some(controllers), Some(relative)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let (base, maximum, current) = if controllers.is_empty() {
            (Path::new("/sys/fs/cgroup"), "memory.max", "memory.current")
        } else if controllers.split(',').any(|name| name == "memory") {
            (
                Path::new("/sys/fs/cgroup/memory"),
                "memory.limit_in_bytes",
                "memory.usage_in_bytes",
            )
        } else {
            continue;
        };
        let relative = Path::new(relative.trim_start_matches('/'));
        if relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        {
            continue;
        }
        let mut directory = base.join(relative);
        loop {
            if let (Some(maximum), Some(current)) = (
                read(&directory.join(maximum)),
                read(&directory.join(current)),
            ) && let (Ok(maximum), Ok(current)) =
                (maximum.trim().parse::<u64>(), current.trim().parse::<u64>())
            {
                available = available.min(maximum.saturating_sub(current));
            }
            if directory == base || !directory.pop() {
                break;
            }
        }
    }
    available
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cgroup_pressure_honors_parents_and_unlimited_is_unknown_not_zero() {
        let values = std::collections::BTreeMap::from([
            ("/sys/fs/cgroup/app/child/memory.max", "max"),
            ("/sys/fs/cgroup/app/child/memory.current", "10"),
            ("/sys/fs/cgroup/app/memory.max", "1073741824"),
            ("/sys/fs/cgroup/app/memory.current", "1006632960"),
        ]);
        let read = |path: &std::path::Path| {
            values
                .get(path.to_str().unwrap().replace('\\', "/").as_str())
                .map(|value| (*value).into())
        };
        assert_eq!(
            constrained_available(8 << 30, "0::/app/child", read),
            64 << 20
        );
        assert_eq!(
            constrained_available(8 << 30, "0::/", |_| Some("max".into())),
            8 << 30
        );
    }
}
