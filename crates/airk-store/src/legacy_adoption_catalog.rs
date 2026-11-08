// SPDX-License-Identifier: Apache-2.0
//! Adoption-only decoder for receipt-bound generation-4/5 saved catalogs.
//! Never enables obsolete formats in ordinary retrieval or executes old models.
use super::{CaptureLimits, LegacyInventory, SavedFile};
use crate::{FileStore, PersistentRevision};
use airk_core::{
    Snapshot,
    identity::{RevisionToken, Sha256Digest},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs::File, time::Instant};

#[path = "legacy_adoption_inventory.rs"]
mod inventory;
const ENGINE_2: &str = "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-4/inventory-2";
const ENGINE_3_SOURCE: &str = "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-5/inventory-2/model-objects-1/source-context-1";
const ENGINE_3: &str =
    "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-5/inventory-2/model-objects-1";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    engine: String,
    token: RevisionToken,
    collection: String,
    chunks: usize,
    vectors: usize,
    model_sha256: Option<String>,
    embedding: String,
    inventory_digest: Sha256Digest,
    components: BTreeMap<String, SavedFile>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalCatalogPublication {
    pub receipt: airk_core::identity::PublicationReceipt,
    pub manifest_schema: u32,
    pub engine: String,
    pub inventory_version: u32,
    pub index_detail: Option<airk_core::extraction::IndexDetail>,
    pub extraction_policy_digest: Option<Sha256Digest>,
    pub embedding_pipeline_digest: Option<Sha256Digest>,
    pub embedding_model: Option<String>,
    pub components: BTreeMap<String, SavedFile>,
    pub resource_conversion: String,
}
pub(super) struct HistoricalCatalog {
    pub original: OriginalCatalogPublication,
    pub snapshot: Snapshot,
    _lease: File,
}
/// A format discriminator only. Unknown formats still reach the unchanged
/// ordinary reader and its explicit refusal; no permissive fallback exists.
pub(super) fn present(store: &FileStore, header: &PersistentRevision) -> Result<bool> {
    if header.storage_format != 2 {
        return Ok(false);
    }
    let token = header.token.as_ref().context("saved v2 token missing")?;
    let receipt = store.receipt(token)?.context("saved v2 receipt missing")?;
    let bytes = crate::blobs::read_verified(
        &store
            .directory()
            .join("generations")
            .join(&header.revision)
            .join("manifest.json"),
        &receipt.manifest_digest.to_string(),
        131_072,
    )?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    Ok(matches!(
        (
            value.get("schema").and_then(|x| x.as_u64()),
            value.get("engine").and_then(|x| x.as_str())
        ),
        (Some(2), Some(ENGINE_2)) | (Some(3), Some(ENGINE_3 | ENGINE_3_SOURCE))
    ))
}
pub(super) fn open(
    store: &FileStore,
    header: &PersistentRevision,
    sealed: &LegacyInventory,
    limits: CaptureLimits,
    until: Instant,
) -> Result<HistoricalCatalog> {
    ensure!(
        Instant::now() < until,
        "historical catalog deadline exhausted"
    );
    let token = header
        .token
        .as_ref()
        .context("historical v2 token missing")?;
    let receipt = store
        .receipt(token)?
        .context("historical committed receipt missing")?;
    let lease = File::open(crate::blobs::reader_lease_path(
        store.directory(),
        &header.revision,
    )?)?;
    lease.lock_shared()?;
    let directory = store
        .directory()
        .join("generations")
        .join(token.revision.to_string());
    let manifest: Manifest = serde_json::from_slice(&crate::blobs::read_verified(
        &directory.join("manifest.json"),
        &receipt.manifest_digest.to_string(),
        131_072,
    )?)?;
    verify_manifest(&manifest, &receipt, sealed, until)?;
    let bytes = crate::blobs::read_verified(
        &directory.join("inventory.json"),
        &receipt.inventory_digest.to_string(),
        limits.max_snapshot_bytes,
    )?;
    ensure!(
        airk_core::identity::revision_digest(manifest.engine.as_bytes(), &bytes) == token.revision,
        "historical revision byte digest mismatch"
    );
    let inventory = inventory::LegacyInventory::from_bytes(&bytes)?;
    ensure!(
        inventory.incarnation == token.incarnation
            && inventory.generation == token.generation
            && inventory.collection == manifest.collection
            && inventory.retrieval_specification == manifest.engine
            && inventory.embedding_model == manifest.model_sha256
            && inventory
                .files
                .values()
                .try_fold(0usize, |sum, file| sum.checked_add(file.chunks))
                == Some(manifest.chunks)
            && manifest.vectors <= manifest.chunks
            && inventory.lineage_digest == manifest.components["lineage.json"].sha256,
        "historical inventory manifest binding mismatch"
    );
    let lineage: airk_core::changes::LineageEnvelope =
        serde_json::from_slice(&crate::blobs::read_verified(
            &directory.join("lineage.json"),
            &inventory.lineage_digest.to_string(),
            airk_core::changes::MAX_LINEAGE_BYTES as u64,
        )?)?;
    lineage.validate()?;
    ensure!(
        lineage.incarnation == inventory.incarnation
            && lineage.observation.generation == inventory.generation,
        "historical lineage identity mismatch"
    );
    if let Some(component) = manifest.components.get("model-objects.json") {
        ensure!(
            component.bytes <= airk_core::model_objects::MAX_MODEL_OBJECT_SET_BYTES as u64,
            "historical model resource exceeds bound"
        );
        let model =
            airk_core::model_objects::ModelObjectSet::from_bytes(&crate::blobs::read_verified(
                &directory.join("model-objects.json"),
                &component.sha256.to_string(),
                component.bytes,
            )?)?;
        ensure!(
            Some(model.legacy_identity.as_str()) == inventory.embedding_model.as_deref()
                && inventory
                    .embedding_pipeline_digest
                    .is_none_or(|digest| digest == model.pipeline_digest),
            "historical model identity mismatch"
        );
        crate::model_objects::ModelObjects::new(store.directory())
            .verify(&model, limits.max_bytes)?;
    }
    crate::sqlite::verify_prepared_catalog(&directory, &receipt)?;
    let snapshot = inventory.projection()?;
    ensure!(
        snapshot.root == header.root
            && snapshot.collection == header.collection
            && snapshot.roots == header.roots
            && snapshot.artifacts.len() == header.files,
        "historical pinned catalog scope mismatch"
    );
    ensure!(
        store.receipt(token)?.as_ref() == Some(&receipt) && Instant::now() < until,
        "historical receipt changed or deadline exhausted"
    );
    let original = OriginalCatalogPublication {
        receipt, manifest_schema: manifest.schema, engine: manifest.engine,
        inventory_version: inventory.version, index_detail: inventory.index_detail,
        extraction_policy_digest: inventory.extraction_policy_digest,
        embedding_pipeline_digest: inventory.embedding_pipeline_digest,
        embedding_model: inventory.embedding_model, components: manifest.components,
        resource_conversion: "saved artifacts and analysis converted; original retrieval/model components archived only".into(),
    };
    Ok(HistoricalCatalog {
        snapshot,
        original,
        _lease: lease,
    })
}

