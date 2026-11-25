// SPDX-License-Identifier: Apache-2.0
//! Explicit bounded physical repacking reuses the current publication journal.
use crate::Kernel;
use airk_core::compression::CompressionMode;
use airk_store::current::postings::Lookup;
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Serialize)]
pub struct CompressionStatus {
    pub mode: CompressionMode,
    pub logical_reference_bytes: u64,
    pub unique_encoded_bytes: u64,
    pub compressed_objects: usize,
    pub physical_allocation_bytes: Option<u64>,
    pub complete: bool,
}
#[derive(Serialize)]
pub struct RepackReport {
    pub examined: usize,
    pub changed: usize,
    pub next_after: Option<String>,
    pub generation: u64,
    pub skipped_large: usize,
    /// Original saved evidence is preserved, never reinterpreted as a current payload.
    pub skipped_legacy: usize,
}

impl Kernel {
    pub fn configure_compression(&self, mode: CompressionMode) -> Result<CompressionMode> {
        let writer = self.store.lock()?;
        self.store.configure_compression(
            &writer,
            &self.collection_quota()?,
            &self.root_id(),
            mode,
        )?;
        Ok(mode)
    }
    pub fn compression_status(&self) -> Result<CompressionStatus> {
        self.require_unrestricted_read()?;
        let mode = self.store.compression_mode(&self.root_id())?;
        let mut result = CompressionStatus {
            mode,
            logical_reference_bytes: 0,
            unique_encoded_bytes: 0,
            compressed_objects: 0,
            physical_allocation_bytes: None,
            complete: true,
        };
        let Some(scope) = self.current_scope()? else {
            return Ok(result);
        };
        let read = self.store.current_read(&scope)?;
        let page = read.lookup_paths(Lookup::Files, "", None, 100_000)?;
        result.complete = page.next_after.is_none();
        let mut seen = BTreeSet::new();
        for path in page.paths {
            let artifact = read
                .artifact(&path)?
                .context("compression artifact missing")?;
            let reference = artifact
                .retained
                .as_ref()
                .context("compression reference missing")?;
            for (kind, logical, decoded, encoding) in [
                (
                    "sources",
                    &artifact.content_hash,
                    if reference.raw_source {
                        artifact.bytes
                    } else {
                        0
                    },
                    reference.source_encoding.as_ref(),
                ),
                (
                    "artifacts",
                    &reference.object,
                    reference.object_bytes,
                    reference.payload_encoding.as_ref(),
                ),
            ] {
                result.logical_reference_bytes = result
                    .logical_reference_bytes
                    .checked_add(decoded as u64)
                    .context("compression accounting overflow")?;
                let stored = encoding.map_or(logical.as_str(), |e| e.stored_digest.as_str());
                if decoded > 0 && seen.insert((kind.to_owned(), stored.to_owned())) {
                    result.unique_encoded_bytes = result
                        .unique_encoded_bytes
                        .checked_add(encoding.map_or(decoded as u64, |e| e.stored_bytes))
                        .context("compression accounting overflow")?;
                    result.compressed_objects += usize::from(encoding.is_some());
                }
            }
        }
        Ok(result)
    }

    /// One journaled batch; callers resume with the returned path, rediscovering
    /// current mappings instead of trusting a saved directory position.
    pub fn repack_current(&self, after: Option<&str>, max_files: usize) -> Result<RepackReport> {
        self.repack_current_observed(after, max_files, &mut |_| {})
    }

    /// The plain index stores every payload compressed in its batch; there is
    /// nothing to repack. Reports the current publication and no work.
    pub fn repack_current_observed(
        &self,
        after: Option<&str>,
        max_files: usize,
        _observer: &mut dyn FnMut(crate::current::CurrentUpdateStage),
    ) -> Result<RepackReport> {
        ensure!(
            (1..=32).contains(&max_files) && after.is_none_or(|p| p.len() <= 4096),
            "invalid repack bounds"
        );
        let scope = self.current_scope()?.context("workspace is not indexed")?;
        let read = self.store.current_read(&scope)?;
        Ok(RepackReport {
            examined: 0,
            changed: 0,
            next_after: None,
            generation: read.token()?.sequence,
            skipped_large: 0,
            skipped_legacy: 0,
        })
    }
}
