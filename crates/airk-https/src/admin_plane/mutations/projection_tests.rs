// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::catalog_management::{CatalogReceipt, catalog_operation_id};
use airk_core::identity::Sha256Digest;
use airk_core::index_catalog::CatalogRevision;

fn key(n: u8) -> RequestKey {
    RequestKey {
        namespace: Incarnation::from_bytes([0xa1; 16]),
        epoch: 7,
        nonce: Incarnation::from_bytes([n; 16]),
    }
}

fn receipt(n: u8) -> CatalogReceipt {
    let caller = Incarnation::from_bytes([n + 1; 16]);
    CatalogReceipt {
        schema: 1,
        operation_id: catalog_operation_id(caller, &key(n)).unwrap(),
        caller,
        key: key(n),
        request_digest: Sha256Digest::from_bytes([0x31; 32]),
        catalog_revision: CatalogRevision::INITIAL,
        authority_fence_required: false,
    }
}

#[test]
fn r18_other_native_receipt_key_is_private_without_a_matching_review() {
    let receipt = receipt(0xc1);
    let response = ControlResponse::CatalogReceipt {
        receipt: Some(receipt.clone()),
        fence: None,
    };
    let (public, caller) = public_projection(&response, Some(&json!(key(0xb1)))).unwrap();
    assert_eq!(caller, None);
    assert_eq!(
        public,
        json!({"status":"catalog_receipt", "receipt": {
        "schema":1,"operation_id":receipt.operation_id,"request_digest":receipt.request_digest,
        "catalog_revision":1,"authority_fence_required":false}})
    );
    assert!(
        !serde_json::to_string(&public)
            .unwrap()
            .contains(&key(0xc1).nonce.to_string())
    );
}

#[test]
fn r18_source_change_history_and_public_extension_data_survive_projection() {
    let begin = receipt(0xc1);
    let completion = receipt(0xd1);
    // A real native SourceChangeObservation carries a fenced begin, a separate
    // completion identity, and opaque configuration results owned by that job.
    let response: ControlResponse = serde_json::from_value(json!({
    "status":"catalog_source_change_observation", "observation": {
      "begin": {"request":{"caller":begin.caller,"key":begin.key,
        "expected_catalog_revision":1,"operation":{"operation":"create_project",
        "id":"prj_01010101010101010101010101010101","name":"Docs"}},
        "metadata":begin,"preview_digest":"31".repeat(32),"prepared_digest":"32".repeat(32),
        "proof":{"plan_digest":"33".repeat(32),"service":"34".repeat(16),
          "gate_generation":1,"confirmations":[]}},
      "job":null,"completion":completion,
      "configuration_result":{"request_key":key(0xc1),"future":{"key":"public",
        "request_key":"documentary", "caller":"public-author", "rows":[1,2,3]}}
    }}))
    .unwrap();
    let (public, _) = public_projection(&response, None).unwrap();
    let encoded = serde_json::to_string(&public).unwrap();
    for n in [0xc1, 0xd1] {
        assert!(!encoded.contains(&key(n).nonce.to_string()));
    }
    assert_eq!(
        public["observation"]["configuration_result"],
        json!({"future":{"key":"public","request_key":"documentary",
        "caller":"public-author","rows":[1,2,3]}})
    );
    assert_eq!(
        public["observation"]["begin"]["request"]["operation"]["name"],
        "Docs"
    );
    assert_eq!(
        public["observation"]["completion"]["operation_id"],
        json!(completion.operation_id)
    );
    let (_, caller) = public_projection(&response, Some(&json!(key(0xd1)))).unwrap();
    assert_eq!(
        caller,
        Some(json!(completion.caller)),
        "the earlier begin caller cannot own this review"
    );
}

