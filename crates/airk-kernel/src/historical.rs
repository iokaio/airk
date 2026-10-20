// SPDX-License-Identifier: Apache-2.0
//! Versioned boundary for removed historical APIs.
use crate::Kernel;
use airk_core::{evidence::RevisionEnvelope, identity::RevisionToken};
use anyhow::Result;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryAvailabilityRequest {
    pub revision: Option<RevisionToken>,
    pub cursor: Option<String>,
    pub limit: usize,
    pub max_bytes: usize,
}
#[derive(Serialize)]
pub struct HistoryAvailability {
    pub current_publication: RevisionEnvelope,
    pub revisions: Vec<RevisionToken>,
    /// Receipts prove retained publication identity; exact reads verify representations.
    pub representation_verification: &'static str,
    pub next_cursor: Option<String>,
    pub complete: bool,
}
impl Kernel {
    pub fn history_availability(
        &self,
        _request: &HistoryAvailabilityRequest,
    ) -> Result<HistoryAvailability> {
        anyhow::bail!(
            "historical revisions are unsupported in AIRK 1.2; only the current index is retained"
        )
    }
}
