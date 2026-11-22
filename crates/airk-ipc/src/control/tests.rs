// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::Incarnation,
    read_attempt::{ReadAttemptBudget, ReadClock},
};

fn attempt() -> ReadAttemptBudget {
    ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([1; 16]),
        clock: ReadClock::UnixMonotonic,
        expires_monotonic_ns: 30_000_000_000,
        remaining_ns: 30_000_000_000,
        native_retry_expires_monotonic_ns: None,
    }
}
fn request() -> ControlRequest {
    ControlRequest {
        protocol: PROTOCOL_VERSION,
        expected_instance: None,
        method: ControlMethod::Resources,
    }
}

#[test]
fn arbiter_native_response_budget_uses_original_clock_and_never_refreshes_it() -> Result<()> {
    assert_eq!(
        arbiter_remaining(
            Some(attempt()),
            30_000,
            ReadClock::UnixMonotonic,
            23_000_000_000
        )?,
        Duration::from_secs(7)
    );
    assert_eq!(
        arbiter_remaining(
            Some(attempt()),
            3000,
            ReadClock::UnixMonotonic,
            23_000_000_000
        )?,
        Duration::from_secs(3)
    );
    assert!(
        arbiter_remaining(
            Some(attempt()),
            30_000,
            ReadClock::UnixMonotonic,
            30_000_000_000
        )
        .is_err()
    );
    assert!(
        arbiter_remaining(
            Some(attempt()),
            30_000,
            ReadClock::WindowsPerformanceCounter,
            0
        )
        .is_err()
    );
    assert!(arbiter_remaining(None, 30_000, ReadClock::UnixMonotonic, 0).is_err());
    assert!(arbiter_remaining(Some(attempt()), 120_001, ReadClock::UnixMonotonic, 0).is_err());
    assert_eq!(
        arbiter_remaining(
            Some(attempt()),
            120_000,
            ReadClock::UnixMonotonic,
            23_000_000_000
        )?,
        Duration::from_secs(7)
    );
    let mut malformed = attempt();
    malformed.schema = 2;
    assert!(arbiter_remaining(Some(malformed), 30_000, ReadClock::UnixMonotonic, 0).is_err());
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn arbiter_framed_response_can_arrive_after_five_seconds_inside_original_deadline()
-> Result<()> {
    let (mut client, mut peer) = tokio::io::duplex(4096);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let task = tokio::spawn(async move {
        exchange_once(
            &mut client,
            &request(),
            65536,
            65536,
            Duration::from_secs(5),
            Some(deadline),
        )
        .await
    });
    let received: ControlRequest = local_ipc::read_message(&mut peer, Duration::from_secs(1))
        .await?
        .unwrap();
    assert!(matches!(received.method, ControlMethod::Resources));
    // Advance only after the peer actually received the one complete request.
    tokio::time::advance(Duration::from_secs(6)).await;
    local_ipc::write_message(
        &mut peer,
        &ControlResponse::Failure {
            code: "observed_once".into(),
        },
        Duration::from_secs(1),
    )
    .await?;
    assert!(matches!(task.await??,ControlResponse::Failure {code} if code=="observed_once"));
    let duplicate: Option<ControlRequest> =
        local_ipc::read_message(&mut peer, Duration::from_secs(1)).await?;
    assert!(duplicate.is_none());
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn arbiter_framed_response_stops_at_original_deadline_without_replay() -> Result<()> {
    let (mut client, mut peer) = tokio::io::duplex(4096);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(7);
    let task = tokio::spawn(async move {
        exchange_once(
            &mut client,
            &request(),
            65536,
            65536,
            Duration::from_secs(5),
            Some(deadline),
        )
        .await
    });
    let received: Option<ControlRequest> =
        local_ipc::read_message(&mut peer, Duration::from_secs(1)).await?;
    assert!(received.is_some());
    tokio::time::advance(Duration::from_secs(8)).await;
    let error = match task.await? {
        Err(error) => error,
        Ok(_) => anyhow::bail!("response succeeded after the original deadline"),
    };
    assert!(format!("{error:#}").contains("deadline"));
    let duplicate: Option<ControlRequest> =
        local_ipc::read_message(&mut peer, Duration::from_secs(1)).await?;
    assert!(duplicate.is_none());
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn arbiter_expired_after_held_connect_refuses_before_writing_any_request() -> Result<()> {
    let (mut client, mut peer) = tokio::io::duplex(4096);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    tokio::time::advance(Duration::from_secs(6)).await;
    assert!(
        exchange_once(
            &mut client,
            &request(),
            65536,
            65536,
            Duration::from_secs(5),
            Some(deadline)
        )
        .await
        .is_err()
    );
    drop(client);
    let received: Option<ControlRequest> =
        local_ipc::read_message(&mut peer, Duration::from_secs(1)).await?;
    assert!(received.is_none());
    Ok(())
}

#[test]
fn tools_call_remaining_keeps_long_queries_and_never_refreshes_short_parents() -> Result<()> {
    for seconds in [2u64, 90] {
        let mut original = attempt();
        original.expires_monotonic_ns = seconds * 1_000_000_000;
        original.remaining_ns = original.expires_monotonic_ns;
        assert_eq!(
            read_remaining(
                "airk_search",
                original,
                ReadClock::UnixMonotonic,
                1_000_000_000
            )?,
            Duration::from_secs(seconds - 1)
        );
        assert!(
            read_remaining(
                "airk_search",
                original,
                ReadClock::UnixMonotonic,
                seconds * 1_000_000_000
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn answer_methods_run_to_answer_ceiling_and_focus_keeps_120_seconds() -> Result<()> {
    let mut long = attempt();
    long.expires_monotonic_ns = 605_000_000_000;
    long.remaining_ns = 605_000_000_000;
    // ToolsCall: the tool's own ceiling.
    assert_eq!(
        read_remaining("airk_answer", long, ReadClock::UnixMonotonic, 0)?,
        Duration::from_secs(605)
    );
    assert!(read_remaining("airk_search", long, ReadClock::UnixMonotonic, 0).is_err());
    // Context actions: up to 600,000 ms; Focus and next-read stay at 120,000.
    assert_eq!(
        answer_arbiter_remaining(Some(long), 600_000, ReadClock::UnixMonotonic, 0)?,
        Duration::from_secs(600)
    );
    assert!(answer_arbiter_remaining(Some(long), 600_001, ReadClock::UnixMonotonic, 0).is_err());
    assert_eq!(
        arbiter_remaining(Some(long), 120_000, ReadClock::UnixMonotonic, 0)?,
        Duration::from_secs(120)
    );
    assert!(arbiter_remaining(Some(long), 120_001, ReadClock::UnixMonotonic, 0).is_err());
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn tools_call_frame_retains_original_parent_until_response_and_sends_once() -> Result<()> {
    let (mut client, mut peer) = tokio::io::duplex(4096);
    let mut original = attempt();
    original.expires_monotonic_ns = 2_000_000_000;
    original.remaining_ns = 2_000_000_000;
    let expected = original;
    let request = ControlRequest {
        protocol: PROTOCOL_VERSION,
        expected_instance: None,
        method: ControlMethod::ToolsCall {
            tool: "airk_answer".into(),
            arguments: serde_json::Map::new(),
            selection: airk_core::service::ToolSelection::Unselected,
            response_mode: airk_core::service::ToolResponseMode::Full,
            usage_client: airk_core::usage::ClientFamily::Cli,
            read_attempt: Some(original),
        },
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let task = tokio::spawn(async move {
        exchange_once(
            &mut client,
            &request,
            65536,
            65536,
            Duration::from_secs(135),
            Some(deadline),
        )
        .await
    });
    let received: ControlRequest = local_ipc::read_message(&mut peer, Duration::from_secs(1))
        .await?
        .unwrap();
    let ControlMethod::ToolsCall {
        read_attempt: Some(observed),
        ..
    } = received.method
    else {
        anyhow::bail!("missing original parent")
    };
    assert_eq!(observed.request_id, expected.request_id);
    assert_eq!(observed.expires_monotonic_ns, expected.expires_monotonic_ns);
    assert_eq!(observed.remaining_ns, expected.remaining_ns);
    tokio::time::advance(Duration::from_secs(3)).await;
    assert!(task.await?.is_err());
    let duplicate: Option<ControlRequest> =
        local_ipc::read_message(&mut peer, Duration::from_secs(1)).await?;
    assert!(duplicate.is_none());
    Ok(())
}

#[test]
fn device_management_frame_keeps_exact_policy_precision_and_legacy_cpu_separate() -> Result<()> {
    let literal = serde_json::json!({"arbiter":{"request":{"operation":"configure_device","stage":"context","expected_revision":7,"configuration":{"enabled":false,"selected_model":"context_compact","device_policy":"auto","gpu_precision":"fp16","indexes":[]}}}});
    let method: airk_core::service::ControlMethod = serde_json::from_value(literal.clone())?;
    assert_eq!(serde_json::to_value(method)?, literal);
    let legacy = serde_json::json!({"arbiter":{"request":{"operation":"configure","stage":"focus","expected_revision":7,"enabled":false,"selected_profile":null,"indexes":[]}}});
    let method: airk_core::service::ControlMethod = serde_json::from_value(legacy.clone())?;
    assert_eq!(serde_json::to_value(method)?, legacy);
    let mut foreign = literal.clone();
    foreign["arbiter"]["request"]["configuration"]["gpu_precision"] = serde_json::json!("bf16");
    assert!(serde_json::from_value::<airk_core::service::ControlMethod>(foreign).is_err());
    let mut legacy_gpu = legacy;
    legacy_gpu["arbiter"]["request"]["gpu_precision"] = serde_json::json!("fp16");
    assert!(serde_json::from_value::<airk_core::service::ControlMethod>(legacy_gpu).is_err());
    Ok(())
}

#[test]
fn current_private_import_preserves_directory_and_cannot_become_public_management() -> Result<()> {
    let directory = if cfg!(windows) {
        r"D:\AIRK Models\verified"
    } else {
        "/opt/AIRK Models/verified"
    };
    let literal = serde_json::json!({"arbiter_import_device":{"request":{"expected_revision":7,"profile":"1111111111111111111111111111111111111111111111111111111111111111","directory":directory}}});
    let method: airk_core::service::ControlMethod = serde_json::from_value(literal.clone())?;
    assert_eq!(serde_json::to_value(method)?, literal);
    assert!(serde_json::from_value::<airk_core::arbiter::ManagementRequest>(serde_json::json!({"operation":"import_device","expected_revision":7,"profile":"1111111111111111111111111111111111111111111111111111111111111111","directory":directory})).is_err());
    Ok(())
}
