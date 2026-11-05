// SPDX-License-Identifier: Apache-2.0
//! Dated saved-byte checks. These contracts imply neither extraction accuracy nor authority.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_SOURCES: usize = 256;
pub const MAX_BYTES: u64 = 1_073_741_824;
pub const MAX_DEADLINE_MS: u64 = 30_000;
pub const STREAM_BYTES: usize = 65_536;
pub const MAX_REFERENCES: usize = 4_096;
pub const MAX_RECEIPT_BYTES: usize = 32_768;
pub const DEFAULT_SOURCES: usize = 8;
pub const DEFAULT_BYTES: u64 = 16 * 1024 * 1024;
pub const DEFAULT_DEADLINE_MS: u64 = 2_000;

#[derive(Debug)]
pub struct VerificationReplyBudget;
impl std::fmt::Display for VerificationReplyBudget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Saved-source verification metadata cannot fit this reply; reduce the result limit/byte budget or increase the plan output budget. No source check was performed.")
    }
}
impl std::error::Error for VerificationReplyBudget {}

pub fn reply_budget_refusal() -> serde_json::Value {
    serde_json::json!({"error":{"code":"verification_reply_budget","message":VerificationReplyBudget.to_string(),"retryable":false,"verification_performed":false}})
}

/// Account for a JSON evidence payload encoded as text inside the native frame.
/// No source reads or hashing occur while establishing this allocation.
pub fn receipt_budget_for_reply(value: &serde_json::Value) -> Result<usize> {
    let quoted = serde_json::to_vec(&serde_json::to_string(value)?)?.len();
    let available = crate::MAX_RESPONSE_BYTES.saturating_sub(quoted + 2048) / 2;
    ensure!(available >= 1024, VerificationReplyBudget);
    Ok(available.min(MAX_RECEIPT_BYTES))
}

#[derive(Debug)]
pub struct VerificationProjectionChanged;
impl std::fmt::Display for VerificationProjectionChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Reply projection removed a checked reference. The response cannot retain its verification claim; increase the output budget or narrow the plan and run it again.")
    }
}
impl std::error::Error for VerificationProjectionChanged {}

/// Late transport observations may consume output budget, but cannot leave a
/// dated receipt pointing at evidence removed by subsequent projection.
pub fn validate_projection(value: &serde_json::Value) -> Result<()> {
    let mut visited = 0;
    validate_projection_inner(value, 0, &mut visited)
}

fn validate_projection_inner(
    value: &serde_json::Value,
    depth: usize,
    visited: &mut usize,
) -> Result<()> {
    *visited += 1;
    ensure!(
        depth <= 32 && *visited <= 32_768,
        VerificationProjectionChanged
    );
    if value["source_verification"]["schema"] == "airk.saved-source-verification/1" {
        let references = value["source_verification"]["references"]
            .as_array()
            .ok_or(VerificationProjectionChanged)?;
        ensure!(
            references.len() <= MAX_REFERENCES,
            VerificationProjectionChanged
        );
        for reference in references {
            let pointer = reference["result_pointer"]
                .as_str()
                .ok_or(VerificationProjectionChanged)?;
            ensure!(
                value
                    .pointer(pointer)
                    .is_some_and(|target| !target.is_null()),
                VerificationProjectionChanged
            );
        }
    }
    if let Some(steps) = value.get("steps").and_then(serde_json::Value::as_array) {
        ensure!(
            steps.len() <= crate::workbench::MAX_STEP_REPORTS,
            VerificationProjectionChanged
        );
        for step in steps {
            if let Some(result) = step.get("result") {
                validate_projection_inner(result, depth + 1, visited)?;
            }
        }
    }
    // A combined result retains complete native responses, including their
    // independently dated receipts. Their JSON pointers remain relative to
    // each native body; do not rebase them onto the composite or certify only
    // its ranking references.
    if let Some(members) = value.get("members").and_then(serde_json::Value::as_array) {
        ensure!(
            members.len() <= crate::index_catalog::MAX_SELECTION,
            VerificationProjectionChanged
        );
        for member in members {
            if let Some(result) = member.get("result") {
                if value["schema"] == crate::combined_reads::COMBINED_READ_SCHEMA {
                    let index = serde_json::from_value::<crate::index_catalog::IndexId>(
                        member["index_id"].clone(),
                    )
                    .map_err(|_| VerificationProjectionChanged)?;
                    let logical =
                        crate::combined_reads::shared_values::materialize_combined_member_result(
                            value, index,
                        )
                        .map_err(|_| VerificationProjectionChanged)?;
                    validate_projection_inner(&logical, depth + 1, visited)?;
                } else {
                    validate_projection_inner(result, depth + 1, visited)?;
                }
            }
        }
    }
    if value["schema"] == crate::combined_reads::COMBINED_READ_SCHEMA
        && let Some(ranking) = value.get("ranking").and_then(serde_json::Value::as_array)
    {
        ensure!(ranking.len() <= 100, VerificationProjectionChanged);
        for rank in ranking {
            crate::combined_reads::materialize_ranked_origin(value, rank)
                .map_err(|_| VerificationProjectionChanged)?;
        }
    }
    if let Some(body) = value.get("structuredContent") {
        validate_projection_inner(body, depth + 1, visited)?;
    }
    if let Some(verification) = value
        .get("member_verification")
        .filter(|verification| !verification.is_null())
    {
        validate_member_projection(value, verification, depth + 1, visited)?;
    }
    Ok(())
}

