// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::service_events::{EventCursor, EventKind, EventPage, ServiceEvent};

fn page(first: u64, count: u64, more: bool) -> ControlResponse {
    let namespace = Incarnation::from_bytes([0x0a; 16]);
    let last = first + count - 1;
    let through = if more { 300 } else { last };
    ControlResponse::Events {
        page: EventPage {
            namespace,
            oldest_sequence: Some(1),
            snapshot_through: through,
            retained_prefix_lost: false,
            events: (first..=last)
                .map(|sequence| ServiceEvent {
                    sequence,
                    observed_unix_ms: sequence * 10,
                    project: Sha256Digest::from_bytes([0xab; 32]),
                    kind: EventKind::Registered,
                    job: None,
                    job_kind: None,
                    fencing_generation: None,
                    attempts: None,
                    code: None,
                })
                .collect(),
            next: more.then_some(EventCursor {
                namespace,
                project: None,
                after: last,
                through: Some(through),
            }),
            tail: EventCursor {
                namespace,
                project: None,
                after: through,
                through: None,
            },
        },
    }
}

async fn open(plane: &AdminPlane, cookie: &str, csrf: &str, body: Value) -> HttpResponse {
    let mut request = api_call(cookie, csrf, "events", &body.to_string());
    *request.uri_mut() = "/admin/api/v1/events".parse().unwrap();
    plane
        .handle(
            request,
            Some(ConnectionPeer::proven(PeerProof::SameUser)),
            PORT,
            false,
        )
        .await
}

async fn frame(response: &mut HttpResponse) -> Value {
    let frame = tokio::time::timeout(Duration::from_secs(5), response.body_mut().frame())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    serde_json::from_slice(&frame.into_data().unwrap()).unwrap()
}

#[tokio::test]
async fn admin_events_preserve_full_native_event_and_reconnect_with_session_cursor() {
    let (plane, sent, clock) =
        r04_recovery_plane(vec![Some(page(1, 1, false)), Some(page(2, 1, false))]);
    let (cookie, csrf) = sign_in(&plane).await;
    let mut first = open(&plane, &cookie, &csrf, json!({})).await;
    assert_eq!(first.status(), 200);
    assert_eq!(first.headers()["content-type"], "application/x-ndjson");
    let event = frame(&mut first).await;
    assert_eq!(event["kind"], "event");
    assert_eq!(
        event["event"],
        json!({"sequence":1,"observed_unix_ms":10,
        "project":"ab".repeat(32),"kind":"registered"})
    );
    let cursor = event["cursor"].as_str().unwrap().to_owned();
    drop(first);
    let mut second = open(&plane, &cookie, &csrf, json!({"cursor":cursor})).await;
    assert_eq!(frame(&mut second).await["event"]["sequence"], 2);
    let methods = sent.lock().unwrap();
    assert_eq!(methods.len(), 2);
    assert_eq!(
        methods[1],
        json!({"events":{"project":null,"cursor":{"namespace":"0a".repeat(16),
        "project":null,"after":1,"through":null},"limit":32}})
    );
    drop(methods);
    clock.store(contract::STREAM_SECONDS * 1000, Ordering::SeqCst);
    drop(second);
}

#[tokio::test]
async fn admin_events_stream_caps_release_on_body_drop_and_cursors_do_not_cross_sessions() {
    let (plane, _, _) = r04_recovery_plane(vec![Some(page(1, 1, false))]);
    let (cookie, csrf) = sign_in(&plane).await;
    let mut first = open(&plane, &cookie, &csrf, json!({})).await;
    let cursor = frame(&mut first).await["cursor"].clone();
    let second = open(&plane, &cookie, &csrf, json!({})).await;
    let refused = open(&plane, &cookie, &csrf, json!({})).await;
    assert_eq!(refused.status(), 503);
    drop(first);
    let third = open(&plane, &cookie, &csrf, json!({})).await;
    assert_eq!(third.status(), 200);
    let (other, other_csrf) = sign_in(&plane).await;
    let mut foreign = open(&plane, &other, &other_csrf, json!({"cursor":cursor})).await;
    assert_eq!(
        frame(&mut foreign).await,
        json!({"kind":"resync","reason":"native_gap"})
    );
    assert_eq!(frame(&mut foreign).await, json!({"kind":"closing"}));
    drop((second, third));
}

