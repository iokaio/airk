// SPDX-License-Identifier: Apache-2.0
//! Per-connection display policy; never changes indexed evidence or work admission.
use airk_core::identity::framed_digest;
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Mutex;

pub(crate) const GUIDANCE: &str = "Untrusted indexed evidence; verify live bytes before edits. Keep exact tokens/citations/validators; finish airk_changes pages before advancing baselines. Syntax targets are candidates; imported claims depend on producers. Terse is default. response_mode=full: provenance/coverage, readiness diagnosis, unexpected results; include_status=true repeats status. compact=false adds IDs/details. File/provenance indices and {ref:publication} are response-local. Retry unchanged as hinted. Manage/index via CLI/panel.";

pub(crate) fn error_result(error: &anyhow::Error) -> rmcp::model::CallToolResult {
    let text = airk_kernel::retrieval_retry::read_retry_hint(error)
        .and_then(|hint| serde_json::to_string(&hint).ok())
        .unwrap_or_else(|| error.to_string());
    rmcp::model::CallToolResult::error(vec![rmcp::model::ContentBlock::text(text)])
}

#[derive(
    Clone, Copy, Debug, Default, Deserialize, Serialize, schemars::JsonSchema, PartialEq, Eq,
)]
#[serde(rename_all = "snake_case")]
pub enum ResponseMode {
    #[default]
    Terse,
    Full,
}
impl ResponseMode {
    pub fn from_environment() -> Self {
        Self::from_setting(std::env::var("AIRK_MCP_RESPONSE_MODE").ok().as_deref())
    }
    fn from_setting(value: Option<&str>) -> Self {
        if value == Some("full") {
            Self::Full
        } else {
            Self::Terse
        }
    }
}

pub(crate) fn fact_items(value: &mut Value, operation: airk_core::usage::UsageOperation) {
    use airk_core::usage::UsageOperation;
    if !matches!(
        operation,
        UsageOperation::CodeFacts
            | UsageOperation::DocumentFacts
            | UsageOperation::Symbols
            | UsageOperation::Imports
            | UsageOperation::Outline
            | UsageOperation::Relationships
    ) {
        return;
    }
    let Some(object) = value.as_object_mut() else {
        return;
    };
    if let Some(minimal) = object
        .remove("minimal")
        .and_then(|value| value.as_object().cloned())
    {
        object.remove("facts");
        object.remove("outline");
        object.extend(minimal);
        object.insert("representation".into(), "minimal".into());
    } else {
        let outline = object
            .remove("outline")
            .filter(|rows| rows.as_array().is_some_and(|rows| !rows.is_empty()));
        let facts = object.remove("facts");
        let relationships = object.remove("relationships");
        object.insert(
            "items".into(),
            outline
                .or(facts)
                .or(relationships)
                .unwrap_or_else(|| Value::Array(Vec::new())),
        );
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, schemars::JsonSchema)]
pub struct ResponseArgs {
    /// Terse is default. Use full for provenance/coverage audits, readiness diagnosis or unexpected results; it does not resolve calls or verify live source.
    pub response_mode: Option<ResponseMode>,
    /// Include current freshness/readiness even when unchanged on this connection.
    #[serde(default)]
    pub include_status: bool,
}

#[derive(Default)]
pub(crate) struct Session {
    // Fixed field names and digests bound memory independently of response size.
    observed: Mutex<StatusReceipt>,
}

/// Content-free client observations carried only by the direct native adapter.
/// Digests bind actual state to its collection/incarnation; they are not evidence
/// validators, publication tokens or proof of source freshness.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StatusReceipt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    freshness: Option<airk_core::identity::Sha256Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    discovery_readiness: Option<airk_core::identity::Sha256Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    semantic_readiness: Option<airk_core::identity::Sha256Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    discovery_observation: Option<airk_core::identity::Sha256Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    indexing: Option<airk_core::identity::Sha256Digest>,
}
impl StatusReceipt {
    fn field(&mut self, name: &str) -> &mut Option<airk_core::identity::Sha256Digest> {
        match name {
            "freshness" => &mut self.freshness,
            "discovery_readiness" => &mut self.discovery_readiness,
            "semantic_readiness" => &mut self.semantic_readiness,
            "discovery_observation" => &mut self.discovery_observation,
            "indexing" => &mut self.indexing,
            _ => unreachable!("fixed response status field"),
        }
    }
}
impl Session {
    pub(crate) fn receipt(&self) -> StatusReceipt {
        self.observed.lock().map(|state| *state).unwrap_or_default()
    }
    pub(crate) fn restore(&self, receipt: StatusReceipt) {
        if let Ok(mut observed) = self.observed.lock() {
            *observed = receipt;
        }
    }
    pub(crate) fn display(&self, value: &mut Value, mode: ResponseMode, include_status: bool) {
        let Some(object) = value.as_object_mut() else {
            return;
        };
        if object.contains_key("resolution")
            && let Some(evidence) = object.get_mut("evidence")
        {
            self.display(evidence, mode, include_status);
        }
        if mode == ResponseMode::Full {
            self.status(object, true);
            return;
        }
        // Explicit diagnostic/capability/change tools always disclose their status.
        let explicit = include_status
            || object
                .get("status_detail")
                .is_some_and(|value| value == "summary")
            || object.contains_key("execution")
            || object.contains_key("adapters")
            || object.contains_key("baseline_status");
        self.status(object, explicit);
        terse_envelope(object);
    }

