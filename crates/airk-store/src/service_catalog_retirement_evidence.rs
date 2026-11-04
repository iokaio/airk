// SPDX-License-Identifier: Apache-2.0
//! Explicit native recovery evidence. A tombstone or missing journal is never
//! equivalent to the full committed native retirement fence.
use super::{ServiceRegistry, operations::index_record, state, validate_existing};
pub use airk_core::catalog_fences::NativeIndexRetirementEvidence;
use airk_core::{
    catalog_fences::FencedReceipt,
    catalog_management::{CatalogOperation, MAX_RECEIPT_BYTES},
    index_catalog::IndexId,
    service::CatalogClock,
};
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, params};

impl ServiceRegistry {
    /// Explicit selected-index recovery only. This scans at most the bounded
    /// 1024 native receipt rows, each capped before decoding to 16 KiB. There is
    /// no JSON expression index, source scan, mutation or retrieval-path work.
    pub fn catalog_index_retirement_evidence(
        &self,
        index_id: IndexId,
    ) -> Result<Option<NativeIndexRetirementEvidence>> {
        let read = self.connection.unchecked_transaction()?;
        validate_existing(&read)?;
        let catalog = state(&read)?;
        ensure!(
            catalog.activated && catalog.authority_contract == 1 && catalog.policy_contract == 0,
            "native retirement evidence requires the active native catalog"
        );
        let row: Option<(String, Option<String>)> = read
            .query_row(
                "SELECT legacy_binding,active_registration FROM index_definitions WHERE id=?1",
                [index_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((binding, active)) = row else {
            return Ok(None);
        };
        let index = index_record(&read, index_id)?;
        if !index.tombstone {
            return Ok(None);
        }
        let mut statement = read.prepare("SELECT id,kind,stage,revision,length(payload),substr(payload,1,16385) FROM index_operation_receipts ORDER BY id LIMIT 1025")?;
        let mut rows = statement.query([])?;
        let mut count = 0;
        let mut retirement = None;
        while let Some(row) = rows.next()? {
            count += 1;
            ensure!(count <= 1024, "native retirement receipt capacity exceeded");
            let length: i64 = row.get(4)?;
            let bytes: Vec<u8> = row.get(5)?;
            ensure!(
                length >= 0
                    && length <= i64::try_from(MAX_RECEIPT_BYTES)?
                    && usize::try_from(length)? == bytes.len(),
                "native retirement receipt exceeds bound"
            );
            let kind: String = row.get(1)?;
            let stage: String = row.get(2)?;
            if kind != "catalog_management_fenced" || stage != "metadata_applied" {
                continue;
            }
            let receipt: FencedReceipt = serde_json::from_slice(&bytes)?;
            receipt.validate()?;
            let operation: String = row.get(0)?;
            let revision: i64 = row.get(3)?;
            ensure!(
                receipt.metadata.operation_id.to_string() == operation
                    && i64::try_from(receipt.metadata.catalog_revision.get())? == revision,
                "native retirement saved receipt identity changed"
            );
            if let CatalogOperation::RetireIndex { id, .. } = &receipt.request.operation
                && *id == index_id
            {
                let evidence = NativeIndexRetirementEvidence {
                    catalog_clock: CatalogClock {
                        namespace: catalog.namespace,
                        revision: catalog.revision,
                    },
                    index: index.clone(),
                    retirement: receipt,
                };
                evidence.validate()?;
                ensure!(
                    retirement.replace(evidence).is_none(),
                    "ambiguous native retirement lineage"
                );
            }
        }
        let Some(evidence) = retirement else {
            return Ok(None);
        };
        ensure!(
            binding == index.backing.binding.to_string()
                && active.as_deref() == Some(binding.as_str()),
            "native retirement backing pointer changed"
        );
        let (payload, registered, paused): (Vec<u8>, bool, bool) = read.query_row(
            "SELECT substr(payload,1,20481),registered,paused FROM projects WHERE binding=?1",
            [&binding],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        ensure!(
            !registered && paused && payload == serde_json::to_vec(&index.backing)?,
            "native retirement retained registration changed"
        );
        let enabled: bool = read.query_row(
            "SELECT EXISTS(SELECT 1 FROM index_sources WHERE index_id=?1 AND enabled<>0)",
            params![index_id.to_string()],
            |row| row.get(0),
        )?;
        ensure!(
            !enabled,
            "native retirement still has enabled source authority"
        );
        Ok(Some(evidence))
    }
}
