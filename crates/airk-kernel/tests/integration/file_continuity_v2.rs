// SPDX-License-Identifier: Apache-2.0
//! Original pure matching fixtures; public publication coverage is added at wiring.
#[path = "../../src/reconciliation.rs"]
mod reconciliation;

use airk_core::{
    changes::{ChangeCause, ContinuityBasis, FileStamps, FileState, ResolutionState},
    identity::{ArtifactId, Incarnation, Sha256Digest, framed_digest},
    publication::{InventoryFile, InventoryV2},
};
use airk_ingest::{
    adapters::registry::AdapterRegistry,
    observations::{DisappearanceCause, ScanObservation},
};
use anyhow::Result;
use std::collections::BTreeMap;

const TEXT: &str =
    "An original detailed fixture records eight separate meaningful words for continuity.\n";
fn producer() -> Sha256Digest {
    Sha256Digest::from_bytes([7; 32])
}

fn fixture_reconcile(
    previous: Option<&InventoryV2>,
    next: &mut InventoryV2,
    observation: Option<&ScanObservation>,
    producer: Sha256Digest,
    informative: impl FnMut(&InventoryFile) -> Result<bool>,
) -> Result<reconciliation::ReconciledFiles> {
    let domains = next
        .files
        .keys()
        .map(|path| (path.clone(), producer))
        .collect();
    reconciliation::reconcile(previous, next, observation, &domains, informative)
}

fn inventory(generation: u64, files: &[(&str, &str)]) -> Result<InventoryV2> {
    let profile = AdapterRegistry::baseline()
        .get("text")
        .unwrap()
        .profile
        .clone();
    let profile_id = profile.identity()?;
    let mut inventory = InventoryV2 {
        embedding_pipeline_digest: None,
        extraction_policy_digest: None,
        version: 2,
        incarnation: Incarnation::from_bytes([1; 16]),
        generation,
        collection: "default".into(),
        root: "/original/root".into(),
        roots: BTreeMap::new(),
        lineage_digest: Sha256Digest::from_bytes([0; 32]),
        profiles: BTreeMap::from([(profile_id, profile)]),
        files: BTreeMap::new(),
        retrieval_specification: "original-fixture/1".into(),
        embedding_model: None,
        extractor_version: airk_core::EXTRACTOR_VERSION.into(),
        analysis: None,
        imported_facts: vec![],
        imported_relationships: vec![],
    };
    for (number, (path, text)) in files.iter().enumerate() {
        let source_hash = framed_digest(b"original-source", &[text.as_bytes()]);
        let mut identity = [0; 16];
        identity[..8].copy_from_slice(&(number as u64 + 1).to_be_bytes());
        identity[8..].copy_from_slice(&generation.to_be_bytes());
        inventory.files.insert(
            (*path).into(),
            InventoryFile {
                continuity: Vec::new(),
                continuity_coverage: Default::default(),
                state: FileState {
                    artifact_id: ArtifactId::from_bytes(identity),
                    stamps: FileStamps {
                        first_generation: generation,
                        path_generation: generation,
                        content_generation: generation,
                        evidence_generation: generation,
                    },
                    source_hash,
                    profile_id,
                    payload_digest: source_hash,
                    evidence_digest: source_hash,
                    root: inventory.root.clone(),
                    path: (*path).into(),
                },
                source_bytes: text.len(),
                payload_bytes: text.len(),
                extracted_bytes: text.len(),
                chunks: 1,
                indexed_lines: 1,
                facts: 0,
                raw_source: true,
                language: "text".into(),
                extraction_method: "utf8".into(),
                parse_status: "fixture".into(),
            },
        );
    }
    fixture_reconcile(None, &mut inventory, None, producer(), |_| Ok(true))?;
    Ok(inventory)
}

fn observation(
    previous: &InventoryV2,
    next: &InventoryV2,
    cause: DisappearanceCause,
) -> ScanObservation {
    ScanObservation {
        complete: true,
        roots: std::iter::once((String::new(), next.root.clone()))
            .chain(next.roots.clone())
            .collect(),
        disappeared: previous
            .files
            .keys()
            .filter(|path| !next.files.contains_key(*path))
            .map(|path| (path.clone(), cause))
            .collect(),
        usage: Default::default(),
    }
}

