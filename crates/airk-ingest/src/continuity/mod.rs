// SPDX-License-Identifier: Apache-2.0
//! Bounded path-free descriptors and conservative matching candidates.
pub mod code;
pub mod documents;
pub mod matching;

/// Enrich only a verified raw payload; the shared budget covers both families.
pub fn prepare(
    payload: &mut airk_core::extraction::ExtractionPayload,
    profile: &airk_core::extraction::ExtractionProfile,
) -> anyhow::Result<()> {
    prepare_with_prose_text(payload, profile, None)
}

pub(crate) fn prepare_with_prose_text(
    payload: &mut airk_core::extraction::ExtractionPayload,
    profile: &airk_core::extraction::ExtractionProfile,
    prose_text: Option<&str>,
) -> anyhow::Result<()> {
    use airk_core::continuity::*;
    use airk_core::extraction::IndexDetail;
    let detail = IndexDetail::from_profile(profile)?;
    crate::detail::select(payload, detail);
    if detail == IndexDetail::Essential {
        for family in [ContinuityFamily::Code, ContinuityFamily::Documents] {
            payload.continuity_coverage.insert(
                family,
                ContinuityCoverage {
                    supported: false,
                    complete: false,
                },
            );
        }
        payload.chunks = crate::chunking::partition(payload, &[])?;
        return payload.validate();
    }
    if payload.parse_status.starts_with("partial-text-only:") {
        payload.partial = true;
        for family in [ContinuityFamily::Code, ContinuityFamily::Documents] {
            payload.continuity_coverage.insert(
                family,
                ContinuityCoverage {
                    supported: false,
                    complete: false,
                },
            );
        }
        payload.chunks = crate::chunking::partition(payload, &[])?;
        return payload.validate();
    }
    let mut budget = ContinuityBudget::default();
    let code = code::describe(payload, profile, &mut budget)?;
    let documents = if detail == IndexDetail::Detailed {
        documents::describe_with_prose_text(payload, profile, &mut budget, prose_text)?
    } else {
        ContinuityDraft::default()
    };
    let mut hints = Vec::new();
    let mut bytes = 2usize;
    payload.continuity.clear();
    for (family, draft) in [
        (ContinuityFamily::Code, code),
        (ContinuityFamily::Documents, documents),
    ] {
        let mut complete = draft.supported && !draft.partial;
        for descriptor in draft.descriptors {
            let size = serde_json::to_vec(&descriptor)?.len() + 1;
            if bytes.saturating_add(size) > MAX_CONTINUITY_BYTES
                || payload.continuity.len() >= MAX_CONTINUITY_UNITS
            {
                complete = false;
                break;
            }
            bytes += size;
            payload.continuity.push(descriptor);
        }
        payload.continuity_coverage.insert(
            family,
            ContinuityCoverage {
                supported: draft.supported,
                complete,
            },
        );
        hints.extend(draft.chunk_hints);
        for diagnostic in draft.diagnostics {
            if payload.diagnostics.len() < 256 {
                payload.diagnostics.push(diagnostic);
            }
        }
    }
    let subjects: std::collections::BTreeSet<_> =
        payload.continuity.iter().map(|item| item.subject).collect();
    for item in &mut payload.continuity {
        if item.owner.is_some_and(|owner| !subjects.contains(&owner)) {
            item.owner = None;
            item.complete = false;
            if let Some(coverage) = payload.continuity_coverage.get_mut(&item.family) {
                coverage.complete = false;
            }
        }
    }
    payload.chunks = crate::chunking::partition(payload, &hints)?;
    payload.validate()
}

pub fn prepare_native(
    artifact: &airk_core::Artifact,
    bytes: &[u8],
    profile: &airk_core::extraction::ExtractionProfile,
) -> anyhow::Result<airk_core::extraction::ExtractionPayload> {
    let mut payload = crate::adapters::legacy::convert_native_legacy(artifact, bytes, profile)?;
    prepare(&mut payload, profile)?;
    Ok(payload)
}
