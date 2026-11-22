// SPDX-License-Identifier: Apache-2.0
//! Lane fairness for the one model compute thread. Ordinary jobs (Search
//! Focus, Answer /2) keep the per-index rule: fewer than two jobs on an index,
//! queued or running, before another is admitted. Answer /3 jobs are keyed by
//! (index, attempt, part): one job per key, at most four parts of one attempt
//! on an index, so the parts of one compound Answer can each queue their next
//! call while one Answer still cannot fill the lane. The queue holds at most
//! eight jobs in all.
use airk_core::{
    arbiter::{Task, answer_loop::is_card_task},
    identity::Incarnation,
    index_catalog::IndexId,
};

/// At most this many jobs are queued in all.
pub(super) const MAX_QUEUED: usize = 8;
/// At most this many parts of one Answer attempt hold a job on one index.
pub(super) const MAX_ATTEMPT_PARTS: usize = airk_core::arbiter::answer_loop::MAX_PARTS;

/// The fairness key of one lane job.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaneKey {
    /// The Answer /3 read attempt, or `None` for ordinary jobs.
    pub attempt: Option<Incarnation>,
    /// The part within the attempt (0 for Split and for ordinary jobs).
    pub part: u8,
}

/// The enrollment task that governs `task`: loop card tasks run under the
/// Context stage's existing per-index enrollment, which names the one-turn
/// Context task. The persisted configuration is unchanged.
pub fn enrollment_task(task: Task) -> Task {
    if is_card_task(task) {
        Task::ContextAction
    } else {
        task
    }
}

/// Whether a job keyed `lane` on `index` may queue now, given every queued
/// and running job's (index, key) and the queued count.
pub(super) fn admits(
    holders: impl Iterator<Item = (IndexId, LaneKey)>,
    queued: usize,
    index: IndexId,
    lane: LaneKey,
) -> bool {
    if queued >= MAX_QUEUED {
        return false;
    }
    let on_index: Vec<LaneKey> = holders
        .filter(|(holder, _)| *holder == index)
        .map(|(_, key)| key)
        .collect();
    match lane.attempt {
        None => on_index.len() < 2,
        Some(attempt) => {
            !on_index.contains(&lane)
                && on_index
                    .iter()
                    .filter(|key| key.attempt == Some(attempt))
                    .count()
                    < MAX_ATTEMPT_PARTS
        }
    }
}

/// The queued job to run next: the first whose (index, attempt) differs from
/// the last run's, so neither one index nor one Answer holds the lane while
/// another waits; otherwise the oldest.
pub(super) fn next_position(
    queued: impl Iterator<Item = (IndexId, Option<Incarnation>)>,
    last: Option<(IndexId, Option<Incarnation>)>,
) -> usize {
    queued
        .enumerate()
        .find(|(_, key)| Some(*key) != last)
        .map_or(0, |(position, _)| position)
}

#[cfg(test)]
mod tests;
