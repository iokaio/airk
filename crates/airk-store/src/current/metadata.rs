// SPDX-License-Identifier: Apache-2.0
//! Authenticated current file identity and selected continuity, outside SQL buckets.
use super::{
    CurrentRead, CurrentScope,
    map::{self, MapDomain},
};
use crate::{FileStore, quota::Reservation};
use airk_core::{
    identity::{Sha256Digest, framed_digest},
    publication::InventoryFile,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::PathBuf,
};

pub const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileMetadataRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<airk_core::compression::EncodedObject>,
    pub path: String,
    pub digest: Sha256Digest,
    pub bytes: usize,
}

fn object(store: &FileStore, digest: Sha256Digest) -> PathBuf {
    store
        .directory()
        .join("current-metadata")
        .join(format!("{digest}.json"))
}

impl FileStore {
    pub fn retain_current_metadata(
        &self,
        path: &str,
        file: &InventoryFile,
        reservation: &Reservation,
    ) -> Result<FileMetadataRef> {
        self.retain_current_metadata_encoded(
            path,
            file,
            reservation,
            airk_core::compression::CompressionMode::Off,
        )
    }
    pub fn retain_current_metadata_encoded(
        &self,
        path: &str,
        file: &InventoryFile,
        reservation: &Reservation,
        mode: airk_core::compression::CompressionMode,
    ) -> Result<FileMetadataRef> {
        ensure!(
            super::journal::valid_current_path(path),
            "invalid current metadata path"
        );
        let bytes = serde_json::to_vec(file)?;
        ensure!(
            bytes.len() <= MAX_METADATA_BYTES,
            "current metadata byte limit exceeded"
        );
        let digest = framed_digest(b"current-file-metadata/1", &[&bytes]);
        let encoded = crate::compression::encode(&bytes, mode)?;
        let reference = FileMetadataRef {
            encoding: encoded.as_ref().map(|(_, encoding)| encoding.clone()),
            path: path.into(),
            digest,
            bytes: bytes.len(),
        };
        let destination = object(
            self,
            reference
                .encoding
                .as_ref()
                .map(|e| e.stored_digest.parse())
                .transpose()?
                .unwrap_or(digest),
        );
        let directory = destination.parent().context("metadata parent missing")?;
        if !directory.try_exists()? {
            std::fs::create_dir(directory)?;
        }
        ensure!(
            std::fs::symlink_metadata(directory)?.is_dir()
                && !std::fs::symlink_metadata(directory)?
                    .file_type()
                    .is_symlink(),
            "linked current metadata directory"
        );
        if !destination.try_exists()? {
            reservation.charge(bytes.len() as u64)?;
            std::fs::create_dir_all(destination.parent().context("metadata parent missing")?)?;
            // A killed writer leaves only reservation-owned disposable state.
            let mut temporary = tempfile::NamedTempFile::new_in(reservation.staging_directory()?)?;
            temporary.write_all(
                encoded
                    .as_ref()
                    .map_or(bytes.as_slice(), |(encoded, _)| encoded.as_slice()),
            )?;
            crate::durability::persist_synced(temporary, &destination)?;
        }
        self.read_current_metadata(&reference)?;
        Ok(reference)
    }

