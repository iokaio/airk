// SPDX-License-Identifier: Apache-2.0
//! Independent response identity/export regressions; no connection-only tokens.
use super::{ResponseMode, Session};
use serde_json::{Value, json};

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
