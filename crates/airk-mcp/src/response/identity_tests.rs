// SPDX-License-Identifier: Apache-2.0
//! Independent response identity/export regressions; no connection-only tokens.
use super::{ResponseMode, Session};
use serde_json::{Value, json};

#[test]
fn permanent_policy_refusal_retains_the_canonical_typed_receipt_through_context() {
    let refusal = airk_kernel::SourcePolicyReadUnavailable;
    let error = anyhow::Error::new(refusal).context("retained native request refused");
    let reply = super::error_result(&error);
    assert_eq!(reply.is_error, Some(true));
    let bodies: Vec<Value> = reply
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .map(|text| serde_json::from_str(&text.text).unwrap())
        .collect();
    assert_eq!(bodies, vec![refusal.receipt()]);
    assert_eq!(bodies[0]["error"]["code"], "source_policy_unavailable");
    assert_eq!(bodies[0]["error"]["retryable"], false);
}

#[test]
fn status_receipts_do_not_cross_frozen_authority() {
    use airk_core::identity::Sha256Digest;
    let first = Session::for_scope(Some(Sha256Digest::from_bytes([1; 32])));
    let second = Session::for_scope(Some(Sha256Digest::from_bytes([2; 32])));
    let mut observed = evidence("same", "same", "same");
    first.display(&mut observed, ResponseMode::Terse, false);
    second.restore(first.receipt());
    let mut fresh = evidence("same", "same", "same");
    second.display(&mut fresh, ResponseMode::Terse, false);
    assert!(fresh.get("semantic_readiness").is_some());
    second.restore(second.receipt());
    let mut repeated = evidence("same", "same", "same");
    second.display(&mut repeated, ResponseMode::Terse, false);
    assert!(repeated.get("semantic_readiness").is_none());
    let legacy = Session::default();
    legacy.restore(second.receipt());
    let mut fresh = evidence("same", "same", "same");
    legacy.display(&mut fresh, ResponseMode::Terse, false);
    assert!(fresh.get("semantic_readiness").is_some());
}

fn evidence(token: &str, collection: &str, incarnation: &str) -> Value {
    json!({"collection":collection,
        "publication":{"token":token,"incarnation":incarnation,"generation":7},
        "freshness":{"state":"idle","observed_unix_ms":1,"live_source_verified":false},
        "semantic_readiness":{"state":"ready","completed_units":1},
        "read":{"validator":"exact-source-validator","original_root":"/fictional/source",
            "publication":{"token":token,"incarnation":incarnation},
            "current_publication":{"token":token,"incarnation":incarnation}},
        "citation":{"path":"src/fixture.rs","content_hash":"unchanged-source-hash",
            "start_line":3,"end_line":8}})
}

#[test]
fn search_execution_is_retained_on_every_terse_reply_without_forcing_full_status() {
    let session = Session::default();
    let healthy =
        json!({"version":1,"requested_mode":"default","served_mode":"hybrid","incomplete":false});
    let degraded = json!({"version":1,"requested_mode":"default","served_mode":"lexical",
        "incomplete":true,"incomplete_reason":"helper_unavailable"});
    for (index, execution) in [&healthy, &degraded, &healthy].into_iter().enumerate() {
        let mut response = evidence("same-publication", "same-index", "same-owner");
        response["generation"] = json!(7);
        response["hits"] = json!([]);
        response["execution"] = execution.clone();
        session.display(&mut response, ResponseMode::Terse, false);
        assert_eq!(&response["execution"], execution);
        assert_eq!(response["publication"], json!({"token":"same-publication"}));
        assert!(response.get("generation").is_none());
        if index > 0 {
            assert!(response.get("semantic_readiness").is_none());
            assert!(response.get("freshness").is_none());
        }
        let exported: Value =
            serde_json::from_slice(&serde_json::to_vec(&response).unwrap()).unwrap();
        assert_eq!(&exported["execution"], execution);
        assert_eq!(
            exported["citation"]["content_hash"],
            "unchanged-source-hash"
        );
    }
}

#[test]
fn runtime_execution_diagnostics_keep_their_existing_explicit_detail() {
    let session = Session::default();
    let mut response = evidence("same-publication", "same-index", "same-owner");
    response["generation"] = json!(7);
    response["execution"] = json!({"runtime":"native","bounded":true});
    let expected = response.clone();
    session.display(&mut response, ResponseMode::Terse, false);
    assert_eq!(response, expected);
}

#[test]
fn every_terse_reply_remains_independently_exportable_after_repeated_status() {
    let session = Session::default();
    let mut first = evidence("publication-a", "first", "owner-a");
    session.display(&mut first, ResponseMode::Terse, false);
    for token in ["publication-a", "publication-b", "publication-a"] {
        let mut next = evidence(token, "first", "owner-a");
        session.display(&mut next, ResponseMode::Terse, false);
        // Reordering or copying one response does not require any previous reply.
        let exported: Value = serde_json::from_slice(&serde_json::to_vec(&next).unwrap()).unwrap();
        assert_eq!(exported["publication"]["token"], token);
        assert_eq!(exported["read"]["publication"]["ref"], "publication");
        assert_eq!(exported["read"]["original_root"], "/fictional/source");
        assert_eq!(exported["read"]["validator"], "exact-source-validator");
        assert_eq!(
            exported["citation"]["content_hash"],
            "unchanged-source-hash"
        );
    }
}

