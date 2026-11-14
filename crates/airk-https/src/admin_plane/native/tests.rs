// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::json;

fn plane() -> AdminPlane {
    let counter = Arc::new(AtomicU8::new(0));
    AdminPlane::with_sources(
        Arc::new(|_| Box::pin(async { anyhow::bail!("unexpected default dispatch") })),
        Incarnation::from_bytes([0x41; 16]),
        Arc::new(|| 0),
        Arc::new(move || Some([counter.fetch_add(1, Ordering::SeqCst) + 1; 32])),
    )
}
fn deadline() -> u64 {
    now().unwrap().checked_add(120_000_000_000).unwrap()
}
fn open(plane: &AdminPlane) -> NativeAdminSessionSecret {
    match plane.native_open(deadline()) {
        ControlResponse::AdminNativeOpened {
            session,
            service_instance,
        } => {
            assert_eq!(service_instance, Incarnation::from_bytes([0x41; 16]));
            session
        }
        _ => panic!("native open failed"),
    }
}
fn id(value: u8) -> Incarnation {
    Incarnation::from_bytes([value; 16])
}
fn body(text: &str) -> Box<serde_json::value::RawValue> {
    serde_json::value::RawValue::from_string(text.into()).unwrap()
}
fn observed(count: Arc<AtomicUsize>) -> NativeDispatch {
    Arc::new(move |_, check| {
        let count = count.clone();
        Box::pin(async move {
            check().map_err(anyhow::Error::msg)?;
            count.fetch_add(1, Ordering::SeqCst);
            Ok(ControlResponse::ClientErrorRecorded {})
        })
    })
}
fn assert_failure(response: ControlResponse, expected: &str) {
    assert!(matches!(response, ControlResponse::Failure { code } if code == expected));
}

#[test]
fn native_admin_open_close_shares_sessions_without_ending_other_owners() {
    let plane = plane();
    let first = open(&plane);
    let second = open(&plane);
    assert!(plane.native_valid(&first) && plane.native_valid(&second));
    assert!(matches!(
        plane.native_close(first.clone()),
        ControlResponse::AdminNativeClosed {}
    ));
    assert!(!plane.native_valid(&first));
    assert!(plane.native_valid(&second));
    assert_failure(plane.native_open(0), "admin_native_deadline");
    plane.end();
    assert!(!plane.native_valid(&second));
}

