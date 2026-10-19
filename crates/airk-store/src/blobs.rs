// SPDX-License-Identifier: Apache-2.0
//! Immutable content-addressed source and extraction objects. No source workspace writes.
use crate::FileStore;
use airk_core::{Artifact, ArtifactRef, ArtifactSource, Snapshot};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_ARTIFACT_BYTES: u64 = airk_core::extraction::MAX_PAYLOAD_BYTES as u64;
pub const MAX_CATALOG_BYTES: u64 = airk_core::publication::MAX_INVENTORY_BYTES as u64;

pub fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn reader_lease_path(base: &Path, revision: &str) -> Result<PathBuf> {
    ensure!(valid_digest(revision), "invalid revision identity");
    let parent = base.parent().context("collection parent missing")?;
    let directory = parent
        .join(".reader-locks")
        .join(base.file_name().context("collection name missing")?);
    let path = directory.join(revision);
    for value in [parent.join(".reader-locks"), directory, path.clone()] {
        if let Ok(metadata) = fs::symlink_metadata(value) {
            ensure!(
                !metadata.file_type().is_symlink(),
                "linked generation lease"
            );
        }
    }
    Ok(path)
}

fn object_path(base: &Path, kind: &str, digest: &str) -> Result<PathBuf> {
    ensure!(valid_digest(digest), "invalid object identity");
    let path = base.join("objects").join(kind).join(digest);
    for value in [
        base.join("objects"),
        base.join("objects").join(kind),
        path.clone(),
    ] {
        if let Ok(metadata) = fs::symlink_metadata(value) {
            ensure!(
                !metadata.file_type().is_symlink(),
                "linked retained object state"
            );
        }
    }
    Ok(path)
}

