// SPDX-License-Identifier: Apache-2.0
//! Strict Answer /3 reply comparison, after owning raw validation.
//!
//! Both replies are first validated by the owning Core type
//! (`ContextResponseV3::from_canonical` under the request budget, or a
//! member's clamped allowance). The comparison then keeps every field and
//! normalizes only:
//! - transport identities: each canonical read's top-level `request_id`
//!   (through `strict`), and the record ids derived from those bodies, which
//!   are renamed by position and rebound everywhere the reply cites them;
//! - digests the reply cannot recompute because they bind record ids, read
//!   bodies or measured work (`decision_log.digest`, each decision's
//!   `basis.digest` and `receipt`, each span projection's `full_digest`):
//!   renamed by first appearance, so equal digests stay equal and different
//!   ones stay different;
//! - timing: `elapsed_ms`, the measured `work` and `decision_log.timings`
//!   microseconds (present or absent stays compared), and each part's
//!   `deadline_share_ms`, which a finished part trims to its elapsed time
//!   (bounded here by the budget deadline).
//! - order: lists the kernel keeps in record-id order (`record_uses` and
//!   the evidence lists) are compared in logical-id order after rebinding.
//!
//! A /3 reply carries span projections of its canonical reads, not the read
//! bodies, so the /2 replay of records through the owning `Gathering` has no
//! /3 equivalent; the full normalized results are compared instead.
use airk_core::{
    arbiter::answer_loop::{BudgetV3, ContextResponseV3},
    combined_context::CombinedContextSummaryV2,
    identity::{Incarnation, framed_digest},
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

fn logical(domain: &[u8], position: usize) -> Result<Value> {
    let ordinal = u64::try_from(position)?;
    Ok(serde_json::to_value(framed_digest(
        domain,
        &[&ordinal.to_le_bytes()],
    ))?)
}

/// Rebind every cited record id outside the canonical results.
fn rebind(value: &mut Value, ids: &BTreeMap<String, Value>, in_records: bool) {
    match value {
        Value::String(text) => {
            if let Some(logical) = ids.get(text.as_str()) {
                *value = logical.clone();
            }
        }
        Value::Array(items) => {
            for item in items {
                rebind(item, ids, in_records);
            }
        }
        Value::Object(fields) => {
            for (key, field) in fields.iter_mut() {
                if in_records && key == "canonical_result" {
                    continue;
                }
                rebind(field, ids, false);
            }
        }
        _ => {}
    }
}

fn custody(value: &mut Value, seen: &mut BTreeMap<String, Value>) -> Result<()> {
    if value.is_null() {
        return Ok(());
    }
    let text = value
        .as_str()
        .context("custody digest is not a digest string")?
        .to_owned();
    let next = seen.len();
    let name = seen
        .entry(text)
        .or_insert_with(|| json!(format!("<custody-digest:{next}>")))
        .clone();
    *value = name;
    Ok(())
}

fn timing(value: &mut Value, pointer: &str) -> Result<()> {
    let Some(field) = value.pointer_mut(pointer) else {
        return Ok(());
    };
    if field.is_null() {
        return Ok(());
    }
    ensure!(field.is_u64(), "invalid Answer timing at {pointer}");
    *field = json!(format!("<timing:{pointer}>"));
    Ok(())
}

fn reply(raw: &Value, budget: &BudgetV3) -> Result<Value> {
    let typed = ContextResponseV3::from_canonical(raw, budget)?;
    let mut result = primitive(raw)?;
    let mut ids = BTreeMap::new();
    let mut seen = BTreeMap::new();
    for (position, record) in typed.records.iter().enumerate() {
        ensure!(
            ids.insert(
                record.id.to_string(),
                logical(b"airk.test.answer-record-position/1", position)?
            )
            .is_none(),
            "duplicate raw record identity"
        );
        let mut projection = primitive(&record.canonical_result)?;
        // A span projection names its full read body by digest; that body
        // carries the read's transport request id.
        if let Some(digest) = projection.get_mut("full_digest") {
            custody(digest, &mut seen)?;
        }
        result["records"][position]["canonical_result"] = projection;
    }
    for (position, id) in typed.omitted_evidence.iter().enumerate() {
        ensure!(
            ids.insert(
                id.to_string(),
                logical(b"airk.test.answer-omitted-position/1", position)?
            )
            .is_none(),
            "omitted evidence repeats a record identity"
        );
    }
    if let Some(fields) = result.as_object_mut() {
        for (key, field) in fields.iter_mut() {
            rebind(field, &ids, key == "records");
        }
    }
    // Lists the kernel keeps in record-id order are in a transport-derived
    // order; after rebinding they are compared in logical-id order.
    let sort = |list: &mut Value| {
        if let Some(items) = list.as_array_mut() {
            items.sort_by_key(Value::to_string);
        }
    };
    sort(&mut result["record_uses"]);
    sort(&mut result["omitted_evidence"]);
    for part in result["parts"].as_array_mut().into_iter().flatten() {
        for need in part["sub_needs"].as_array_mut().into_iter().flatten() {
            sort(&mut need["evidence"]);
        }
    }
    for paragraph in result["paragraphs"].as_array_mut().into_iter().flatten() {
        sort(&mut paragraph["evidence"]);
    }
    for conflict in result["union"]["conflicts"]
        .as_array_mut()
        .into_iter()
        .flatten()
    {
        sort(&mut conflict["evidence"]);
    }
    custody(&mut result["decision_log"]["digest"], &mut seen)?;
    for position in 0..typed.decisions.len() {
        custody(
            &mut result["decisions"][position]["basis"]["digest"],
            &mut seen,
        )?;
        if let Some(receipt) = result["decisions"][position].get_mut("receipt") {
            custody(receipt, &mut seen)?;
        }
    }
    timing(&mut result, "/elapsed_ms")?;
    for field in ["queue_us", "compile_us", "inference_us", "policy_us"] {
        timing(&mut result, &format!("/work/{field}"))?;
        timing(&mut result, &format!("/decision_log/timings/{field}"))?;
    }
    for (position, part) in typed.parts.iter().enumerate() {
        ensure!(
            part.deadline_share_ms <= budget.deadline_ms,
            "part deadline share exceeds the Answer deadline"
        );
        timing(&mut result, &format!("/parts/{position}/deadline_share_ms"))?;
    }
    if raw.get("source_verification").is_some() {
        result["source_verification"]["selection_digest"] =
            serde_json::to_value(airk_kernel::verification::selection_digest(&result)?)?;
    }
    Ok(result)
}

/// Both full raw replies are validated by the owning Core type FIRST.
pub fn pair(left: &Value, right: &Value, budget: &BudgetV3) -> Result<(Value, Value)> {
    Ok((reply(left, budget)?, reply(right, budget)?))
}

/// The budget a combined member ran under: its allowance clamped to the
/// original request budget.
pub fn member_budget(summary: &CombinedContextSummaryV2, position: usize) -> Result<BudgetV3> {
    summary
        .members
        .get(position)
        .and_then(|member| member.allowance.as_ref())
        .context("queried member allowance")?
        .clamp(&summary.budget)
}

/// Normalize the whole verifier's duplicate native replies only after the
/// original complete projection and every retained reply have been proved.
pub fn whole_verification(
    raw: &Value,
    normalized: &Value,
    budget: &BudgetV3,
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
    let summary: CombinedContextSummaryV2 = serde_json::from_value(raw["context"].clone())?;
    summary.validate(&ids)?;
    ensure!(
        summary.budget == *budget,
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
        let native_reply = if let Some(body) = members[native]["result"].get("structuredContent") {
            body.clone()
        } else {
            members[native]["result"].clone()
        };
        ensure!(
            steps[0]["result"] == native_reply,
            "whole verifier changed duplicated native reply bytes"
        );
        let normalized_reply = reply(&native_reply, &member_budget(&summary, native)?)?;
        ensure!(
            normalized["members"][native]["result"] == normalized_reply,
            "whole proof normalization differs from the validated native reply"
        );
        let mut copy = primitive(own)?;
        copy["steps"][0]["result"] = normalized_reply;
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
pub fn envelopes(left: &Value, right: &Value, budget: &BudgetV3) -> Result<(Value, Value)> {
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
    use airk_core::identity::Sha256Digest;
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
