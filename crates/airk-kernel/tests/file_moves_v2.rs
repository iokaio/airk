// SPDX-License-Identifier: Apache-2.0
//! Original public writer/read fixtures for observed file continuity.
use airk_core::{
    changes::{ChangesRequest, ContinuityBasis, RememberedSelector, ValidationOutcome},
    evidence::{EvidenceReadOptions, EvidenceReadStatus},
};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::Result;
use std::fs;

const TEXT: &str = "This original artifact describes amber bridges cedar forests granite harbors ivory mountains quiet rivers.\n";
const OTHER: &str = "A replacement artifact describes bronze deserts emerald gardens lunar oceans silver valleys winter meadows.\n";

#[test]
fn copy_reuses_extraction_without_transferring_identity_and_publishes_new_inventory() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("a.txt"), TEXT)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let old = store.catalog()?.unwrap();
    fs::copy(root.path().join("a.txt"), root.path().join("copy.txt"))?;
    let report = kernel.index()?;
    assert_eq!(
        report.indexed, 0,
        "both native payloads must come from verified cache"
    );
    assert_eq!(report.reused, 2);
    assert_eq!(report.generation, old.snapshot.generation + 1);
    let next = store.catalog()?.unwrap();
    let files = &next.inventory.as_ref().unwrap().files;
    assert_ne!(
        files["a.txt"].state.artifact_id,
        files["copy.txt"].state.artifact_id
    );
    assert_eq!(
        files["a.txt"].state.payload_digest,
        files["copy.txt"].state.payload_digest
    );
    assert_eq!(
        files["a.txt"].state.source_hash,
        files["copy.txt"].state.source_hash
    );
    for kind in ["sources", "artifacts"] {
        assert_eq!(
            fs::read_dir(store.directory().join("objects").join(kind))?.count(),
            1,
            "copy must share the {kind} object"
        );
    }
    assert_eq!(
        files["a.txt"].state,
        old.inventory.as_ref().unwrap().files["a.txt"].state
    );
    assert!(next.lineage.as_ref().unwrap().resolvers.is_empty());
    let reopened = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(reopened.index()?.generation, next.snapshot.generation);
    Ok(())
}

#[test]
fn chained_moves_follow_exact_handles_then_terminal_deletion_and_live_path_reuse() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("a.txt"), TEXT)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let old = store.catalog()?.unwrap();
    let original = &old.inventory.as_ref().unwrap().files["a.txt"];
    let baseline = old.receipt.as_ref().unwrap().token;
    for (from, to) in [("a.txt", "b.txt"), ("b.txt", "c.txt")] {
        fs::rename(root.path().join(from), root.path().join(to))?;
        let report = kernel.index()?;
        assert_eq!(report.indexed, 0, "move must avoid native parser");
        assert_eq!(report.reused, 1);
        let next = store.catalog()?.unwrap();
        let state = &next.inventory.as_ref().unwrap().files[to].state;
        assert_eq!(state.artifact_id, original.state.artifact_id);
        assert_eq!(state.payload_digest, original.state.payload_digest);
        assert_eq!(state.source_hash, original.state.source_hash);
        assert_eq!(
            state.stamps.content_generation,
            original.state.stamps.content_generation
        );
        assert_eq!(state.stamps.path_generation, next.snapshot.generation);
    }
    let default =
        kernel.read_file_conditional("a.txt", 1, 1, 4096, &EvidenceReadOptions::default())?;
    assert_eq!(default.read.status, EvidenceReadStatus::MoveCandidate);
    assert!(default.content.is_none());
    let follow = EvidenceReadOptions {
        follow_moves: true,
        known_hash: Some(original.state.source_hash),
        artifact_id: Some(original.state.artifact_id),
        ..Default::default()
    };
    let redirected = kernel.read_file_conditional("a.txt", 1, 1, 4096, &follow)?;
    assert_eq!(redirected.read.status, EvidenceReadStatus::Content);
    assert_eq!(redirected.read.resolved_path.as_deref(), Some("c.txt"));
    assert_eq!(redirected.content.unwrap().hits[0].text, TEXT);
    let changes = kernel.changes(&ChangesRequest {
        since: Some(baseline),
        limit: 100,
        max_bytes: 65_536,
        ..Default::default()
    })?;
    assert_eq!(changes.changes.len(), 1);
    assert_eq!(changes.changes[0].observations, 2);
    assert_eq!(
        changes.changes[0].basis,
        ContinuityBasis::UniqueCompatibleContent
    );
    fs::write(root.path().join("a.txt"), OTHER)?;
    kernel.index()?;
    let live = kernel.read_file_conditional(
        "a.txt",
        1,
        1,
        4096,
        &EvidenceReadOptions {
            follow_moves: true,
            ..Default::default()
        },
    )?;
    assert_eq!(live.read.resolved_path.as_deref(), Some("a.txt"));
    assert_eq!(live.content.unwrap().hits[0].text, OTHER);
    let validations = kernel.changes(&ChangesRequest {
        selectors: vec![
            RememberedSelector::Artifact {
                artifact_id: original.state.artifact_id,
            },
            RememberedSelector::Citation {
                path: "a.txt".into(),
                source_hash: original.state.source_hash,
            },
        ],
        max_bytes: 65_536,
        ..Default::default()
    })?;
    assert_eq!(
        validations.validations[0].current.as_ref().unwrap().path,
        "c.txt"
    );
    assert_eq!(
        validations.validations[1].outcome,
        ValidationOutcome::PathReused
    );
    fs::remove_file(root.path().join("c.txt"))?;
    kernel.index()?;
    let deleted = kernel.read_file_conditional("b.txt", 1, 1, 4096, &follow)?;
    assert_eq!(deleted.read.status, EvidenceReadStatus::RemovedFromIndex);
    assert!(deleted.content.is_none());
    assert_eq!(fs::read_to_string(root.path().join("a.txt"))?, OTHER);
    Ok(())
}

