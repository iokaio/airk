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

pub const MAX_RESOURCE_BYTES: usize = 256 << 20;
pub const KEYS: [&str; 5] = [
    "analysis",
    "embedding-cache",
    "semantic-work",
    "semantic-policy",
    "scan-work",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceReference {
    pub digest: Sha256Digest,
    pub bytes: usize,
}

#[derive(Clone, Debug)]
pub struct ResourceChange {
    pub key: String,
    pub reference: Option<ResourceReference>,
}

impl FileStore {
    pub fn retain_current_resource(
        &self,
        bytes: &[u8],
        reservation: &Reservation,
    ) -> Result<ResourceReference> {
        ensure!(
            bytes.len() <= MAX_RESOURCE_BYTES,
            "current resource exceeds byte budget"
        );
        let reference = ResourceReference {
            digest: framed_digest(b"current-resource/1", &[bytes]),
            bytes: bytes.len(),
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
        let path = directory.join(format!("{}.json", reference.digest));
        if !path.try_exists()? {
            reservation.charge(bytes.len() as u64)?;
            let mut temporary = tempfile::NamedTempFile::new_in(reservation.staging_directory()?)?;
            temporary.write_all(bytes)?;
            crate::durability::persist_synced(temporary, &path)?;
        }
        self.read_current_resource(&reference)?;
        Ok(reference)
    }

    pub fn read_current_resource(&self, reference: &ResourceReference) -> Result<Vec<u8>> {
        ensure!(
            reference.bytes <= MAX_RESOURCE_BYTES,
            "current resource exceeds byte budget"
        );
        let directory = self.directory().join("current-resources");
        let metadata = std::fs::symlink_metadata(&directory)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "linked current resource directory"
        );
        let path = directory.join(format!("{}.json", reference.digest));
        let metadata = std::fs::symlink_metadata(&path)?;
        ensure!(
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && metadata.len() == reference.bytes as u64,
            "invalid current resource size or ownership"
        );
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(reference.bytes as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() == reference.bytes
                && framed_digest(b"current-resource/1", &[&bytes]) == reference.digest,
            "current resource digest mismatch"
        );
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
