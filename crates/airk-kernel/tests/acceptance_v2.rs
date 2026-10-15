// SPDX-License-Identifier: Apache-2.0
//! Independent P12 acceptance through actual publication and retained readers.
use airk_core::{
    changes::{ChangeCause, ChangesRequest, RememberedSelector, ValidationOutcome},
    identity::RevisionToken,
};
use airk_ingest::adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
use airk_kernel::Kernel;
use airk_store::{
    FileStore,
    history::ConversionStatus,
    quota::Quota,
    retention::{MaintenanceBudget, RetentionMode, RetentionPolicy},
};
use anyhow::{Context, Result};
use std::fs;

const SOURCE: &str = "// Original acceptance fixture: amber bridges cedar forests granite harbors quiet rivers.\npub fn original_beacon(value: u32) -> u32 {\n    value.saturating_add(17)\n}\n";

fn manual(kernel: &Kernel) -> Result<()> {
    kernel.configure_retention(RetentionPolicy {
        mode: RetentionMode::Manual,
        ..Default::default()
    })?;
    Ok(())
}
fn token(store: &FileStore) -> Result<RevisionToken> {
    Ok(store
        .catalog()?
        .context("catalog")?
        .receipt
        .as_ref()
        .context("receipt")?
        .token)
}

fn readmission(cause: ChangeCause) -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("original.rs"), SOURCE)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    manual(&kernel)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let before = store.catalog()?.context("initial catalog")?;
    let baseline = before.receipt.as_ref().unwrap().token;
    let file = before.inventory.as_ref().unwrap().files["original.rs"].clone();
    let key = kernel
        .outline("original.rs", 10, 8192)?
        .facts
        .iter()
        .find(|f| f.name == "original_beacon")
        .and_then(|f| f.continuity_key)
        .context("original symbol key")?;
    let limits = kernel.scan_limits()?;
    match cause {
        ChangeCause::Ignored => fs::write(
            root.path().join(".airkignore"),
            "original.rs\n.airkignore\n",
        )?,
        ChangeCause::Oversize => {
            kernel.configure_scan_limits(airk_ingest::ScanLimits {
                file_bytes: 32,
                ..limits
            })?;
        }
        ChangeCause::ExtractionOmitted => {
            kernel.configure_extraction_policy(ExtractionPolicy {
                version: 1,
                rules: vec![SelectionRule {
                    selector: RuleMatch::Exact {
                        path: "original.rs".into(),
                    },
                    case_sensitive: true,
                    adapter_id: "pdf".into(),
                }],
            })?;
        }
        _ => unreachable!(),
    }
    kernel.index()?;
    let gap = store.catalog()?.context("gap publication")?;
    assert!(
        !gap.inventory
            .as_ref()
            .unwrap()
            .files
            .contains_key("original.rs")
    );
    assert!(
        gap.lineage
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| event.cause == cause)
    );
    let gap_token = gap.receipt.as_ref().unwrap().token;
    match cause {
        ChangeCause::Ignored => fs::write(root.path().join(".airkignore"), ".airkignore\n")?,
        ChangeCause::Oversize => {
            kernel.configure_scan_limits(limits)?;
        }
        ChangeCause::ExtractionOmitted => {
            kernel.configure_extraction_policy(ExtractionPolicy {
                version: 1,
                rules: vec![],
            })?;
        }
        _ => unreachable!(),
    }
    kernel.index()?;
    let after = store.catalog()?.context("readmitted publication")?;
    let current = &after.inventory.as_ref().unwrap().files["original.rs"];
    assert_ne!(
        current.state.artifact_id, file.state.artifact_id,
        "an observation gap cannot resurrect an artifact handle"
    );
    assert_eq!(current.state.source_hash, file.state.source_hash);
    assert_eq!(current.state.profile_id, file.state.profile_id);
    let new_key = kernel
        .outline("original.rs", 10, 8192)?
        .facts
        .iter()
        .find(|f| f.name == "original_beacon")
        .and_then(|f| f.continuity_key)
        .context("new symbol key")?;
    assert_ne!(
        new_key, key,
        "identical readmitted source cannot resurrect a symbol handle"
    );
    let airk_core::continuity::ContinuityKey::Symbol(symbol_key) = key else {
        anyhow::bail!("native Rust declaration must have a symbol key");
    };
    let changes = kernel.changes(&ChangesRequest {
        since: Some(baseline),
        selectors: vec![
            RememberedSelector::Artifact {
                artifact_id: file.state.artifact_id,
            },
            RememberedSelector::Symbol { symbol_key },
        ],
        include_symbols: true,
        limit: 100,
        max_bytes: 65536,
        ..Default::default()
    })?;
    assert_eq!(changes.validations.len(), 2);
    for validation in &changes.validations {
        assert!(
            matches!(
                validation.outcome,
                ValidationOutcome::RemovedFromIndex | ValidationOutcome::Unknown
            ),
            "old handles must not validate new evidence: {validation:?}"
        );
        assert_ne!(validation.artifact_id, Some(current.state.artifact_id));
    }
    assert!(
        changes
            .changes
            .iter()
            .any(|change| change.causes.contains(&cause))
    );
    assert!(after.receipt.as_ref().unwrap().token.generation > gap_token.generation);
    assert_eq!(fs::read_to_string(root.path().join("original.rs"))?, SOURCE);
    Ok(())
}

