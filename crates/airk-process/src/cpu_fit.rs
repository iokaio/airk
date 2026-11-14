// SPDX-License-Identifier: Apache-2.0
//! Conservative usable physical-core observation; logical CPUs never substitute.
#[cfg(any(windows, test))]
fn windows_core_count(bytes: &[u8], process_mask: u64) -> Option<usize> {
    let mut offset = 0usize;
    let mut count = 0usize;
    while offset < bytes.len() {
        let header = bytes.get(offset..offset.checked_add(8)?)?;
        let relationship = u32::from_ne_bytes(header[..4].try_into().ok()?);
        let size = u32::from_ne_bytes(header[4..8].try_into().ok()?) as usize;
        if size < 8 {
            return None;
        }
        let row = bytes.get(offset..offset.checked_add(size)?)?;
        if relationship == 0 {
            let groups = u16::from_ne_bytes(row.get(30..32)?.try_into().ok()?);
            if groups != 1 || row.len() < 48 {
                return None;
            }
            let mask = u64::from_ne_bytes(row[32..40].try_into().ok()?);
            let group = u16::from_ne_bytes(row[40..42].try_into().ok()?);
            if group != 0 || mask == 0 {
                return None;
            }
            if mask & process_mask != 0 {
                count = count.checked_add(1)?;
            }
        }
        offset = offset.checked_add(size)?;
    }
    (count > 0).then_some(count)
}
#[cfg(windows)]
pub fn usable_physical_cores() -> Option<usize> {
    use windows_sys::Win32::System::{
        SystemInformation::{GetLogicalProcessorInformationEx, RelationProcessorCore},
        Threading::{GetActiveProcessorGroupCount, GetCurrentProcess, GetProcessAffinityMask},
    };
    // Multi-group affinity requires a richer admission policy; unknown refuses.
    if unsafe { GetActiveProcessorGroupCount() } != 1 {
        return None;
    }
    let mut process_mask = 0usize;
    let mut system_mask = 0usize;
    if unsafe { GetProcessAffinityMask(GetCurrentProcess(), &mut process_mask, &mut system_mask) }
        == 0
        || process_mask == 0
    {
        return None;
    }
    let mut length = 0u32;
    unsafe {
        GetLogicalProcessorInformationEx(RelationProcessorCore, std::ptr::null_mut(), &mut length);
    }
    if length == 0 || length > 1 << 20 {
        return None;
    }
    let mut aligned = vec![0u64; (length as usize).div_ceil(8)];
    if unsafe {
        GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            aligned.as_mut_ptr().cast(),
            &mut length,
        )
    } == 0
    {
        return None;
    }
    if length as usize > aligned.len() * 8 {
        return None;
    }
    // SAFETY: successful native call initialized length bytes in aligned storage.
    let bytes =
        unsafe { std::slice::from_raw_parts(aligned.as_ptr().cast::<u8>(), length as usize) };
    windows_core_count(bytes, process_mask as u64)
}
#[cfg(target_os = "macos")]
pub fn usable_physical_cores() -> Option<usize> {
    let mut count = 0u32;
    let mut size = std::mem::size_of_val(&count);
    // SAFETY: fixed SDK sysctl name and correctly sized output storage.
    let result = unsafe {
        libc::sysctlbyname(
            c"hw.physicalcpu".as_ptr(),
            (&mut count as *mut u32).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    let usable = std::thread::available_parallelism().ok()?.get();
    (result == 0 && size == 4 && count > 0).then_some((count as usize).min(usable))
}
#[cfg(target_os = "linux")]
pub fn usable_physical_cores() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let list = status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))?
        .trim();
    let mut physical = std::collections::BTreeSet::new();
    for segment in list.split(',') {
        let mut bounds = segment.split('-');
        let start: usize = bounds.next()?.parse().ok()?;
        let end: usize = bounds
            .next()
            .map(str::parse)
            .transpose()
            .ok()?
            .unwrap_or(start);
        if bounds.next().is_some() || end < start || end > 65535 {
            return None;
        }
        for cpu in start..=end {
            let root = format!("/sys/devices/system/cpu/cpu{cpu}/topology");
            let package = std::fs::read_to_string(format!("{root}/physical_package_id")).ok()?;
            let core = std::fs::read_to_string(format!("{root}/core_id")).ok()?;
            physical.insert((
                package.trim().parse::<u32>().ok()?,
                core.trim().parse::<u32>().ok()?,
            ));
        }
    }
    let count = physical
        .len()
        .min(std::thread::available_parallelism().ok()?.get());
    (count > 0).then_some(count)
}
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn usable_physical_cores() -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smt_and_affinity_count_physical_cores_and_unknown_rows_refuse() {
        let mut bytes = vec![];
        for mask in [3u64, 12, 48, 192] {
            let mut row = [0u8; 48];
            row[4..8].copy_from_slice(&48u32.to_ne_bytes());
            row[30..32].copy_from_slice(&1u16.to_ne_bytes());
            row[32..40].copy_from_slice(&mask.to_ne_bytes());
            bytes.extend(row);
        }
        assert_eq!(windows_core_count(&bytes, 255), Some(4));
        assert_eq!(windows_core_count(&bytes, 15), Some(2));
        assert_eq!(windows_core_count(&bytes, 0), None);
        bytes[30..32].copy_from_slice(&2u16.to_ne_bytes());
        assert_eq!(windows_core_count(&bytes, 255), None);
        assert_eq!(windows_core_count(&bytes[..7], 255), None);
    }
}
