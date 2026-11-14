// SPDX-License-Identifier: Apache-2.0
use super::*;

async fn call(
    plane: &AdminPlane,
    cookie: &str,
    csrf: &str,
    operation: &str,
    body: Value,
) -> (u16, Value) {
    // Raw real handler request: no test helper automatically submits an ack.
    let (status, _, bytes) = answer(
        plane,
        api_call(cookie, csrf, operation, &body.to_string()),
        PeerProof::SameUser,
    )
    .await;
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn prepare(plane: &AdminPlane, cookie: &str, csrf: &str, split: bool) -> String {
    let (status, reply) = call(
        plane,
        cookie,
        csrf,
        r04_recovery_operation(split),
        r04_recovery_body(split),
    )
    .await;
    assert_eq!(status, 200, "{reply}");
    assert!(reply.get("request").is_none());
    assert!(reply.get("review").is_none());
    let ack: contract::MutationPreparation =
        serde_json::from_value(reply["result"].clone()).unwrap();
    assert_eq!(
        ack.receipt_operation.as_deref(),
        Some(if split {
            "catalog.split_recovery_receipt"
        } else {
            "catalog.policy_resume_receipt"
        })
    );
    ack.preparation
}

#[tokio::test]
async fn r17_ack_precedes_effects_and_lost_submit_recovers_the_original_native_key() {
    for split in [false, true] {
        let fixture = r04_fixture();
        let name = if split {
            "split_already_complete"
        } else {
            "policy_known_accepted"
        };
        let case = fixture["recovery_receipts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap();
        let (plane, sent, _) = r04_recovery_plane(vec![
            Some(epoch(7)),
            None,
            None,
            Some(r04_response(&case["response"])),
        ]);
        let (cookie, csrf) = sign_in(&plane).await;
        let token = prepare(&plane, &cookie, &csrf, split).await;
        assert_eq!(sent.lock().unwrap().len(), 1, "prepare only reads epoch");
        let (status, unknown) = call(
            &plane,
            &cookie,
            &csrf,
            r04_recovery_operation(split),
            json!({"prepared":token}),
        )
        .await;
        assert_eq!(status, 503, "{unknown}");
        assert_eq!(unknown["review"], token);
        {
            let methods = sent.lock().unwrap();
            assert_eq!(methods.len(), 3);
            assert_eq!(methods[1], methods[2]);
        }
        let (status, again) = call(
            &plane,
            &cookie,
            &csrf,
            r04_recovery_operation(split),
            json!({"prepared":token}),
        )
        .await;
        assert_eq!(status, 503, "{again}");
        assert_eq!(sent.lock().unwrap().len(), 3, "no resubmit");
        let (status, recovered) = call(
            &plane,
            &cookie,
            &csrf,
            r04_recovery_operation(split),
            json!({"recover":token}),
        )
        .await;
        assert_eq!(status, 200, "{recovered}");
        assert_eq!(recovered["result"], case["expect"]["native"]);
        let methods = sent.lock().unwrap();
        assert_eq!(methods.len(), 4);
        let submitted = if split {
            &methods[1]["catalog_split_recover"]["request"]
        } else {
            &methods[1]["catalog_policy_resume"]
        };
        let receipt = if split {
            &methods[3]["catalog_split_recovery_receipt"]
        } else {
            &methods[3]["catalog_policy_resume_receipt"]
        };
        assert_eq!(submitted["key"], receipt["key"]);
        assert_eq!(submitted["caller"], receipt["caller"]);
    }
}

#[tokio::test]
async fn r17_ready_recovery_and_changed_body_or_operation_never_dispatch() {
    let (plane, sent, _) = r04_recovery_plane(vec![Some(epoch(7))]);
    let (cookie, csrf) = sign_in(&plane).await;
    let token = prepare(&plane, &cookie, &csrf, false).await;
    let (status, ready) = call(
        &plane,
        &cookie,
        &csrf,
        "catalog.policy_resume",
        json!({"recover":token}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(ready["result"]["preparation"], token);
    for (operation, body) in [
        (
            "catalog.policy_resume",
            json!({"prepared":token,"method":r04_recovery_body(false)}),
        ),
        ("catalog.split_recover", json!({"prepared":token})),
        (
            "catalog.policy_resume",
            json!({"prepared":token,"recover":token}),
        ),
    ] {
        let (status, _) = call(&plane, &cookie, &csrf, operation, body).await;
        assert_eq!(status, 400);
    }
    assert_eq!(sent.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn r17_foreign_expired_and_logout_handles_refuse_before_dispatch() {
    for end in ["foreign", "expired", "logout"] {
        let (plane, sent, clock) = r04_recovery_plane(vec![Some(epoch(7))]);
        let (mut cookie, mut csrf) = sign_in(&plane).await;
        let token = prepare(&plane, &cookie, &csrf, false).await;
        match end {
            "foreign" => (cookie, csrf) = sign_in(&plane).await,
            "expired" => clock.store(SESSION_IDLE_MS, Ordering::SeqCst),
            "logout" => {
                plane.secrets().sessions.remove(&cookie);
            }
            _ => unreachable!(),
        }
        let (status, _) = call(
            &plane,
            &cookie,
            &csrf,
            "catalog.policy_resume",
            json!({"prepared":token}),
        )
        .await;
        assert!(matches!(status, 400 | 401), "{end}: {status}");
        assert_eq!(sent.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn r17_missing_receipt_remains_unknown_and_never_unlocks_replay() {
    let (plane, sent, _) = r04_recovery_plane(vec![
        Some(epoch(7)),
        None,
        None,
        Some(r04_recovery_reply(false)),
    ]);
    let (cookie, csrf) = sign_in(&plane).await;
    let token = prepare(&plane, &cookie, &csrf, false).await;
    call(
        &plane,
        &cookie,
        &csrf,
        "catalog.policy_resume",
        json!({"prepared":token}),
    )
    .await;
    let (status, reply) = call(
        &plane,
        &cookie,
        &csrf,
        "catalog.policy_resume",
        json!({"recover":token}),
    )
    .await;
    assert_eq!(status, 503, "{reply}");
    assert_eq!(reply["error"]["retryable"], false);
    assert!(reply["error"]["native"]["receipt"].is_null());
    assert_eq!(sent.lock().unwrap().len(), 4);
}

#[tokio::test]
async fn r17_known_reply_cache_is_bounded_without_truncating_original_delivery() {
    let mut response = r04_policy_success();
    let ControlResponse::CatalogPolicyResumed { snapshot, .. } = &mut response else {
        unreachable!()
    };
    for n in 2..5 {
        snapshot
            .policy
            .roots
            .push(airk_core::source_policy::SourceRoot {
                id: airk_core::index_catalog::SourceRootId::from_identity(Incarnation::from_bytes(
                    [n; 16],
                )),
                canonical_path: format!("/fiction/root{n}/{}", "public-evidence".repeat(1900)),
                aliases: vec![],
                owner: false,
            });
    }
    snapshot.validate().unwrap();
    let expected = serde_json::to_value(&response).unwrap();
    assert!(serde_json::to_vec(&response).unwrap().len() > 65_536);
    let (plane, sent, _) = r04_recovery_plane(vec![Some(epoch(7)), Some(response)]);
    let (cookie, csrf) = sign_in(&plane).await;
    let token = prepare(&plane, &cookie, &csrf, false).await;
    let (status, reply) = call(
        &plane,
        &cookie,
        &csrf,
        "catalog.policy_resume",
        json!({"prepared":token}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(reply["result"], expected);
    let (status, reply) = call(
        &plane,
        &cookie,
        &csrf,
        "catalog.policy_resume",
        json!({"prepared":token}),
    )
    .await;
    assert_eq!(status, 503);
    assert_eq!(error_code(&reply), "admin_prepared_reply_unavailable");
    let decoded: FailureEnvelope = serde_json::from_value(reply).unwrap();
    decoded.validate().unwrap();
    assert_eq!(
        call(
            &plane,
            &cookie,
            &csrf,
            "catalog.policy_resume",
            json!({"acknowledged":token})
        )
        .await
        .0,
        200,
        "the retained full known large outcome may release pinned custody"
    );
    assert_eq!(sent.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn r17_lifecycle_lost_reply_has_no_invented_receipt_or_status_substitution() {
    let (plane, sent, _) = r04_recovery_plane(vec![Some(epoch(7)), None, None]);
    let (cookie, csrf) = sign_in(&plane).await;
    let (status, ack) = call(
        &plane,
        &cookie,
        &csrf,
        "service.stop",
        json!({"lifecycle":{"action":"stop"}}),
    )
    .await;
    assert_eq!(status, 200);
    assert!(ack["result"]["receipt_operation"].is_null());
    let token = ack["result"]["preparation"].as_str().unwrap();
    assert_eq!(sent.lock().unwrap().len(), 1);
    let (status, unknown) = call(
        &plane,
        &cookie,
        &csrf,
        "service.stop",
        json!({"prepared":token}),
    )
    .await;
    assert_eq!(status, 503);
    assert!(unknown["error"].get("native").is_none());
    let (status, unknown) = call(
        &plane,
        &cookie,
        &csrf,
        "service.stop",
        json!({"recover":token}),
    )
    .await;
    assert_eq!(status, 503);
    assert!(unknown["error"].get("native").is_none());
    let methods = sent.lock().unwrap();
    assert_eq!(methods.len(), 3);
    assert_eq!(methods[1], methods[2]);
    assert!(
        methods
            .iter()
            .all(|method| method.get("lifecycle_status").is_none())
    );
}

#[tokio::test]
async fn r17_concurrent_submit_claims_the_prepared_method_once() {
    let (mut plane, _, _) = r04_recovery_plane(vec![]);
    let sent = Arc::new(Mutex::new(Vec::new()));
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (log, started, finish) = (sent.clone(), entered.clone(), release.clone());
    plane.dispatch = Arc::new(move |method| {
        log.lock()
            .unwrap()
            .push(serde_json::to_value(&method).unwrap());
        let (started, finish) = (started.clone(), finish.clone());
        Box::pin(async move {
            if matches!(method, ControlMethod::RequestEpoch) {
                return Ok(epoch(7));
            }
            started.notify_one();
            finish.notified().await;
            Ok(ControlResponse::Failure {
                code: "native_refused".into(),
            })
        })
    });
    let (cookie, csrf) = sign_in(&plane).await;
    let (_, ack) = call(
        &plane,
        &cookie,
        &csrf,
        "service.stop",
        json!({"lifecycle":{"action":"stop"}}),
    )
    .await;
    let token = ack["result"]["preparation"].as_str().unwrap();
    let first = call(
        &plane,
        &cookie,
        &csrf,
        "service.stop",
        json!({"prepared":token}),
    );
    let second = async {
        entered.notified().await;
        let (status, _) = call(
            &plane,
            &cookie,
            &csrf,
            "service.stop",
            json!({"acknowledged":token}),
        )
        .await;
        assert_eq!(status, 400, "an in-flight mutation cannot be acknowledged");
        let (status, reply) = call(
            &plane,
            &cookie,
            &csrf,
            "service.stop",
            json!({"prepared":token}),
        )
        .await;
        assert_eq!(status, 503, "{reply}");
        release.notify_one();
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(first, second)
    })
    .await
    .unwrap();
    assert_eq!(sent.lock().unwrap().len(), 2, "one epoch and one mutation");
}

#[tokio::test]
async fn r17_prepared_slots_share_existing_capacity_and_cannot_be_evicted() {
    let (plane, sent, _) = r04_recovery_plane((0..33).map(|_| Some(epoch(7))).collect());
    let (cookie, csrf) = sign_in(&plane).await;
    let mut tokens = Vec::new();
    for _ in 0..32 {
        tokens.push(prepare(&plane, &cookie, &csrf, false).await);
    }
    let (status, reply) = call(
        &plane,
        &cookie,
        &csrf,
        "catalog.policy_resume",
        r04_recovery_body(false),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(error_code(&reply), "admin_review_capacity");
    let (status, ready) = call(
        &plane,
        &cookie,
        &csrf,
        "catalog.policy_resume",
        json!({"recover":tokens[0]}),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(ready["result"]["preparation"], tokens[0]);
    assert_eq!(
        sent.lock().unwrap().len(),
        33,
        "no prepared mutation dispatched"
    );
    assert_eq!(plane.secrets().sessions.review_count(&cookie), 32);
}

#[tokio::test]
async fn r17_explicit_known_acknowledgment_keeps_review_and_preparation_maps_bounded() {
    let script = (0..40)
        .flat_map(|_| [Some(epoch(7)), Some(r04_policy_success())])
        .collect();
    let (plane, sent, _) = r04_recovery_plane(script);
    let (cookie, csrf) = sign_in(&plane).await;
    let mut first = None;
    for iteration in 0..40 {
        let token = prepare(&plane, &cookie, &csrf, false).await;
        first.get_or_insert(token.clone());
        let (status, retained) = call(
            &plane,
            &cookie,
            &csrf,
            "catalog.policy_resume",
            json!({"prepared":token}),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(retained["result"]["status"], "catalog_policy_resumed");
        if iteration == 0 {
            let cached = call(
                &plane,
                &cookie,
                &csrf,
                "catalog.policy_resume",
                json!({"prepared":token}),
            )
            .await;
            assert_eq!(
                cached,
                (status, retained.clone()),
                "a lost known reply is recovered in full without redispatch"
            );
        }
        let (status, ack) = call(
            &plane,
            &cookie,
            &csrf,
            "catalog.policy_resume",
            json!({"acknowledged":token}),
        )
        .await;
        assert_eq!(status, 200);
        assert_eq!(ack["result"], json!({"acknowledged":token}));
        let secrets = plane.secrets();
        let reviews = secrets.sessions.review_count(&cookie);
        assert!(reviews <= 32);
        assert_eq!(secrets.sessions.preparation_count(&cookie), reviews);
    }
    assert_eq!(plane.secrets().sessions.preparation_count(&cookie), 32);
    let (status, _) = call(
        &plane,
        &cookie,
        &csrf,
        "catalog.policy_resume",
        json!({"recover":first.unwrap()}),
    )
    .await;
    assert_eq!(status, 400, "eviction removes both maps");
    assert_eq!(sent.lock().unwrap().len(), 80);
}

#[tokio::test]
async fn r17_lost_ack_reply_is_idempotent_and_unknown_ack_never_releases_custody() {
    let (plane, sent, _) = r04_recovery_plane(vec![
        Some(epoch(7)),
        Some(ControlResponse::Failure {
            code: "key_conflict".into(),
        }),
    ]);
    let (cookie, csrf) = sign_in(&plane).await;
    let token = prepare(&plane, &cookie, &csrf, false).await;
    assert_eq!(
        call(
            &plane,
            &cookie,
            &csrf,
            "catalog.policy_resume",
            json!({"acknowledged":token})
        )
        .await
        .0,
        400
    );
    let (status, retained) = call(
        &plane,
        &cookie,
        &csrf,
        "catalog.policy_resume",
        json!({"prepared":token}),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(error_code(&retained), "key_conflict");
    for _ in 0..2 {
        // Discarding the first ack reply models its loss after acceptance.
        assert_eq!(
            call(
                &plane,
                &cookie,
                &csrf,
                "catalog.policy_resume",
                json!({"acknowledged":token})
            )
            .await
            .0,
            200
        );
    }
    assert_eq!(plane.secrets().sessions.review_count(&cookie), 0);
    assert_eq!(plane.secrets().sessions.preparation_count(&cookie), 0);
    assert_eq!(sent.lock().unwrap().len(), 2);

    let (plane, sent, _) = r04_recovery_plane(vec![Some(epoch(7)), None, None]);
    let (cookie, csrf) = sign_in(&plane).await;
    let token = prepare(&plane, &cookie, &csrf, false).await;
    assert_eq!(
        call(
            &plane,
            &cookie,
            &csrf,
            "catalog.policy_resume",
            json!({"prepared":token})
        )
        .await
        .0,
        503
    );
    assert_eq!(
        call(
            &plane,
            &cookie,
            &csrf,
            "catalog.policy_resume",
            json!({"acknowledged":token})
        )
        .await
        .0,
        400
    );
    assert_eq!(plane.secrets().sessions.review_count(&cookie), 1);
    assert_eq!(plane.secrets().sessions.preparation_count(&cookie), 1);
    assert_eq!(sent.lock().unwrap().len(), 3);
}