#[test]
fn restoring_status_receipt_does_not_alias_project_or_runtime_identity() {
    let original = Session::default();
    let mut first = evidence("publication-a", "first", "owner-a");
    original.display(&mut first, ResponseMode::Terse, false);
    let receipt = original.receipt();
    for (collection, incarnation) in [("second", "owner-a"), ("first", "owner-b")] {
        let next = Session::default();
        next.restore(receipt);
        let mut changed = evidence("new-publication", collection, incarnation);
        next.display(&mut changed, ResponseMode::Terse, false);
        assert!(changed.get("freshness").is_some());
        assert!(changed.get("semantic_readiness").is_some());
        assert_eq!(changed["publication"]["token"], "new-publication");
    }
}

#[test]
fn explicit_status_and_full_replies_restore_metadata_without_mutating_source_identity() {
    let session = Session::default();
    let original = evidence("publication-a", "first", "owner-a");
    let mut initial = original.clone();
    session.display(&mut initial, ResponseMode::Terse, false);
    let mut terse = original.clone();
    session.display(&mut terse, ResponseMode::Terse, false);
    assert!(terse.get("freshness").is_none());
    let mut requested = original.clone();
    session.display(&mut requested, ResponseMode::Terse, true);
    assert!(requested.get("freshness").is_some());
    assert_eq!(requested["citation"], original["citation"]);
    let mut full = original.clone();
    session.display(&mut full, ResponseMode::Full, false);
    assert_eq!(full, original);
}

#[test]
fn flattened_conditional_file_family_rows_and_unknown_fields_survive_display_modes() {
    let report = serde_json::json!({"schema":"airk.profile-families/1","families":airk_core::profile_families::Family::ALL.map(|family|serde_json::json!({"family":family,"support":"unknown","retention":"selected","reply_inclusion":"omitted"})),"future_native_field":{"complete":true}});
    for mode in [ResponseMode::Terse, ResponseMode::Full] {
        for detail in [
            None,
            Some(airk_core::reply::ReplyDetail::Lean),
            Some(airk_core::reply::ReplyDetail::Rich),
        ] {
            // Use the production serializer: ConditionalRead flattens the
            // payload. This remains a display-only fixture; actual native
            // transport parity is covered by CLI combined_reads.
            let mut value = serde_json::to_value(airk_core::evidence::ConditionalRead {
                read: airk_core::evidence::ReadEvidence {
                    publication: None,
                    current_publication: None,
                    historical: false,
                    original_root: None,
                    status: airk_core::evidence::EvidenceReadStatus::Content,
                    requested_path: "api/policy.md".into(),
                    resolved_path: Some("api/policy.md".into()),
                    validator: None,
                    basis: airk_core::changes::ContinuityBasis::SamePathObservation,
                    reason: None,
                },
                content: Some(serde_json::json!({
                    "path":"api/policy.md",
                    "profile_families":report,
                    "future_native_payload_field":{"complete":true}
                })),
            })
            .unwrap();
            assert!(value.get("content").is_none());
            assert_eq!(value["profile_families"], report);
            airk_core::reply::ReplyDetail::annotate(&mut value, detail);
            Session::default().display(&mut value, mode, false);
            assert_eq!(value["profile_families"], report);
            assert_eq!(value["path"], "api/policy.md");
            assert_eq!(value["read"]["status"], "content");
            assert_eq!(
                value["future_native_payload_field"],
                serde_json::json!({"complete":true})
            );
        }
    }
}

#[test]
fn typed_contention_and_generic_read_failures_keep_distinct_machine_readable_contracts() {
    for error in [
        anyhow::anyhow!("native publication admission")
            .context(airk_kernel::CurrentGateContention::Applying),
        anyhow::Error::new(std::fs::TryLockError::WouldBlock)
            .context(airk_kernel::CurrentGateContention::Applying),
    ] {
        let expected =
            serde_json::to_value(airk_kernel::retrieval_retry::read_retry_hint(&error).unwrap())
                .unwrap();
        let reply = super::typed_error_result(&error.context("outer read boundary")).unwrap();
        let body: Value = serde_json::from_str(&reply.content[0].as_text().unwrap().text).unwrap();
        assert_eq!(body, expected);
        assert_eq!(body["action"], "repeat_same_request");
    }
    for text in [
        "selection_busy",
        "quota_ledger_busy",
        "invalid current relationship query",
    ] {
        let error = anyhow::anyhow!(text);
        assert!(super::typed_error_result(&error).is_none());
        let reply = super::error_result(&error);
        let body: Value = serde_json::from_str(&reply.content[0].as_text().unwrap().text).unwrap();
        assert_eq!(body["error"]["code"], "read_failed");
        assert_eq!(body["error"]["retryable"], false);
        assert_eq!(body["error"]["message"], text);
    }
    let error = anyhow::anyhow!("{}tail-sentinel", "🙂".repeat(2000));
    let reply = super::error_result(&error);
    let body: Value = serde_json::from_str(&reply.content[0].as_text().unwrap().text).unwrap();
    assert!(body["error"]["message"].as_str().unwrap().len() <= 1024);
    assert_eq!(body["error"]["message_truncated"], true);
    assert!(!body.to_string().contains("tail-sentinel"));
}
