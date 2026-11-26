// SPDX-License-Identifier: Apache-2.0
//! Request-local arithmetic for one foreground lexical repair attempt.
//! Callers provide actual admission times and independently measured bounds.
use std::time::{Duration, Instant};

use airk_core::identity::Incarnation;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use tokio_util::sync::CancellationToken;

pub(super) struct StandaloneLexicalRepair {
    arrival: Instant,
    ordinary_allowance: Duration,
    offered: AtomicBool,
}

impl StandaloneLexicalRepair {
    pub(super) fn new(arrival: Instant, ordinary_allowance: Duration) -> Self {
        Self {
            arrival,
            ordinary_allowance,
            offered: AtomicBool::new(false),
        }
    }

    pub(super) fn offer(
        &self,
        request_id: Incarnation,
        hard_deadline: Instant,
        cancellation: &CancellationToken,
    ) -> Option<LexicalRepairOrigin> {
        (!self.offered.swap(true, Ordering::AcqRel)).then(|| LexicalRepairOrigin {
            request_id,
            arrival: self.arrival,
            ordinary_allowance: self.ordinary_allowance,
            hard_deadline,
            cancellation: cancellation.child_token(),
        })
    }
}

/// Opaque original timing and cancellation for one private Host handoff.
pub struct LexicalRepairOrigin {
    request_id: Incarnation,
    arrival: Instant,
    ordinary_allowance: Duration,
    hard_deadline: Instant,
    cancellation: CancellationToken,
}

impl LexicalRepairOrigin {
    pub(super) fn request_id(&self) -> Incarnation {
        self.request_id
    }

    pub fn hard_deadline(&self) -> Instant {
        self.hard_deadline
    }

    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    pub(super) fn claim(self, worker_deadline: Instant) -> Option<ClaimedLexicalRepair> {
        if self.cancellation.is_cancelled() {
            return None;
        }
        let admission = LexicalRepairAdmission::new(
            self.arrival,
            self.ordinary_allowance,
            self.hard_deadline.min(worker_deadline),
        )?;
        Some(ClaimedLexicalRepair {
            admission: Mutex::new(Some(admission)),
            cancellation: self.cancellation,
        })
    }
}

pub(super) struct ClaimedLexicalRepair {
    admission: Mutex<Option<LexicalRepairAdmission>>,
    cancellation: CancellationToken,
}

impl ClaimedLexicalRepair {
    pub(super) fn take(&self) -> Option<LexicalRepairAdmission> {
        if self.cancellation.is_cancelled() {
            return None;
        }
        self.admission.try_lock().ok()?.take()
    }

    pub(super) fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }
}

#[derive(Clone, Copy)]
pub(crate) struct LexicalRepairEligibility {
    pub(crate) root_known: bool,
    pub(crate) pinned: bool,
    pub(crate) composed: bool,
    pub(crate) busy: bool,
    pub(crate) cancelled: bool,
}

impl LexicalRepairEligibility {
    fn permits(self) -> bool {
        self.root_known && !self.pinned && !self.composed && !self.busy && !self.cancelled
    }
}

/// Bounds cover the full work before the verified repair event, the canonical
/// replay, and final delivery respectively. Arithmetic cannot measure these.
#[derive(Clone, Copy)]
pub(crate) struct LexicalRepairBounds {
    pub(crate) repair: Duration,
    pub(crate) replay: Duration,
    pub(crate) delivery: Duration,
}

/// Not cloneable. A caller may decide once whether to wait for repair.
pub(crate) struct LexicalRepairAdmission {
    arrival: Instant,
    deadline: Option<Instant>,
}

impl LexicalRepairAdmission {
    pub(crate) fn new(
        arrival: Instant,
        ordinary_allowance: Duration,
        hard_deadline: Instant,
    ) -> Option<Self> {
        if ordinary_allowance.is_zero() || hard_deadline <= arrival {
            return None;
        }
        let doubled = ordinary_allowance.checked_mul(2)?;
        let deadline = arrival.checked_add(doubled)?.min(hard_deadline);
        Some(Self {
            arrival,
            deadline: Some(deadline),
        })
    }

    pub(super) fn narrow_to(mut self, outer_deadline: Instant) -> Option<Self> {
        let deadline = self.deadline?.min(outer_deadline);
        if deadline <= self.arrival {
            return None;
        }
        self.deadline = Some(deadline);
        Some(self)
    }

    /// A refusal returns immediately to the ordinary result. It cannot be
    /// retried with a new clock sample or a smaller estimate.
    pub(crate) fn admit(
        &mut self,
        now: Instant,
        eligibility: LexicalRepairEligibility,
        bounds: Option<LexicalRepairBounds>,
    ) -> Option<LexicalRepairAttempt> {
        let deadline = self.deadline.take()?;
        if now < self.arrival || !eligibility.permits() {
            return None;
        }
        let bounds = bounds?;
        if bounds.repair.is_zero() || bounds.replay.is_zero() || bounds.delivery.is_zero() {
            return None;
        }
        let replay_until = deadline.checked_sub(bounds.delivery)?;
        let wait_until = replay_until.checked_sub(bounds.replay)?;
        if now.checked_add(bounds.repair)? > wait_until {
            return None;
        }
        Some(LexicalRepairAttempt {
            admitted_at: now,
            deadline,
            wait_until,
            replay_until,
            replay_available: true,
        })
    }
}

/// Timing only; an actual event still needs identity and publication checks.
pub(crate) struct LexicalRepairAttempt {
    admitted_at: Instant,
    deadline: Instant,
    wait_until: Instant,
    replay_until: Instant,
    replay_available: bool,
}