pub(crate) fn read_verified(path: &Path, digest: &str, limit: u64) -> Result<Vec<u8>> {
    let file = File::open(path)
        .context("retained object missing; previous generation cannot be reconstructed")?;
    ensure!(
        file.metadata()?.len() <= limit,
        "retained object exceeds byte limit"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("read retained object {}", path.display()))?;
    ensure!(
        bytes.len() as u64 <= limit && format!("{:x}", Sha256::digest(&bytes)) == digest,
        "retained object digest mismatch"
    );
    Ok(bytes)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ComponentDigest {
    bytes: u64,
    sha256: airk_core::identity::Sha256Digest,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestV2 {
    schema: u32,
    engine: String,
    token: airk_core::identity::RevisionToken,
    collection: String,
    chunks: usize,
    vectors: usize,
    vector_occurrences: usize,
    vector_selection_digest: airk_core::identity::Sha256Digest,
    vector_eligible_artifacts: usize,
    vector_excluded_artifacts: usize,
    model_sha256: Option<String>,
    embedding: String,
    inventory_digest: airk_core::identity::Sha256Digest,
    components: std::collections::BTreeMap<String, ComponentDigest>,
}

impl ManifestV2 {
    fn supported_format(&self) -> bool {
        self.schema == 4
            && self.engine == crate::reset::CURRENT_RETRIEVAL_ENGINE
            && !self
                .components
                .keys()
                .any(|path| path.starts_with("model/"))
            && self.components.contains_key("model-objects.json") == self.model_sha256.is_some()
            && self.components.contains_key("vector-map/meta.json")
            && self.vectors <= self.vector_occurrences
            && (self.vectors == 0) == (self.vector_occurrences == 0)
            && (self.model_sha256.is_some() || self.vectors == 0)
            && self
                .vector_eligible_artifacts
                .checked_add(self.vector_excluded_artifacts)
                .is_some_and(|count| count <= 1_000_000)
            && (self.vectors == 0 || self.vector_eligible_artifacts > 0)
    }

    fn verify_selection(&self, inventory: &airk_core::publication::InventoryV2) -> Result<()> {
        verify_vector_selection(
            inventory,
            self.vector_selection_digest,
            self.vector_eligible_artifacts,
            self.vector_excluded_artifacts,
        )
    }
}

pub(crate) fn verify_vector_selection(
    inventory: &airk_core::publication::InventoryV2,
    digest: airk_core::identity::Sha256Digest,
    eligible_artifacts: usize,
    excluded_artifacts: usize,
) -> Result<()> {
    let selection = inventory.vector_selection.validated()?;
    let mut eligible = 0usize;
    for (path, file) in &inventory.files {
        let profile = inventory
            .profiles
            .get(&file.state.profile_id)
            .context("vector selection profile missing from inventory")?;
        eligible += usize::from(selection.decision(path, profile, &file.language)?.eligible);
    }
    ensure!(
        digest == inventory.vector_selection.digest()?
            && eligible_artifacts == eligible
            && excluded_artifacts == inventory.files.len() - eligible,
        "vector selection manifest differs from authoritative inventory"
    );
    Ok(())
}

fn parse_manifest(bytes: &[u8]) -> Result<ManifestV2> {
    ensure!(
        crate::reset::current_manifest(bytes)?,
        "obsolete index format; run airk index --full"
    );
    Ok(serde_json::from_slice(bytes)?)
}

fn model_set(
    directory: &Path,
    manifest: &ManifestV2,
    inventory: &airk_core::publication::InventoryV2,
) -> Result<Option<airk_core::model_objects::ModelObjectSet>> {
    let Some(component) = manifest.components.get("model-objects.json") else {
        return Ok(None);
    };
    ensure!(
        manifest.schema == 4
            && component.bytes <= airk_core::model_objects::MAX_MODEL_OBJECT_SET_BYTES as u64,
        "invalid model object sidecar format or bound"
    );
    let path = directory.join("model-objects.json");
    let metadata = path.symlink_metadata()?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() == component.bytes,
        "invalid model object sidecar file"
    );
    let set = airk_core::model_objects::ModelObjectSet::from_bytes(&read_verified(
        &path,
        &component.sha256.to_string(),
        component.bytes,
    )?)?;
    ensure!(
        Some(set.legacy_identity.as_str()) == manifest.model_sha256.as_deref()
            && inventory.embedding_model.as_deref() == Some(set.legacy_identity.as_str())
            && inventory.embedding_pipeline_digest == Some(set.pipeline_digest),
        "model objects disagree with publication identity or pipeline"
    );
    Ok(Some(set))
}

fn verify_model_objects(
    base: &Path,
    directory: &Path,
    manifest: &ManifestV2,
    inventory: &airk_core::publication::InventoryV2,
) -> Result<Vec<airk_core::model_objects::ModelObjectRef>> {
    let Some(set) = model_set(directory, manifest, inventory)? else {
        return Ok(vec![]);
    };
    let maximum = set.assets.values().try_fold(0u64, |total, asset| {
        total
            .checked_add(asset.bytes)
            .context("model verification length overflow")
    })?;
    crate::model_objects::ModelObjects::new(base).verify(&set, maximum)
}

/// Authenticate reusable pack ownership without reading the embedding payloads.
/// Lookup verifies the selected records; maintenance verifies complete packs.
fn embedding_manifest(
    base: &Path,
    directory: &Path,
    manifest: &ManifestV2,
    inventory: &airk_core::publication::InventoryV2,
) -> Result<Option<crate::vector_cache::VectorCacheManifest>> {
    let path = directory.join("embedding-cache.json");
    let Some(component) = manifest.components.get("embedding-cache.json") else {
        match path.symlink_metadata() {
            Ok(_) => anyhow::bail!("unanchored embedding sidecar"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        return Ok(None);
    };
    ensure!(
        component.bytes <= crate::vector_cache::MAX_CACHE_MANIFEST_BYTES as u64,
        "embedding sidecar exceeds byte bound"
    );
    let metadata = path.symlink_metadata()?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() == component.bytes,
        "embedding sidecar type or length mismatch"
    );
    let bytes = read_verified(&path, &component.sha256.to_string(), component.bytes)?;
    ensure!(
        bytes.len() as u64 == component.bytes,
        "embedding sidecar length changed"
    );
    let cache = crate::vector_cache::VectorCacheManifest::from_bytes(&bytes)?;
    let model = model_set(directory, manifest, inventory)?
        .context("embedding sidecar requires authenticated model objects")?;
    ensure!(
        inventory.embedding_pipeline_digest == Some(cache.pipeline_digest)
            && model.pipeline_digest == cache.pipeline_digest,
        "embedding pack pipeline differs from publication"
    );
    let asset = model
        .assets
        .get(&airk_core::model_objects::ModelAssetName::ModelJson)
        .context("model metadata asset missing")?;
    let model_path = object_path(base, "models", &asset.sha256.to_string())?;
    let metadata = model_path.symlink_metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() == asset.bytes,
        "model metadata length mismatch"
    );
    let metadata: serde_json::Value = serde_json::from_slice(&read_verified(
        &model_path,
        &asset.sha256.to_string(),
        asset.bytes,
    )?)?;
    let schema = if model
        .assets
        .contains_key(&airk_core::model_objects::ModelAssetName::VocabularyJson)
    {
        1
    } else {
        2
    };
    ensure!(
        metadata.get("schema").and_then(serde_json::Value::as_u64) == Some(schema)
            && metadata
                .get("dimensions")
                .and_then(serde_json::Value::as_u64)
                == Some(u64::from(cache.dimensions)),
        "embedding pack schema or dimensions differ from model metadata"
    );
    Ok(Some(cache))
}

/// Verify against a committed receipt (or a newly prepared receipt before commit),
/// never against an untrusted manifest's self-asserted identity alone.
pub(crate) fn verify_v2_generation(
    base: &Path,
    receipt: &airk_core::identity::PublicationReceipt,
) -> Result<airk_core::publication::InventoryV2> {
    let directory = base
        .join("generations")
        .join(receipt.token.revision.to_string());
    verify_v2_directory(base, &directory, receipt)
}

impl FileStore {
    /// Receipt-authenticated full-generation model dependencies. Historical
    /// representations deliberately do not retain these retrieval assets.
    pub fn model_object_references(
        &self,
        receipt: &airk_core::identity::PublicationReceipt,
    ) -> Result<Vec<airk_core::model_objects::ModelObjectRef>> {
        self.model_object_references_with_budget(receipt, 8 * 1024 * 1024 * 1024)
    }

    pub(crate) fn model_object_references_with_budget(
        &self,
        receipt: &airk_core::identity::PublicationReceipt,
        maximum: u64,
    ) -> Result<Vec<airk_core::model_objects::ModelObjectRef>> {
        let Some(set) = self.authenticated_model_set(receipt)? else {
            return Ok(vec![]);
        };
        crate::model_objects::ModelObjects::new(self.directory()).verify(&set, maximum)
    }

    pub fn model_object_set(
        &self,
        receipt: &airk_core::identity::PublicationReceipt,
    ) -> Result<Option<airk_core::model_objects::ModelObjectSet>> {
        let set = self.authenticated_model_set(receipt)?;
        if let Some(set) = &set {
            crate::model_objects::ModelObjects::new(self.directory())
                .verify(set, 8 * 1024 * 1024 * 1024)?;
        }
        Ok(set)
    }

    fn authenticated_model_set(
        &self,
        receipt: &airk_core::identity::PublicationReceipt,
    ) -> Result<Option<airk_core::model_objects::ModelObjectSet>> {
        ensure!(
            self.receipt(&receipt.token)?.as_ref() == Some(receipt),
            "model references require committed receipt"
        );
        let directory = self
            .directory()
            .join("generations")
            .join(receipt.token.revision.to_string());
        for path in [
            self.directory().join("generations"),
            directory.clone(),
            directory.join("manifest.json"),
            directory.join("inventory.json"),
        ] {
            ensure!(
                !path.symlink_metadata()?.file_type().is_symlink(),
                "linked model publication metadata"
            );
        }
        let manifest = parse_manifest(&read_verified(
            &directory.join("manifest.json"),
            &receipt.manifest_digest.to_string(),
            131072,
        )?)?;
        ensure!(
            manifest.supported_format()
                && manifest.token == receipt.token
                && manifest.inventory_digest == receipt.inventory_digest
                && manifest.components.len() <= 1024,
            "model manifest receipt mismatch"
        );
        let bytes = read_verified(
            &directory.join("inventory.json"),
            &receipt.inventory_digest.to_string(),
            MAX_CATALOG_BYTES,
        )?;
        ensure!(
            airk_core::identity::revision_digest(manifest.engine.as_bytes(), &bytes)
                == receipt.token.revision,
            "model publication revision mismatch"
        );
        let inventory: airk_core::publication::InventoryV2 = serde_json::from_slice(&bytes)?;
        inventory.validate()?;
        manifest.verify_selection(&inventory)?;
        ensure!(
            inventory.incarnation == receipt.token.incarnation
                && inventory.generation == receipt.token.generation
                && inventory.retrieval_specification == manifest.engine
                && inventory.collection == manifest.collection
                && inventory.embedding_model == manifest.model_sha256
                && self.directory().file_name().and_then(|name| name.to_str())
                    == Some(inventory.collection.as_str()),
            "model publication scope mismatch"
        );
        let Some(set) = model_set(&directory, &manifest, &inventory)? else {
            ensure!(
                !directory.join("model-objects.json").exists(),
                "unanchored model object sidecar"
            );
            return Ok(None);
        };
        Ok(Some(set))
    }

    /// Authenticate reusable embedding-pack ownership against a committed receipt.
    /// The caller keeps its catalog reader lease alive while using the result.
    pub fn embedding_cache_manifest(
        &self,
        receipt: &airk_core::identity::PublicationReceipt,
    ) -> Result<Option<crate::vector_cache::VectorCacheManifest>> {
        ensure!(
            self.receipt(&receipt.token)?.as_ref() == Some(receipt),
            "embedding references require this collection's committed receipt"
        );
        let directory = self
            .directory()
            .join("generations")
            .join(receipt.token.revision.to_string());
        let manifest_path = directory.join("manifest.json");
        for path in [
            self.directory().join("generations"),
            directory.clone(),
            manifest_path.clone(),
            directory.join("inventory.json"),
        ] {
            ensure!(
                !fs::symlink_metadata(path)?.file_type().is_symlink(),
                "linked embedding manifest"
            );
        }
        let manifest = parse_manifest(&read_verified(
            &manifest_path,
            &receipt.manifest_digest.to_string(),
            131_072,
        )?)?;
        ensure!(
            manifest.supported_format()
                && manifest.token == receipt.token
                && manifest.inventory_digest == receipt.inventory_digest
                && manifest.components.len() <= 1024
                && self.directory().file_name().and_then(|name| name.to_str())
                    == Some(manifest.collection.as_str()),
            "embedding manifest receipt or collection mismatch"
        );
        let bytes = read_verified(
            &directory.join("inventory.json"),
            &receipt.inventory_digest.to_string(),
            MAX_CATALOG_BYTES,
        )?;
        ensure!(
            airk_core::identity::revision_digest(manifest.engine.as_bytes(), &bytes)
                == receipt.token.revision,
            "embedding publication revision mismatch"
        );
        let inventory: airk_core::publication::InventoryV2 = serde_json::from_slice(&bytes)?;
        inventory.validate()?;
        manifest.verify_selection(&inventory)?;
        ensure!(
            inventory.incarnation == receipt.token.incarnation
                && inventory.generation == receipt.token.generation
                && inventory.collection == manifest.collection
                && inventory.retrieval_specification == manifest.engine
                && inventory.embedding_model == manifest.model_sha256,
            "embedding publication scope mismatch"
        );
        embedding_manifest(self.directory(), &directory, &manifest, &inventory)
    }
}

fn verify_components(
    base: &Path,
    directory: &Path,
    receipt: &airk_core::identity::PublicationReceipt,
) -> Result<ManifestV2> {
    Ok(verify_selected_components(base, directory, receipt, false)?.0)
}

/// A usage baseline reads authenticated catalog rows only. It must not become
/// an integrity gate for the independent retrieval operation, whose verifier
/// continues to hash every generation component and referenced model asset.
pub(crate) fn verify_usage_catalog(
    base: &Path,
    receipt: &airk_core::identity::PublicationReceipt,
    metrics: &mut crate::UsageReadMetrics,
) -> Result<()> {
    let started = std::time::Instant::now();
    let directory = base
        .join("generations")
        .join(receipt.token.revision.to_string());
    let (_, bytes) = verify_selected_components(base, &directory, receipt, true)?;
    metrics.verification_bytes = bytes;
    metrics.verification_us = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
    Ok(())
}

fn verify_selected_components(
    base: &Path,
    directory: &Path,
    receipt: &airk_core::identity::PublicationReceipt,
    catalog_only: bool,
) -> Result<(ManifestV2, u64)> {
    let directory = directory.to_path_buf();
    for path in [base.join("generations"), directory.clone()] {
        ensure!(
            !fs::symlink_metadata(path)?.file_type().is_symlink(),
            "linked v2 generation"
        );
    }
    ensure!(
        !fs::symlink_metadata(directory.join("manifest.json"))?
            .file_type()
            .is_symlink(),
        "linked generation manifest"
    );
    let bytes = read_verified(
        &directory.join("manifest.json"),
        &receipt.manifest_digest.to_string(),
        131_072,
    )?;
    let manifest = parse_manifest(&bytes)?;
    let mut verified_bytes = bytes.len() as u64;
    ensure!(
        manifest.supported_format()
            && manifest.token == receipt.token
            && manifest.inventory_digest == receipt.inventory_digest
            && manifest.components.len() <= 1024,
        "v2 manifest receipt mismatch"
    );
    ensure!(
        manifest.embedding.len() <= 4096
            && manifest
                .model_sha256
                .as_ref()
                .is_none_or(|value| valid_digest(value)),
        "invalid embedding manifest binding"
    );
    for required in ["inventory.json", "lineage.json", "catalog.sqlite"] {
        ensure!(
            manifest.components.contains_key(required),
            "required v2 component missing"
        );
    }
    ensure!(
        manifest.components["inventory.json"].sha256 == receipt.inventory_digest,
        "inventory checksum binding mismatch"
    );
    // Unlisted SQLite WAL/SHM or rollback files must not overlay verified main bytes.
    let mut actual = std::collections::BTreeSet::new();
    let mut pending = vec![directory.clone()];
    let mut visited = 0usize;
    while let Some(parent) = pending.pop() {
        for entry in fs::read_dir(parent)? {
            visited += 1;
            ensure!(visited <= 4096, "generation entry bound exceeded");
            let entry = entry?;
            let metadata = entry.file_type()?;
            ensure!(!metadata.is_symlink(), "linked v2 component entry");
            let path = entry.path();
            let relative = path
                .strip_prefix(&directory)?
                .to_str()
                .context("non-UTF-8 component path")?
                .replace('\\', "/");
            ensure!(relative.len() <= 4096, "generation path exceeds bound");
            if metadata.is_dir() {
                let prefix = format!("{relative}/");
                ensure!(
                    manifest
                        .components
                        .keys()
                        .any(|name| name.starts_with(&prefix)),
                    "unlisted generation directory"
                );
                pending.push(path);
            } else {
                ensure!(metadata.is_file(), "nonregular generation entry");
                if relative != "manifest.json" {
                    actual.insert(relative);
                }
            }
        }
    }
    ensure!(
        actual.len() == manifest.components.len()
            && actual
                .iter()
                .all(|name| manifest.components.contains_key(name)),
        "generation component set differs from anchored manifest"
    );
    for (relative, component) in &manifest.components {
        ensure!(
            !relative.is_empty()
                && relative.len() <= 4096
                && !relative.contains(['\\', ':', '\0'])
                && relative
                    .split('/')
                    .all(|part| !part.is_empty() && part != "." && part != "..")
                && relative != "manifest.json",
            "unsafe manifest component path"
        );
        let mut path = directory.clone();
        for part in relative.split('/') {
            path.push(part);
            ensure!(
                !fs::symlink_metadata(&path)?.file_type().is_symlink(),
                "linked generation component"
            );
        }
        let mut file = File::open(&path)
            .with_context(|| format!("open manifest component {}", path.display()))?;
        ensure!(
            file.metadata()?.is_file() && file.metadata()?.len() == component.bytes,
            "component length mismatch"
        );
        if catalog_only && relative != "catalog.sqlite" {
            continue;
        }
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65_536];
        let mut total = 0u64;
        // An empty component's metadata establishes its empty digest. Do not
        // issue an EOF read on Tantivy coordination files: Windows rejects that
        // read while another reader holds the file's byte-range lock.
        loop {
            if component.bytes == 0 {
                break;
            }
            let count = file
                .read(&mut buffer)
                .with_context(|| format!("hash manifest component {}", path.display()))?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count as u64)
                .context("component length overflow")?;
            verified_bytes = verified_bytes.saturating_add(count as u64);
            ensure!(
                total <= component.bytes,
                "component grew during verification"
            );
            hash.update(&buffer[..count]);
        }
        ensure!(
            total == component.bytes
                && file.metadata()?.len() == component.bytes
                && airk_core::identity::Sha256Digest::from_bytes(hash.finalize().into())
                    == component.sha256,
            "component checksum mismatch"
        );
    }
    Ok((manifest, verified_bytes))
}

