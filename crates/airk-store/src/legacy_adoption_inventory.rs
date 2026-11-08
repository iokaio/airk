// SPDX-License-Identifier: Apache-2.0
//! Adoption-only inventory decoder for the authentic pre-Current generations.
//! Retains the historical schema and canonical identity; never used by normal readers.
//! Validation/projection follows AIRK inventory 2/3 through v1.1.4, including
//! its optional detail field and bounded larger-source contract.
use airk_core::{
    AnalysisManifest, Artifact, ArtifactRef, FactHit, RelationshipHit, Snapshot,
    extraction::ExtractionProfile,
    identity::{Incarnation, Sha256Digest},
    publication::InventoryFile,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const INVENTORY_VERSION: u32 = 3;
const SNAPSHOT_PROJECTION_VERSION: u32 = 4;
const MAX_INVENTORY_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LegacyInventory {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) index_detail: Option<airk_core::extraction::IndexDetail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) embedding_pipeline_digest: Option<Sha256Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) extraction_policy_digest: Option<Sha256Digest>,
    pub(super) version: u32,
    pub(super) incarnation: Incarnation,
    pub(super) generation: u64,
    pub(super) collection: String,
    pub(super) root: String,
    pub(super) roots: BTreeMap<String, String>,
    pub(super) lineage_digest: Sha256Digest,
    pub(super) profiles: BTreeMap<Sha256Digest, ExtractionProfile>,
    pub(super) files: BTreeMap<String, InventoryFile>,
    pub(super) retrieval_specification: String,
    pub(super) embedding_model: Option<String>,
    pub(super) extractor_version: String,
    /// Provider assertions remain bound to their own source/build manifest.
    pub(super) analysis: Option<AnalysisManifest>,
    pub(super) imported_facts: Vec<FactHit>,
    pub(super) imported_relationships: Vec<RelationshipHit>,
}

impl LegacyInventory {
    pub(super) fn from_bytes(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_INVENTORY_BYTES,
            "historical inventory exceeds byte bound"
        );
        let inventory: Self = serde_json::from_slice(bytes)?;
        ensure!(
            inventory.canonical_bytes()? == bytes,
            "historical inventory is not canonical"
        );
        Ok(inventory)
    }

    pub(super) fn validate(&self) -> Result<()> {
        ensure!(
            (2..=INVENTORY_VERSION).contains(&self.version),
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
                matches!(profile.contract_version, 1 | 2),
                "unsupported historical profile contract"
            );
            ensure!(
                *identity == profile.identity()?,
                "profile identity mismatch"
            );
        }
        let mut artifacts = BTreeSet::new();
        let mut continuity_keys = BTreeSet::new();
        for (path, file) in &self.files {
            file.state.validate(self.generation)?;
            ensure!(
                file.continuity.len() <= airk_core::continuity::MAX_CONTINUITY_UNITS
                    && serde_json::to_vec(&file.continuity)?.len()
                        <= airk_core::continuity::MAX_CONTINUITY_BYTES,
                "continuity inventory budget exceeded"
            );
            let mut subjects = BTreeSet::new();
            for record in &file.continuity {
                use airk_core::continuity::{ContinuityFamily, ContinuityKey};
                ensure!(
                    continuity_keys.insert(record.key)
                        && subjects.insert(record.descriptor.subject),
                    "duplicate continuity identity"
                );
                ensure!(
                    matches!(
                        (record.key, record.descriptor.family),
                        (ContinuityKey::Symbol(_), ContinuityFamily::Code)
                            | (ContinuityKey::Document(_), ContinuityFamily::Documents)
                    ),
                    "continuity key family mismatch"
                );
                ensure!(
                    record.first_generation >= file.state.stamps.first_generation
                        && record.first_generation <= self.generation
                        && record.changed.len() <= 14
                        && record
                            .changed
                            .values()
                            .all(|stamp| *stamp >= record.first_generation
                                && *stamp <= self.generation),
                    "invalid continuity generation stamps"
                );
                ensure!(
                    record
                        .descriptor
                        .dimensions
                        .keys()
                        .all(|dimension| record.changed.contains_key(dimension))
                        && record.changed.keys().all(|dimension| *dimension
                            == airk_core::changes::ChangeDimension::Location
                            || record.descriptor.dimensions.contains_key(dimension)),
                    "continuity stamps do not match supported dimensions"
                );
                ensure!(
                    record.descriptor.region.is_some()
                        || record.descriptor.profile == file.state.profile_id,
                    "continuity profile binding mismatch"
                );
                ensure!(
                    record.descriptor.algorithm_version
                        == airk_core::continuity::CONTINUITY_VERSION
                        && !record.descriptor.category.is_empty()
                        && record.descriptor.category.len() <= 256
                        && record.descriptor.dimensions.len() <= 12
                        && record.descriptor.unsupported_dimensions.len() <= 12
                        && record
                            .descriptor
                            .unsupported_dimensions
                            .iter()
                            .all(|dimension| !record.descriptor.dimensions.contains_key(dimension)),
                    "invalid durable continuity descriptor"
                );
            }
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
                file.source_bytes <= 256 * 1024 * 1024
                    && file.payload_bytes <= 256 * 1024 * 1024
                    && file.extracted_bytes <= 32 * 1024 * 1024
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

    pub(super) fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_INVENTORY_BYTES,
            "inventory exceeds byte bound"
        );
        Ok(bytes)
    }

    pub(super) fn projection(&self) -> Result<Snapshot> {
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
                            legacy_origin: None,
                            source_encoding: None,
                            payload_encoding: None,
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
            // Storage engine generation and inventory schema evolve independently.
            storage_format: 2,
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
    use airk_core::{
        changes::{FileStamps, FileState},
        identity::ArtifactId,
    };

    fn inventory() -> LegacyInventory {
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
        LegacyInventory {
            embedding_pipeline_digest: None,
            index_detail: None,
            extraction_policy_digest: None,
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
                    continuity: Vec::new(),
                    continuity_coverage: Default::default(),
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
        let decoded = LegacyInventory::from_bytes(&bytes)?;
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

    #[test]
    fn historical_schema_and_canonical_bytes_are_not_replaced_by_current_defaults() -> Result<()> {
        for version in [2, 3] {
            let mut value = inventory();
            value.version = version;
            for detail in [None, Some(airk_core::extraction::IndexDetail::Detailed)] {
                value.index_detail = detail;
                value.retrieval_specification = if detail.is_some() {
                    "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-5/inventory-2/model-objects-1/source-context-1"
                } else {
                    "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-5/inventory-2/model-objects-1"
                }.into();
                let bytes = value.canonical_bytes()?;
                let decoded = LegacyInventory::from_bytes(&bytes)?;
                assert_eq!(decoded.canonical_bytes()?, bytes);
                assert_eq!(
                    decoded.retrieval_specification,
                    value.retrieval_specification
                );
                assert_eq!(decoded.index_detail, detail);
                let original: serde_json::Value = serde_json::from_slice(&bytes)?;
                assert!(original.get("vector_selection").is_none());
                assert!(original.get("embedding_pipeline_digest").is_none());
                assert_eq!(original.get("index_detail").is_some(), detail.is_some());
                let mut padded = bytes.clone();
                padded.push(b' ');
                assert!(LegacyInventory::from_bytes(&padded).is_err());
                let mut modern = serde_json::to_value(&value)?;
                modern["vector_selection"] = serde_json::json!({});
                assert!(LegacyInventory::from_bytes(&serde_json::to_vec(&modern)?).is_err());
            }
        }
        let mut value = inventory();
        value.version = 4;
        assert!(value.validate().is_err());
        Ok(())
    }
}
