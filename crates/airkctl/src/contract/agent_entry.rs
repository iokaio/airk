// SPDX-License-Identifier: Apache-2.0
//! I2 agent-entry data and strict decode/schema rules; editing stays in agents.
use airk_core::identity::Incarnation;
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize};

/// The agents whose files AIRK reads (`agentMigration.ts` `locations`).
/// Only Claude Code, Codex and VS Code (`copilot`) can be connected (U91).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Agent {
    Codex,
    Claude,
    Copilot,
    Gemini,
    Antigravity,
    Cursor,
    Roo,
    Junie,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Scope {
    Project,
    User,
}

/// The §6.4 classes, plus `unreadable` for a present file that could not be
/// read or parsed (it could hide a broken entry).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Class {
    StdioRetired,
    StdioMissingCommand,
    HttpsUnknownConnection,
    HttpsEndpointMismatch,
    HttpsHelperMissing,
    HttpsTrustMissing,
    Ok,
    Unreadable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Transport {
    Stdio,
    Https,
}

/// One recognized entry, healthy ones included.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Entry {
    pub(crate) agent: Agent,
    pub(crate) scope: Scope,
    pub(crate) file: String,
    /// Exact declaration container, carried unchanged from the native report.
    pub(crate) container_path: Vec<String>,
    /// A proven helper or launcher can establish ownership with a custom key.
    pub(crate) server_key: String,
    pub(crate) transport: Transport,
    /// `None`: recognized, but an observation it needs is unavailable.
    #[serde(deserialize_with = "entry_status")]
    #[schemars(schema_with = "entry_status_schema", required)]
    pub(crate) status: Option<Class>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "entry_connection_schema")]
    pub(crate) connection_id: Option<Incarnation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) collection: Option<String>,
}

// Unlike optional observation fields, the report always emits status, including null.
fn entry_status<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Class>, D::Error> {
    Option::<Class>::deserialize(deserializer)
}

fn entry_status_schema(generator: &mut SchemaGenerator) -> Schema {
    json_schema!({"anyOf": [generator.subschema_for::<Class>(), {"type": "null"}]})
}

fn entry_connection_schema(_: &mut SchemaGenerator) -> Schema {
    json_schema!({"anyOf": [{"type": "string", "pattern": "^[0-9a-f]{32}$"}, {"type": "null"}]})
}
