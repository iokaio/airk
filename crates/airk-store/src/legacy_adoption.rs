// SPDX-License-Identifier: Apache-2.0
//! Provenance of saved legacy evidence already adopted into a Current
//! publication by an earlier release. Adoption itself is no longer supported;
//! these persisted shapes remain readable through the `legacy-adoption` resource.
use crate::PersistentRevision;
use airk_core::identity::Sha256Digest;
use serde::{Deserialize, Serialize};

mod current_archive;
mod historical_catalog;
pub use current_archive::OriginalCurrentPublication;
pub use historical_catalog::OriginalCatalogPublication;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyPublicationIdentity {
    pub header: Option<PersistentRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<OriginalCurrentPublication>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<OriginalCatalogPublication>,
    pub generation: u64,
    pub original_root_sha256: Sha256Digest,
    pub archive_inventory_sha256: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedFile {
    pub bytes: u64,
    pub sha256: Sha256Digest,
}

pub mod publication;
pub use publication::{AdoptionResource, LegacyCompatibility};