#[test]
fn r18_busy_and_documentary_unknown_results_are_preserved_exactly() {
    let busy: ControlResponse = serde_json::from_value(json!({"status":"source_change_busy",
        "code":"source_change_jobs_pending", "busy":{"index_id":"idx_01010101010101010101010101010101",
        "pending_jobs":2,"running_jobs":1}})).unwrap();
    assert_eq!(
        public_projection(&busy, None).unwrap().0,
        serde_json::to_value(&busy).unwrap()
    );
    let response = ControlResponse::NativeRead {
        result: json!({"key":"public-key",
        "request_key":"schema-name", "caller":"author", "future":[{"anything":true}]}),
    };
    assert_eq!(
        public_projection(&response, None).unwrap().0,
        serde_json::to_value(&response).unwrap()
    );
}

#[tokio::test]
async fn r18_full_admin_envelope_never_exposes_another_native_receipt_identity() {
    use http_body_util::BodyExt;
    let receipt = receipt(0xc1);
    let response = super::super::native_request(
        "catalog.receipt",
        ControlResponse::CatalogReceipt {
            receipt: Some(receipt.clone()),
            fence: None,
        },
        None,
        None,
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let stdout = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        !stdout.contains(&receipt.key.nonce.to_string()),
        "full envelope: {stdout}"
    );
    assert!(
        !stdout.contains(&receipt.caller.to_string()),
        "full envelope: {stdout}"
    );
    assert!(stdout.contains(&receipt.operation_id.to_string()));
    assert!(stdout.contains("catalog_revision"));
}

#[test]
fn r18_actual_setup_review_apply_receipt_and_unknown_preserve_public_evidence() {
    use airk_core::catalog_setup::{
        IndexSetupDraft, IndexSetupReceipt, IndexSetupRequest, SetupSource, SetupStage,
        SetupStorageAction, setup_index_id, setup_source_ids,
    };
    use airk_core::service::{CatalogOutcomeObservation, ProjectRegistration};
    let registration = ProjectRegistration::new(
        "/fiction/docs".into(),
        "/fiction/index".into(),
        "docs".into(),
    )
    .unwrap();
    let request = IndexSetupRequest {
        caller: Incarnation::from_bytes([0xc2; 16]),
        key: key(0xc1),
        expected_catalog_revision: CatalogRevision::INITIAL,
        name: "Docs".into(),
        registration,
        sources: vec![SetupSource {
            canonical_path: "/fiction/docs".into(),
            aliases: vec![],
            owner: true,
        }],
        setup_watch: false,
    };
    let request_digest = request.identity().unwrap();
    let draft = IndexSetupDraft::new(
        request.clone(),
        SetupStorageAction::CreateMissingLeaf,
        Sha256Digest::from_bytes([0x35; 32]),
    )
    .unwrap();
    let receipt = IndexSetupReceipt {
        schema: 1,
        operation_id: catalog_operation_id(request.caller, &request.key).unwrap(),
        request: request.clone(),
        request_digest,
        index_id: setup_index_id(&request).unwrap(),
        source_ids: setup_source_ids(&request).unwrap(),
        stage: SetupStage::Reserved,
        catalog_revision: CatalogRevision::INITIAL.next().unwrap(),
        source_revision: CatalogRevision::INITIAL,
        preparation: None,
        completion: None,
        legacy_adoption: None,
    };
    receipt.validate().unwrap();
    for response in [
        ControlResponse::CatalogIndexSetupDraft {
            draft,
            request_digest,
        },
        ControlResponse::CatalogIndexSetupApplied {
            receipt: receipt.clone(),
        },
        ControlResponse::CatalogIndexSetupReceipt {
            receipt: Some(receipt.clone()),
        },
        ControlResponse::CatalogOutcomeUnknown {
            code: "catalog_apply_outcome_unknown".into(),
            caller: Incarnation::from_bytes([0xd2; 16]),
            key: key(0xd1),
            request_digest,
            observation: Some(CatalogOutcomeObservation::IndexSetup {
                receipt: Box::new(receipt),
            }),
        },
    ] {
        let original = serde_json::to_value(&response).unwrap();
        let (public, _) = public_projection(&response, None).unwrap();
        let encoded = serde_json::to_string(&public).unwrap();
        assert!(!encoded.contains(&request.caller.to_string()), "{encoded}");
        for n in [0xc1, 0xd1] {
            assert!(!encoded.contains(&key(n).nonce.to_string()), "{encoded}");
        }
        // Independent removal of exactly the known request identity slots.
        let mut expected = original;
        if let Some(object) = expected.as_object_mut() {
            object.remove("caller");
            object.remove("key");
        }
        for path in [
            "/draft/request",
            "/receipt/request",
            "/observation/receipt/request",
        ] {
            if let Some(object) = expected.pointer_mut(path).and_then(Value::as_object_mut) {
                object.remove("caller");
                object.remove("key");
            }
        }
        assert_eq!(public, expected);
    }
}