#[test]
fn unique_move_preserves_handle_and_content_but_rebinds_evidence_and_path_stamp() -> Result<()> {
    let old = inventory(1, &[("a.txt", TEXT)])?;
    let mut next = inventory(2, &[("b.txt", TEXT)])?;
    let observed = observation(&old, &next, DisappearanceCause::DeletedObserved);
    let result = fixture_reconcile(Some(&old), &mut next, Some(&observed), producer(), |_| {
        Ok(true)
    })?;
    let before = &old.files["a.txt"].state;
    let after = &next.files["b.txt"].state;
    assert_eq!(after.artifact_id, before.artifact_id);
    assert_eq!(after.stamps.first_generation, 1);
    assert_eq!(after.stamps.content_generation, 1);
    assert_eq!(after.stamps.path_generation, 2);
    assert_ne!(after.evidence_digest, before.evidence_digest);
    assert_eq!(result.events.len(), 1);
    assert_eq!(
        result.events[0].basis,
        ContinuityBasis::UniqueCompatibleContent
    );
    assert_eq!(result.resolvers[0].state, ResolutionState::ExactMove);
    assert_eq!(
        result.resolvers[0].from.citation_path.as_deref(),
        Some("a.txt")
    );
    assert_eq!(
        result.resolvers[0]
            .to
            .as_ref()
            .unwrap()
            .citation_path
            .as_deref(),
        Some("b.txt")
    );
    let mut third = inventory(3, &[("c.txt", TEXT)])?;
    let observed = observation(&next, &third, DisappearanceCause::DeletedObserved);
    let moved = fixture_reconcile(Some(&next), &mut third, Some(&observed), producer(), |_| {
        Ok(true)
    })?;
    assert_eq!(third.files["c.txt"].state.artifact_id, before.artifact_id);
    assert_eq!(moved.resolvers[0].from.path, "b.txt");
    let mut deleted = inventory(4, &[])?;
    let observed = observation(&third, &deleted, DisappearanceCause::DeletedObserved);
    let removed = fixture_reconcile(
        Some(&third),
        &mut deleted,
        Some(&observed),
        producer(),
        |_| Ok(true),
    )?;
    assert_eq!(removed.resolvers[0].artifact_id, before.artifact_id);
    assert_eq!(removed.resolvers[0].state, ResolutionState::DeletedObserved);
    Ok(())
}

#[test]
fn duplicate_copy_tiny_and_uninformative_sources_never_transfer_handles() -> Result<()> {
    for (old_files, new_files, informative) in [
        (
            vec![("a.txt", TEXT), ("copy.txt", TEXT)],
            vec![("b.txt", TEXT), ("copy.txt", TEXT)],
            true,
        ),
        (
            vec![("a.txt", TEXT)],
            vec![("b.txt", TEXT), ("copy.txt", TEXT)],
            true,
        ),
        (vec![("a.txt", "")], vec![("b.txt", "")], true),
        (vec![("a.txt", "tiny")], vec![("b.txt", "tiny")], true),
        (vec![("a.txt", TEXT)], vec![("b.txt", TEXT)], false),
    ] {
        let old = inventory(1, &old_files)?;
        let mut next = inventory(2, &new_files)?;
        let observed = observation(&old, &next, DisappearanceCause::DeletedObserved);
        let result = fixture_reconcile(Some(&old), &mut next, Some(&observed), producer(), |_| {
            Ok(informative)
        })?;
        assert_ne!(
            old.files["a.txt"].state.artifact_id,
            next.files["b.txt"].state.artifact_id
        );
        assert!(
            result
                .resolvers
                .iter()
                .all(|resolver| resolver.state != ResolutionState::ExactMove)
        );
        assert!(
            result
                .resolvers
                .iter()
                .any(|resolver| resolver.basis == ContinuityBasis::EquivalentContentCandidate)
        );
    }
    Ok(())
}

#[test]
fn exclusions_missing_observation_and_rebound_alias_do_not_prove_moves() -> Result<()> {
    for cause in [
        DisappearanceCause::ExcludedFromDiscovery,
        DisappearanceCause::Oversized,
        DisappearanceCause::ExtractionFailed,
        DisappearanceCause::Unknown,
    ] {
        let old = inventory(1, &[("a.txt", TEXT)])?;
        let mut next = inventory(2, &[("b.txt", TEXT)])?;
        let observed = observation(&old, &next, cause);
        let result = fixture_reconcile(Some(&old), &mut next, Some(&observed), producer(), |_| {
            panic!("excluded source cannot qualify")
        })?;
        assert!(
            result
                .resolvers
                .iter()
                .all(|resolver| resolver.state == ResolutionState::RemovedFromIndex)
        );
        assert_ne!(
            old.files["a.txt"].state.artifact_id,
            next.files["b.txt"].state.artifact_id
        );
    }
    let old = inventory(1, &[("a.txt", TEXT)])?;
    let mut next = inventory(2, &[])?;
    let result = fixture_reconcile(Some(&old), &mut next, None, producer(), |_| Ok(true))?;
    assert_eq!(result.events[0].cause, ChangeCause::RemovedUnknown);
    Ok(())
}