#[test]
fn readmission_after_ignore_mints_new_artifact_and_symbol_handles() -> Result<()> {
    readmission(ChangeCause::Ignored)
}
#[test]
fn readmission_after_oversize_mints_new_artifact_and_symbol_handles() -> Result<()> {
    readmission(ChangeCause::Oversize)
}
#[test]
fn readmission_after_extraction_omission_mints_new_artifact_and_symbol_handles() -> Result<()> {
    readmission(ChangeCause::ExtractionOmitted)
}

#[test]
fn actual_case_only_rename_preserves_exact_observed_identity() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("Beacon.rs"), SOURCE)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let old = store.catalog()?.unwrap();
    let before = &old.inventory.as_ref().unwrap().files["Beacon.rs"];
    fs::rename(root.path().join("Beacon.rs"), root.path().join("beacon.rs"))?;
    let actual = fs::read_dir(root.path())?
        .map(|entry| entry.map(|e| e.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    assert!(
        actual.iter().any(|name| name == "beacon.rs"),
        "filesystem did not retain requested case-only spelling"
    );
    kernel.index()?;
    let after = store.catalog()?.unwrap();
    let files = &after.inventory.as_ref().unwrap().files;
    assert!(!files.contains_key("Beacon.rs"));
    assert_eq!(
        files["beacon.rs"].state.artifact_id,
        before.state.artifact_id
    );
    assert_eq!(
        files["beacon.rs"].state.payload_digest,
        before.state.payload_digest
    );
    fs::create_dir(root.path().join("Folder"))?;
    fs::rename(
        root.path().join("beacon.rs"),
        root.path().join("Folder/beacon.rs"),
    )?;
    kernel.index()?;
    fs::rename(root.path().join("Folder"), root.path().join("folder"))?;
    kernel.index()?;
    let directory_move = store.catalog()?.unwrap();
    let files = &directory_move.inventory.as_ref().unwrap().files;
    assert!(!files.contains_key("Folder/beacon.rs"));
    assert_eq!(
        files["folder/beacon.rs"].state.artifact_id,
        before.state.artifact_id
    );
    assert_eq!(
        files["folder/beacon.rs"].state.payload_digest,
        before.state.payload_digest
    );
    Ok(())
}

#[test]
fn two_reader_leases_survive_writer_then_guard_history_relocation_and_removal() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("original.rs"), SOURCE)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    manual(&kernel)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections/default"));
    let old = token(&store)?;
    let first = store.catalog_at(&old, 64 * 1024 * 1024)?.unwrap();
    let second = store.catalog_at(&old, 64 * 1024 * 1024)?.unwrap();
    for value in [29, 41] {
        fs::write(
            root.path().join("original.rs"),
            SOURCE.replace("17", &value.to_string()),
        )?;
        kernel.index()?;
    }
    let quota = Quota::open(data.path())?;
    let budget = MaintenanceBudget {
        max_duration_ms: 60_000,
        ..Default::default()
    };
    {
        let writer = store.lock()?;
        assert_eq!(
            store.history_convert(&writer, &quota, &old, budget)?.status,
            ConversionStatus::Protected
        );
        drop(first);
        assert_eq!(
            store.history_convert(&writer, &quota, &old, budget)?.status,
            ConversionStatus::Protected
        );
        drop(second);
        assert_eq!(
            store.history_convert(&writer, &quota, &old, budget)?.status,
            ConversionStatus::Complete
        );
    }
    let first = store.history_at(&old, 64 * 1024 * 1024)?.unwrap();
    let second = store.history_at(&old, 64 * 1024 * 1024)?.unwrap();
    fs::write(root.path().join("original.rs"), SOURCE.replace("17", "53"))?;
    kernel.index()?;
    assert!(token(&store)?.generation > old.generation);
    assert_eq!(first.source("original.rs")?, SOURCE.as_bytes());
    assert_eq!(second.source("original.rs")?, SOURCE.as_bytes());
    {
        let writer = store.lock()?;
        assert_eq!(
            store.history_remove(&writer, &quota, &old, budget)?.status,
            ConversionStatus::Protected
        );
    }
    drop(kernel);
    let destination_parent = tempfile::tempdir()?;
    let destination = destination_parent.path().join("relocated");
    assert!(airk_store::relocation::relocate(data.path(), &destination).is_err());
    assert!(airk_store::remove_collection(data.path(), "default").is_err());
    drop(first);
    assert!(airk_store::relocation::relocate(data.path(), &destination).is_err());
    assert_eq!(second.source("original.rs")?, SOURCE.as_bytes());
    drop(second);
    airk_store::relocation::relocate(data.path(), &destination)?;
    let moved = FileStore::new(destination.join("collections/default"));
    let reopened = moved
        .history_at(&old, 64 * 1024 * 1024)?
        .context("relocated exact history")?;
    assert_eq!(reopened.descriptor.publication.token, old);
    assert_eq!(reopened.source("original.rs")?, SOURCE.as_bytes());
    assert!(airk_store::remove_collection(&destination, "default").is_err());
    drop(reopened);
    airk_store::remove_collection(&destination, "default")?;
    Ok(())
}
