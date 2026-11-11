// SPDX-License-Identifier: Apache-2.0
//! Pure extraction over protected, already hashed coordinator inputs.
use airk_core::{
    Artifact,
    extraction::{ExtractionPayload, ExtractionProfile},
    identity::Sha256Digest,
};
use anyhow::{Context, Result};

pub(crate) struct CapturedExtraction {
    pub path: String,
    pub hash: Sha256Digest,
    pub bytes: Vec<u8>,
    pub profile: ExtractionProfile,
}

impl CapturedExtraction {
    pub fn parallel_eligible(&self) -> bool {
        self.bytes.len() <= crate::MAX_STRUCTURED_BYTES
            && !matches!(
                self.profile.adapter_id.as_str(),
                "pdf" | "docx" | "xlsx" | "pptx"
            )
    }
}

pub(crate) enum ExtractionFailure {
    Skip(anyhow::Error),
    Fatal(anyhow::Error),
    /// The one PDF parser slot stayed busy past its bounded wait. The document
    /// is neither extracted, failed nor missing: it is retried later and keeps
    /// its previously indexed generation meanwhile.
    Deferred(anyhow::Error),
}

/// Stable skip code for a document left unsettled by a busy PDF parser slot.
const DEFERRED_CODE: &str = "extraction_deferred";

/// Slot refusals are never document outcomes. An interrupted wait stops the
/// job with the typed `PdfSlotInterrupted` in its chain, for the host to
/// report as its own drain, cancellation or stall; a busy slot defers.
fn classify(
    error: anyhow::Error,
    otherwise: fn(anyhow::Error) -> ExtractionFailure,
) -> ExtractionFailure {
    match crate::pdf_slot::slot_outcome(&error) {
        Some(crate::pdf_slot::SlotOutcome::Interrupted) => ExtractionFailure::Fatal(error),
        Some(crate::pdf_slot::SlotOutcome::Busy) => ExtractionFailure::Deferred(error),
        None => otherwise(error),
    }
}

/// A document extractor's own failure: capacity exhaustion stops the job;
/// anything else is this document's extraction failure.
fn document_failure(error: anyhow::Error) -> ExtractionFailure {
    if error.is::<crate::pdf_worker::CapacityFailure>() {
        ExtractionFailure::Fatal(error)
    } else {
        ExtractionFailure::Skip(error)
    }
}

/// True when `path` resolves through the same root in this scan as in
/// `previous`, so its previous artifact still describes this location. This
/// mirrors the scan's previous-path reconciliation; a malformed alias path
/// never qualifies.
fn binding_unchanged(
    path: &str,
    previous: &airk_core::Snapshot,
    roots: &std::collections::BTreeMap<String, String>,
) -> bool {
    let (alias, old) = if let Some(aliased) = path.strip_prefix('@') {
        let Some((alias, _)) = aliased.split_once('/') else {
            return false;
        };
        (alias, previous.roots.get(alias))
    } else {
        ("", Some(&previous.root))
    };
    old.is_some_and(|old| roots.get(alias) == Some(old))
}

/// A document deferred by a busy PDF parser slot, awaiting its one retry.
struct Deferred {
    captured: CapturedExtraction,
    /// Its previously indexed artifact stands in for it until the retry.
    carried: bool,
}

pub(crate) struct FreshExtraction {
    pub captured: CapturedExtraction,
    pub prepared: PreparedExtraction,
}

pub(crate) enum PreparedExtraction {
    Fresh(Box<std::result::Result<ExtractionPayload, ExtractionFailure>>),
    Reused(Box<crate::observations::VerifiedReuse>),
}

pub(crate) fn extract(captured: CapturedExtraction) -> FreshExtraction {
    let payload = prepare(&captured);
    FreshExtraction {
        captured,
        prepared: PreparedExtraction::Fresh(Box::new(payload)),
    }
}