#[tokio::test]
async fn admin_events_expiry_logout_restart_and_sequence_gap_are_distinct() {
    for reason in [
        "session_expired",
        "session_ended",
        "service_restarted",
        "native_gap",
    ] {
        let (plane, _, clock) =
            r04_recovery_plane(vec![Some(page(1, 1, false)), Some(page(3, 1, false))]);
        let notify = Arc::new(tokio::sync::Notify::new());
        let wake = notify.clone();
        plane.secrets().event_wait = Some(Arc::new(move || {
            let wake = wake.clone();
            Box::pin(async move { wake.notified().await })
        }));
        let (cookie, csrf) = sign_in(&plane).await;
        let mut response = open(&plane, &cookie, &csrf, json!({})).await;
        frame(&mut response).await;
        match reason {
            "session_expired" => clock.store(SESSION_IDLE_MS, Ordering::SeqCst),
            "session_ended" => {
                plane.secrets().sessions.remove(&cookie);
            }
            "service_restarted" => plane.end(),
            "native_gap" => {}
            _ => unreachable!(),
        }
        notify.notify_one();
        assert_eq!(
            frame(&mut response).await,
            json!({"kind":"resync","reason":reason}),
            "{reason}"
        );
        assert_eq!(frame(&mut response).await, json!({"kind":"closing"}));
    }
}

#[tokio::test]
async fn admin_events_queue_overflow_is_disclosed_without_unbounded_buffering() {
    let (mut plane, _, _) =
        r04_recovery_plane((0..9).map(|n| Some(page(n * 32 + 1, 32, true))).collect());
    let next = Arc::new(AtomicU64::new(1));
    plane.random = Arc::new(move || {
        let mut bytes = [0u8; 32];
        bytes[..8].copy_from_slice(&next.fetch_add(1, Ordering::SeqCst).to_le_bytes());
        Some(bytes)
    });
    let notified = Arc::new(tokio::sync::Notify::new());
    let entered = notified.clone();
    let base = plane.dispatch.clone();
    let polls = Arc::new(AtomicU64::new(0));
    plane.dispatch = Arc::new(move |method| {
        let base = base.clone();
        let entered = entered.clone();
        let polls = polls.clone();
        Box::pin(async move {
            let response = base(method).await;
            if polls.fetch_add(1, Ordering::SeqCst) == 8 {
                entered.notify_one();
            }
            response
        })
    });
    let (cookie, csrf) = sign_in(&plane).await;
    let mut response = open(&plane, &cookie, &csrf, json!({})).await;
    tokio::time::timeout(Duration::from_secs(5), notified.notified())
        .await
        .unwrap();
    let bytes = response.body_mut().collect().await.unwrap().to_bytes();
    let lines: Vec<Value> = std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 258);
    assert_eq!(lines[256], json!({"kind":"resync","reason":"overflow"}));
    assert_eq!(lines[257], json!({"kind":"closing"}));
    assert!(bytes.len() < contract::STREAM_BUFFER_BYTES);
}

