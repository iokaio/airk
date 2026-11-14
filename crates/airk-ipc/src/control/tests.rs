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
    assert!(arbiter_remaining(Some(attempt()), 30_001, ReadClock::UnixMonotonic, 0).is_err());
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
