// SPDX-License-Identifier: Apache-2.0
//! Producer assertions owned by the current head, bound to exact indexed sources.
use super::CurrentRead;
use crate::FileStore;
use airk_core::{AnalysisManifest, Fact, FactHit, RelationshipHit};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisEvidence {
    pub manifest: AnalysisManifest,
    pub facts: Vec<FactHit>,
    pub relationships: Vec<RelationshipHit>,
}

impl AnalysisEvidence {
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            self.facts.len() <= 100_000
                && self.relationships.len() <= 100_000
                && self.manifest.source_hashes.len() <= 100_000,
            "current analysis record bound exceeded"
        );
        for citation in self
            .facts
            .iter()
            .map(|f| &f.citation)
            .chain(self.relationships.iter().map(|e| &e.citation))
        {
            ensure!(
                self.manifest.source_hashes.get(&citation.path) == Some(&citation.content_hash)
                    && citation.start_line > 0
                    && citation.end_line >= citation.start_line,
                "current analysis source binding mismatch"
            );
        }
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= 64 << 20,
            "current analysis byte bound exceeded"
        );
        Ok(bytes)
    }

    pub(crate) fn native_facts(&self) -> BTreeMap<String, Vec<Fact>> {
        let mut facts: BTreeMap<String, Vec<Fact>> = BTreeMap::new();
        for hit in &self.facts {
            facts
                .entry(hit.citation.path.clone())
                .or_default()
                .push(Fact {
                    name: hit.name.clone(),
                    kind: hit.kind.clone(),
                    syntax_kind: hit.syntax_kind.clone(),
                    start_line: hit.citation.start_line,
                    end_line: hit.citation.end_line,
                    location: hit.citation.location.clone(),
                    document: hit.document.clone(),
                    code: hit.code.clone(),
                });
        }
        facts
    }
}

impl CurrentRead {
    pub fn analysis(&self, store: &FileStore) -> Result<Option<AnalysisEvidence>> {
        let Some(reference) = self.resource_reference("analysis")? else {
            return Ok(None);
        };
        ensure!(
            reference.bytes <= 64 << 20,
            "current analysis byte bound exceeded"
        );
        let bytes = store.read_current_resource(&reference)?;
        let evidence: AnalysisEvidence = serde_json::from_slice(&bytes)?;
        ensure!(
            evidence.canonical_bytes()? == bytes
                && evidence.manifest.root == self.head().scope.root
                && evidence.manifest.collection == self.head().scope.collection,
            "current analysis ownership mismatch"
        );
        Ok(Some(evidence))
    }

    pub(crate) fn verify_analysis_path(
        &self,
        evidence: &AnalysisEvidence,
        path: &str,
    ) -> Result<()> {
        let artifact = self
            .artifact(path)?
            .context("current analysis artifact missing")?;
        ensure!(
            evidence.manifest.source_hashes.get(path) == Some(&artifact.content_hash),
            "current analysis source expired"
        );
        Ok(())
    }
}