#[test]
fn same_path_edits_swaps_and_profile_changes_preserve_only_same_path_identity() -> Result<()> {
    let other =
        "A different original fixture contains sufficient meaningful content for another file.";
    let old = inventory(1, &[("a.txt", TEXT), ("b.txt", other)])?;
    let mut next = inventory(2, &[("a.txt", other), ("b.txt", TEXT)])?;
    let observed = observation(&old, &next, DisappearanceCause::DeletedObserved);
    let result = fixture_reconcile(Some(&old), &mut next, Some(&observed), producer(), |_| {
        panic!("same paths win")
    })?;
    assert_eq!(
        next.files["a.txt"].state.artifact_id,
        old.files["a.txt"].state.artifact_id
    );
    assert_eq!(
        next.files["b.txt"].state.artifact_id,
        old.files["b.txt"].state.artifact_id
    );
    assert!(result.resolvers.is_empty());
    assert_eq!(result.events.len(), 2);
    let mut changed_profile = inventory(2, &[("a.txt", TEXT)])?;
    changed_profile
        .files
        .get_mut("a.txt")
        .unwrap()
        .state
        .profile_id = Sha256Digest::from_bytes([5; 32]);
    let result = fixture_reconcile(Some(&old), &mut changed_profile, None, producer(), |_| {
        Ok(true)
    })?;
    assert_eq!(result.events[0].cause, ChangeCause::ProfileChanged);
    Ok(())
}

#[test]
fn overflow_is_explicit_and_reconciliation_rejects_foreign_or_partial_scan() -> Result<()> {
    let names = (0..4100)
        .map(|number| format!("file-{number}.txt"))
        .collect::<Vec<_>>();
    let files = names
        .iter()
        .map(|name| (name.as_str(), TEXT))
        .collect::<Vec<_>>();
    let old = inventory(1, &files)?;
    let mut next = inventory(2, &[])?;
    let mut observed = observation(&old, &next, DisappearanceCause::DeletedObserved);
    let result = fixture_reconcile(Some(&old), &mut next, Some(&observed), producer(), |_| {
        Ok(true)
    })?;
    assert!(!result.complete);
    assert_eq!(result.events.len() + result.resolvers.len(), 8000);
    observed.complete = false;
    assert!(
        fixture_reconcile(Some(&old), &mut next, Some(&observed), producer(), |_| Ok(
            true
        ))
        .is_err()
    );
    next.incarnation = Incarnation::from_bytes([2; 16]);
    assert!(fixture_reconcile(Some(&old), &mut next, None, producer(), |_| Ok(true)).is_err());
    Ok(())
}

