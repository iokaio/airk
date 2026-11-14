// SPDX-License-Identifier: Apache-2.0
//! Explicit replay metadata for one failed required read member only.
use super::{
    COMBINED_READ_SCHEMA, MemberAllocation, MemberAvailability, PublicationVector,
    SelectedScopeObservation,
};
use crate::{
    identity::Sha256Digest,
    index_catalog::{IndexId, MAX_SELECTION, QueryProjectId, encoded_size, unique_bounded},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SingleRequiredMemberRetry {
    pub schema: u32,
    pub scope: String,
    pub index_id: IndexId,
    pub selection_identity: Sha256Digest,
    pub action: String,
    pub query_replay_safe: bool,
    /// Exact pointer into the complete, unmodified original native failure.
    pub result_pointer: String,
    pub code: String,
    pub retry_after_ms: u64,
    pub fresh_brokerage_required: bool,
}

const STORAGE_HINT_FIELDS: &[&str] = &[
    "status",
    "code",
    "retryable",
    "action",
    "retry_after_ms",
    "request_id",
    "scope_observation",
    "query_replay_safe",
    "query_submitted",
    "fresh_brokerage_required",
];
const DIRECT_HINT_FIELDS: &[&str] = &[
    "error",
    "request_id",
    "scope_observation",
    "query_replay_safe",
    "query_submitted",
    "fresh_brokerage_required",
];
const DIRECT_ERROR_FIELDS: &[&str] = &[
    "code",
    "reason",
    "query_submitted",
    "retryable",
    "retry_after_ms",
    "fresh_brokerage_required",
    "query_replay_safe",
];

fn only_fields(value: &Value, fields: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.keys().all(|key| fields.contains(&key.as_str())))
}

fn framing_agrees(
    body: &Value,
    selection: &SelectedScopeObservation,
    index_id: IndexId,
) -> Option<()> {
    if let Some(request_id) = body.get("request_id") {
        let request_id = request_id.as_str()?;
        if request_id.is_empty() || request_id.len() > 128 {
            return None;
        }
    }
    if let Some(observation) = body.get("scope_observation") {
        let observation: crate::scoped_reads::ScopeObservation =
            serde_json::from_value(observation.clone()).ok()?;
        observation.validate().ok()?;
        // Hand-written source agreement: a catalog observation matches only the
        // same grant and revision; a selection observation matches only the
        // same selection. A catalog observation never agrees with a selection.
        let same_source = match (
            &observation.grant,
            &observation.selection,
            selection.grant_id,
            selection.grant_revision,
            &selection.selection,
        ) {
            (Some(grant), None, Some(id), Some(revision), None) => {
                grant.id == id && grant.revision == revision
            }
            (None, Some(observed), None, None, Some(selected)) => observed == selected,
            _ => false,
        };
        if !same_source
            || observation.members.len() != 1
            || observation.members[0].index_id != index_id
            || observation.members[0]
                .memberships
                .iter()
                .any(|membership| !selection.project_ids.contains(&membership.project_id))
        {
            return None;
        }
    }
    Some(())
}

