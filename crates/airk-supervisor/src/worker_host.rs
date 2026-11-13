// SPDX-License-Identifier: Apache-2.0
//! Where one supervised owner runs, and how its exit is observed and forced.
//!
//! The service hosts its project, cleanup and source-watch owners on its own
//! threads. A child process remains only for the existing process fixtures.
//! Both hosts report exit the same way: an in-process owner has exited only
//! when its thread returned, after its runtime finished every blocking task
//! that holds its lease, exactly what a process exit proves for a child.
//!
//! A child that does not stop is killed. An in-process owner cannot be
//! killed: it is asked to drain at its next safe point, and if it still has
//! not exited by the drain deadline it is quarantined. A quarantined owner
//! keeps its lease (its own thread holds it), its project stays blocked and
//! the service keeps serving every other project; an explicit service
//! restart is the recovery. Its supervisor keeps polling it without blocking,
//! and reaps it normally if its thread later returns on its own.
use anyhow::Result;
use std::{
    io,
    process::Child,
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// The observed end of one owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkerExit {
    success: bool,
}
impl WorkerExit {
    pub(crate) fn success(&self) -> bool {
        self.success
    }
}

pub(crate) enum WorkerHost {
    /// Only the existing child-owner fixtures construct this host.
    #[cfg(test)]
    Child(Child),
    Thread(OwnerThread),
}

/// An owner running on a service thread.
pub(crate) struct OwnerThread {
    handle: Option<JoinHandle<Result<()>>>,
    exit: Option<WorkerExit>,
}
impl OwnerThread {
    pub(crate) fn new(handle: JoinHandle<Result<()>>) -> Self {
        Self {
            handle: Some(handle),
            exit: None,
        }
    }
    fn try_wait(&mut self) -> Option<WorkerExit> {
        if self.exit.is_none()
            && self.handle.as_ref().is_some_and(JoinHandle::is_finished)
            && let Some(handle) = self.handle.take()
        {
            // The thread already finished, so joining it cannot block. A
            // returned error or a panic is a failed exit, like a nonzero code.
            let joined = handle.join();
            if airk_host::process_diagnostics::directory().is_some()
                && let Ok(Err(error)) = &joined
            {
                eprintln!("airk: in-process owner exited: {error:#}");
            }
            self.exit = Some(WorkerExit {
                success: matches!(joined, Ok(Ok(()))),
            });
        }
        self.exit
    }
}

/// Exit observation shared by both hosts and by bare child fixtures.
pub(crate) trait ObservedExit {
    fn observed_exit(&mut self) -> io::Result<Option<WorkerExit>>;
}
#[cfg(test)]
impl ObservedExit for Child {
    fn observed_exit(&mut self) -> io::Result<Option<WorkerExit>> {
        Ok(self.try_wait()?.map(|status| WorkerExit {
            success: status.success(),
        }))
    }
}
impl ObservedExit for WorkerHost {
    fn observed_exit(&mut self) -> io::Result<Option<WorkerExit>> {
        self.try_wait()
    }
}

impl WorkerHost {
    pub(crate) fn try_wait(&mut self) -> io::Result<Option<WorkerExit>> {
        match self {
            #[cfg(test)]
            Self::Child(child) => child.observed_exit(),
            Self::Thread(owner) => Ok(owner.try_wait()),
        }
    }
    /// Only a child has a process of its own. An in-process owner's memory is
    /// part of the service's own resident set and is observed there.
    pub(crate) fn id(&self) -> Option<u32> {
        match self {
            #[cfg(test)]
            Self::Child(child) => Some(child.id()),
            Self::Thread(_) => None,
        }
    }
    pub(crate) fn child_mut(&mut self) -> Option<&mut Child> {
        match self {
            #[cfg(test)]
            Self::Child(child) => Some(child),
            Self::Thread(_) => None,
        }
    }
    pub(crate) fn in_process(&self) -> bool {
        matches!(self, Self::Thread(_))
    }
}