    pub fn read_current_metadata(&self, reference: &FileMetadataRef) -> Result<InventoryFile> {
        ensure!(
            reference.bytes <= MAX_METADATA_BYTES
                && super::journal::valid_current_path(&reference.path),
            "invalid current metadata reference"
        );
        let path = object(
            self,
            reference
                .encoding
                .as_ref()
                .map(|e| e.stored_digest.parse())
                .transpose()?
                .unwrap_or(reference.digest),
        );
        let directory = path.parent().context("metadata directory missing")?;
        ensure!(
            std::fs::symlink_metadata(directory)?.is_dir()
                && !std::fs::symlink_metadata(directory)?
                    .file_type()
                    .is_symlink(),
            "linked current metadata directory"
        );
        let metadata = std::fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len()
                    == reference
                        .encoding
                        .as_ref()
                        .map_or(reference.bytes as u64, |e| e.stored_bytes),
            "current metadata size or ownership mismatch"
        );
        let mut bytes = Vec::new();
        if let Some(encoding) = &reference.encoding {
            bytes = crate::compression::read_all(
                &path,
                encoding,
                &encoding.logical_digest,
                MAX_METADATA_BYTES as u64,
            )?;
        } else {
            std::fs::File::open(path)?
                .take(reference.bytes as u64 + 1)
                .read_to_end(&mut bytes)?;
        }
        ensure!(
            bytes.len() == reference.bytes
                && framed_digest(b"current-file-metadata/1", &[&bytes]) == reference.digest,
            "current metadata digest mismatch"
        );
        let value: InventoryFile = serde_json::from_slice(&bytes)?;
        ensure!(
            serde_json::to_vec(&value)? == bytes,
            "noncanonical current metadata"
        );
        Ok(value)
    }
}

impl CurrentRead {
    pub fn file_metadata_bytes(&self, path: &str) -> Result<usize> {
        let Some(bytes) = self.map_get(MapDomain::FileMetadata, path.as_bytes())? else {
            return Ok(0);
        };
        let reference: FileMetadataRef = serde_json::from_slice(&bytes)?;
        ensure!(
            reference.path == path
                && reference.bytes <= MAX_METADATA_BYTES
                && serde_json::to_vec(&reference)? == bytes,
            "invalid current metadata reference"
        );
        Ok(reference.bytes)
    }
    pub fn artifact_path(
        &self,
        store: &FileStore,
        id: airk_core::identity::ArtifactId,
    ) -> Result<Option<String>> {
        let Some(bytes) = self.map_get(MapDomain::ArtifactHandle, id.as_bytes())? else {
            return Ok(None);
        };
        let path = String::from_utf8(bytes)?;
        let file = self
            .file_metadata(store, &path)?
            .context("artifact handle owner missing")?;
        ensure!(
            file.state.artifact_id == id,
            "artifact handle binding mismatch"
        );
        Ok(Some(path))
    }

    pub fn unit_owner(
        &self,
        store: &FileStore,
        key: airk_core::continuity::ContinuityKey,
    ) -> Result<Option<(String, InventoryFile)>> {
        let Some(bytes) = self.map_get(MapDomain::UnitHandle, &serde_json::to_vec(&key)?)? else {
            return Ok(None);
        };
        let path = String::from_utf8(bytes)?;
        let file = self
            .file_metadata(store, &path)?
            .context("unit handle owner missing")?;
        ensure!(
            file.continuity.iter().filter(|r| r.key == key).count() == 1,
            "unit handle binding mismatch"
        );
        Ok(Some((path, file)))
    }

    pub fn file_metadata(&self, store: &FileStore, path: &str) -> Result<Option<InventoryFile>> {
        let Some(bytes) = self.map_get(MapDomain::FileMetadata, path.as_bytes())? else {
            return Ok(None);
        };
        let reference: FileMetadataRef = serde_json::from_slice(&bytes)?;
        ensure!(
            reference.path == path && serde_json::to_vec(&reference)? == bytes,
            "current metadata path mismatch"
        );
        let file = store.read_current_metadata(&reference)?;
        let artifact = self
            .artifact(path)?
            .context("current metadata artifact missing")?;
        verify(&artifact, &file, self.head().sequence)?;
        Ok(Some(file))
    }
}

pub(crate) fn verify(
    artifact: &airk_core::Artifact,
    file: &InventoryFile,
    sequence: u64,
) -> Result<()> {
    file.state.validate(sequence)?;
    let retained = artifact
        .retained
        .as_ref()
        .context("current metadata extraction missing")?;
    ensure!(
        file.state.source_hash.to_string() == artifact.content_hash
            && file.state.payload_digest.to_string() == retained.object
            && file.state.profile_id
                == retained
                    .profile
                    .as_ref()
                    .context("current metadata profile missing")?
                    .identity()?
            && file.source_bytes == artifact.bytes
            && file.chunks == retained.chunks
            && file.facts == retained.facts,
        "current metadata differs from captured artifact"
    );
    Ok(())
}

