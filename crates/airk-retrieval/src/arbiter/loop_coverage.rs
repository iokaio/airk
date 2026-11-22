// SPDX-License-Identifier: Apache-2.0
//! Coverage task policy: the verdict for each sub-need given its evidence.
//! The first card is "covered"; the others read more at a location or search
//! a term. Batched calls carry several sub-needs, each its own segment, and
//! each segment is decided from its own cards' scores.
use airk_core::{
    arbiter::answer_loop::CardClass, identity::Sha256Digest, native_loop::CardSegment,
};
use anyhow::{Result, ensure};

pub const QUESTION: &str = "For each sub-need s, which card is the right verdict for that sub-need given only its shown evidence? Cards of different sub-needs are independent.";

pub fn policy_digest() -> Result<Sha256Digest> {
    super::loop_cards::framed_policy(
        b"airk.arbiter.loop-coverage-task-policy/1",
        include_bytes!("loop_coverage.rs"),
        QUESTION,
    )
}

/// Covered first, then only alternatives.
pub(super) fn validate_roster(segment: &CardSegment) -> Result<()> {
    ensure!(
        segment.cards.len() >= 2
            && segment.cards[0].class == CardClass::Covered
            && segment.cards[1..].iter().all(|card| matches!(
                card.class,
                CardClass::ReadMoreHere | CardClass::SearchForTerm
            )),
        "Coverage roster must be covered first, then read-more or search cards"
    );
    Ok(())
}