impl LexicalRepairAttempt {
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Time to stop waiting and preserve the original reply.
    pub(crate) fn wait_until(&self) -> Instant {
        self.wait_until
    }

    /// Deadline for canonical replay, preserving final delivery time.
    pub(crate) fn replay_until(&self) -> Instant {
        self.replay_until
    }

    /// Shares the original work clock and consumes the only replay credit,
    /// including when a late or no-longer-authorized event is refused.
    pub(crate) fn try_replay(
        &mut self,
        now: Instant,
        eligibility: LexicalRepairEligibility,
    ) -> bool {
        std::mem::take(&mut self.replay_available)
            && eligibility.permits()
            && now >= self.admitted_at
            && now <= self.wait_until
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    const ELIGIBLE: LexicalRepairEligibility = LexicalRepairEligibility {
        root_known: true,
        pinned: false,
        composed: false,
        busy: false,
        cancelled: false,
    };
    const BOUNDS: LexicalRepairBounds = LexicalRepairBounds {
        repair: ms(100),
        replay: ms(20),
        delivery: ms(10),
    };

    #[test]
    fn doubles_ordinary_allowance_once_and_respects_earlier_hard_deadline() {
        let arrival = Instant::now();
        for (hard, cap) in [(300, 200), (150, 150)] {
            let mut admission = LexicalRepairAdmission::new(arrival, ms(100), arrival + ms(hard))
                .expect("valid timing");
            let mut attempt = admission
                .admit(arrival + ms(20), ELIGIBLE, Some(BOUNDS))
                .expect("all measured reserves fit");
            assert_eq!(attempt.deadline(), arrival + ms(cap));
            assert_eq!(attempt.wait_until(), arrival + ms(cap - 30));
            assert_eq!(attempt.replay_until(), arrival + ms(cap - 10));
            assert!(attempt.try_replay(arrival + ms(cap - 30), ELIGIBLE));
            assert!(!attempt.try_replay(arrival + ms(cap - 30), ELIGIBLE));
            assert!(admission.admit(arrival, ELIGIBLE, Some(BOUNDS)).is_none());
        }
    }

    #[test]
    fn queue_time_and_late_events_preserve_fallback() {
        let arrival = Instant::now();
        let admission =
            || LexicalRepairAdmission::new(arrival, ms(100), arrival + ms(150)).unwrap();
        assert!(
            admission()
                .admit(arrival + ms(21), ELIGIBLE, Some(BOUNDS))
                .is_none()
        );
        let mut attempt = admission().admit(arrival, ELIGIBLE, Some(BOUNDS)).unwrap();
        assert!(!attempt.try_replay(arrival + ms(121), ELIGIBLE));
        assert!(!attempt.try_replay(arrival + ms(119), ELIGIBLE));
    }

    #[test]
    fn unknown_zero_reversed_and_overflowing_timing_are_refused() {
        let arrival = Instant::now() + ms(1);
        assert!(LexicalRepairAdmission::new(arrival, Duration::ZERO, arrival + ms(200)).is_none());
        assert!(LexicalRepairAdmission::new(arrival, ms(100), arrival - ms(1)).is_none());
        assert!(LexicalRepairAdmission::new(arrival, Duration::MAX, arrival + ms(200)).is_none());
        let admission =
            || LexicalRepairAdmission::new(arrival, ms(100), arrival + ms(200)).unwrap();
        assert!(
            admission()
                .admit(arrival - ms(1), ELIGIBLE, Some(BOUNDS))
                .is_none()
        );
        assert!(admission().admit(arrival, ELIGIBLE, None).is_none());
        assert!(
            admission()
                .admit(
                    arrival,
                    ELIGIBLE,
                    Some(LexicalRepairBounds {
                        repair: Duration::ZERO,
                        ..BOUNDS
                    })
                )
                .is_none()
        );
        assert!(
            admission()
                .admit(
                    arrival,
                    ELIGIBLE,
                    Some(LexicalRepairBounds {
                        repair: Duration::MAX,
                        ..BOUNDS
                    })
                )
                .is_none()
        );
        let mut attempt = admission()
            .admit(arrival + ms(1), ELIGIBLE, Some(BOUNDS))
            .unwrap();
        assert!(!attempt.try_replay(arrival, ELIGIBLE));
    }

    #[test]
    fn withheld_authority_or_capacity_never_renews_credit() {
        let arrival = Instant::now();
        let admission =
            || LexicalRepairAdmission::new(arrival, ms(100), arrival + ms(200)).unwrap();
        for refused in [
            LexicalRepairEligibility {
                root_known: false,
                ..ELIGIBLE
            },
            LexicalRepairEligibility {
                pinned: true,
                ..ELIGIBLE
            },
            LexicalRepairEligibility {
                composed: true,
                ..ELIGIBLE
            },
            LexicalRepairEligibility {
                busy: true,
                ..ELIGIBLE
            },
            LexicalRepairEligibility {
                cancelled: true,
                ..ELIGIBLE
            },
        ] {
            let mut first = admission();
            assert!(first.admit(arrival, refused, Some(BOUNDS)).is_none());
            assert!(first.admit(arrival, ELIGIBLE, Some(BOUNDS)).is_none());
            let mut attempt = admission().admit(arrival, ELIGIBLE, Some(BOUNDS)).unwrap();
            assert!(!attempt.try_replay(arrival + ms(100), refused));
            assert!(!attempt.try_replay(arrival + ms(101), ELIGIBLE));
        }
    }
}
