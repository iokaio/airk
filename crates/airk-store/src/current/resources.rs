// SPDX-License-Identifier: Apache-2.0
//! Small authenticated references to immutable current analysis and cache manifests.
use super::{
    CurrentRead,
    map::{self, MapDomain},
};
use crate::{FileStore, quota::Reservation};
use airk_core::identity::{Sha256Digest, framed_digest};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// For a codec read whose bounds came from an authenticated reference, a
/// non-I/O decode/identity failure is structural evidence. Filesystem errors
/// remain uncertainty and never become a corruption claim.
pub(crate) fn authenticated_decode_error(error: anyhow::Error) -> anyhow::Error {
    if error.downcast_ref::<std::io::Error>().is_some() {
        error
    } else {
        error.context(super::PrivateIntegrityFailure)
    }
}

pub const MAX_RESOURCE_BYTES: usize = 256 << 20;
pub const KEYS: [&str; 7] = [
    "analysis",
    "analysis-projection",
    "embedding-cache",
    "semantic-work",
    "semantic-policy",
    "scan-work",
    "source-controls/1",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceReference {
    pub digest: Sha256Digest,
    pub bytes: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<airk_core::compression::EncodedObject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection: Option<super::query_projection::ProjectionSummary>,
}
impl ResourceReference {
    pub(crate) fn heap_bytes(&self) -> usize {
        self.encoding.as_ref().map_or(0, |encoding| {
            encoding.logical_digest.capacity()
                + encoding.stored_digest.capacity()
                + encoding.index_digest.capacity()
        })
    }
    pub fn object_digest(&self) -> Result<Sha256Digest> {
        self.encoding
            .as_ref()
            .map_or(Ok(self.digest), |encoding| encoding.stored_digest.parse())
    }
    pub fn stored_bytes(&self) -> u64 {
        self.encoding
            .as_ref()
            .map_or(self.bytes as u64, |encoding| encoding.stored_bytes)
    }
}

/// Exact canonical and encoded identity prepared before optional quota admission.
pub(crate) struct PreparedResource {
    pub(crate) reference: ResourceReference,
    stored: Vec<u8>,
}
impl PreparedResource {
    pub(crate) fn memory_bytes(&self) -> usize {
        self.stored.capacity() + self.reference.heap_bytes() + std::mem::size_of::<Self>()
    }
}
pub(crate) fn prepare_resource(
    bytes: Vec<u8>,
    mode: airk_core::compression::CompressionMode,
) -> Result<PreparedResource> {
    ensure!(
        bytes.len() <= MAX_RESOURCE_BYTES,
        "current resource exceeds byte budget"
    );
    let encoded = crate::compression::encode(&bytes, mode)?;
    let reference = ResourceReference {
        digest: framed_digest(b"current-resource/1", &[&bytes]),
        bytes: bytes.len(),
        encoding: encoded.as_ref().map(|(_, encoding)| encoding.clone()),
        projection: None,
    };
    Ok(PreparedResource {
        reference,
        stored: encoded.map_or(bytes, |(stored, _)| stored),
    })
}

#[derive(Clone, Debug)]
pub struct ResourceChange {
    pub key: String,
    pub reference: Option<ResourceReference>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ResourceWriteCounts {
    pub created_objects: u64,
    pub reused_objects: u64,
    pub created_logical_bytes: u64,
    pub reused_logical_bytes: u64,
    pub created_stored_bytes: u64,
    pub reused_stored_bytes: u64,
}
impl ResourceWriteCounts {
    fn record(&mut self, created: bool, bytes: usize, stored: u64) -> Result<()> {
        let mut next = self.clone();
        if created {
            next.created_objects = next
                .created_objects
                .checked_add(1)
                .context("resource count overflow")?;
            next.created_logical_bytes = next
                .created_logical_bytes
                .checked_add(bytes as u64)
                .context("resource byte overflow")?;
        } else {
            next.reused_objects = next
                .reused_objects
                .checked_add(1)
                .context("resource count overflow")?;
            next.reused_logical_bytes = next
                .reused_logical_bytes
                .checked_add(bytes as u64)
                .context("resource byte overflow")?;
        }
        if created {
            next.created_stored_bytes = next
                .created_stored_bytes
                .checked_add(stored)
                .context("resource stored byte overflow")?;
        } else {
            next.reused_stored_bytes = next
                .reused_stored_bytes
                .checked_add(stored)
                .context("resource stored byte overflow")?;
        }
        *self = next;
        Ok(())
    }
}
impl FileStore {
    pub(crate) fn prepared_resource_exists(&self, value: &PreparedResource) -> Result<bool> {
        let directory = self.directory().join("current-resources");
        match std::fs::symlink_metadata(&directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
            Ok(metadata) => ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "linked current resource directory"
            ),
        }
        let path = directory.join(format!("{}.json", value.reference.object_digest()?));
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
            Ok(metadata) => {
                ensure!(
                    metadata.is_file()
                        && !metadata.file_type().is_symlink()
                        && metadata.len() == value.reference.stored_bytes(),
                    "invalid prepared resource ownership"
                );
                crate::managed_collection::reject_file_aliases(&path, &metadata)?;
                Ok(true)
            }
        }
    }
    /// Owning callers validate writer/reservation once for their bounded batch.
    pub(crate) fn retain_prepared_resource(
        &self,
        value: &PreparedResource,
        reservation: Option<&Reservation>,
        counts: Option<&mut ResourceWriteCounts>,
    ) -> Result<()> {
        let created = !self.prepared_resource_exists(value)?;
        if created {
            let reservation = reservation.context("new optional resource lacks admitted quota")?;
            reservation.charge(value.reference.bytes as u64)?;
            let directory = self.directory().join("current-resources");
            if !directory.try_exists()? {
                std::fs::create_dir(&directory)?;
            }
            let metadata = std::fs::symlink_metadata(&directory)?;
            ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "linked current resource directory"
            );
            let path = directory.join(format!("{}.json", value.reference.object_digest()?));
            let mut temporary = tempfile::NamedTempFile::new_in(reservation.staging_directory()?)?;
            temporary.write_all(&value.stored)?;
            crate::durability::persist_synced(temporary, &path)?;
        }
        self.read_current_resource(&value.reference)?;
        if let Some(counts) = counts {
            counts.record(
                created,
                value.reference.bytes,
                value.reference.stored_bytes(),
            )?;
        }
        Ok(())
    }
    /// Writer-owned producer seam; legacy callers remain unqualified for census promotion.
    pub fn retain_current_resource_owned(
        &self,
        writer: &crate::WriterLock,
        bytes: &[u8],
        reservation: &Reservation,
    ) -> Result<ResourceReference> {
        super::gate::check_writer(self, writer)?;
        writer.invalidate_census_before_mutation()?;
        self.retain_current_resource(bytes, reservation)
    }

    pub fn retain_current_resource(
        &self,
        bytes: &[u8],
        reservation: &Reservation,
    ) -> Result<ResourceReference> {
        self.retain_current_resource_encoded_counted(
            bytes,
            reservation,
            airk_core::compression::CompressionMode::Off,
            None,
        )
    }
    pub(crate) fn retain_current_resource_encoded_counted(
        &self,
        bytes: &[u8],
        reservation: &Reservation,
        mode: airk_core::compression::CompressionMode,
        counts: Option<&mut ResourceWriteCounts>,
    ) -> Result<ResourceReference> {
        ensure!(
            bytes.len() <= MAX_RESOURCE_BYTES,
            "current resource exceeds byte budget"
        );
        let encoded = crate::compression::encode(bytes, mode)?;
        let reference = ResourceReference {
            digest: framed_digest(b"current-resource/1", &[bytes]),
            bytes: bytes.len(),
            encoding: encoded.as_ref().map(|(_, encoding)| encoding.clone()),
            projection: None,
        };
        let directory = self.directory().join("current-resources");
        if !directory.try_exists()? {
            std::fs::create_dir(&directory)?;
        }
        let metadata = std::fs::symlink_metadata(&directory)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "linked current resource directory"
        );
        let path = directory.join(format!("{}.json", reference.object_digest()?));
        let created = !path.try_exists()?;
        if created {
            reservation.charge(bytes.len() as u64)?;
            let mut temporary = tempfile::NamedTempFile::new_in(reservation.staging_directory()?)?;
            temporary.write_all(
                encoded
                    .as_ref()
                    .map_or(bytes, |(bytes, _)| bytes.as_slice()),
            )?;
            crate::durability::persist_synced(temporary, &path)?;
        }
        self.read_current_resource(&reference)?;
        if let Some(counts) = counts {
            counts.record(created, bytes.len(), reference.stored_bytes())?;
        }
        Ok(reference)
    }

    pub fn read_current_resource(&self, reference: &ResourceReference) -> Result<Vec<u8>> {
        ensure!(
            reference.bytes <= MAX_RESOURCE_BYTES,
            "current resource exceeds byte budget"
        );
        let directory = self.directory().join("current-resources");
        let metadata = std::fs::symlink_metadata(&directory)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(anyhow::anyhow!("linked current resource directory")
                .context(super::PrivateIntegrityFailure));
        }
        let path = directory.join(format!("{}.json", reference.object_digest()?));
        let metadata = std::fs::symlink_metadata(&path)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() != reference.stored_bytes()
        {
            return Err(
                anyhow::anyhow!("invalid current resource size or ownership")
                    .context(super::PrivateIntegrityFailure),
            );
        }
        let bytes = if let Some(encoding) = &reference.encoding {
            crate::compression::read_all(
                &path,
                encoding,
                &encoding.logical_digest,
                reference.bytes as u64,
            )
            .map_err(authenticated_decode_error)?
        } else {
            let mut bytes = Vec::new();
            std::fs::File::open(path)?
                .take(reference.bytes as u64 + 1)
                .read_to_end(&mut bytes)?;
            bytes
        };
        if bytes.len() != reference.bytes
            || framed_digest(b"current-resource/1", &[&bytes]) != reference.digest
        {
            return Err(anyhow::anyhow!("current resource digest mismatch")
                .context(super::PrivateIntegrityFailure));
        }
        Ok(bytes)
    }
}

