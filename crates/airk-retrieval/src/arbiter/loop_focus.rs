// SPDX-License-Identifier: Apache-2.0
//! Batched Focus for the Answer loop: the ranking rule of one batch whose
//! pairs each carry their own sub-need question, and the identity a batch
//! receipt records. The pair itself is scored under the Focus pair policy
//! (`compiler::focus`); this owner adds the batch semantics on top, so a
//! change to batched ranking moves the batch identity, not the pair policy
//! or the closed CPU Focus row.
use airk_core::identity::{Sha256Digest, framed_digest};
use anyhow::{Result, ensure};

/// The identity a batched Focus receipt records: the Focus pair policy the
/// pairs were scored under, this module's ranking rule and the Core loop
/// semantics (batch receipt, per-candidate questions and native inputs).
pub fn policy_digest(pair_policy: Sha256Digest) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"airk.arbiter.loop-focus-batch-policy/1",
        &[
            pair_policy.as_bytes(),
            super::loop_cards::source_lf(include_bytes!("loop_focus.rs"))?.as_bytes(),
            airk_core::arbiter::answer_loop::loop_semantic_policy_digest()?.as_bytes(),
        ],
    ))
}

/// The ranked permutation of a complete batch: the scored prefix by
/// relevance, highest first and ties to the lower index, then the unscored
/// tail in canonical order. `scored` holds (candidate index, relevance) for
/// exactly the first `scored.len()` candidates of `count`.
pub fn rank(scored: &[(u16, f32)], count: usize) -> Result<Vec<u16>> {
    ensure!(
        scored.len() <= count
            && scored
                .iter()
                .enumerate()
                .all(|(position, (index, relevance))| {
                    usize::from(*index) == position && relevance.is_finite()
                }),
        "a Focus batch ranks exactly its scored prefix"
    );
    let mut ordered = scored.to_vec();
    ordered.sort_by(|(left_index, left), (right_index, right)| {
        right.total_cmp(left).then(left_index.cmp(right_index))
    });
    Ok(ordered
        .into_iter()
        .map(|(index, _)| index)
        .chain((scored.len()..count).map(|index| index as u16))
        .collect())
}

#[cfg(test)]
mod tests;
