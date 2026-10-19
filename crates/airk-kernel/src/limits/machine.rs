// SPDX-License-Identifier: Apache-2.0
//! Best-effort stable memory capacity, not fluctuating free memory.

pub(crate) fn memory_bytes() -> Option<u64> {
    platform_memory().filter(|bytes| *bytes > 0)
}

#[cfg(windows)]
fn platform_memory() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: the API receives a correctly sized, writable structure.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status.ullTotalPhys)
}

#[cfg(target_os = "macos")]
fn platform_memory() -> Option<u64> {
    let mut bytes = 0u64;
    let mut size = std::mem::size_of::<u64>();
    // SAFETY: hw.memsize returns a u64 into the provided buffer; no value is set.
    let result = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut bytes as *mut u64).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (result == 0 && size == std::mem::size_of::<u64>()).then_some(bytes)
}

#[cfg(target_os = "linux")]
fn platform_memory() -> Option<u64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    let host = info.lines().find_map(|line| {
        line.strip_prefix("MemTotal:")?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()?
            .checked_mul(1024)
    })?;
    let groups = std::fs::read_to_string("/proc/self/cgroup").unwrap_or_default();
    Some(constrained_memory(host, &groups, |path| {
        std::fs::read_to_string(path).ok()
    }))
}

#[cfg(any(target_os = "linux", test))]
fn constrained_memory(
    host: u64,
    groups: &str,
    mut read: impl FnMut(&std::path::Path) -> Option<String>,
) -> u64 {
    // Standard cgroup mounts, including a process in a nested systemd slice.
    // Walk ancestors because a parent can enforce a smaller limit than a child.
    let mut memory = host;
    for line in groups.lines() {
        let mut parts = line.splitn(3, ':');
        let (Some(_), Some(controllers), Some(relative)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let (base, name) = if controllers.is_empty() {
            (std::path::Path::new("/sys/fs/cgroup"), "memory.max")
        } else if controllers.split(',').any(|value| value == "memory") {
            (
                std::path::Path::new("/sys/fs/cgroup/memory"),
                "memory.limit_in_bytes",
            )
        } else {
            continue;
        };
        if let Some(value) = read(&base.join(name))
            && let Ok(limit) = value.trim().parse::<u64>()
            && limit > 0
        {
            memory = memory.min(limit);
        }
        let relative = std::path::Path::new(relative.trim_start_matches('/'));
        if relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            continue;
        }
        let mut directory = base.join(relative);
        loop {
            if let Some(value) = read(&directory.join(name))
                && let Ok(limit) = value.trim().parse::<u64>()
                && limit > 0
            {
                memory = memory.min(limit);
            }
            if directory == base || !directory.pop() {
                break;
            }
        }
    }
    memory
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn platform_memory() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn cgroup_parent_limit_wins_and_unlimited_is_not_zero() {
        let values = BTreeMap::from([
            ("/sys/fs/cgroup/memory.max", "max"),
            ("/sys/fs/cgroup/user.slice/memory.max", "2147483648"),
            ("/sys/fs/cgroup/user.slice/app/memory.max", "4294967296"),
            (
                "/sys/fs/cgroup/memory/memory.limit_in_bytes",
                "9223372036854771712",
            ),
            (
                "/sys/fs/cgroup/memory/app/memory.limit_in_bytes",
                "1073741824",
            ),
        ]);
        let read = |path: &std::path::Path| {
            values
                .get(path.to_str().unwrap().replace('\\', "/").as_str())
                .map(|value| (*value).into())
        };
        assert_eq!(
            constrained_memory(16 << 30, "0::/user.slice/app", read),
            2 << 30
        );
        assert_eq!(
            constrained_memory(16 << 30, "7:cpu,memory:/app", read),
            1 << 30
        );
        assert_eq!(constrained_memory(16 << 30, "0::/", read), 16 << 30);
        assert_eq!(
            constrained_memory(16 << 30, "0::/../../escape", |_| Some("1073741824".into())),
            1 << 30
        );
        assert_eq!(
            constrained_memory(16 << 30, "0::/", |_| Some("invalid".into())),
            16 << 30
        );
    }
}