/// Derive only from the complete canonical composition. This is not a scan for
/// arbitrary nested `retryable` flags and cannot authorize a partial union replay.
pub fn proposed_retry(combined: &Value) -> Option<SingleRequiredMemberRetry> {
    encoded_size(combined, crate::MAX_RESPONSE_BYTES).ok()?;
    if combined.get("schema")?.as_str()? != COMBINED_READ_SCHEMA
        || !combined.get("partial")?.as_bool()?
        || combined.get("truncated")?.as_bool()?
        || !combined.get("ranking")?.as_array()?.is_empty()
        || combined
            .get("shared_values")
            .is_some_and(|value| !value.is_null())
    {
        return None;
    }
    let selection: SelectedScopeObservation =
        serde_json::from_value(combined.get("selection")?.clone()).ok()?;
    if selection.member_availability != MemberAvailability::Require
        || !selection.has_one_source()
        || selection.index_ids.len() != 1
        || selection.project_ids.is_empty()
    {
        return None;
    }
    unique_bounded(&selection.project_ids, MAX_SELECTION).ok()?;
    let index_id = selection.index_ids[0];
    let members = combined.get("members")?.as_array()?;
    if members.len() != 1 {
        return None;
    }
    let member = &members[0];
    if member.get("index_id")?.as_str()? != index_id.to_string()
        || member.get("outcome")?.as_str()? != "failed"
        || !member.get("is_error")?.as_bool()?
        || member.get("cutoff").is_some_and(|value| !value.is_null())
    {
        return None;
    }
    let allocation: MemberAllocation =
        serde_json::from_value(member.get("allocation")?.clone()).ok()?;
    if allocation.index_id != index_id
        || !(1..=100).contains(&allocation.limit)
        || !(1..=65_536).contains(&allocation.max_bytes)
        || !(1..=30_000).contains(&allocation.deadline_ms)
        || allocation.verification_max_sources > crate::read_limits::MAX_SOURCES
        || allocation.verification_max_bytes > crate::read_limits::MAX_BYTES
        || allocation.verification_deadline_ms > crate::read_limits::MAX_DEADLINE_MS
    {
        return None;
    }
    let failure = combined.get("error")?;
    let failed = failure.get("failed_members")?.as_array()?;
    if failure.get("code")?.as_str()? != "member_unavailable"
        || failure.get("retryable")?.as_bool()?
        || failed.len() != 1
        || failed[0].as_str()? != index_id.to_string()
    {
        return None;
    }
    let publications: PublicationVector =
        serde_json::from_value(combined.get("publication_vector")?.clone()).ok()?;
    publications.validate().ok()?;
    if publications.members.len() != 1
        || publications.members[0].index_id != index_id
        || publications.members[0].publication.is_some()
    {
        return None;
    }
    let result = member.get("result")?;
    if !result.is_object() || result.get("isError").is_some_and(|value| value != true) {
        return None;
    }
    let (body, suffix) = if let Some(body) = result.get("structuredContent") {
        if !result.get("isError")?.as_bool()? {
            return None;
        }
        let content = result.get("content")?.as_array()?;
        if content.len() != 1 || content[0].get("type")?.as_str()? != "text" {
            return None;
        }
        let text: Value = serde_json::from_str(content[0].get("text")?.as_str()?).ok()?;
        if &text != body {
            return None;
        }
        (body, "/structuredContent")
    } else {
        if result.get("content").is_some() {
            return None;
        }
        (result, "")
    };
    if !body.is_object()
        || body
            .get("query_replay_safe")
            .is_some_and(|value| value != true)
        || body
            .get("query_submitted")
            .is_some_and(|value| value != false)
    {
        return None;
    }
    framing_agrees(body, &selection, index_id)?;
    let (code, delay, fresh) = if body.get("status").and_then(Value::as_str) == Some("retry") {
        let code = body.get("code")?.as_str()?;
        if !only_fields(body, STORAGE_HINT_FIELDS)
            || !matches!(code, "publication_busy" | "selection_busy")
            || !body.get("retryable")?.as_bool()?
            || body.get("action")?.as_str()? != "repeat_same_request"
            || body.get("error").is_some_and(|value| !value.is_null())
            || body
                .get("fresh_brokerage_required")
                .is_some_and(|value| value != false)
        {
            return None;
        }
        (code, body.get("retry_after_ms")?.as_u64()?, false)
    } else {
        if !only_fields(body, DIRECT_HINT_FIELDS) {
            return None;
        }
        let error = body.get("error")?;
        if !only_fields(error, DIRECT_ERROR_FIELDS) {
            return None;
        }
        let code = error.get("code")?.as_str()?;
        let fresh = match code {
            "direct_query_admission" => false,
            "direct_query_setup" => true,
            _ => return None,
        };
        if error.get("reason")?.as_str()? != "busy"
            || error.get("query_submitted")?.as_bool()?
            || !error.get("retryable")?.as_bool()?
            || error.get("fresh_brokerage_required")?.as_bool()? != fresh
            || error
                .get("query_replay_safe")
                .is_some_and(|value| value != true)
            || body
                .get("fresh_brokerage_required")
                .is_some_and(|value| value != fresh)
        {
            return None;
        }
        (code, error.get("retry_after_ms")?.as_u64()?, fresh)
    };
    if !(1..=1000).contains(&delay) {
        return None;
    }
    Some(SingleRequiredMemberRetry {
        schema: 1,
        scope: "same_single_required_member".into(),
        index_id,
        selection_identity: selection.identity,
        action: "repeat_same_request".into(),
        query_replay_safe: true,
        result_pointer: format!("/members/0/result{suffix}"),
        code: code.into(),
        retry_after_ms: delay,
        fresh_brokerage_required: fresh,
    })
}

