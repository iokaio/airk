// SPDX-License-Identifier: Apache-2.0
//! Inert advice from a bounded set of observations already in a read result.
//! Never discovers capabilities, reads sources, audits coverage or starts work.
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_OBSERVATIONS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Unsupported,
    Unavailable,
    NotRetained,
    ReplyOmitted,
    Pending,
    Failed,
    Stale,
    Excluded,
    Truncated,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NextRead {
    InspectCapabilities,
    InspectFileInfo,
    InspectSourceStatus,
    InspectSavedPolicy,
    RequestRich,
    ContinuePage,
    ReadSelectedPassage,
}

/// Machine-readable vocabulary, without an effective-profile inventory or new input.
pub fn catalog() -> Value {
    serde_json::json!({"version":1,"max_observations":MAX_OBSERVATIONS,
        "basis":"returned_metadata_only","starts_work":false,
        "reasons":[Reason::Unsupported,Reason::Unavailable,Reason::NotRetained,Reason::ReplyOmitted,Reason::Pending,Reason::Failed,Reason::Stale,Reason::Excluded,Reason::Truncated,Reason::Unknown],
        "next_reads":[NextRead::InspectCapabilities,NextRead::InspectFileInfo,NextRead::InspectSourceStatus,NextRead::InspectSavedPolicy,NextRead::RequestRich,NextRead::ContinuePage,NextRead::ReadSelectedPassage]})
}

#[derive(Clone, Debug, Serialize)]
pub struct Observation {
    pub reason: Reason,
    pub subject: &'static str,
    /// Field path in the original full result, not a live-source assertion.
    pub basis: &'static str,
    pub message: &'static str,
    pub next_read: NextRead,
}

#[derive(Default, Serialize)]
struct Guidance {
    version: u32,
    observations: Vec<Observation>,
    truncated: bool,
}
impl Guidance {
    fn add(
        &mut self,
        reason: Reason,
        subject: &'static str,
        basis: &'static str,
        message: &'static str,
        next_read: NextRead,
    ) {
        if self.observations.len() < MAX_OBSERVATIONS {
            self.observations.push(Observation {
                reason,
                subject,
                basis,
                message,
                next_read,
            });
        } else {
            self.truncated = true;
        }
    }
}