#[tokio::test]
async fn native_admin_cancel_before_delivery_prevents_dispatch_and_duplicate_call() {
    let plane = plane();
    let session = open(&plane);
    let count = Arc::new(AtomicUsize::new(0));
    assert!(matches!(
        plane.native_cancel(session.clone(), id(1)),
        ControlResponse::AdminNativeCancelled {}
    ));
    for _ in 0..2 {
        assert_failure(
            plane
                .native_call(
                    session.clone(),
                    id(1),
                    "service.health".into(),
                    body("\"health\""),
                    deadline(),
                    observed(count.clone()),
                    Arc::new(|| true),
                )
                .await,
            "admin_native_outcome_unknown",
        );
    }
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn native_admin_retains_complete_unicode_response_and_refuses_replay() {
    let plane = plane();
    let session = open(&plane);
    let document = "é日本語🙂\\\"".repeat(50_000);
    let expected = json!({"schema":"airk-admin/1", "ok":true, "operation":"service.health",
        "result":{"status":"tools_catalog", "document":document}});
    let copy = document.clone();
    let dispatch: NativeDispatch = Arc::new(move |_, check| {
        let document = copy.clone();
        Box::pin(async move {
            check().map_err(anyhow::Error::msg)?;
            Ok(ControlResponse::ToolsCatalog { document })
        })
    });
    let mut response = plane
        .native_call(
            session.clone(),
            id(2),
            "service.health".into(),
            body("\"health\""),
            deadline(),
            dispatch.clone(),
            Arc::new(|| true),
        )
        .await;
    let mut bytes = Vec::new();
    let mut chunks = 0;
    loop {
        let ControlResponse::AdminNativeChunk { chunk } = response else {
            panic!("missing chunk");
        };
        assert_eq!(chunk.http_status, 200);
        assert_eq!(chunk.offset, bytes.len() as u64);
        chunk.validate().unwrap();
        bytes.extend_from_slice(&chunk.data);
        chunks += 1;
        if chunk.done {
            break;
        }
        response = plane.native_read(session.clone(), id(2), bytes.len() as u64);
    }
    assert!(chunks > 1);
    let actual: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(actual, expected);
    assert_failure(
        plane.native_read(session.clone(), id(2), bytes.len() as u64),
        "admin_native_outcome_unknown",
    );
    assert_failure(
        plane
            .native_call(
                session,
                id(2),
                "service.health".into(),
                body("\"health\""),
                deadline(),
                dispatch,
                Arc::new(|| true),
            )
            .await,
        "admin_native_outcome_unknown",
    );
}

#[tokio::test]
async fn native_admin_cancellation_after_dispatch_suppresses_late_reply() {
    let plane = plane();
    let session = open(&plane);
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let notification = entered.notified();
    let start = entered.clone();
    let finish = release.clone();
    let dispatch: NativeDispatch = Arc::new(move |_, check| {
        let start = start.clone();
        let finish = finish.clone();
        Box::pin(async move {
            check().map_err(anyhow::Error::msg)?;
            start.notify_one();
            finish.notified().await;
            Ok(ControlResponse::ClientErrorRecorded {})
        })
    });
    let call = plane.native_call(
        session.clone(),
        id(3),
        "service.health".into(),
        body("\"health\""),
        deadline(),
        dispatch,
        Arc::new(|| true),
    );
    let cancel_call = async {
        notification.await;
        assert!(matches!(
            plane.native_cancel(session.clone(), id(3)),
            ControlResponse::AdminNativeCancelled {}
        ));
        release.notify_one();
    };
    let (response, ()) = tokio::join!(call, cancel_call);
    assert_failure(response, "admin_native_outcome_unknown");
}

#[tokio::test]
async fn native_admin_body_operation_drain_and_deadline_refusals_have_no_dispatch() {
    let plane = plane();
    let session = open(&plane);
    let count = Arc::new(AtomicUsize::new(0));
    for (index, operation, text) in [
        (4, "unknown.operation", "\"health\""),
        (
            5,
            "service.health",
            "{\"admin_native_open\":{\"deadline_ns\":1}}",
        ),
        (6, "service.health", "\"shutdown\""),
    ] {
        let response = plane
            .native_call(
                session.clone(),
                id(index),
                operation.into(),
                body(text),
                deadline(),
                observed(count.clone()),
                Arc::new(|| true),
            )
            .await;
        let ControlResponse::AdminNativeChunk { chunk } = response else {
            panic!("missing refusal envelope");
        };
        assert!(chunk.http_status >= 400);
    }
    assert_failure(
        plane
            .native_call(
                session.clone(),
                id(7),
                "service.health".into(),
                body("\"health\""),
                0,
                observed(count.clone()),
                Arc::new(|| true),
            )
            .await,
        "admin_native_deadline",
    );
    let response = plane
        .native_call(
            session.clone(),
            id(8),
            "service.health".into(),
            body("\"health\""),
            deadline(),
            observed(count.clone()),
            Arc::new(|| false),
        )
        .await;
    assert!(
        matches!(response, ControlResponse::AdminNativeChunk { chunk } if chunk.http_status == 503)
    );
    let huge = serde_json::to_string(&"日".repeat(MAX_BODY_BYTES / 3 + 1)).unwrap();
    assert_failure(
        plane
            .native_call(
                session,
                id(9),
                "service.health".into(),
                body(&huge),
                deadline(),
                observed(count.clone()),
                Arc::new(|| true),
            )
            .await,
        "admin_native_request_too_large",
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn native_admin_stream_keeps_full_existing_32_mib_boundary_and_strict_offsets() {
    let mut requests = Requests::default();
    let plane = plane();
    let session = open(&plane);
    let permit = plane
        .secrets()
        .sessions
        .native_permit(session.cookie(), session.csrf(), 0)
        .unwrap();
    let key = ([0x31; 32], id(10));
    let bytes = vec![b'x'; MAX_RESPONSE_BYTES as usize];
    requests.entries.insert(
        key,
        Entry {
            deadline: deadline(),
            phase: Arc::new(AtomicU8::new(1)),
            body: Some(Reply {
                status: 200,
                bytes,
                next: 0,
            }),
            reservation: None,
            session: Some(permit),
        },
    );
    assert_failure(requests.chunk(&key, 1, 0), "admin_native_outcome_unknown");
    let mut offset = 0;
    let mut chunks = 0;
    loop {
        let ControlResponse::AdminNativeChunk { chunk } = requests.chunk(&key, offset, 0) else {
            panic!("stream lost");
        };
        offset = chunk.validate().unwrap();
        assert_eq!(chunk.data, vec![b'x'; MAX_CHUNK_BYTES]);
        chunks += 1;
        if chunk.done {
            break;
        }
    }
    assert_eq!((offset, chunks), (33_554_432, 512));
    assert!(requests.entries[&key].reservation.is_none());
}

#[tokio::test]
async fn native_admin_cancel_and_close_keep_capacity_until_real_producers_finish() {
    let plane = plane();
    let session = open(&plane);
    let (entered, mut observed) = tokio::sync::mpsc::channel(2);
    let released = Arc::new(tokio::sync::Notify::new());
    let release = released.clone();
    let dispatch: NativeDispatch = Arc::new(move |_, check| {
        let entered = entered.clone();
        let release = release.clone();
        Box::pin(async move {
            check().map_err(anyhow::Error::msg)?;
            entered.try_send(()).unwrap();
            release.notified().await;
            Ok(ControlResponse::ClientErrorRecorded {})
        })
    });
    let first = plane.native_call(
        session.clone(),
        id(11),
        "service.health".into(),
        body("\"health\""),
        deadline(),
        dispatch.clone(),
        Arc::new(|| true),
    );
    let second = plane.native_call(
        session.clone(),
        id(12),
        "service.health".into(),
        body("\"health\""),
        deadline(),
        dispatch,
        Arc::new(|| true),
    );
    let control = async {
        observed.recv().await.unwrap();
        observed.recv().await.unwrap();
        assert_eq!(
            plane.native.lock().unwrap().charged.load(Ordering::SeqCst),
            2
        );
        assert!(matches!(
            plane.native_cancel(session.clone(), id(11)),
            ControlResponse::AdminNativeCancelled {}
        ));
        assert!(matches!(
            plane.native_close(session.clone()),
            ControlResponse::AdminNativeClosed {}
        ));
        let replacement = open(&plane);
        assert_failure(
            plane
                .native_call(
                    replacement,
                    id(13),
                    "service.health".into(),
                    body("\"health\""),
                    deadline(),
                    observed_dispatch(),
                    Arc::new(|| true),
                )
                .await,
            "admin_native_busy",
        );
        assert_eq!(
            plane.native.lock().unwrap().charged.load(Ordering::SeqCst),
            2
        );
        released.notify_waiters();
    };
    let (first, second, ()) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(first, second, control)
    })
    .await
    .expect("producer/cancellation observations did not complete");
    assert_failure(first, "admin_native_outcome_unknown");
    assert_failure(second, "admin_native_outcome_unknown");
    assert_eq!(
        plane.native.lock().unwrap().charged.load(Ordering::SeqCst),
        0
    );
}

fn observed_dispatch() -> NativeDispatch {
    Arc::new(|_, _| Box::pin(async { anyhow::bail!("capacity refusal dispatched unexpectedly") }))
}

#[tokio::test]
async fn native_admin_expiry_during_dispatch_suppresses_completed_result() {
    let clock = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let clock_source = clock.clone();
    let plane = AdminPlane::with_sources(
        Arc::new(|_| Box::pin(async { anyhow::bail!("unexpected default dispatch") })),
        Incarnation::from_bytes([0x41; 16]),
        Arc::new(move || clock_source.load(Ordering::SeqCst)),
        Arc::new(|| Some([0x23; 32])),
    );
    let session = open(&plane);
    let dispatch: NativeDispatch = Arc::new(move |_, check| {
        let clock = clock.clone();
        Box::pin(async move {
            check().map_err(anyhow::Error::msg)?;
            clock.store(SESSION_IDLE_MS, Ordering::SeqCst);
            assert_eq!(check(), Err("admin_session_invalid"));
            Ok(ControlResponse::ClientErrorRecorded {})
        })
    });
    assert_failure(
        plane
            .native_call(
                session,
                id(14),
                "service.health".into(),
                body("\"health\""),
                deadline(),
                dispatch,
                Arc::new(|| true),
            )
            .await,
        "admin_session_invalid",
    );
    assert_eq!(
        plane.native.lock().unwrap().charged.load(Ordering::SeqCst),
        0
    );
}

#[test]
fn native_admin_chunk_rechecks_expiry_after_session_admission() {
    let plane = plane();
    let session = open(&plane);
    let permit = plane
        .secrets()
        .sessions
        .native_permit(session.cookie(), session.csrf(), 0)
        .unwrap();
    let key = ([0x45; 32], id(15));
    let mut requests = Requests::default();
    requests.entries.insert(
        key,
        Entry {
            deadline: deadline(),
            phase: Arc::new(AtomicU8::new(1)),
            body: Some(Reply {
                status: 200,
                bytes: vec![1; MAX_CHUNK_BYTES + 1],
                next: 0,
            }),
            reservation: None,
            session: Some(permit),
        },
    );
    assert!(matches!(
        requests.chunk(&key, 0, 0),
        ControlResponse::AdminNativeChunk { .. }
    ));
    assert_failure(
        requests.chunk(&key, MAX_CHUNK_BYTES as u64, SESSION_IDLE_MS),
        "admin_session_invalid",
    );
    assert!(requests.entries[&key].body.is_none());
}

#[test]
fn native_admin_chunk_rechecks_original_deadline_at_delivery() {
    let plane = plane();
    let session = open(&plane);
    let permit = plane
        .secrets()
        .sessions
        .native_permit(session.cookie(), session.csrf(), 0)
        .unwrap();
    let key = ([0x46; 32], id(16));
    let mut requests = Requests::default();
    requests.entries.insert(
        key,
        Entry {
            deadline: 0,
            phase: Arc::new(AtomicU8::new(1)),
            body: Some(Reply {
                status: 200,
                bytes: vec![1],
                next: 0,
            }),
            reservation: None,
            session: Some(permit),
        },
    );
    assert_failure(requests.chunk(&key, 0, 0), "admin_native_outcome_unknown");
    assert!(requests.entries[&key].body.is_none());
    assert_eq!(requests.entries[&key].phase.load(Ordering::Acquire), 3);
}
