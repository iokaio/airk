// SPDX-License-Identifier: Apache-2.0
//! Follow-up task policy: which proposed read best closes a sub-need's gap.
//! Every card is a query or a structural hop built by kernel code from terms
//! found in what was read; batched calls carry several gaps.
use airk_core::{identity::Sha256Digest, native_loop::CardSegment};
use anyhow::{Result, ensure};

pub const QUESTION: &str = "Which listed read best closes the gap of its sub-need?";

pub fn policy_digest() -> Result<Sha256Digest> {
    super::loop_cards::framed_policy(
        b"airk.arbiter.loop-follow-up-task-policy/1",
        include_bytes!("loop_follow_up.rs"),
        QUESTION,
    )
}

/// At least two proposed reads; Core has bound every class to this stage.
pub(super) fn validate_roster(segment: &CardSegment) -> Result<()> {
    ensure!(
        segment.cards.len() >= 2,
        "a one-candidate follow-up roster is never scored"
    );
    Ok(())
}
