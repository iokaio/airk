// SPDX-License-Identifier: Apache-2.0
//! Persisted provenance of an adopted receipt-bound generation-4/5 catalog.
use super::SavedFile;
use airk_core::identity::Sha256Digest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalCatalogPublication {
    pub receipt: airk_core::identity::PublicationReceipt,
    pub manifest_schema: u32,
    pub engine: String,
    pub inventory_version: u32,
    pub index_detail: Option<airk_core::extraction::IndexDetail>,
    pub extraction_policy_digest: Option<Sha256Digest>,
    pub embedding_pipeline_digest: Option<Sha256Digest>,
    pub embedding_model: Option<String>,
    pub components: BTreeMap<String, SavedFile>,
    pub resource_conversion: String,
}
