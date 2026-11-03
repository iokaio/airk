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
        for step in steps.iter().take(12) {
            if let Some(result) = step.get("result") {
                validate_projection(result)?;
            }
        }
    }
    Ok(())
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
