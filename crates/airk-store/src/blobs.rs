// SPDX-License-Identifier: Apache-2.0
//! Immutable content-addressed source and extraction objects. No source workspace writes.
use crate::FileStore;
use airk_core::{Artifact, ArtifactRef, ArtifactSource, Snapshot};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_ARTIFACT_BYTES: u64 = 67_108_864;
pub const MAX_CATALOG_BYTES: u64 = 134_217_728;

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
    model_sha256: Option<String>,
    embedding: String,
    inventory_digest: airk_core::identity::Sha256Digest,
    components: std::collections::BTreeMap<String, ComponentDigest>,
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

pub(crate) fn verify_v2_directory(
    base: &Path,
    directory: &Path,
    receipt: &airk_core::identity::PublicationReceipt,
) -> Result<airk_core::publication::InventoryV2> {
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
    let manifest: ManifestV2 = serde_json::from_slice(&bytes)?;
    ensure!(
        manifest.schema == 2
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
    ensure!(
        inventory.incarnation == receipt.token.incarnation
            && inventory.generation == receipt.token.generation
            && inventory.collection == manifest.collection
            && inventory.retrieval_specification == manifest.engine
            && inventory
                .files
                .values()
                .try_fold(0usize, |sum, file| sum.checked_add(file.chunks))
                == Some(manifest.chunks)
            && manifest.vectors <= manifest.chunks
            && inventory.lineage_digest == manifest.components["lineage.json"].sha256,
        "v2 inventory manifest binding mismatch"
    );
    crate::sqlite::verify_prepared_catalog(&directory, receipt)?;
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
    pub fn load_retained_bounded(&self, entry: &Artifact, maximum: u64) -> Result<Artifact> {
        let Some(reference) = &entry.retained else {
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
        };
        let bytes = read_verified(
            &object_path(self.directory(), "artifacts", &reference.object)?,
            &reference.object,
            MAX_ARTIFACT_BYTES.min(maximum),
        )?;
        ensure!(
            bytes.len() == reference.object_bytes,
            "retained artifact length does not match pinned inventory"
        );
        let artifact: Artifact = if let Some(profile) = &reference.profile {
            let payload: airk_core::extraction::ExtractionPayload = serde_json::from_slice(&bytes)?;
            airk_core::binding::bind_payload(
                &payload,
                profile,
                airk_core::binding::ArtifactBinding {
                    path: &entry.path,
                    source_hash: entry.content_hash.parse()?,
                    source_bytes: entry.bytes,
                },
            )?
        } else {
            serde_json::from_slice(&bytes)?
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
        Ok(artifact)
    }

    pub fn read_retained_source(&self, entry: &Artifact) -> Result<Vec<u8>> {
        ensure!(
            entry.retained.as_ref().is_some_and(|r| r.raw_source),
            "original source bytes unavailable in legacy retained state; reindex from source"
        );
        let bytes = read_verified(
            &object_path(self.directory(), "sources", &entry.content_hash)?,
            &entry.content_hash,
            16_777_216,
        )?;
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
        for _ in 0..3 {
            let Some(header) = self.persistent_revision()? else {
                self.check_legacy_catalog_size(maximum)?;
                return Ok(crate::SnapshotStore::load(self)?.map(|snapshot| Catalog {
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
        let expected = self
            .inventory
            .as_ref()
            .context("verified inventory missing")?
            .files
            .iter()
            .find(|(_, file)| file.state.artifact_id == id);
        ensure!(
            actual.as_ref().map(|(path, file)| (path, file)) == expected,
            "artifact metadata differs from pinned receipt-bound inventory"
        );
        Ok(actual.map(|(_, file)| file))
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

#[cfg(test)]
mod tests {
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
}