fn validate_member_projection(
    projection: &serde_json::Value,
    verification: &serde_json::Value,
    depth: usize,
    visited: &mut usize,
) -> Result<()> {
    use crate::index_catalog::IndexId;
    if verification["schema"] != "airk.combined-verification/1" {
        // A failed native observation has no dated success claim to rebase.
        // Retain its complete typed failure, rather than invent member receipts
        // or lose the error while validating an unavailable observation.
        let body = verification
            .get("structuredContent")
            .unwrap_or(verification);
        ensure!(
            verification["isError"] == true
                || body.get("error").is_some_and(|error| !error.is_null()),
            VerificationProjectionChanged
        );
        ensure!(
            body.get("source_verification").is_none() && body.get("members").is_none(),
            VerificationProjectionChanged
        );
        return Ok(());
    }
    ensure!(
        verification["scope_identity"] == projection["scope_identity"]
            && verification["scope_identity"].is_string(),
        VerificationProjectionChanged
    );
    let members = verification["members"]
        .as_array()
        .ok_or(VerificationProjectionChanged)?;
    ensure!(
        members.len() <= crate::index_catalog::MAX_SELECTION,
        VerificationProjectionChanged
    );
    let mut seen = std::collections::BTreeSet::new();
    for member in members {
        let index: IndexId = serde_json::from_value(member["index_id"].clone())
            .map_err(|_| VerificationProjectionChanged)?;
        ensure!(seen.insert(index), VerificationProjectionChanged);
        let result = member.get("result").ok_or(VerificationProjectionChanged)?;
        validate_projection_inner(result, depth + 1, visited)?;
        let body = result.get("structuredContent").unwrap_or(result);
        let Some(receipt) = body.get("source_verification") else {
            // A complete failed observation may contain no source receipt.
            ensure!(
                result["isError"] == true
                    || body.get("error").is_some_and(|error| !error.is_null()),
                VerificationProjectionChanged
            );
            continue;
        };
        ensure!(
            receipt["schema"] == "airk.saved-source-verification/1",
            VerificationProjectionChanged
        );
        let bindings = member["reference_bindings"]
            .as_array()
            .ok_or(VerificationProjectionChanged)?;
        let references = receipt["references"]
            .as_array()
            .ok_or(VerificationProjectionChanged)?;
        ensure!(
            bindings.len() <= MAX_REFERENCES && references.len() <= MAX_REFERENCES,
            VerificationProjectionChanged
        );
        for reference in references {
            let pointer = reference["result_pointer"]
                .as_str()
                .ok_or(VerificationProjectionChanged)?;
            let binding = bindings
                .iter()
                .filter_map(|binding| {
                    let prefix = binding["member_pointer"].as_str()?;
                    let suffix = pointer.strip_prefix(prefix)?;
                    (suffix.is_empty() || suffix.starts_with('/'))
                        .then_some((prefix, suffix, binding))
                })
                .max_by_key(|(prefix, _, _)| prefix.len())
                .ok_or(VerificationProjectionChanged)?;
            let outer = binding.2["result_pointer"]
                .as_str()
                .ok_or(VerificationProjectionChanged)?;
            ensure!(
                binding.0.len() <= 4096
                    && outer.len() <= 4096
                    && (outer.is_empty() || outer.starts_with('/')),
                VerificationProjectionChanged
            );
            let outer_pointer = format!("{outer}{}", binding.1);
            let target = logical_projection_target(projection, &outer_pointer)?;
            let checked = body
                .pointer(pointer)
                .filter(|target| !target.is_null())
                .ok_or(VerificationProjectionChanged)?;
            ensure!(&target == checked, VerificationProjectionChanged);
            // Identical bytes in another index remain another authority. Find
            // the explicit nearest member/flattened origin, never infer it from
            // matching source paths or hashes.
            let mut ancestor = outer_pointer.as_str();
            let explicit = loop {
                if let Some(value) = projection.pointer(ancestor)
                    && let Some(raw) = value
                        .pointer("/origin/index_id")
                        .or_else(|| value.get("index_id"))
                {
                    break serde_json::from_value::<IndexId>(raw.clone()).ok();
                }
                let Some((parent, _)) = ancestor.rsplit_once('/') else {
                    break None;
                };
                ancestor = parent;
            };
            ensure!(explicit == Some(index), VerificationProjectionChanged);
        }
    }
    let inventory = ReferenceInventory::collect(projection);
    ensure!(inventory.complete, VerificationProjectionChanged);
    ensure!(
        inventory.references.iter().all(|reference| reference
            .index_id
            .is_some_and(|index| seen.contains(&index))),
        VerificationProjectionChanged
    );
    Ok(())
}

