// SPDX-License-Identifier: Apache-2.0
//! Explicit local extraction transactions; original sources remain read-only.
use crate::Kernel;
use airk_ingest::{ocr::OcrRequest, providers::LiveRequest};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::path::Path;

impl Kernel {
    pub fn run_provider(&self, adapter: &str, request: &LiveRequest) -> Result<Value> {
        ensure!(
            matches!(adapter, "lsp" | "ctags"),
            "unknown live provider adapter"
        );
        let _lock = self.store.lock()?;
        let catalog = self.catalog()?;
        let mut snapshot = catalog.snapshot.clone();
        let current = self.verify_source_inventory(&snapshot)?;
        ensure!(
            airk_ingest::semantic::source_inventory(&current)
                == airk_ingest::semantic::source_inventory(&snapshot)
                && snapshot.extractor_version == airk_core::EXTRACTOR_VERSION,
            "source changed; reindex before running provider"
        );
        let reservation = self.publication_reservation()?;
        // Provider copies/output have a separate hard bound. Charge that temporary
        // allowance even though the subprocess sandbox resides in OS temporary storage.
        reservation.charge(160 * 1024 * 1024)?;
        let analysis = match adapter {
            "lsp" => airk_ingest::providers::run_lsp(&self.root, &snapshot, &catalog, request)?,
            _ => airk_ingest::providers::run_ctags(&self.root, &snapshot, &catalog, request)?,
        };
        let current = self.verify_source_inventory(&snapshot)?;
        ensure!(
            airk_ingest::semantic::source_inventory(&current)
                == airk_ingest::semantic::source_inventory(&snapshot),
            "source inventory changed while provider ran; previous generation retained"
        );
        airk_ingest::providers::apply(&mut snapshot, &analysis, &catalog)?;
        snapshot.generation = snapshot
            .generation
            .checked_add(1)
            .context("generation overflow")?;
        snapshot.persistent_indexes = true;
        snapshot.storage_format = 1;
        snapshot.embedding_model = self.configured_model()?;
        self.publish(&snapshot, &reservation)?;
        Ok(json!({"status":self.status()?,"provider":adapter,"coverage":analysis.coverage}))
    }

    pub fn extract_ocr(&self, path: &str, request: &OcrRequest) -> Result<Value> {
        let _lock = self.store.lock()?;
        let mut snapshot = self.load()?;
        ensure!(
            snapshot.extractor_version == airk_core::EXTRACTOR_VERSION,
            "reindex the collection before adding OCR to an older extractor revision"
        );
        let limits = self.scan_limits()?;
        let roots = self.configured_roots()?;
        ensure!(
            roots == snapshot.roots,
            "index pending source root changes before OCR"
        );
        let location = airk_store::roots::source_location(&snapshot.root, &snapshot.roots, path)?;
        let bytes = airk_ingest::read_selected_source(
            Path::new(&location.root),
            &self.data_root,
            &location.relative_path,
        )?;
        ensure!(
            bytes.len() <= limits.file_bytes,
            "OCR source exceeds configured file limit"
        );
        let reservation = self.publication_reservation()?;
        reservation.charge(64 * 1024 * 1024)?;
        let output = airk_ingest::ocr::extract(path, &bytes, request)?;
        ensure!(
            airk_ingest::read_selected_source(
                Path::new(&location.root),
                &self.data_root,
                &location.relative_path
            )? == bytes,
            "OCR source changed during extraction; previous generation retained"
        );
        snapshot.artifacts.insert(path.into(), output.artifact);
        let (source_bytes, extracted_bytes) = snapshot.artifacts.values().try_fold(
            (0usize, 0usize),
            |(sources, text), artifact| {
                Ok::<_, anyhow::Error>((
                    sources
                        .checked_add(artifact.bytes)
                        .context("OCR corpus size overflow")?,
                    text.checked_add(artifact.extracted_bytes())
                        .context("OCR extracted size overflow")?,
                ))
            },
        )?;
        ensure!(
            snapshot.artifacts.len() <= limits.files
                && source_bytes <= limits.corpus_bytes
                && extracted_bytes <= limits.extracted_bytes,
            "OCR publication exceeds configured corpus limits; previous generation retained"
        );
        let artifact = snapshot
            .artifacts
            .get_mut(path)
            .context("OCR artifact missing")?;
        *artifact =
            self.store
                .retain_artifact_with_budget(artifact, Some(&bytes), Some(&reservation))?;
        // Imported evidence binds the complete extraction/source inventory.
        snapshot.analysis = None;
        snapshot.imported_facts.clear();
        snapshot.imported_relationships.clear();
        snapshot.generation = snapshot
            .generation
            .checked_add(1)
            .context("generation overflow")?;
        snapshot.persistent_indexes = true;
        snapshot.storage_format = 1;
        snapshot.embedding_model = self.configured_model()?;
        self.publish(&snapshot, &reservation)?;
        Ok(json!({"status":self.status()?,"path":path,"provenance":output.provenance}))
    }
}
