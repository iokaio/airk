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
