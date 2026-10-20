// SPDX-License-Identifier: Apache-2.0
//! Declared semantic coverage for one committed current revision.
use crate::identity::Sha256Digest;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticState {
    Disabled,
    Pending,
    Partial,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultSearchPolicy {
    #[default]
    PartialHybrid,
    LexicalWhilePending,
    RequireComplete,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticReadiness {
    pub version: u32,
    pub state: SemanticState,
    pub default_search_policy: DefaultSearchPolicy,
    pub selected_artifacts: u64,
    pub completed_artifacts: u64,
    pub selected_units: Option<u64>,
    pub completed_units: u64,
    pub first_ready_unix_ms: Option<u64>,
    pub fully_ready_unix_ms: Option<u64>,
    pub selection_digest: Option<Sha256Digest>,
    pub pipeline_digest: Option<Sha256Digest>,
    pub failure: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscoveryReadiness {
    pub version: u32,
    pub state: DiscoveryState,
    pub epoch: Option<crate::identity::Incarnation>,
    pub processed_artifacts: u64,
    pub committed_artifacts: u64,
    /// Unknown until a complete, validated scan permits the absence sweep.
    pub total_artifacts: Option<u64>,
    pub failure: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryState {
    Pending,
    Partial,
    Ready,
    Failed,
}