#[test]
fn alias_continuity_requires_both_bindings_and_path_recreation_gets_a_new_handle() -> Result<()> {
    let mut old = inventory(1, &[("@docs/a.txt", TEXT)])?;
    old.roots
        .insert("docs".into(), "/original/documents".into());
    let file = old.files.get_mut("@docs/a.txt").unwrap();
    file.state.root = "/original/documents".into();
    file.state.path = "a.txt".into();
    let mut next = inventory(2, &[("b.txt", TEXT)])?;
    next.roots = old.roots.clone();
    let observed = observation(&old, &next, DisappearanceCause::DeletedObserved);
    let result = fixture_reconcile(Some(&old), &mut next, Some(&observed), producer(), |_| {
        Ok(true)
    })?;
    assert_eq!(result.resolvers[0].state, ResolutionState::ExactMove);
    let mut removed_alias = inventory(2, &[("b.txt", TEXT)])?;
    let observed = observation(&old, &removed_alias, DisappearanceCause::AliasRemoved);
    let result = fixture_reconcile(
        Some(&old),
        &mut removed_alias,
        Some(&observed),
        producer(),
        |_| panic!("removed alias is not a move"),
    )?;
    assert_eq!(result.resolvers[0].state, ResolutionState::RemovedFromIndex);
    assert_eq!(
        result
            .events
            .last()
            .unwrap()
            .before
            .as_ref()
            .unwrap()
            .citation_path
            .as_deref(),
        Some("@docs/a.txt")
    );
    assert_eq!(
        result.resolvers[0].from.citation_path.as_deref(),
        Some("@docs/a.txt")
    );
    assert_eq!(
        result.events.last().unwrap().cause,
        ChangeCause::AliasRemoved
    );
    let mut rebound = inventory(2, &[("@docs/a.txt", TEXT)])?;
    rebound
        .roots
        .insert("docs".into(), "/replacement/documents".into());
    let file = rebound.files.get_mut("@docs/a.txt").unwrap();
    file.state.root = "/replacement/documents".into();
    file.state.path = "a.txt".into();
    let mut observed = observation(&old, &rebound, DisappearanceCause::AliasRebound);
    observed
        .disappeared
        .insert("@docs/a.txt".into(), DisappearanceCause::AliasRebound);
    fixture_reconcile(
        Some(&old),
        &mut rebound,
        Some(&observed),
        producer(),
        |_| panic!("rebound alias is not a move"),
    )?;
    assert_ne!(
        rebound.files["@docs/a.txt"].state.artifact_id,
        old.files["@docs/a.txt"].state.artifact_id
    );
    let removed = inventory(2, &[])?;
    let mut recreated = inventory(3, &[("@docs/a.txt", TEXT)])?;
    recreated.roots = old.roots.clone();
    let file = recreated.files.get_mut("@docs/a.txt").unwrap();
    file.state.root = "/original/documents".into();
    file.state.path = "a.txt".into();
    fixture_reconcile(Some(&removed), &mut recreated, None, producer(), |_| {
        Ok(true)
    })?;
    assert_ne!(
        recreated.files["@docs/a.txt"].state.artifact_id,
        old.files["@docs/a.txt"].state.artifact_id
    );
    assert_eq!(
        recreated.files["@docs/a.txt"].state.stamps.first_generation,
        3
    );
    Ok(())
}

#[test]
fn provider_changes_update_only_the_files_in_their_bound_domain() -> Result<()> {
    let old = inventory(1, &[("a.txt", TEXT), ("b.txt", TEXT)])?;
    let mut next = inventory(2, &[("a.txt", TEXT), ("b.txt", TEXT)])?;
    let domains = BTreeMap::from([
        ("a.txt".into(), Sha256Digest::from_bytes([9; 32])),
        ("b.txt".into(), producer()),
    ]);
    let result = reconciliation::reconcile(Some(&old), &mut next, None, &domains, |_| Ok(true))?;
    assert_eq!(result.events.len(), 1);
    assert_eq!(result.events[0].cause, ChangeCause::ProviderInvalidated);
    assert_eq!(result.events[0].after.as_ref().unwrap().path, "a.txt");
    assert_eq!(next.files["b.txt"].state, old.files["b.txt"].state);
    assert_eq!(next.files["a.txt"].state.stamps.evidence_generation, 2);
    Ok(())
}

#[test]
fn large_root_bindings_charge_actual_serialized_record_bytes() -> Result<()> {
    let names = (0..220)
        .map(|number| format!("file-{number}.txt"))
        .collect::<Vec<_>>();
    let files = names
        .iter()
        .map(|name| (name.as_str(), TEXT))
        .collect::<Vec<_>>();
    let mut old = inventory(1, &files)?;
    old.root = format!("/{}", "r".repeat(32_000));
    for file in old.files.values_mut() {
        file.state.root = old.root.clone();
    }
    let mut next = inventory(2, &[])?;
    next.root = old.root.clone();
    let observed = observation(&old, &next, DisappearanceCause::DeletedObserved);
    let result = fixture_reconcile(Some(&old), &mut next, Some(&observed), producer(), |_| {
        Ok(true)
    })?;
    assert!(!result.complete);
    assert!(result.events.len() < 220);
    let bytes = result
        .events
        .iter()
        .map(|event| serde_json::to_vec(event).map(|bytes| bytes.len()))
        .sum::<Result<usize, _>>()?
        + result
            .resolvers
            .iter()
            .map(|resolver| serde_json::to_vec(resolver).map(|bytes| bytes.len()))
            .sum::<Result<usize, _>>()?;
    assert!(bytes <= 12 * 1024 * 1024);
    Ok(())
}