/// Resolve a reference at its ORIGINAL returned pointer while comparing a
/// logical copy of an encoded member header. Evidence rows never move, and no
/// receipt digest, saved plan result or raw projection is rewritten or rehashed.
fn logical_projection_target(
    projection: &serde_json::Value,
    pointer: &str,
) -> Result<serde_json::Value> {
    let raw = projection
        .pointer(pointer)
        .filter(|value| !value.is_null())
        .ok_or(VerificationProjectionChanged)?;
    let mut ancestor = pointer;
    loop {
        if canonical_result_container(ancestor)
            && let Some(combined) = projection.pointer(ancestor)
            && combined["schema"] == crate::combined_reads::COMBINED_READ_SCHEMA
        {
            let relative = pointer
                .strip_prefix(ancestor)
                .ok_or(VerificationProjectionChanged)?;
            let tail = relative
                .strip_prefix("/members/")
                .ok_or(VerificationProjectionChanged)?;
            let (offset, suffix) = tail
                .split_once("/result")
                .ok_or(VerificationProjectionChanged)?;
            ensure!(
                suffix.is_empty() || suffix.starts_with('/'),
                VerificationProjectionChanged
            );
            let offset = offset
                .parse::<usize>()
                .map_err(|_| VerificationProjectionChanged)?;
            let member = combined
                .get("members")
                .and_then(serde_json::Value::as_array)
                .and_then(|members| members.get(offset))
                .ok_or(VerificationProjectionChanged)?;
            let index =
                serde_json::from_value::<crate::index_catalog::IndexId>(member["index_id"].clone())
                    .map_err(|_| VerificationProjectionChanged)?;
            let logical = crate::combined_reads::shared_values::materialize_combined_member_result(
                combined, index,
            )
            .map_err(|_| VerificationProjectionChanged)?;
            return logical
                .pointer(suffix)
                .filter(|value| !value.is_null())
                .cloned()
                .ok_or_else(|| VerificationProjectionChanged.into());
        }
        let Some((parent, _)) = ancestor.rsplit_once('/') else {
            break;
        };
        ancestor = parent;
    }
    Ok(raw.clone())
}

