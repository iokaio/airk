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

const MAX_ARTIFACT_BYTES: u64 = airk_core::extraction::MAX_PAYLOAD_BYTES as u64;
pub const MAX_CATALOG_BYTES: u64 = airk_core::publication::MAX_INVENTORY_BYTES as u64;

/// A missing derived source/extraction object. This typed context is attached
/// only to NotFound reads, never to corrupt, inaccessible or missing model state.
#[derive(Debug)]
pub struct MissingRetainedObject {
    kind: &'static str,
    path: PathBuf,
    logical_digest: String,
}

impl std::fmt::Display for MissingRetainedObject {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "missing retained {} object {} (content SHA-256 {})",
            self.kind,
            self.path.display(),
            self.logical_digest
        )
    }
}

impl std::error::Error for MissingRetainedObject {}

fn retained_read_context<T>(result: Result<T>, kind: &str, path: &Path, digest: &str) -> Result<T> {
    result.map_err(|error| {
        let missing = error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound);
        let repair_kind = match kind {
            "sources" => Some("source"),
            "artifacts" => Some("extraction"),
            _ => None,
        };
        if missing && let Some(kind) = repair_kind {
            error.context(MissingRetainedObject {
                kind,
                path: path.into(),
                logical_digest: digest.into(),
            })
        } else {
            error.context(format!(
                "read retained {kind} object {} (content SHA-256 {digest})",
                path.display()
            ))
        }
    })
}

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
    let file = File::open(path).with_context(|| {
        format!(
            "retained object missing; previous generation cannot be reconstructed: {}",
            path.display()
        )
    })?;
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
    let staging = reservation
        .map(crate::quota::Reservation::staging_directory)
        .transpose()?;
    let temporary_parent = staging.as_deref().unwrap_or(&parent).canonicalize()?;
    let mut file = tempfile::Builder::new()
        .prefix("building-")
        .tempfile_in(&temporary_parent)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(destination)
        .context("publish retained object")?;
    Ok(())
}

fn put_encoded(
    base: &Path,
    kind: &str,
    bytes: &[u8],
    digest: &str,
    reservation: Option<&crate::quota::Reservation>,
    mode: airk_core::compression::CompressionMode,
) -> Result<Option<airk_core::compression::EncodedObject>> {
    if let Some((encoded, reference)) = crate::compression::encode(bytes, mode)? {
        put(base, kind, &encoded, &reference.stored_digest, reservation)?;
        Ok(Some(reference))
    } else {
        put(base, kind, bytes, digest, reservation)?;
        Ok(None)
    }
}

