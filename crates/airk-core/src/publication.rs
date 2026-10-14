// SPDX-License-Identifier: Apache-2.0
//! Authoritative, non-circular v2 inventory. Snapshot is only a compatibility view.
use crate::{
    AnalysisManifest, Artifact, ArtifactRef, FactHit, RelationshipHit, Snapshot,
    changes::FileState,
    extraction::ExtractionProfile,
    identity::{Incarnation, Sha256Digest},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const INVENTORY_VERSION: u32 = 2;
pub const SNAPSHOT_PROJECTION_VERSION: u32 = 4;
pub const MAX_INVENTORY_BYTES: usize = 134_217_728;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryFile {
    pub state: FileState,
    pub source_bytes: usize,
    pub payload_bytes: usize,
    pub extracted_bytes: usize,
    pub chunks: usize,
    pub indexed_lines: usize,
    pub facts: usize,
    pub raw_source: bool,
    pub language: String,
    pub extraction_method: String,
    pub parse_status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryV2 {
    pub version: u32,
    pub incarnation: Incarnation,
    pub generation: u64,
    pub collection: String,
    pub root: String,
    pub roots: BTreeMap<String, String>,
    pub lineage_digest: Sha256Digest,
    pub profiles: BTreeMap<Sha256Digest, ExtractionProfile>,
    pub files: BTreeMap<String, InventoryFile>,
    pub retrieval_specification: String,
    pub embedding_model: Option<String>,
    pub extractor_version: String,
    /// Provider assertions remain bound to their own source/build manifest.
    pub analysis: Option<AnalysisManifest>,
    pub imported_facts: Vec<FactHit>,
    pub imported_relationships: Vec<RelationshipHit>,
}

impl InventoryV2 {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == INVENTORY_VERSION,
            "unsupported inventory format"
        );
        ensure!(
            !self.root.is_empty() && self.root.len() <= 32_768,
            "invalid owner root"
        );
        ensure!(
            !self.collection.is_empty()
                && self.collection.len() <= 64
                && self
                    .collection
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
            "invalid collection identity"
        );
        ensure!(
            self.roots.len() <= 256
                && self.profiles.len() <= 10_000
                && self.files.len() <= 1_000_000,
            "inventory count limit exceeded"
        );
        ensure!(
            !self.retrieval_specification.is_empty()
                && self.retrieval_specification.len() <= 1024
                && self.extractor_version.len() <= 256,
            "invalid retrieval/extractor specification"
        );
        for (alias, root) in &self.roots {
            ensure!(
                !alias.is_empty()
                    && alias.len() <= 128
                    && alias
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                    && !root.is_empty()
                    && root.len() <= 32_768,
                "invalid source root binding"
            );
        }
        for (identity, profile) in &self.profiles {
            ensure!(
                *identity == profile.identity()?,
                "profile identity mismatch"
            );
        }
        let mut artifacts = BTreeSet::new();
        for (path, file) in &self.files {
            file.state.validate(self.generation)?;
            ensure!(
                artifacts.insert(file.state.artifact_id),
                "duplicate artifact identity"
            );
            let (expected_root, relative_path) = if let Some(aliased) = path.strip_prefix('@')
                && let Some((alias, relative)) = aliased.split_once('/')
                && let Some(root) = self.roots.get(alias)
            {
                (root, relative)
            } else {
                (&self.root, path.as_str())
            };
            ensure!(
                file.state.root == *expected_root
                    && file.state.path == relative_path
                    && path.len() <= 4096,
                "file path binding mismatch"
            );
            let profile = self
                .profiles
                .get(&file.state.profile_id)
                .ok_or_else(|| anyhow::anyhow!("file profile missing"))?;
            ensure!(
                file.language == profile.language,
                "file profile language mismatch"
            );
            ensure!(
                file.source_bytes <= 16_777_216
                    && file.payload_bytes <= 67_108_864
                    && file.extracted_bytes <= 16_777_216
                    && file.facts <= 10_000
                    && file.chunks <= 500_000,
                "file metadata exceeds bounds"
            );
            ensure!(
                file.language.len() <= 256
                    && file.extraction_method.len() <= 256
                    && file.parse_status.len() <= 65_536,
                "file provenance exceeds bounds"
            );
        }
        ensure!(
            self.analysis.is_some()
                || (self.imported_facts.is_empty() && self.imported_relationships.is_empty()),
            "provider evidence has no source/build binding"
        );
        if let Some(analysis) = &self.analysis {
            ensure!(
                analysis.root == self.root && analysis.collection == self.collection,
                "foreign provider binding"
            );
            for (path, hash) in &analysis.source_hashes {
                ensure!(
                    self.files
                        .get(path)
                        .is_some_and(|f| f.state.source_hash.to_string() == *hash),
                    "stale provider source binding"
                );
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_INVENTORY_BYTES,
            "inventory exceeds byte bound"
        );
        Ok(bytes)
    }

    pub fn projection(&self) -> Result<Snapshot> {
        self.validate()?;
        let artifacts = self
            .files
            .iter()
            .map(|(path, file)| {
                (
                    path.clone(),
                    Artifact {
                        path: path.clone(),
                        content_hash: file.state.source_hash.to_string(),
                        bytes: file.source_bytes,
                        chunks: vec![],
                        language: file.language.clone(),
                        extraction_method: file.extraction_method.clone(),
                        parse_status: file.parse_status.clone(),
                        facts: vec![],
                        retained: Some(ArtifactRef {
                            object: file.state.payload_digest.to_string(),
                            object_bytes: file.payload_bytes,
                            raw_source: file.raw_source,
                            chunks: file.chunks,
                            extracted_bytes: file.extracted_bytes,
                            facts: file.facts,
                            profile: Some(self.profiles[&file.state.profile_id].clone()),
                        }),
                    },
                )
            })
            .collect();
        Ok(Snapshot {
            schema_version: SNAPSHOT_PROJECTION_VERSION,
            extractor_version: self.extractor_version.clone(),
            root: self.root.clone(),
            collection: self.collection.clone(),
            generation: self.generation,
            storage_format: INVENTORY_VERSION,
            roots: self.roots.clone(),
            persistent_indexes: true,
            embedding_model: self.embedding_model.clone(),
            artifacts,
            analysis: self.analysis.clone(),
            imported_facts: self.imported_facts.clone(),
            imported_relationships: self.imported_relationships.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{changes::FileStamps, identity::ArtifactId};

    fn inventory() -> InventoryV2 {
        let profile = ExtractionProfile {
            contract_version: 1,
            adapter_id: "text".into(),
            adapter_version: "native-facts/7".into(),
            language: "text".into(),
            dialect: None,
            grammar_identity: None,
            mapping_version: 1,
            chunk_version: 1,
            options: BTreeMap::new(),
            configuration_inputs: BTreeMap::new(),
        };
        let profile_id = profile.identity().unwrap();
        let digest = Sha256Digest::from_bytes([1; 32]);
        InventoryV2 {
            version: 2,
            incarnation: Incarnation::from_bytes([2; 16]),
            generation: 4,
            collection: "fixture".into(),
            root: "/original".into(),
            roots: BTreeMap::new(),
            lineage_digest: digest,
            profiles: BTreeMap::from([(profile_id, profile)]),
            files: BTreeMap::from([(
                "notes.txt".into(),
                InventoryFile {
                    state: FileState {
                        artifact_id: ArtifactId::from_bytes([3; 16]),
                        stamps: FileStamps {
                            first_generation: 1,
                            path_generation: 1,
                            content_generation: 2,
                            evidence_generation: 3,
                        },
                        source_hash: digest,
                        profile_id,
                        payload_digest: digest,
                        evidence_digest: digest,
                        root: "/original".into(),
                        path: "notes.txt".into(),
                    },
                    source_bytes: 5,
                    payload_bytes: 128,
                    extracted_bytes: 5,
                    chunks: 1,
                    indexed_lines: 1,
                    facts: 0,
                    raw_source: true,
                    language: "text".into(),
                    extraction_method: "utf8".into(),
                    parse_status: "plain".into(),
                },
            )]),
            retrieval_specification: "fixture-engine/1".into(),
            embedding_model: None,
            extractor_version: "native-facts/7".into(),
            analysis: None,
            imported_facts: vec![],
            imported_relationships: vec![],
        }
    }

    #[test]
    fn canonical_inventory_and_projection_keep_binding() -> Result<()> {
        let value = inventory();
        let bytes = value.canonical_bytes()?;
        let decoded: InventoryV2 = serde_json::from_slice(&bytes)?;
        assert_eq!(decoded.canonical_bytes()?, bytes);
        let projection = decoded.projection()?;
        assert_eq!(projection.schema_version, 4);
        assert_eq!(projection.storage_format, 2);
        assert_eq!(
            projection.artifacts["notes.txt"]
                .retained
                .as_ref()
                .unwrap()
                .profile
                .as_ref()
                .unwrap()
                .identity()?,
            value.files["notes.txt"].state.profile_id
        );
        let mut rebound = value.clone();
        rebound.root = "/elsewhere".into();
        assert!(rebound.validate().is_err());
        let mut changed = value;
        changed.profiles.values_mut().next().unwrap().chunk_version = 2;
        assert!(changed.validate().is_err());
        Ok(())
    }

    #[test]
    fn aliases_duplicate_handles_and_future_stamps_fail_closed() -> Result<()> {
        let mut value = inventory();
        let file = value.files.remove("notes.txt").unwrap();
        value.files.insert("@docs/notes.txt".into(), file);
        assert!(value.validate().is_err());
        value.roots.insert("docs".into(), "/original".into());
        value.validate()?;
        let mut duplicate = value.files["@docs/notes.txt"].clone();
        duplicate.state.path = "copy.txt".into();
        value.files.insert("@docs/copy.txt".into(), duplicate);
        assert!(value.validate().is_err());
        value.files.remove("@docs/copy.txt");
        value
            .files
            .get_mut("@docs/notes.txt")
            .unwrap()
            .state
            .stamps
            .content_generation = 5;
        assert!(value.validate().is_err());
        Ok(())
    }
}
