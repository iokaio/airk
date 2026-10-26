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
        let writer = self.lock_current_writer()?;
        let baseline = self.load()?;
        let source = airk_store::blobs::RetainedSource {
            store: &self.store,
            snapshot: &baseline,
        };
        let mut retained_analysis = None;
        self.with_pressure_retry(&writer, || {
            let mut snapshot = baseline.clone();
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
            if retained_analysis.is_none() {
                retained_analysis = Some(match adapter {
                    "lsp" => {
                        airk_ingest::providers::run_lsp(&self.root, &snapshot, &source, request)?
                    }
                    _ => {
                        airk_ingest::providers::run_ctags(&self.root, &snapshot, &source, request)?
                    }
                });
            }
            let analysis = retained_analysis
                .as_ref()
                .context("provider output missing")?;
            let current = self.verify_source_inventory(&snapshot)?;
            ensure!(
                airk_ingest::semantic::source_inventory(&current)
                    == airk_ingest::semantic::source_inventory(&snapshot),
                "source inventory changed while provider ran; previous generation retained"
            );
            airk_ingest::providers::apply(&mut snapshot, analysis, &source)?;
            snapshot.generation = snapshot
                .generation
                .checked_add(1)
                .context("generation overflow")?;
            snapshot.persistent_indexes = true;
            snapshot.storage_format = 1;
            snapshot.embedding_model = self.configured_model()?;
            self.publish_v2(
                &writer,
                &snapshot,
                reservation,
                if adapter == "lsp" {
                    airk_core::changes::PublicationReason::Lsp
                } else {
                    airk_core::changes::PublicationReason::Ctags
                },
            )?;
            Ok(())
        })?;
        Ok(
            json!({"status":self.status()?,"provider":adapter,"coverage":retained_analysis.context("provider output missing")?.coverage}),
        )
    }

    pub fn extract_ocr(&self, path: &str, request: &OcrRequest) -> Result<Value> {
        let writer = self.lock_current_writer()?;
        let baseline = self.load()?;
        let policy = self.extraction_policy()?;
        ensure!(
            !policy.pending_reindex,
            "index pending extraction policy before OCR"
        );
        let detail = policy.published_detail.unwrap_or_default();
        ensure!(
            baseline.extractor_version == airk_core::EXTRACTOR_VERSION,
            "reindex the collection before adding OCR to an older extractor revision"
        );
        let limits = self.scan_limits()?;
        let roots = self.configured_roots()?;
        ensure!(
            roots == baseline.roots,
            "index pending source root changes before OCR"
        );
        let location = airk_store::roots::source_location(&baseline.root, &baseline.roots, path)?;
        let bytes = airk_ingest::read_selected_source(
            Path::new(&location.root),
            &self.data_root,
            &location.relative_path,
        )?;
        ensure!(
            bytes.len() <= limits.file_bytes,
            "OCR source exceeds configured file limit"
        );
        let mut retained_output = None;
        self.with_pressure_retry(&writer, || {
            let mut snapshot = baseline.clone();
            let reservation = self.publication_reservation()?;
            reservation.charge(64 * 1024 * 1024)?;
            if retained_output.is_none() {
                retained_output = Some(airk_ingest::ocr::extract(path, &bytes, request)?);
            }
            let output = retained_output.as_ref().context("OCR output missing")?;
            ensure!(
                airk_ingest::read_selected_source(
                    Path::new(&location.root),
                    &self.data_root,
                    &location.relative_path
                )? == bytes,
                "OCR source changed during extraction; previous generation retained"
            );
            snapshot
                .artifacts
                .insert(path.into(), output.artifact.clone());
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
            let mut profile = airk_ingest::adapters::legacy::legacy_profile(artifact)?;
            if detail != airk_core::extraction::IndexDetail::Detailed {
                profile
                    .options
                    .insert("index_detail".into(), detail.as_str().into());
            }
            let payload = airk_ingest::continuity::prepare_native(artifact, &bytes, &profile)?;
            let prepared = airk_core::binding::bind_payload(
                &payload,
                &profile,
                airk_core::binding::ArtifactBinding {
                    path,
                    source_hash: artifact.content_hash.parse()?,
                    source_bytes: bytes.len(),
                },
            )?;
            *artifact = self.store.retain_payload_with_budget(
                &prepared,
                &payload,
                &profile,
                Some(&bytes),
                Some(&reservation),
            )?;
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
            self.publish_v2(
                &writer,
                &snapshot,
                reservation,
                airk_core::changes::PublicationReason::Ocr,
            )?;
            Ok(())
        })?;
        Ok(
            json!({"status":self.status()?,"path":path,"provenance":retained_output.context("OCR output missing")?.provenance}),
        )
    }
}
