// SPDX-License-Identifier: Apache-2.0
//! Explicit bounded physical repacking reuses the current publication journal.
use crate::Kernel;
use airk_core::compression::CompressionMode;
use airk_store::{
    current::{
        CapturedInput,
        graph::{GraphOverlay, GraphOverlayLimits},
        postings::Lookup,
    },
    quota::Quota,
};
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
}

impl Kernel {
    pub fn configure_compression(&self, mode: CompressionMode) -> Result<CompressionMode> {
        let writer = self.store.lock()?;
        self.store.configure_compression(
            &writer,
            &Quota::open(&self.data_root)?,
            &self.root_id(),
            mode,
        )?;
        Ok(mode)
    }
    pub fn compression_status(&self) -> Result<CompressionStatus> {
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

    /// Observe durable journal boundaries; an observer must not reenter the writer.
    pub fn repack_current_observed(
        &self,
        after: Option<&str>,
        max_files: usize,
        observer: &mut dyn FnMut(crate::current::CurrentUpdateStage),
    ) -> Result<RepackReport> {
        ensure!(
            (1..=32).contains(&max_files) && after.is_none_or(|p| p.len() <= 4096),
            "invalid repack bounds"
        );
        let _execution = crate::execution::acquire(&self.data_root)?;
        let scope = self.current_scope()?.context("workspace is not indexed")?;
        let quota = Quota::open(&self.data_root)?;
        crate::current::recover_current(&self.store, &quota, &scope)?;
        let writer = self.store.lock()?;
        let reservation = quota.reserve_available(&self.collection, 256 << 20)?;
        let read = self.store.current_read(&scope)?;
        let base = read.token()?;
        let page = read.lookup_paths(Lookup::Files, "", after, max_files + 1)?;
        let mode = self.store.compression_mode(&self.root_id())?;
        let model = read
            .collection_state()?
            .context("repack collection missing")?
            .configuration
            .model;
        let mut report = RepackReport {
            examined: 0,
            changed: 0,
            next_after: None,
            generation: base.sequence,
            skipped_large: 0,
        };
        let mut inputs = Vec::new();
        let mut admitted = 0usize;
        for path in page.paths.iter().take(max_files) {
            let artifact = read.artifact(path)?.context("repack artifact missing")?;
            let reference = artifact
                .retained
                .as_ref()
                .context("repack reference missing")?;
            let bytes = artifact
                .bytes
                .checked_add(reference.object_bytes)
                .context("repack byte overflow")?;
            if bytes > 64 << 20 {
                report.skipped_large += 1;
                report.examined += 1;
                report.next_after = Some(path.clone());
                continue;
            }
            if admitted.saturating_add(bytes) > 64 << 20 {
                break;
            }
            admitted += bytes;
            let retained = self.store.load_retrieval_artifact(&artifact)?;
            let source = self.store.read_retained_source(&artifact)?;
            let updated = self.store.retain_payload_encoded(
                &retained.artifact,
                &retained.payload,
                &retained.profile,
                Some(&source),
                Some(&reservation),
                mode,
            )?;
            if serde_json::to_vec(&updated)? != serde_json::to_vec(&artifact)? {
                inputs.push(CapturedInput { artifact: updated });
            }
            report.examined += 1;
            report.next_after = Some(path.clone());
        }
        if report.examined == page.paths.len() && page.next_after.is_none() {
            report.next_after = None;
        }
        report.changed = inputs.len();
        if inputs.is_empty() {
            return Ok(report);
        }
        let overlay = GraphOverlay::new(
            &reservation,
            read.clone(),
            GraphOverlayLimits::for_indexing(&reservation),
        )?;
        drop(read);
        let sealed = overlay.seal()?;
        let capacity = self.store.current_database_capacity(&scope)?;
        let mut current = self
            .store
            .current_write(&writer, &quota, &scope, capacity)?;
        let pending = current.prepare_repack(&base, &sealed, &inputs, model)?;
        observer(crate::current::CurrentUpdateStage::Prepared);
        current.mark_applying(&pending)?;
        observer(crate::current::CurrentUpdateStage::Applying);
        report.generation = current
            .commit(&pending, pending.target.lexical_commit)?
            .token
            .sequence;
        observer(crate::current::CurrentUpdateStage::Committed);
        drop(current);
        drop(writer);
        drop(reservation);
        self.maintain_current()?;
        Ok(report)
    }
}
