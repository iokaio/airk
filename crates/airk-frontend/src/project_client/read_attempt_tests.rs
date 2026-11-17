// SPDX-License-Identifier: Apache-2.0
use super::*;

#[tokio::test]
async fn dropping_actual_connection_wait_keeps_native_capacity_until_complete_reply() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("runtime");
    local_ipc::prepare_root(&root)?;
    let endpoint = local_ipc::endpoint(&root, "owned-read-lifetime")?;
    let mut listener = local_ipc::Listener::bind(&endpoint)?;
    let sent = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let peer_sent = sent.clone();
    let peer_release = release.clone();
    let peer = tokio::spawn(async move {
        let mut stream = listener.accept().await?;
        let received: Option<Query> =
            local_ipc::read_message(&mut stream, Duration::from_secs(5)).await?;
        ensure!(received.is_some(), "native read not submitted");
        peer_sent.notify_one();
        peer_release.notified().await;
        local_ipc::write_message(
            &mut stream,
            &protocol::QueryResult {
                read_admission: None,
                result: airk_mcp::unified::success(serde_json::json!({"complete":true})),
                status_receipt: Default::default(),
                rebind: false,
            },
            Duration::from_secs(5),
        )
        .await
    });
    let stream = local_ipc::connect(&endpoint).await?;
    let now = Instant::now();
    let mut connection = Connection {
        stream: Some(stream),
        identity: airk_core::service::WorkerIdentity {
            service: airk_core::identity::Incarnation::from_bytes([1; 16]),
            worker: airk_core::identity::Incarnation::from_bytes([2; 16]),
            project: "ab".repeat(32).parse()?,
            fencing_generation: 1,
        },
        authorized_until: now + Duration::from_secs(55),
        last_finished: now,
        healthy: true,
        scoped: false,
        read_scope: None,
        read_attempt_supported: false,
        status_receipt: Default::default(),
    };
    let capacity = Arc::new(tokio::sync::Semaphore::new(1));
    let permit = capacity.clone().acquire_owned().await?;
    let context = airk_mcp::read_attempt::ReadAttemptContext::new(
        now + Duration::from_secs(5),
        Default::default(),
    )
    .with_lifetime_guard(Arc::new(permit));
    let caller = tokio::spawn(async move {
        connection
            .call_with_read_context("airk_status".into(), Default::default(), context)
            .await
    });
    sent.notified().await;
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert_eq!(
        capacity.available_permits(),
        0,
        "abandoning HTTP wait released native capacity"
    );
    release.notify_one();
    peer.await??;
    tokio::time::timeout(Duration::from_secs(5), async {
        while capacity.available_permits() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert_eq!(capacity.available_permits(), 1);
    Ok(())
}

async fn exchange_fixture(proof: &str, supported: bool, twice_busy: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("runtime");
    local_ipc::prepare_root(&root)?;
    let endpoint = local_ipc::endpoint(&root, "read-attempt-proof-test")?;
    let mut listener = local_ipc::Listener::bind(&endpoint)?;
    let identity = airk_core::service::WorkerIdentity {
        service: airk_core::identity::Incarnation::from_bytes([1; 16]),
        worker: airk_core::identity::Incarnation::from_bytes([2; 16]),
        project: "ab".repeat(32).parse()?,
        fencing_generation: 1,
    };
    let proof_case = proof.to_owned();
    let valid = proof.starts_with("valid");
    let native = proof == "valid_native";
    let expects_retry = valid && supported;
    let peer = tokio::spawn(async move {
        let mut stream = listener.accept().await?;
        let first: Query = local_ipc::read_message(&mut stream, Duration::from_secs(5))
            .await?
            .context("first read missing")?;
        ensure!(
            first.read_attempt.is_some() == supported,
            "legacy gate changed"
        );
        if native {
            ensure!(
                first.native.is_some()
                    && first
                        .read_attempt
                        .is_some_and(|attempt| attempt.native_retry_expires_monotonic_ns.is_some()),
                "native retry deadline missing"
            );
        }
        let admission = if proof_case == "none" || !supported {
            None
        } else {
            let mut witness = protocol::SelectionReadAdmission {
                request_id: first.read_attempt.unwrap().request_id,
                identity,
                scope: None,
            };
            match proof_case.as_str() {
                "request" => {
                    witness.request_id = airk_core::identity::Incarnation::from_bytes([9; 16])
                }
                "identity" => witness.identity.fencing_generation += 1,
                "scope" => {
                    witness.scope = Some(serde_json::from_value(serde_json::json!({
                        "schema":1,"lease_generation":1,"index_id":format!("idx_{}","a".repeat(32)),
                        "grant_id":format!("grt_{}","b".repeat(32)),"grant_revision":1,
                        "fingerprint":"c".repeat(64),
                    }))?)
                }
                _ => (),
            }
            Some(witness)
        };
        let result = airk_mcp::unified::success(
            serde_json::json!({"status":"retry","code":"selection_busy","retryable":true,"retry_after_ms":25,"action":"repeat_same_request"}),
        );
        local_ipc::write_message(
            &mut stream,
            &protocol::QueryResult {
                result: result.clone(),
                status_receipt: Default::default(),
                rebind: false,
                read_admission: admission,
            },
            Duration::from_secs(5),
        )
        .await?;
        if expects_retry {
            let second: Query = local_ipc::read_message(&mut stream, Duration::from_secs(5))
                .await?
                .context("safe second attempt missing")?;
            ensure!(
                serde_json::to_value(&first)? == serde_json::to_value(&second)?,
                "request/budget changed on retry"
            );
            local_ipc::write_message(
                &mut stream,
                &protocol::QueryResult {
                    result: if twice_busy {
                        result
                    } else {
                        airk_mcp::unified::success(serde_json::json!({"done":true}))
                    },
                    status_receipt: Default::default(),
                    rebind: false,
                    read_admission: twice_busy.then(|| protocol::SelectionReadAdmission {
                        request_id: second.read_attempt.unwrap().request_id,
                        identity,
                        scope: None,
                    }),
                },
                Duration::from_secs(5),
            )
            .await?;
        }
        let extra =
            local_ipc::read_message::<_, Query>(&mut stream, Duration::from_millis(100)).await;
        ensure!(!matches!(extra, Ok(Some(_))), "unexpected replay");
        Ok::<_, anyhow::Error>(())
    });
    let stream = local_ipc::connect(&endpoint).await?;
    let now = Instant::now();
    let mut connection = Connection {
        stream: Some(stream),
        identity,
        authorized_until: now + Duration::from_secs(55),
        last_finished: now,
        healthy: true,
        scoped: false,
        read_scope: None,
        read_attempt_supported: supported,
        status_receipt: Default::default(),
    };
    let context = airk_mcp::read_attempt::ReadAttemptContext::new(
        now + Duration::from_secs(5),
        Default::default(),
    );
    let result = if native {
        connection
            .read_native_with_context(airk_mcp::local::NativeRead::UsagePolicy, context)
            .await
    } else {
        connection
            .call_with_read_context("airk_status".into(), Default::default(), context)
            .await
    };
    if supported && matches!(proof, "request" | "identity" | "scope") {
        assert!(result.is_err());
        assert!(!connection.healthy, "untrusted proof left reusable stream");
    } else {
        let result = result?;
        let body: serde_json::Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap())?;
        if expects_retry && !twice_busy {
            assert_eq!(body["done"], true);
        } else {
            assert_eq!(body["code"], "selection_busy");
        }
    }
    drop(connection);
    peer.await??;
    Ok(())
}

