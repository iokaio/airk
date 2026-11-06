// SPDX-License-Identifier: Apache-2.0
//! Actual canonical controls and native indexed/setup fixtures. Isolated
//! authority-format settlement is NOT managed installer/activation qualification.
use super::*;
use airk_core::{
    catalog_management::{CatalogKind, CatalogPageRequest, MAX_PAGE_BYTES},
    index_catalog::PathScope,
    service::ProjectRegistration,
};
use std::{fs, path::PathBuf};

pub(crate) struct NativeSplitFixture {
    pub(crate) control: PathBuf,
    pub(crate) registry: ServiceRegistry,
    pub(crate) original: ProjectRegistration,
    pub(crate) selection: SplitDraftSelection,
    pub(crate) snapshot: Option<SplitSnapshot>,
    pub(crate) engine: Sha256Digest,
    // Rust drops fields in declaration order: close SQLite before TempDir.
    pub(crate) temporary: tempfile::TempDir,
}
fn response(
    registry: &mut ServiceRegistry,
    control: &Path,
    method: ControlMethod,
) -> Result<ControlResponse> {
    execute(registry, control, &method)?
        .context("canonical split handler did not accept split method")
}

/// Root worker tests can request an actual paid sealed DestinationPrepared
/// fixture. The original publication/settings/incarnation come from real Kernel
/// indexing; no claimed publication or job result is fabricated here.
pub(crate) fn native_fixture(prepared: bool) -> Result<NativeSplitFixture> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let root = base.join("source");
    fs::create_dir_all(root.join("docs"))?;
    fs::write(
        root.join("docs/selected.md"),
        b"selected native split document\n",
    )?;
    fs::write(root.join("remainder.md"), b"original positive remainder\n")?;
    let control = base.join("control");
    crate::local_ipc::prepare_root(&control)?;
    let namespace = Incarnation::from_bytes([91; 16]);
    let original =
        crate::service_registration::prepare(&root, &base.join("original-data"), "original")?;
    data_scope::bind(Path::new(&original.data_root), &control, namespace)?;
    let kernel = airk_kernel::Kernel::open_service_collection(
        &root,
        Some(PathBuf::from(&original.data_root)),
        &original.collection,
    )?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    drop(kernel);
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    registry.register(&original)?;
    drop(registry);
    let upgrade =
        airk_store::service::registry::catalog::upgrade::CatalogUpgrade::acquire(&control)?;
    airk_kernel::service_catalog_upgrade::prepare(&upgrade, Sha256Digest::from_bytes([92; 32]))?;
    upgrade.migrate()?;
    drop(upgrade);
    let sql = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
    let tx = sql.unchecked_transaction()?;
    tx.execute_batch(include_str!(
        "../../airk-store/src/service_catalog_authority_v1.sql"
    ))?;
    tx.execute(
        "UPDATE index_catalog_meta SET activated=1,authority_contract=1",
        [],
    )?;
    tx.commit()?;
    drop(sql);
    let marker = control.join("service-1.4.owner.json");
    let mut owner: serde_json::Value = serde_json::from_slice(&fs::read(&marker)?)?;
    owner.as_object_mut().unwrap().remove("catalog_upgrade");
    fs::write(marker, serde_json::to_vec(&owner)?)?;
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    let page = registry.catalog_page(&CatalogPageRequest {
        kind: CatalogKind::Indexes,
        index_id: None,
        project_id: None,
        after: None,
        expected_catalog_revision: None,
        limit: 2,
        max_bytes: MAX_PAGE_BYTES,
    })?;
    let [CatalogItem::Index(index)] = page.items.as_slice() else {
        anyhow::bail!("native fixture needs exactly one imported original index")
    };
    ensure!(
        index.backing == original
            && index.source_import == airk_core::index_catalog::SourceImportState::Complete,
        "native original metadata import incomplete"
    );
    let state = registry.catalog_split_source_state(index.id, page.revision)?;
    let source = state
        .policy
        .roots
        .iter()
        .find(|source| source.owner)
        .context("native original primary source absent")?;
    let (namespace, epoch) = registry.saved_request_epoch()?;
    let selection = SplitDraftSelection {
        key: RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([93; 16]),
        },
        expected_catalog_revision: page.revision,
        index_id: index.id,
        selectors: vec![ScopeAtom {
            source_root_id: source.id,
            kind: PathScope::Subtree,
            path: "docs".into(),
        }],
        destination_name: "Native split destination".into(),
        destination_data_root: base.join("destination-data").to_str().unwrap().into(),
        destination_collection: "destination".into(),
        projects: state
            .affected_projects
            .iter()
            .map(|effect| SplitProjectSelection {
                project_id: effect.project_id,
                retain_original: true,
                membership: SplitMembershipSelection::Omit,
            })
            .collect(),
        capacity: SplitCapacitySelection {
            destination_growth: 256 << 20,
            destination_temporary: 256 << 20,
            original_rewrite: 128 << 20,
            cleanup_allowance: 64 << 20,
        },
        require_semantic_ready: false,
        destination_watch_enabled: false,
    };
    let engine = Sha256Digest::from_bytes([94; 32]);
    let mut snapshot = None;
    if prepared {
        let ControlResponse::CatalogSplitDraft { draft } = response(
            &mut registry,
            &control,
            ControlMethod::CatalogSplitDraft {
                selection: Box::new(selection.clone()),
            },
        )?
        else {
            anyhow::bail!("draft response differs")
        };
        let ControlResponse::CatalogSplitAccepted { snapshot: accepted } = response(
            &mut registry,
            &control,
            ControlMethod::CatalogSplitApply { draft },
        )?
        else {
            anyhow::bail!("apply response differs")
        };
        let operation = accepted.receipt.operation_id;
        for _ in 0..8 {
            let current = registry.catalog_split_status(operation)?.unwrap();
            if current.receipt.stage == SplitStage::DestinationPrepared {
                snapshot = Some(current);
                break;
            }
            match advance_pending(&mut registry, &control, engine, operation)? {
                SplitAdvance::Advanced(_) => {}
                _ => anyhow::bail!("native fixture failed before sealed DestinationPrepared"),
            }
        }
        ensure!(
            snapshot.is_some(),
            "native fixture exceeded bounded setup/funding turns"
        );
    }
    Ok(NativeSplitFixture {
        temporary,
        control,
        registry,
        original,
        selection,
        snapshot,
        engine,
    })
}
fn jobs(registry: &ServiceRegistry, control: &Path) -> Result<i64> {
    let _ = registry;
    Ok(rusqlite::Connection::open_with_flags(
        control.join("service-1.4.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?
    .query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?)
}
fn quota_entries(root: &str) -> Result<Vec<std::ffi::OsString>> {
    let mut entries = fs::read_dir(Path::new(root).join(".quota-1.4/reservations"))?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();
    Ok(entries)
}

#[test]
fn canonical_split_reads_and_draft_have_no_job_quota_or_source_effects() -> Result<()> {
    let mut fixture = native_fixture(false)?;
    let source = Path::new(&fixture.original.root).join("docs/selected.md");
    let bytes = fs::read(&source)?;
    let before = fixture.registry.catalog_state()?;
    let queued = jobs(&fixture.registry, &fixture.control)?;
    let quota = quota_entries(&fixture.original.data_root)?;
    let ControlResponse::CatalogSplitDraft { draft } = response(
        &mut fixture.registry,
        &fixture.control,
        ControlMethod::CatalogSplitDraft {
            selection: Box::new(fixture.selection.clone()),
        },
    )?
    else {
        anyhow::bail!("draft response differs")
    };
    draft.validate()?;
    response(
        &mut fixture.registry,
        &fixture.control,
        ControlMethod::CatalogSplitSourceState {
            index_id: fixture.selection.index_id,
            expected_catalog_revision: fixture.selection.expected_catalog_revision,
        },
    )?;
    response(
        &mut fixture.registry,
        &fixture.control,
        ControlMethod::CatalogSplitStatus {
            operation_id: draft.request.operation_id()?,
        },
    )?;
    response(
        &mut fixture.registry,
        &fixture.control,
        ControlMethod::CatalogSplitReceipt {
            caller: draft.request.caller,
            key: draft.request.key.clone(),
        },
    )?;
    response(
        &mut fixture.registry,
        &fixture.control,
        ControlMethod::CatalogSplitRecoveryReceipt {
            operation_id: draft.request.operation_id()?,
            caller: draft.request.caller,
            key: draft.request.key.clone(),
        },
    )?;
    assert_eq!(fixture.registry.catalog_state()?, before);
    assert_eq!(jobs(&fixture.registry, &fixture.control)?, queued);
    assert_eq!(quota_entries(&fixture.original.data_root)?, quota);
    assert_eq!(fs::read(source)?, bytes);
    assert!(!Path::new(&fixture.selection.destination_data_root).exists());
    Ok(())
}

#[test]
fn canonical_apply_is_exact_durable_intent_before_setup_or_funding() -> Result<()> {
    let mut fixture = native_fixture(false)?;
    let ControlResponse::CatalogSplitDraft { draft } = response(
        &mut fixture.registry,
        &fixture.control,
        ControlMethod::CatalogSplitDraft {
            selection: Box::new(fixture.selection.clone()),
        },
    )?
    else {
        anyhow::bail!("draft response differs")
    };
    let apply = ControlMethod::CatalogSplitApply {
        draft: draft.clone(),
    };
    let first = response(&mut fixture.registry, &fixture.control, apply.clone())?;
    let ControlResponse::CatalogSplitAccepted { snapshot } = first else {
        anyhow::bail!("accepted split intent absent")
    };
    assert_eq!(snapshot.receipt.stage, SplitStage::Accepted);
    assert!(snapshot.receipt.funding_receipts.is_empty() && snapshot.receipt.jobs.is_empty());
    assert!(!Path::new(&fixture.selection.destination_data_root).exists());
    let repeated = response(&mut fixture.registry, &fixture.control, apply)?;
    assert_eq!(
        serde_json::to_value(repeated)?,
        serde_json::to_value(ControlResponse::CatalogSplitAccepted {
            snapshot: snapshot.clone()
        })?
    );
    let exact = response(
        &mut fixture.registry,
        &fixture.control,
        ControlMethod::CatalogSplitReceipt {
            caller: draft.request.caller,
            key: draft.request.key.clone(),
        },
    )?;
    let ControlResponse::CatalogSplitReceipt {
        snapshot: Some(saved),
        expired,
        ..
    } = exact
    else {
        anyhow::bail!("exact split receipt absent")
    };
    assert!(!expired);
    assert_eq!(*saved, *snapshot);
    assert_eq!(jobs(&fixture.registry, &fixture.control)?, 0);
    Ok(())
}

#[test]
fn lost_recovery_reply_reconciles_exact_key_without_resubmitting_after_status_advances()
-> Result<()> {
    let mut fixture = native_fixture(false)?;
    let ControlResponse::CatalogSplitDraft { draft } = response(
        &mut fixture.registry,
        &fixture.control,
        ControlMethod::CatalogSplitDraft {
            selection: Box::new(fixture.selection.clone()),
        },
    )?
    else {
        anyhow::bail!("draft response differs")
    };
    let ControlResponse::CatalogSplitAccepted { snapshot } = response(
        &mut fixture.registry,
        &fixture.control,
        ControlMethod::CatalogSplitApply { draft },
    )?
    else {
        anyhow::bail!("accepted response differs")
    };
    let request = SplitRecoveryRequest {
        caller: snapshot.request.caller,
        key: RequestKey {
            nonce: Incarnation::from_bytes([95; 16]),
            ..snapshot.request.key.clone()
        },
        operation_id: snapshot.receipt.operation_id,
        request_digest: snapshot.receipt.request_digest,
        expected_receipt_digest: snapshot.receipt_digest,
        expected_finalization_revision: None,
        action: SplitRecoveryAction::Cancel,
    };
    let discarded = response(
        &mut fixture.registry,
        &fixture.control,
        ControlMethod::CatalogSplitRecover {
            request: Box::new(request.clone()),
        },
    )?;
    let ControlResponse::CatalogSplitRecovered {
        receipt: accepted, ..
    } = discarded
    else {
        anyhow::bail!("cancel recovery response differs")
    };
    let current = fixture
        .registry
        .catalog_split_status(request.operation_id)?
        .unwrap();
    advance_pending(
        &mut fixture.registry,
        &fixture.control,
        fixture.engine,
        request.operation_id,
    )?;
    let observed = response(
        &mut fixture.registry,
        &fixture.control,
        ControlMethod::CatalogSplitRecoveryReceipt {
            operation_id: request.operation_id,
            caller: request.caller,
            key: request.key.clone(),
        },
    )?;
    let ControlResponse::CatalogSplitRecoveryReceipt {
        receipt: Some(saved),
        snapshot: Some(latest),
        expired,
        ..
    } = observed
    else {
        anyhow::bail!("saved recovery acknowledgment absent")
    };
    assert!(!expired);
    assert_eq!(*saved, *accepted);
    assert_ne!(latest.receipt_digest, current.receipt_digest);
    let before = fixture
        .registry
        .catalog_split_status(request.operation_id)?;
    response(
        &mut fixture.registry,
        &fixture.control,
        ControlMethod::CatalogSplitRecover {
            request: Box::new(request),
        },
    )?;
    assert_eq!(
        fixture
            .registry
            .catalog_split_status(latest.receipt.operation_id)?,
        before
    );
    assert!(!Path::new(&fixture.selection.destination_data_root).exists());
    assert_eq!(
        fs::read(Path::new(&fixture.original.root).join("remainder.md"))?,
        b"original positive remainder\n"
    );
    Ok(())
}