#[test]
fn r18_private_transport_response_is_refused_before_public_serialization() {
    let response = ControlResponse::AdminLaunchCode {
        code: "A".repeat(43).try_into().unwrap(),
        port: 12345,
        expires_in_ms: 1000,
    };
    assert!(matches!(
        public_projection(&response, None),
        Err(AdminCode::AdminInternal)
    ));
}

#[test]
fn r18_split_recovery_retains_evidence_without_any_of_its_three_native_keys() {
    use airk_core::catalog_split::{
        SplitRecoveryAction, SplitRecoveryOutcome, SplitRecoveryReceipt, SplitRecoveryRequest,
    };
    let digest = Sha256Digest::from_bytes([0x35; 32]);
    let operation = Incarnation::from_bytes([0x36; 16]);
    let response = ControlResponse::CatalogSplitRecoveryReceipt {
        operation_id: operation,
        caller: Incarnation::from_bytes([0xb2; 16]),
        key: key(0xb1),
        receipt: Some(Box::new(SplitRecoveryReceipt {
            request: SplitRecoveryRequest {
                caller: Incarnation::from_bytes([0xc2; 16]),
                key: key(0xc1),
                operation_id: operation,
                request_digest: digest,
                expected_receipt_digest: digest,
                expected_finalization_revision: None,
                action: SplitRecoveryAction::Resume,
            },
            request_digest: digest,
            outcome: SplitRecoveryOutcome::Unknown {
                caller: Incarnation::from_bytes([0xd2; 16]),
                key: key(0xd1),
            },
        })),
        snapshot: None,
        expired: false,
    };
    let (public, caller) = public_projection(&response, Some(&json!(key(0xc1)))).unwrap();
    assert_eq!(caller, Some(json!(Incarnation::from_bytes([0xc2; 16]))));
    assert_eq!(
        public,
        json!({"status":"catalog_split_recovery_receipt",
        "operation_id":operation,"receipt":{"request":{"operation_id":operation,
            "request_digest":digest,"expected_receipt_digest":digest,"action":"resume"},
            "request_digest":digest,"outcome":{"outcome":"unknown"}},
        "snapshot":null,"expired":false})
    );
}

#[test]
fn r18_read_only_lifecycle_status_does_not_reveal_a_prior_mutation_identity() {
    use airk_core::service_lifecycle::{
        LifecycleAction, LifecyclePhase, LifecycleRecord, LifecycleRequest,
    };
    let record = LifecycleRecord {
        id: Sha256Digest::from_bytes([0x35; 32]),
        request: LifecycleRequest {
            caller: Incarnation::from_bytes([0xc2; 16]),
            key: key(0xc1),
            action: LifecycleAction::Restart,
            expected_instance: Incarnation::from_bytes([0x36; 16]),
            runtime: Sha256Digest::from_bytes([0x37; 32]),
            manager: true,
        },
        phase: LifecyclePhase::Draining,
    };
    let response = ControlResponse::LifecycleStatus {
        explicitly_stopped: false,
        pending: Some(record.clone()),
    };
    let (public, caller) = public_projection(&response, None).unwrap();
    assert_eq!(caller, None);
    assert_eq!(
        public,
        json!({"status":"lifecycle_status","explicitly_stopped":false,
        "pending":{"id":record.id,"request":{"action":"restart",
            "expected_instance":record.request.expected_instance,"runtime":record.request.runtime,
            "manager":true},"phase":"draining"}})
    );
}
