// SPDX-License-Identifier: Apache-2.0
//! Independent byte and recovery cases; no native publication is exercised.
use super::*;
use serde_json::{Value, json};

macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!(
            "../../../../../../fixtures/split/agents/config_edits/",
            $name
        ))
    };
}

fn bytes(name: &str) -> &'static [u8] {
    match name {
        "linux-prepared.json" => fixture!("linux-prepared.json"),
        "linux-started.json" => fixture!("linux-started.json"),
        "linux-published.json" => fixture!("linux-published.json"),
        "linux-conflict.json" => fixture!("linux-conflict.json"),
        "linux-unknown.json" => fixture!("linux-unknown.json"),
        "linux-deletion-refused.json" => fixture!("linux-deletion-refused.json"),
        "linux-open-writer-oversized.json" => fixture!("linux-open-writer-oversized.json"),
        "macos-published.json" => fixture!("macos-published.json"),
        "windows-published.json" => fixture!("windows-published.json"),
        "windows-error-1177.json" => fixture!("windows-error-1177.json"),
        "marker-unix.json" => fixture!("marker-unix.json"),
        "marker-windows.json" => fixture!("marker-windows.json"),
        _ => panic!("fixture not independently registered: {name}"),
    }
}

fn cases() -> Value {
    serde_json::from_slice(fixture!("cases.json")).unwrap()
}

fn replace(value: &mut Value, pointer: &str, replacement: Value) {
    let (parent, key) = pointer.rsplit_once('/').unwrap();
    value
        .pointer_mut(parent)
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert(key.into(), replacement);
}

fn record(name: &str) -> Receipt {
    Receipt::decode(bytes(name)).unwrap_or_else(|error| panic!("{name}: {error}"))
}

#[test]
fn retained_edit_exact_bytes_match_independent_records_and_markers() {
    let cases = cases();
    assert_eq!(cases["schema"], "airk.agent-config-edit-cases/1");
    assert_eq!(cases["valid"].as_array().unwrap().len(), 10);
    for case in cases["valid"].as_array().unwrap() {
        let name = case["file"].as_str().unwrap();
        let receipt = record(name);
        assert_eq!(
            serde_json::to_value(receipt.state).unwrap(),
            case["state"],
            "{name}"
        );
        assert_eq!(
            receipt.sequence,
            case["sequence"].as_u64().unwrap(),
            "{name}"
        );
        assert_eq!(
            receipt.encode().unwrap(),
            bytes(name),
            "exact encoding: {name}"
        );
    }
    for name in ["marker-unix.json", "marker-windows.json"] {
        let marker = RootMarker::decode(bytes(name)).unwrap();
        assert_eq!(
            marker.encode().unwrap(),
            bytes(name),
            "exact encoding: {name}"
        );
    }
}

