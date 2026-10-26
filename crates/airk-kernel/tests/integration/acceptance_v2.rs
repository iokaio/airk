// SPDX-License-Identifier: Apache-2.0
//! Independent P12 acceptance through actual publication and retained readers.
use super::current_inspection::InspectCurrent;
use airk_core::changes::{ChangeCause, ChangesRequest, RememberedSelector, ValidationOutcome};
use airk_ingest::adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

const SOURCE: &str = "// Original acceptance fixture: amber bridges cedar forests granite harbors quiet rivers.\npub fn original_beacon(value: u32) -> u32 {\n    value.saturating_add(17)\n}\n";

fn readmission(cause: ChangeCause) -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("original.rs"), SOURCE)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let before = store.current_test_catalog()?.context("initial catalog")?;
    let baseline = before.token;
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
                detail: None,
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
    let gap = store.current_test_catalog()?.context("gap publication")?;
    assert!(
        !gap.inventory
            .as_ref()
            .unwrap()
            .files
            .contains_key("original.rs")
    );
    let gap_token = gap.token;
    match cause {
        ChangeCause::Ignored => fs::write(root.path().join(".airkignore"), ".airkignore\n")?,
        ChangeCause::Oversize => {
            kernel.configure_scan_limits(limits)?;
        }
        ChangeCause::ExtractionOmitted => {
            kernel.configure_extraction_policy(ExtractionPolicy {
                detail: None,
                version: 1,
                rules: vec![],
            })?;
        }
        _ => unreachable!(),
    }
    kernel.index()?;
    let after = store
        .current_test_catalog()?
        .context("readmitted publication")?;
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
    assert_eq!(
        changes.baseline_status,
        airk_core::changes::BaselineStatus::Verified
    );
    assert!(changes.changes.iter().any(|change|
        matches!(change.subject, airk_core::changes::ChangeSubject::Artifact { artifact_id } if artifact_id == file.state.artifact_id)
            && change.before.is_some() && change.after.is_none()));
    assert!(changes.changes.iter().any(|change|
        matches!(change.subject, airk_core::changes::ChangeSubject::Artifact { artifact_id } if artifact_id == current.state.artifact_id)
            && change.before.is_none() && change.after.is_some()));
    assert!(after.token.generation > gap_token.generation);
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
    kernel.configure_index_detail(airk_core::extraction::IndexDetail::Detailed)?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let old = store.current_test_catalog()?.unwrap();
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
    let after = store.current_test_catalog()?.unwrap();
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
    let directory_move = store.current_test_catalog()?.unwrap();
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
fn active_requests_preserve_publication_and_exclude_relocation_and_removal() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("original.rs"), SOURCE)?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let scope = store
        .current_scope(&root.path().canonicalize()?.to_string_lossy(), "default")?
        .unwrap();
    let first = store.current_read(&scope)?;
    let second = store.current_read(&scope)?;
    let before = first.token()?;
    fs::write(root.path().join("original.rs"), SOURCE.replace("17", "29"))?;
    kernel.index()?;
    assert_ne!(store.current_read(&scope)?.token()?, before);
    assert_eq!(first.token()?, before);
    let destination_parent = tempfile::tempdir()?;
    let destination = destination_parent.path().join("relocated");
    assert!(airk_store::relocation::relocate(data.path(), &destination).is_err());
    assert!(kernel.remove_collection("default").is_err());
    drop(first);
    kernel.index()?;
    assert_eq!(second.token()?, before);
    drop(second);
    kernel.index()?;
    assert_ne!(store.current_read(&scope)?.token()?, before);
    airk_store::relocation::relocate(data.path(), &destination)?;
    let reopened = Kernel::open(root.path(), Some(destination))?;
    assert!(
        reopened
            .read_file("original.rs", 1, 4, 8192)?
            .hits
            .iter()
            .any(|h| h.text.contains("29"))
    );
    reopened.remove_collection("default")?;
    assert!(root.path().join("original.rs").exists());
    Ok(())
}
