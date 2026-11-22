// SPDX-License-Identifier: Apache-2.0
//! Lane fairness for the one model compute thread, with at most eight jobs
//! queued in all.
//! - Ordinary jobs (Search Focus, Answer /2) keep the per-index rule: fewer
//!   than two jobs on an index, queued or running, counting every job on it.
//! - Answer /3 jobs are admitted while fewer than four jobs of any kind
//!   hold the index, across every Answer on it, so one index never takes more than half the queue
//!   and another index always keeps room. Within it, one job per (attempt,
//!   part): a job names every part it serves (a batched call spans parts), and
//!   two jobs of one attempt may not share a part.
use airk_core::{
    arbiter::{Task, answer_loop::is_card_task},
    identity::Incarnation,
    index_catalog::IndexId,
};

/// At most this many jobs are queued in all.
pub(super) const MAX_QUEUED: usize = 8;
/// An Answer /3 job is admitted only while fewer than this many jobs of any
/// kind hold its index, across every attempt.
pub(super) const MAX_INDEX_JOBS: usize = airk_core::arbiter::answer_loop::MAX_PARTS;

/// The fairness key of one lane job.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaneKey {
    /// The Answer /3 read attempt, or `None` for ordinary jobs.
    pub attempt: Option<Incarnation>,
    /// Bit `p` is set for each part `p` the job serves (part 0 is Split).
    /// Ordinary jobs keep 0.
    pub parts: u8,
}
impl LaneKey {
    /// The key of an Answer /3 job serving `parts` of `attempt`.
    pub fn answer(attempt: Incarnation, parts: impl IntoIterator<Item = u8>) -> Self {
        Self {
            attempt: Some(attempt),
            parts: parts
                .into_iter()
                .filter(|part| *part < 8)
                .fold(0, |mask, part| mask | (1 << part)),
        }
    }
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
            lane.parts != 0
                && on_index.len() < MAX_INDEX_JOBS
                && !on_index
                    .iter()
                    .any(|key| key.attempt == Some(attempt) && key.parts & lane.parts != 0)
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
