// SPDX-License-Identifier: Apache-2.0
//! Persisted provenance of an adopted historical Current publication.
use crate::current::{
    CurrentHead, CurrentToken,
    collection::CollectionState,
    resources::{KEYS, ResourceReference},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Original evidence, not permission to reuse a model or producer in the new
/// namespace. Resources and graph remain in the immutable retained archive.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalCurrentPublication {
    pub token: CurrentToken,
    pub head: CurrentHead,
    pub collection: CollectionState,
    pub resources: BTreeMap<String, ResourceReference>,
    pub resource_conversion: String,
}
impl OriginalCurrentPublication {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.head.token()? == self.token && self.collection.version == 1,
            "original Current token or configuration changed"
        );
        self.collection.configuration.validate()?;
        ensure!(
            self.resources.len() <= KEYS.len()
                && self
                    .resources
                    .keys()
                    .all(|key| KEYS.contains(&key.as_str()))
                && self.resource_conversion
                    == "retained_original_archive; only validated analysis and artifact evidence are converted; graph, model, policies and other resource execution are not adopted",
            "original Current resource provenance changed"
        );
        Ok(())
    }
}
