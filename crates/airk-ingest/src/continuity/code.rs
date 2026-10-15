// SPDX-License-Identifier: Apache-2.0
//! Bounded native declaration descriptors; durable key assignment is a kernel concern.
use airk_core::changes::ChangeDimension;
use airk_core::continuity::{
    CONTINUITY_VERSION, ContinuityBudget, ContinuityDescriptor, ContinuityDraft, ContinuityFamily,
    ContinuitySubject, MAX_CONTINUITY_BYTES,
};
use airk_core::extraction::{ExtractionPayload, ExtractionProfile, LocalChunk, LocalFact};
use airk_core::identity::framed_digest;
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

pub fn describe(
    payload: &ExtractionPayload,
    profile: &ExtractionProfile,
    budget: &mut ContinuityBudget,
) -> Result<ContinuityDraft> {
    ensure!(
        payload.facts.len() <= 10_000 && payload.syntax_identities.len() <= 20_000,
        "code continuity input exceeds record limits"
    );
    let profile_id = profile.identity()?;
    if matches!(
        profile.adapter_id.as_str(),
        "cmake" | "make" | "msbuild" | "github-actions" | "compose"
    ) {
        return Ok(ContinuityDraft::default());
    }
    let identities: BTreeMap<_, _> = payload
        .syntax_identities
        .iter()
        .map(|item| (item.id, item))
        .collect();
    ensure!(
        identities.len() == payload.syntax_identities.len(),
        "duplicate syntax identity"
    );
    let regions: BTreeMap<_, _> = payload.regions.iter().map(|item| (item.id, item)).collect();
    let mut declarations: BTreeMap<_, &LocalFact> = BTreeMap::new();
    let mut duplicate = BTreeSet::new();
    for fact in &payload.facts {
        budget.charge(0)?;
        if fact.kind != "definition" {
            continue;
        }
        let Some(code) = &fact.code else {
            continue;
        };
        if declarations.insert(code.syntax_id, fact).is_some() {
            duplicate.insert(code.syntax_id);
        }
    }
    let mut draft = ContinuityDraft {
        supported: profile.grammar_identity.is_some() || !declarations.is_empty(),
        partial: payload.partial || payload.parse_status == "partial-syntax",
        ..Default::default()
    };
    let mut serialized_bytes = 0usize;
    for (id, fact) in &declarations {
        let code = fact.code.as_ref().expect("grouped declaration");
        let identity = identities
            .get(id)
            .context("declaration syntax identity missing")?;
        ensure!(
            identity.region == fact.region
                && code
                    .span
                    .as_ref()
                    .is_some_and(|span| identity.span.start == span.start_byte
                        && identity.span.end == span.end_byte),
            "declaration identity span differs"
        );
        let (text, effective_profile) = match fact.region {
            Some(id) => {
                let region = regions.get(&id).context("declaration region missing")?;
                (region.text.as_str(), region.profile.identity()?)
            }
            None => (payload.text.as_str(), profile_id),
        };
        ensure!(
            text.get(identity.span.start..identity.span.end).is_some(),
            "invalid declaration span"
        );
        let mut dimensions = BTreeMap::new();
        let mut unsupported = vec![];
        let fingerprints = code
            .fingerprints
            .as_ref()
            .filter(|value| value.version == 1);
        for (dimension, digest) in [
            (
                ChangeDimension::CodeName,
                fingerprints.and_then(|value| value.name),
            ),
            (
                ChangeDimension::CodeSignature,
                fingerprints.and_then(|value| value.signature),
            ),
            (
                ChangeDimension::CodeBody,
                fingerprints.and_then(|value| value.body),
            ),
            (
                ChangeDimension::Documentation,
                fingerprints.and_then(|value| value.documentation),
            ),
        ] {
            if let Some(digest) = digest {
                dimensions.insert(dimension, digest);
            } else {
                unsupported.push(dimension);
            }
        }
        let location = serde_json::to_vec(&fact.location)?;
        dimensions.insert(
            ChangeDimension::Location,
            framed_digest(
                b"code-location/1",
                &[
                    &(identity.span.start as u64).to_be_bytes(),
                    &(identity.span.end as u64).to_be_bytes(),
                    &location,
                ],
            ),
        );
        let provenance_bytes =
            serde_json::to_vec(&(&code.basis, &code.producer, &code.producer_version))?;
        budget.charge(location.len().saturating_add(provenance_bytes.len()))?;
        let name = fingerprints.and_then(|value| value.qualified_name);
        let owner_known = code
            .owner_id
            .is_none_or(|owner| declarations.contains_key(&owner));
        let complete = name.is_some()
            && owner_known
            && !duplicate.contains(id)
            && !payload.partial
            && payload.parse_status != "partial-syntax";
        // Missing dimensions remain unknown, but do not erase independently
        // complete name/owner evidence used for a unique match.
        draft.partial |= !complete || !unsupported.is_empty();
        let descriptor = ContinuityDescriptor {
            algorithm_version: CONTINUITY_VERSION,
            subject: ContinuitySubject::Syntax(*id),
            owner: code.owner_id.map(ContinuitySubject::Syntax),
            region: fact.region,
            family: ContinuityFamily::Code,
            category: identity.syntax_kind.clone(),
            name,
            explicit_identity: None,
            profile: effective_profile,
            provenance: framed_digest(b"code-provenance/1", &[&provenance_bytes]),
            dimensions,
            unsupported_dimensions: unsupported,
            span: identity.span.clone(),
            location: fact.location.clone(),
            complete,
            body_informative: fingerprints
                .is_some_and(|value| value.body_informative && value.body.is_some()),
        };
        serialized_bytes = serialized_bytes
            .checked_add(serde_json::to_vec(&descriptor)?.len())
            .context("code descriptor byte overflow")?;
        ensure!(
            serialized_bytes <= MAX_CONTINUITY_BYTES,
            "code descriptor byte limit exceeded"
        );
        draft.descriptors.push(descriptor);
        draft.chunk_hints.push(LocalChunk {
            span: identity.span.clone(),
            start_line: fact.start_line,
            end_line: fact.end_line,
            location: fact.location.clone(),
            region: fact.region,
        });
    }
    // Outermost declarations partition first; nested methods remain descriptors.
    draft.chunk_hints.sort_by_key(|hint| {
        (
            hint.region,
            hint.span.start,
            std::cmp::Reverse(hint.span.end),
        )
    });
    let mut hints: Vec<LocalChunk> = Vec::new();
    for hint in draft.chunk_hints {
        if let Some(previous) = hints
            .last()
            .filter(|previous| previous.region == hint.region)
            && hint.span.start < previous.span.end
        {
            ensure!(
                hint.span.end <= previous.span.end,
                "crossing declaration chunk spans"
            );
            continue;
        }
        hints.push(hint);
    }
    draft.chunk_hints = hints;
    if draft.partial {
        draft.diagnostics.push("Code continuity is partial where native fingerprints or owner declarations are unavailable".into());
    }
    Ok(draft)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::Artifact;
    use sha2::{Digest, Sha256};

    fn fixture(path: &str, text: &str) -> Result<(ExtractionPayload, ExtractionProfile)> {
        let extracted = crate::documents::extract(path, text.as_bytes())?;
        let artifact = Artifact {
            path: path.into(),
            content_hash: format!("{:x}", Sha256::digest(text.as_bytes())),
            bytes: text.len(),
            chunks: extracted.chunks,
            language: extracted.language,
            extraction_method: extracted.method,
            parse_status: extracted.parse_status,
            facts: extracted.facts,
            retained: None,
        };
        let profile = crate::adapters::legacy::legacy_profile(&artifact)?;
        let payload =
            crate::adapters::legacy::convert_native_legacy(&artifact, text.as_bytes(), &profile)?;
        Ok((payload, profile))
    }

    fn descriptors(text: &str) -> Result<ContinuityDraft> {
        let (payload, profile) = fixture("original.rs", text)?;
        describe(&payload, &profile, &mut ContinuityBudget::default())
    }

    #[test]
    fn independent_dimensions_survive_line_shifts_and_detect_written_changes() -> Result<()> {
        let original = "/// Original contract.\nfn compute(value: i32) -> i32 { value + 1 }\n";
        let before = descriptors(original)?.descriptors.remove(0);
        assert!(before.complete);
        let shifted = descriptors(&format!("\n\n{original}"))?
            .descriptors
            .remove(0);
        for dimension in [
            ChangeDimension::CodeName,
            ChangeDimension::CodeSignature,
            ChangeDimension::CodeBody,
            ChangeDimension::Documentation,
        ] {
            assert_eq!(
                before.dimensions[&dimension],
                shifted.dimensions[&dimension]
            );
        }
        assert_ne!(
            before.dimensions[&ChangeDimension::Location],
            shifted.dimensions[&ChangeDimension::Location]
        );
        for (changed, dimension, preserved) in [
            (
                original.replace("i32", "i64"),
                ChangeDimension::CodeSignature,
                ChangeDimension::CodeBody,
            ),
            (
                original.replace("+ 1", "+ 2"),
                ChangeDimension::CodeBody,
                ChangeDimension::CodeSignature,
            ),
            (
                original.replace("Original", "Revised"),
                ChangeDimension::Documentation,
                ChangeDimension::CodeBody,
            ),
            (
                original.replace("compute", "calculate"),
                ChangeDimension::CodeName,
                ChangeDimension::CodeBody,
            ),
        ] {
            let after = descriptors(&changed)?.descriptors.remove(0);
            assert_ne!(before.dimensions[&dimension], after.dimensions[&dimension]);
            assert_eq!(before.dimensions[&preserved], after.dimensions[&preserved]);
        }
        Ok(())
    }

    #[test]
    fn overloads_keep_distinct_signatures_and_duplicate_names_are_not_assigned_keys() -> Result<()>
    {
        let (payload, profile) = fixture(
            "Original.java",
            "class Original { int run(int x) { return x; } int run(String x) { return 1; } }\n",
        )?;
        let draft = describe(&payload, &profile, &mut ContinuityBudget::default())?;
        let methods: Vec<_> = draft
            .descriptors
            .iter()
            .filter(|item| item.category == "method_declaration")
            .collect();
        assert_eq!(methods.len(), 2);
        assert_eq!(methods[0].name, methods[1].name);
        assert_ne!(
            methods[0].dimensions[&ChangeDimension::CodeSignature],
            methods[1].dimensions[&ChangeDimension::CodeSignature]
        );
        assert_eq!(methods[0].owner, methods[1].owner);
        assert!(methods[0].owner.is_some());
        assert_eq!(
            draft.chunk_hints.len(),
            1,
            "outer class owns maximal chunk hint"
        );
        let duplicate = descriptors("fn repeated() {}\nfn repeated() {}\n")?;
        assert_eq!(duplicate.descriptors.len(), 2);
        assert_eq!(duplicate.descriptors[0].name, duplicate.descriptors[1].name);
        Ok(())
    }

    #[test]
    fn full_utf8_names_docs_and_literals_are_not_display_fingerprints() -> Result<()> {
        let name = format!("original_{}", "x".repeat(1200));
        let text = format!(
            "/// {} final α\nfn {name}() {{ let literal = \"{} tail α\"; }}\n",
            "d".repeat(1400),
            "z".repeat(1400)
        );
        let original = descriptors(&text)?.descriptors.remove(0);
        assert!(original.complete);
        let changed_doc = descriptors(&text.replace("final α", "final β"))?
            .descriptors
            .remove(0);
        assert_ne!(
            original.dimensions[&ChangeDimension::Documentation],
            changed_doc.dimensions[&ChangeDimension::Documentation]
        );
        let changed_literal = descriptors(&text.replace("tail α", "tail β"))?
            .descriptors
            .remove(0);
        assert_ne!(
            original.dimensions[&ChangeDimension::CodeBody],
            changed_literal.dimensions[&ChangeDimension::CodeBody]
        );
        let changed_name = descriptors(&text.replace(&name, &format!("{name}y")))?
            .descriptors
            .remove(0);
        assert_ne!(original.name, changed_name.name);
        Ok(())
    }

    #[test]
    fn shared_framework_node_and_suppressed_owner_remain_conservative() -> Result<()> {
        let (mut payload, profile) =
            fixture("original.ts", "class Original { run() { target(); } }\n")?;
        let method = payload
            .facts
            .iter()
            .find(|fact| fact.name == "run")
            .unwrap()
            .clone();
        let mut cue = method.clone();
        cue.id = payload.facts.iter().map(|fact| fact.id).max().unwrap() + 1;
        cue.kind = "test_declaration".into();
        payload.facts.push(cue);
        let complete = describe(&payload, &profile, &mut ContinuityBudget::default())?;
        assert_eq!(
            complete.descriptors.len(),
            2,
            "framework fact shares syntax subject"
        );
        payload.facts.retain(|fact| fact.name != "Original");
        let partial = describe(&payload, &profile, &mut ContinuityBudget::default())?;
        assert_eq!(partial.descriptors.len(), 1);
        assert!(partial.partial);
        assert!(!partial.descriptors[0].complete);
        assert!(partial.descriptors[0].owner.is_some());
        Ok(())
    }

    #[test]
    fn unsupported_dimension_does_not_erase_known_name_continuity() -> Result<()> {
        let (mut payload, profile) = fixture("original.rs", "fn original() { let count = 23; }\n")?;
        payload.facts[0]
            .code
            .as_mut()
            .unwrap()
            .fingerprints
            .as_mut()
            .unwrap()
            .documentation = None;
        let before = describe(&payload, &profile, &mut ContinuityBudget::default())?;
        assert!(before.partial);
        assert!(before.descriptors[0].complete);
        assert!(
            before.descriptors[0]
                .unsupported_dimensions
                .contains(&ChangeDimension::Documentation)
        );
        let matched =
            crate::continuity::matching::reconcile(&before.descriptors, &before.descriptors);
        assert_eq!(matched.pairs.len(), 1);
        Ok(())
    }

    #[test]
    fn legacy_missing_fingerprints_and_exhausted_budgets_do_not_invent_equality() -> Result<()> {
        let (mut payload, profile) = fixture("original.rs", "fn original() {}\n")?;
        payload.facts[0].code.as_mut().unwrap().fingerprints = None;
        let draft = describe(&payload, &profile, &mut ContinuityBudget::default())?;
        assert!(draft.partial);
        assert!(draft.descriptors[0].name.is_none());
        assert_eq!(draft.descriptors[0].unsupported_dimensions.len(), 4);
        let mut exhausted = ContinuityBudget {
            units: airk_core::continuity::MAX_CONTINUITY_UNITS,
            hash_bytes: 0,
        };
        assert!(describe(&payload, &profile, &mut exhausted).is_err());
        Ok(())
    }

    #[test]
    fn trivial_and_comment_only_bodies_do_not_qualify_rename_fallback() -> Result<()> {
        for text in [
            "fn empty() {}\n",
            "fn comment() { /* alpha bravo charlie delta echo foxtrot */ }\n",
        ] {
            let descriptor = descriptors(text)?.descriptors.remove(0);
            assert!(!descriptor.body_informative);
            assert!(
                descriptor
                    .dimensions
                    .contains_key(&ChangeDimension::CodeBody)
            );
        }
        let descriptor = descriptors("fn original() { let combined = first_value + second_value; consume(combined, final_value); }\n")?.descriptors.remove(0);
        assert!(descriptor.body_informative);
        Ok(())
    }
}
