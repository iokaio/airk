// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::json;

#[test]
fn required_verification_failure_preserves_complete_text_and_structured_data() {
    let body = json!({"error":{"code":"verification_required_unmet","retryable":false},"publication":{"token":"exact-saved-publication"},"scope_observation":{"fingerprint":"exact-scope"},"source_verification":{"status":"requirements_unmet","sources":[{"state":"mismatch","expected_hash":"old","observed_hash":"new"}],"future_receipt":{"preserved":true}}});
    let text = body.to_string();
    let result = complete_public_envelope(CallToolResult::error(vec![ContentBlock::text(
        text.clone(),
    )]));
    assert_eq!(result.is_error, Some(true));
    assert_eq!(result.structured_content, Some(body));
    assert_eq!(result.content[0].as_text().unwrap().text, text);
    assert_eq!(
        serde_json::to_value(complete_public_envelope(result.clone())).unwrap(),
        serde_json::to_value(result).unwrap()
    );
}

#[test]
fn duplicated_public_data_refuses_complete_wire_overflow_without_truncating_sources() {
    let body = json!({"source_verification":{"sources":[{"evidence":"x".repeat(PUBLIC_TOOL_ENVELOPE_BYTES / 2)}]},"scope_observation":{"fingerprint":"exact-scope"}});
    let result = complete_public_envelope(CallToolResult::error(vec![ContentBlock::text(
        body.to_string(),
    )]));
    let refusal = result.structured_content.as_ref().unwrap();
    assert_eq!(result.is_error, Some(true));
    assert_eq!(refusal["error"]["code"], "scope_reply_budget");
    assert_eq!(refusal["error"]["retryable"], false);
    assert_eq!(refusal["scope_observation"], body["scope_observation"]);
    assert!(
        refusal.get("source_verification").is_none(),
        "oversize must be a typed refusal, never partial evidence"
    );
    assert!(serde_json::to_vec(&result).unwrap().len() < airk_core::MAX_RESPONSE_BYTES);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&result.content[0].as_text().unwrap().text)
            .unwrap(),
        *refusal
    );
}

#[test]
fn already_finalized_envelope_keeps_its_own_budget_and_exact_delivery_receipt() {
    let body = json!({"items":[{"evidence":"x".repeat(airk_core::MAX_RESPONSE_BYTES)}],"delivery_observation":{"digest_basis":"canonical_body_without_delivery_observation","receipt":{"result_digest":"preserve-native-receipt"}}});
    let result = CallToolResult::structured(body);
    let before = serde_json::to_value(&result).unwrap();
    assert!(serde_json::to_vec(&result).unwrap().len() > airk_core::MAX_RESPONSE_BYTES);
    assert_eq!(
        serde_json::to_value(complete_public_envelope(result)).unwrap(),
        before
    );
}

#[test]
fn complete_native_body_can_use_both_representations_above_single_frame_size() {
    let body = json!({"capabilities":{"retained":"x".repeat(airk_core::MAX_RESPONSE_BYTES / 2)},"scope_observation":{"fingerprint":"same-scope"}});
    let result = complete_public_envelope(CallToolResult::success(vec![ContentBlock::text(
        body.to_string(),
    )]));
    assert_eq!(result.structured_content, Some(body.clone()));
    assert_ne!(result.is_error, Some(true));
    let bytes = serde_json::to_vec(&result).unwrap();
    assert!(bytes.len() > airk_core::MAX_RESPONSE_BYTES);
    assert!(bytes.len() < PUBLIC_TOOL_ENVELOPE_BYTES);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&result.content[0].as_text().unwrap().text)
            .unwrap(),
        body
    );
}