fn prepare(
    input: &CapturedExtraction,
) -> std::result::Result<ExtractionPayload, ExtractionFailure> {
    use ExtractionFailure::{Fatal, Skip};
    if input.bytes.len() <= crate::MAX_STRUCTURED_BYTES
        && crate::composite::is_host(&input.profile.adapter_id)
    {
        // Composite preparation has always belonged to fallible per-file extraction.
        let payload = crate::composite::extract(&input.path, &input.bytes, &input.profile)
            .map_err(|error| classify(error, Skip))?;
        // Both preparation paths call payload.validate(), including its serialized
        // MAX_PAYLOAD_BYTES check, before any completed result can enter the pool.
        return Ok(payload);
    }
    let producer =
        crate::retention::producer_profile(&input.path, &input.profile).map_err(Fatal)?;
    let extracted = crate::documents::extract_selected(&input.path, &input.bytes, &producer)
        .map_err(|error| classify(error, document_failure))?;
    let artifact = Artifact {
        path: input.path.clone(),
        content_hash: input.hash.to_string(),
        bytes: input.bytes.len(),
        chunks: extracted.chunks,
        facts: extracted.facts,
        language: extracted.language,
        extraction_method: extracted.method,
        parse_status: extracted.parse_status,
        retained: None,
    };
    if input.profile.contract_version == airk_core::retention::PROFILE_CONTRACT {
        let mut payload =
            crate::adapters::legacy::convert_native_legacy(&artifact, &input.bytes, &producer)
                .map_err(Fatal)?;
        crate::continuity::prepare_with_seeds(
            &mut payload,
            &input.profile,
            None,
            extracted.semantic_seeds.as_ref(),
        )
        .map_err(Fatal)?;
        Ok(payload)
    } else {
        crate::continuity::prepare_native_with_seeds(
            &artifact,
            &input.bytes,
            &input.profile,
            extracted.semantic_seeds.as_ref(),
        )
        .map_err(Fatal)
    }
}

pub(crate) fn bind(input: &CapturedExtraction, payload: &ExtractionPayload) -> Result<Artifact> {
    airk_core::binding::bind_payload(
        payload,
        &input.profile,
        airk_core::binding::ArtifactBinding {
            path: &input.path,
            source_hash: input.hash,
            source_bytes: input.bytes.len(),
        },
    )
}

pub(crate) struct ScanSink<'a, F> {
    previous: &'a airk_core::Snapshot,
    limits: crate::ScanLimits,
    emit: F,
    pub artifacts: std::collections::BTreeMap<String, Artifact>,
    pub source_bytes: usize,
    pub failed: bool,
    extracted_bytes: usize,
    /// At most one document awaits its retry. PDFs extract serially and the
    /// scan drains before each serial document, so a second deferral cannot
    /// arrive before the first one's retry; the bound also caps held bytes.
    deferred: Option<Deferred>,
    /// The document being finished is a deferred document's retry.
    retrying: bool,
}

