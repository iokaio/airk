// SPDX-License-Identifier: Apache-2.0
//! Receipt stages belong to their native owners, not a shared stage vocabulary.
use super::*;
use airk_core::{
    catalog_fences::FencedReceipt,
    catalog_management::{CatalogReceipt, catalog_operation_id},
    catalog_setup::{IndexSetupReceipt, SetupStage},
};

pub(super) fn capture_operation_receipts(
    connection: &Connection,
    snapshot: &mut CheckpointSnapshot,
) -> Result<()> {
    if !table(snapshot, "index_operation_receipts") {
        return Ok(());
    }
    let mut statement = connection.prepare(&format!("SELECT id,kind,stage,revision,length(payload),substr(payload,1,65537) FROM index_operation_receipts ORDER BY id LIMIT {}", MAX_ROWS + 1))?;
    let mut rows = statement.query([])?;
    let mut count = 0;
    let mut classifier = catalog::split::CheckpointReceiptClassifier::default();
    while let Some(row) = rows.next()? {
        count += 1;
        ensure!(
            count <= MAX_ROWS,
            CheckpointLimit("operation receipt count")
        );
        let id: String = row.get(0)?;
        let kind: String = row.get(1)?;
        let stage: String = row.get(2)?;
        let revision: i64 = row.get(3)?;
        let length: i64 = row.get(4)?;
        let payload: Vec<u8> = row.get(5)?;
        ensure!(
            payload.len() <= MAX_ROW_BYTES && usize::try_from(length)? == payload.len(),
            CheckpointLimit("operation receipt bytes")
        );
        if terminal(
            &mut classifier,
            connection,
            &id,
            &kind,
            &stage,
            revision,
            &payload,
        )? {
            continue;
        }
        ensure!(
            snapshot.unfinished.len() + snapshot.retained_intents.len() < 4096,
            CheckpointLimit("unfinished operation count")
        );
        snapshot.unfinished.push(UnfinishedWork {
            owner: "index_operation_receipts".into(),
            id,
            kind,
            state: stage,
            original_engine: None,
            payload_digest: Some(framed_digest(b"airk/checkpoint-operation/1", &[&payload])),
        });
    }
    Ok(())
}

pub(crate) fn terminal(
    classifier: &mut catalog::split::CheckpointReceiptClassifier,
    connection: &Connection,
    id: &str,
    kind: &str,
    stage: &str,
    revision: i64,
    payload: &[u8],
) -> Result<bool> {
    match (kind, stage) {
        ("catalog_index_setup", "ready") => {
            let saved: IndexSetupReceipt = serde_json::from_slice(payload)?;
            saved.validate()?;
            ensure!(
                saved.stage == SetupStage::Ready
                    && saved.operation_id.to_string() == id
                    && i64::try_from(saved.catalog_revision.get())? == revision,
                "checkpoint setup receipt identity changed"
            );
            Ok(true)
        }
        ("catalog_management_fenced", "metadata_applied") => {
            let saved: FencedReceipt = serde_json::from_slice(payload)?;
            saved.validate()?;
            receipt_identity(&saved.metadata, id, revision)?;
            Ok(true)
        }
        ("catalog_management" | "catalog_source_completion", "metadata_applied") => {
            let saved: CatalogReceipt = serde_json::from_slice(payload)?;
            receipt_identity(&saved, id, revision)?;
            ensure!(
                kind != "catalog_source_completion" || !saved.authority_fence_required,
                "checkpoint unfenced metadata receipt requires a native fence"
            );
            Ok(true)
        }
        (kind, _) if kind.starts_with("catalog_split_final_") => {
            classifier.terminal(connection, id, kind, stage, revision, payload)
        }
        // Unknown owners and stages remain accepted work; a familiar stage
        // string never grants permission to stop its native owner.
        _ => Ok(false),
    }
}

fn receipt_identity(saved: &CatalogReceipt, id: &str, revision: i64) -> Result<()> {
    ensure!(
        saved.schema == 1
            && catalog_operation_id(saved.caller, &saved.key)? == saved.operation_id
            && saved.operation_id.to_string() == id
            && i64::try_from(saved.catalog_revision.get())? == revision,
        "checkpoint catalog receipt identity changed"
    );
    Ok(())
}
