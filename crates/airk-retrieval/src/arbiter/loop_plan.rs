// SPDX-License-Identifier: Apache-2.0
//! Plan task policy: which proposed sub-needs a part needs. The last card is
//! the reference "no further sub-need"; selection takes the proposals scored
//! above it, at least one and at most four.
use airk_core::{identity::Sha256Digest, native_loop::CardSegment};
use anyhow::{Result, ensure};

pub const QUESTION: &str = "Which listed sub-need is most necessary to answer this part? The last option means no further sub-need is needed.";

/// The reference card's exact text, the last card of every Plan roster.
pub const REFERENCE: &str = "no further sub-need";

pub fn policy_digest() -> Result<Sha256Digest> {
    super::loop_cards::framed_policy(
        b"airk.arbiter.loop-plan-task-policy/1",
        include_bytes!("loop_plan.rs"),
        QUESTION,
    )
}

/// At least one proposal, then exactly one reference card, last.
pub(super) fn validate_roster(segment: &CardSegment) -> Result<()> {
    let Some((reference, proposals)) = segment.cards.split_last() else {
        anyhow::bail!("Plan roster is empty");
    };
    ensure!(
        !proposals.is_empty()
            && reference.text == REFERENCE
            && proposals
                .iter()
                .all(|card| card.text.starts_with("need ") && card.text != REFERENCE),
        "Plan roster must be proposed needs followed by the reference card"
    );
    Ok(())
}
