// SPDX-License-Identifier: Apache-2.0
//! Best-effort process counters. Missing counters remain unknown, never zero.
//!
//! The counters are process-wide. One service process hosts every project
//! owner on its own threads, so a span's start/end delta is attributable to
//! that span only when no other AIRK work was registered at any point during
//! it. Requests register through their usage span (measured or not); owner
//! work outside a request (worker jobs, source scans, cleanup passes) registers
//! through [`OwnerActivity`]. Any overlap reports `overlapped: true` with no
//! counter deltas instead of attributing shared counters to one project.
use airk_core::usage::{ResourceCounter as Counter, ResourceMeasurement};
use std::{
    cell::Cell,
    collections::BTreeMap,
    marker::PhantomData,
    sync::{Mutex, MutexGuard},
};

struct Active {
    /// Open registrations: spans, unmeasured requests and owner activities.
    count: u64,
    /// Advances on every registration start and end.
    epoch: u64,
}

struct Tracker(Mutex<Active>);
static GLOBAL: Tracker = Tracker::new();

thread_local! {
    /// Owner activities held by this thread: (tracker address, count).
    static HELD: Cell<(usize, u64)> = const { Cell::new((0, 0)) };
}

impl Tracker {
    const fn new() -> Self {
        Self(Mutex::new(Active { count: 0, epoch: 0 }))
    }

    fn address(&'static self) -> usize {
        self as *const Self as usize
    }

    fn active(&self) -> MutexGuard<'_, Active> {
        self.0.lock().unwrap_or_else(|poisoned| {
            // Only complete integer assignments run under this lock, so the
            // values are intact. Advance the epoch anyway: every span open
            // across the panic is reported as overlapped, never as exclusive.
            let mut active = poisoned.into_inner();
            active.epoch = active.epoch.wrapping_add(1);
            self.0.clear_poison();
            active
        })
    }

    fn held(&'static self) -> u64 {
        HELD.with(|held| {
            let (owner, count) = held.get();
            if owner == self.address() { count } else { 0 }
        })
    }
}

/// One open registration. Its start and end both advance the shared epoch.
struct Registration {
    tracker: &'static Tracker,
    epoch: u64,
    closed: bool,
}

impl Registration {
    fn open(tracker: &'static Tracker) -> Self {
        let mut active = tracker.active();
        active.count = active.count.saturating_add(1);
        active.epoch = active.epoch.wrapping_add(1);
        Self {
            tracker,
            epoch: active.epoch,
            closed: false,
        }
    }

    /// True only when nothing else started or ended since this registration
    /// opened and the only other open registrations are `discounted` owner
    /// activities held by the thread that opened it.
    fn close(&mut self, discounted: u64) -> bool {
        self.closed = true;
        let mut active = self.tracker.active();
        let exclusive =
            active.epoch == self.epoch && Some(active.count) == discounted.checked_add(1);
        active.count = active.count.saturating_sub(1);
        active.epoch = active.epoch.wrapping_add(1);
        exclusive
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        if !self.closed {
            self.close(0);
        }
    }
}

/// Owner work that runs outside any usage request, such as one worker job, a
/// source scan pass or a cleanup pass. Hold it on the thread doing the work
/// for the duration of that unit. Every usage span open anywhere in the
/// process while it is held is reported as overlapped, except a span opened
/// later on this same thread inside the unit: that span is the unit's own
/// work. Helper threads serving such a span must not enter an activity.
/// The guard is deliberately `!Send` so its thread accounting stays exact.
pub struct OwnerActivity {
    registration: Registration,
    tracked: bool,
    _thread_bound: PhantomData<*const ()>,
}

impl OwnerActivity {
    #[must_use = "the activity ends when the guard is dropped"]
    pub fn enter() -> Self {
        Self::enter_in(&GLOBAL)
    }

    fn enter_in(tracker: &'static Tracker) -> Self {
        let registration = Registration::open(tracker);
        let tracked = HELD.with(|held| {
            let (owner, count) = held.get();
            if count == 0 || owner == tracker.address() {
                held.set((tracker.address(), count.saturating_add(1)));
                true
            } else {
                false // Another tracker: never discount, so never under-report overlap.
            }
        });
        Self {
            registration,
            tracked,
            _thread_bound: PhantomData,
        }
    }
}

