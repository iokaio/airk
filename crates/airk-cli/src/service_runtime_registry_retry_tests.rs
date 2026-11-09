// SPDX-License-Identifier: Apache-2.0
//! Exact registry admission retries at the actual shared native control seam.
use super::*;
use airk_core::{
    identity::Incarnation,
    service::{JobKind, ProjectBinding, RequestKey},
};

fn busy() -> ControlResponse {
    ControlResponse::Failure {
        code: "registry_busy".into(),
    }
}

#[tokio::test]
async fn registry_busy_retry_retains_full_mutation_and_instance_on_all_request_variants()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    local_ipc::prepare_root(owner.path())?;
    let address = local_ipc::endpoint(owner.path(), "control")?;
    let mut listener = Listener::bind(&address)?;
    let instance = Incarnation::from_bytes([31; 16]);
    let original = ControlMethod::Enqueue {
        project: ProjectBinding::from_bytes([32; 32]),
        caller: Incarnation::from_bytes([33; 16]),
        key: RequestKey {
            namespace: Incarnation::from_bytes([34; 16]),
            epoch: 7,
            nonce: Incarnation::from_bytes([35; 16]),
        },
        kind: JobKind::Index,
    };
    let expected = serde_json::to_value(&original)?;
    let server = tokio::spawn(async move {
        for variant in 0..4 {
            for attempt in 0..2 {
                let mut stream = listener.accept().await?;
                let received: ControlRequest =
                    local_ipc::read_message(&mut stream, Duration::from_secs(5))
                        .await?
                        .unwrap();
                assert_eq!(received.protocol, PROTOCOL_VERSION);
                assert_eq!(
                    received.expected_instance,
                    (variant >= 2).then_some(instance)
                );
                assert_eq!(serde_json::to_value(received.method)?, expected);
                let response = if attempt == 0 {
                    busy()
                } else {
                    ControlResponse::Stopping
                };
                local_ipc::write_message(&mut stream, &response, Duration::from_secs(5)).await?;
            }
        }
        Ok::<_, anyhow::Error>(())
    });
    for variant in 0..4 {
        let response = match variant {
            0 => request(owner.path(), original.clone()).await,
            1 => {
                request_for_version(owner.path(), original.clone(), env!("CARGO_PKG_VERSION")).await
            }
            2 => request_for_instance(owner.path(), original.clone(), instance).await,
            _ => {
                request_for_version_and_instance(
                    owner.path(),
                    original.clone(),
                    env!("CARGO_PKG_VERSION"),
                    instance,
                )
                .await
            }
        }?;
        assert!(matches!(response, ControlResponse::Stopping));
    }
    server.await??;
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn registry_busy_retry_has_six_increasing_waits_and_keeps_last_busy() -> Result<()> {
    let mut dispatches = Vec::new();
    let response = registry_busy_retry(|| {
        dispatches.push(tokio::time::Instant::now());
        std::future::ready(Ok(busy()))
    })
    .await?;
    assert!(matches!(response, ControlResponse::Failure { code } if code == "registry_busy"));
    assert_eq!(dispatches.len(), 7);
    for (pair, millis) in dispatches.windows(2).zip([100, 200, 400, 600, 700, 800]) {
        assert!(pair[1].duration_since(pair[0]) >= Duration::from_millis(millis));
    }
    assert!(dispatches[6].duration_since(dispatches[0]) < Duration::from_secs(3));
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn registry_busy_retry_never_redispatches_after_original_window_or_unknown_response()
-> Result<()> {
    let mut calls = 0;
    let response = registry_busy_retry(|| {
        calls += 1;
        async {
            tokio::time::sleep(Duration::from_millis(2950)).await;
            Ok(busy())
        }
    })
    .await?;
    assert_eq!(calls, 1);
    assert!(matches!(response, ControlResponse::Failure { code } if code == "registry_busy"));
    for code in [
        "registry_response_unknown_retry_with_key",
        "invalid_registry_request",
        "catalog_preparation_rejected",
        "source_change_jobs_pending",
    ] {
        calls = 0;
        let response = registry_busy_retry(|| {
            calls += 1;
            std::future::ready(Ok(ControlResponse::Failure { code: code.into() }))
        })
        .await?;
        assert_eq!(calls, 1);
        assert!(
            matches!(response, ControlResponse::Failure { code: observed } if observed == code)
        );
    }
    calls = 0;
    let error = registry_busy_retry(|| {
        calls += 1;
        std::future::ready(Err(anyhow::anyhow!("transport outcome unknown")))
    })
    .await
    .err()
    .expect("transport failure must be retained");
    assert_eq!(calls, 1);
    assert_eq!(error.to_string(), "transport outcome unknown");
    Ok(())
}
