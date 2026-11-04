// SPDX-License-Identifier: Apache-2.0
//! Explicit selected-index source recovery. This is a bounded native metadata
//! receipt scan, not an indexed lookup or work performed for retrieval/startup.
use super::*;
use airk_core::{
    catalog_fences::FenceIntent,
    catalog_setup::{IndexSetupReceipt, SetupStage},
    index_catalog::SourceImportState,
};

pub(super) fn source_observation(
    db: &Connection,
    begin_operation: Incarnation,
) -> Result<SourceChangeObservation> {
    let begin = begin_receipt(db, begin_operation)?;
    let CatalogOperation::BeginIndexSourceChange { plan, .. } = &begin.request.operation else {
        unreachable!()
    };
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM service_jobs WHERE id=?1)",
        [plan.job.to_string()],
        |row| row.get(0),
    )?;
    let job = if exists {
        Some(super::super::super::jobs::record(db, plan.job)?)
    } else {
        None
    };
    let configuration_result = if job.is_some() {
        super::super::super::configuration::result(db, plan.job)?
    } else {
        None
    };
    let completion = completed(db, &begin.request)?;
    let result = SourceChangeObservation {
        begin,
        job,
        configuration_result,
        completion,
    };
    encoded_size(&result, 48 * 1024)?;
    Ok(result)
}

impl ServiceRegistry {
    /// Discover current source work only for an explicitly selected index.
    /// The native receipt table is bounded to 1024 rows of at most 16 KiB each;
    /// no receipt JSON expression index exists. Never call this per query or as
    /// a startup/corpus walk. Unknown/complete/tombstoned indexes return None,
    /// which is diagnostic absence, not source readiness or query authority.
    pub fn catalog_pending_source_change(
        &self,
        index_id: IndexId,
    ) -> Result<Option<SourceChangeObservation>> {
        let read = self.connection.unchecked_transaction()?;
        super::super::validate_existing(&read)?;
        let catalog = super::super::state(&read)?;
        ensure!(
            catalog.activated && catalog.authority_contract == 1 && catalog.policy_contract == 0,
            "source recovery discovery requires the active native catalog"
        );
        let exists: bool = read.query_row(
            "SELECT EXISTS(SELECT 1 FROM index_definitions WHERE id=?1)",
            [index_id.to_string()],
            |row| row.get(0),
        )?;
        if !exists {
            return Ok(None);
        }
        let index = super::super::operations::index_record(&read, index_id)?;
        if index.tombstone || index.source_import == SourceImportState::Complete {
            return Ok(None);
        }

        // Bound bytes BEFORE decoding JSON, including corrupted rows. A corrupt
        // candidate must remain a diagnostic error, never fabricated absence.
        let mut statement = read.prepare("SELECT id,kind,stage,revision,length(payload),substr(payload,1,16385) FROM index_operation_receipts ORDER BY id LIMIT 1025")?;
        let mut rows = statement.query([])?;
        let mut count = 0;
        let mut begin = None;
        let mut setup = None;
        while let Some(row) = rows.next()? {
            count += 1;
            ensure!(count <= 1024, "source recovery receipt capacity exceeded");
            let operation: String = row.get(0)?;
            let kind: String = row.get(1)?;
            let stage: String = row.get(2)?;
            let revision: i64 = row.get(3)?;
            let length: i64 = row.get(4)?;
            let bytes: Vec<u8> = row.get(5)?;
            ensure!(
                length >= 0
                    && length <= i64::try_from(MAX_RECEIPT_BYTES)?
                    && usize::try_from(length)? == bytes.len(),
                "source recovery receipt exceeds bound"
            );
            match kind.as_str() {
                "catalog_management_fenced" => {
                    let request = if stage == "metadata_applied" {
                        let receipt: FencedReceipt = serde_json::from_slice(&bytes)?;
                        receipt.validate()?;
                        ensure!(
                            receipt.metadata.operation_id.to_string() == operation
                                && number(receipt.metadata.catalog_revision) == revision,
                            "source discovery saved fence identity changed"
                        );
                        receipt.request
                    } else {
                        let intent: FenceIntent = serde_json::from_slice(&bytes)?;
                        intent.validate()?;
                        ensure!(
                            intent.operation_id.to_string() == operation
                                && intent.stage.as_str() == stage
                                && number(intent.request.expected_catalog_revision) == revision,
                            "source discovery pending fence identity changed"
                        );
                        intent.request
                    };
                    if let CatalogOperation::BeginIndexSourceChange {
                        id,
                        expected_source_revision,
                        ..
                    } = &request.operation
                        && *id == index_id
                        && expected_source_revision.next()? == index.source_revision
                    {
                        ensure!(
                            stage == "metadata_applied"
                                && request.key.namespace == catalog.namespace,
                            "current source mutation has no committed native begin"
                        );
                        ensure!(
                            begin.replace(operation.parse()?).is_none(),
                            "ambiguous current source mutation lineage"
                        );
                    }
                }
                "catalog_index_setup" => {
                    let receipt: IndexSetupReceipt = serde_json::from_slice(&bytes)?;
                    receipt.validate()?;
                    ensure!(
                        receipt.operation_id.to_string() == operation
                            && receipt.stage.as_str() == stage
                            && number(receipt.catalog_revision) == revision,
                        "source discovery setup receipt identity changed"
                    );
                    if receipt.index_id == index_id
                        && matches!(receipt.stage, SetupStage::Reserved | SetupStage::Prepared)
                        && receipt.source_revision == index.source_revision
                    {
                        ensure!(
                            receipt.request.key.namespace == catalog.namespace
                                && receipt.request.registration == index.backing,
                            "source discovery setup backing changed"
                        );
                        super::super::setup::reserved_sources(&read, &receipt)?;
                        ensure!(
                            setup.replace(receipt).is_none(),
                            "ambiguous pending native setup lineage"
                        );
                    }
                }
                _ => {}
            }
        }
        match (begin, setup) {
            (Some(_), Some(_)) => anyhow::bail!("pending source work conflicts with native setup"),
            (Some(operation), None) => {
                let observed = source_observation(&read, operation)?;
                pending_index(&read, &observed.begin.request)?;
                ensure!(
                    observed.completion.is_none(),
                    "pending source work already has a completion receipt"
                );
                Ok(Some(observed))
            }
            (None, Some(_)) => Ok(None),
            (None, None) => anyhow::bail!(
                "pending source metadata has no canonical begin or native setup receipt"
            ),
        }
    }
}