/// One step of stopping an owner that was asked to stop at `requested`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StopStep {
    /// Keep waiting for the owner's own exit.
    Wait,
    /// A child passed its grace period: kill it, as before.
    Kill,
    /// An in-process owner passed its grace period: ask it to drain now.
    Drain,
    /// An asked-to-drain owner never reached a safe point: quarantine it.
    /// Returned on every later step; the caller records it once.
    Quarantine,
}

/// Pure stop policy. `grace` is the existing child stop period; `drain` is
/// the further time an in-process owner has to reach its next safe point.
pub(crate) fn stop_step(
    in_process: bool,
    requested: Instant,
    now: Instant,
    grace: Duration,
    drain: Duration,
    drain_requested: bool,
) -> StopStep {
    let elapsed = now.saturating_duration_since(requested);
    match (in_process, drain_requested) {
        _ if elapsed < grace => StopStep::Wait,
        (false, _) => StopStep::Kill,
        (true, false) => StopStep::Drain,
        (true, true) if elapsed < grace.saturating_add(drain) => StopStep::Wait,
        (true, true) => StopStep::Quarantine,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_policy_keeps_child_kill_and_quarantines_only_an_undrained_thread() {
        let start = Instant::now();
        let grace = Duration::from_secs(10);
        let drain = Duration::from_secs(20);
        let at = |seconds| start + Duration::from_secs(seconds);
        for in_process in [false, true] {
            assert_eq!(
                stop_step(in_process, start, at(9), grace, drain, false),
                StopStep::Wait
            );
        }
        assert_eq!(
            stop_step(false, start, at(10), grace, drain, false),
            StopStep::Kill
        );
        assert_eq!(
            stop_step(true, start, at(10), grace, drain, false),
            StopStep::Drain
        );
        assert_eq!(
            stop_step(true, start, at(29), grace, drain, true),
            StopStep::Wait
        );
        assert_eq!(
            stop_step(true, start, at(30), grace, drain, true),
            StopStep::Quarantine
        );
        // Quarantine is final for the policy; it never turns into a kill.
        assert_eq!(
            stop_step(true, start, at(3600), grace, drain, true),
            StopStep::Quarantine
        );
        // A child is still killed, never quarantined.
        assert_eq!(
            stop_step(false, start, at(3600), grace, drain, true),
            StopStep::Kill
        );
        // A clock that appears to move backwards never escalates.
        assert_eq!(
            stop_step(true, at(5), start, grace, drain, true),
            StopStep::Wait
        );
    }

    #[test]
    fn a_thread_host_reports_success_failure_and_panic_like_a_process() {
        let mut ok = WorkerHost::Thread(OwnerThread::new(std::thread::spawn(|| Ok(()))));
        let mut failed = WorkerHost::Thread(OwnerThread::new(std::thread::spawn(|| {
            anyhow::bail!("owner failed")
        })));
        let mut panicked = WorkerHost::Thread(OwnerThread::new(std::thread::spawn(|| {
            panic!("owner panicked")
        })));
        let (release, gate) = std::sync::mpsc::channel::<()>();
        let mut running = WorkerHost::Thread(OwnerThread::new(std::thread::spawn(move || {
            let _ = gate.recv();
            Ok(())
        })));
        let wait = |host: &mut WorkerHost| loop {
            // Each fixture thread ends on its own; observe the event itself.
            if let Some(exit) = host.try_wait().unwrap() {
                return exit;
            }
            std::thread::yield_now();
        };
        assert!(wait(&mut ok).success());
        assert!(!wait(&mut failed).success());
        assert!(!wait(&mut panicked).success());
        assert_eq!(running.try_wait().unwrap(), None);
        assert_eq!(running.id(), None);
        assert!(running.in_process());
        release.send(()).unwrap();
        assert!(wait(&mut running).success());
        // The observation is stable after the join.
        assert!(wait(&mut ok).success());
    }
}
