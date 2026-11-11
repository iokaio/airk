// SPDX-License-Identifier: Apache-2.0
//! Golden wire fixtures for simplification plan S1.2/S1.3: the exact bytes of
//! one control read (`catalog_receipt`), its persisted receipt payload
//! (`index_operation_receipts.payload` is `serde_json::to_vec(&CatalogReceipt)`)
//! and the operation-id digest vector, plus reviewed malformed inputs. The
//! checked-in file is the reviewed expectation. On a mismatch this test writes
//! the observed rendering to a temporary file for review and fails. It never
//! rewrites the fixture.
use crate::{
    catalog_management::{CatalogReceipt, catalog_operation_id},
    identity::Incarnation,
    index_catalog::CatalogRevision,
    service::{ControlMethod, ControlRequest, ControlResponse, PROTOCOL_VERSION, RequestKey},
};
use serde_json::{Value, json};

const FIXTURE: &str = include_str!("../../../fixtures/wire/catalog_receipt.json");

fn bytes<T: serde::Serialize>(value: &T) -> String {
    String::from_utf8(serde_json::to_vec(value).unwrap()).unwrap()
}

fn receipt(inputs: &Value) -> CatalogReceipt {
    let caller: Incarnation = inputs["caller"].as_str().unwrap().parse().unwrap();
    let key: RequestKey = serde_json::from_value(inputs["key"].clone()).unwrap();
    CatalogReceipt {
        schema: 1,
        operation_id: catalog_operation_id(caller, &key).unwrap(),
        caller,
        key,
        request_digest: inputs["request_digest"].as_str().unwrap().parse().unwrap(),
        catalog_revision: CatalogRevision::try_from(inputs["catalog_revision"].as_u64().unwrap())
            .unwrap(),
        authority_fence_required: inputs["authority_fence_required"].as_bool().unwrap(),
    }
}

/// The fixture with every observed field recomputed from its inputs.
fn rendered(fixture: &Value) -> Value {
    let mut document = fixture.clone();
    for case in document["receipts"].as_array_mut().unwrap() {
        let receipt = receipt(&case["inputs"]);
        let request = ControlRequest {
            protocol: PROTOCOL_VERSION,
            expected_instance: None,
            method: ControlMethod::CatalogReceipt {
                caller: receipt.caller,
                key: receipt.key.clone(),
            },
        };
        case["operation_id"] = json!(receipt.operation_id.to_string());
        case["persisted"] = json!(bytes(&receipt));
        case["control_request"] = json!(bytes(&request));
        case["response"] = json!(bytes(&ControlResponse::CatalogReceipt {
            receipt: Some(receipt),
            fence: None,
        }));
    }
    document["response_absent"] = json!(bytes(&ControlResponse::CatalogReceipt {
        receipt: None,
        fence: None,
    }));
    document
}

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}

#[test]
fn catalog_receipt_wire_bytes_match_reviewed_fixture() {
    let fixture = fixture();
    assert_eq!(fixture["schema"], "airk.wire-fixture.catalog-receipt/1");
    assert!(!fixture["receipts"].as_array().unwrap().is_empty());
    let actual = rendered(&fixture);
    if actual != fixture {
        let path = std::env::temp_dir().join("airk-wire-fixture-catalog_receipt.json");
        std::fs::write(&path, serde_json::to_string_pretty(&actual).unwrap() + "\n").unwrap();
        panic!(
            "catalog_receipt wire bytes differ from the reviewed fixture; observed rendering: {}",
            path.display()
        );
    }
    // P9: persisted payload bytes survive a parse and re-serialization unchanged.
    for case in fixture["receipts"].as_array().unwrap() {
        let persisted = case["persisted"].as_str().unwrap();
        let parsed: CatalogReceipt = serde_json::from_str(persisted).unwrap();
        assert_eq!(bytes(&parsed), persisted, "{}", case["name"]);
        let response: ControlResponse =
            serde_json::from_str(case["response"].as_str().unwrap()).unwrap();
        assert_eq!(bytes(&response), case["response"].as_str().unwrap());
    }
}