impl CurrentRead {
    pub fn resource_reference(&self, key: &str) -> Result<Option<ResourceReference>> {
        ensure!(KEYS.contains(&key), "unknown current resource");
        self.map_get(MapDomain::Metadata, key.as_bytes())?
            .as_deref()
            .map(decode)
            .transpose()
    }
    pub fn resource(&self, store: &FileStore, key: &str) -> Result<Option<Vec<u8>>> {
        self.resource_reference(key)?
            .as_ref()
            .map(|r| store.read_current_resource(r))
            .transpose()
    }
}

pub(crate) fn decode(bytes: &[u8]) -> Result<ResourceReference> {
    let reference: ResourceReference = serde_json::from_slice(bytes)?;
    if let Some(summary) = &reference.projection {
        summary.validate()?;
    }
    ensure!(
        reference.bytes <= MAX_RESOURCE_BYTES && serde_json::to_vec(&reference)? == bytes,
        "noncanonical current resource reference"
    );
    Ok(reference)
}

pub(crate) fn prepare(
    store: &FileStore,
    scope: &super::CurrentScope,
    plan: &mut map::MapPlan<'_>,
    resources: &[ResourceChange],
) -> Result<Vec<map::MapChange>> {
    ensure!(resources.len() <= KEYS.len(), "too many current resources");
    let mut keys = std::collections::BTreeSet::new();
    resources
        .iter()
        .map(|resource| {
            ensure!(
                KEYS.contains(&resource.key.as_str()) && keys.insert(&resource.key),
                "unknown or duplicate current resource"
            );
            if let Some(reference) = &resource.reference {
                store.read_current_resource(reference)?;
            }
            let change = map::MapChange::new(
                scope,
                MapDomain::Metadata,
                resource.key.as_bytes(),
                resource
                    .reference
                    .as_ref()
                    .map(serde_json::to_vec)
                    .transpose()?,
            )?;
            plan.put(change.key(), change.value())
                .context("stage current resource")?;
            Ok(change)
        })
        .collect()
}