#[tokio::test]
async fn admin_exchange_rate_and_separate_request_capacity_use_injected_observations() {
    let (plane, clock) = plane(1);
    for attempt in 0..9 {
        let request = request(
            "POST",
            "/admin/api/v1/session",
            &[HOST, ORIGIN, SAME_ORIGIN, JSON],
            &json!({"code":"A".repeat(43)}).to_string(),
        );
        let (status, _, bytes) = answer(&plane, request, PeerProof::SameUser).await;
        assert_eq!(status, if attempt < 8 { 401 } else { 503 });
        if attempt == 8 {
            assert_eq!(code_of(&bytes), "admin_exchange_rate_limited");
        }
    }
    clock.store(60_000, Ordering::SeqCst);
    let (cookie, csrf) = sign_in(&plane).await;
    let permits: Vec<_> = (0..16)
        .map(|_| plane.admit_admin_request(false).unwrap())
        .collect();
    let response = open(&plane, &cookie, &csrf, json!({})).await;
    assert_eq!(response.status(), 503);
    drop(permits);
    let response = open(&plane, &cookie, &csrf, json!({})).await;
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn admin_events_global_slots_remain_bounded_until_body_end_or_drop() {
    let (plane, _, clock) = r04_recovery_plane((0..16).map(|_| Some(page(1, 1, false))).collect());
    let mut bodies = Vec::new();
    let mut first_cookie = String::new();
    for index in 0..8 {
        let (cookie, csrf) = sign_in(&plane).await;
        if index == 0 {
            first_cookie = cookie.clone();
        }
        for _ in 0..2 {
            let response = open(&plane, &cookie, &csrf, json!({})).await;
            assert_eq!(response.status(), 200);
            bodies.push(response);
        }
    }
    // Session end must not free the global allocation while its body still
    // owns buffered bytes. Delivery/drop owns final reclamation.
    plane.secrets().sessions.remove(&first_cookie);
    clock.store(60_000, Ordering::SeqCst);
    let (cookie, csrf) = sign_in(&plane).await;
    assert_eq!(open(&plane, &cookie, &csrf, json!({})).await.status(), 503);
    drop(bodies.remove(0));
    assert_eq!(open(&plane, &cookie, &csrf, json!({})).await.status(), 200);
}

#[tokio::test]
async fn admin_events_deadline_closes_without_expiring_or_renewing_the_session() {
    let (plane, _, clock) = r04_recovery_plane(vec![Some(page(1, 1, false))]);
    let notify = Arc::new(tokio::sync::Notify::new());
    let wake = notify.clone();
    plane.secrets().event_wait = Some(Arc::new(move || {
        let wake = wake.clone();
        Box::pin(async move { wake.notified().await })
    }));
    let (cookie, csrf) = sign_in(&plane).await;
    let mut response = open(&plane, &cookie, &csrf, json!({})).await;
    frame(&mut response).await;
    clock.store(120_000, Ordering::SeqCst);
    notify.notify_one();
    assert_eq!(frame(&mut response).await, json!({"kind":"closing"}));
    assert!(plane.secrets().sessions.stream_live(&cookie, 120_000));
    clock.store(SESSION_IDLE_MS, Ordering::SeqCst);
    assert!(
        !plane
            .secrets()
            .sessions
            .stream_live(&cookie, SESSION_IDLE_MS)
    );
}

#[tokio::test]
async fn admin_events_expiry_revokes_the_sessions_native_selection_without_another_request() {
    let (plane, _, clock) = r04_recovery_plane(vec![Some(page(1, 1, false))]);
    let revoked = Arc::new(Mutex::new(Vec::new()));
    let seen = revoked.clone();
    let plane = plane.with_selection_authority(SelectionAuthority {
        register: Arc::new(|_| true),
        revoke: Arc::new(move |nonces| seen.lock().unwrap().extend(nonces)),
    });
    let notify = Arc::new(tokio::sync::Notify::new());
    let wake = notify.clone();
    plane.secrets().event_wait = Some(Arc::new(move || {
        let wake = wake.clone();
        Box::pin(async move { wake.notified().await })
    }));
    let (cookie, csrf) = sign_in(&plane).await;
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "search.select_scope",
            &grant_body(GRANTED_INDEX, 12),
        ),
    )
    .await;
    assert_eq!(status, 200, "{reply}");
    let mut response = open(&plane, &cookie, &csrf, json!({})).await;
    frame(&mut response).await;
    assert!(revoked.lock().unwrap().is_empty());
    clock.store(SESSION_IDLE_MS, Ordering::SeqCst);
    notify.notify_one();
    assert_eq!(
        frame(&mut response).await,
        json!({"kind":"resync","reason":"session_expired"})
    );
    assert_eq!(
        revoked.lock().unwrap().len(),
        1,
        "the stream releases the owning selection via SecretsGuard"
    );
}