#[test]
fn catalog_receipt_variants_and_rejections_match_fixture() {
    let fixture = fixture();
    let variants = fixture["accepted_variants"].as_array().unwrap();
    let rejected = fixture["rejected"].as_array().unwrap();
    assert!(!variants.is_empty() && !rejected.is_empty());
    for case in variants {
        let json = case["json"].as_str().unwrap();
        let canonical = match case["kind"].as_str().unwrap() {
            "control_response" => bytes(&serde_json::from_str::<ControlResponse>(json).unwrap()),
            "control_request" => bytes(&serde_json::from_str::<ControlRequest>(json).unwrap()),
            "catalog_receipt" => bytes(&serde_json::from_str::<CatalogReceipt>(json).unwrap()),
            kind => panic!("unknown fixture kind {kind}"),
        };
        assert_eq!(
            canonical,
            case["canonical"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
    for case in rejected {
        let json = case["json"].as_str().unwrap();
        let refused = match case["kind"].as_str().unwrap() {
            "control_response" => serde_json::from_str::<ControlResponse>(json).is_err(),
            "control_request" => serde_json::from_str::<ControlRequest>(json).is_err(),
            "catalog_receipt" => serde_json::from_str::<CatalogReceipt>(json).is_err(),
            kind => panic!("unknown fixture kind {kind}"),
        };
        assert!(
            refused,
            "{} must be refused: {}",
            case["name"], case["reason"]
        );
    }
}

// Usage-page codec vectors (split C1, `fixtures/wire/usage_page.py`). The
// generator states the canonical bytes, paging and assembly rules itself; this
// test checks the Rust codec reproduces them exactly.
const USAGE_PAGE_FIXTURE: &str = include_str!("../../../fixtures/wire/usage_page.json");

fn usage_fixture() -> Value {
    let fixture: Value = serde_json::from_str(USAGE_PAGE_FIXTURE).unwrap();
    assert_eq!(fixture["schema"], "airk.wire-fixture.usage-page/1");
    fixture
}

fn usage_cases<'a>(fixture: &'a Value, group: &str) -> &'a Vec<Value> {
    let cases = fixture[group].as_array().unwrap();
    assert!(!cases.is_empty(), "{group}: zero selected usage-page cases");
    cases
}

/// `{"$repeat": s, "count": n}` is the fixture's compact form of `s` repeated `n` times.
fn expand_repeats(value: &Value) -> Value {
    match value {
        Value::Object(map) if map.len() == 2 && map.contains_key("$repeat") => Value::String(
            map["$repeat"]
                .as_str()
                .unwrap()
                .repeat(usize::try_from(map["count"].as_u64().unwrap()).unwrap()),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, item)| (key.clone(), expand_repeats(item)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(expand_repeats).collect()),
        other => other.clone(),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn max_bytes(case: &Value) -> usize {
    usize::try_from(case["max_bytes"].as_u64().unwrap()).unwrap()
}

#[test]
fn usage_page_wire_bytes_match_reviewed_fixture() -> anyhow::Result<()> {
    use crate::usage_page::{Page, assemble, page};
    let fixture = usage_fixture();
    for case in usage_cases(&fixture, "pages") {
        let name = case["name"].as_str().unwrap();
        let input = expand_repeats(&case["input"]);
        let pages = case["pages"].as_array().unwrap();
        assert!(!pages.is_empty(), "{name}");
        let mut restored = Value::Null;
        for expected in pages {
            let result = page(input.clone(), expected["cursor"].as_str(), max_bytes(case))?;
            let wire = serde_json::to_vec(&result)?;
            assert_eq!(
                result.digest,
                expected["digest"].as_str().unwrap(),
                "{name}"
            );
            assert_eq!(
                result.cursor.as_deref(),
                expected["next_cursor"].as_str(),
                "{name}"
            );
            assert_eq!(
                json!(result.record_bytes),
                expected["record_bytes"],
                "{name}"
            );
            assert_eq!(json!(result.total_items), expected["total_items"], "{name}");
            let paths = result
                .items
                .iter()
                .map(|item| serde_json::to_value(&item.path))
                .collect::<Result<Vec<_>, _>>()?;
            assert_eq!(Value::Array(paths), expected["paths"], "{name}");
            match expected["bytes"].as_str() {
                Some(text) => assert_eq!(std::str::from_utf8(&wire)?, text, "{name}"),
                None => {
                    assert_eq!(json!(wire.len()), expected["bytes_length"], "{name}");
                    assert_eq!(
                        sha256_hex(&wire),
                        expected["bytes_sha256"].as_str().unwrap(),
                        "{name}"
                    );
                }
            }
            // P9: the page bytes survive a parse and re-serialization unchanged.
            let parsed: Page = serde_json::from_slice(&wire)?;
            assert_eq!(serde_json::to_vec(&parsed)?, wire, "{name}");
            for record in result.items {
                assemble(&mut restored, record)?;
            }
        }
        assert_eq!(restored, input, "{name}: pages reassemble the document");
    }
    for case in usage_cases(&fixture, "refused_pages") {
        let refused = page(
            expand_repeats(&case["input"]),
            case["cursor"].as_str(),
            max_bytes(case),
        );
        assert!(refused.is_err(), "{}: {}", case["name"], case["rule"]);
    }
    Ok(())
}

#[test]
fn usage_page_assembly_digests_and_rejections_match_reviewed_fixture() -> anyhow::Result<()> {
    use crate::usage_page::{Page, Record, assemble};
    let fixture = usage_fixture();
    for case in usage_cases(&fixture, "assemble") {
        let name = case["name"].as_str().unwrap();
        let mut document = Value::Null;
        let mut outcome = Ok(());
        for item in case["records"].as_array().unwrap() {
            let record: Record = serde_json::from_value(item.clone())?;
            assert_eq!(&serde_json::to_value(&record)?, item, "{name}");
            outcome = assemble(&mut document, record);
            if outcome.is_err() {
                break;
            }
        }
        if case["refused"] == true {
            assert!(outcome.is_err(), "{name}: {}", case["rule"]);
        } else {
            outcome?;
            assert_eq!(document, case["document"], "{name}: {}", case["rule"]);
        }
    }
    for case in usage_cases(&fixture, "digests") {
        let value: Value = serde_json::from_str(case["input_json"].as_str().unwrap())?;
        let canonical = serde_json::to_vec(&value)?;
        assert_eq!(
            std::str::from_utf8(&canonical)?,
            case["canonical"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            sha256_hex(&canonical),
            case["sha256"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
    for case in usage_cases(&fixture, "rejected") {
        let json = case["json"].as_str().unwrap();
        let refused = match case["kind"].as_str().unwrap() {
            "record" => serde_json::from_str::<Record>(json).is_err(),
            "page" => serde_json::from_str::<Page>(json).is_err(),
            kind => panic!("unknown fixture kind {kind}"),
        };
        assert!(refused, "{} must be refused", case["name"]);
    }
    Ok(())
}

// Native reply outcome classes (split C8, `fixtures/wire/native_outcomes.py`).
// The classes are asserted by the TypeScript consumer test; here every reply
// marked `control_response` must be a complete native `ControlResponse`.
#[test]
fn native_outcome_replies_are_native_control_responses() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../fixtures/wire/native_outcomes.json")).unwrap();
    assert_eq!(fixture["schema"], "airk.wire-fixture.native-outcomes/1");
    let mut checked = 0;
    for case in fixture["cases"].as_array().unwrap() {
        if case["control_response"] != true {
            continue;
        }
        let reply = &case["reply"];
        let parsed: ControlResponse = serde_json::from_value(reply.clone())
            .unwrap_or_else(|error| panic!("{}: {error}", case["name"]));
        assert_eq!(
            &serde_json::to_value(parsed).unwrap(),
            reply,
            "{}",
            case["name"]
        );
        checked += 1;
    }
    assert!(checked > 0, "zero selected native reply cases");
}

const CONTROL_METHODS_FIXTURE: &str =
    include_str!("../../../fixtures/split/i1/control_methods.json");

/// Each landed C2 variant group: exact request and response bytes, the
/// per-method response budget (contracts §2.8) and reviewed refusals.
#[test]
fn control_method_groups_match_reviewed_fixture() {
    let fixture: Value = serde_json::from_str(CONTROL_METHODS_FIXTURE).unwrap();
    assert_eq!(fixture["schema"], "airk.split-fixture.i1-control-methods/1");
    let groups = fixture["variants"].as_array().unwrap();
    assert!(!groups.is_empty());
    for group in groups {
        let variant = group["variant"].as_str().unwrap();
        for case in group["methods"].as_array().unwrap() {
            let text = case["bytes"].as_str().unwrap();
            let method: ControlMethod = serde_json::from_str(text).unwrap();
            assert_eq!(bytes(&method), text, "{variant} {}", case["name"]);
            assert_eq!(
                method.response_frame_budget() as u64,
                case["response_frame_budget"].as_u64().unwrap(),
                "{variant} {}",
                case["name"]
            );
            assert_eq!(
                method.request_frame_budget() as u64,
                case["request_frame_budget"].as_u64().unwrap(),
                "{variant} {}",
                case["name"]
            );
            assert!(text.len() <= method.request_frame_budget());
        }
        for case in group["rejected_methods"].as_array().unwrap() {
            assert!(
                serde_json::from_str::<ControlMethod>(case["bytes"].as_str().unwrap()).is_err(),
                "{variant} {}",
                case["name"]
            );
        }
        for case in group["responses"].as_array().unwrap() {
            let text = case["bytes"].as_str().unwrap();
            let response: ControlResponse = serde_json::from_str(text).unwrap();
            assert_eq!(bytes(&response), text, "{variant} {}", case["name"]);
        }
        for case in group["rejected_responses"].as_array().unwrap() {
            assert!(
                serde_json::from_str::<ControlResponse>(case["bytes"].as_str().unwrap()).is_err(),
                "{variant} {}",
                case["name"]
            );
        }
    }
}
