// SPDX-License-Identifier: Apache-2.0
//! Per-search execution observations; persisted coverage remains independent.
use crate::{
    evidence::RevisionEnvelope,
    readiness::{DiscoveryReadiness, SemanticReadiness},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestedMode {
    Default,
    Lexical,
    Vector,
    Hybrid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServedMode {
    Lexical,
    Vector,
    Hybrid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncompleteReason {
    HelperUnavailable,
    SemanticCoverageIncomplete,
    SemanticUpgradePending,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchExecution {
    pub version: u32,
    pub requested_mode: RequestedMode,
    pub served_mode: ServedMode,
    /// A semantic component was omitted or its indexed coverage was incomplete.
    /// Count/text clipping remains the separate SearchResponse::truncated flag.
    pub incomplete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incomplete_reason: Option<IncompleteReason>,
}

impl SearchExecution {
    pub fn new(
        requested_mode: RequestedMode,
        served_mode: ServedMode,
        incomplete_reason: Option<IncompleteReason>,
    ) -> Self {
        Self {
            version: 1,
            requested_mode,
            served_mode,
            incomplete: incomplete_reason.is_some(),
            incomplete_reason,
        }
    }
}

/// Bounded context attached only to a typed helper-availability failure.
/// The underlying transport cause remains in the error chain, not in the receipt.
#[derive(Clone, Debug, Serialize)]
pub struct SearchUnavailable {
    pub requested_mode: RequestedMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publication: Option<RevisionEnvelope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_readiness: Option<SemanticReadiness>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_readiness: Option<DiscoveryReadiness>,
}

impl std::fmt::Display for SearchUnavailable {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("The embedding helper is unavailable for this search.")
    }
}
impl std::error::Error for SearchUnavailable {}

/// One receipt for native adapters. It never authorizes automatic read replay.
pub fn helper_unavailable_receipt(error: &anyhow::Error) -> Option<serde_json::Value> {
    let context = error.downcast_ref::<SearchUnavailable>()?;
    let mut value = serde_json::to_value(context).ok()?;
    value.as_object_mut()?.insert(
        "error".into(),
        serde_json::json!({
            "code": "helper_unavailable",
            "message": context.to_string(),
            "retryable": false,
            "action": "retry_later_without_replay"
        }),
    );
    crate::guidance::annotate(&mut value);
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_preserves_typed_context_without_exposing_underlying_error_or_replay() {
        let error =
            anyhow::anyhow!("private model path and request details").context(SearchUnavailable {
                requested_mode: RequestedMode::Vector,
                publication: None,
                semantic_readiness: None,
                discovery_readiness: None,
            });
        let mut receipt = helper_unavailable_receipt(&error).unwrap();
        assert!(!receipt.to_string().contains("private model"));
        receipt.as_object_mut().unwrap().remove("guidance");
        assert_eq!(
            receipt,
            serde_json::json!({
                "requested_mode":"vector",
                "error": {
                    "code":"helper_unavailable",
                    "message":"The embedding helper is unavailable for this search.",
                    "retryable":false,
                    "action":"retry_later_without_replay"
                }
            })
        );
        assert!(
            error
                .chain()
                .any(|cause| cause.to_string().contains("private model"))
        );
        assert!(helper_unavailable_receipt(&anyhow::anyhow!("ordinary failure")).is_none());
    }
}
