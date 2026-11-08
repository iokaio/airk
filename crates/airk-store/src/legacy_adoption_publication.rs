// SPDX-License-Identifier: Apache-2.0
//! Canonical saved-adoption provenance and opaque proofs from actual Current reads.
use super::LegacyPublicationIdentity;
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// These observations describe work still needed by the adoption coordinator;
/// retaining an archive is mandatory, never a claim that vectors were imported.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyCompatibility {
    pub retained_legacy_artifacts: usize,
    pub unmanifested_imported_facts: usize,
    pub unmanifested_imported_relationships: usize,
    pub original_embedding_model: Option<String>,
    pub original_persistent_indexes: bool,
    pub retrieval_converted: bool,
    pub requires_archive_retention: bool,
    pub analysis_requires_legacy_retention: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdoptionResource {
    pub schema: u32,
    pub original: LegacyPublicationIdentity,
    pub target_incarnation: airk_core::identity::Incarnation,
    pub target_sequence: u64,
    pub root: String,
    pub collection: String,
    pub roots: BTreeMap<String, String>,
    pub data_root: String,
    pub compatibility: LegacyCompatibility,
}

/// Native storage proof. Only an actual pinned Current publication and its
/// authenticated canonical adoption resource can construct this value.
pub struct VerifiedLegacyPublication {
    _writer_exclusion: crate::ExistingWriterExclusion,
    pin: std::sync::Arc<crate::current::CurrentRead>,
    current: airk_core::identity::RevisionToken,
    resource_sha256: airk_core::identity::Sha256Digest,
    compatibility_sha256: airk_core::identity::Sha256Digest,
    original_publication_sha256: airk_core::identity::Sha256Digest,
    archive_inventory_sha256: airk_core::identity::Sha256Digest,
    root: String,
    collection: String,
    roots: BTreeMap<String, String>,
    data_root: String,
}

impl VerifiedLegacyPublication {
    /// Retain this actual shared publication gate through the native metadata
    /// CAS. Hashes alone are not a continuing publication exclusion capability.
    pub fn revalidate(&self) -> Result<()> {
        let token = self.pin.token()?;
        ensure!(
            token.incarnation == self.current.incarnation
                && token.sequence == self.current.generation
                && token.digest == self.current.revision
                && !self.pin.has_prepared_update()?
                && self
                    .pin
                    .resource_reference("legacy-adoption")?
                    .is_some_and(|reference| reference.digest == self.resource_sha256),
            "legacy publication pin changed"
        );
        Ok(())
    }
    pub fn current_revision_token(&self) -> airk_core::identity::RevisionToken {
        self.current
    }
    pub fn resource_sha256(&self) -> airk_core::identity::Sha256Digest {
        self.resource_sha256
    }
    pub fn compatibility_sha256(&self) -> airk_core::identity::Sha256Digest {
        self.compatibility_sha256
    }
    pub fn original_publication_sha256(&self) -> airk_core::identity::Sha256Digest {
        self.original_publication_sha256
    }
    pub fn archive_inventory_sha256(&self) -> airk_core::identity::Sha256Digest {
        self.archive_inventory_sha256
    }
    pub fn root(&self) -> &str {
        &self.root
    }
    pub fn collection(&self) -> &str {
        &self.collection
    }
    pub fn roots(&self) -> &BTreeMap<String, String> {
        &self.roots
    }
    pub fn data_root(&self) -> &str {
        &self.data_root
    }
}

pub fn verify_publication(
    store: &crate::FileStore,
    read: &crate::current::CurrentRead,
) -> Result<VerifiedLegacyPublication> {
    let writer_exclusion = store.existing_writer_exclusion()?;
    ensure!(
        !read.restricts_paths(),
        "legacy publication proof requires the complete destination"
    );
    ensure!(
        !read.has_prepared_update()?,
        "legacy publication proof requires settled current storage"
    );
    let managed = store.managed_path()?;
    ensure!(
        managed.namespace == "collections-1.4"
            && store.service_incarnation()? == Some(read.head().scope.incarnation),
        "legacy proof requires this exact service incarnation"
    );
    let reference = read
        .resource_reference("legacy-adoption")?
        .context("current publication has no legacy adoption resource")?;
    let bytes = store.read_current_resource(&reference)?;
    ensure!(
        bytes.len() <= 2 << 20,
        "legacy adoption resource exceeds verification bound"
    );
    let resource: AdoptionResource = serde_json::from_slice(&bytes)?;
    ensure!(
        serde_json::to_vec(&resource)? == bytes,
        "legacy adoption resource is not canonical"
    );
    let token = read.token()?;
    let state = read
        .collection_state()?
        .context("legacy current configuration missing")?;
    let data_root = managed.root.canonicalize()?;
    let data_root = data_root.to_str().context("legacy data root is not UTF8")?;
    ensure!(
        resource.schema == 1
            && resource.target_incarnation == token.incarnation
            && resource.target_sequence == token.sequence
            && resource.root == read.head().scope.root
            && resource.collection == managed.collection
            && resource.collection == read.head().scope.collection
            && resource.roots == state.configuration.roots
            && resource.data_root == data_root,
        "legacy adoption resource does not describe the exact current destination"
    );
    if let Some(header) = &resource.original.header {
        ensure!(
            header.root == resource.root
                && header.collection == resource.collection
                && header.roots == resource.roots
                && header.generation == resource.original.generation,
            "legacy original publication descriptor differs from adopted scope"
        );
    }
    if let Some(original) = &resource.original.current {
        original.validate()?;
        ensure!(
            resource.original.header.is_none()
                && original.head.scope.root == resource.root
                && original.head.scope.collection == resource.collection
                && original.collection.configuration.roots == resource.roots
                && original.token.sequence == resource.original.generation
                && original.token.digest == resource.original.original_root_sha256,
            "original Current publication differs from adopted provenance"
        );
    }
    ensure!(
        resource.compatibility.requires_archive_retention
            && !resource.compatibility.retrieval_converted,
        "unsupported legacy adoption compatibility contract"
    );
    let pin = store.current_read(&read.head().scope)?;
    ensure!(
        !pin.has_prepared_update()?
            && pin.token()? == token
            && pin
                .resource_reference("legacy-adoption")?
                .is_some_and(|value| value.digest == reference.digest),
        "legacy current publication changed while acquiring native completion pin"
    );
    Ok(VerifiedLegacyPublication {
        _writer_exclusion: writer_exclusion,
        pin,
        current: airk_core::identity::RevisionToken {
            incarnation: token.incarnation,
            generation: token.sequence,
            revision: token.digest,
        },
        resource_sha256: reference.digest,
        compatibility_sha256: airk_core::identity::Sha256Digest::from_bytes(
            Sha256::digest(serde_json::to_vec(&resource.compatibility)?).into(),
        ),
        original_publication_sha256: resource.original.original_root_sha256,
        archive_inventory_sha256: resource.original.archive_inventory_sha256,
        root: resource.root,
        collection: resource.collection,
        roots: resource.roots,
        data_root: resource.data_root,
    })
}
