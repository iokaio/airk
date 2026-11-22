// SPDX-License-Identifier: Apache-2.0
//! The bounds of the service's stop and upgrade stages, and the wait that
//! `airkctl service stop|restart` and the setup drain give a stopping service
//! (L2 gap M3, service_runtime CONTRACT.md "Stop and upgrade wait").
use std::time::Duration;

/// The arbiter's shared shutdown cutoff, from the start of the stop. The
/// ingress, connection and selection-revoke stages run inside it. The
/// arbiter exists only with `local-inference`.
#[cfg(any(test, feature = "local-inference"))]
pub(super) const ARBITER_SHUTDOWN: Duration = Duration::from_secs(30);
/// Open connection handlers, after ingress stops.
pub(super) const CONNECTION_DRAIN: Duration = Duration::from_secs(10);
/// Revoking the ended window and agent grants in the selection owner.
pub(super) const SELECTION_REVOKE: Duration = Duration::from_secs(5);

/// The supervisor's owner stop, owned by airk-supervisor
/// (`project_supervisor.rs`): an in-process owner gets `OWNER_STOP_GRACE`
/// (10 s), then `IN_PROCESS_DRAIN` (20 s), then is quarantined, all owners in
/// parallel. Child-process owners exist only in that crate's tests. Recorded
/// here because the supervisor does not export the figure; the follow-up is
/// for it to export one constant this module reads.
#[cfg(test)]
pub(super) const OWNER_STOP: Duration = Duration::from_secs(30);
/// The supervisor actor's turn (`recv_timeout`, 250 ms) before it sees the
/// service drop it, and its stopping-owner poll period (100 ms).
#[cfg(test)]
pub(super) const ACTOR_NOTICE: Duration = Duration::from_millis(250);
#[cfg(test)]
pub(super) const OWNER_POLL: Duration = Duration::from_millis(100);

#[cfg(test)]
mod tests {
    use super::*;

    /// A clock the model advances; it never sleeps.
    struct Clock(Duration);
    impl Clock {
        fn wait(&mut self, bound: Duration) {
            self.0 += bound;
        }
        /// Wait until an absolute deadline from the start of the stop.
        fn until(&mut self, deadline: Duration) {
            self.0 = self.0.max(deadline);
        }
        /// Poll every `period` until `done` holds.
        fn poll(&mut self, period: Duration, done: impl Fn(Duration) -> bool) {
            while !done(self.0) {
                self.0 += period;
            }
        }
    }

    /// The service's stop in its source order (`service_runtime.rs`, after
    /// the main loop), with every stage at its bound.
    fn worst_case_stop(local_inference: bool) -> Duration {
        let mut clock = Clock(Duration::ZERO);
        let arbiter_deadline = local_inference.then_some(ARBITER_SHUTDOWN);
        clock.wait(CONNECTION_DRAIN);
        clock.wait(SELECTION_REVOKE);
        if let Some(deadline) = arbiter_deadline {
            clock.until(deadline);
        }
        // Dropping the state drops the supervisor: its actor notices on its
        // next turn and asks every in-process owner to stop at once.
        clock.wait(ACTOR_NOTICE);
        let requested = clock.0;
        clock.poll(OWNER_POLL, |now| now >= requested + OWNER_STOP);
        clock.0
    }

    #[test]
    fn the_worst_stop_exceeded_sixty_seconds_and_fits_the_stop_wait() {
        let worst = worst_case_stop(true);
        // 30 s arbiter cutoff, 0.25 s actor turn, 30 s owner stop: the old
        // 60 s waits reported a stop that was still within its bounds as
        // failed (L2 gap M3).
        assert_eq!(worst, Duration::from_millis(60_250));
        assert!(worst > Duration::from_secs(60));
        assert!(worst_case_stop(false) < worst);
        // Both waiters now wait for the stop's bound with margin.
        let margin = Duration::from_secs(20);
        assert!(crate::setup_role::effects::EXIT >= worst + margin);
        assert!(airkctl::lifecycle::EXIT >= worst + margin);
        assert_eq!(crate::setup_role::effects::EXIT, airkctl::lifecycle::EXIT);
    }
}