impl<'a, F> ScanSink<'a, F>
where
    F: FnMut(&ExtractionProfile, &Artifact, &[u8], Option<&ExtractionPayload>) -> Result<Artifact>,
{
    pub fn new(previous: &'a airk_core::Snapshot, limits: crate::ScanLimits, emit: F) -> Self {
        Self {
            previous,
            limits,
            emit,
            artifacts: Default::default(),
            source_bytes: 0,
            extracted_bytes: 0,
            failed: false,
            deferred: None,
            retrying: false,
        }
    }

    pub fn retain(
        &mut self,
        input: &CapturedExtraction,
        artifact: &Artifact,
        payload: Option<&ExtractionPayload>,
    ) -> Result<()> {
        self.retain_inner(input, artifact, payload)
            .inspect_err(|_| self.failed = true)
    }

    fn retain_inner(
        &mut self,
        input: &CapturedExtraction,
        artifact: &Artifact,
        payload: Option<&ExtractionPayload>,
    ) -> Result<()> {
        use anyhow::{Context, ensure};
        self.source_bytes = self
            .source_bytes
            .checked_add(input.bytes.len())
            .context("source corpus byte count overflow")?;
        self.extracted_bytes = self
            .extracted_bytes
            .checked_add(artifact.extracted_bytes())
            .context("extracted corpus byte count overflow")?;
        ensure!(
            self.extracted_bytes <= self.limits.extracted_bytes,
            "extracted corpus exceeds configured byte limit: {} > {} bytes at {}; previous generation retained. Inspect AIRK ingestion limits or run `airk collections limits describe`",
            self.extracted_bytes,
            self.limits.extracted_bytes,
            input.path
        );
        ensure!(
            self.source_bytes <= self.limits.corpus_bytes,
            "corpus exceeds configured byte limit: {} > {} eligible source bytes at {}; previous generation retained. Inspect AIRK ingestion limits or run `airk collections limits describe`",
            self.source_bytes,
            self.limits.corpus_bytes,
            input.path
        );
        ensure!(
            self.artifacts.len() < self.limits.files,
            "corpus exceeds configured file count: {} > {} at {}; previous generation retained",
            self.artifacts.len() + 1,
            self.limits.files,
            input.path
        );
        let retained = (self.emit)(&input.profile, artifact, &input.bytes, payload)?;
        ensure!(
            retained.path == input.path && retained.content_hash == input.hash.to_string(),
            "retention changed artifact identity"
        );
        self.artifacts.insert(input.path.clone(), retained);
        Ok(())
    }

    pub fn finish(
        &mut self,
        completed: crate::extraction_pool::Completed<FreshExtraction>,
        report: &mut crate::IndexReport,
        observation: &mut crate::observations::ScanObservation,
    ) -> Result<()> {
        self.finish_inner(completed, report, observation)
            .inspect_err(|_| self.failed = true)
    }

    fn finish_inner(
        &mut self,
        completed: crate::extraction_pool::Completed<FreshExtraction>,
        report: &mut crate::IndexReport,
        observation: &mut crate::observations::ScanObservation,
    ) -> Result<()> {
        report.extraction_work_us = report.extraction_work_us.saturating_add(completed.work_us);
        let FreshExtraction { captured, prepared } = completed.result?;
        let payload = match prepared {
            PreparedExtraction::Fresh(payload) => *payload,
            PreparedExtraction::Reused(reused) => {
                let old = reused.artifact;
                anyhow::ensure!(
                    old.path == captured.path
                        && old.content_hash == captured.hash.to_string()
                        && old.bytes == captured.bytes.len()
                        && old.language == captured.profile.language,
                    "cache returned incompatible artifact binding"
                );
                anyhow::ensure!(
                    old.retained
                        .as_ref()
                        .and_then(|reference| reference.profile.as_ref())
                        .is_none_or(|profile| profile == &captured.profile),
                    "cache returned incompatible extraction profile"
                );
                report.reused += 1;
                match reused.basis {
                    crate::observations::ReuseBasis::SamePath => {
                        observation.usage.same_path_hits += 1
                    }
                    crate::observations::ReuseBasis::PriorContent => {
                        observation.usage.prior_content_hits += 1
                    }
                }
                return self.retain(&captured, &old, None);
            }
        };
        let outcome = payload.and_then(|payload| {
            bind(&captured, &payload)
                .map(|artifact| (payload, artifact))
                .map_err(|error| {
                    if crate::composite::is_host(&captured.profile.adapter_id)
                        && captured.bytes.len() <= crate::MAX_STRUCTURED_BYTES
                    {
                        ExtractionFailure::Skip(error)
                    } else {
                        ExtractionFailure::Fatal(error)
                    }
                })
        });
        match outcome {
            Ok((payload, artifact)) => {
                report.indexed += 1;
                self.retain(&captured, &artifact, Some(&payload))
            }
            Err(ExtractionFailure::Fatal(error)) => Err(error),
            Err(ExtractionFailure::Deferred(error)) => {
                self.defer(captured, error, report, &observation.roots);
                Ok(())
            }
            Err(ExtractionFailure::Skip(error)) => {
                if self.previous.artifacts.contains_key(&captured.path) {
                    let unsupported = !matches!(
                        captured.profile.adapter_id.as_str(),
                        "pdf" | "docx" | "xlsx" | "pptx"
                    ) && (std::str::from_utf8(&captured.bytes).is_err()
                        || captured.bytes.contains(&0));
                    observation.disappeared.insert(
                        captured.path.clone(),
                        if unsupported {
                            crate::observations::DisappearanceCause::Unsupported
                        } else {
                            crate::observations::DisappearanceCause::ExtractionFailed
                        },
                    );
                }
                let code = if crate::documents::budget_exceeded(&error) {
                    "extraction_budget_exceeded"
                } else {
                    "extraction_failed"
                };
                report.skip(crate::SkippedFile {
                    path: captured.path,
                    reason: error.to_string(),
                    code: Some(code.into()),
                });
                Ok(())
            }
        }
    }

    /// Leave a document unsettled behind a busy PDF parser slot. Its previous
    /// artifact, when it still describes this location, stands in for it, so
    /// it is neither indexed, failed nor reconciled as missing. A first
    /// deferral waits for one retry; a deferred retry is reported as deferred
    /// and left for a later scan, which sees it stale (changed bytes) or
    /// missing (new file) and extracts it again.
    fn defer(
        &mut self,
        captured: CapturedExtraction,
        error: anyhow::Error,
        report: &mut crate::IndexReport,
        roots: &std::collections::BTreeMap<String, String>,
    ) {
        let previous = self.previous;
        let carried = match previous.artifacts.get(&captured.path) {
            Some(old) if binding_unchanged(&captured.path, previous, roots) => {
                self.artifacts.insert(captured.path.clone(), old.clone());
                true
            }
            _ => false,
        };
        if !self.retrying && self.deferred.is_none() {
            self.deferred = Some(Deferred { captured, carried });
            return;
        }
        // Counted apart from the bounded skip list, so a verifier can refuse
        // a scan that left any document on a stand-in.
        report.extraction_deferred += 1;
        report.skip(crate::SkippedFile {
            path: captured.path,
            reason: error.to_string(),
            code: Some(DEFERRED_CODE.into()),
        });
    }

    /// Retry the deferred document once, after the batch ahead of it and in
    /// place of its stand-in. A busy slot again leaves it deferred; any other
    /// result settles it as usual, and an interruption stops the scan.
    fn retry_deferred(
        &mut self,
        report: &mut crate::IndexReport,
        observation: &mut crate::observations::ScanObservation,
    ) -> Result<()> {
        let Some(Deferred { captured, carried }) = self.deferred.take() else {
            return Ok(());
        };
        if carried {
            self.artifacts.remove(&captured.path);
        }
        observation.usage.parser_calls += 1;
        report.extraction_serial_jobs += 1;
        self.retrying = true;
        let finished = self.finish(
            crate::extraction_pool::execute(captured, extract),
            report,
            observation,
        );
        self.retrying = false;
        finished
    }

    /// Finish every admitted result in source order, then retry a deferred
    /// document. The scan drains before each serial document and once at its
    /// end, so a deferred document is retried before the next PDF and before
    /// the snapshot is built.
    pub fn drain(
        &mut self,
        pool: &mut crate::extraction_pool::OrderedPool<CapturedExtraction, FreshExtraction>,
        report: &mut crate::IndexReport,
        observation: &mut crate::observations::ScanObservation,
    ) -> Result<()> {
        while let Some(completed) = pool.next().inspect_err(|_| self.failed = true)? {
            self.finish(completed, report, observation)?;
        }
        self.retry_deferred(report, observation)
    }

    pub fn finish_next(
        &mut self,
        pool: &mut crate::extraction_pool::OrderedPool<CapturedExtraction, FreshExtraction>,
        report: &mut crate::IndexReport,
        observation: &mut crate::observations::ScanObservation,
    ) -> Result<()> {
        let completed = pool
            .next()
            .inspect_err(|_| self.failed = true)?
            .context("extraction admission has no pending result")?;
        self.finish(completed, report, observation)
    }
}

