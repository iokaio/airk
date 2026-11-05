// SPDX-License-Identifier: Apache-2.0
//! Bounded query-grant discovery inputs. Metadata identifiers confer no authority.
use crate::index_catalog::{IndexId, QueryProjectId};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryScopeView {
    Projects,
    Indexes,
    Sources,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryScopeArgs {
    pub view: QueryScopeView,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub project_id: Option<QueryProjectId>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub index_id: Option<IndexId>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub cursor: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub limit: Option<usize>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub max_bytes: Option<usize>,
}
fn present<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
impl QueryScopeArgs {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            match self.view {
                QueryScopeView::Projects => self.project_id.is_none() && self.index_id.is_none(),
                QueryScopeView::Indexes => self.project_id.is_some() && self.index_id.is_none(),
                QueryScopeView::Sources => self.project_id.is_some() && self.index_id.is_some(),
            },
            "query_scope view requires its exact Project/index selectors"
        );
        ensure!(
            (1..=32).contains(&self.limit())
                && (1..=65536).contains(&self.max_bytes())
                && self
                    .cursor
                    .as_ref()
                    .is_none_or(|cursor| !cursor.is_empty() && cursor.len() <= 32768),
            "query_scope pagination budget exceeded"
        );
        Ok(())
    }
    pub fn limit(&self) -> usize {
        self.limit.unwrap_or(16)
    }
    pub fn max_bytes(&self) -> usize {
        self.max_bytes.unwrap_or(16384)
    }
}

/// Object-shaped catalog for MCP clients; cross-field requirements are also
/// checked by the same native validator before metadata or member work.
pub fn schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["view"],"properties":{
        "view":{"type":"string","enum":["projects","indexes","sources"],"description":"projects accepts no IDs; indexes requires project_id only; sources requires project_id and index_id. The native owner rejects incompatible selectors before reading."},
        "project_id":{"type":"string","pattern":"^prj_[0-9a-f]{32}$"},
        "index_id":{"type":"string","pattern":"^idx_[0-9a-f]{32}$"},
        "cursor":{"type":"string","minLength":1,"maxLength":32768},
        "limit":{"type":"integer","minimum":1,"maximum":32,"default":16},
        "max_bytes":{"type":"integer","minimum":1,"maximum":65536,"default":16384}
    }})
}

/// The legacy configured-binding catalog cannot infer a reviewed Project grant.
pub fn legacy_body(args: &QueryScopeArgs) -> Result<Value> {
    args.validate()?;
    ensure!(
        args.cursor.is_none(),
        "legacy query_scope has no continuation"
    );
    let body = json!({"schema":"airk.query-scope/1","mode":"legacy","status":"configuration_required",
        "projects":[],"indexes":[],"sources":[],"truncated":false,"continuation":null,
        "grant":null,"scope_revision":null,
        "reason":"Connect using an explicitly reviewed Project/index grant to discover new query scope."});
    crate::index_catalog::encoded_size(&body, args.max_bytes())?;
    Ok(body)
}