/// Pure, constant-roster projection. Array lengths are inspected without walking
/// evidence, files, adapters or a corpus. No user-controlled strings are copied.
/// Results without qualifying observations gain no field. Existing evidence and
/// qualifiers are preserved.
pub fn annotate(value: &mut Value) {
    // Plans retain native advice on each step and their own completeness fields.
    // Do not derive a second verdict from an aggregate or inspect its children.
    if !value.is_object()
        || value.get("schema").and_then(Value::as_str) == Some(crate::workbench::RESULT_SCHEMA)
    {
        return;
    }
    let mut advice = Guidance {
        version: 1,
        ..Default::default()
    };
    let source = value
        .get("evidence")
        .filter(|v| v.is_object())
        .unwrap_or(value);
    let get = |key: &str| value.get(key).or_else(|| source.get(key));
    if get("source_availability")
        .and_then(|v| v.get("state"))
        .and_then(Value::as_str)
        == Some("unavailable")
    {
        advice.add(Reason::Unavailable,"source_access","source_availability.state","The saved source root is unavailable. Retained evidence may remain readable; inspect status and the existing availability reason before requesting source work.",NextRead::InspectSourceStatus);
    }
    if let Some(execution) = get("execution")
        && execution.get("version").and_then(Value::as_u64) == Some(1)
        && execution.get("incomplete") == Some(&Value::Bool(true))
        && matches!(
            execution.get("requested_mode").and_then(Value::as_str),
            Some("default" | "lexical" | "vector" | "hybrid")
        )
        && matches!(
            execution.get("served_mode").and_then(Value::as_str),
            Some("lexical" | "vector" | "hybrid")
        )
    {
        match execution.get("incomplete_reason").and_then(Value::as_str) {
            Some("helper_unavailable") => advice.add(Reason::Unavailable,"semantic_execution","execution.incomplete_reason","This search reports helper unavailability and incomplete semantic retrieval. Keep its served mode and indexed publication. Saved-source checks apply to returned file references and do not establish semantic completeness. Review recovery before an explicit later read; do not replay an uncertain operation.",NextRead::InspectSourceStatus),
            Some("semantic_coverage_incomplete") => advice.add(if get("semantic_readiness").and_then(|value| value.get("state")).and_then(Value::as_str)==Some("failed"){Reason::Failed}else{Reason::Pending},"semantic_execution","execution.incomplete_reason","Semantic coverage for this search is incomplete. Keep the served mode and indexed publication with the evidence. Saved-source checks apply to returned file references and do not establish semantic completeness.",NextRead::InspectSourceStatus),
            Some("semantic_upgrade_pending") => advice.add(Reason::Pending,"semantic_execution","execution.incomplete_reason","A semantic index upgrade was pending for this search. Keep its served mode and publication; saved-source matches do not establish complete semantic retrieval. This is not an observation of helper health.",NextRead::InspectSourceStatus),
            _ => {}
        }
    } else if get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        == Some("helper_unavailable")
    {
        advice.add(Reason::Unavailable,"semantic_execution","error.code","This search returned a helper-unavailable refusal. Review recovery before an explicit later read; do not replay an uncertain operation. Saved-source checks do not establish semantic completeness.",NextRead::InspectSourceStatus);
    }
    if get("indexing")
        .and_then(|v| v.get("level"))
        .and_then(Value::as_str)
        == Some("essential")
    {
        advice.add(Reason::NotRetained,"native_facts","indexing.level","Essential does not retain native facts or relationships. Explicit imported analysis is separate; Rich cannot recreate omitted native evidence.",NextRead::InspectSavedPolicy);
    }
    if get("representation").and_then(Value::as_str) == Some("minimal")
        || get("minimal").is_some_and(Value::is_object)
    {
        advice.add(Reason::ReplyOmitted,"reply_projection","minimal/representation","Minimal navigation omits durable symbol IDs and some details. Restart a selected discovery with reply_detail rich before an ID-based follow-up.",NextRead::RequestRich);
    }
    if let Some(adapters) = get("adapters").and_then(Value::as_array)
        && adapters.len() == 1
        && let Some(capabilities) = adapters[0].get("capabilities")
    {
        const FAMILIES: [&str; 9] = [
            "lexical",
            "definitions",
            "imports",
            "call_candidates",
            "structural_facts",
            "embedded_regions",
            "native_locations",
            "continuity",
            "producer_assertions",
        ];
        if FAMILIES
            .iter()
            .any(|key| capabilities.get(*key) == Some(&Value::Bool(false)))
        {
            advice.add(Reason::Unsupported,"selected_adapter","adapters[0].capabilities","The selected adapter declares unsupported families. Inspect its false capability flags; this does not classify other adapters or files.",NextRead::InspectCapabilities);
        }
    }
    if let Some(selection) = get("vector_selection")
        && selection.get("eligible") == Some(&Value::Bool(false))
        && selection.get("excluded_by").is_some_and(Value::is_string)
    {
        advice.add(Reason::Excluded,"file_vectors","vector_selection","Saved selection policy excludes this file from vectors. Lexical/text evidence remains independent; inspect the existing selection decision.",NextRead::InspectFileInfo);
    }
    for (key, subject) in [
        ("discovery_readiness", "discovery"),
        ("semantic_readiness", "semantic"),
        ("freshness", "source_work"),
    ] {
        let state = get(key)
            .and_then(|v| v.get("state"))
            .and_then(Value::as_str);
        let basis = match key {
            "discovery_readiness" => "discovery_readiness.state",
            "semantic_readiness" => "semantic_readiness.state",
            _ => "freshness.state",
        };
        match state {
            Some("pending"|"partial"|"running"|"paused"|"blocked")=>advice.add(Reason::Pending,subject,basis,"This observed work is incomplete or paused. Retained evidence may still be readable. Inspect status once when needed; this advice starts no work.",NextRead::InspectSourceStatus),
            Some("failed")=>advice.add(Reason::Failed,subject,basis,"This observed work failed. Retain the original failure and publication; inspect status and review an explicit retry rather than replaying an uncertain operation.",NextRead::InspectSourceStatus),
            _=>{}
        }
    }
    if let Some(freshness) = get("freshness")
        && let (Some(requested), Some(completed)) = (
            freshness.get("requested_watermark").and_then(Value::as_u64),
            freshness.get("completed_watermark").and_then(Value::as_u64),
        )
        && requested > completed
    {
        advice.add(Reason::Stale,"source_work_watermark","freshness.requested_watermark/completed_watermark","Observed source work has not caught up. This is a work-watermark observation, not proof that any particular cited file differs on disk.",NextRead::InspectSourceStatus);
    }
    let page = get("page");
    let cursor = [get("cursor"), page.and_then(|v| v.get("cursor"))]
        .into_iter()
        .flatten()
        .any(|v| v.as_str().is_some_and(|v| !v.is_empty()));
    if get("truncated") == Some(&Value::Bool(true))
        || cursor
        || page.and_then(|v| v.get("more")) == Some(&Value::Bool(true))
    {
        advice.add(Reason::Truncated,"returned_evidence","truncated/cursor/page.more",if cursor {"A continuation is available; further pages may contain no matches. Repeat the same tool, scope, filters, publication and representation with its cursor; honor required_record_bytes."} else {"The reply is limited. Where a citation is available, select it for a bounded passage read; otherwise review the reported limits explicitly. No complete absence follows from this reply."},if cursor {NextRead::ContinuePage}else{NextRead::ReadSelectedPassage});
    }
    // Empty primary arrays do not rule out returned supplemental pointers.
    // In particular they cannot establish unsupported, excluded or not retained.
    let arrays = [
        "hits",
        "items",
        "facts",
        "outline",
        "paths",
        "relationships",
    ];
    let mut has_records = false;
    let mut has_array = false;
    for key in arrays {
        if let Some(rows) = get(key).and_then(Value::as_array) {
            has_array = true;
            has_records |= !rows.is_empty();
        }
    }
    if let Some(rows) = get("minimal")
        .and_then(|v| v.get("items"))
        .and_then(Value::as_array)
    {
        has_array = true;
        has_records |= !rows.is_empty();
    }
    if has_array && !has_records {
        advice.add(Reason::Unknown,"empty_saved_query","primary record arrays","Primary result rows were empty; supplemental pointers and limits may still be present. Empty matches alone prove no cause or source absence. Inspect selected file metadata and capabilities before widening scope.",NextRead::InspectFileInfo);
    }
    if !advice.observations.is_empty()
        && let Ok(advice) = serde_json::to_value(advice)
        && let Some(fields) = value.as_object_mut()
    {
        fields.insert("guidance".into(), advice);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn reasons_require_explicit_observations_and_never_infer_why_zero_hits() {
        for (mut value, reason) in [
            (
                json!({"adapters":[{"capabilities":{"definitions":false}}]}),
                "unsupported",
            ),
            (json!({"indexing":{"level":"essential"}}), "not_retained"),
            (
                json!({"representation":"minimal","items":[{}]}),
                "reply_omitted",
            ),
            (json!({"semantic_readiness":{"state":"partial"}}), "pending"),
            (json!({"discovery_readiness":{"state":"failed"}}), "failed"),
            (
                json!({"freshness":{"requested_watermark":3,"completed_watermark":2}}),
                "stale",
            ),
            (
                json!({"vector_selection":{"eligible":false,"excluded_by":"rule"}}),
                "excluded",
            ),
            (json!({"truncated":true}), "truncated"),
            (json!({"hits":[]}), "unknown"),
            (
                json!({"hits":[],"definitions":[{"name":"Supplemental","matches":[{"symbol_id":"retained-pointer"}]}]}),
                "unknown",
            ),
        ] {
            let before = value.clone();
            annotate(&mut value);
            assert!(
                value["guidance"]["observations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v["reason"] == reason)
            );
            value.as_object_mut().unwrap().remove("guidance");
            assert_eq!(value, before);
        }
        let mut empty = json!({"hits":[],"freshness":{"live_source_verified":false}});
        annotate(&mut empty);
        assert_eq!(
            empty["guidance"]["observations"].as_array().unwrap().len(),
            1
        );
        assert_eq!(empty["guidance"]["observations"][0]["reason"], "unknown");
        let mut healthy = json!({"hits":[{}],"freshness":{"state":"idle","live_source_verified":false},"semantic_readiness":{"state":"ready"}});
        let before = healthy.clone();
        annotate(&mut healthy);
        assert_eq!(healthy, before);
        let mut compact = json!({"facts":[],"outline":[{}]});
        annotate(&mut compact);
        assert!(compact.get("guidance").is_none());
    }
    #[test]
    fn advice_is_bounded_inert_and_keeps_original_continuation_qualifiers() {
        let mut unavailable =
            json!({"source_availability":{"state":"unavailable","reason":"missing"}});
        annotate(&mut unavailable);
        assert_eq!(
            unavailable["guidance"]["observations"][0]["reason"],
            "unavailable"
        );
        let mut aggregate =
            json!({"schema":crate::workbench::RESULT_SCHEMA,"truncated":true,"steps":[]});
        let original = aggregate.clone();
        annotate(&mut aggregate);
        assert_eq!(aggregate, original);
        let mut value = json!({"indexing":{"level":"essential"},"representation":"minimal","items":[],"adapters":[{"capabilities":{"lexical":false}}],"vector_selection":{"eligible":false,"excluded_by":"secret-rule"},"discovery_readiness":{"state":"partial"},"semantic_readiness":{"state":"failed"},"freshness":{"state":"pending","requested_watermark":2,"completed_watermark":1},"truncated":true,"cursor":"exact-cursor"});
        annotate(&mut value);
        let advice = &value["guidance"];
        assert_eq!(
            advice["observations"].as_array().unwrap().len(),
            MAX_OBSERVATIONS
        );
        assert_eq!(advice["truncated"], true);
        assert!(serde_json::to_vec(advice).unwrap().len() < 4096);
        assert!(!advice.to_string().contains("secret-rule"));
        assert_eq!(value["cursor"], "exact-cursor");
        let mut paged =
            json!({"page":{"cursor":"c","required_record_bytes":4096},"truncated":true});
        annotate(&mut paged);
        assert_eq!(
            paged["guidance"]["observations"][0]["next_read"],
            "continue_page"
        );
        assert_eq!(paged["page"]["required_record_bytes"], 4096);
    }

    #[test]
    fn execution_advice_is_distinct_from_index_readiness_and_source_verification() {
        for (reason, state, expected) in [
            ("helper_unavailable", "ready", "unavailable"),
            ("semantic_coverage_incomplete", "partial", "pending"),
            ("semantic_coverage_incomplete", "failed", "failed"),
            ("semantic_upgrade_pending", "pending", "pending"),
        ] {
            let mut value = json!({"publication":{"token":"retained-publication"},"hits":[{}],
                "execution":{"version":1,"requested_mode":"default","served_mode":"lexical","incomplete":true,"incomplete_reason":reason},
                "semantic_readiness":{"state":state},
                "source_verification":{"status":"verified","observed_to_unix_ms":1000}});
            let before = value.clone();
            annotate(&mut value);
            let execution = value["guidance"]["observations"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["subject"] == "semantic_execution")
                .unwrap();
            assert_eq!(execution["reason"], expected);
            assert_eq!(execution["basis"], "execution.incomplete_reason");
            let message = execution["message"].as_str().unwrap();
            assert!(message.contains("semantic"));
            assert!(message.contains("source"));
            assert!(message.len() <= 512);
            value.as_object_mut().unwrap().remove("guidance");
            assert_eq!(value, before, "advice cannot rewrite the returned evidence");
        }
        let mut strict = json!({"error":{"code":"helper_unavailable","retryable":false,"action":"retry_later_without_replay"},
            "requested_mode":"vector","semantic_readiness":{"state":"ready"}});
        let before = strict.clone();
        annotate(&mut strict);
        assert_eq!(
            strict["guidance"]["observations"][0]["reason"],
            "unavailable"
        );
        assert_eq!(strict["guidance"]["observations"][0]["basis"], "error.code");
        assert!(
            strict["guidance"]["observations"][0]["message"]
                .as_str()
                .unwrap()
                .contains("do not replay")
        );
        strict.as_object_mut().unwrap().remove("guidance");
        assert_eq!(strict, before);
    }

    #[test]
    fn execution_guidance_never_infers_helper_health_from_unknown_or_complete_metadata() {
        for execution in [
            json!({"version":1,"requested_mode":"lexical","served_mode":"lexical","incomplete":false}),
            json!({"version":1,"requested_mode":"default","served_mode":"lexical","incomplete":false}),
            json!({"version":2,"requested_mode":"default","served_mode":"lexical","incomplete":true,"incomplete_reason":"helper_unavailable"}),
            json!({"version":1,"requested_mode":"future","served_mode":"lexical","incomplete":true,"incomplete_reason":"helper_unavailable"}),
            json!({"version":1,"requested_mode":"default","served_mode":"lexical","incomplete":true,"incomplete_reason":"future-reason"}),
        ] {
            let mut value =
                json!({"hits":[{}],"semantic_readiness":{"state":"ready"},"execution":execution});
            let before = value.clone();
            annotate(&mut value);
            assert_eq!(
                value, before,
                "unknown execution metadata is not a new verdict"
            );
        }
        let mut absent = json!({"hits":[{}],"semantic_readiness":{"state":"ready"}});
        let before = absent.clone();
        annotate(&mut absent);
        assert_eq!(absent, before);
    }
}