#[cfg(test)]
mod retained_profile_tests {
    use super::*;
    use airk_core::retention::{AppliedRetention, RetainedProfile};
    use sha2::{Digest, Sha256};
    #[test]
    fn actual_manifest_extraction_preserves_new_lean_core_and_rich_superset() -> Result<()> {
        let registry = crate::adapters::registry::AdapterRegistry::baseline();
        for (path,bytes) in [("Cargo.toml",b"[package]\nname = \"selected\"\nversion = \"0.1.0\"\n[workspace]\nmembers = [\"crates/one\"]\n[dependencies]\nserde = \"1\"\n".as_slice()),("package.json",br#"{"name":"selected","version":"1","scripts":{"test":"actual"},"dependencies":{"one":"1"},"workspaces":["pkg/*"]}"#.as_slice())] {
            let base=registry.effective_profile(path,"project-manifest")?;
            let mut outputs=Vec::new();
            for target in [RetainedProfile::Lean,RetainedProfile::Rich] {
                let crate::retention::Selection::Qualified{retained,..}=crate::retention::select_profile(path,&base,target)? else {anyhow::bail!("manifest not qualified");};
                let input=CapturedExtraction{path:path.into(),hash:Sha256Digest::from_bytes(Sha256::digest(bytes).into()),bytes:bytes.into(),profile:retained};
                let output=prepare(&input).map_err(|failure|match failure{ExtractionFailure::Fatal(error)|ExtractionFailure::Skip(error)|ExtractionFailure::Deferred(error)=>error})?;output.validate()?;
                assert!(output.facts.iter().any(|fact|fact.kind=="package_identity"));assert!(output.facts.iter().any(|fact|fact.kind=="package_dependency"));assert!(matches!(AppliedRetention::from_profile(&input.profile)?,AppliedRetention::Selected{..}));outputs.push(output);
            }
            assert_eq!(serde_json::to_vec(&outputs[0].semantic_projection)?,serde_json::to_vec(&outputs[1].semantic_projection)?);
            for fact in &outputs[0].facts {assert!(outputs[1].facts.iter().any(|other|other.kind==fact.kind&&other.name==fact.name&&other.location==fact.location));}
        }
        Ok(())
    }

    use crate::observations::{DisappearanceCause, ScanObservation};
    use crate::pdf_slot::{PdfSlotBusy, PdfSlotInterrupted};
    use std::{cell::Cell, collections::BTreeMap, time::Duration};

    const ROOT: &str = "fixture-root";
    const PDF: &str = "docs/manual.pdf";
    const OLD_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    fn pdf_input(bytes: &[u8]) -> Result<CapturedExtraction> {
        let registry = crate::adapters::registry::AdapterRegistry::baseline();
        Ok(CapturedExtraction {
            path: PDF.into(),
            hash: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
            bytes: bytes.into(),
            profile: registry.effective_profile(PDF, "pdf")?,
        })
    }

    /// A snapshot that indexed `PDF` before, under `ROOT`, with other bytes.
    fn indexed_before() -> airk_core::Snapshot {
        let mut previous = airk_core::Snapshot::empty(ROOT.into());
        previous.artifacts.insert(
            PDF.into(),
            Artifact {
                path: PDF.into(),
                content_hash: OLD_HASH.into(),
                bytes: 1,
                chunks: vec![],
                language: "pdf".into(),
                extraction_method: "pdf-text-layer".into(),
                parse_status: "complete".into(),
                facts: vec![],
                retained: None,
            },
        );
        previous
    }

    fn observed() -> ScanObservation {
        let mut observation = ScanObservation::default();
        observation.roots.insert(String::new(), ROOT.into());
        observation
    }

    fn hold_the_parser_slot() -> Result<crate::pdf_slot::SlotPermit<'static>> {
        crate::pdf_slot::process_slot().acquire(
            &crate::pdf_slot::SystemClock,
            crate::pdf_slot::SLOT_WAIT,
            &|| false,
        )
    }