/// Validate canonical metadata against its exact child. A caller repeating work
/// must also bind it to its original authority and arguments with the helpers
/// below; a self-consistent older receipt is not current caller authority.
pub fn validated_retry(combined: &Value) -> Option<SingleRequiredMemberRetry> {
    let declared: SingleRequiredMemberRetry =
        serde_json::from_value(combined.get("read_retry")?.clone()).ok()?;
    (declared == proposed_retry(combined)?).then_some(declared)
}

fn query_binding(
    selected: &SelectedScopeObservation,
    projects: &[QueryProjectId],
    indexes: &[IndexId],
    arguments: &serde_json::Map<String, Value>,
) -> Option<()> {
    encoded_size(arguments, crate::MAX_RESPONSE_BYTES).ok()?;
    unique_bounded(projects, MAX_SELECTION).ok()?;
    unique_bounded(indexes, MAX_SELECTION).ok()?;
    if projects.is_empty() || indexes.is_empty() || arguments.contains_key("project") {
        return None;
    }
    let explicit_projects = arguments.contains_key("project_ids");
    if !explicit_projects
        && (arguments.contains_key("index_ids") || arguments.contains_key("member_availability"))
    {
        return None;
    }
    let projects_requested: Vec<QueryProjectId> = arguments
        .get("project_ids")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()
        .ok()?
        .unwrap_or_else(|| projects.to_vec());
    let indexes_requested: Option<Vec<IndexId>> = arguments
        .get("index_ids")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()
        .ok()?
        .or_else(|| (!explicit_projects).then(|| indexes.to_vec()));
    let availability: MemberAvailability = arguments
        .get("member_availability")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()
        .ok()?
        .unwrap_or_default();
    let request = super::QuerySelection {
        project_ids: projects_requested,
        index_ids: indexes_requested,
        member_availability: availability,
    };
    request.validate().ok()?;
    if request.member_availability != MemberAvailability::Require
        || !request.project_ids.iter().all(|id| projects.contains(id))
        || !selected
            .project_ids
            .iter()
            .all(|id| request.project_ids.contains(id))
        || !selected.index_ids.iter().all(|id| indexes.contains(id))
    {
        return None;
    }
    if let Some(requested) = request.index_ids
        && (!requested.iter().all(|id| indexes.contains(id))
            || requested.len() != selected.index_ids.len()
            || !requested.iter().all(|id| selected.index_ids.contains(id)))
    {
        return None;
    }
    // With explicit projects and no indexes the native catalog resolves their
    // memberships. Only the known permitted subset can be checked here; never
    // fabricate that expansion or unpruned project memberships in the caller.
    Some(())
}

/// Bind a W4/A2 result to its original immutable public selection and request.
/// Namespace is bound by the captured scope identity; public replies never gain
/// the private nonce or service instance merely to validate a retry.
pub fn validated_retry_for_selection(
    combined: &Value,
    expected: &crate::service::WindowScopeSelection,
    source: &crate::scoped_reads::ScopeSelectionObservation,
    arguments: &serde_json::Map<String, Value>,
) -> Option<SingleRequiredMemberRetry> {
    let retry = validated_retry(combined)?;
    let selected: SelectedScopeObservation =
        serde_json::from_value(combined.get("selection")?.clone()).ok()?;
    if source.catalog_revision != expected.expected_catalog_revision
        || selected.selection.as_ref() != Some(source)
        || selected.grant_id.is_some()
        || selected.grant_revision.is_some()
    {
        return None;
    }
    query_binding(
        &selected,
        &expected.project_ids,
        &expected.index_ids,
        arguments,
    )?;
    Some(retry)
}