pub(crate) fn prepare(
    store: &FileStore,
    scope: &CurrentScope,
    plan: &mut map::MapPlan<'_>,
    artifacts: &[super::ArtifactChange],
    references: &[FileMetadataRef],
    sequence: u64,
) -> Result<Vec<map::MapChange>> {
    let mut changes = Vec::new();
    let mut refs = references.iter().peekable();
    let mut metadata_bytes = 0usize;
    for change in artifacts {
        let path = change.path();
        let key = map::key(scope, MapDomain::FileMetadata, path.as_bytes())?;
        if let Some(old) = plan.get(key)? {
            let old: FileMetadataRef = serde_json::from_slice(&old)?;
            ensure!(old.path == path, "prior current metadata path mismatch");
            metadata_bytes = metadata_bytes
                .checked_add(old.bytes)
                .context("metadata admission overflow")?;
            ensure!(
                metadata_bytes <= super::MAX_CURRENT_METADATA_READ_BYTES,
                "current metadata admission exceeded"
            );
            let file = store.read_current_metadata(&old)?;
            for (domain, identity) in handles(&file)? {
                let c = map::MapChange::new(scope, domain, &identity, None)?;
                plan.put(c.key(), c.value())?;
                changes.push(c);
            }
        }
    }
    for change in artifacts {
        let path = change.path();
        let measurement = match change {
            super::ArtifactChange::Upsert(input) => Some(serde_json::to_vec(
                &airk_core::usage::SourceContextSize::measure(
                    &store.load_retained(&input.artifact)?,
                )?,
            )?),
            super::ArtifactChange::Delete(_) => None,
        };
        let measured = map::MapChange::new(
            scope,
            MapDomain::SourceMeasurement,
            path.as_bytes(),
            measurement,
        )?;
        plan.put(measured.key(), measured.value())?;
        changes.push(measured);
        let reference = if refs.peek().is_some_and(|r| r.path == path) {
            refs.next()
        } else {
            None
        };
        if let Some(reference) = reference {
            metadata_bytes = metadata_bytes
                .checked_add(reference.bytes)
                .context("metadata admission overflow")?;
            ensure!(
                metadata_bytes <= 2 * super::MAX_CURRENT_METADATA_READ_BYTES,
                "current metadata admission exceeded"
            );
            let super::ArtifactChange::Upsert(input) = change else {
                anyhow::bail!("metadata attached to deleted artifact");
            };
            let file = store.read_current_metadata(reference)?;
            verify(&input.artifact, &file, sequence)?;
            for (domain, identity) in handles(&file)? {
                let c =
                    map::MapChange::new(scope, domain, &identity, Some(path.as_bytes().to_vec()))?;
                plan.put(c.key(), c.value())?;
                changes.push(c);
            }
        }
        let c = map::MapChange::new(
            scope,
            MapDomain::FileMetadata,
            path.as_bytes(),
            reference.map(serde_json::to_vec).transpose()?,
        )?;
        plan.put(c.key(), c.value())?;
        changes.push(c);
    }
    ensure!(
        refs.next().is_none(),
        "unsorted or foreign current metadata references"
    );
    // Replacements remove and then insert handles. Capture only their final afterimage.
    let mut unique = std::collections::BTreeMap::new();
    for change in changes {
        unique.insert(change.key(), change);
    }
    Ok(unique.into_values().collect())
}

fn handles(file: &InventoryFile) -> Result<Vec<(MapDomain, Vec<u8>)>> {
    let mut keys = vec![(
        MapDomain::ArtifactHandle,
        file.state.artifact_id.as_bytes().to_vec(),
    )];
    for record in &file.continuity {
        keys.push((MapDomain::UnitHandle, serde_json::to_vec(&record.key)?));
    }
    Ok(keys)
}