#[test]
fn duplicates_empty_and_tiny_content_never_automatically_redirect() -> Result<()> {
    for (text, duplicate) in [(TEXT, true), ("", false), ("tiny fixture\n", false)] {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        fs::write(root.path().join("a.txt"), text)?;
        if duplicate {
            fs::write(root.path().join("survivor.txt"), text)?;
        }
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.index()?;
        let store = FileStore::new(data.path().join("collections/default"));
        let old = store.catalog()?.unwrap();
        let old_id = old.inventory.as_ref().unwrap().files["a.txt"]
            .state
            .artifact_id;
        fs::rename(root.path().join("a.txt"), root.path().join("b.txt"))?;
        let report = kernel.index()?;
        assert_eq!(report.indexed, 0, "reuse does not require continuity proof");
        let next = store.catalog()?.unwrap();
        assert_ne!(
            next.inventory.as_ref().unwrap().files["b.txt"]
                .state
                .artifact_id,
            old_id
        );
        let read = kernel.file_info_conditional(
            "a.txt",
            &EvidenceReadOptions {
                follow_moves: true,
                ..Default::default()
            },
        )?;
        assert_ne!(read.read.status, EvidenceReadStatus::Content);
        assert!(read.content.is_none());
    }
    Ok(())
}

#[test]
fn cross_alias_move_requires_stable_bindings_and_missing_root_aborts_publication() -> Result<()> {
    let root = tempfile::tempdir()?;
    let alias = tempfile::tempdir()?;
    let replacement = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("a.txt"), TEXT)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.add_root("docs", alias.path())?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let old = store.catalog()?.unwrap();
    let old_id = old.inventory.as_ref().unwrap().files["a.txt"]
        .state
        .artifact_id;
    fs::rename(root.path().join("a.txt"), alias.path().join("b.txt"))?;
    let report = kernel.index()?;
    assert_eq!(report.indexed, 0);
    assert_eq!(
        store.catalog()?.unwrap().inventory.unwrap().files["@docs/b.txt"]
            .state
            .artifact_id,
        old_id
    );
    let read = kernel.file_info_conditional(
        "a.txt",
        &EvidenceReadOptions {
            follow_moves: true,
            ..Default::default()
        },
    )?;
    assert_eq!(read.read.resolved_path.as_deref(), Some("@docs/b.txt"));
    fs::write(replacement.path().join("b.txt"), TEXT)?;
    kernel.remove_root("docs")?;
    kernel.add_root("docs", replacement.path())?;
    kernel.index()?;
    let rebound = store.catalog()?.unwrap();
    assert_ne!(
        rebound.inventory.as_ref().unwrap().files["@docs/b.txt"]
            .state
            .artifact_id,
        old_id
    );
    let head = rebound.receipt.as_ref().unwrap().token;
    fs::remove_file(replacement.path().join("b.txt"))?;
    // Removing only this original empty temporary root demonstrates root failure,
    // not a complete scan proving deletion of its former indexed contents.
    fs::remove_dir(replacement.path())?;
    assert!(kernel.index().is_err());
    assert_eq!(store.catalog()?.unwrap().receipt.unwrap().token, head);
    assert_eq!(fs::read_to_string(alias.path().join("b.txt"))?, TEXT);
    Ok(())
}