// Only transport envelopes and generated step results contain native codecs.
// JSON embedded inside source records is ordinary evidence, even if it happens
// to contain a schema with the same spelling.
fn canonical_result_container(pointer: &str) -> bool {
    let mut tokens = pointer.split('/').skip(1);
    while let Some(token) = tokens.next() {
        match token {
            "structuredContent" => {}
            "steps" => {
                if !tokens
                    .next()
                    .is_some_and(|index| index.parse::<usize>().is_ok())
                    || tokens.next() != Some("result")
                {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

mod inventory;
pub use inventory::{ReferenceInventory, SavedSourceReference};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationMode {
    Off,
    #[default]
    Report,
    Require,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationScope {
    #[default]
    Response,
    Steps,
}

/// Flat canonical controls. Omission selects the published bounded Report preset;
/// explicit Off is the no-source-I/O path. Supplied budgets override that preset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationControls {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub verification: Option<VerificationMode>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub verification_max_sources: Option<usize>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub verification_max_bytes: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_value"
    )]
    pub verification_deadline_ms: Option<u64>,
}

/// A missing optional control differs from an explicitly null wire value.
pub fn present_value<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

pub fn catalog() -> serde_json::Value {
    serde_json::json!({"schema":"airk.saved-source-verification-capability/1",
        "receipt_schema":"airk.saved-source-verification/1","method":"sha256-full-saved-source/1",
        "modes":["off","report","require"],"default":"report","default_budgets":{"max_sources":DEFAULT_SOURCES,"max_bytes":DEFAULT_BYTES,"deadline_ms":DEFAULT_DEADLINE_MS},"enabled_budgets":"published_defaults_with_explicit_overrides",
        "maximums":{"sources":MAX_SOURCES,"raw_bytes":MAX_BYTES,"cooperative_deadline_ms":MAX_DEADLINE_MS,"references":MAX_REFERENCES,"receipt_bytes":MAX_RECEIPT_BYTES},
        "stream_bytes":STREAM_BYTES,"eof_probe_reserve_bytes":1,"concurrency":1,"refresh":false,"whole_index_certification":false,
        "limitations":["saved_bytes_only","dated_observation","extraction_not_certified","unsaved_buffers_not_checked","coverage_not_certified","written_targets_not_resolved","blocking_filesystem_calls_retain_admission"]})
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerificationBudget {
    pub mode: VerificationMode,
    pub max_sources: usize,
    pub max_bytes: u64,
    pub deadline_ms: u64,
}

/// Internal deterministic share of a valid outer verification request. Zero
/// means no source work is available for this member, never the public default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationAllocation {
    pub max_sources: usize,
    pub max_bytes: u64,
    pub deadline_ms: u64,
}
impl VerificationAllocation {
    pub fn apply(self, requested: VerificationBudget) -> Result<VerificationBudget> {
        ensure!(
            self.max_sources <= requested.max_sources
                && self.max_bytes <= requested.max_bytes
                && self.deadline_ms <= requested.deadline_ms,
            "member verification allocation exceeds the requested outer budget"
        );
        Ok(VerificationBudget {
            mode: requested.mode,
            max_sources: self.max_sources,
            max_bytes: self.max_bytes,
            deadline_ms: self.deadline_ms,
        })
    }
}

impl VerificationControls {
    pub fn off() -> Self {
        Self {
            verification: Some(VerificationMode::Off),
            ..Self::default()
        }
    }

    pub fn is_set(self) -> bool {
        self.verification.is_some()
            || self.verification_max_sources.is_some()
            || self.verification_max_bytes.is_some()
            || self.verification_deadline_ms.is_some()
    }

    pub fn with_overrides(self, other: Self) -> Self {
        Self {
            verification: other.verification.or(self.verification),
            verification_max_sources: other
                .verification_max_sources
                .or(self.verification_max_sources),
            verification_max_bytes: other.verification_max_bytes.or(self.verification_max_bytes),
            verification_deadline_ms: other
                .verification_deadline_ms
                .or(self.verification_deadline_ms),
        }
    }

    pub fn resolved(budget: Option<VerificationBudget>) -> Self {
        budget.map_or_else(Self::off, |budget| Self {
            verification: Some(budget.mode),
            verification_max_sources: Some(budget.max_sources),
            verification_max_bytes: Some(budget.max_bytes),
            verification_deadline_ms: Some(budget.deadline_ms),
        })
    }

    pub fn resolve(self) -> Result<Option<VerificationBudget>> {
        let mode = self.verification.unwrap_or_default();
        let supplied = self.verification_max_sources.is_some()
            || self.verification_max_bytes.is_some()
            || self.verification_deadline_ms.is_some();
        if mode == VerificationMode::Off {
            ensure!(
                !supplied,
                "verification budgets require report or require mode"
            );
            return Ok(None);
        }
        let max_sources = self.verification_max_sources.unwrap_or(DEFAULT_SOURCES);
        let max_bytes = self.verification_max_bytes.unwrap_or(DEFAULT_BYTES);
        let deadline_ms = self.verification_deadline_ms.unwrap_or(DEFAULT_DEADLINE_MS);
        ensure!(
            (1..=MAX_SOURCES).contains(&max_sources)
                && (1..=MAX_BYTES).contains(&max_bytes)
                && (1..=MAX_DEADLINE_MS).contains(&deadline_ms),
            "verification budget exceeds supported bounds"
        );
        Ok(Some(VerificationBudget {
            mode,
            max_sources,
            max_bytes,
            deadline_ms,
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceCheckState {
    Matched,
    Mismatch,
    Missing,
    Unreadable,
    OutOfScope,
    ChangedDuringCheck,
    BudgetExhausted,
    PublicationExpired,
    Unsupported,
}

/// Capture result; no source contents or OS error/path text cross this boundary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCheck {
    pub state: SourceCheckState,
    pub observed_hash: Option<crate::identity::Sha256Digest>,
    pub bytes_read: u64,
    pub reason: Option<String>,
}

impl SourceCheck {
    pub fn unchecked(state: SourceCheckState, bytes_read: u64, reason: &'static str) -> Self {
        debug_assert!(!matches!(
            state,
            SourceCheckState::Matched | SourceCheckState::Mismatch
        ));
        Self {
            state,
            observed_hash: None,
            bytes_read,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationReceipt {
    pub schema: String,
    pub method: String,
    pub mode: VerificationMode,
    pub status: String,
    pub scope_digest: crate::identity::Sha256Digest,
    pub selection_digest: crate::identity::Sha256Digest,
    pub observed_from_unix_ms: u64,
    pub observed_to_unix_ms: u64,
    pub publication_fence: String,
    pub inventory_complete: bool,
    pub all_references_accounted: bool,
    pub source_rows_omitted: usize,
    pub requirements_satisfied: Option<bool>,
    pub counts: VerificationCounts,
    pub limits: VerificationLimits,
    pub origins: Vec<VerificationOrigin>,
    pub sources: Vec<VerifiedSource>,
    pub references: Vec<VerifiedReference>,
    pub qualifications: Vec<VerificationQualification>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<VerificationFailure>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationCounts {
    pub references: Option<usize>,
    pub references_lower_bound: usize,
    pub requested_sources: Option<usize>,
    pub requested_sources_lower_bound: usize,
    pub checked_sources: usize,
    pub matched_sources: usize,
    pub mismatched_sources: usize,
    pub unchecked_sources: Option<usize>,
    pub bytes_read: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationLimits {
    pub max_sources: usize,
    pub max_bytes: u64,
    pub deadline_ms: u64,
    pub effective_deadline_ms: u64,
    pub stream_bytes: usize,
    pub eof_probe_reserve_bytes: usize,
    pub concurrency: usize,
    pub reference_inventory_max: usize,
    pub receipt_bytes_max: usize,
    pub receipt_bytes_budget: usize,
    pub hit: Vec<String>,
}

impl From<VerificationBudget> for VerificationLimits {
    fn from(budget: VerificationBudget) -> Self {
        Self {
            max_sources: budget.max_sources,
            max_bytes: budget.max_bytes,
            deadline_ms: budget.deadline_ms,
            effective_deadline_ms: budget.deadline_ms,
            stream_bytes: STREAM_BYTES,
            eof_probe_reserve_bytes: 1,
            concurrency: 1,
            reference_inventory_max: MAX_REFERENCES,
            receipt_bytes_max: MAX_RECEIPT_BYTES,
            receipt_bytes_budget: MAX_RECEIPT_BYTES,
            hit: vec![],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationOrigin {
    pub id: String,
    pub index_id: Option<String>,
    pub legacy_project: String,
    pub collection: String,
    pub incarnation: crate::identity::Incarnation,
    pub publication: crate::identity::RevisionToken,
    pub project_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedSource {
    pub id: crate::identity::Sha256Digest,
    pub origin_id: String,
    pub source_root_id: Option<String>,
    pub original_root: Option<String>,
    pub source_relative_path: Option<String>,
    pub path: Option<String>,
    pub expected_hash: Option<crate::identity::Sha256Digest>,
    pub observed_hash: Option<crate::identity::Sha256Digest>,
    pub profile_digest: Option<crate::identity::Sha256Digest>,
    pub project_ids: Vec<String>,
    pub reference_ids: Vec<String>,
    pub state: SourceCheckState,
    pub reason: Option<String>,
    pub observed_from_unix_ms: u64,
    pub observed_to_unix_ms: u64,
    pub bytes_read: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedReference {
    pub id: String,
    pub source_id: crate::identity::Sha256Digest,
    pub result_pointer: String,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationQualification {
    pub kind: String,
    pub verified: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationFailure {
    pub code: String,
    pub retryable: bool,
}

/// Replace only the plan's saved-source freshness sentence. Other evidence
/// qualifications and the need to recheck before subsequent edits survive.
pub fn dated_warnings(warnings: &mut [String], receipt: &VerificationReceipt) {
    if receipt.status != "verified" || receipt.requirements_satisfied != Some(true) {
        return;
    }
    for warning in warnings {
        if let Some(rest) =
            warning.strip_prefix("Untrusted indexed evidence; verify live files before edits.")
        {
            *warning = format!(
                "Untrusted indexed evidence. Saved sources verified at the observation time recorded in source_verification, against its exact indexed revisions. Recheck before later edits.{rest}"
            );
        }
    }
}

/// Preserve the full attempted evidence on a required-check failure. This is a
/// nonretryable outcome, shared by native transports and the plan fallback.
pub fn attach_response(
    value: &mut serde_json::Value,
    receipt: VerificationReceipt,
) -> Result<bool> {
    value["source_verification"] = serde_json::to_value(receipt)?;
    Ok(mark_required_failure(value))
}

/// Plans already carry the native receipt. Mark their response-wide required
/// failure without performing another check or discarding attempted evidence.
pub fn mark_required_failure(value: &mut serde_json::Value) -> bool {
    let failed = value["source_verification"]["schema"] == "airk.saved-source-verification/1"
        && value["source_verification"]["mode"] == "require"
        && value["source_verification"]["status"] == "requirements_unmet";
    if failed {
        value["error"] = serde_json::json!({"code":"verification_required_unmet","message":"Required saved-source verification did not match every returned file reference.","retryable":false});
    }
    failed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_projection_cannot_keep_receipts_for_removed_evidence() -> Result<()> {
        let mut value = serde_json::json!({"hits":[{"citation":{"path":"a.md"}}],
            "source_verification":{"schema":"airk.saved-source-verification/1",
                "references":[{"result_pointer":"/hits/0/citation"}]}});
        validate_projection(&value)?;
        value["hits"] = serde_json::json!([]);
        assert!(
            validate_projection(&value)
                .unwrap_err()
                .is::<VerificationProjectionChanged>()
        );
        let nested = serde_json::json!({"steps":[{"result":value}]});
        assert!(
            validate_projection(&nested)
                .unwrap_err()
                .is::<VerificationProjectionChanged>()
        );
        validate_projection(&serde_json::json!({"hits":[]}))?;
        let mut plan = serde_json::json!({"steps":[{"result":{"path":"a.md"}}],
            "source_verification":{"schema":"airk.saved-source-verification/1",
                "references":[{"result_pointer":"/steps/0/result"}]}});
        validate_projection(&plan)?;
        plan["steps"][0]["result"] = serde_json::Value::Null;
        assert!(
            validate_projection(&plan)
                .unwrap_err()
                .is::<VerificationProjectionChanged>()
        );
        Ok(())
    }

    #[test]
    fn encoded_file_info_final_bindings_compare_logical_copies_without_changing_raw() -> Result<()>
    {
        use crate::{
            identity::{Incarnation, RevisionToken, Sha256Digest},
            index_catalog::IndexId,
        };
        let index = IndexId::from_identity(Incarnation::from_bytes([21; 16]));
        let other = IndexId::from_identity(Incarnation::from_bytes([22; 16]));
        let publication = crate::evidence::RevisionEnvelope::new(
            "fixture".into(),
            RevisionToken {
                incarnation: Incarnation::from_bytes([23; 16]),
                generation: 1,
                revision: Sha256Digest::from_bytes([24; 32]),
            },
        );
        let logical = serde_json::json!({"path":"same.rs","content_hash":"ab".repeat(32),
            "collection":"fixture".repeat(1024),"publication":publication,"generation":1});
        let mut first = logical.clone();
        let mut second = logical.clone();
        first["publication"] = serde_json::json!({"member":0});
        second["publication"] = serde_json::json!({"member":1});
        let table = crate::combined_reads::header_templates::share_native_headers(&mut [
            &mut first,
            &mut second,
        ])?
        .expect("large repeated headers factor");
        for mode in ["report", "require"] {
            let mut projection = serde_json::json!({"scope_identity":"cd".repeat(32),"steps":[{"result":{
                "schema":"airk.combined-read/1","shared_values":table,
                "publication_vector":{"members":[{"index_id":index,"publication":publication},{"index_id":other,"publication":publication}]},
                "members":[{"index_id":index,"result":first},{"index_id":other,"result":second}]}}],
                "member_verification":{"schema":"airk.combined-verification/1","scope_identity":"cd".repeat(32),"members":[]}});
            for (offset, id) in [index, other].iter().enumerate() {
                projection["member_verification"]["members"].as_array_mut().unwrap().push(serde_json::json!({
                    "index_id":id,"reference_bindings":[{"member_pointer":"/steps/0/result","result_pointer":format!("/steps/0/result/members/{offset}/result")}],
                    "result":{"steps":[{"result":logical}],"source_verification":{"schema":"airk.saved-source-verification/1","mode":mode,
                        "references":[{"result_pointer":"/steps/0/result"}]}}}));
            }
            let raw = projection.clone();
            validate_projection(&projection)?;
            let inventory = ReferenceInventory::collect(&projection);
            assert!(inventory.complete);
            assert!(
                inventory
                    .references
                    .iter()
                    .all(|reference| reference.publication == Some(publication.token))
            );
            assert_eq!(
                projection, raw,
                "decoding never rewrites returned receipt bytes"
            );
            projection["steps"][0]["result"]["shared_values"]["values"] = serde_json::json!({});
            assert!(!ReferenceInventory::collect(&projection).complete);
            assert!(validate_projection(&projection).is_err());
        }
        assert!(!canonical_result_container("/facts/0/attributes"));
        Ok(())
    }

    #[test]
    fn combined_projection_preserves_member_relative_receipts_and_checks_every_member() -> Result<()>
    {
        let member = serde_json::json!({"content":[],"structuredContent":{
            "hits":[{"citation":{"path":"shared/a.rs"}}],
            "source_verification":{"schema":"airk.saved-source-verification/1",
                "references":[{"result_pointer":"/hits/0/citation"}]}}});
        let mut combined = serde_json::json!({"schema":"airk.combined-read/1",
            "members":[{"index_id":crate::index_catalog::IndexId::from_identity(crate::identity::Incarnation::from_bytes([1;16])),"result":member.clone()},
                {"index_id":crate::index_catalog::IndexId::from_identity(crate::identity::Incarnation::from_bytes([2;16])),"result":member}]});
        validate_projection(&combined)?;
        combined["members"][1]["result"]["structuredContent"]["hits"] = serde_json::json!([]);
        assert!(
            validate_projection(&combined)
                .unwrap_err()
                .is::<VerificationProjectionChanged>()
        );
        Ok(())
    }

    #[test]
    fn final_member_checks_bind_the_outer_qualified_projection_and_expire_after_clipping()
    -> Result<()> {
        use crate::{
            identity::{Incarnation, RevisionToken, Sha256Digest},
            index_catalog::{IndexId, QueryProjectId, SourceRootId},
        };
        let index = IndexId::from_identity(Incarnation::from_bytes([3; 16]));
        let other = IndexId::from_identity(Incarnation::from_bytes([4; 16]));
        let citation = serde_json::json!({"path":"same.rs","content_hash":"ab".repeat(32)});
        let origin = serde_json::json!({"index_id":index,"source_root_id":SourceRootId::from_identity(Incarnation::from_bytes([5;16])),
            "project_ids":[QueryProjectId::from_identity(Incarnation::from_bytes([6;16]))],"source_relative_path":"same.rs",
            "publication":crate::evidence::RevisionEnvelope::new("fixture".into(),RevisionToken{incarnation:Incarnation::from_bytes([7;16]),generation:1,revision:Sha256Digest::from_bytes([8;32])})});
        let mut projection = serde_json::json!({"scope_identity":"cd".repeat(32),"evidence":[{"citation":citation,"origin":origin}],
            "member_verification":{"schema":"airk.combined-verification/1","scope_identity":"cd".repeat(32),"members":[{"index_id":index,
                "reference_bindings":[{"member_pointer":"/steps/0/result","result_pointer":"/evidence/0"}],
                "result":{"steps":[{"result":{"citation":citation}}],"source_verification":{"schema":"airk.saved-source-verification/1","references":[{"result_pointer":"/steps/0/result/citation"}]}}}]}});
        validate_projection(&projection)?;
        projection["evidence"][0]["origin"]["index_id"] = serde_json::json!(other);
        assert!(
            validate_projection(&projection).is_err(),
            "identical bytes cannot change originating index"
        );
        projection["evidence"][0]["origin"]["index_id"] = serde_json::json!(index);
        projection["evidence"][0]["citation"]["content_hash"] = serde_json::json!("ef".repeat(32));
        assert!(
            validate_projection(&projection).is_err(),
            "changed source identity expires checked projection"
        );
        projection["evidence"] = serde_json::json!([]);
        assert!(
            validate_projection(&projection).is_err(),
            "late clipping cannot retain an old member claim"
        );
        Ok(())
    }

    #[test]
    fn expanded_plan_reports_validate_every_receipt_through_the_full_bound() -> Result<()> {
        for count in [13, crate::workbench::MAX_STEP_REPORTS] {
            let report = serde_json::json!({"result":{"hits":[{"citation":{"path":"fixture.rs"}}],
                "source_verification":{"schema":"airk.saved-source-verification/1","references":[{"result_pointer":"/hits/0/citation"}]}}});
            let mut value = serde_json::json!({"steps":vec![report.clone();count]});
            validate_projection(&value)?;
            value["steps"][count - 1]["result"]["hits"] = serde_json::json!([]);
            assert!(
                validate_projection(&value).is_err(),
                "late fanout report {count} must remain validated"
            );
        }
        let value = serde_json::json!({"steps":vec![serde_json::json!({});crate::workbench::MAX_STEP_REPORTS+1]});
        assert!(validate_projection(&value).is_err());
        Ok(())
    }

    #[test]
    fn unavailable_final_observation_retains_its_failure_without_a_verification_claim() -> Result<()>
    {
        let mut value = serde_json::json!({"member_verification":{"isError":true,"structuredContent":{"error":{"code":"verification_reply_budget","verification_performed":false}}}});
        validate_projection(&value)?;
        value["member_verification"]["structuredContent"]["source_verification"] =
            serde_json::json!({"schema":"airk.saved-source-verification/1","references":[]});
        assert!(
            validate_projection(&value).is_err(),
            "an unavailable observation cannot carry an unbound dated claim"
        );
        Ok(())
    }

    #[test]
    fn internal_member_allocations_never_expand_public_request_or_reinterpret_zero() -> Result<()> {
        let requested = VerificationControls::default().resolve()?.unwrap();
        let zero = VerificationAllocation {
            max_sources: 0,
            max_bytes: 0,
            deadline_ms: 0,
        };
        assert_eq!(zero.apply(requested)?.max_sources, 0);
        for share in [
            VerificationAllocation {
                max_sources: requested.max_sources + 1,
                ..zero
            },
            VerificationAllocation {
                max_bytes: requested.max_bytes + 1,
                ..zero
            },
            VerificationAllocation {
                deadline_ms: requested.deadline_ms + 1,
                ..zero
            },
        ] {
            assert!(share.apply(requested).is_err());
        }
        assert!(
            VerificationControls {
                verification_max_sources: Some(0),
                ..Default::default()
            }
            .resolve()
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn controls_match_design_fixtures_and_resolve_the_published_report_preset() -> Result<()> {
        let fixtures: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../docs/research/contracts-v190/fixtures.json"
            ))?)?;
        for case in fixtures["scope_cases"].as_array().unwrap() {
            let mut controls = case["value"].as_object().unwrap().clone();
            controls.retain(|key, _| key.starts_with("verification"));
            if controls.is_empty() {
                continue;
            }
            let controls: VerificationControls = serde_json::from_value(controls.into())?;
            assert_eq!(
                controls.resolve().is_ok(),
                case["valid"],
                "{}",
                case["name"]
            );
        }
        assert_eq!(VerificationControls::off().resolve()?, None);
        assert_eq!(
            VerificationControls::default().resolve()?,
            Some(VerificationBudget {
                mode: VerificationMode::Report,
                max_sources: DEFAULT_SOURCES,
                max_bytes: DEFAULT_BYTES,
                deadline_ms: DEFAULT_DEADLINE_MS
            })
        );
        assert!(
            serde_json::from_value::<VerificationControls>(
                serde_json::json!({"verification":null})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<VerificationControls>(
                serde_json::json!({"verification_max_bytes":null})
            )
            .is_err()
        );
        for (sources, bytes, deadline) in [
            (0, 1, 1),
            (257, 1, 1),
            (1, 0, 1),
            (1, MAX_BYTES + 1, 1),
            (1, 1, 0),
            (1, 1, MAX_DEADLINE_MS + 1),
        ] {
            assert!(
                VerificationControls {
                    verification: Some(VerificationMode::Report),
                    verification_max_sources: Some(sources),
                    verification_max_bytes: Some(bytes),
                    verification_deadline_ms: Some(deadline),
                }
                .resolve()
                .is_err()
            );
        }
        Ok(())
    }

    #[test]
    fn typed_receipts_preserve_every_valid_design_example() -> Result<()> {
        let fixtures: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../docs/research/contracts-v190/fixtures.json"
            ))?)?;
        for case in fixtures["receipt_cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["valid"] == true)
        {
            let receipt: VerificationReceipt = serde_json::from_value(case["value"].clone())?;
            assert_eq!(
                serde_json::to_value(receipt)?,
                case["value"],
                "{}",
                case["name"]
            );
        }
        Ok(())
    }
}