/// Reuse semantic validation only after re-authenticating every component byte.
/// No timestamp or filesystem identity substitutes for the anchored digests.
/// The caller holds the generation lease for verification and subsequent reads.
pub(crate) fn verify_v2_for_query(
    base: &Path,
    receipt: &airk_core::identity::PublicationReceipt,
) -> Result<airk_core::extraction::IndexDetail> {
    let directory = base
        .join("generations")
        .join(receipt.token.revision.to_string());
    let key = directory.canonicalize()?;
    if let Some(detail) = crate::verified_cache::get(&key, receipt) {
        verify_components(base, &directory, receipt)?;
        return Ok(detail);
    }
    Ok(verify_v2_directory(base, &directory, receipt)?
        .index_detail
        .unwrap_or_default())
}

pub(crate) fn verify_v2_directory(
    base: &Path,
    directory: &Path,
    receipt: &airk_core::identity::PublicationReceipt,
) -> Result<airk_core::publication::InventoryV2> {
    let manifest = verify_components(base, directory, receipt)?;
    let bytes = read_verified(
        &directory.join("inventory.json"),
        &receipt.inventory_digest.to_string(),
        MAX_CATALOG_BYTES,
    )?;
    ensure!(
        airk_core::identity::revision_digest(manifest.engine.as_bytes(), &bytes)
            == receipt.token.revision,
        "v2 revision byte digest mismatch"
    );
    let inventory: airk_core::publication::InventoryV2 = serde_json::from_slice(&bytes)?;
    inventory.validate()?;
    manifest.verify_selection(&inventory)?;
    ensure!(
        inventory.incarnation == receipt.token.incarnation
            && inventory.generation == receipt.token.generation
            && inventory.collection == manifest.collection
            && inventory.retrieval_specification == manifest.engine
            && inventory.embedding_model == manifest.model_sha256
            && inventory
                .files
                .values()
                .try_fold(0usize, |sum, file| sum.checked_add(file.chunks))
                == Some(manifest.chunks)
            && manifest.vectors <= manifest.vector_occurrences
            && inventory.lineage_digest == manifest.components["lineage.json"].sha256,
        "v2 inventory manifest binding mismatch"
    );
    verify_model_objects(base, directory, &manifest, &inventory)?;
    embedding_manifest(base, directory, &manifest, &inventory)?;
    crate::sqlite::verify_prepared_catalog(directory, receipt)?;
    if manifest.model_sha256.is_none() {
        crate::verified_cache::remember(
            directory.canonicalize()?,
            receipt.clone(),
            inventory.index_detail.unwrap_or_default(),
        );
    }
    Ok(inventory)
}