fn verify_manifest(
    manifest: &Manifest,
    receipt: &airk_core::identity::PublicationReceipt,
    sealed: &LegacyInventory,
    until: Instant,
) -> Result<()> {
    let prefix = format!("generations/{}/", receipt.token.revision);
    ensure!(
        (manifest.schema == 2 && manifest.engine == ENGINE_2)
            || (manifest.schema == 3
                && matches!(manifest.engine.as_str(), ENGINE_3 | ENGINE_3_SOURCE)),
        "unsupported historical catalog engine"
    );
    ensure!(
        manifest.token == receipt.token
            && manifest.inventory_digest == receipt.inventory_digest
            && manifest.components.len() <= 1024
            && manifest.embedding.len() <= 4096
            && manifest
                .model_sha256
                .as_ref()
                .is_none_or(|x| crate::blobs::valid_digest(x)),
        "historical manifest receipt mismatch"
    );
    ensure!(
        manifest.schema != 2 || !manifest.components.contains_key("model-objects.json"),
        "historical schema2 model representation mismatch"
    );
    ensure!(
        manifest.schema != 3
            || (!manifest.components.keys().any(|x| x.starts_with("model/"))
                && manifest.components.contains_key("model-objects.json")
                    == manifest.model_sha256.is_some()),
        "historical schema3 model representation mismatch"
    );
    for name in ["inventory.json", "lineage.json", "catalog.sqlite"] {
        ensure!(
            manifest.components.contains_key(name),
            "historical required component missing: {name}"
        );
    }
    ensure!(
        manifest.components["inventory.json"].sha256 == receipt.inventory_digest,
        "historical inventory digest binding mismatch"
    );
    // capture_tree authenticated the complete sealed preimage immediately before
    // this reader. Compare every anchored component against that bounded census.
    let actual = sealed
        .files
        .keys()
        .filter_map(|name| name.strip_prefix(&prefix))
        .filter(|name| *name != "manifest.json")
        .collect::<Vec<_>>();
    ensure!(
        actual.len() == manifest.components.len()
            && actual
                .iter()
                .all(|name| manifest.components.contains_key(*name)),
        "historical generation component set mismatch"
    );
    for (name, component) in &manifest.components {
        ensure!(
            Instant::now() < until,
            "historical component verification deadline exhausted"
        );
        ensure!(
            !name.is_empty()
                && name.len() <= 4096
                && !name.contains(['\\', ':', '\0'])
                && name != "manifest.json"
                && name
                    .split('/')
                    .all(|p| !p.is_empty() && p != "." && p != ".."),
            "unsafe historical component path"
        );
        ensure!(
            sealed.files.get(&format!("{prefix}{name}")) == Some(component),
            "historical component differs from anchored bytes: {name}"
        );
    }
    for name in sealed
        .directories
        .iter()
        .filter_map(|name| name.strip_prefix(&prefix))
    {
        ensure!(
            manifest
                .components
                .keys()
                .any(|component| component.starts_with(&format!("{name}/"))),
            "unlisted historical generation directory"
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "legacy_adoption_catalog_tests.rs"]
mod tests;
