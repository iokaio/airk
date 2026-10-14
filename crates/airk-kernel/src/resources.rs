// SPDX-License-Identifier: Apache-2.0
//! Best-effort process counters. Missing counters remain unknown, never zero.
use airk_core::usage::{ResourceCounter as Counter, ResourceMeasurement};
use std::{collections::BTreeMap, sync::Mutex};

#[derive(Default)]
struct Active {
    count: u64,
    epoch: u64,
}
static ACTIVE: Mutex<Active> = Mutex::new(Active { count: 0, epoch: 0 });

#[derive(Default)]
struct Sample {
    counters: BTreeMap<Counter, u64>,
    peak: Option<u64>,
}

pub(crate) struct ResourceSpan {
    start: Sample,
    epoch: Option<u64>,
    closed: bool,
}

impl ResourceSpan {
    pub(crate) fn start() -> Self {
        let epoch = ACTIVE.lock().ok().map(|mut active| {
            if active.count > 0 {
                active.epoch = active.epoch.wrapping_add(1);
            }
            active.count += 1;
            active.epoch
        });
        Self {
            start: sample(),
            epoch,
            closed: false,
        }
    }

    pub(crate) fn finish(mut self) -> ResourceMeasurement {
        let end = sample();
        let overlapped = self.close();
        let mut counters = BTreeMap::new();
        if !overlapped {
            for (counter, before) in &self.start.counters {
                if let Some(delta) = end
                    .counters
                    .get(counter)
                    .and_then(|after| after.checked_sub(*before))
                {
                    counters.insert(*counter, delta);
                }
            }
        }
        ResourceMeasurement {
            overlapped,
            counters,
            peak_rss_bytes: end.peak.max(self.start.peak),
        }
    }

    fn close(&mut self) -> bool {
        self.closed = true;
        let Some(epoch) = self.epoch else {
            return true;
        };
        let Ok(mut active) = ACTIVE.lock() else {
            return true;
        };
        let overlap = active.count != 1 || active.epoch != epoch;
        active.count = active.count.saturating_sub(1);
        overlap
    }
}

impl Drop for ResourceSpan {
    fn drop(&mut self) {
        if !self.closed {
            self.close();
        }
    }
}

#[cfg(windows)]
fn sample() -> Sample {
    use windows_sys::Win32::{
        Foundation::FILETIME,
        System::{
            ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
            Threading::{GetCurrentProcess, GetProcessIoCounters, GetProcessTimes, IO_COUNTERS},
        },
    };
    let mut result = Sample::default();
    // SAFETY: current-process pseudo handle needs no close. All OS output buffers
    // are correctly sized, initialized and exclusively borrowed for each call.
    unsafe {
        let handle = GetCurrentProcess();
        let mut creation: FILETIME = std::mem::zeroed();
        let mut exit: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        if GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) != 0 {
            let ticks = |time: FILETIME| {
                (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
            };
            if let Some(total) = ticks(kernel).checked_add(ticks(user)) {
                result.counters.insert(Counter::ProcessCpuUs, total / 10);
            }
        }
        let mut io: IO_COUNTERS = std::mem::zeroed();
        if GetProcessIoCounters(handle, &mut io) != 0 {
            result
                .counters
                .insert(Counter::WindowsReadTransferBytes, io.ReadTransferCount);
            result
                .counters
                .insert(Counter::WindowsWriteTransferBytes, io.WriteTransferCount);
        }
        let mut memory: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        memory.cb = size;
        if GetProcessMemoryInfo(handle, &mut memory, size) != 0 {
            result.peak = u64::try_from(memory.PeakWorkingSetSize).ok();
        }
    }
    result
}

#[cfg(unix)]
fn sample() -> Sample {
    let mut result = Sample::default();
    // SAFETY: getrusage writes a correctly sized libc buffer for this process.
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) == 0 {
            let micros = |value: libc::timeval| -> Option<u64> {
                u64::try_from(value.tv_sec)
                    .ok()?
                    .checked_mul(1_000_000)?
                    .checked_add(u64::try_from(value.tv_usec).ok()?)
            };
            if let Some(total) = micros(usage.ru_utime)
                .zip(micros(usage.ru_stime))
                .and_then(|(a, b)| a.checked_add(b))
            {
                result.counters.insert(Counter::ProcessCpuUs, total);
            }
            #[cfg(target_os = "linux")]
            {
                result.peak = u64::try_from(usage.ru_maxrss)
                    .ok()
                    .and_then(|v| v.checked_mul(1024));
            }
            #[cfg(target_os = "macos")]
            {
                result.peak = u64::try_from(usage.ru_maxrss).ok();
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::io::Read;
        let mut text = String::new();
        if let Ok(file) = std::fs::File::open("/proc/self/io")
            && file.take(8192).read_to_string(&mut text).is_ok()
        {
            for (key, value) in text.lines().filter_map(|line| line.split_once(':')) {
                let counter = match key {
                    "rchar" => Counter::LinuxReadSyscallBytes,
                    "wchar" => Counter::LinuxWriteSyscallBytes,
                    "read_bytes" => Counter::OsStorageReadBytes,
                    "write_bytes" => Counter::OsStorageWriteBytes,
                    "cancelled_write_bytes" => Counter::LinuxCancelledWriteBytes,
                    _ => continue,
                };
                if let Ok(value) = value.trim().parse() {
                    result.counters.insert(counter, value);
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    // SAFETY: RUSAGE_INFO_V2 selects exactly this libc-defined output layout.
    unsafe {
        let mut info: libc::rusage_info_v2 = std::mem::zeroed();
        if libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V2,
            (&mut info as *mut libc::rusage_info_v2).cast(),
        ) == 0
        {
            result
                .counters
                .insert(Counter::OsStorageReadBytes, info.ri_diskio_bytesread);
            result
                .counters
                .insert(Counter::OsStorageWriteBytes, info.ri_diskio_byteswritten);
        }
    }
    result
}

#[cfg(not(any(unix, windows)))]
fn sample() -> Sample {
    Sample::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_and_abandoned_spans_do_not_double_count() {
        let first = ResourceSpan::start();
        let second = ResourceSpan::start();
        let second = second.finish();
        assert!(second.overlapped && second.counters.is_empty());
        let first = first.finish();
        assert!(first.overlapped && first.counters.is_empty());
        drop(ResourceSpan::start());
        let probe = sample();
        #[cfg(any(windows, unix))]
        assert!(probe.counters.contains_key(&Counter::ProcessCpuUs));
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        assert!(probe.peak.is_some_and(|bytes| bytes > 0));
    }
}