fn put(
    base: &Path,
    kind: &str,
    bytes: &[u8],
    digest: &str,
    reservation: Option<&crate::quota::Reservation>,
) -> Result<()> {
    ensure!(
        format!("{:x}", Sha256::digest(bytes)) == digest,
        "source digest changed before retention"
    );
    let path = object_path(base, kind, digest)?;
    if path.exists() {
        read_verified(&path, digest, bytes.len() as u64)?;
        return Ok(());
    }
    if let Some(reservation) = reservation {
        reservation.charge(bytes.len() as u64)?;
    }
    let parent = path.parent().context("object parent missing")?;
    fs::create_dir_all(parent)?;
    // tempfile uses native MoveFileExW for persistence. Unlike std::fs, it does
    // not add the extended Windows path prefix when a digest crosses MAX_PATH.
    // Canonicalize the existing parent for both sides of the atomic rename.
    let parent = parent.canonicalize()?;
    let destination = parent.join(digest);
    let mut file = tempfile::Builder::new()
        .prefix("building-")
        .tempfile_in(&parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(destination)
        .context("publish retained object")?;
    Ok(())
}

impl FileStore {
    /// Store one extraction and its original bytes, returning compact inventory metadata.
    pub fn retain_artifact(&self, artifact: &Artifact, source: Option<&[u8]>) -> Result<Artifact> {
        self.retain_artifact_with_budget(artifact, source, None)
    }

    pub fn retain_artifact_with_budget(
        &self,
        artifact: &Artifact,
        source: Option<&[u8]>,
        reservation: Option<&crate::quota::Reservation>,
    ) -> Result<Artifact> {
        if let Some(source) = source {
            ensure!(
                source.len() == artifact.bytes,
                "source byte count changed before retention"
            );
            put(
                self.directory(),
                "sources",
                source,
                &artifact.content_hash,
                reservation,
            )?;
        }
        if let Some(reference) = &artifact.retained {
            let mut compact = artifact.clone();
            let mut reference = reference.clone();
            reference.raw_source |= source.is_some();
            compact.retained = Some(reference);
            return Ok(compact);
        }
        let bytes = serde_json::to_vec(artifact)?;
        ensure!(
            bytes.len() as u64 <= MAX_ARTIFACT_BYTES,
            "extracted artifact exceeds 64 MiB; previous generation retained"
        );
        let object = format!("{:x}", Sha256::digest(&bytes));
        put(self.directory(), "artifacts", &bytes, &object, reservation)?;
        let mut compact = artifact.clone();
        compact.retained = Some(ArtifactRef {
            profile: None,
            object,
            object_bytes: bytes.len(),
            raw_source: source.is_some(),
            chunks: artifact.chunks.len(),
            extracted_bytes: artifact.extracted_bytes(),
            facts: artifact.facts.len(),
        });
        compact.chunks.clear();
        compact.facts.clear();
        Ok(compact)
    }

    pub fn retain_payload_with_budget(
        &self,
        artifact: &Artifact,
        payload: &airk_core::extraction::ExtractionPayload,
        profile: &airk_core::extraction::ExtractionProfile,
        source: Option<&[u8]>,
        reservation: Option<&crate::quota::Reservation>,
    ) -> Result<Artifact> {
        payload.validate()?;
        profile.identity()?;
        let bound = airk_core::binding::bind_payload(
            payload,
            profile,
            airk_core::binding::ArtifactBinding {
                path: &artifact.path,
                source_hash: artifact.content_hash.parse()?,
                source_bytes: artifact.bytes,
            },
        )?;
        let mut expected = artifact.clone();
        expected.retained = None;
        ensure!(
            serde_json::to_vec(&bound)? == serde_json::to_vec(&expected)?,
            "path-free conversion changed artifact evidence"
        );
        if let Some(source) = source {
            ensure!(
                source.len() == artifact.bytes,
                "source byte count changed before retention"
            );
            put(
                self.directory(),
                "sources",
                source,
                &artifact.content_hash,
                reservation,
            )?;
        }
        let bytes = serde_json::to_vec(payload)?;
        ensure!(
            bytes.len() as u64 <= MAX_ARTIFACT_BYTES,
            "path-free payload exceeds byte limit"
        );
        let object = format!("{:x}", Sha256::digest(&bytes));
        put(self.directory(), "artifacts", &bytes, &object, reservation)?;
        let mut compact = artifact.clone();
        compact.retained = Some(ArtifactRef {
            object,
            object_bytes: bytes.len(),
            raw_source: source.is_some()
                || artifact
                    .retained
                    .as_ref()
                    .is_some_and(|value| value.raw_source),
            chunks: artifact.chunks.len(),
            extracted_bytes: artifact.extracted_bytes(),
            facts: artifact.facts.len(),
            profile: Some(profile.clone()),
        });
        compact.chunks.clear();
        compact.facts.clear();
        Ok(compact)
    }

    pub fn load_retained(&self, entry: &Artifact) -> Result<Artifact> {
        self.load_retained_bounded(entry, MAX_ARTIFACT_BYTES)
    }
    /// One authenticated decode supplies both the evidence projection and the
    /// extraction coordinates needed by semantic-unit planning.
    pub fn load_retrieval_artifact(
        &self,
        entry: &Artifact,
    ) -> Result<airk_core::RetrievalArtifact> {
        let reference = entry
            .retained
            .as_ref()
            .context("semantic extraction metadata missing; run airk index --full")?;
        let profile = reference
            .profile
            .as_ref()
            .context("obsolete extraction format; run airk index --full")?;
        ensure!(
            profile.chunk_version == 3,
            "obsolete semantic extraction format; run airk index --full"
        );
        let (artifact, payload) = self.decode_retained(entry, MAX_ARTIFACT_BYTES)?;
        Ok(airk_core::RetrievalArtifact {
            artifact,
            profile: profile.clone(),
            payload: payload
                .context("semantic extraction payload missing; run airk index --full")?,
            payload_digest: reference.object.parse()?,
        })
    }
    /// Read a verified path-free payload using only immutable retained objects.
    /// Legacy path-bound objects require explicit migration, never reinterpretation.
    pub fn load_extraction_payload(
        &self,
        entry: &Artifact,
    ) -> Result<airk_core::extraction::ExtractionPayload> {
        ensure!(
            entry
                .retained
                .as_ref()
                .is_some_and(|reference| reference.profile.is_some()),
            "path-free extraction payload unavailable; legacy evidence requires migration"
        );
        self.decode_retained(entry, MAX_ARTIFACT_BYTES)?
            .1
            .context("path-free payload missing")
    }
    pub fn load_retained_bounded(&self, entry: &Artifact, maximum: u64) -> Result<Artifact> {
        if entry.retained.is_none() {
            struct Counter {
                bytes: u64,
                maximum: u64,
            }
            impl Write for Counter {
                fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                    self.bytes = self.bytes.saturating_add(bytes.len() as u64);
                    if self.bytes > self.maximum {
                        return Err(std::io::Error::other("artifact exceeds request byte limit"));
                    }
                    Ok(bytes.len())
                }
                fn flush(&mut self) -> std::io::Result<()> {
                    Ok(())
                }
            }
            serde_json::to_writer(&mut Counter { bytes: 0, maximum }, entry)?;
            return Ok(entry.clone());
        }
        Ok(self.decode_retained(entry, maximum)?.0)
    }

    fn decode_retained(
        &self,
        entry: &Artifact,
        maximum: u64,
    ) -> Result<(Artifact, Option<airk_core::extraction::ExtractionPayload>)> {
        let reference = entry
            .retained
            .as_ref()
            .context("retained reference missing")?;
        let bytes = read_verified(
            &object_path(self.directory(), "artifacts", &reference.object)?,
            &reference.object,
            MAX_ARTIFACT_BYTES.min(maximum),
        )?;
        ensure!(
            bytes.len() == reference.object_bytes,
            "retained artifact length does not match pinned inventory"
        );
        let (artifact, payload): (Artifact, _) = if let Some(profile) = &reference.profile {
            let payload: airk_core::extraction::ExtractionPayload = serde_json::from_slice(&bytes)?;
            let artifact = airk_core::binding::bind_payload(
                &payload,
                profile,
                airk_core::binding::ArtifactBinding {
                    path: &entry.path,
                    source_hash: entry.content_hash.parse()?,
                    source_bytes: entry.bytes,
                },
            )?;
            (artifact, Some(payload))
        } else {
            (serde_json::from_slice(&bytes)?, None)
        };
        ensure!(
            artifact.retained.is_none()
                && artifact.path == entry.path
                && artifact.content_hash == entry.content_hash
                && artifact.bytes == entry.bytes
                && artifact.language == entry.language
                && artifact.extraction_method == entry.extraction_method
                && artifact.parse_status == entry.parse_status
                && artifact.chunks.len() == reference.chunks
                && artifact.extracted_bytes() == reference.extracted_bytes
                && artifact.facts.len() == reference.facts,
            "retained artifact does not match pinned inventory"
        );
        Ok((artifact, payload))
    }

    pub fn read_retained_source(&self, entry: &Artifact) -> Result<Vec<u8>> {
        self.read_retained_source_bounded(entry, airk_core::extraction::MAX_SOURCE_BYTES as u64)
    }

    pub fn read_retained_source_bounded(&self, entry: &Artifact, maximum: u64) -> Result<Vec<u8>> {
        let maximum = maximum.min(airk_core::extraction::MAX_SOURCE_BYTES as u64);
        ensure!(
            u64::try_from(entry.bytes)? <= maximum,
            "retained source exceeds read budget"
        );
        ensure!(
            entry.retained.as_ref().is_some_and(|r| r.raw_source),
            "original source bytes unavailable in legacy retained state; reindex from source"
        );
        let path = object_path(self.directory(), "sources", &entry.content_hash)?;
        ensure!(
            path.symlink_metadata()?.is_file(),
            "retained source is not a regular file"
        );
        let bytes = read_verified(&path, &entry.content_hash, maximum)?;
        ensure!(
            bytes.len() == entry.bytes,
            "retained source byte count mismatch"
        );
        Ok(bytes)
    }

    /// Pin one immutable catalog. Legacy state stays available for migration.
    pub fn catalog(&self) -> Result<Option<Catalog>> {
        self.catalog_bounded(MAX_CATALOG_BYTES)
    }
    pub fn catalog_bounded(&self, maximum: u64) -> Result<Option<Catalog>> {
        let lifecycle = self.lifecycle(false)?;
        self.ensure_index_ready()?;
        for _ in 0..3 {
            let Some(header) = self.persistent_revision()? else {
                self.check_legacy_catalog_size(maximum)?;
                return Ok(crate::SnapshotStore::load(self)?.map(|snapshot| Catalog {
                    artifact_ids: BTreeSet::new(),
                    continuity_keys: BTreeSet::new(),
                    snapshot,
                    inventory: None,
                    lineage: None,
                    receipt: None,
                    store: self.clone(),
                    _lease: None,
                    _lifecycle: lifecycle.clone(),
                }));
            };
            if header.storage_format == 0 {
                self.check_legacy_catalog_size(maximum)?;
                return Ok(crate::SnapshotStore::load(self)?.map(|snapshot| Catalog {
                    artifact_ids: BTreeSet::new(),
                    continuity_keys: BTreeSet::new(),
                    snapshot,
                    inventory: None,
                    lineage: None,
                    receipt: None,
                    store: self.clone(),
                    _lease: None,
                    _lifecycle: lifecycle.clone(),
                }));
            }
            let open = || -> Result<Catalog> {
                let lease = File::open(reader_lease_path(self.directory(), &header.revision)?)?;
                lease.lock_shared()?;
                if header.storage_format == 2 {
                    let token = header.token.context("v2 token missing")?;
                    let receipt = self.receipt(&token)?.context("v2 receipt missing")?;
                    let catalog =
                        self.open_catalog_v2(receipt, maximum, lease, lifecycle.clone())?;
                    ensure!(
                        catalog.snapshot.root == header.root
                            && catalog.snapshot.collection == header.collection
                            && catalog.snapshot.roots == header.roots
                            && catalog.snapshot.artifacts.len() == header.files,
                        "v2 pinned catalog identity mismatch"
                    );
                    return Ok(catalog);
                }
                ensure!(
                    header.storage_format == 1,
                    "unsupported retained inventory format"
                );
                let path = self
                    .directory()
                    .join("generations")
                    .join(&header.revision)
                    .join("inventory.json");
                let bytes = read_verified(
                    &path,
                    header
                        .inventory_sha256
                        .as_deref()
                        .context("catalog digest missing")?,
                    MAX_CATALOG_BYTES.min(maximum),
                )?;
                let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
                // Legacy revision selection is bound to its original encoded bytes.
                // Newly defaulted fields must never change which directory is read.
                let mut manifest_bytes = Vec::new();
                File::open(
                    self.directory()
                        .join("generations")
                        .join(&header.revision)
                        .join("manifest.json"),
                )?
                .take(16_385)
                .read_to_end(&mut manifest_bytes)?;
                ensure!(
                    manifest_bytes.len() <= 16_384,
                    "legacy manifest exceeds bound"
                );
                let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
                let engine = manifest["engine"]
                    .as_str()
                    .context("legacy engine missing")?;
                let mut hash = Sha256::new();
                hash.update(engine.as_bytes());
                hash.update(&bytes);
                ensure!(
                    format!("{:x}", hash.finalize()) == header.revision,
                    "legacy inventory revision digest mismatch"
                );
                ensure!(
                    snapshot.schema_version == airk_core::SCHEMA_VERSION
                        && snapshot.storage_format == 1
                        && snapshot.root == header.root
                        && snapshot.roots == header.roots
                        && snapshot.collection == header.collection
                        && snapshot.generation == header.generation
                        && snapshot.artifacts.len() == header.files
                        && snapshot.extractor_version == header.extractor_version
                        && snapshot
                            .artifacts
                            .values()
                            .try_fold(0usize, |sum, artifact| sum
                                .checked_add(artifact.chunk_count()))
                            == Some(header.chunks)
                        && snapshot
                            .artifacts
                            .values()
                            .try_fold(0usize, |sum, artifact| sum.checked_add(artifact.bytes))
                            == Some(header.source_bytes),
                    "catalog identity mismatch"
                );
                ensure!(
                    snapshot
                        .artifacts
                        .values()
                        .all(|a| a.retained.is_some() && a.chunks.is_empty() && a.facts.is_empty()),
                    "catalog contains inline source payloads"
                );
                Ok(Catalog {
                    artifact_ids: BTreeSet::new(),
                    continuity_keys: BTreeSet::new(),
                    snapshot,
                    inventory: None,
                    lineage: None,
                    receipt: None,
                    store: self.clone(),
                    _lease: Some(lease),
                    _lifecycle: lifecycle.clone(),
                })
            };
            match open() {
                Ok(catalog) => return Ok(Some(catalog)),
                Err(error) => {
                    if self
                        .persistent_revision()?
                        .is_none_or(|current| current.revision == header.revision)
                    {
                        return Err(error);
                    }
                }
            }
        }
        anyhow::bail!("catalog changed repeatedly while pinning revision")
    }

    /// Open exactly one retained v2 representation. No fallback to current data.
    pub fn catalog_at(
        &self,
        token: &airk_core::identity::RevisionToken,
        maximum: u64,
    ) -> Result<Option<Catalog>> {
        if !self.directory().join("index.sqlite3").exists() {
            return Ok(None);
        }
        let lifecycle = self.lifecycle(false)?;
        let Some(current) = self.persistent_revision()? else {
            return Ok(None);
        };
        let Some(current_token) = current.token else {
            return Ok(None);
        };
        token.validate_against(&current_token)?;
        crate::sqlite::validate_retained_token(self.directory(), token)?;
        if crate::sqlite::retirement_proof(self.directory(), token)?.is_some() {
            ensure!(token != &current_token, "active revision cannot be retired");
            return Ok(None);
        }
        let Some(receipt) = self.receipt(token)? else {
            ensure!(
                token != &current_token,
                "active publication receipt missing"
            );
            // A retired representation may still have a committed ancestry
            // header. Expiry must not turn a provable fork into an absent page.
            if let Some(current_catalog) = self.catalog_bounded(MAX_CATALOG_BYTES)?
                && let Some(lineage) = &current_catalog.lineage
            {
                for known in lineage
                    .ancestry
                    .iter()
                    .map(|header| header.token)
                    .chain(lineage.observation.parent)
                {
                    if known.generation == token.generation {
                        ensure!(known == *token, "forked retired revision token");
                    }
                }
            }
            match fs::symlink_metadata(
                self.directory()
                    .join("generations")
                    .join(token.revision.to_string()),
            ) {
                Ok(_) => anyhow::bail!("retained generation has no committed receipt"),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            return Ok(None);
        };
        let lease = match File::open(reader_lease_path(
            self.directory(),
            &token.revision.to_string(),
        )?) {
            Ok(lease) => lease,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                ensure!(
                    self.receipt(token)?.is_none(),
                    "retained revision lease missing for committed receipt"
                );
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };
        lease.lock_shared()?;
        let directory = self
            .directory()
            .join("generations")
            .join(token.revision.to_string());
        match fs::symlink_metadata(&directory) {
            Ok(metadata) => ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "invalid retained generation directory"
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if let Some(binding) = crate::sqlite::history_binding(self.directory(), token)? {
                    ensure!(binding.publication == receipt, "historical receipt changed");
                    let mut consumed = 0u64;
                    if crate::history::retired_package(self, &binding, &mut |bytes| {
                        consumed = consumed
                            .checked_add(bytes)
                            .context("historical verification overflow")?;
                        ensure!(
                            consumed <= maximum,
                            "historical verification exceeds byte limit"
                        );
                        Ok(())
                    })? {
                        return Ok(None);
                    }
                    crate::history::verify_package(
                        self,
                        &self
                            .directory()
                            .join("history")
                            .join(token.revision.to_string()),
                        &binding,
                        &mut |bytes| {
                            consumed = consumed
                                .checked_add(bytes)
                                .context("historical verification overflow")?;
                            ensure!(
                                consumed <= maximum,
                                "historical verification exceeds byte limit"
                            );
                            Ok(())
                        },
                    )?;
                    ensure!(binding.publication == receipt, "historical receipt changed");
                    return Ok(None);
                }
                ensure!(
                    self.receipt(token)?.is_none(),
                    "retained generation missing for committed receipt"
                );
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        }
        let catalog = self.open_catalog_v2(receipt, maximum, lease, lifecycle)?;
        ensure!(
            catalog.snapshot.collection == current.collection
                && catalog.snapshot.root == current.root,
            "foreign retained catalog binding"
        );
        Ok(Some(catalog))
    }

    fn open_catalog_v2(
        &self,
        receipt: airk_core::identity::PublicationReceipt,
        maximum: u64,
        lease: File,
        lifecycle: Option<std::sync::Arc<File>>,
    ) -> Result<Catalog> {
        let directory = self
            .directory()
            .join("generations")
            .join(receipt.token.revision.to_string());
        let maximum = maximum.min(MAX_CATALOG_BYTES);
        ensure!(
            fs::metadata(directory.join("inventory.json"))?.len() <= maximum,
            "catalog exceeds request byte limit"
        );
        let inventory = verify_v2_generation(self.directory(), &receipt)?;
        let lineage_bytes = read_verified(
            &directory.join("lineage.json"),
            &inventory.lineage_digest.to_string(),
            airk_core::changes::MAX_LINEAGE_BYTES as u64,
        )?;
        let lineage: airk_core::changes::LineageEnvelope = serde_json::from_slice(&lineage_bytes)?;
        lineage.validate()?;
        ensure!(
            lineage.incarnation == inventory.incarnation
                && lineage.observation.generation == inventory.generation,
            "v2 pinned lineage identity mismatch"
        );
        Ok(Catalog {
            artifact_ids: inventory
                .files
                .values()
                .map(|file| file.state.artifact_id)
                .collect(),
            continuity_keys: inventory
                .files
                .values()
                .flat_map(|file| file.continuity.iter().map(|record| record.key))
                .collect(),
            snapshot: inventory.projection()?,
            inventory: Some(inventory),
            lineage: Some(lineage),
            receipt: Some(receipt),
            store: self.clone(),
            _lease: Some(lease),
            _lifecycle: lifecycle,
        })
    }

    fn check_legacy_catalog_size(&self, maximum: u64) -> Result<()> {
        let size = if self.directory().join("index.sqlite3").exists() {
            crate::sqlite::legacy_payload_bytes(self.directory())?
        } else {
            match fs::metadata(self.directory().join("snapshot.json")) {
                Ok(metadata) => metadata.len(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
                Err(error) => return Err(error.into()),
            }
        };
        ensure!(size <= maximum, "legacy catalog exceeds request byte limit");
        Ok(())
    }
}

pub struct Catalog {
    // Built once from authenticated inventory; negative indexed lookups must
    // distinguish a genuinely absent identity from a deleted SQLite row.
    artifact_ids: BTreeSet<airk_core::identity::ArtifactId>,
    continuity_keys: BTreeSet<airk_core::continuity::ContinuityKey>,
    pub snapshot: Snapshot,
    pub inventory: Option<airk_core::publication::InventoryV2>,
    pub lineage: Option<airk_core::changes::LineageEnvelope>,
    pub receipt: Option<airk_core::identity::PublicationReceipt>,
    store: FileStore,
    _lease: Option<File>,
    _lifecycle: Option<std::sync::Arc<File>>,
}

impl Catalog {
    pub fn file_by_artifact(
        &self,
        id: airk_core::identity::ArtifactId,
    ) -> Result<Option<airk_core::publication::InventoryFile>> {
        let receipt = self
            .receipt
            .as_ref()
            .context("artifact lookup requires v2 catalog")?;
        let actual =
            crate::sqlite::file_by_artifact_in_catalog(self.store.directory(), receipt, id)?;
        ensure!(
            actual.is_some() == self.artifact_ids.contains(&id),
            "artifact index presence differs from pinned inventory"
        );
        let inventory = self
            .inventory
            .as_ref()
            .context("verified inventory missing")?;
        if let Some((path, file)) = &actual {
            ensure!(
                file.state.artifact_id == id && inventory.files.get(path) == Some(file),
                "artifact metadata differs from pinned receipt-bound inventory"
            );
        }
        Ok(actual.map(|(_, file)| file))
    }

    /// One indexed key lookup; verification examines only its bounded owning file.
    pub fn continuity_by_key(
        &self,
        key: airk_core::continuity::ContinuityKey,
    ) -> Result<Option<(String, airk_core::continuity::ContinuityRecord)>> {
        let Some(receipt) = &self.receipt else {
            return Ok(None);
        };
        let actual = crate::sqlite::continuity_in_catalog(self.store.directory(), receipt, key)?;
        ensure!(
            actual.is_some() == self.continuity_keys.contains(&key),
            "continuity index presence differs from pinned inventory"
        );
        if let Some((path, record)) = &actual {
            let file = self
                .inventory
                .as_ref()
                .context("verified inventory missing")?
                .files
                .get(path)
                .context("continuity owner file missing")?;
            ensure!(
                file.continuity
                    .iter()
                    .find(|candidate| candidate.key == key)
                    == Some(record),
                "continuity metadata differs from pinned receipt-bound inventory"
            );
        }
        Ok(actual)
    }

    pub fn publication_header(
        &self,
        generation: u64,
    ) -> Result<Option<&airk_core::changes::RetainedPublicationHeader>> {
        let lineage = self
            .lineage
            .as_ref()
            .context("ancestry requires v2 catalog")?;
        Ok(lineage
            .ancestry
            .binary_search_by_key(&generation, |header| header.token.generation)
            .ok()
            .map(|index| &lineage.ancestry[index]))
    }

    pub fn events_since(&self, generation: u64) -> Result<&[airk_core::changes::ChangeEvent]> {
        let lineage = self.lineage.as_ref().context("events require v2 catalog")?;
        ensure!(
            generation <= lineage.observation.generation,
            "future event baseline"
        );
        let start = lineage
            .events
            .partition_point(|event| event.generation <= generation);
        Ok(&lineage.events[start..])
    }

    pub fn resolvers_for_artifact(
        &self,
        id: airk_core::identity::ArtifactId,
    ) -> Result<Vec<&airk_core::changes::ResolverRecord>> {
        let lineage = self
            .lineage
            .as_ref()
            .context("resolution requires v2 catalog")?;
        Ok(lineage
            .resolvers
            .iter()
            .filter(|record| record.artifact_id == id)
            .collect())
    }

    pub fn resolvers_for_path(
        &self,
        root: &str,
        path: &str,
    ) -> Result<Vec<&airk_core::changes::ResolverRecord>> {
        ensure!(
            root.len() <= 4096 && path.len() <= 4096,
            "resolver endpoint exceeds bound"
        );
        let lineage = self
            .lineage
            .as_ref()
            .context("resolution requires v2 catalog")?;
        Ok(lineage
            .resolvers
            .iter()
            .filter(|record| record.from.root == root && record.from.path == path)
            .collect())
    }

    /// Indexed lookup in this already verified, leased catalog. No inventory parse.
    pub fn file(&self, path: &str) -> Result<Option<airk_core::publication::InventoryFile>> {
        let receipt = self
            .receipt
            .as_ref()
            .context("indexed metadata requires v2 catalog")?;
        let file = crate::sqlite::file_in_catalog(self.store.directory(), receipt, path)?;
        let expected = self
            .inventory
            .as_ref()
            .context("verified v2 inventory missing")?
            .files
            .get(path);
        ensure!(
            file.as_ref() == expected,
            "indexed metadata differs from pinned receipt-bound inventory"
        );
        Ok(file)
    }
    pub fn load_artifact_bounded(&self, path: &str, maximum: u64) -> Result<Artifact> {
        self.store.load_retained_bounded(
            self.snapshot
                .artifacts
                .get(path)
                .context("artifact is not in indexed inventory")?,
            maximum,
        )
    }
    pub fn materialize(&self) -> Result<Snapshot> {
        let mut snapshot = self.snapshot.clone();
        for (path, artifact) in &mut snapshot.artifacts {
            *artifact = self.load_artifact(path)?;
        }
        Ok(snapshot)
    }
}

impl ArtifactSource for Catalog {
    fn load_artifact(&self, path: &str) -> Result<Artifact> {
        self.store.load_retained(
            self.snapshot
                .artifacts
                .get(path)
                .context("artifact is not in indexed inventory")?,
        )
    }
}

impl airk_core::RetrievalSource for Catalog {
    fn load_retrieval_artifact(&self, path: &str) -> Result<airk_core::RetrievalArtifact> {
        self.store.load_retrieval_artifact(
            self.snapshot
                .artifacts
                .get(path)
                .context("artifact is not in indexed inventory")?,
        )
    }
}

pub struct RetainedSource<'a> {
    pub store: &'a FileStore,
    pub snapshot: &'a Snapshot,
}
impl ArtifactSource for RetainedSource<'_> {
    fn load_artifact(&self, path: &str) -> Result<Artifact> {
        self.store.load_retained(
            self.snapshot
                .artifacts
                .get(path)
                .context("artifact is not in indexed inventory")?,
        )
    }
}