#[tokio::test]
async fn exact_private_selection_proof_repeats_original_query_once() -> Result<()> {
    exchange_fixture("valid", true, false).await?;
    exchange_fixture("valid_native", true, false).await?;
    exchange_fixture("valid", true, true).await
}

#[tokio::test]
async fn public_hint_or_unsupported_peer_never_authorizes_internal_replay() -> Result<()> {
    exchange_fixture("none", true, false).await?;
    exchange_fixture("valid", false, false).await
}

#[tokio::test]
async fn crossed_request_or_worker_proof_fails_closed_without_replay() -> Result<()> {
    exchange_fixture("request", true, false).await?;
    exchange_fixture("identity", true, false).await?;
    exchange_fixture("scope", true, false).await
}

async fn partition_purpose_exchange(row: &str, refusals: bool) -> Result<()> {
    use airk_core::combined_context::PartitionAllowance;
    let fixtures: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../fixtures/split/arbiter/focus_only_allowance_cases.json"
    ))?;
    let allowance: PartitionAllowance = serde_json::from_value(fixtures[row]["allowance"].clone())?;
    allowance.validate()?;
    let canonical = match fixtures[row]["canonical_tool"].as_str() {
        Some("search") => "airk_search",
        Some("answer") => "airk_answer",
        _ => anyhow::bail!("fixture canonical operation missing"),
    }
    .to_owned();
    let arguments = if canonical == "airk_search" {
        serde_json::json!({"query":"fixed native purpose control", "limit":allowance.resources.candidates, "max_bytes":4096})
    } else {
        let mut budget = allowance.parent_budget.clone();
        budget.calls = allowance.resources.calls;
        budget.candidates = allowance.resources.candidates;
        budget.max_bytes = allowance.resources.bytes;
        budget.input_tokens = allowance.resources.input_tokens.max(1);
        let request = airk_core::arbiter::context_action::ContextRequestV2 {
            schema: airk_core::arbiter::context_action::REQUEST_SCHEMA_V2.into(),
            question: "fixed native purpose control".into(),
            budget,
        };
        request.validate()?;
        serde_json::to_value(request)?
    };
    if canonical == "airk_search" {
        let _: airk_mcp::SearchArgs = serde_json::from_value(arguments.clone())?;
    } else {
        serde_json::from_value::<airk_mcp::AnswerArgs>(arguments.clone())?
            .request
            .validate()?;
    }
    let arguments = arguments.as_object().unwrap().clone();
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("runtime");
    local_ipc::prepare_root(&root)?;
    let endpoint = local_ipc::endpoint(&root, "partition-purpose")?;
    let mut listener = local_ipc::Listener::bind(&endpoint)?;
    let expected = allowance.clone();
    let expected_arguments = arguments.clone();
    let expected_tool = canonical.clone();
    let peer = tokio::spawn(async move {
        let mut stream = listener.accept().await?;
        let query: Query = local_ipc::read_message(&mut stream, Duration::from_secs(5))
            .await?
            .context("valid partition was not submitted")?;
        ensure!(
            query.tool == expected_tool && query.native.is_none(),
            "wrong purpose crossed exchange"
        );
        ensure!(
            query.arguments == expected_arguments,
            "canonical arguments changed"
        );
        ensure!(
            serde_json::to_value(&query.context_partition)?
                == serde_json::to_value(Some(&expected))?,
            "original partition changed"
        );
        let attempt = query.read_attempt.context("original attempt omitted")?;
        ensure!(
            attempt.request_id == expected.attempt,
            "original attempt identity changed"
        );
        ensure!(
            airk_host::read_attempt::decode(attempt)?.deadline() > Instant::now(),
            "original attempt expired"
        );
        local_ipc::write_message(
            &mut stream,
            &protocol::QueryResult {
                result: airk_mcp::unified::success(serde_json::json!({"complete":true})),
                status_receipt: Default::default(),
                rebind: false,
                read_admission: None,
            },
            Duration::from_secs(5),
        )
        .await?;
        let extra =
            local_ipc::read_message::<_, Query>(&mut stream, Duration::from_secs(5)).await?;
        ensure!(extra.is_none(), "unexpected second frame or replay");
        Ok::<_, anyhow::Error>(())
    });
    let stream = local_ipc::connect(&endpoint).await?;
    let now = Instant::now();
    let mut connection = Connection {
        stream: Some(stream),
        identity: airk_core::service::WorkerIdentity {
            service: airk_core::identity::Incarnation::from_bytes([1; 16]),
            worker: airk_core::identity::Incarnation::from_bytes([2; 16]),
            project: "ab".repeat(32).parse()?,
            fencing_generation: 1,
        },
        authorized_until: now + Duration::from_secs(55),
        last_finished: now,
        healthy: true,
        scoped: true,
        read_scope: None,
        read_attempt_supported: true,
        status_receipt: Default::default(),
    };
    let context = airk_mcp::read_attempt::ReadAttemptContext::new(
        now + Duration::from_secs(5),
        Default::default(),
    )
    .with_request_id(allowance.attempt)?
    .with_context_partition(allowance.clone())?;
    if refusals {
        let wrong = if canonical == "airk_search" {
            "airk_answer"
        } else {
            "airk_search"
        };
        assert!(
            connection
                .call_with_read_context(wrong.into(), arguments.clone(), context.clone())
                .await
                .is_err()
        );
        assert!(
            connection.healthy && connection.stream.is_some(),
            "crossed purpose reached submission"
        );
        assert!(
            connection
                .read_native_with_context(airk_mcp::local::NativeRead::UsagePolicy, context.clone())
                .await
                .is_err()
        );
        assert!(
            connection.healthy && connection.stream.is_some(),
            "native request reached submission"
        );
        connection.read_attempt_supported = false;
        assert!(
            connection
                .call_with_read_context(canonical.clone(), arguments.clone(), context.clone())
                .await
                .is_err()
        );
        assert!(
            connection.healthy && connection.stream.is_some(),
            "unsupported attempt reached submission"
        );
        connection.read_attempt_supported = true;
        let mut malformed = allowance.clone();
        malformed.resources.calls = malformed.parent_budget.calls + 1;
        assert!(
            airk_mcp::read_attempt::ReadAttemptContext::new(
                now + Duration::from_secs(5),
                Default::default()
            )
            .with_request_id(malformed.attempt)?
            .with_context_partition(malformed)
            .is_err()
        );
    }
    let reply = connection
        .call_with_read_context(canonical, arguments, context)
        .await?;
    let body: serde_json::Value = serde_json::from_str(
        reply["content"][0]["text"]
            .as_str()
            .context("body missing")?,
    )?;
    assert_eq!(body["complete"], true);
    drop(connection);
    peer.await??;
    Ok(())
}