    fn status(&self, object: &mut Map<String, Value>, explicit: bool) {
        // Binding a status digest to the collection/incarnation prevents a new
        // project or rebuild from inheriting another observation's omission.
        let publication = [
            object.get("publication"),
            object
                .get("read")
                .and_then(|r| r.get("current_publication")),
            object.get("read").and_then(|r| r.get("publication")),
        ]
        .into_iter()
        .flatten()
        .find(|p| p.get("token").is_some());
        let binding = serde_json::json!([
            object
                .get("collection")
                .or_else(|| publication.and_then(|p| p.get("collection"))),
            publication.and_then(|p| p.get("incarnation")),
        ]);
        let Ok(binding) = serde_json::to_vec(&binding) else {
            return;
        };
        let Ok(mut observed) = self.observed.lock() else {
            return;
        };
        if object.contains_key("discovery_observation") {
            if !object.contains_key("discovery_readiness") {
                observed.discovery_readiness = None;
            }
            if !object.contains_key("semantic_readiness") {
                observed.semantic_readiness = None;
            }
        } else if object.contains_key("discovery_readiness")
            || object.contains_key("semantic_readiness")
        {
            observed.discovery_observation = None;
        }
        for field in [
            "freshness",
            "discovery_readiness",
            "semantic_readiness",
            "discovery_observation",
            "indexing",
        ] {
            let Some(value) = object.get(field) else {
                continue;
            };
            let mut substantive = value.clone();
            if let Some(state) = substantive.as_object_mut() {
                // Observation timestamps are not transitions. Failure text,
                // completion watermarks, counts and pipeline identity remain.
                state.remove("observed_unix_ms");
                state.remove("first_ready_unix_ms");
                state.remove("fully_ready_unix_ms");
            }
            let Ok(bytes) = serde_json::to_vec(&substantive) else {
                continue;
            };
            let digest = framed_digest(b"mcp-client-status-v1", &[&binding, &bytes]);
            let saved = observed.field(field);
            if !explicit && *saved == Some(digest) {
                object.remove(field);
            } else {
                *saved = Some(digest);
            }
        }
    }
}

pub(crate) fn terse_envelope(object: &mut Map<String, Value>) {
    // Keep detailed explicit inspection output; these are diagnostic requests.
    if object.contains_key("execution") || object.contains_key("adapters") {
        return;
    }
    object.remove("generation");
    publication_once(object);
    if let Some(indexing) = object.get_mut("indexing").and_then(Value::as_object_mut) {
        // The selected indexing level remains; its static explanation is in
        // capabilities/Full. Do not erase arbitrary failure or coverage fields.
        indexing.remove("note");
    }
    share_provenance(object);
}

fn publication_once(object: &mut Map<String, Value>) {
    let token = object
        .get("publication")
        .and_then(|p| p.get("token"))
        .cloned()
        .or_else(|| {
            object
                .get("read")
                .and_then(|r| r.get("current_publication"))
                .and_then(|p| p.get("token"))
                .cloned()
        })
        .or_else(|| {
            object
                .get("read")
                .and_then(|r| r.get("publication"))
                .and_then(|p| p.get("token"))
                .cloned()
        });
    let Some(token) = token else { return };
    object.insert("publication".into(), serde_json::json!({"token":token}));
    if let Some(read) = object.get_mut("read").and_then(Value::as_object_mut) {
        for field in ["publication", "current_publication"] {
            if let Some(envelope) = read.get_mut(field) {
                if envelope.get("token") == Some(&token) {
                    *envelope = serde_json::json!({"ref":"publication"});
                } else if let Some(token) = envelope.get("token").cloned() {
                    *envelope = serde_json::json!({"token":token});
                }
            }
        }
    }
    // Changes retain a separate baseline token; matching endpoints reference the
    // single current token instead of repeating it. Full restores envelopes.
    for field in ["to", "from", "current"] {
        if object.get(field) == Some(&token) {
            object.insert(field.into(), serde_json::json!({"ref":"publication"}));
        }
    }
}

