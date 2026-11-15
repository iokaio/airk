// SPDX-License-Identifier: Apache-2.0
//! Strict /2 unassessed fallback comparison, after owning raw validation.
use airk_core::{
    arbiter::{
        Budget,
        context_action::{AssessmentResult, ContextResponseV2},
    },
    identity::{Incarnation, Sha256Digest, framed_digest},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn primitive(raw: &Value) -> Result<Value> {
    let mut rules = super::strict::Normalization::default();
    if let Some(id) = raw.get("request_id") {
        let _: Incarnation = serde_json::from_value(id.clone())?;
        rules.identities.insert(
            "/request_id".into(),
            id.as_str().context("primitive request ID")?.into(),
        );
    }
    let mut result = super::strict::normalize(raw, &rules).map_err(anyhow::Error::msg)?;
    if let Some(freshness) = raw.get("freshness") {
        let _: airk_core::changes::FreshnessObservation =
            serde_json::from_value(freshness.clone())?;
        let mut timing = super::strict::Normalization::default();
        timing.timing.push("/observed_unix_ms".into());
        result["freshness"] =
            super::strict::normalize(freshness, &timing).map_err(anyhow::Error::msg)?;
    }
    if let Some(receipt) = raw.get("source_verification") {
        let typed: airk_core::verification::VerificationReceipt =
            serde_json::from_value(receipt.clone())?;
        ensure!(
            typed.selection_digest == airk_kernel::verification::selection_digest(raw)?,
            "raw saved-source verification selection digest mismatch"
        );
        ensure!(
            typed.observed_to_unix_ms >= typed.observed_from_unix_ms,
            "crossed source verification interval"
        );
        let mut normalized = receipt.clone();
        normalized["observed_from_unix_ms"] = json!("<verified-source-observation-start>");
        normalized["observed_to_unix_ms"] = json!("<verified-source-observation-end>");
        for (position, source) in typed.sources.iter().enumerate() {
            ensure!(
                source.observed_to_unix_ms >= source.observed_from_unix_ms,
                "crossed per-source verification interval"
            );
            normalized["sources"][position]["observed_from_unix_ms"] =
                json!("<verified-source-observation-start>");
            normalized["sources"][position]["observed_to_unix_ms"] =
                json!("<verified-source-observation-end>");
        }
        result["source_verification"] = normalized;
    }
    if raw.get("source_verification").is_some() {
        result["source_verification"]["selection_digest"] =
            serde_json::to_value(airk_kernel::verification::selection_digest(&result)?)?;
    }
    Ok(result)
}

fn authenticated(raw: &Value, budget: &Budget) -> Result<ContextResponseV2> {
    let typed = ContextResponseV2::from_canonical(raw, budget)?;
    ensure!(
        typed
            .assessment_history
            .iter()
            .all(|event| matches!(&event.result, AssessmentResult::Unavailable { .. })),
        "learned decisions require explicit prompt/decision parity, not fallback rebinding"
    );
    let scope = &typed.assessment.scope;
    let application = typed
        .applications
        .iter()
        .find(|application| application.scope == *scope)
        .context("fallback replay requires the actual same-scope application")?
        .clone();
    let now = std::time::Instant::now();
    let request = airk_core::arbiter::ContextRequest {
        schema: airk_core::arbiter::REQUEST_SCHEMA.into(),
        question: typed.question.clone(),
        budget: budget.clone(),
    };
    let mut gathering =
        airk_kernel::arbiter::Gathering::new(request, scope.clone(), application, now)?;
    for record in &typed.records {
        let dispatch = gathering
            .next(now)?
            .context("record is not in the owning fallback dispatch prefix")?;
        ensure!(
            dispatch.choice.operation == record.operation
                && dispatch
                    .arguments
                    .clone()
                    .into_iter()
                    .collect::<BTreeMap<_, _>>()
                    == record.arguments,
            "raw record operation/arguments differ from owning dispatch"
        );
        let observed = if record.canonical_result["isError"] == true {
            Err(airk_kernel::workbench::ReadFailure(record.canonical_result.clone()).into())
        } else {
            Ok(record.canonical_result.clone())
        };
        gathering.observe(observed, now)?;
        let generated = gathering
            .records()
            .last()
            .context("raw record was not retained by owning recipe")?;
        ensure!(
            serde_json::to_value(generated)? == serde_json::to_value(record)?,
            "raw record digest/scope/body do not match owning canonical dispatch"
        );
    }
    if typed.omitted_count > 0 {
        prove_disclosed_prefix(&typed, &mut gathering, now)?;
    }
    Ok(typed)
}

/// Authenticate only the returned deterministic prefix. The owning packer
/// removes suffix records, never the mandatory first Search; omitted contents
/// are unavailable and are neither reconstructed nor declared equivalent.
fn prove_disclosed_prefix(
    packet: &ContextResponseV2,
    gathering: &mut airk_kernel::arbiter::Gathering,
    now: std::time::Instant,
) -> Result<()> {
    ensure!(
        !packet.records.is_empty(),
        "omitted packet lost mandatory initial native Search"
    );
    ensure!(
        packet.records.len().checked_add(packet.omitted_count) == Some(packet.candidate_count)
            && packet.work.calls >= packet.candidate_count,
        "disclosed prefix counters contradict actual calls"
    );
    let retained_bytes =
        packet
            .records
            .iter()
            .try_fold(0usize, |total, record| -> Result<usize> {
                total
                    .checked_add(serde_json::to_vec(&record.canonical_result)?.len())
                    .context("retained prefix bytes overflow")
            })?;
    ensure!(
        packet.work.bytes >= retained_bytes,
        "disclosed prefix native work lost retained byte charge"
    );
    ensure!(
        gathering.next(now)?.is_some(),
        "claimed omitted suffix has no next owning native dispatch"
    );
    Ok(())
}
fn packet(raw: &Value, budget: &Budget) -> Result<Value> {
    let typed = authenticated(raw, budget)?;
    let mut result = primitive(raw)?;
    let mut records = BTreeMap::new();
    for (position, record) in typed.records.iter().enumerate() {
        let ordinal = u64::try_from(position)?;
        let logical = framed_digest(
            b"airk.test.context-record-position/1",
            &[&ordinal.to_le_bytes()],
        );
        ensure!(
            records.insert(record.id, logical).is_none(),
            "duplicate raw record identity"
        );
        result["records"][position]["id"] = serde_json::to_value(logical)?;
        result["records"][position]["canonical_result"] = primitive(&record.canonical_result)?;
    }
    let rebound = |id: Sha256Digest| -> Result<Value> {
        serde_json::to_value(
            records
                .get(&id)
                .context("orphaned raw evidence reference")?,
        )
        .map_err(Into::into)
    };
    for (position, paragraph) in typed.paragraphs.iter().enumerate() {
        for (reference, id) in paragraph.evidence.iter().enumerate() {
            result["paragraphs"][position]["evidence"][reference] = rebound(*id)?;
        }
    }
    for (position, event) in typed.assessment_history.iter().enumerate() {
        for (reference, unit) in event.basis.source_units_presented.iter().enumerate() {
            result["assessment_history"][position]["basis"]["source_units_presented"][reference]
                ["record"] = rebound(unit.record)?;
        }
        if let AssessmentResult::Unavailable {
            advice: Some(advice),
            ..
        } = &event.result
        {
            for (reference, id) in advice.evidence.iter().enumerate() {
                result["assessment_history"][position]["result"]["advice"]["evidence"][reference] =
                    rebound(*id)?;
            }
        }
    }
    if raw.get("source_verification").is_some() {
        result["source_verification"]["selection_digest"] =
            serde_json::to_value(airk_kernel::verification::selection_digest(&result)?)?;
    }
    Ok(result)
}

/// Both full raw graphs are authenticated by the owning native recipe FIRST.
pub fn pair(left: &Value, right: &Value, budget: &Budget) -> Result<(Value, Value)> {
    Ok((packet(left, budget)?, packet(right, budget)?))
}

/// Normalize the whole verifier's duplicate native packets only after the
/// original complete projection and every retained packet have been proved.
pub fn whole_verification(
    raw: &Value,
    normalized: &Value,
    budget: &Budget,
    controls: airk_core::verification::VerificationControls,
) -> Result<Option<Value>> {
    airk_core::verification::validate_projection(raw)?;
    let Some(requested) = controls.resolve()? else {
        ensure!(
            raw.get("member_verification").is_none(),
            "off verification cannot carry a whole proof"
        );
        return Ok(None);
    };
    let proof = raw
        .get("member_verification")
        .filter(|proof| !proof.is_null())
        .context("enabled verification requires its complete whole proof")?;
    if proof["schema"] != "airk.combined-verification/1" {
        return Ok(Some(proof.clone()));
    }
    let members = raw["members"]
        .as_array()
        .context("whole proof native member roster")?;
    let ids = members
        .iter()
        .map(|member| {
            serde_json::from_value::<airk_core::index_catalog::IndexId>(member["index_id"].clone())
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let summary: airk_core::combined_context::CombinedContextSummary =
        serde_json::from_value(raw["context"].clone())?;
    summary.validate(&ids)?;
    ensure!(
        serde_json::to_value(&summary.budget)? == serde_json::to_value(budget)?,
        "whole proof original Answer budget changed"
    );
    let mut result = proof.clone();
    let mut sources = 0usize;
    let mut bytes = 0u64;
    let mut window = 0u64;
    for (position, member) in proof["members"]
        .as_array()
        .context("whole proof members")?
        .iter()
        .enumerate()
    {
        let index: airk_core::index_catalog::IndexId =
            serde_json::from_value(member["index_id"].clone())?;
        let native = ids
            .iter()
            .position(|id| *id == index)
            .context("whole proof foreign native index")?;
        let allocation: airk_core::verification::VerificationAllocation =
            serde_json::from_value(member["allocation"].clone())?;
        sources = sources
            .checked_add(allocation.max_sources)
            .context("whole source allocation overflow")?;
        bytes = bytes
            .checked_add(allocation.max_bytes)
            .context("whole source byte allocation overflow")?;
        window = window
            .checked_add(allocation.deadline_ms)
            .context("whole source window overflow")?;
        ensure!(
            sources <= requested.max_sources
                && bytes <= requested.max_bytes
                && window <= requested.deadline_ms.min(budget.deadline_ms),
            "whole proof exceeds original source budget"
        );
        result["members"][position]["allocation"]["deadline_ms"] =
            json!("<validated-shared-source-window>");
        let own = &member["result"];
        let Some(receipt) = own.get("source_verification") else {
            continue;
        };
        let receipt: airk_core::verification::VerificationReceipt =
            serde_json::from_value(receipt.clone())?;
        ensure!(
            receipt.mode == requested.mode
                && receipt.limits.max_sources <= allocation.max_sources
                && receipt.limits.max_bytes <= allocation.max_bytes
                && receipt.limits.deadline_ms <= allocation.deadline_ms
                && receipt.limits.effective_deadline_ms <= receipt.limits.deadline_ms,
            "whole receipt exceeds original member allocation"
        );
        let steps = own["steps"]
            .as_array()
            .context("whole proof native steps")?;
        ensure!(
            steps.len() == 1
                && member["reference_bindings"]
                    == json!([{"member_pointer":"/steps/0/result","result_pointer":format!("/members/{native}/result{}",if members[native]["result"].get("structuredContent").is_some(){"/structuredContent"}else{""})}]),
            "whole proof is not the owning one-member projection"
        );
        let native_packet = if let Some(body) = members[native]["result"].get("structuredContent") {
            body.clone()
        } else {
            members[native]["result"].clone()
        };
        ensure!(
            steps[0]["result"] == native_packet,
            "whole verifier changed duplicated native packet bytes"
        );
        let allowance = summary.members[native]
            .allowance
            .as_ref()
            .context("whole proof has no original packet allowance")?;
        let mut child = budget.clone();
        child.calls = allowance.resources.calls;
        child.input_tokens = allowance.resources.input_tokens.max(1);
        child.candidates = allowance.resources.candidates;
        child.max_bytes = allowance.resources.bytes;
        let packet = packet(&native_packet, &child)?;
        ensure!(
            normalized["members"][native]["result"] == packet,
            "whole proof normalization differs from authenticated native packet"
        );
        let mut copy = primitive(own)?;
        copy["steps"][0]["result"] = packet;
        copy["source_verification"]["limits"]["deadline_ms"] =
            json!("<validated-member-source-window>");
        copy["source_verification"]["limits"]["effective_deadline_ms"] =
            json!("<validated-member-source-headroom>");
        copy["source_verification"]["selection_digest"] =
            serde_json::to_value(airk_kernel::verification::selection_digest(&copy)?)?;
        result["members"][position]["result"] = copy;
        result["members"][position]["allocation"]["deadline_ms"] =
            json!("<validated-shared-source-window>");
    }
    Ok(Some(result))
}

/// Only the actually negotiated older MCP framing omits this SDK constant.
pub fn negotiated_mcp_pair(left: &Value, right: &Value, protocol: &str) -> Result<(Value, Value)> {
    ensure!(
        matches!(
            protocol,
            "2024-11-05" | "2025-03-26" | "2025-06-18" | "2025-11-25" | "2026-07-28"
        ),
        "unknown actual negotiated MCP protocol"
    );
    let mut left = left.clone();
    if protocol != "2026-07-28" {
        ensure!(
            right.get("resultType").is_none(),
            "negotiated older MCP wire retained resultType"
        );
        if let Some(kind) = left
            .as_object_mut()
            .context("native envelope")?
            .remove("resultType")
        {
            ensure!(kind == "complete", "unknown native result discriminator");
        }
    }
    Ok((left, right.clone()))
}

fn envelope_body(raw: &Value) -> Result<Value> {
    ensure!(
        raw.get("isError") != Some(&Value::Bool(true)),
        "finite Answer failed; no replay permitted"
    );
    let text = raw
        .pointer("/content/0/text")
        .and_then(Value::as_str)
        .context("Answer text body")?;
    let body: Value = serde_json::from_str(text)?;
    if let Some(structured) = raw.get("structuredContent") {
        ensure!(
            body == *structured,
            "Answer text/structured duplicate mismatch"
        );
    }
    Ok(body)
}
pub fn envelopes(left: &Value, right: &Value, budget: &Budget) -> Result<(Value, Value)> {
    let (a, b) = pair(&envelope_body(left)?, &envelope_body(right)?, budget)?;
    Ok((replace_body(left, a)?, replace_body(right, b)?))
}

pub fn replace_body(raw: &Value, body: Value) -> Result<Value> {
    envelope_body(raw)?;
    let mut result = raw.clone();
    result["content"][0]["text"] = serde_json::to_value(serde_json::to_string(&body)?)?;
    if raw.get("structuredContent").is_some() {
        result["structuredContent"] = body;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dated_source_receipts_normalize_only_valid_clocks_and_keep_hashes_and_future_data()
    -> Result<()> {
        use airk_core::verification::*;
        let mut raw = json!({"hits":[],"future":{"elapsed_ms":17,"request_id":"evidence-owned"}});
        let hash = Sha256Digest::from_bytes([1; 32]);
        let receipt = VerificationReceipt {
            schema: "airk.saved-source-verification/1".into(),
            method: "sha256-full-saved-source/1".into(),
            mode: VerificationMode::Report,
            status: "matched".into(),
            scope_digest: hash,
            selection_digest: airk_kernel::verification::selection_digest(&raw)?,
            observed_from_unix_ms: 1,
            observed_to_unix_ms: 2,
            publication_fence: "current".into(),
            inventory_complete: true,
            all_references_accounted: true,
            source_rows_omitted: 0,
            requirements_satisfied: Some(true),
            counts: VerificationCounts::default(),
            limits: VerificationBudget {
                mode: VerificationMode::Report,
                max_sources: 1,
                max_bytes: 1024,
                deadline_ms: 1000,
            }
            .into(),
            origins: Vec::new(),
            references: Vec::new(),
            qualifications: Vec::new(),
            error: None,
            sources: vec![VerifiedSource {
                id: hash,
                origin_id: "native-origin".into(),
                source_root_id: None,
                original_root: None,
                source_relative_path: None,
                path: Some("api.rs".into()),
                expected_hash: Some(hash),
                observed_hash: Some(hash),
                profile_digest: None,
                project_ids: Vec::new(),
                reference_ids: Vec::new(),
                state: SourceCheckState::Matched,
                reason: None,
                observed_from_unix_ms: 1,
                observed_to_unix_ms: 2,
                bytes_read: 0,
            }],
        };
        raw["source_verification"] = serde_json::to_value(receipt)?;
        let mut later = raw.clone();
        for pointer in [
            "/source_verification/observed_from_unix_ms",
            "/source_verification/observed_to_unix_ms",
            "/source_verification/sources/0/observed_from_unix_ms",
            "/source_verification/sources/0/observed_to_unix_ms",
        ] {
            *later.pointer_mut(pointer).unwrap() = json!(9);
        }
        assert_eq!(primitive(&raw)?, primitive(&later)?);
        let mut changed = later.clone();
        changed["source_verification"]["sources"][0]["expected_hash"] =
            json!(Sha256Digest::from_bytes([2; 32]));
        assert_ne!(primitive(&raw)?, primitive(&changed)?);
        let mut changed = later.clone();
        changed["future"]["elapsed_ms"] = json!(18);
        assert!(
            primitive(&changed).is_err(),
            "a valid old receipt cannot certify changed unknown native evidence"
        );
        changed["source_verification"]["selection_digest"] =
            serde_json::to_value(airk_kernel::verification::selection_digest(&changed)?)?;
        assert_ne!(
            primitive(&raw)?,
            primitive(&changed)?,
            "valid newly bound future data must remain unequal"
        );
        let mut changed = later;
        changed["source_verification"]["sources"][0]["observed_from_unix_ms"] = json!(10);
        assert!(
            primitive(&changed).is_err(),
            "crossed source intervals cannot be normalized away"
        );
        Ok(())
    }
}