/// A single-index catalog caller knows the exact grant, index and permitted
/// projects. The response has no metadata clock to advance; grant revision is
/// always exact. The separate setup-envelope clock-forward rule is unchanged.
pub fn validated_retry_for_catalog(
    combined: &Value,
    expected: &crate::service::ScopedConnectRequest,
    arguments: &serde_json::Map<String, Value>,
) -> Option<SingleRequiredMemberRetry> {
    expected.validate().ok()?;
    let retry = validated_retry(combined)?;
    let selected: SelectedScopeObservation =
        serde_json::from_value(combined.get("selection")?.clone()).ok()?;
    if selected.selection.is_some()
        || selected.grant_id != Some(expected.grant_id)
        || selected.grant_revision != Some(expected.expected_grant_revision)
    {
        return None;
    }
    query_binding(
        &selected,
        &expected.project_ids,
        &[expected.index_id],
        arguments,
    )?;
    Some(retry)
}

/// A catalog union can bind only indexes already known to its caller. An
/// unexpanded grant with absent index IDs conservatively retains the refusal;
/// this helper does not resolve a catalog or infer memberships from a receipt.
pub fn validated_retry_for_combined_catalog(
    combined: &Value,
    expected: &crate::service::CombinedConnectRequest,
    arguments: &serde_json::Map<String, Value>,
) -> Option<SingleRequiredMemberRetry> {
    expected.validate().ok()?;
    let retry = validated_retry(combined)?;
    let selected: SelectedScopeObservation =
        serde_json::from_value(combined.get("selection")?.clone()).ok()?;
    if selected.selection.is_some()
        || selected.grant_id != Some(expected.grant_id)
        || selected.grant_revision != Some(expected.expected_grant_revision)
    {
        return None;
    }
    query_binding(
        &selected,
        &expected.project_ids,
        expected.index_ids.as_deref()?,
        arguments,
    )?;
    Some(retry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn combined(hint: Value) -> Value {
        let index = format!("idx_{}", "01".repeat(16));
        json!({"schema":COMBINED_READ_SCHEMA,"selection":{"project_ids":[format!("prj_{}","02".repeat(16))],
            "index_ids":[index],"grant_id":format!("grt_{}","03".repeat(16)),"grant_revision":1,
            "member_availability":"require","identity":"04".repeat(32)},
            "publication_vector":{"authority":"05".repeat(32),"observed_from_unix_ms":1,"observed_to_unix_ms":2,
                "members":[{"index_id":index,"publication":null}]},
            "members":[{"index_id":index,"is_error":true,"outcome":"failed",
                "allocation":{"index_id":index,"limit":1,"max_bytes":4096,"deadline_ms":2000,
                    "verification_max_sources":1,"verification_max_bytes":4096,"verification_deadline_ms":2000},
                "result":{"isError":true,"content":[{"type":"text","text":hint.to_string()}],"structuredContent":hint}}],
            "ranking":[],"partial":true,"truncated":false,
            "error":{"code":"member_unavailable","retryable":false,"failed_members":[index]}})
    }
    fn hint() -> Value {
        json!({"status":"retry","code":"selection_busy","retryable":true,"action":"repeat_same_request","retry_after_ms":25})
    }

    #[test]
    fn only_exact_single_required_member_metadata_authorizes_same_read_retry() {
        let mut value = combined(hint());
        assert!(
            validated_retry(&value).is_none(),
            "an inner retry alone is not an outer replay instruction"
        );
        let proposed = proposed_retry(&value).unwrap();
        assert_eq!(
            proposed.result_pointer,
            "/members/0/result/structuredContent"
        );
        value["read_retry"] = serde_json::to_value(&proposed).unwrap();
        assert_eq!(validated_retry(&value), Some(proposed));
        for (pointer, replacement) in [
            ("/selection/member_availability", json!("report")),
            ("/selection/identity", json!("09".repeat(32))),
            (
                "/selection/index_ids",
                json!([
                    format!("idx_{}", "01".repeat(16)),
                    format!("idx_{}", "09".repeat(16))
                ]),
            ),
            (
                "/members/0/index_id",
                json!(format!("idx_{}", "09".repeat(16))),
            ),
            ("/members/0/is_error", json!(false)),
            ("/members/0/outcome", json!("complete")),
            ("/members/0/allocation/limit", json!(0)),
            ("/members/0/result/content/0/text", json!("different")),
            ("/partial", json!(false)),
            ("/truncated", json!(true)),
            (
                "/ranking",
                json!([{"result_pointer":"/members/0/result/hits/0"}]),
            ),
            (
                "/publication_vector/members/0/publication",
                json!({"token":"published"}),
            ),
            ("/error/retryable", json!(true)),
            (
                "/read_retry/result_pointer",
                json!("/members/0/result/error"),
            ),
            ("/read_retry/retry_after_ms", json!(1)),
            ("/read_retry/query_replay_safe", json!(false)),
        ] {
            let mut invalid = value.clone();
            *invalid.pointer_mut(pointer).unwrap() = replacement;
            assert!(validated_retry(&invalid).is_none(), "{pointer}");
        }
        let mut multiple = value.clone();
        multiple["members"]
            .as_array_mut()
            .unwrap()
            .push(value["members"][0].clone());
        assert!(validated_retry(&multiple).is_none());
        for hint in [
            json!({"status":"retry","code":"unknown_busy","retryable":true,"action":"repeat_same_request","retry_after_ms":25}),
            json!({"status":"retry","code":"selection_busy","retryable":true,"action":"repeat_same_request","retry_after_ms":1001}),
            json!({"error":{"code":"unavailable","retryable":true,"nested":hint()}}),
            json!({"error":{"code":"direct_query_setup","reason":"busy","query_submitted":true,"retryable":true,"fresh_brokerage_required":true,"retry_after_ms":25}}),
        ] {
            assert!(proposed_retry(&combined(hint)).is_none());
        }
        let mut evidence = combined(hint());
        evidence["members"][0]["result"]["structuredContent"]["hits"] = json!([]);
        evidence["members"][0]["result"]["content"][0]["text"] =
            json!(evidence["members"][0]["result"]["structuredContent"].to_string());
        assert!(proposed_retry(&evidence).is_none());
        for (field, value) in [
            ("future_detail", json!({"preserved":true})),
            ("index_state", json!("available")),
            ("chunks", json!(1)),
            ("symbols", json!([])),
            ("meta", json!({"evidence":"partial"})),
        ] {
            let mut mixed = hint();
            mixed[field] = value;
            let original = combined(mixed);
            assert!(proposed_retry(&original).is_none(), "{field}");
            assert!(
                original["members"][0]["result"]["structuredContent"]
                    .get(field)
                    .is_some(),
                "unknown data must remain intact"
            );
        }
        let mut framed = hint();
        framed["request_id"] = json!("original-request");
        framed["scope_observation"] = json!({"schema":1,"qualifier":"selected_subset","fingerprint":"07".repeat(32),
            "provenance":"user_reviewed_project_grant","grant":{"id":format!("grt_{}","03".repeat(16)),"revision":1},
            "members":[{"index_id":format!("idx_{}","01".repeat(16)),
                "memberships":[{"project_id":format!("prj_{}","02".repeat(16)),"membership_revision":1}],
                "source_root_ids":[format!("src_{}","06".repeat(16))],"source_revision":1,"policy_epoch":0,"policy_digest":null}]});
        assert!(proposed_retry(&combined(framed.clone())).is_some());
        framed["scope_observation"]["grant"]["revision"] = json!(2);
        assert!(proposed_retry(&combined(framed.clone())).is_none());
        framed["scope_observation"]["grant"]["revision"] = json!(1);
        framed["scope_observation"]["future_evidence"] = json!({"symbols":[]});
        assert!(proposed_retry(&combined(framed)).is_none());
        for (field, value) in [
            ("query_replay_safe", json!(false)),
            ("query_submitted", json!(true)),
            ("fresh_brokerage_required", json!(true)),
        ] {
            let mut unsafe_hint = hint();
            unsafe_hint[field] = value;
            assert!(proposed_retry(&combined(unsafe_hint)).is_none(), "{field}");
        }
        for (code, fresh) in [
            ("direct_query_admission", false),
            ("direct_query_setup", true),
        ] {
            let value = combined(
                json!({"error":{"code":code,"reason":"busy","query_submitted":false,
                "retryable":true,"fresh_brokerage_required":fresh,"retry_after_ms":50}}),
            );
            assert_eq!(
                proposed_retry(&value).unwrap().fresh_brokerage_required,
                fresh
            );
            for (pointer, flag) in [
                ("/error/query_replay_safe", json!(false)),
                ("/status", json!("complete")),
                ("/error/future_detail", json!({"preserved":true})),
            ] {
                let mut invalid = value.clone();
                let body = &mut invalid["members"][0]["result"]["structuredContent"];
                if pointer == "/status" {
                    body["status"] = flag;
                } else if pointer == "/error/query_replay_safe" {
                    body["error"]["query_replay_safe"] = flag;
                } else {
                    body["error"]["future_detail"] = flag;
                }
                invalid["members"][0]["result"]["content"][0]["text"] =
                    json!(invalid["members"][0]["result"]["structuredContent"].to_string());
                assert!(proposed_retry(&invalid).is_none(), "{pointer}");
            }
        }
    }

    /// Hand-reviewed selection-source cases (selection authority spec §5,
    /// F10). The catalog-grant cases above are unchanged.
    #[test]
    fn selection_sourced_retry_agrees_only_with_the_same_selection() {
        let selected_source =
            json!({"owner":"window","scope_identity":"0c".repeat(32),"catalog_revision":5});
        let catalog_grant = json!({"id":format!("grt_{}","03".repeat(16)),"revision":1});
        let observed = |source: (&str, Value)| {
            let mut value = json!({"schema":1,"qualifier":"selected_subset","fingerprint":"07".repeat(32),
                "members":[{"index_id":format!("idx_{}","01".repeat(16)),
                    "memberships":[{"project_id":format!("prj_{}","02".repeat(16)),"membership_revision":1}],
                    "source_root_ids":[format!("src_{}","06".repeat(16))],"source_revision":1,"policy_epoch":0,"policy_digest":null}]});
            if source.0 == "grant" {
                value["provenance"] = json!("user_reviewed_project_grant");
            }
            value[source.0] = source.1;
            value
        };
        let with_observation = |observation: Option<Value>| {
            let mut framed = hint();
            if let Some(observation) = observation {
                framed["scope_observation"] = observation;
            }
            framed
        };
        let selection_sourced = |framed: Value| {
            let mut value = combined(framed);
            let selection = value["selection"].as_object_mut().unwrap();
            selection.remove("grant_id");
            selection.remove("grant_revision");
            selection.insert("selection".into(), selected_source.clone());
            value
        };
        assert!(proposed_retry(&selection_sourced(with_observation(None))).is_some());
        let same = observed(("selection", selected_source.clone()));
        let proposed =
            proposed_retry(&selection_sourced(with_observation(Some(same.clone())))).unwrap();
        assert_eq!(proposed.code, "selection_busy");
        let mut declared = selection_sourced(with_observation(Some(same)));
        declared["read_retry"] = serde_json::to_value(&proposed).unwrap();
        assert_eq!(validated_retry(&declared), Some(proposed));
        for (field, replacement) in [
            ("scope_identity", json!("0d".repeat(32))),
            ("owner", json!("admin")),
            ("catalog_revision", json!(6)),
        ] {
            let mut other = selected_source.clone();
            other[field] = replacement;
            let observation = observed(("selection", other));
            assert!(
                proposed_retry(&selection_sourced(with_observation(Some(observation)))).is_none(),
                "{field}"
            );
        }
        let catalog_observation = observed(("grant", catalog_grant.clone()));
        assert!(
            proposed_retry(&selection_sourced(with_observation(Some(
                catalog_observation.clone()
            ))))
            .is_none(),
            "a catalog observation never agrees with a selection"
        );
        assert!(proposed_retry(&combined(with_observation(Some(catalog_observation)))).is_some());
        assert!(
            proposed_retry(&combined(with_observation(Some(observed((
                "selection",
                selected_source.clone()
            ))))))
            .is_none(),
            "a selection observation never agrees with a catalog grant"
        );
        let mut both = observed(("selection", selected_source.clone()));
        both["grant"] = catalog_grant;
        both["provenance"] = json!("user_reviewed_project_grant");
        assert!(proposed_retry(&combined(with_observation(Some(both)))).is_none());
        let mut both_sources = combined(with_observation(None));
        both_sources["selection"]["selection"] = selected_source.clone();
        assert!(proposed_retry(&both_sources).is_none());
        let mut neither = combined(with_observation(None));
        let selection = neither["selection"].as_object_mut().unwrap();
        selection.remove("grant_id");
        selection.remove("grant_revision");
        assert!(proposed_retry(&neither).is_none());
        let mut half = combined(with_observation(None));
        half["selection"]
            .as_object_mut()
            .unwrap()
            .remove("grant_revision");
        assert!(proposed_retry(&half).is_none());
        let mut nonce = selection_sourced(with_observation(None));
        nonce["selection"]["selection"]["grant_nonce"] = json!("0a".repeat(16));
        assert!(
            proposed_retry(&nonce).is_none(),
            "the public selection observation never carries a nonce"
        );
    }

    /// Reviewed cases shared with the TypeScript mirror (`readRetry.ts`), see
    /// `fixtures/wire/read_retry_decisions.py`. `expected` is the native decision.
    #[test]
    fn shared_decision_cases_match_reviewed_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../fixtures/wire/read_retry_decisions.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["schema"],
            "airk.wire-fixture.read-retry-decisions/1"
        );
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty(), "zero selected decision cases");
        for case in cases {
            let expected: Option<SingleRequiredMemberRetry> =
                serde_json::from_value(case["expected"].clone()).unwrap();
            assert_eq!(
                validated_retry(&case["input"]),
                expected,
                "{}: {}",
                case["name"],
                case["rule"]
            );
        }
    }

    #[test]
    fn shared_caller_binding_cases_match_reviewed_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../fixtures/wire/read_retry_decisions.json"
        ))
        .unwrap();
        let cases = fixture["binding_cases"].as_array().unwrap();
        assert!(!cases.is_empty(), "zero selected caller binding cases");
        for case in cases {
            let input = &case["input"];
            let arguments = case["arguments"].as_object().unwrap();
            let actual = match case["kind"].as_str().unwrap() {
                "selection" => validated_retry_for_selection(
                    input,
                    &serde_json::from_value(case["expected_scope"].clone()).unwrap(),
                    &serde_json::from_value(case["expected_selection"].clone()).unwrap(),
                    arguments,
                ),
                "catalog_single" => validated_retry_for_catalog(
                    input,
                    &serde_json::from_value(case["expected_scope"].clone()).unwrap(),
                    arguments,
                ),
                "catalog_combined" => validated_retry_for_combined_catalog(
                    input,
                    &serde_json::from_value(case["expected_scope"].clone()).unwrap(),
                    arguments,
                ),
                kind => panic!("unreviewed caller scope kind {kind}"),
            };
            let expected: Option<SingleRequiredMemberRetry> =
                serde_json::from_value(case["expected"].clone()).unwrap();
            assert_eq!(actual, expected, "{}: {}", case["name"], case["rule"]);
        }
    }
}
