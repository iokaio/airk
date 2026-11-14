// SPDX-License-Identifier: Apache-2.0
mod common;
use common::parity::strict::{self, Normalization, Snapshot, Source};
use serde_json::{Value, json};

fn document() -> Value {
    json!({"request_id":"11111111111111111111111111111111","observed_unix_ms":100,
        "elapsed_ms":10,"evidence":{"content_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "model_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "publication":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "revision":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
        "text":"11111111111111111111111111111111","count":4},"continuation":null})
}
fn rules(request: &str) -> Normalization {
    Normalization {
        identities: [("/request_id".into(), request.into())].into(),
        timing: vec!["/observed_unix_ms".into(), "/elapsed_ms".into()],
    }
}
#[test]
fn strict_parity_normalizes_only_explicit_transport_fields_and_observation_timing() {
    let original = document();
    let mut changed = original.clone();
    changed["request_id"] = json!("22222222222222222222222222222222");
    changed["observed_unix_ms"] = json!(200);
    changed["elapsed_ms"] = json!(20);
    assert!(
        strict::equivalent(
            &original,
            &rules("11111111111111111111111111111111"),
            &changed,
            &rules("22222222222222222222222222222222")
        )
        .unwrap()
    );
    assert_eq!(
        strict::normalize(&original, &rules("11111111111111111111111111111111")).unwrap()["evidence"]
            ["text"],
        "11111111111111111111111111111111",
        "same token elsewhere is stable content"
    );
}
#[test]
fn strict_parity_stable_digest_and_full_result_mutations_must_fail() {
    for pointer in [
        "/evidence/content_hash",
        "/evidence/model_sha256",
        "/evidence/publication",
        "/evidence/revision",
        "/evidence/text",
    ] {
        let original = document();
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() =
            json!("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");
        assert!(
            !strict::equivalent(
                &original,
                &rules("11111111111111111111111111111111"),
                &changed,
                &rules("11111111111111111111111111111111")
            )
            .unwrap(),
            "{pointer}"
        );
    }
    let original = document();
    for changed in [
        json!({"request_id":"11111111111111111111111111111111","observed_unix_ms":100,"elapsed_ms":10}),
        {
            let mut value = original.clone();
            value["continuation"] = json!({"next":true});
            value
        },
    ] {
        assert!(
            !strict::equivalent(
                &original,
                &rules("11111111111111111111111111111111"),
                &changed,
                &rules("11111111111111111111111111111111")
            )
            .unwrap()
        );
    }
}
#[test]
fn strict_parity_rejects_stable_field_masks_unknown_timing_and_wrong_transport_context() {
    let original = document();
    let invalid = Normalization {
        identities: [("/evidence/content_hash".into(), "a".repeat(64))].into(),
        timing: vec![],
    };
    assert!(strict::normalize(&original, &invalid).is_err());
    let invalid = Normalization {
        identities: Default::default(),
        timing: vec!["/evidence/revision".into()],
    };
    assert!(strict::normalize(&original, &invalid).is_err());
    assert!(strict::normalize(&original, &rules("wrong-context")).is_err());
    let documentary =
        json!({"evidence":{"document":{"request_id":"x","service_instance":"y","elapsed_ms":7}}});
    for (pointer, observed) in [
        ("/evidence/document/request_id", "x"),
        ("/evidence/document/service_instance", "y"),
    ] {
        let invalid = Normalization {
            identities: [(pointer.into(), observed.into())].into(),
            timing: vec![],
        };
        assert!(
            strict::normalize(&documentary, &invalid).is_err(),
            "{pointer}"
        );
    }
    let invalid = Normalization {
        identities: Default::default(),
        timing: vec!["/evidence/document/elapsed_ms".into()],
    };
    assert!(strict::normalize(&documentary, &invalid).is_err());
}
#[test]
fn strict_parity_raw_capture_preserves_digests_and_never_reinterprets_or_overwrites_legacy() {
    let owner = tempfile::tempdir().unwrap();
    let path = owner.path().join("strict.json");
    let source = Source {
        runtime_manifest_sha256: "1".repeat(64),
        service_sha256: "2".repeat(64),
        cli_sha256: "3".repeat(64),
    };
    strict::capture(&path, source.clone(), document()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let record: Snapshot = serde_json::from_slice(&bytes).unwrap();
    record.validate().unwrap();
    assert_eq!(record.value, document());
    assert_eq!(record.source.service_sha256, "2".repeat(64));
    assert!(strict::capture(&path, source, document()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    let mut legacy = record;
    legacy.schema = "airk.split.parity/1".into();
    assert!(legacy.validate().is_err());
}