impl Drop for OwnerActivity {
    fn drop(&mut self) {
        if self.tracked {
            HELD.with(|held| {
                let (owner, count) = held.get();
                held.set((owner, count.saturating_sub(1)));
            });
        }
        self.registration.close(0);
    }
}

/// A request whose usage is not recorded still runs in the shared process and
/// must make concurrently measured spans report overlap.
pub(crate) struct UnmeasuredWork {
    _registration: Registration,
}

impl UnmeasuredWork {
    pub(crate) fn start() -> Self {
        Self {
            _registration: Registration::open(&GLOBAL),
        }
    }
}

#[derive(Default)]
struct Sample {
    counters: BTreeMap<Counter, u64>,
    peak: Option<u64>,
}

pub(crate) struct ResourceSpan {
    start: Sample,
    registration: Registration,
    /// Owner activities held by the opening thread when the span started.
    discounted: u64,
}

impl ResourceSpan {
    pub(crate) fn start() -> Self {
        Self::start_in(&GLOBAL)
    }

    fn start_in(tracker: &'static Tracker) -> Self {
        let discounted = tracker.held();
        let registration = Registration::open(tracker);
        Self {
            start: sample(),
            registration,
            discounted,
        }
    }

    pub(crate) fn finish(mut self) -> ResourceMeasurement {
        let end = sample();
        let overlapped = !self.registration.close(self.discounted);
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

    // Each test owns its tracker so concurrently running kernel tests that
    // open process-wide spans cannot change the expected outcome.
    #[test]
    fn exclusive_span_keeps_counters_and_same_thread_owner_work_is_discounted() {
        static TRACKER: Tracker = Tracker::new();
        let alone = ResourceSpan::start_in(&TRACKER).finish();
        assert!(!alone.overlapped);
        #[cfg(any(windows, unix))]
        assert!(alone.counters.contains_key(&Counter::ProcessCpuUs));
        let activity = OwnerActivity::enter_in(&TRACKER);
        let own = ResourceSpan::start_in(&TRACKER).finish();
        assert!(!own.overlapped, "the unit's own span is not shared work");
        let span = ResourceSpan::start_in(&TRACKER);
        drop(activity);
        let ended = span.finish();
        assert!(ended.overlapped && ended.counters.is_empty());
        assert_eq!(TRACKER.active().count, 0);
    }

    #[test]
    fn other_owner_activity_or_unmeasured_work_marks_span_overlapped() {
        static TRACKER: Tracker = Tracker::new();
        // Another owner thread starts and finishes a unit inside the span.
        let span = ResourceSpan::start_in(&TRACKER);
        std::thread::scope(|scope| {
            scope.spawn(|| drop(OwnerActivity::enter_in(&TRACKER)));
        });
        let inside = span.finish();
        assert!(inside.overlapped && inside.counters.is_empty());
        // Another owner thread holds a unit across the whole span.
        let (entered, wait_entered) = std::sync::mpsc::channel();
        let (release, wait_release) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|scope| {
            scope.spawn(move || {
                let _activity = OwnerActivity::enter_in(&TRACKER);
                entered.send(()).unwrap();
                let _ = wait_release.recv();
            });
            wait_entered.recv().unwrap();
            let across = ResourceSpan::start_in(&TRACKER).finish();
            release.send(()).unwrap();
            assert!(across.overlapped && across.counters.is_empty());
        });
        // Unrecorded work that began earlier and ended during the span.
        let earlier = Registration::open(&TRACKER);
        let span = ResourceSpan::start_in(&TRACKER);
        drop(earlier);
        assert!(span.finish().overlapped);
        assert_eq!(TRACKER.active().count, 0);
        assert!(!ResourceSpan::start_in(&TRACKER).finish().overlapped);
    }

    #[test]
    fn poisoned_tracker_recovers_and_reports_spans_crossing_the_panic_as_overlapped() {
        static TRACKER: Tracker = Tracker::new();
        let crossing = ResourceSpan::start_in(&TRACKER);
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = TRACKER.0.lock().unwrap();
            panic!("owner panicked while holding the shared usage lock");
        }));
        assert!(panicked.is_err() && TRACKER.0.is_poisoned());
        assert!(crossing.finish().overlapped);
        assert!(!TRACKER.0.is_poisoned());
        assert!(!ResourceSpan::start_in(&TRACKER).finish().overlapped);
        assert_eq!(TRACKER.active().count, 0);
    }
}