    #[test]
    fn pdf_slot_refusals_defer_or_stop_and_are_never_document_failures() {
        let busy = || anyhow::Error::new(PdfSlotBusy).context("extract docs/manual.pdf");
        assert!(matches!(
            classify(busy(), document_failure),
            ExtractionFailure::Deferred(_)
        ));
        assert!(matches!(
            classify(busy(), ExtractionFailure::Skip),
            ExtractionFailure::Deferred(_)
        ));
        let interrupted = anyhow::Error::new(PdfSlotInterrupted).context("extract");
        match classify(interrupted, document_failure) {
            ExtractionFailure::Fatal(error) => assert!(error.is::<PdfSlotInterrupted>()),
            _ => panic!("an interrupted wait must stop the job"),
        }
        // Interruption wins over a busy cause in the same chain.
        let both = anyhow::Error::new(PdfSlotBusy).context(PdfSlotInterrupted);
        assert!(matches!(
            classify(both, document_failure),
            ExtractionFailure::Fatal(_)
        ));
        // Capacity stays fatal; an ordinary parser refusal stays this document's skip.
        let capacity = anyhow::anyhow!("disk full").context(crate::pdf_worker::CapacityFailure);
        assert!(matches!(
            classify(capacity, document_failure),
            ExtractionFailure::Fatal(_)
        ));
        assert!(matches!(
            classify(
                anyhow::anyhow!("PDF extraction refused: bad"),
                document_failure
            ),
            ExtractionFailure::Skip(_)
        ));
    }

