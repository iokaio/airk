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
        let payload =
            crate::composite::extract(&input.path, &input.bytes, &input.profile).map_err(Skip)?;
        // Both preparation paths call payload.validate(), including its serialized
        // MAX_PAYLOAD_BYTES check, before any completed result can enter the pool.
        return Ok(payload);
    }
    let producer =
        crate::retention::producer_profile(&input.path, &input.profile).map_err(Fatal)?;
    let extracted = crate::documents::extract_selected(&input.path, &input.bytes, &producer)
        .map_err(|error| {
            if error.is::<crate::pdf_worker::CapacityFailure>() {
                Fatal(error)
            } else {
                Skip(error)
            }
        })?;
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

    pub fn drain(
        &mut self,
        pool: &mut crate::extraction_pool::OrderedPool<CapturedExtraction, FreshExtraction>,
        report: &mut crate::IndexReport,
        observation: &mut crate::observations::ScanObservation,
    ) -> Result<()> {
        while let Some(completed) = pool.next().inspect_err(|_| self.failed = true)? {
            self.finish(completed, report, observation)?;
        }
        Ok(())
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
                let output=prepare(&input).map_err(|failure|match failure{ExtractionFailure::Fatal(error)|ExtractionFailure::Skip(error)=>error})?;output.validate()?;
                assert!(output.facts.iter().any(|fact|fact.kind=="package_identity"));assert!(output.facts.iter().any(|fact|fact.kind=="package_dependency"));assert!(matches!(AppliedRetention::from_profile(&input.profile)?,AppliedRetention::Selected{..}));outputs.push(output);
            }
            assert_eq!(serde_json::to_vec(&outputs[0].semantic_projection)?,serde_json::to_vec(&outputs[1].semantic_projection)?);
            for fact in &outputs[0].facts {assert!(outputs[1].facts.iter().any(|other|other.kind==fact.kind&&other.name==fact.name&&other.location==fact.location));}
        }
        Ok(())
    }
}
