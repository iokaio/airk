// SPDX-License-Identifier: Apache-2.0
//! Explicit neutral projections for fictional retrieval fixtures only.
use airk_core::{
    DocumentLocation, RetrievalArtifact, RetrievalSource, Snapshot,
    extraction::{
        ExtractionPayload, ExtractionProfile, LocalChunk, LocalSpan, SemanticBoundary,
        SemanticProjection,
    },
    identity::Sha256Digest,
};
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Deliberately in-memory, original test fixture; runtime uses charged disk staging.
#[derive(Default)]
pub struct FixtureRegistry(
    pub Mutex<BTreeMap<Sha256Digest, airk_core::vector_selection::PreparedVectorNode>>,
);

impl airk_core::vector_selection::VectorNodeRegistry for FixtureRegistry {
    fn lookup(
        &self,
        key: Sha256Digest,
    ) -> Result<Option<airk_core::vector_selection::PreparedVectorNode>> {
        Ok(self.0.lock().unwrap().get(&key).copied())
    }
    fn remember(
        &self,
        key: Sha256Digest,
        node: airk_core::vector_selection::PreparedVectorNode,
    ) -> Result<()> {
        ensure!(
            node.graph_id.is_none_or(|id| id > 0),
            "invalid fixture node"
        );
        let mut entries = self.0.lock().unwrap();
        ensure!(
            entries.get(&key).is_none_or(|old| old == &node),
            "conflicting fixture node"
        );
        entries.insert(key, node);
        Ok(())
    }
}

pub struct FixtureSource {
    pub artifacts: BTreeMap<String, RetrievalArtifact>,
}

pub fn profile() -> ExtractionProfile {
    ExtractionProfile {
        contract_version: 1,
        adapter_id: "text".into(),
        adapter_version: "original-retrieval-fixture/1".into(),
        language: "text".into(),
        dialect: None,
        grammar_identity: None,
        mapping_version: 1,
        chunk_version: 3,
        options: BTreeMap::new(),
        configuration_inputs: BTreeMap::new(),
    }
}

impl FixtureSource {
    /// These tests define each fictional chunk as a separate semantic group.
    /// Mapped fixtures must supply their original host/region/source-map explicitly.
    pub fn new(snapshot: &Snapshot) -> Result<Self> {
        let mut artifacts = BTreeMap::new();
        for (path, artifact) in &snapshot.artifacts {
            let mut text = String::new();
            let mut chunks = Vec::new();
            let mut boundaries = Vec::new();
            for (index, chunk) in artifact.chunks.iter().enumerate() {
                ensure!(
                    !matches!(chunk.location, DocumentLocation::Mapped { .. }),
                    "mapped fixture requires an explicit neutral source map"
                );
                let start = text.len();
                text.push_str(&chunk.text);
                let span = LocalSpan {
                    start,
                    end: text.len(),
                };
                chunks.push(LocalChunk {
                    span: span.clone(),
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    location: chunk.location.clone(),
                    region: None,
                });
                if start != text.len() {
                    boundaries.push(SemanticBoundary {
                        region: None,
                        span,
                        group: index as u32,
                        context: None,
                        split_context: None,
                    });
                }
            }
            let payload = ExtractionPayload {
                contract_version: 1,
                extraction_method: artifact.extraction_method.clone(),
                parse_status: artifact.parse_status.clone(),
                provenance: vec![],
                text,
                facts: vec![],
                syntax_identities: vec![],
                chunks,
                regions: vec![],
                relationships: vec![],
                continuity: vec![],
                continuity_coverage: BTreeMap::new(),
                semantic_projection: SemanticProjection {
                    version: 1,
                    boundaries,
                },
                diagnostics: vec![],
                partial: false,
            };
            payload.validate()?;
            let mut profile = profile();
            profile.language = artifact.language.clone();
            artifacts.insert(
                path.clone(),
                RetrievalArtifact {
                    artifact: artifact.clone(),
                    profile,
                    payload_digest: Sha256Digest::from_bytes(
                        Sha256::digest(serde_json::to_vec(&payload)?).into(),
                    ),
                    payload,
                },
            );
        }
        Ok(Self { artifacts })
    }
}

impl RetrievalSource for FixtureSource {
    fn load_retrieval_artifact(&self, path: &str) -> Result<RetrievalArtifact> {
        let value = self
            .artifacts
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("fictional artifact missing"))?;
        Ok(RetrievalArtifact {
            artifact: value.artifact.clone(),
            profile: value.profile.clone(),
            payload: value.payload.clone(),
            payload_digest: value.payload_digest,
        })
    }
}