    #[test]
    fn a_deferred_document_stands_on_its_previous_artifact_only_under_an_unchanged_binding() {
        let mut previous = airk_core::Snapshot::empty("root-a".into());
        previous.roots.insert("docs".into(), "docs-a".into());
        let mut roots = BTreeMap::from([
            (String::new(), "root-a".to_owned()),
            ("docs".to_owned(), "docs-a".to_owned()),
        ]);
        assert!(binding_unchanged("manual.pdf", &previous, &roots));
        assert!(binding_unchanged("@docs/manual.pdf", &previous, &roots));
        assert!(!binding_unchanged("@docs", &previous, &roots));
        assert!(!binding_unchanged("@other/manual.pdf", &previous, &roots));
        roots.insert("docs".into(), "docs-b".into());
        assert!(!binding_unchanged("@docs/manual.pdf", &previous, &roots));
        roots.insert(String::new(), "root-b".into());
        assert!(!binding_unchanged("manual.pdf", &previous, &roots));
    }

    #[test]
    fn a_busy_pdf_slot_defers_once_then_leaves_the_document_unsettled() -> Result<()> {
        let previous = indexed_before();
        let empty = airk_core::Snapshot::empty(ROOT.into());
        let emitted = Cell::new(0);
        // Copy: it captures only a shared reference, so each sink gets one.
        let emit = |_: &ExtractionProfile,
                    artifact: &Artifact,
                    _: &[u8],
                    _: Option<&ExtractionPayload>|
         -> Result<Artifact> {
            emitted.set(emitted.get() + 1);
            Ok(artifact.clone())
        };
        let mut pool = crate::extraction_pool::OrderedPool::new(
            crate::ExtractionExecution::default(),
            extract,
        )?;
        // Another document holds the one parser slot; every wait here expires
        // at once on a zero budget, so no parser starts.
        let held = hold_the_parser_slot()?;
        // An indexed document keeps its previous artifact; a new one stays absent.
        for (snapshot, carried) in [(&previous, true), (&empty, false)] {
            let mut sink = ScanSink::new(snapshot, crate::ScanLimits::default(), emit);
            let mut report = crate::IndexReport::default();
            let mut observation = observed();
            crate::pdf_worker::with_slot_wait(Duration::ZERO, || -> Result<()> {
                sink.finish(
                    crate::extraction_pool::execute(pdf_input(b"%PDF-1.4\nheld")?, extract),
                    &mut report,
                    &mut observation,
                )?;
                // Deferred, not settled: no outcome is reported before the retry.
                assert_eq!(sink.artifacts.contains_key(PDF), carried);
                assert!(report.skipped.is_empty());
                // The one retry, at the next drain, finds the slot still busy.
                sink.drain(&mut pool, &mut report, &mut observation)
            })?;
            assert!(!sink.failed);
            assert_eq!(report.indexed, 0);
            assert_eq!(
                sink.artifacts.get(PDF).map(|old| old.content_hash.as_str()),
                carried.then_some(OLD_HASH)
            );
            assert_eq!(report.skipped.len(), 1);
            assert_eq!(report.skipped[0].path, PDF);
            assert_eq!(report.skipped[0].code.as_deref(), Some(DEFERRED_CODE));
            assert!(
                observation.disappeared.is_empty(),
                "{:?}",
                observation.disappeared
            );
            assert_eq!(observation.usage.parser_calls, 1, "exactly one retry");
        }
        drop(held);
        assert_eq!(emitted.get(), 0);
        Ok(())
    }

