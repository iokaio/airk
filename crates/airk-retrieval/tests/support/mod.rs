// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]
pub mod source;
use airk_core::{
    RetrievalSource, Snapshot,
    changes::{FileStamps, FileState},
    identity::{
        ArtifactId, Incarnation, PublicationReceipt, RevisionToken, Sha256Digest, revision_digest,
    },
    model_objects::ModelObjectSet,
    publication::{InventoryFile, InventoryV2},
};
use airk_retrieval::{
    model,
    persistent::{self, V2Preparation},
};
use anyhow::Result;
use sha2::{Digest, Sha256};
pub use source::{FixtureRegistry, FixtureSource};
use std::{collections::BTreeMap, fs, path::Path};

pub const LINEAGE: &[u8] = b"original retrieval fixture lineage";
pub fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

pub fn install_model(base: &Path, directory: &Path) -> Result<ModelObjectSet> {
    let set = model::inspect_object_plan(directory)?;
    let objects = base.join("objects/models");
    fs::create_dir_all(&objects)?;
    for (name, reference) in &set.assets {
        fs::copy(
            directory.join(name.filename()),
            objects.join(reference.sha256.to_string()),
        )?;
    }
    Ok(set)
}

pub fn inventory(
    snapshot: &Snapshot,
    source: &dyn RetrievalSource,
    model: Option<&ModelObjectSet>,
) -> Result<InventoryV2> {
    let mut profiles = BTreeMap::new();
    let mut files = BTreeMap::new();
    for (index, path) in snapshot.artifacts.keys().enumerate() {
        let value = source.load_retrieval_artifact(path)?;
        let artifact = &value.artifact;
        let profile_id = value.profile.identity()?;
        profiles.insert(profile_id, value.profile);
        files.insert(
            path.clone(),
            InventoryFile {
                state: FileState {
                    artifact_id: ArtifactId::from_bytes((index as u128 + 1).to_le_bytes()),
                    stamps: FileStamps {
                        first_generation: 1,
                        path_generation: 1,
                        content_generation: 1,
                        evidence_generation: 1,
                    },
                    source_hash: artifact.content_hash.parse()?,
                    profile_id,
                    payload_digest: value.payload_digest,
                    evidence_digest: value.payload_digest,
                    root: snapshot.root.clone(),
                    path: path.clone(),
                },
                source_bytes: artifact.bytes,
                payload_bytes: serde_json::to_vec(&value.payload)?.len(),
                extracted_bytes: artifact.extracted_bytes(),
                chunks: artifact.chunks.len(),
                indexed_lines: artifact
                    .chunks
                    .iter()
                    .map(|chunk| chunk.end_line - chunk.start_line + 1)
                    .sum(),
                facts: 0,
                raw_source: true,
                language: artifact.language.clone(),
                extraction_method: artifact.extraction_method.clone(),
                parse_status: artifact.parse_status.clone(),
                continuity: vec![],
                continuity_coverage: BTreeMap::new(),
            },
        );
    }
    Ok(InventoryV2 {
        version: 4,
        vector_selection: Default::default(),
        incarnation: Incarnation::from_bytes([1; 16]),
        generation: snapshot.generation.max(1),
        collection: snapshot.collection.clone(),
        root: snapshot.root.clone(),
        roots: BTreeMap::new(),
        lineage_digest: digest(LINEAGE),
        profiles,
        files,
        retrieval_specification: String::from_utf8(persistent::retrieval_specification().to_vec())?,
        embedding_model: model.map(|set| set.legacy_identity.clone()),
        embedding_pipeline_digest: model.map(|set| set.pipeline_digest),
        extractor_version: snapshot.extractor_version.clone(),
        analysis: None,
        imported_facts: vec![],
        imported_relationships: vec![],
        index_detail: None,
        extraction_policy_digest: None,
    })
}

pub fn prepare(
    base: &Path,
    snapshot: &Snapshot,
    source: &dyn RetrievalSource,
    directory: Option<&Path>,
) -> Result<PublicationReceipt> {
    prepare_options(
        base,
        snapshot,
        source,
        directory,
        Default::default(),
        &FixtureRegistry::default(),
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn prepare_options(
    base: &Path,
    snapshot: &Snapshot,
    source: &dyn RetrievalSource,
    directory: Option<&Path>,
    selection: airk_core::vector_selection::VectorSelectionPolicy,
    registry: &dyn airk_core::vector_selection::VectorNodeRegistry,
    cache: Option<&mut model::EmbeddingCacheContext<'_>>,
) -> Result<PublicationReceipt> {
    let set = directory
        .map(|directory| install_model(base, directory))
        .transpose()?;
    let mut inventory = inventory(snapshot, source, set.as_ref())?;
    inventory.vector_selection = selection;
    let canonical = inventory.canonical_bytes()?;
    let token = RevisionToken {
        incarnation: inventory.incarnation,
        generation: inventory.generation,
        revision: revision_digest(persistent::retrieval_specification(), &canonical),
    };
    persistent::prepare_streaming(
        base,
        &V2Preparation {
            snapshot: &inventory.projection()?,
            canonical_inventory: &canonical,
            token,
        },
        set.as_ref(),
        source,
        Some(registry),
        None,
        None,
        cache,
        |directory| {
            fs::write(
                directory.join("catalog.sqlite"),
                b"original closed fixture catalog",
            )?;
            fs::write(directory.join("lineage.json"), LINEAGE)?;
            Ok(())
        },
        |_| Ok(()),
    )
}
