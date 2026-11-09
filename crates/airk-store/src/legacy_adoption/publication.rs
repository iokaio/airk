// SPDX-License-Identifier: Apache-2.0
//! Canonical saved-adoption provenance resource of a Current publication.
use super::LegacyPublicationIdentity;
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