impl airk_core::RetrievalSource for RetainedSource<'_> {
    fn load_retrieval_artifact(&self, path: &str) -> Result<airk_core::RetrievalArtifact> {
        self.store.load_retrieval_artifact(
            self.snapshot
                .artifacts
                .get(path)
                .context("artifact is not in indexed inventory")?,
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn current_manifest_requires_mapping_and_independent_vector_counts() -> anyhow::Result<()> {
        use serde_json::json;
        let digest = "a".repeat(64);
        let mut value = json!({
            "schema":4,"engine":crate::reset::CURRENT_RETRIEVAL_ENGINE,
            "token":format!("airk-r1:11111111111111111111111111111111:1:{digest}"),
            "collection":"fixture","chunks":1,"vectors":3,"vector_occurrences":4,
            "vector_selection_digest":digest,"vector_eligible_artifacts":1,"vector_excluded_artifacts":0,
            "model_sha256":digest,"embedding":"fixture","inventory_digest":digest,
            "components":{"vector-map/meta.json":{"bytes":2,"sha256":digest},"model-objects.json":{"bytes":2,"sha256":digest}}
        });
        let valid = |value: &serde_json::Value| -> anyhow::Result<bool> {
            Ok(super::parse_manifest(&serde_json::to_vec(value)?)?.supported_format())
        };
        assert!(
            valid(&value)?,
            "long semantic windows may outnumber lexical chunks"
        );
        value["vectors"] = json!(5);
        assert!(!valid(&value)?);
        value["vectors"] = json!(0);
        assert!(!valid(&value)?);
        value["vectors"] = json!(3);
        value["components"]
            .as_object_mut()
            .unwrap()
            .remove("vector-map/meta.json");
        assert!(!valid(&value)?);
        value["schema"] = json!(3);
        let error = super::parse_manifest(&serde_json::to_vec(&value)?)
            .err()
            .unwrap();
        assert!(error.to_string().contains("airk index --full"));
        Ok(())
    }
    use super::*;

    #[cfg(windows)]
    #[test]
    fn blob_publication_handles_windows_paths_beyond_legacy_max_path() -> Result<()> {
        use std::os::windows::ffi::OsStrExt;
        let directory = tempfile::tempdir()?;
        let collection = directory
            .path()
            .join("a".repeat(90))
            .join("b".repeat(90))
            .join("c".repeat(90));
        let bytes = b"original long-path evidence";
        let digest = format!("{:x}", Sha256::digest(bytes));
        let path = object_path(&collection, "sources", &digest)?;
        assert!(path.as_os_str().encode_wide().count() > 300);
        let store = FileStore::new(&collection);
        let writer = store.lock()?;
        assert!(
            store.lock().is_err(),
            "long paths retain native writer exclusion"
        );
        drop(writer);
        assert!(!collection.join("writer.lock").exists());
        put(&collection, "sources", bytes, &digest, None)?;
        assert_eq!(read_verified(&path, &digest, bytes.len() as u64)?, bytes);
        put(&collection, "sources", bytes, &digest, None)?;
        fs::write(&path, b"corrupt retained fixture")?;
        assert!(
            put(&collection, "sources", bytes, &digest, None).is_err(),
            "existing mismatched content must never be overwritten"
        );
        assert_eq!(fs::read(path)?, b"corrupt retained fixture");
        Ok(())
    }

    #[test]
    fn retained_object_must_match_declared_length_and_requested_budget() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let store = FileStore::new(directory.path());
        let raw = b"retained evidence";
        let artifact = Artifact {
            path: "evidence.txt".into(),
            content_hash: format!("{:x}", Sha256::digest(raw)),
            bytes: raw.len(),
            chunks: vec![airk_core::Chunk {
                start_line: 1,
                end_line: 1,
                text: String::from_utf8(raw.to_vec())?,
                location: airk_core::DocumentLocation::Lines { start: 1, end: 1 },
            }],
            language: "text".into(),
            extraction_method: "text".into(),
            parse_status: "parsed".into(),
            facts: Vec::new(),
            retained: None,
        };
        let mut retained = store.retain_artifact(&artifact, Some(raw))?;
        assert!(
            store.load_extraction_payload(&artifact).is_err(),
            "inline legacy evidence is not a path-free payload"
        );
        assert!(
            store.load_extraction_payload(&retained).is_err(),
            "legacy retained objects need explicit migration"
        );
        let bytes = retained
            .retained
            .as_ref()
            .context("retained reference")?
            .object_bytes;
        assert_eq!(
            store.load_retained_bounded(&retained, bytes as u64)?.chunks,
            artifact.chunks
        );
        assert!(
            store
                .load_retained_bounded(&retained, bytes as u64 - 1)
                .is_err()
        );
        retained
            .retained
            .as_mut()
            .context("retained reference")?
            .object_bytes -= 1;
        assert!(
            store.load_retained(&retained).is_err(),
            "a valid payload digest must not conceal inconsistent inventory length"
        );
        assert_eq!(store.read_retained_source(&retained)?, raw);
        Ok(())
    }

    #[test]
    fn extraction_payload_load_verifies_object_profile_binding_without_source_reads() -> Result<()>
    {
        use airk_core::extraction::{ExtractionPayload, ExtractionProfile, LocalChunk, LocalSpan};
        use std::collections::BTreeMap;
        let directory = tempfile::tempdir()?;
        let store = FileStore::new(directory.path());
        let text = "Original retained payload α.\n";
        let profile = ExtractionProfile {
            contract_version: 1,
            adapter_id: "text".into(),
            adapter_version: "original-fixture/1".into(),
            language: "text".into(),
            dialect: None,
            grammar_identity: None,
            mapping_version: 1,
            chunk_version: 3,
            options: BTreeMap::new(),
            configuration_inputs: BTreeMap::new(),
        };
        let payload = ExtractionPayload {
            semantic_projection: Default::default(),
            continuity_coverage: Default::default(),
            contract_version: 1,
            extraction_method: "utf8".into(),
            parse_status: "plain".into(),
            provenance: vec![],
            text: text.into(),
            facts: vec![],
            syntax_identities: vec![],
            chunks: vec![LocalChunk {
                span: LocalSpan {
                    start: 0,
                    end: text.len(),
                },
                start_line: 1,
                end_line: 1,
                location: airk_core::DocumentLocation::Lines { start: 1, end: 1 },
                region: None,
            }],
            regions: vec![],
            relationships: vec![],
            continuity: vec![],
            diagnostics: vec![],
            partial: false,
        };
        let artifact = airk_core::binding::bind_payload(
            &payload,
            &profile,
            airk_core::binding::ArtifactBinding {
                path: "original.txt",
                source_hash: airk_core::identity::Sha256Digest::from_bytes(
                    Sha256::digest(text.as_bytes()).into(),
                ),
                source_bytes: text.len(),
            },
        )?;
        let compact =
            store.retain_payload_with_budget(&artifact, &payload, &profile, None, None)?;
        assert!(!store.directory().join("objects/sources").exists());
        assert_eq!(
            serde_json::to_value(store.load_extraction_payload(&compact)?)?,
            serde_json::to_value(&payload)?
        );
        assert_eq!(store.load_retained(&compact)?.chunks, artifact.chunks);
        let retrieved = store.load_retrieval_artifact(&compact)?;
        assert_eq!(retrieved.artifact.chunks, artifact.chunks);
        assert_eq!(retrieved.profile, profile);
        assert_eq!(
            retrieved.payload_digest.to_string(),
            compact.retained.as_ref().unwrap().object
        );
        assert_eq!(
            serde_json::to_value(retrieved.payload)?,
            serde_json::to_value(&payload)?
        );
        let mut changed = compact.clone();
        changed.language = "foreign".into();
        assert!(store.load_extraction_payload(&changed).is_err());
        changed = compact.clone();
        changed.retained.as_mut().unwrap().object_bytes += 1;
        assert!(store.load_extraction_payload(&changed).is_err());
        assert!(store.load_retrieval_artifact(&changed).is_err());
        changed = compact.clone();
        changed
            .retained
            .as_mut()
            .unwrap()
            .profile
            .as_mut()
            .unwrap()
            .contract_version = u32::MAX;
        assert!(store.load_extraction_payload(&changed).is_err());
        let reference = compact.retained.as_ref().unwrap();
        assert!(
            store
                .load_retained_bounded(&compact, reference.object_bytes as u64 - 1)
                .is_err()
        );
        fs::write(
            object_path(store.directory(), "artifacts", &reference.object)?,
            b"forged payload",
        )?;
        assert!(store.load_extraction_payload(&compact).is_err());
        Ok(())
    }
}
