// SPDX-License-Identifier: Apache-2.0
//! Original fixtures for exact retained evidence and explicit operator controls.
use airk_core::{
    evidence::{EvidenceReadOptions, EvidenceReadStatus},
    identity::RevisionToken,
};
use airk_kernel::{
    HistoryAvailabilityRequest, Kernel,
    history_controls::{
        CreateCheckpoint, HistoryPolicyV1, HistoryWorkBudget, PurgeApplyRequest, PurgeRequest,
        RemoveCheckpoint,
    },
};
use airk_store::{
    FileStore,
    retention::{RetentionMode, RetentionPolicy},
};
use anyhow::{Context, Result};
use std::fs;

struct Fixture {
    source: tempfile::TempDir,
    _data: tempfile::TempDir,
    kernel: Kernel,
    store: FileStore,
}
impl Fixture {
    fn new() -> Result<Self> {
        let source = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
        kernel.configure_retention(RetentionPolicy {
            mode: RetentionMode::Manual,
            ..Default::default()
        })?;
        let store = FileStore::new(data.path().join("collections/default"));
        Ok(Self {
            source,
            _data: data,
            kernel,
            store,
        })
    }
    fn publish(&self, value: u32) -> Result<RevisionToken> {
        fs::write(
            self.source.path().join("original.rs"),
            format!("pub fn original_beacon() -> u32 {{\n    {value}\n}}\n"),
        )?;
        self.kernel.index()?;
        self.token()
    }
    fn token(&self) -> Result<RevisionToken> {
        Ok(self
            .store
            .catalog()?
            .context("catalog")?
            .receipt
            .as_ref()
            .context("receipt")?
            .token)
    }
}
fn options(revision: RevisionToken) -> EvidenceReadOptions {
    EvidenceReadOptions {
        revision: Some(revision),
        ..Default::default()
    }
}

#[test]
fn exact_file_context_info_outline_never_fall_back_to_current_or_live_bytes() -> Result<()> {
    let fixture = Fixture::new()?;
    let old = fixture.publish(17)?;
    let current = fixture.publish(29)?;
    fs::write(
        fixture.source.path().join("original.rs"),
        "LIVE_UNINDEXED_SENTINEL\n",
    )?;
    let request = options(old);
    let read = fixture
        .kernel
        .read_file_conditional("original.rs", 1, 3, 8192, &request)?;
    assert_eq!(read.read.status, EvidenceReadStatus::Content);
    assert_eq!(read.read.publication.as_ref().unwrap().token, old);
    assert_eq!(
        read.read.current_publication.as_ref().unwrap().token,
        current
    );
    assert!(read.read.historical);
    let payload = read.content.context("old content")?;
    assert!(payload.hits[0].text.contains("17"));
    assert!(!payload.hits[0].text.contains("29"));
    assert!(!payload.hits[0].text.contains("LIVE_UNINDEXED"));
    let context = fixture
        .kernel
        .context_conditional("original.rs", 2, 0, 0, 8192, &request)?;
    assert!(context.content.unwrap().hits[0].text.contains("17"));
    let info = fixture
        .kernel
        .file_info_conditional("original.rs", &request)?;
    assert_eq!(info.content.unwrap().generation, old.generation);
    let outline = fixture
        .kernel
        .outline_conditional("original.rs", 10, 8192, &request)?;
    assert!(
        outline
            .content
            .unwrap()
            .facts
            .iter()
            .any(|fact| fact.name == "original_beacon")
    );
    let not_modified = fixture.kernel.read_file_conditional(
        "original.rs",
        1,
        3,
        8192,
        &EvidenceReadOptions {
            if_evidence: read.read.validator,
            ..options(old)
        },
    )?;
    assert_eq!(not_modified.read.status, EvidenceReadStatus::NotModified);
    assert!(not_modified.content.is_none());
    assert!(
        fixture
            .kernel
            .read_file_conditional(
                "original.rs",
                1,
                3,
                8192,
                &EvidenceReadOptions {
                    follow_moves: true,
                    ..options(old)
                }
            )
            .is_err()
    );
    assert!(
        fixture
            .kernel
            .read_file_conditional("original.rs", 0, 3, 8192, &request)
            .is_err()
    );
    fixture.kernel.prune_indexes(false)?;
    let unavailable = fixture
        .kernel
        .read_file_conditional("original.rs", 1, 3, 8192, &request)?;
    assert_eq!(
        unavailable.read.status,
        EvidenceReadStatus::HistoryUnavailable
    );
    assert!(unavailable.content.is_none());
    assert!(unavailable.read.publication.is_none());
    assert_eq!(
        fs::read_to_string(fixture.source.path().join("original.rs"))?,
        "LIVE_UNINDEXED_SENTINEL\n"
    );
    Ok(())
}

