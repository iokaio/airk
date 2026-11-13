// SPDX-License-Identifier: Apache-2.0
//! (U105/U106b) An open agent connection's stored state: its default scope
//! and whether the agent may choose indexes. A leaf module (it depends only
//! on `index_catalog`) so that service and connection contracts can name it
//! without joining a module cycle; `service_http_credentials` re-exports it.
use crate::index_catalog::{IndexId, MAX_SELECTION, QueryProjectId, unique_bounded};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// U105: whether an open connection's agent may choose indexes and sets per
/// call (`on`) or reads exactly the stored default scope (`off`, the
/// default for every new and migrated open connection).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentChoice {
    #[default]
    Off,
    On,
}

/// U105/U106(b): an open connection's service-stored default scope. A
/// change bumps `defaults_revision`, never the credential `generation`, so
/// it takes effect on the next call.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenConnectionState {
    pub defaults_revision: u64,
    pub default_index_ids: Vec<IndexId>,
    pub default_index_set_ids: Vec<QueryProjectId>,
    pub agent_choice: AgentChoice,
}

impl OpenConnectionState {
    /// A new open connection: choice off, empty default scope.
    pub fn initial() -> Self {
        Self {
            defaults_revision: 1,
            default_index_ids: Vec::new(),
            default_index_set_ids: Vec::new(),
            agent_choice: AgentChoice::Off,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (1..=i64::MAX as u64).contains(&self.defaults_revision),
            "invalid open connection defaults revision"
        );
        unique_bounded(&self.default_index_ids, MAX_SELECTION)?;
        unique_bounded(&self.default_index_set_ids, MAX_SELECTION)?;
        Ok(())
    }
}