    #[test]
    fn a_deferred_pdf_is_retried_at_the_next_drain_and_settled_by_its_real_outcome() -> Result<()> {
        let previous = indexed_before();
        let mut sink = ScanSink::new(
            &previous,
            crate::ScanLimits::default(),
            |_: &ExtractionProfile,
             artifact: &Artifact,
             _: &[u8],
             _: Option<&ExtractionPayload>|
             -> Result<Artifact> { Ok(artifact.clone()) },
        );
        let mut pool = crate::extraction_pool::OrderedPool::new(
            crate::ExtractionExecution::default(),
            extract,
        )?;
        let mut report = crate::IndexReport::default();
        let mut observation = observed();
        let held = hold_the_parser_slot()?;
        crate::pdf_worker::with_slot_wait(Duration::ZERO, || {
            sink.finish(
                crate::extraction_pool::execute(pdf_input(b"%PDF-1.4\nmalformed")?, extract),
                &mut report,
                &mut observation,
            )
        })?;
        assert_eq!(sink.artifacts[PDF].content_hash, OLD_HASH);
        drop(held);
        // The retry waits its turn with the ordinary budget; a real parser
        // child refuses the malformed document.
        sink.drain(&mut pool, &mut report, &mut observation)?;
        assert!(!sink.failed);
        assert!(
            !sink.artifacts.contains_key(PDF),
            "the stand-in was not removed"
        );
        assert_eq!(
            observation.disappeared.get(PDF),
            Some(&DisappearanceCause::ExtractionFailed)
        );
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].code.as_deref(), Some("extraction_failed"));
        assert_eq!(observation.usage.parser_calls, 1);
        Ok(())
    }

    #[test]
    fn an_interrupted_pdf_wait_stops_the_scan_and_settles_nothing() -> Result<()> {
        let previous = indexed_before();
        let mut sink = ScanSink::new(
            &previous,
            crate::ScanLimits::default(),
            |_: &ExtractionProfile,
             _: &Artifact,
             _: &[u8],
             _: Option<&ExtractionPayload>|
             -> Result<Artifact> {
                anyhow::bail!("an interrupted document must not be retained")
            },
        );
        let mut report = crate::IndexReport::default();
        let mut observation = observed();
        let error = crate::pdf_slot::with_interruption(std::sync::Arc::new(|| true), || {
            sink.finish(
                crate::extraction_pool::execute(pdf_input(b"%PDF-1.4\nstopped")?, extract),
                &mut report,
                &mut observation,
            )
        })
        .expect_err("an interrupted wait must stop the scan");
        assert!(
            crate::pdf_slot::slot_outcome(&error)
                == Some(crate::pdf_slot::SlotOutcome::Interrupted),
            "{error:#}"
        );
        assert!(sink.failed);
        // Neither stood in for, deferred, skipped nor reconciled as missing.
        assert!(sink.artifacts.is_empty());
        assert!(sink.deferred.is_none());
        assert!(report.skipped.is_empty());
        assert!(observation.disappeared.is_empty());
        Ok(())
    }
}