fn read_object(
    base: &Path,
    kind: &str,
    digest: &str,
    encoding: Option<&airk_core::compression::EncodedObject>,
    maximum: u64,
) -> Result<Vec<u8>> {
    let stored_digest = encoding.map_or(digest, |value| value.stored_digest.as_str());
    let path = object_path(base, kind, stored_digest)?;
    let result = if let Some(encoding) = encoding {
        crate::compression::read_all(&path, encoding, digest, maximum)
    } else {
        read_verified(&path, digest, maximum)
    };
    retained_read_context(result, kind, &path, digest)
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
            source_encoding: None,
            payload_encoding: None,
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
        self.retain_payload_encoded(
            artifact,
            payload,
            profile,
            source,
            reservation,
            airk_core::compression::CompressionMode::Off,
        )
    }

    pub fn retain_payload_encoded(
        &self,
        artifact: &Artifact,
        payload: &airk_core::extraction::ExtractionPayload,
        profile: &airk_core::extraction::ExtractionProfile,
        source: Option<&[u8]>,
        reservation: Option<&crate::quota::Reservation>,
        mode: airk_core::compression::CompressionMode,
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
        let source_encoding = if let Some(source) = source {
            ensure!(
                source.len() == artifact.bytes,
                "source byte count changed before retention"
            );
            put_encoded(
                self.directory(),
                "sources",
                source,
                &artifact.content_hash,
                reservation,
                mode,
            )?
        } else {
            artifact
                .retained
                .as_ref()
                .and_then(|r| r.source_encoding.clone())
        };
        let bytes = serde_json::to_vec(payload)?;
        ensure!(
            bytes.len() as u64 <= MAX_ARTIFACT_BYTES,
            "path-free payload exceeds byte limit"
        );
        let object = format!("{:x}", Sha256::digest(&bytes));
        let payload_encoding = put_encoded(
            self.directory(),
            "artifacts",
            &bytes,
            &object,
            reservation,
            mode,
        )?;
        let mut compact = artifact.clone();
        compact.retained = Some(ArtifactRef {
            source_encoding,
            payload_encoding,
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
    /// Check that reusable objects are present without decoding them during a
    /// source scan. Reads that consume an object still authenticate its bytes.
    pub fn check_retained_present(&self, entry: &Artifact) -> Result<()> {
        let reference = entry
            .retained
            .as_ref()
            .context("retained reference missing")?;
        ensure!(reference.raw_source, "retained source unavailable");
        for (kind, logical, stored, expected) in [
            (
                "artifacts",
                reference.object.as_str(),
                reference
                    .payload_encoding
                    .as_ref()
                    .map_or(reference.object.as_str(), |e| e.stored_digest.as_str()),
                reference
                    .payload_encoding
                    .as_ref()
                    .map_or(reference.object_bytes as u64, |e| e.stored_bytes),
            ),
            (
                "sources",
                entry.content_hash.as_str(),
                reference
                    .source_encoding
                    .as_ref()
                    .map_or(entry.content_hash.as_str(), |e| e.stored_digest.as_str()),
                reference
                    .source_encoding
                    .as_ref()
                    .map_or(entry.bytes as u64, |e| e.stored_bytes),
            ),
        ] {
            let path = object_path(self.directory(), kind, stored)?;
            let metadata = retained_read_context(
                fs::metadata(&path).map_err(Into::into),
                kind,
                &path,
                logical,
            )?;
            ensure!(
                metadata.is_file() && metadata.len() == expected,
                "retained {kind} object length mismatch for indexed path {:?}",
                entry.path
            );
        }
        Ok(())
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
        let bytes = read_object(
            self.directory(),
            "artifacts",
            &reference.object,
            reference.payload_encoding.as_ref(),
            MAX_ARTIFACT_BYTES.min(maximum),
        )
        .with_context(|| format!("load retained extraction for indexed path {:?}", entry.path))?;
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

    /// Authenticate retained source identity without allocating/decoding a copy
    /// when evidence materialization uses the independently verified payload.
    pub fn verify_retained_source(&self, entry: &Artifact) -> Result<()> {
        ensure!(
            entry.bytes <= airk_core::extraction::MAX_SOURCE_BYTES
                && entry.retained.as_ref().is_some_and(|r| r.raw_source),
            "retained source unavailable or oversized"
        );
        if let Some(encoding) = entry
            .retained
            .as_ref()
            .and_then(|r| r.source_encoding.as_ref())
        {
            let path = object_path(self.directory(), "sources", &encoding.stored_digest)?;
            ensure!(
                retained_read_context(
                    crate::compression::verify(
                        &path,
                        encoding,
                        &entry.content_hash,
                        entry.bytes as u64,
                    ),
                    "sources",
                    &path,
                    &entry.content_hash,
                )
                .with_context(|| {
                    format!(
                        "verify retained source for indexed path {:?}: {} (content SHA-256 {})",
                        entry.path,
                        path.display(),
                        entry.content_hash
                    )
                })? == entry.bytes,
                "encoded source length mismatch"
            );
        } else {
            let path = object_path(self.directory(), "sources", &entry.content_hash)?;
            let mut file = retained_read_context(
                File::open(&path).map_err(Into::into),
                "sources",
                &path,
                &entry.content_hash,
            )
            .with_context(|| {
                format!(
                    "verify retained source for indexed path {:?}: {}",
                    entry.path,
                    path.display()
                )
            })?;
            ensure!(
                file.metadata()?.is_file() && file.metadata()?.len() == entry.bytes as u64,
                "retained source length mismatch"
            );
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 65536];
            let mut total = 0usize;
            loop {
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                total = total
                    .checked_add(count)
                    .context("source verification overflow")?;
                ensure!(
                    total <= entry.bytes,
                    "retained source grew during verification"
                );
                hash.update(&buffer[..count]);
            }
            ensure!(
                total == entry.bytes && format!("{:x}", hash.finalize()) == entry.content_hash,
                "retained source digest mismatch"
            );
        }
        Ok(())
    }

    pub fn read_retained_source_range(
        &self,
        entry: &Artifact,
        range: std::ops::Range<usize>,
        max_decoded_bytes: usize,
    ) -> Result<Vec<u8>> {
        ensure!(
            range.start <= range.end
                && range.end <= entry.bytes
                && entry.bytes <= airk_core::extraction::MAX_SOURCE_BYTES
                && entry.retained.as_ref().is_some_and(|r| r.raw_source),
            "source range outside admitted object"
        );
        if let Some(encoding) = entry
            .retained
            .as_ref()
            .and_then(|r| r.source_encoding.as_ref())
        {
            crate::compression::read_range(
                &object_path(self.directory(), "sources", &encoding.stored_digest)?,
                encoding,
                &entry.content_hash,
                entry.bytes as u64,
                range,
                max_decoded_bytes,
            )
        } else {
            ensure!(
                entry.bytes <= max_decoded_bytes,
                "raw source verification exceeds decoded-work allowance"
            );
            Ok(self.read_retained_source(entry)?[range].to_vec())
        }
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
        let bytes = read_object(
            self.directory(),
            "sources",
            &entry.content_hash,
            entry
                .retained
                .as_ref()
                .and_then(|r| r.source_encoding.as_ref()),
            maximum,
        )
        .with_context(|| format!("read retained source for indexed path {:?}", entry.path))?;
        ensure!(
            bytes.len() == entry.bytes,
            "retained source byte count mismatch"
        );
        Ok(bytes)
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