#[test]
fn retained_edit_malformed_version_identity_and_path_cases_refuse() {
    for case in cases()["malformed"].as_array().unwrap() {
        let mut value: Value = serde_json::from_slice(bytes("linux-prepared.json")).unwrap();
        replace(
            &mut value,
            case["pointer"].as_str().unwrap(),
            case["value"].clone(),
        );
        assert!(
            Receipt::decode(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{}",
            case["name"]
        );
    }
    for key in [
        "schema",
        "transaction",
        "owner",
        "root",
        "parent",
        "target_basename",
        "operation",
        "original",
        "prepared",
        "primitive",
        "state",
        "sequence",
    ] {
        let mut value: Value = serde_json::from_slice(bytes("linux-prepared.json")).unwrap();
        value.as_object_mut().unwrap().remove(key);
        assert!(
            Receipt::decode(&serde_json::to_vec(&value).unwrap()).is_err(),
            "missing {key}"
        );
    }
    let source = std::str::from_utf8(bytes("linux-prepared.json")).unwrap();
    for duplicate in [
        source.replacen("\"sequence\":0", "\"sequence\":0,\"sequence\":0", 1),
        source.replacen("\"length\":3", "\"length\":3,\"length\":3", 1),
    ] {
        assert!(
            Receipt::decode(duplicate.as_bytes()).is_err(),
            "duplicate identity metadata"
        );
    }
}

#[test]
fn retained_edit_recovery_revalidates_private_binding_without_authorizing_restore() {
    let marker = RootMarker::decode(bytes("marker-unix.json")).unwrap();
    let original = record("linux-published.json");
    for case in cases()["recovery_binding"].as_array().unwrap() {
        let mut value = serde_json::to_value(&original).unwrap();
        if !case["change"].is_null() {
            replace(
                &mut value,
                case["change"]["pointer"].as_str().unwrap(),
                case["change"]["value"].clone(),
            );
        }
        let accepted = Receipt::decode(&serde_json::to_vec(&value).unwrap())
            .and_then(|receipt| {
                receipt
                    .validate_binding(&marker, original.transaction, &original.target_basename)
                    .map_err(str::to_owned)
            })
            .is_ok();
        assert_eq!(
            accepted,
            case["accepted"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
    for name in [
        "linux-prepared.json",
        "linux-started.json",
        "linux-published.json",
        "linux-conflict.json",
        "linux-unknown.json",
        "linux-deletion-refused.json",
        "linux-open-writer-oversized.json",
    ] {
        record(name)
            .validate_binding(&marker, original.transaction, &original.target_basename)
            .unwrap();
    }
    // A late deletion remains absence; an enlarged open-writer object is
    // recorded without a fabricated full hash. No record supplies restore paths.
    assert_eq!(
        record("linux-deletion-refused.json").after.unwrap().target,
        Observation::Absent
    );
    assert!(matches!(
        record("linux-open-writer-oversized.json")
            .after
            .unwrap()
            .prepared,
        Observation::Incomplete {
            length: Some(67_108_865),
            observed_bytes: 1_048_576,
            reason: IncompleteReason::Oversized,
            ..
        }
    ));
    let uncertain = record("windows-error-1177.json");
    assert_eq!(uncertain.state, State::OutcomeUnknown);
    assert_eq!(uncertain.after.unwrap().native_error, Some(1177));
}

#[test]
fn retained_edit_successors_preserve_lineage_and_never_replay_publication() {
    for case in cases()["successors"].as_array().unwrap() {
        let before = record(case["from"].as_str().unwrap());
        let after = record(case["to"].as_str().unwrap());
        assert_eq!(
            before.validate_successor(&after).is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{case}"
        );
    }
    let prepared = record("linux-prepared.json");
    for (pointer, replacement) in [
        ("/transaction", json!("02".repeat(16))),
        ("/operation", json!("repair")),
        ("/target_basename", json!("other.json")),
        ("/original/sha256", json!("03".repeat(32))),
        ("/prepared/sha256", json!("04".repeat(32))),
        ("/root/inode", json!(200)),
        ("/parent/inode", json!(201)),
    ] {
        let mut value: Value = serde_json::from_slice(bytes("linux-started.json")).unwrap();
        replace(&mut value, pointer, replacement);
        let changed = Receipt::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            prepared.validate_successor(&changed).is_err(),
            "changed lineage: {pointer}"
        );
    }
    let unknown = record("linux-unknown.json");
    let mut replay = record("linux-started.json");
    replay.sequence = 3;
    assert!(
        unknown.validate_successor(&replay).is_err(),
        "uncertain publication never becomes replayable"
    );
}

#[test]
fn retained_edit_admission_uses_independent_count_byte_reservation_and_disk_boundaries() {
    for case in cases()["admission"].as_array().unwrap() {
        assert_eq!(
            admit(
                case["transactions"].as_u64().unwrap(),
                case["observed"].as_u64().unwrap(),
                case["reserved"].as_u64().unwrap(),
                case["available"].as_u64().unwrap()
            )
            .is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{case}"
        );
    }
}

#[test]
fn retained_edit_post_publication_claims_require_complete_matching_evidence() {
    for (name, pointer, replacement) in [
        (
            "linux-published.json",
            "/after/target/file/sha256",
            json!("05".repeat(32)),
        ),
        (
            "linux-published.json",
            "/after/prepared/file/sha256",
            json!("05".repeat(32)),
        ),
        ("linux-published.json", "/after", Value::Null),
        (
            "linux-conflict.json",
            "/after/prepared",
            json!({"status":"absent"}),
        ),
        (
            "linux-conflict.json",
            "/after/prepared",
            json!({"status":"incomplete","identity":null,"length":null,"observed_bytes":0,"reason":"unreadable"}),
        ),
        (
            "linux-published.json",
            "/after/before",
            json!({"status":"complete","file":serde_json::to_value(record("linux-published.json").original).unwrap()}),
        ),
        (
            "linux-open-writer-oversized.json",
            "/after/prepared/observed_bytes",
            json!(1_048_577),
        ),
        (
            "linux-open-writer-oversized.json",
            "/after/prepared/length",
            json!(1_048_576),
        ),
        (
            "linux-open-writer-oversized.json",
            "/after/prepared/sha256",
            json!("05".repeat(32)),
        ),
        (
            "linux-open-writer-oversized.json",
            "/after/prepared/identity/inode",
            json!(100),
        ),
        (
            "windows-published.json",
            "/after/before/file/length",
            json!(1_048_577),
        ),
    ] {
        let mut value: Value = serde_json::from_slice(bytes(name)).unwrap();
        replace(&mut value, pointer, replacement);
        assert!(
            Receipt::decode(&serde_json::to_vec(&value).unwrap()).is_err(),
            "{name}: {pointer}"
        );
    }
    let mut conflict = record("linux-conflict.json");
    conflict.after.as_mut().unwrap().prepared = Observation::Complete {
        file: conflict.original.clone(),
    };
    assert!(
        conflict.validate().is_err(),
        "matching reviewed object is not a conflict observation"
    );
}

#[test]
fn retained_edit_root_versions_owner_platforms_and_basename_limits_refuse() {
    for (pointer, replacement) in [
        ("/schema", json!("airk.agent-config-edit-root/2")),
        ("/root/inode", json!(90)),
        ("/root/device", json!(8)),
        ("/restore_path", json!("/tmp/mcp.json")),
    ] {
        let mut marker: Value = serde_json::from_slice(bytes("marker-unix.json")).unwrap();
        replace(&mut marker, pointer, replacement);
        assert!(
            RootMarker::decode(&serde_json::to_vec(&marker).unwrap()).is_err(),
            "{pointer}"
        );
    }
    for sid in [
        "S-2-5-18",
        "S-1-05-18",
        "S-1-5--18",
        "S-1-5-4294967296",
        "S-1-281474976710656-18",
        "S-1-5-18/other",
    ] {
        let mut marker: Value = serde_json::from_slice(bytes("marker-windows.json")).unwrap();
        replace(&mut marker, "/owner/sid", json!(sid));
        assert!(
            RootMarker::decode(&serde_json::to_vec(&marker).unwrap()).is_err(),
            "{sid}"
        );
    }
    let mut receipt = record("windows-published.json");
    for basename in [
        "NUL",
        "con.json",
        "COM1.json",
        "LPT³",
        "mcp.json.",
        "mcp.json ",
        "",
        "..",
        "a\0b",
    ] {
        receipt.target_basename = basename.into();
        assert!(receipt.validate().is_err(), "{basename:?}");
    }
    receipt.target_basename = "a".repeat(255);
    receipt.validate().unwrap();
    receipt.target_basename.push('a');
    assert!(receipt.validate().is_err());
    receipt.target_basename = "配".repeat(86);
    assert!(receipt.validate().is_err(), "basename bound is UTF-8 bytes");
}

#[test]
fn retained_edit_decoders_bound_raw_bytes_before_parsing() {
    let mut receipt = bytes("linux-prepared.json").to_vec();
    receipt.resize(65_536, b' ');
    assert!(Receipt::decode(&receipt).is_ok());
    receipt.push(b' ');
    assert!(Receipt::decode(&receipt).is_err());
    let mut marker = bytes("marker-unix.json").to_vec();
    marker.resize(65_536, b' ');
    assert!(RootMarker::decode(&marker).is_ok());
    marker.push(b' ');
    assert!(RootMarker::decode(&marker).is_err());
    assert!(Receipt::decode(b"{").is_err());
    assert!(RootMarker::decode(b"null").is_err());
}
