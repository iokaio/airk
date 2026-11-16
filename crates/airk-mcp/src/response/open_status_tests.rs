// SPDX-License-Identifier: Apache-2.0
//! Independent open-session status fixtures; no native service qualification.
use super::OpenStatusSession;
use serde_json::{Map, Value, json};

fn body(scope: u8, incarnation: u8) -> Value {
    use airk_core::identity::{Incarnation, RevisionToken, Sha256Digest};
    let token = RevisionToken {
        incarnation: Incarnation::from_bytes([incarnation; 16]),
        generation: 1,
        revision: Sha256Digest::from_bytes([8; 32]),
    }
    .to_string();
    json!({"collection":"sample","publication":{"token":token},
        "freshness":{"state":"idle","observed_unix_ms":1,"live_source_verified":false},
        "semantic_readiness":{"state":"ready","first_ready_unix_ms":1,"fully_ready_unix_ms":1},
        "indexing":{"level":"complete","note":"retained native detail"},
        "scope_observation":{"schema":1,"qualifier":"selected_subset",
            "fingerprint":Sha256Digest::from_bytes([scope;32]),
            "selection":{"owner":"agent","scope_identity":Sha256Digest::from_bytes([9;32]),"catalog_revision":1},
            "members":[{"index_id":"idx_01010101010101010101010101010101","memberships":[],
                "source_root_ids":[],"source_revision":1,"policy_epoch":0,"policy_digest":null}]},
        "read_scope":{"index_ids":["idx_01010101010101010101010101010101"],"source":"explicit"},
        "source_verification":{"sources":[{"path":"policy.md","status":"matches"}]},
        "hits":[{"path":"policy.md","text":"freshness is source text", "freshness":{"state":"user evidence"},
            "citation":{"content_hash":"exact-original-hash","start_line":7}}]})
}
fn envelope(body: &Value) -> Value {
    json!({"content":[{"type":"text","text":body.to_string()}],"structuredContent":body,"isError":false})
}
fn payload(value: &Value) -> Value {
    let text: Value = serde_json::from_str(value["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, value["structuredContent"]);
    text
}
fn call(session: &OpenStatusSession, tool: &str, arguments: Value, body: &Value) -> Value {
    let mut value = envelope(body);
    session.display(tool, arguments.as_object().unwrap(), &mut value);
    payload(&value)
}

#[test]
fn repeated_open_status_elides_only_unchanged_top_level_fields() {
    let session = OpenStatusSession::default();
    let original = body(1, 2);
    assert_eq!(call(&session, "search", json!({}), &original), original);
    let mut later = original.clone();
    later["freshness"]["observed_unix_ms"] = 99.into();
    later["semantic_readiness"]["first_ready_unix_ms"] = 99.into();
    later["semantic_readiness"]["fully_ready_unix_ms"] = 99.into();
    let actual = call(&session, "airk_search", json!({}), &later);
    let mut expected = later;
    for field in ["freshness", "semantic_readiness", "indexing"] {
        expected.as_object_mut().unwrap().remove(field);
    }
    assert_eq!(actual, expected);
    assert_eq!(actual["hits"], original["hits"]);
    assert_eq!(actual["scope_observation"], original["scope_observation"]);
    assert_eq!(actual["read_scope"], original["read_scope"]);
    assert_eq!(
        actual["source_verification"],
        original["source_verification"]
    );
}

#[test]
fn full_explicit_tools_changes_and_failures_remain_visible() {
    let session = OpenStatusSession::default();
    let original = body(1, 2);
    call(&session, "search", json!({}), &original);
    for (tool, args) in [
        ("search", json!({"response_mode":"full"})),
        ("search", json!({"include_status":true})),
        ("status", json!({})),
        ("airk_capabilities", json!({})),
        ("diagnostics", json!({})),
        ("changes", json!({})),
    ] {
        assert_eq!(call(&session, tool, args, &original), original);
    }
    let mut failed = original.clone();
    failed["semantic_readiness"]["failure"] = "model unavailable".into();
    let actual = call(&session, "search", json!({}), &failed);
    assert_eq!(actual["semantic_readiness"], failed["semantic_readiness"]);
    let mut error = envelope(&failed);
    error["isError"] = true.into();
    session.display("search", &Map::new(), &mut error);
    assert_eq!(payload(&error), failed);
    assert_eq!(error["isError"], true);
}

#[test]
fn status_memory_isolated_by_protocol_session_scope_and_publication_incarnation() {
    let first = OpenStatusSession::default();
    let second = OpenStatusSession::default();
    let original = body(1, 2);
    call(&first, "search", json!({}), &original);
    assert_eq!(call(&second, "search", json!({}), &original), original);
    assert!(
        call(&first, "search", json!({}), &original)
            .get("freshness")
            .is_none()
    );
    let changed_scope = body(3, 2);
    assert_eq!(
        call(&first, "search", json!({}), &changed_scope),
        changed_scope
    );
    assert_eq!(call(&first, "search", json!({}), &original), original);
    let rebuilt = body(1, 4);
    assert_eq!(call(&first, "search", json!({}), &rebuilt), rebuilt);
    assert!(
        call(&first, "search", json!({}), &rebuilt)
            .get("freshness")
            .is_none()
    );
}

#[test]
fn bound_combined_and_disagreeing_representations_are_preserved_exactly() {
    let session = OpenStatusSession::default();
    let original = body(1, 2);
    call(&session, "search", json!({}), &original);
    for mut combined in [original.clone(), original.clone()] {
        combined["schema"] = "airk.combined-read/1".into();
        combined["delivery_observation"] = json!({"binding":"exact-byte-bound"});
        combined["members"] = json!([{"result":original}]);
        let mut value = envelope(&combined);
        let exact = serde_json::to_vec(&value).unwrap();
        session.display("search", &Map::new(), &mut value);
        assert_eq!(serde_json::to_vec(&value).unwrap(), exact);
    }
    let mut mismatched = envelope(&original);
    mismatched["structuredContent"]["freshness"]["state"] = "different".into();
    let exact = mismatched.clone();
    session.display("search", &Map::new(), &mut mismatched);
    assert_eq!(mismatched, exact);
    let mut arbitrary = envelope(&json!({"freshness":{"state":"user content"},"hits":[original]}));
    let exact = arbitrary.clone();
    session.display("search", &Map::new(), &mut arbitrary);
    assert_eq!(arbitrary, exact);
}

#[test]
fn text_only_structured_only_and_invalid_scope_preserve_native_shape() {
    let session = OpenStatusSession::default();
    let original = body(1, 2);
    let mut text_only = envelope(&original);
    text_only
        .as_object_mut()
        .unwrap()
        .remove("structuredContent");
    session.display("search", &Map::new(), &mut text_only);
    assert!(text_only.get("structuredContent").is_none());
    let mut repeated = text_only.clone();
    session.display("search", &Map::new(), &mut repeated);
    let repeated_body: Value =
        serde_json::from_str(repeated["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(repeated_body.get("freshness").is_none());
    let mut structured = json!({"content":[],"structuredContent":original,"isError":false});
    session.display("search", &Map::new(), &mut structured);
    assert!(structured["structuredContent"].get("freshness").is_none());
    assert_eq!(structured["content"], json!([]));
    let mut invalid = body(1, 2);
    invalid["scope_observation"]["schema"] = 99.into();
    assert_eq!(call(&session, "search", json!({}), &invalid), invalid);
}
