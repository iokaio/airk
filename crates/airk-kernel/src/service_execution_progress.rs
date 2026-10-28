// SPDX-License-Identifier: Apache-2.0
use airk_core::{
    readiness::IndexProgress,
    service_progress::{WorkPhase, WorkProgress},
    service_resources::WorkAllowance,
};
use std::time::{Duration, Instant};
#[cfg(test)]
#[path = "service_execution_progress_tests.rs"]
mod tests;

pub(super) struct Tracker {
    snapshot: WorkProgress,
    started: Instant,
    phase_started: Instant,
    advanced: Instant,
    has_advanced: bool,
    paused_since: Option<Instant>,
    paused_duration: Duration,
    waiting: bool,
    stalled_since: Option<Instant>,
    finished: Option<Instant>,
}
fn ms(duration: Duration) -> u64 {
    duration.as_millis().min(u64::MAX as u128) as u64
}
impl Tracker {
    pub(super) fn new(grant: &WorkAllowance, now: Instant) -> Self {
        Self {
            snapshot: WorkProgress {
                job: grant.job,
                grant: grant.id,
                sequence: 0,
                phase: WorkPhase::Starting,
                paused: false,
                stalled: false,
                elapsed_ms: 0,
                phase_elapsed_ms: 0,
                phase_totals_ms: Default::default(),
                last_advance_age_ms: None,
                unpaused_idle_ms: 0,
                captured_files: 0,
                extracted_files: 0,
                prepared_changes: 0,
                committed_changes: 0,
                publications: 0,
                verified_objects: 0,
                indexed_files: None,
                total_files: None,
                completed_semantic_units: None,
                publication: None,
            },
            started: now,
            phase_started: now,
            advanced: now,
            has_advanced: false,
            paused_since: None,
            paused_duration: Duration::ZERO,
            waiting: false,
            stalled_since: None,
            finished: None,
        }
    }
    fn changed(&mut self) {
        self.snapshot.sequence = self.snapshot.sequence.saturating_add(1);
    }
    pub(super) fn watchdog(&mut self, now: Instant) -> (bool, bool) {
        if self.snapshot.phase == WorkPhase::Finished {
            return (false, false);
        }
        let snapshot = self.snapshot(now);
        if self.stalled_since.is_none()
            && !snapshot.paused
            && !self.waiting
            && snapshot.unpaused_idle_ms >= 600_000
        {
            self.stalled_since = Some(now);
            self.snapshot.stalled = true;
            self.changed();
        }
        (
            self.stalled_since.is_some(),
            self.stalled_since
                .is_some_and(|since| now.duration_since(since) >= Duration::from_secs(30)),
        )
    }
    fn advance(&mut self, now: Instant) {
        self.advanced = now;
        self.has_advanced = true;
        self.paused_duration = Duration::ZERO;
        if self.paused_since.is_some() {
            self.paused_since = Some(now);
        }
        self.changed();
    }
    pub(super) fn phase(&mut self, phase: WorkPhase, now: Instant) {
        if self.finished.is_some() {
            return;
        }
        if self.snapshot.phase != phase {
            let prior = self
                .snapshot
                .phase_totals_ms
                .entry(self.snapshot.phase)
                .or_default();
            *prior = prior.saturating_add(ms(now.duration_since(self.phase_started)));
            self.snapshot.phase = phase;
            self.phase_started = now;
            self.changed();
            if phase == WorkPhase::Finished {
                self.finished = Some(now);
            }
        }
    }
    pub(super) fn pause(&mut self, paused: bool, now: Instant) {
        if self.finished.is_some() {
            return;
        }
        self.snapshot.paused = paused;
        self.blocked(paused || self.waiting, now);
    }
    pub(super) fn wait(&mut self, waiting: bool, now: Instant) {
        self.waiting = waiting;
        self.blocked(waiting || self.snapshot.paused, now);
    }
    fn blocked(&mut self, paused: bool, now: Instant) {
        if paused && self.paused_since.is_none() {
            self.paused_since = Some(now);
            self.changed();
        }
        if !paused && let Some(since) = self.paused_since.take() {
            self.paused_duration += now.duration_since(since);
            self.changed();
        }
    }
    pub(super) fn capture(&mut self, now: Instant) {
        self.snapshot.captured_files = self.snapshot.captured_files.saturating_add(1);
        self.advance(now);
    }
    pub(super) fn extracted(&mut self, now: Instant) {
        self.snapshot.extracted_files = self.snapshot.extracted_files.saturating_add(1);
        self.advance(now);
    }
    pub(super) fn prepared(&mut self, changes: u64, now: Instant) {
        self.snapshot.prepared_changes = self.snapshot.prepared_changes.saturating_add(changes);
        self.advance(now);
    }
    pub(super) fn semantic_prepared(&mut self, now: Instant) {
        // Real candidate work advances the watchdog without claiming publication.
        self.advance(now);
    }
    pub(super) fn committed(&mut self, changes: u64, now: Instant) {
        self.snapshot.committed_changes = self.snapshot.committed_changes.saturating_add(changes);
        self.snapshot.publications = self.snapshot.publications.saturating_add(1);
        self.advance(now);
    }
    pub(super) fn verified(&mut self, objects: u64, now: Instant) {
        self.snapshot.verified_objects = self.snapshot.verified_objects.saturating_add(objects);
        if objects > 0 {
            self.advance(now);
        }
    }
    pub(super) fn indexed(&mut self, event: &IndexProgress, now: Instant) {
        let phase = match event.phase {
            "scan_started" => WorkPhase::Discovering,
            "enrichment_started" | "enrichment_prepared" => WorkPhase::Embedding,
            "enrichment_committed" => WorkPhase::Publishing,
            _ => WorkPhase::Preparing,
        };
        self.phase(phase, now);
        if event.phase.starts_with("base_") {
            self.snapshot.indexed_files = Some(event.committed_artifacts);
            self.snapshot.total_files = event.total_artifacts;
        }
        if let Some(units) = event.completed_units {
            self.snapshot.completed_semantic_units = Some(units);
        }
        if let Some(revision) = event
            .revision
            .as_ref()
            .and_then(|revision| revision.parse().ok())
            && self.snapshot.publication != Some(revision)
        {
            self.snapshot.publication = Some(revision);
            self.changed();
        }
    }
    pub(super) fn snapshot(&self, now: Instant) -> WorkProgress {
        let now = self.finished.unwrap_or(now);
        let mut value = self.snapshot.clone();
        value.elapsed_ms = ms(now.duration_since(self.started));
        value.phase_elapsed_ms = ms(now.duration_since(self.phase_started));
        let current = value.phase_totals_ms.entry(value.phase).or_default();
        *current = current.saturating_add(value.phase_elapsed_ms);
        value.last_advance_age_ms = self
            .has_advanced
            .then(|| ms(now.duration_since(self.advanced)));
        value.unpaused_idle_ms = ms(self
            .paused_since
            .unwrap_or(now)
            .duration_since(self.advanced)
            .saturating_sub(self.paused_duration));
        value
    }
}
