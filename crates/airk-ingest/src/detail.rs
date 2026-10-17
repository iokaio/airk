// SPDX-License-Identifier: Apache-2.0
//! Reduce optional derived evidence before building continuity or durable objects.
use airk_core::extraction::{ExtractionPayload, IndexDetail, LocalRelationshipTarget};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn select(payload: &mut ExtractionPayload, detail: IndexDetail) {
    if detail == IndexDetail::Detailed {
        return;
    }
    payload
        .facts
        .retain(|fact| detail == IndexDetail::Balanced && !matches!(fact.kind.as_str(), "call"));
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
        if edge.kind == "call" {
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
    payload.continuity.clear();
    payload.continuity_coverage.clear();
}
