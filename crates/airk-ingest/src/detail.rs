// SPDX-License-Identifier: Apache-2.0
//! Reduce optional derived evidence before building continuity or durable objects.
use airk_core::extraction::{
    ExtractionPayload, IndexDetail, LocalRelationshipTarget, LocalSyntaxId,
};
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn select(payload: &mut ExtractionPayload, detail: IndexDetail) -> Result<()> {
    payload.facts.retain(|fact| detail.retains_fact(&fact.kind));
    if detail == IndexDetail::AgentLean {
        for fact in &mut payload.facts {
            if let Some(code) = &mut fact.code {
                code.fingerprints = None;
            }
        }
    }
    // Fact IDs also address relationships and document continuity. Renumber
    // before continuity generation so returned fact ordinals stay consistent.
    let ids: BTreeMap<_, _> = payload
        .facts
        .iter()
        .enumerate()
        .map(|(ordinal, fact)| (fact.id, ordinal as u32))
        .collect();
    for fact in &mut payload.facts {
        fact.id = ids[&fact.id];
        fact.owner = fact.owner.and_then(|id| ids.get(&id).copied());
    }
    payload.relationships.retain_mut(|edge| {
        if detail != IndexDetail::Detailed && edge.kind == "call" {
            return false;
        }
        let Some(source) = ids.get(&edge.source) else {
            return false;
        };
        edge.source = *source;
        if let LocalRelationshipTarget::Local { id } = &mut edge.target {
            let Some(target) = ids.get(id) else {
                return false;
            };
            *id = *target;
        }
        true
    });
    let syntax: BTreeSet<_> = payload
        .facts
        .iter()
        .filter_map(|fact| fact.code.as_ref())
        .flat_map(|code| std::iter::once(code.syntax_id).chain(code.owner_id))
        .collect();
    payload
        .syntax_identities
        .retain(|identity| syntax.contains(&identity.id));
    // Optional producers can omit intermediate nodes before conversion. Stable
    // ordering makes the selected payload independent of those discarded facts.
    // Syntax identities and fact ordinals are separate namespaces: several facts
    // may share a node, and a non-emitted node can still own a selected fact.
    payload.syntax_identities.sort_by(|left, right| {
        (
            left.region,
            left.span.start,
            left.span.end,
            &left.syntax_kind,
            left.id,
        )
            .cmp(&(
                right.region,
                right.span.start,
                right.span.end,
                &right.syntax_kind,
                right.id,
            ))
    });
    let mut syntax_ids = BTreeMap::new();
    for (ordinal, identity) in payload.syntax_identities.iter_mut().enumerate() {
        let id = LocalSyntaxId(u32::try_from(ordinal).context("syntax identity overflow")?);
        ensure!(
            syntax_ids.insert(identity.id, id).is_none(),
            "duplicate selected syntax identity"
        );
        identity.id = id;
    }
    for code in payload
        .facts
        .iter_mut()
        .filter_map(|fact| fact.code.as_mut())
    {
        code.syntax_id = *syntax_ids
            .get(&code.syntax_id)
            .context("selected fact has no syntax identity")?;
        code.owner_id = code
            .owner_id
            .map(|id| {
                syntax_ids
                    .get(&id)
                    .copied()
                    .context("selected fact has no owner syntax identity")
            })
            .transpose()?;
    }
    payload.continuity.clear();
    payload.continuity_coverage.clear();
    Ok(())
}

#[cfg(test)]
mod tests;