#[tokio::test]
async fn actual_connection_submits_focus_search_and_context_answer_with_original_allowance()
-> Result<()> {
    for row in ["ordinary_focus", "context", "zero_optional_work"] {
        partition_purpose_exchange(row, false).await?;
    }
    Ok(())
}
#[tokio::test]
async fn actual_connection_refuses_crossed_native_malformed_and_unsupported_partition_purposes()
-> Result<()> {
    for row in ["ordinary_focus", "context", "zero_optional_work"] {
        partition_purpose_exchange(row, true).await?;
    }
    Ok(())
}

fn admitted_lifetime_fixture(now: Instant) -> Connection {
    Connection {
        stream: None,
        identity: airk_core::service::WorkerIdentity {
            service: airk_core::identity::Incarnation::from_bytes([81; 16]),
            worker: airk_core::identity::Incarnation::from_bytes([82; 16]),
            project: "ab".repeat(32).parse().unwrap(),
            fencing_generation: 1,
        },
        authorized_until: now + Duration::from_secs(55),
        last_finished: now,
        healthy: true,
        scoped: false,
        read_scope: None,
        read_attempt_supported: true,
        status_receipt: Default::default(),
    }
}
#[test]
fn admitted_original_parent_survives_admission_expiry_but_never_renews_or_reuses_it() -> Result<()>
{
    let now = Instant::now();
    let mut connection = admitted_lifetime_fixture(now);
    let parent = airk_mcp::read_attempt::ReadAttemptContext::new(
        now + Duration::from_secs(120),
        Default::default(),
    )
    .with_request_id(airk_core::identity::Incarnation::from_bytes([83; 16]))?;
    let accepted = connection.admitted_read_context(parent.clone(), now)?;
    assert_eq!(accepted.deadline(), now + Duration::from_secs(120));
    assert_eq!(accepted.request_id(), parent.request_id());
    require_original_read_delivery(&accepted, now + Duration::from_secs(70))?;
    // Keep idle-age independently fresh: this refusal is admission expiry.
    connection.last_finished = now + Duration::from_secs(70);
    assert!(
        connection
            .admitted_read_context(parent.clone(), now + Duration::from_secs(70))
            .is_err()
    );
    assert!(require_original_read_delivery(&accepted, now + Duration::from_secs(120)).is_err());
    parent.cancellation().cancel();
    assert!(require_original_read_delivery(&accepted, now + Duration::from_secs(1)).is_err());
    Ok(())
}
#[tokio::test]
async fn actual_connection_keeps_captured_120_and_125_second_wire_parents_past_55_second_reuse_cut()
-> Result<()> {
    for seconds in [120, 125] {
        let owned = tempfile::tempdir()?;
        let root = owned.path().join("runtime");
        local_ipc::prepare_root(&root)?;
        let endpoint = local_ipc::endpoint(&root, "original-read-admission-clock")?;
        let mut listener = local_ipc::Listener::bind(&endpoint)?;
        let now = Instant::now();
        let id = airk_core::identity::Incarnation::from_bytes([84; 16]);
        let parent = airk_mcp::read_attempt::ReadAttemptContext::new(
            now + Duration::from_secs(seconds),
            Default::default(),
        )
        .with_request_id(id)?;
        let original = airk_host::read_attempt::encode(&parent, id)?;
        let context = parent.with_wire_attempt(original)?;
        let arguments = serde_json::json!({"response_mode":"full","include_status":true})
            .as_object()
            .unwrap()
            .clone();
        let expected_arguments = arguments.clone();
        let peer = tokio::spawn(async move {
            let mut stream = listener.accept().await?;
            let query: Query = local_ipc::read_message(&mut stream, Duration::from_secs(5))
                .await?
                .context("original read missing")?;
            let carried = query
                .read_attempt
                .context("captured original parent omitted")?;
            ensure!(
                carried.request_id == original.request_id
                    && carried.clock == original.clock
                    && carried.expires_monotonic_ns == original.expires_monotonic_ns
                    && carried.remaining_ns <= original.remaining_ns,
                "connection admission narrowed/replaced original wire parent"
            );
            ensure!(
                query.tool == "airk_status" && query.arguments == expected_arguments,
                "original canonical input changed"
            );
            local_ipc::write_message(
                &mut stream,
                &protocol::QueryResult {
                    read_admission: None,
                    result: airk_mcp::unified::success(serde_json::json!({"complete":true})),
                    status_receipt: Default::default(),
                    rebind: false,
                },
                Duration::from_secs(5),
            )
            .await?;
            let extra =
                local_ipc::read_message::<_, Query>(&mut stream, Duration::from_secs(5)).await?;
            ensure!(extra.is_none(), "accepted query was replayed");
            Ok::<_, anyhow::Error>(())
        });
        let stream = local_ipc::connect(&endpoint).await?;
        let mut connection = admitted_lifetime_fixture(now);
        connection.stream = Some(stream);
        let value = connection
            .call_with_read_context("airk_status".into(), arguments, context)
            .await?;
        assert_eq!(value["structuredContent"]["complete"], true);
        drop(connection);
        peer.await??;
    }
    Ok(())
}
