// SPDX-License-Identifier: Apache-2.0
//! Lane fairness for the one model compute thread, with at most eight jobs
//! queued in all.
//! - Every index holds fewer than four jobs of any kind, queued or running,
//!   so one index never takes more than half the queue.
//! - One Answer /3 attempt holds at most three of them, so a different
//!   Answer or an ordinary job always has a slot on that index: a compound
//!   request cannot monopolize it (plan 12.1 item 1).
//! - Within an attempt, one job per part: a job names every part it serves (a
//!   batched call spans parts), and two jobs of one attempt may not share one.
//! - Ordinary jobs (Search Focus, Answer /2) also keep fewer than two
//!   ordinary jobs per index, as before.
use airk_core::{
    arbiter::{Task, answer_loop::is_card_task},
    identity::Incarnation,
    index_catalog::IndexId,
};

/// At most this many jobs are queued in all.
pub(super) const MAX_QUEUED: usize = 8;
/// Fewer than this many jobs of any kind hold one index.
pub(super) const MAX_INDEX_JOBS: usize = airk_core::arbiter::answer_loop::MAX_PARTS;
/// One Answer attempt holds at most this many jobs on one index.
pub(super) const MAX_ATTEMPT_INDEX_JOBS: usize = MAX_INDEX_JOBS - 1;
/// Ordinary jobs hold fewer than this many slots on one index.
pub(super) const MAX_ORDINARY_INDEX_JOBS: usize = 2;

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
    if on_index.len() >= MAX_INDEX_JOBS {
        return false;
    }
    let same =
        |attempt: Option<Incarnation>| on_index.iter().filter(move |key| key.attempt == attempt);
    match lane.attempt {
        None => same(None).count() < MAX_ORDINARY_INDEX_JOBS,
        Some(attempt) => {
            lane.parts != 0
                && same(Some(attempt)).count() < MAX_ATTEMPT_INDEX_JOBS
                && !same(Some(attempt)).any(|key| key.parts & lane.parts != 0)
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