#[test]
fn checkpoint_conversion_and_explicit_purge_publish_lineage_without_reindexing() -> Result<()> {
    let fixture = Fixture::new()?;
    let old = fixture.publish(17)?;
    fixture.kernel.configure_history(HistoryPolicyV1 {
        enabled: true,
        recent: 0,
        hourly_hours: 0,
        ..Default::default()
    })?;
    assert_eq!(
        fixture.token()?,
        old,
        "policy configuration is not publication"
    );
    fixture.kernel.create_checkpoint(
        &CreateCheckpoint {
            label: "original café checkpoint".into(),
            revision: old,
        },
        HistoryWorkBudget::default(),
    )?;
    fixture.publish(29)?;
    let head = fixture.publish(31)?;
    let conversion_budget = HistoryWorkBudget {
        maintenance: airk_store::retention::MaintenanceBudget {
            max_duration_ms: 10_000,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut conversion_complete = false;
    for _ in 0..8 {
        let run = fixture.kernel.maintain_history(conversion_budget)?;
        assert!(run.error.is_none(), "{run:?}");
        // A verified package can be readable while full-generation retirement
        // is still pending. Purge requires the conversion to finish as well.
        if !run.pending {
            conversion_complete = true;
            break;
        }
    }
    assert!(
        conversion_complete,
        "bounded history conversion did not finish"
    );
    assert!(
        fixture.store.history_at(&old, 268_435_456)?.is_some(),
        "checkpoint must retain a verified historical representation"
    );
    assert_eq!(fixture.kernel.checkpoints()?.len(), 1);
    let read = fixture
        .kernel
        .context_conditional("original.rs", 2, 0, 0, 8192, &options(old))?;
    assert!(read.content.unwrap().hits[0].text.contains("17"));
    assert_eq!(
        fixture.token()?,
        head,
        "representation conversion does not publish source changes"
    );
    assert!(
        fixture
            .kernel
            .remove_checkpoint(
                &RemoveCheckpoint {
                    label: "original café checkpoint".into(),
                    expected_revision: head
                },
                HistoryWorkBudget::default()
            )
            .is_err()
    );
    let request = PurgeRequest {
        revisions: vec![old],
        checkpoints: vec![RemoveCheckpoint {
            label: "original café checkpoint".into(),
            expected_revision: old,
        }],
        clear_lineage: true,
    };
    let preview = fixture
        .kernel
        .purge_history_preview(&request, HistoryWorkBudget::default())?;
    let result = fixture.kernel.purge_history(
        &PurgeApplyRequest {
            request,
            expected_plan_digest: preview.digest,
        },
        HistoryWorkBudget::default(),
    )?;
    assert!(result.storage.error.is_none(), "{:?}", result.storage.error);
    assert!(
        result.lineage_publication.is_some(),
        "explicit lineage purge must publish its own revision"
    );
    assert!(fixture.token()?.generation > head.generation);
    assert!(fixture.kernel.checkpoints()?.is_empty());
    let catalog = fixture.store.catalog()?.unwrap();
    assert_eq!(
        catalog.lineage.as_ref().unwrap().observation.reason,
        airk_core::changes::PublicationReason::LineagePurge
    );
    drop(catalog);
    let unavailable = fixture
        .kernel
        .file_info_conditional("original.rs", &options(old))?;
    assert_eq!(
        unavailable.read.status,
        EvidenceReadStatus::HistoryUnavailable
    );
    assert!(unavailable.content.is_none());
    assert!(fs::read_to_string(fixture.source.path().join("original.rs"))?.contains("31"));
    Ok(())
}

#[test]
fn availability_cursor_binds_publication_budgets_and_collection_incarnation() -> Result<()> {
    let fixture = Fixture::new()?;
    let first = fixture.publish(17)?;
    fixture.publish(29)?;
    fixture.publish(31)?;
    let request = HistoryAvailabilityRequest {
        revision: None,
        cursor: None,
        limit: 1,
        max_bytes: 4096,
    };
    let page = fixture.kernel.history_availability(&request)?;
    assert_eq!(page.revisions.len(), 1);
    let cursor = page.next_cursor.context("next page cursor")?;
    let second = fixture
        .kernel
        .history_availability(&HistoryAvailabilityRequest {
            cursor: Some(cursor.clone()),
            ..request.clone()
        })?;
    assert!(!second.revisions.contains(&page.revisions[0]));
    assert!(
        fixture
            .kernel
            .history_availability(&HistoryAvailabilityRequest {
                cursor: Some(cursor.clone()),
                limit: 2,
                ..request.clone()
            })
            .is_err()
    );
    let tiny = fixture
        .kernel
        .history_availability(&HistoryAvailabilityRequest {
            max_bytes: 1,
            ..request.clone()
        })?;
    assert!(tiny.revisions.is_empty() && !tiny.complete);
    assert!(
        fixture
            .kernel
            .history_availability(&HistoryAvailabilityRequest {
                limit: 65,
                ..request.clone()
            })
            .is_err()
    );
    assert!(
        fixture
            .kernel
            .history_availability(&HistoryAvailabilityRequest {
                revision: Some(first),
                cursor: Some(cursor.clone()),
                ..request.clone()
            })
            .is_err()
    );
    fixture.publish(37)?;
    assert!(
        fixture
            .kernel
            .history_availability(&HistoryAvailabilityRequest {
                cursor: Some(cursor),
                ..request.clone()
            })
            .is_err()
    );
    let other = Fixture::new()?;
    let foreign = other.publish(41)?;
    assert!(
        fixture
            .kernel
            .history_availability(&HistoryAvailabilityRequest {
                revision: Some(foreign),
                ..request
            })
            .is_err()
    );
    assert!(
        fixture
            .kernel
            .file_info_conditional("original.rs", &options(foreign))
            .is_err()
    );
    Ok(())
}
