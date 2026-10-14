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

fn read_verified(path: &Path, digest: &str, limit: u64) -> Result<Vec<u8>> {
    let file = File::open(path)
        .context("retained object missing; previous generation cannot be reconstructed")?;
    ensure!(
        file.metadata()?.len() <= limit,
        "retained object exceeds byte limit"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
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
        let artifact: Artifact = serde_json::from_slice(&bytes)?;
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
            1_048_576,
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
                    store: self.clone(),
                    _lease: None,
                    _lifecycle: lifecycle.clone(),
                }));
            };
            if header.storage_format == 0 {
                self.check_legacy_catalog_size(maximum)?;
                return Ok(crate::SnapshotStore::load(self)?.map(|snapshot| Catalog {
                    snapshot,
                    store: self.clone(),
                    _lease: None,
                    _lifecycle: lifecycle.clone(),
                }));
            }
            let open = || -> Result<Catalog> {
                let lease = File::open(reader_lease_path(self.directory(), &header.revision)?)?;
                lease.lock_shared()?;
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
    store: FileStore,
    _lease: Option<File>,
    _lifecycle: Option<std::sync::Arc<File>>,
}

impl Catalog {
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