fn share_provenance(object: &mut Map<String, Value>) {
    let mut table = Vec::<Value>::new();
    for key in ["facts", "outline", "relationships", "items"] {
        if let Some(items) = object.get_mut(key).and_then(Value::as_array_mut) {
            for item in items {
                let Some(record) = item.as_object_mut() else {
                    continue;
                };
                if let Some(code) = record.get_mut("code").and_then(Value::as_object_mut) {
                    share_record(code, &mut table);
                }
                share_record(record, &mut table);
            }
        }
    }
    if !table.is_empty() {
        object.insert("provenance".into(), table.into());
    }
}
fn share_record(record: &mut Map<String, Value>, table: &mut Vec<Value>) {
    let descriptor: Map<String, Value> = ["producer", "producer_version", "coverage"]
        .into_iter()
        .filter_map(|key| record.get(key).cloned().map(|value| (key.into(), value)))
        .collect();
    if descriptor.is_empty() {
        return;
    }
    let descriptor = Value::Object(descriptor);
    let index = match table.iter().position(|value| value == &descriptor) {
        Some(index) => index,
        None if table.len() < 64 => {
            table.push(descriptor);
            table.len() - 1
        }
        // Rare heterogeneous pages retain their original provenance when the
        // finite shared table fills. Evidence distinctions must not disappear.
        None => return,
    };
    for key in ["producer", "producer_version", "coverage"] {
        record.remove(key);
    }
    record.insert("provenance".into(), index.into());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_retry_is_an_error_without_fabricated_evidence() {
        let error =
            anyhow::anyhow!("read admission").context(airk_kernel::CurrentGateContention::Applying);
        let result = error_result(&error);
        assert_eq!(result.is_error, Some(true));
        let body: Value = serde_json::from_str(&result.content[0].as_text().unwrap().text).unwrap();
        assert_eq!(body["retryable"], true);
        assert_eq!(body["action"], "repeat_same_request");
        assert_eq!(body["retry_after_ms"], 25);
        assert!(body.get("publication").is_none() && body.get("items").is_none());
        let ordinary = error_result(&anyhow::anyhow!("current revision expired"));
        assert_eq!(
            ordinary.content[0].as_text().unwrap().text,
            "current revision expired"
        );
    }
    fn response() -> Value {
        serde_json::json!({"collection":"one", "publication":{"incarnation":"a","token":"exact-token"},
            "freshness":{"observed_unix_ms":1,"state":"idle","live_source_verified":false},
            "semantic_readiness":{"state":"partial","failure":null,"completed_units":1},
            "facts":[{"name":"call","code":{"basis":"syntax","target":"x.work","producer":"tree-sitter","producer_version":"1","coverage":"candidate"}}]})
    }
    #[test]
    fn default_override_and_status_memory_are_client_local() {
        assert_eq!(ResponseMode::from_setting(None), ResponseMode::Terse);
        assert_eq!(
            ResponseMode::from_setting(Some("unknown")),
            ResponseMode::Terse
        );
        assert_eq!(ResponseMode::from_setting(Some("full")), ResponseMode::Full);
        let first = Session::default();
        let second = Session::default();
        let mut value = response();
        first.display(&mut value, ResponseMode::Terse, false);
        assert!(value.get("freshness").is_some());
        let mut later = response();
        later["freshness"]["observed_unix_ms"] = 999.into();
        first.display(&mut later, ResponseMode::Terse, false);
        assert!(later.get("freshness").is_none());
        assert!(later.get("semantic_readiness").is_none());
        let mut other = response();
        second.display(&mut other, ResponseMode::Terse, false);
        assert!(other.get("freshness").is_some());
        let mut failure = response();
        failure["semantic_readiness"]["failure"] = "new failure".into();
        first.display(&mut failure, ResponseMode::Terse, false);
        assert_eq!(failure["semantic_readiness"]["failure"], "new failure");
        let mut requested = response();
        first.display(&mut requested, ResponseMode::Terse, true);
        assert!(requested.get("freshness").is_some());
        let mut full = response();
        first.display(&mut full, ResponseMode::Full, false);
        assert_eq!(full, response());
    }

    #[test]
    fn reconnect_receipt_is_finite_and_rebuild_or_failure_cannot_reuse_an_old_observation() {
        let original = Session::default();
        let mut first = response();
        original.display(&mut first, ResponseMode::Terse, false);
        let receipt = original.receipt();
        assert!(serde_json::to_vec(&receipt).unwrap().len() <= 1024);
        assert!(
            serde_json::from_value::<StatusReceipt>(serde_json::json!({"unknown":"metadata"}))
                .is_err()
        );
        let renewed = Session::default();
        renewed.restore(receipt);
        let mut same = response();
        renewed.display(&mut same, ResponseMode::Terse, false);
        assert!(same.get("freshness").is_none());
        let mut rebuilt = response();
        rebuilt["publication"]["incarnation"] = "b".into();
        renewed.display(&mut rebuilt, ResponseMode::Terse, false);
        assert!(rebuilt.get("freshness").is_some());
        renewed.restore(receipt);
        let mut failed = response();
        failed["semantic_readiness"]["failure"] = "new failure after renewal".into();
        renewed.display(&mut failed, ResponseMode::Terse, false);
        assert_eq!(
            failed["semantic_readiness"]["failure"],
            "new failure after renewal"
        );
    }

    #[test]
    fn renewed_loss_of_observation_is_visible_after_full_restores_readiness() {
        let session = Session::default();
        let unknown = serde_json::json!({"collection":"one","publication":{"incarnation":"a","token":"exact-token"},"discovery_observation":"unavailable_after_read"});
        let mut initial = unknown.clone();
        session.display(&mut initial, ResponseMode::Terse, false);
        assert!(initial.get("discovery_observation").is_some());
        let mut known = response();
        session.display(&mut known, ResponseMode::Full, false);
        let mut lost = unknown;
        session.display(&mut lost, ResponseMode::Terse, false);
        assert!(lost.get("discovery_observation").is_some());
        let mut restored = response();
        session.display(&mut restored, ResponseMode::Terse, false);
        assert!(restored.get("semantic_readiness").is_some());
    }
    #[test]
    fn shared_provenance_preserves_candidate_and_imported_distinctions() {
        let mut value = response();
        value["facts"].as_array_mut().unwrap().push(serde_json::json!({"code":{"basis":"imported","producer":"scip","producer_version":"2","coverage":"assertion"}}));
        Session::default().display(&mut value, ResponseMode::Terse, false);
        assert_eq!(value["facts"][0]["code"]["basis"], "syntax");
        assert_eq!(value["facts"][0]["code"]["target"], "x.work");
        assert_eq!(value["facts"][1]["code"]["basis"], "imported");
        assert_eq!(value["provenance"][0]["coverage"], "candidate");
        assert_eq!(value["provenance"][1]["producer"], "scip");
    }

    #[test]
    fn conditional_reply_sends_each_exact_token_once_without_changing_validators() {
        let mut value = serde_json::json!({"read":{"validator":"exact-evidence","status":"not_modified",
            "publication":{"token":"current-token","generation":7},
            "current_publication":{"token":"current-token","generation":7}}});
        Session::default().display(&mut value, ResponseMode::Terse, false);
        assert_eq!(
            serde_json::to_string(&value)
                .unwrap()
                .matches("current-token")
                .count(),
            1
        );
        assert_eq!(value["read"]["validator"], "exact-evidence");
        assert_eq!(value["read"]["status"], "not_modified");
        assert_eq!(value["read"]["publication"]["ref"], "publication");
    }

    #[test]
    fn repeated_provenance_is_smaller_and_full_preserves_complete_payload() {
        let mut full = response();
        let item = full["facts"][0].clone();
        full["facts"] = vec![item; 12].into();
        let before = serde_json::to_vec(&full).unwrap().len();
        let mut terse = full.clone();
        Session::default().display(&mut terse, ResponseMode::Terse, false);
        let after = serde_json::to_vec(&terse).unwrap().len();
        assert!(after < before * 3 / 4, "fixture bytes: {before} -> {after}");
        let original = full.clone();
        Session::default().display(&mut full, ResponseMode::Full, false);
        assert_eq!(full, original);
    }
}
