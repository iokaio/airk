// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::Sha256Digest,
    index_catalog::{CatalogRevision, GrantId, IndexId, QueryProjectId},
    service::ScopedConnectRequest,
    service_http::{EndpointState, HttpSelection, LoopbackMcpUrl},
};
use airk_endpoint::http_endpoint::{InstallationBinding, TlsIdentity, read_client_token};
use airk_mcp::remote::{RemoteFuture, RemoteTools};
use anyhow::Context as _;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Echo;

#[tokio::test]
async fn saturated_authentication_refuses_before_native_validation() -> anyhow::Result<()> {
    let f = fixture(57422)?;
    let permits = f
        .gateway
        .0
        .authentication
        .clone()
        .acquire_many_owned(AUTH_TASKS as u32)
        .await?;
    let refused = f
        .gateway
        .handle(
            request(&f, "/api/v1/tools/airk_search"),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: serde_json::Value =
        serde_json::from_slice(&refused.into_body().collect().await?.to_bytes())?;
    assert_eq!(
        body,
        serde_json::json!({"schema":"airk.local-api-error/1","error":{"code":"authentication_busy"}})
    );
    assert_eq!(f.checks.load(Ordering::SeqCst), 0);
    assert_eq!(f.gateway.0.validation.available_permits(), AUTH_TASKS);
    drop(permits);
    assert_eq!(f.gateway.0.authentication.available_permits(), AUTH_TASKS);
    f.gateway.close().await;
    Ok(())
}

#[tokio::test]
async fn abandoned_native_validation_retains_its_bounded_blocking_capacity() -> anyhow::Result<()> {
    let mut f = fixture(57421)?;
    let block = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    struct Release(Arc<(Mutex<bool>, std::sync::Condvar)>);
    impl Drop for Release {
        fn drop(&mut self) {
            *self.0.0.lock().unwrap() = true;
            self.0.1.notify_all();
        }
    }
    let release = Release(block.clone());
    let started = Arc::new(AtomicUsize::new(0));
    let observed = started.clone();
    Arc::get_mut(&mut f.gateway.0).unwrap().validate = Arc::new(move |_, context| {
        let block = block.clone();
        let started = observed.clone();
        Box::pin(async move {
            let guard = context.lifetime_guard();
            tokio::task::spawn_blocking(move || {
                let _guard = guard;
                started.fetch_add(1, Ordering::SeqCst);
                let (open, wake) = &*block;
                let mut open = open.lock().unwrap();
                while !*open {
                    open = wake.wait(open).unwrap();
                }
                Ok(Arc::new(|| Ok(Arc::new(Echo) as Arc<dyn RemoteTools>))
                    as AuthenticatedBackendFactory)
            })
            .await?
        })
    });
    let mut callers = Vec::new();
    for expected in 1..=AUTH_TASKS {
        let gateway = f.gateway.clone();
        let input = request(&f, "/api/v1/tools/airk_search");
        callers.push(tokio::spawn(async move {
            gateway.handle(input, CancellationToken::new()).await
        }));
        tokio::time::timeout(Duration::from_secs(10), async {
            while started.load(Ordering::SeqCst) < expected {
                tokio::task::yield_now().await;
            }
        })
        .await?;
    }
    for caller in callers {
        caller.abort();
        let _ = caller.await;
    }
    assert_eq!(f.gateway.0.validation.available_permits(), 0);
    let refused = f
        .gateway
        .handle(
            request(&f, "/api/v1/tools/airk_search"),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(refused.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: serde_json::Value =
        serde_json::from_slice(&refused.into_body().collect().await?.to_bytes())?;
    assert_eq!(body["error"]["code"], "native_validation_busy");
    drop(release);
    tokio::time::timeout(Duration::from_secs(10), async {
        while f.gateway.0.validation.available_permits() != AUTH_TASKS {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    f.gateway.close().await;
    Ok(())
}

impl RemoteTools for Echo {
    fn call(
        &self,
        name: String,
        args: serde_json::Map<String, serde_json::Value>,
    ) -> RemoteFuture<'_> {
        Box::pin(async move {
            Ok(
                serde_json::json!({"content":[],"structuredContent":{"tool":name,"args":args,"future":{"retained":true}},"isError":false}),
            )
        })
    }
}
struct Fixture {
    _root: tempfile::TempDir,
    gateway: Gateway,
    token: String,
    id: Incarnation,
    checks: Arc<AtomicUsize>,
    fenced: Arc<AtomicBool>,
    ca: Vec<u8>,
}
fn fixture(port: u16) -> anyhow::Result<Fixture> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("http");
    crate::local_ipc::prepare_root(&path)?;
    let binding = InstallationBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        user: crate::local_ipc::current_user()?,
        namespace: Incarnation::from_bytes([2; 16]),
    };
    let identity = TlsIdentity::load_or_create(&path, binding.clone(), now_ms())?;
    let bundle: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path.join("tls-identity.json"))?)?;
    let ca = serde_json::from_value(bundle["ca_der"].clone())?;
    let endpoint = EndpointDescriptor {
        schema: 1,
        installation: binding.installation,
        user: binding.user.clone(),
        namespace: binding.namespace,
        generation: 1,
        service_instance: Incarnation::from_bytes([3; 16]),
        executable_sha256: Sha256Digest::from_bytes([4; 32]),
        url: LoopbackMcpUrl::new(port)?,
        ca_sha256: identity.ca_sha256(),
        certificate_sha256: identity.certificate_sha256(),
        state: EndpointState::Ready,
        predecessor_digest: None,
    };
    let id = Incarnation::from_bytes([5; 16]);
    let mut credentials = CredentialStore::open(&path, binding.clone())?;
    let selection = HttpSelection::Scoped {
        selection: ScopedConnectRequest {
            schema: 1,
            context_key: Sha256Digest::from_bytes([6; 32]),
            namespace: binding.namespace,
            expected_catalog_revision: CatalogRevision::INITIAL,
            grant_id: GrantId::from_identity(id),
            expected_grant_revision: CatalogRevision::INITIAL,
            index_id: IndexId::from_identity(id),
            project_ids: vec![QueryProjectId::from_identity(id)],
        },
    };
    credentials.enroll(id, selection, now_ms() + 60_000, now_ms())?;
    let token = read_client_token(&path, id)?;
    let checks = Arc::new(AtomicUsize::new(0));
    let checked = checks.clone();
    let validate: ValidateRequest = Arc::new(move |_, _| {
        checked.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(Arc::new(|| Ok(Arc::new(Echo) as Arc<dyn RemoteTools>))
                as AuthenticatedBackendFactory)
        })
    });
    let fenced = Arc::new(AtomicBool::new(false));
    let fence = fenced.clone();
    let (_sender, tls) = watch::channel(identity.acceptor());
    let native_gate = NativeCatalogAdmission::new(
        endpoint.service_instance,
        endpoint.namespace,
        CatalogRevision::INITIAL,
        1,
    )?;
    let gateway = Gateway::new(
        &endpoint,
        Arc::new(Mutex::new(credentials)),
        tls,
        validate,
        NativeAdmission {
            closed: Arc::new(move || fence.load(Ordering::SeqCst)),
            gate: native_gate,
            admit: Arc::new(|| Ok(Arc::new(()))),
            settle: Arc::new(|_| Ok(())),
        },
    )?;
    Ok(Fixture {
        _root: root,
        gateway,
        token,
        id,
        checks,
        fenced,
        ca,
    })
}
fn request(f: &Fixture, path: &str) -> Request<Full<Bytes>> {
    Request::builder()
        .method(http::Method::POST)
        .uri(path)
        .header("host", &f.gateway.0.authority)
        .header("authorization", format!("Bearer {}", f.token))
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from_static(b"{}")))
        .unwrap()
}
#[tokio::test]
async fn every_request_revalidates_credentials_and_grants_before_reusing_its_session_context()
-> anyhow::Result<()> {
    let f = fixture(57421)?;
    for _ in 0..2 {
        let response = f
            .gateway
            .handle(
                request(&f, "/api/v1/tools/airk_search"),
                CancellationToken::new(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let value: serde_json::Value =
            serde_json::from_slice(&response.into_body().collect().await?.to_bytes())?;
        assert_eq!(value["structuredContent"]["future"]["retained"], true);
    }
    assert_eq!(f.checks.load(Ordering::SeqCst), 2);
    assert_eq!(f.gateway.0.clients.lock().unwrap().len(), 1);
    f.gateway.0.credentials.lock().unwrap().revoke(f.id)?;
    assert_eq!(
        f.gateway
            .handle(
                request(&f, "/api/v1/tools/airk_search"),
                CancellationToken::new()
            )
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.checks.load(Ordering::SeqCst),
        2,
        "revoked ledger cannot reach native validation or cached SDK context"
    );
    f.gateway.revoke(f.id).await;
    assert!(f.gateway.0.clients.lock().unwrap().is_empty());
    f.gateway.close().await;
    Ok(())
}
#[tokio::test]
async fn canceled_authentication_wait_keeps_bounded_work_until_lock_settles() -> anyhow::Result<()>
{
    let f = fixture(57421)?;
    let store = f.gateway.0.credentials.clone();
    let (locked_tx, locked_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _guard = store.lock().unwrap();
        locked_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    locked_rx.await?;
    let canceled = CancellationToken::new();
    let gateway = f.gateway.clone();
    let request = request(&f, "/api/v1/tools/airk_search");
    let token = canceled.clone();
    let task = tokio::spawn(async move { gateway.handle(request, token).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while f.gateway.0.authentication.available_permits() != AUTH_TASKS - 1 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await?;
    canceled.cancel();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), task)
            .await??
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        f.gateway.0.authentication.available_permits(),
        AUTH_TASKS - 1
    );
    assert_eq!(f.checks.load(Ordering::SeqCst), 0);
    release_tx.send(())?;
    holder.join().unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while f.gateway.0.authentication.available_permits() != AUTH_TASKS {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await?;
    assert_eq!(f.checks.load(Ordering::SeqCst), 0);
    f.gateway.close().await;
    Ok(())
}

#[tokio::test]
async fn unrelated_revocation_fences_old_authentication_as_unavailable_not_invalid_credential()
-> anyhow::Result<()> {
    let f = fixture(57421)?;
    let other_id = Incarnation::from_bytes([47; 16]);
    let other_token = {
        let mut store = f.gateway.0.credentials.lock().unwrap();
        let selection = store.authenticate(&f.token, now_ms())?.selection;
        let issued = store.enroll_default(other_id, selection, now_ms())?;
        std::fs::read_to_string(issued.credential_file)?
    };
    let store = f.gateway.0.credentials.clone();
    let (locked_tx, locked_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || -> anyhow::Result<()> {
        let mut guard = store.lock().unwrap();
        guard.revoke(other_id)?;
        let _ = locked_tx.send(());
        // Keep actual asynchronous authentication blocked at its native ledger
        // read until the durable unrelated revocation reaches the gateway.
        let _ = release_rx.recv_timeout(Duration::from_secs(10));
        Ok(())
    });
    locked_rx.await?;
    let gateway = f.gateway.clone();
    let input = request(&f, "/api/v1/tools/airk_search");
    let pending =
        tokio::spawn(async move { gateway.handle(input, CancellationToken::new()).await });
    tokio::time::timeout(Duration::from_secs(5), async {
        while f.gateway.0.authentication.available_permits() != AUTH_TASKS - 1 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    f.gateway.revoke(other_id).await;
    release_tx.send(())?;
    holder.join().unwrap()?;
    let response = pending.await?;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await?.to_bytes())?;
    assert_eq!(
        body["error"]["code"],
        "credential_authentication_epoch_retired"
    );
    assert_eq!(f.checks.load(Ordering::SeqCst), 0);
    let fresh = f
        .gateway
        .handle(
            request(&f, "/api/v1/tools/airk_search"),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(fresh.status(), StatusCode::OK);
    let _ = fresh.into_body().collect().await?;
    let mut revoked = request(&f, "/api/v1/tools/airk_search");
    revoked.headers_mut().insert(
        http::header::AUTHORIZATION,
        format!("Bearer {other_token}").parse()?,
    );
    let refused = f.gateway.handle(revoked, CancellationToken::new()).await;
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value =
        serde_json::from_slice(&refused.into_body().collect().await?.to_bytes())?;
    assert_eq!(body["error"]["code"], "credential_refused");
    f.gateway.close().await;
    Ok(())
}

#[tokio::test]
async fn hostile_headers_authentication_saturation_and_checkpoint_fence_are_bounded()
-> anyhow::Result<()> {
    let f = fixture(57421)?;
    let mut hostile = request(&f, "/mcp");
    hostile
        .headers_mut()
        .insert("origin", "https://hostile.example".parse()?);
    assert_eq!(
        f.gateway
            .handle(hostile, CancellationToken::new())
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let mut hostile = request(&f, "/mcp");
    hostile
        .headers_mut()
        .insert("host", "localhost:57421".parse()?);
    assert_eq!(
        f.gateway
            .handle(hostile, CancellationToken::new())
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(f.checks.load(Ordering::SeqCst), 0);
    let permit = f
        .gateway
        .0
        .authentication
        .clone()
        .acquire_many_owned(AUTH_TASKS as u32)
        .await?;
    assert_eq!(
        f.gateway
            .handle(request(&f, "/mcp"), CancellationToken::new())
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    drop(permit);
    f.fenced.store(true, Ordering::SeqCst);
    assert_eq!(
        f.gateway
            .handle(request(&f, "/mcp"), CancellationToken::new())
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let mut catalog = request(&f, "/api/v1/tools");
    *catalog.method_mut() = http::Method::GET;
    assert_eq!(
        f.gateway
            .handle(catalog, CancellationToken::new())
            .await
            .status(),
        StatusCode::OK
    );
    f.gateway.close().await;
    Ok(())
}
fn output(bytes: Bytes, budget: Arc<Semaphore>, limit: usize) -> OutputBody {
    OutputBody::new(
        Full::new(bytes).boxed(),
        budget,
        limit,
        ReadAttemptContext::new(
            Instant::now() + Duration::from_secs(125),
            CancellationToken::new(),
        ),
    )
}
#[tokio::test]
async fn output_byte_charge_follows_actual_network_clones_and_never_silently_truncates()
-> anyhow::Result<()> {
    let budget = Arc::new(Semaphore::new(10));
    let mut body = output(Bytes::from_static(b"123456"), budget.clone(), 10);
    let frame = body.frame().await.unwrap()?.into_data().unwrap();
    let cloned = frame.clone();
    assert_eq!(budget.available_permits(), 4);
    drop(body);
    drop(frame);
    assert_eq!(
        budget.available_permits(),
        4,
        "network clone retains full memory charge"
    );
    drop(cloned);
    assert_eq!(budget.available_permits(), 10);
    let mut oversized = output(Bytes::from_static(b"123456"), budget.clone(), 5);
    assert!(oversized.frame().await.unwrap().is_err());
    assert!(oversized.frame().await.is_none());
    let held = budget.clone().acquire_many_owned(10).await?;
    let mut saturated = output(Bytes::from_static(b"1"), budget.clone(), 10);
    assert!(
        saturated.frame().await.unwrap().is_err(),
        "aggregate saturation explicitly fails the stream without an output wait queue"
    );
    drop(held);
    let mut large = output(
        Bytes::from(vec![0; MAX_FRAME + 1]),
        Arc::new(Semaphore::new(OUTPUT_BYTES)),
        OUTPUT_BYTES,
    );
    assert!(large.frame().await.unwrap().is_err());
    Ok(())
}
#[tokio::test]
async fn real_loopback_tls_gateway_serves_authenticated_canonical_json_and_drains()
-> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let foreign = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let held = BoundEndpoint::bind(Some(foreign.local_addr()?.port()))?;
    let port = held.port()?;
    let f = fixture(port)?;
    let mut roots = rustls::RootCertStore::empty();
    roots.add(rustls::pki_types::CertificateDer::from(f.ca.clone()))?;
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let shutdown = CancellationToken::new();
    let stop = shutdown.clone();
    let gateway = f.gateway.clone();
    let task = tokio::spawn(async move { gateway.run(held, stop).await });
    // Hold the shared ledger owner while four real TLS requests arrive. All
    // valid credentials must wait within bounded admission, never become 503.
    let store = f.gateway.0.credentials.clone();
    let (locked_tx, locked_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _guard = store.lock().unwrap();
        locked_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    locked_rx.await?;
    let mut requests = tokio::task::JoinSet::new();
    for _ in 0..4 {
        let connector = connector.clone();
        let token = f.token.clone();
        requests.spawn(async move {
            let socket = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
            let mut tls = connector.connect(
                rustls::pki_types::ServerName::try_from("127.0.0.1")?, socket,
            ).await?;
            let wire = format!(
                "POST /api/v1/tools/airk_search HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}"
            );
            tls.write_all(wire.as_bytes()).await?;
            let mut bytes = Vec::new();
            tokio::time::timeout(Duration::from_secs(5), tls.read_to_end(&mut bytes)).await??;
            let response = String::from_utf8(bytes)?;
            anyhow::ensure!(response.starts_with("HTTP/1.1 200"), "{response}");
            anyhow::ensure!(response.contains("\"retained\":true"));
            anyhow::ensure!(!response.contains(&token));
            Ok::<_, anyhow::Error>(())
        });
    }
    tokio::time::timeout(Duration::from_secs(3), async {
        while f.gateway.0.authentication.available_permits() != AUTH_TASKS - 4 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await?;
    release_tx.send(())?;
    holder.join().unwrap();
    while let Some(result) = requests.join_next().await {
        result??;
    }
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(5), task).await???;
    Ok(())
}

#[tokio::test]
async fn output_lifetime_cannot_extend_original_deadline_and_drop_cancels_only_that_request()
-> anyhow::Result<()> {
    let service = CancellationToken::new();
    let request = service.child_token();
    let context =
        ReadAttemptContext::new(Instant::now() - Duration::from_millis(1), request.clone());
    let mut body = OutputBody::new(
        Full::new(Bytes::from_static(b"delayed")).boxed(),
        Arc::new(Semaphore::new(32)),
        32,
        context,
    );
    assert!(body.frame().await.unwrap().is_err());
    assert!(request.is_cancelled());
    assert!(!service.is_cancelled());
    let request = service.child_token();
    let context = ReadAttemptContext::new(Instant::now() + Duration::from_secs(1), request.clone());
    drop(OutputBody::new(
        Full::new(Bytes::new()).boxed(),
        Arc::new(Semaphore::new(32)),
        32,
        context,
    ));
    assert!(request.is_cancelled());
    assert!(!service.is_cancelled());
    Ok(())
}

struct LifetimeProbe(Arc<AtomicUsize>);
impl Drop for LifetimeProbe {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn buffered_tls_request_deadline_and_cancellation_fence_actual_bytes_until_close()
-> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for cancel in [false, true] {
        for read_driven in [false, true] {
            let f = fixture(57421)?;
            let delivery = ConnectionFence::new(f.gateway.0.native_gate.clone());
            let (server_socket, client_socket) = tokio::io::duplex(65536);
            let bytes = Arc::new(AtomicUsize::new(0));
            let raw = FencedIo {
                inner: CountWrites {
                    inner: server_socket,
                    bytes: bytes.clone(),
                },
                delivery: delivery.clone(),
            };
            let mut roots = rustls::RootCertStore::empty();
            roots.add(rustls::pki_types::CertificateDer::from(f.ca.clone()))?;
            let config = rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()?
            .with_root_certificates(roots)
            .with_no_client_auth();
            let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
            let acceptor = f.gateway.0.tls.borrow().clone();
            let (server, client) = tokio::time::timeout(Duration::from_secs(5), async {
                tokio::join!(
                    acceptor.accept(raw),
                    connector.connect(
                        rustls::pki_types::ServerName::try_from("127.0.0.1").unwrap(),
                        client_socket
                    )
                )
            })
            .await?;
            let mut server = FlushTrackedIo {
                inner: server?,
                delivery: delivery.clone(),
            };
            let mut client = client?;
            let dropped = Arc::new(AtomicUsize::new(0));
            let canceled = CancellationToken::new();
            let context = ReadAttemptContext::new(
                Instant::now() + Duration::from_millis(40),
                canceled.clone(),
            )
            .with_lifetime_guard(Arc::new(LifetimeProbe(dropped.clone())));
            delivery.bind_request(context).await?;
            std::io::Write::write_all(
                &mut server.inner.get_mut().1.writer(),
                b"buffered private response",
            )?;
            delivery.body_complete();
            let before = bytes.load(Ordering::SeqCst);
            if cancel {
                canceled.cancel();
            } else {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            if read_driven {
                client.get_mut().0.write_all(&[23, 3, 3, 0, 1, 0]).await?;
                assert!(
                    tokio::time::timeout(Duration::from_secs(2), server.read(&mut [0u8; 32]))
                        .await?
                        .is_err()
                );
            } else {
                assert!(server.flush().await.is_err());
            }
            assert_eq!(
                bytes.load(Ordering::SeqCst),
                before,
                "expired/canceled ciphertext escaped raw request fence"
            );
            assert_eq!(
                dropped.load(Ordering::SeqCst),
                0,
                "blocked TLS ciphertext still owns admission"
            );
            drop(server);
            assert_eq!(
                dropped.load(Ordering::SeqCst),
                1,
                "actual connection close releases delivery ownership"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn real_tls_keeps_sequential_and_pipelined_requests_on_their_own_flush_boundary()
-> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let foreign = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let held = BoundEndpoint::bind(Some(foreign.local_addr()?.port()))?;
    let port = held.port()?;
    let f = fixture(port)?;
    let mut roots = rustls::RootCertStore::empty();
    roots.add(rustls::pki_types::CertificateDer::from(f.ca.clone()))?;
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let stop = CancellationToken::new();
    let gateway = f.gateway.clone();
    let shutdown = stop.clone();
    let owner = tokio::spawn(async move { gateway.run(held, shutdown).await });
    let socket = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let tls = connector
        .connect(
            rustls::pki_types::ServerName::try_from("127.0.0.1")?,
            socket,
        )
        .await?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(tls)).await?;
    let client = tokio::spawn(connection);
    for _ in 0..3 {
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/tools/airk_search")
            .header("host", format!("127.0.0.1:{port}"))
            .header("authorization", format!("Bearer {}", f.token))
            .header("content-type", "application/json")
            .body(Full::new(Bytes::from_static(b"{}")))?;
        let response =
            tokio::time::timeout(Duration::from_secs(3), sender.send_request(request)).await??;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = response.into_body().collect().await?.to_bytes();
        assert!(String::from_utf8_lossy(&bytes).contains("\"retained\":true"));
    }
    drop(sender);
    tokio::time::timeout(Duration::from_secs(3), client).await???;
    let socket = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let mut tls = connector
        .connect(
            rustls::pki_types::ServerName::try_from("127.0.0.1")?,
            socket,
        )
        .await?;
    let wire = |close: bool| {
        format!(
            "POST /api/v1/tools/airk_search HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: 2\r\n{}\r\n{{}}",
            f.token,
            if close { "Connection: close\r\n" } else { "" }
        )
    };
    tls.write_all(format!("{}{}", wire(false), wire(true)).as_bytes())
        .await?;
    let mut replies = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), tls.read_to_end(&mut replies)).await??;
    let replies = String::from_utf8(replies)?;
    assert_eq!(replies.matches("HTTP/1.1 200").count(), 2, "{replies}");
    assert_eq!(replies.matches("\"retained\":true").count(), 2, "{replies}");
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(5), owner).await???;
    Ok(())
}

#[tokio::test]
async fn real_tls_scope_refusal_preserves_fresh_403_and_fences_old_buffered_evidence()
-> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for read_driven in [false, true] {
        let foreign = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        let held = BoundEndpoint::bind(Some(foreign.local_addr()?.port()))?;
        let port = held.port()?;
        let mut f = fixture(port)?;
        let refuse = Arc::new(AtomicBool::new(false));
        let validation = refuse.clone();
        Arc::get_mut(&mut f.gateway.0).unwrap().validate =
            Arc::new(move |_credential, _context| {
                let validation = validation.clone();
                Box::pin(async move {
                    anyhow::ensure!(
                        !validation.load(Ordering::SeqCst),
                        "native scope no longer valid"
                    );
                    Ok(Arc::new(|| Ok(Arc::new(Echo) as Arc<dyn RemoteTools>))
                        as AuthenticatedBackendFactory)
                })
            });
        let mut roots = rustls::RootCertStore::empty();
        roots.add(rustls::pki_types::CertificateDer::from(f.ca.clone()))?;
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let delivery = ConnectionFence::new(f.gateway.0.native_gate.clone());
        let bytes = Arc::new(AtomicUsize::new(0));
        let (old_socket, old_peer) = tokio::io::duplex(65536);
        let raw = FencedIo {
            inner: CountWrites {
                inner: old_socket,
                bytes: bytes.clone(),
            },
            delivery: delivery.clone(),
        };
        let acceptor = f.gateway.0.tls.borrow().clone();
        let (old_server, old_client) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                acceptor.accept(raw),
                connector.connect(
                    rustls::pki_types::ServerName::try_from("127.0.0.1").unwrap(),
                    old_peer
                )
            )
        })
        .await
        .context("scope-refusal fixture: original buffered TLS handshake deadline")?;
        let mut old_server = FlushTrackedIo {
            inner: old_server?,
            delivery: delivery.clone(),
        };
        let mut old_client = old_client?;
        // Use the actual native gateway dispatch to bind the old validated read
        // context, then retain its real result in rustls before any socket flush.
        let response = f
            .gateway
            .handle_on_connection(
                request(&f, "/api/v1/tools/airk_search"),
                CancellationToken::new(),
                delivery.clone(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let old_body = response.into_body().collect().await?.to_bytes();
        assert!(String::from_utf8_lossy(&old_body).contains("\"retained\":true"));
        std::io::Write::write_all(&mut old_server.inner.get_mut().1.writer(), &old_body)?;
        // The copied bytes now belong to rustls; the production delivery wrapper
        // retains request admission until flush or transport close. Do not retain
        // a second application Bytes owner across factory shutdown.
        drop(old_body);
        let read_cell = Arc::downgrade(
            &f.gateway
                .0
                .clients
                .lock()
                .unwrap()
                .get(&f.id)
                .unwrap()
                .read_delivery,
        );
        let before = bytes.load(Ordering::SeqCst);
        delivery.check()?;
        assert!(
            matches!(delivery.poll(|| Poll::Ready(Ok(()))), Poll::Ready(Ok(()))),
            "old buffered output must be deliverable before native scope retirement"
        );

        // Native validation fails without rotating the credential or advancing
        // the catalog: the distinct read-context fence must do the retirement.
        refuse.store(true, Ordering::SeqCst);
        let stop = CancellationToken::new();
        let gateway = f.gateway.clone();
        let shutdown = stop.clone();
        let server = tokio::spawn(async move { gateway.run(held, shutdown).await });
        let socket = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
        let fresh = connector
            .connect(
                rustls::pki_types::ServerName::try_from("127.0.0.1")?,
                socket,
            )
            .await?;
        // Hyper decodes HTTP framing (including chunked bodies) over the real
        // TLS socket; raw bytes after the headers are not necessarily JSON.
        let (mut sender, connection) =
            hyper::client::conn::http1::handshake(TokioIo::new(fresh)).await?;
        let client = tokio::spawn(connection);
        let mut req = request(&f, "/api/v1/tools/airk_search");
        req.headers_mut().insert(
            http::header::CONNECTION,
            http::HeaderValue::from_static("close"),
        );
        let response = tokio::time::timeout(Duration::from_secs(5), sender.send_request(req))
            .await
            .context("scope-refusal fixture: fresh HTTPS response headers deadline")??;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            response.headers()[http::header::CONTENT_TYPE],
            "application/json"
        );
        let refusal_body = tokio::time::timeout(
            Duration::from_secs(5),
            http_body_util::Limited::new(response.into_body(), 8192).collect(),
        )
        .await
        .context("scope-refusal fixture: fresh HTTPS decoded body deadline")?
        .map_err(anyhow::Error::from_boxed)?
        .to_bytes();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&refusal_body)?,
            serde_json::json!({"schema":"airk.local-api-error/1","error":{"code":"native_grant_refused"}})
        );
        assert!(!String::from_utf8_lossy(&refusal_body).contains(&f.token));
        eprintln!(
            "scope-refusal fixture: decoded native HTTP403 body (read_driven={read_driven}): {}",
            String::from_utf8_lossy(&refusal_body)
        );
        drop(sender);
        tokio::time::timeout(Duration::from_secs(5), client)
            .await
            .context("scope-refusal fixture: fresh Hyper client shutdown deadline")???;
        assert!(f.gateway.0.clients.lock().unwrap().is_empty());
        assert_eq!(f.gateway.0.delivery_cells.lock().unwrap().cells.len(), 1);
        f.gateway
            .0
            .credentials
            .lock()
            .unwrap()
            .authenticate(&f.token, now_ms())?;
        assert!(delivery.check().is_err());
        if read_driven {
            old_client
                .get_mut()
                .0
                .write_all(&[23, 3, 3, 0, 1, 0])
                .await?;
            assert!(
                tokio::time::timeout(Duration::from_secs(2), old_server.read(&mut [0u8; 32]))
                    .await
                    .context("scope-refusal fixture: old buffered TLS read-driven fence deadline")?
                    .is_err()
            );
        } else {
            assert!(old_server.flush().await.is_err());
        }
        assert_eq!(
            bytes.load(Ordering::SeqCst),
            before,
            "retired read context emitted buffered TLS evidence"
        );
        assert!(
            delivery
                .bind_read_context(ReadDeliveryCell::default())
                .is_err(),
            "old TCP cannot replace its retired read context"
        );
        drop(old_server);
        drop(old_client);
        drop(delivery);
        assert!(
            read_cell.upgrade().is_none(),
            "closed sockets must release their retired context cell"
        );
        eprintln!(
            "scope-refusal fixture: old buffered TLS output fenced (read_driven={read_driven},bytes={before})"
        );
        stop.cancel();
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .context("scope-refusal fixture: gateway owned shutdown deadline")???;
    }
    Ok(())
}

#[tokio::test]
async fn overlapping_request_wait_is_bounded_without_releasing_unflushed_admission()
-> anyhow::Result<()> {
    let f = fixture(57421)?;
    let delivery = ConnectionFence::new(f.gateway.0.native_gate.clone());
    let dropped = Arc::new(AtomicUsize::new(0));
    delivery
        .bind_request(
            ReadAttemptContext::new(
                Instant::now() + Duration::from_secs(1),
                CancellationToken::new(),
            )
            .with_lifetime_guard(Arc::new(LifetimeProbe(dropped.clone()))),
        )
        .await?;
    delivery.body_complete();
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    let next = ReadAttemptContext::new(
        Instant::now() + Duration::from_millis(10),
        CancellationToken::new(),
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(1), delivery.bind_request(next))
            .await?
            .is_err()
    );
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    assert!(matches!(
        delivery.poll(|| Poll::Ready(Ok(()))),
        Poll::Ready(Err(_))
    ));
    delivery.close_requests();
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    Ok(())
}
#[tokio::test]
async fn post_checkpoint_guard_survives_handler_body_and_network_clones() -> anyhow::Result<()> {
    let dropped = Arc::new(AtomicUsize::new(0));
    let mut f = fixture(57421)?;
    let observed = dropped.clone();
    Arc::get_mut(&mut f.gateway.0).unwrap().admit =
        Arc::new(move || Ok(Arc::new(LifetimeProbe(observed.clone()))));
    let response = f
        .gateway
        .handle(
            request(&f, "/api/v1/tools/airk_search"),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body();
    let bytes = body.frame().await.unwrap()?.into_data().unwrap();
    let cloned = bytes.clone();
    drop(body);
    drop(bytes);
    assert_eq!(
        dropped.load(Ordering::SeqCst),
        0,
        "actual network clone retains native checkpoint obligation"
    );
    drop(cloned);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    let mut catalog = request(&f, "/api/v1/tools");
    *catalog.method_mut() = http::Method::GET;
    f.gateway
        .handle(catalog, CancellationToken::new())
        .await
        .into_body()
        .collect()
        .await?;
    assert_eq!(
        dropped.load(Ordering::SeqCst),
        1,
        "idle metadata GET owns no checkpoint read guard"
    );
    f.gateway.close().await;
    Ok(())
}
fn close_native(gate: &NativeCatalogAdmission, namespace: Incarnation) -> anyhow::Result<()> {
    use airk_core::{
        catalog_management::{CatalogOperation, CatalogRequest},
        service::RequestKey,
    };
    gate.close_request_admission(&CatalogRequest {
        caller: Incarnation::from_bytes([12; 16]),
        key: RequestKey {
            namespace,
            epoch: 1,
            nonce: Incarnation::from_bytes([13; 16]),
        },
        expected_catalog_revision: CatalogRevision::INITIAL,
        operation: CatalogOperation::RevokeGrant {
            id: GrantId::from_identity(Incarnation::from_bytes([5; 16])),
            expected_revision: CatalogRevision::INITIAL,
        },
    })?;
    Ok(())
}
struct PendingSocket<S> {
    inner: S,
    pending: Arc<tokio::sync::Notify>,
}
impl<S: AsyncRead + Unpin> AsyncRead for PendingSocket<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}
impl<S: AsyncWrite + Unpin> AsyncWrite for PendingSocket<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write(cx, bytes);
        if result.is_pending() {
            self.pending.notify_one();
        }
        result
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let result = Pin::new(&mut self.inner).poll_flush(cx);
        if result.is_pending() {
            self.pending.notify_one();
        }
        result
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
#[tokio::test]
async fn catalog_close_stops_actual_partial_socket_writes_and_buffered_flush() -> anyhow::Result<()>
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for flush in [false, true] {
        let f = fixture(57421)?;
        let credential = f
            .gateway
            .0
            .credentials
            .lock()
            .unwrap()
            .authenticate(&f.token, now_ms())?;
        let delivery = ConnectionFence::new(f.gateway.0.native_gate.clone());
        delivery.capture(&credential.selection)?;
        let (socket, mut peer) = tokio::io::duplex(4);
        let pending = Arc::new(tokio::sync::Notify::new());
        let writer = PendingSocket {
            inner: socket,
            pending: pending.clone(),
        };
        let fenced = delivery.clone();
        let task = tokio::spawn(async move {
            if flush {
                let buffered = tokio::io::BufWriter::with_capacity(32, writer);
                let mut io = FencedIo {
                    inner: buffered,
                    delivery: fenced,
                };
                io.write_all(b"abcdefgh").await?;
                io.flush().await
            } else {
                let mut io = FencedIo {
                    inner: writer,
                    delivery: fenced,
                };
                io.write_all(b"abcdefgh").await
            }
        });
        tokio::time::timeout(Duration::from_secs(2), pending.notified()).await?;
        close_native(&f.gateway.0.native_gate, credential.selection.namespace())?;
        let mut bytes = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), peer.read_to_end(&mut bytes)).await??;
        assert_eq!(
            bytes, b"abcd",
            "only bytes written before the native close may reach the socket"
        );
        assert!(task.await?.is_err());
        assert!(
            delivery.capture(&credential.selection).is_err(),
            "keepalive cannot replace its old invalid issue ticket"
        );
        f.gateway.close().await;
    }
    Ok(())
}

struct CountWrites {
    inner: tokio::io::DuplexStream,
    bytes: Arc<AtomicUsize>,
}
impl AsyncRead for CountWrites {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}
impl AsyncWrite for CountWrites {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let result = Pin::new(&mut self.inner).poll_write(cx, buf);
        if let Poll::Ready(Ok(count)) = result {
            self.bytes.fetch_add(count, Ordering::SeqCst);
        }
        result
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

#[tokio::test]
async fn tls_read_driven_alert_cannot_flush_ciphertext_after_native_fence() -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for (below_tls, revoke_credential) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let f = fixture(57421)?;
        let delivery = ConnectionFence::new(f.gateway.0.native_gate.clone());
        let (server_socket, client_socket) = tokio::io::duplex(65536);
        let bytes = Arc::new(AtomicUsize::new(0));
        let raw = FencedIo {
            inner: CountWrites {
                inner: server_socket,
                bytes: bytes.clone(),
            },
            delivery: if below_tls {
                delivery.clone()
            } else {
                ConnectionFence::new(f.gateway.0.native_gate.clone())
            },
        };
        let mut roots = rustls::RootCertStore::empty();
        roots.add(rustls::pki_types::CertificateDer::from(f.ca.clone()))?;
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
        let acceptor = f.gateway.0.tls.borrow().clone();
        let (server, client) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(
                acceptor.accept(raw),
                connector.connect(
                    rustls::pki_types::ServerName::try_from("127.0.0.1").unwrap(),
                    client_socket
                )
            )
        })
        .await?;
        let mut server = server?;
        let mut client = client?;
        let credential = f
            .gateway
            .0
            .credentials
            .lock()
            .unwrap()
            .authenticate(&f.token, now_ms())?;
        delivery.capture(&credential.selection)?;
        if revoke_credential {
            let cell = f.gateway.0.delivery_cells.lock().unwrap().capture(
                0,
                &credential,
                delivery.authenticated_ticket()?,
            )?;
            delivery.bind_credential(cell)?;
        }
        // Queue a real rustls application record, without flushing the socket.
        std::io::Write::write_all(&mut server.get_mut().1.writer(), b"retained evidence")?;
        let before = bytes.load(Ordering::SeqCst);
        if revoke_credential {
            f.gateway.0.credentials.lock().unwrap().revoke(f.id)?;
            f.gateway.revoke(f.id).await;
            let response = f
                .gateway
                .handle(
                    request(&f, "/api/v1/tools/airk_search"),
                    CancellationToken::new(),
                )
                .await;
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert!(
                f.gateway
                    .0
                    .delivery_cells
                    .lock()
                    .unwrap()
                    .capture(0, &credential, delivery.authenticated_ticket()?)
                    .is_err(),
                "stale authentication cannot recreate a delivery cell after revoke acknowledgement"
            );
        } else {
            close_native(&f.gateway.0.native_gate, credential.selection.namespace())?;
        }
        // Invalid encrypted record triggers rustls read_io's last-gasp write.
        client.get_mut().0.write_all(&[23, 3, 3, 0, 1, 0]).await?;
        let mut reader: Box<dyn AsyncRead + Unpin> = if below_tls {
            Box::new(server)
        } else {
            Box::new(FencedIo {
                inner: server,
                delivery,
            })
        };
        let mut output = [0u8; 32];
        assert!(
            tokio::time::timeout(Duration::from_secs(2), reader.read(&mut output))
                .await?
                .is_err()
        );
        if below_tls {
            assert_eq!(
                bytes.load(Ordering::SeqCst),
                before,
                "fenced raw socket must receive no post-fence TLS bytes"
            );
        } else {
            assert!(
                bytes.load(Ordering::SeqCst) > before,
                "control demonstrates a TLS read can bypass an outer write-only fence"
            );
        }
        f.gateway.close().await;
    }
    Ok(())
}

#[tokio::test]
async fn credential_cells_reject_connection_switch_expiry_and_retired_generation()
-> anyhow::Result<()> {
    let f = fixture(57421)?;
    let credential = f
        .gateway
        .0
        .credentials
        .lock()
        .unwrap()
        .authenticate(&f.token, now_ms())?;
    let delivery = ConnectionFence::new(f.gateway.0.native_gate.clone());
    delivery.capture(&credential.selection)?;
    let ticket = delivery.authenticated_ticket()?;
    let mut registry = CredentialDeliveryRegistry::default();
    let first = registry.capture(0, &credential, ticket)?;
    delivery.bind_credential(first.clone())?;
    let mut other = credential.clone();
    other.connection_id = Incarnation::from_bytes([99; 16]);
    assert!(
        delivery
            .bind_credential(registry.capture(0, &other, ticket)?)
            .is_err()
    );
    let mut changed_expiry = credential.clone();
    changed_expiry.expires_unix_ms += 1;
    assert!(registry.capture(0, &changed_expiry, ticket).is_err());
    registry.retire(credential.connection_id);
    assert!(delivery.check().is_err());
    let fresh = registry.capture(0, &credential, ticket)?;
    assert!(!Arc::ptr_eq(&fresh, &first));
    assert!(delivery.bind_credential(fresh).is_err());
    // Expiry is checked at delivery, even without the periodic sweeper.
    let expired = CredentialDelivery {
        id: credential.connection_id,
        generation: credential.generation,
        expires: now_ms(),
        ticket,
        revoked: false,
    };
    assert!(expired.check().is_err());
    registry.revoke(credential.connection_id);
    assert!(registry.capture(0, &credential, ticket).is_err());
    f.gateway.close().await;
    Ok(())
}

#[tokio::test]
async fn credential_retirement_reclaims_registry_slots_but_fences_old_authentication()
-> anyhow::Result<()> {
    let f = fixture(57421)?;
    let mut credential = f
        .gateway
        .0
        .credentials
        .lock()
        .unwrap()
        .authenticate(&f.token, now_ms())?;
    let delivery = ConnectionFence::new(f.gateway.0.native_gate.clone());
    delivery.capture(&credential.selection)?;
    let ticket = delivery.authenticated_ticket()?;
    let mut registry = CredentialDeliveryRegistry::default();
    for number in 1..=160u128 {
        credential.connection_id = Incarnation::from_bytes(number.to_be_bytes());
        let epoch = registry.epoch;
        let old = registry.capture(epoch, &credential, ticket)?;
        registry.revoke(credential.connection_id);
        assert!(old.lock().unwrap().check().is_err());
        assert!(registry.cells.is_empty());
        assert!(registry.capture(epoch, &credential, ticket).is_err());
    }
    // A fresh authentication from the durable ledger uses the current epoch;
    // historical revocations consume no active map slots.
    credential.connection_id = f.id;
    let retained = registry.capture(registry.epoch, &credential, ticket)?;
    registry.epoch = u64::MAX;
    // Exhaustion fences every retained cell, including an unrelated ID.
    registry.revoke(Incarnation::from_bytes([211; 16]));
    assert!(retained.lock().unwrap().check().is_err());
    assert!(registry.capture(u64::MAX, &credential, ticket).is_err());
    f.gateway.close().await;
    Ok(())
}

#[tokio::test]
async fn async_scope_validation_cannot_cross_the_captured_delivery_fence() -> anyhow::Result<()> {
    let mut f = fixture(57421)?;
    let gate = f.gateway.0.native_gate.clone();
    let observed = gate.clone();
    Arc::get_mut(&mut f.gateway.0).unwrap().validate = Arc::new(move |credential, _| {
        let gate = observed.clone();
        Box::pin(async move {
            close_native(&gate, credential.selection.namespace())?;
            Ok(Arc::new(|| Ok(Arc::new(Echo) as Arc<dyn RemoteTools>))
                as AuthenticatedBackendFactory)
        })
    });
    let response = f
        .gateway
        .handle(
            request(&f, "/api/v1/tools/airk_search"),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await?.to_bytes())?;
    assert_eq!(body["error"]["code"], "native_delivery_fenced");
    assert!(f.gateway.0.clients.lock().unwrap().is_empty());
    f.gateway.close().await;
    Ok(())
}

#[tokio::test]
async fn completed_revocation_cannot_be_undone_by_delayed_validated_cache_insertion()
-> anyhow::Result<()> {
    for already_cached in [false, true] {
        let f = fixture(57421)?;
        if already_cached {
            let initial = f
                .gateway
                .handle(
                    request(&f, "/api/v1/tools/airk_search"),
                    CancellationToken::new(),
                )
                .await;
            assert_eq!(initial.status(), StatusCode::OK);
            initial.into_body().collect().await?;
            assert_eq!(f.gateway.0.clients.lock().unwrap().len(), 1);
        }
        let credential = f
            .gateway
            .0
            .credentials
            .lock()
            .unwrap()
            .authenticate(&f.token, now_ms())?;
        let delivery = ConnectionFence::new(f.gateway.0.native_gate.clone());
        delivery.capture(&credential.selection)?;
        let ticket = delivery.authenticated_ticket()?;
        let cell = {
            let mut registry = f.gateway.0.delivery_cells.lock().unwrap();
            let epoch = registry.epoch;
            registry.capture(epoch, &credential, ticket)?
        };
        delivery.bind_credential(cell.clone())?;
        let backend = (f.gateway.0.validate)(
            credential.clone(),
            ReadAttemptContext::new(
                Instant::now() + Duration::from_secs(5),
                CancellationToken::new(),
            ),
        )
        .await?;
        // This is the same pre-cache check passed by dispatch. Delay only cache
        // publication until actual durable revocation and removal complete.
        delivery.check()?;
        f.gateway.0.credentials.lock().unwrap().revoke(f.id)?;
        f.gateway.revoke(f.id).await;
        assert!(f.gateway.0.clients.lock().unwrap().is_empty());
        let refused = match f
            .gateway
            .cache_validated_service(credential, backend, &delivery, cell, ticket)
        {
            Ok(_) => anyhow::bail!("revoked validated request recreated a cached context"),
            Err(refused) => refused,
        };
        assert_eq!(refused.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body: serde_json::Value =
            serde_json::from_slice(&refused.into_body().collect().await?.to_bytes())?;
        assert_eq!(body["error"]["code"], "native_delivery_fenced");
        assert!(f.gateway.0.clients.lock().unwrap().is_empty());
        f.gateway.close().await;
    }
    Ok(())
}

#[tokio::test]
async fn fresh_tcp_can_capture_unrelated_catalog_advance_but_existing_tcp_cannot_replace_its_old_ticket()
-> anyhow::Result<()> {
    let f = fixture(57421)?;
    let credential = f
        .gateway
        .0
        .credentials
        .lock()
        .unwrap()
        .authenticate(&f.token, now_ms())?;
    let old = ConnectionFence::new(f.gateway.0.native_gate.clone());
    old.capture(&credential.selection)?;
    let first = f
        .gateway
        .handle(
            request(&f, "/api/v1/tools/airk_search"),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(first.status(), StatusCode::OK);
    first.into_body().collect().await?;
    let old_cached = f
        .gateway
        .0
        .clients
        .lock()
        .unwrap()
        .get(&credential.connection_id)
        .unwrap()
        .ticket;
    let next = CatalogRevision::INITIAL.next()?;
    f.gateway
        .0
        .native_gate
        .observe_catalog_commit(CatalogRevision::INITIAL, next)?;
    assert!(old.check().is_err());
    assert!(old.capture(&credential.selection).is_err());
    let fresh = ConnectionFence::new(f.gateway.0.native_gate.clone());
    fresh.capture(&credential.selection)?;
    fresh.check()?;
    let second = f
        .gateway
        .handle(
            request(&f, "/api/v1/tools/airk_search"),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(second.status(), StatusCode::OK);
    second.into_body().collect().await?;
    let new_cached = f
        .gateway
        .0
        .clients
        .lock()
        .unwrap()
        .get(&credential.connection_id)
        .unwrap()
        .ticket;
    assert_ne!(
        old_cached, new_cached,
        "new catalog generation cannot reuse an old native session factory"
    );
    let mut ahead = credential.selection.clone();
    if let HttpSelection::Scoped { selection } = &mut ahead {
        selection.expected_catalog_revision = next.next()?;
    }
    assert!(
        ConnectionFence::new(f.gateway.0.native_gate.clone())
            .capture(&ahead)
            .is_err(),
        "future saved minimum cannot grant current delivery"
    );
    f.gateway.close().await;
    Ok(())
}

struct CatalogClockEcho {
    revision: CatalogRevision,
    calls: Arc<AtomicUsize>,
}
impl RemoteTools for CatalogClockEcho {
    fn catalog(&self) -> Vec<airk_mcp::remote::Tool> {
        vec![
            serde_json::from_value(serde_json::json!({
                "name":"search","description":"Original saved-clock regression fixture",
                "inputSchema":{"type":"object","additionalProperties":true},
                "annotations":{"readOnlyHint":true}
            }))
            .unwrap(),
        ]
    }
    fn local_help(&self) -> bool {
        false
    }
    fn call(
        &self,
        name: String,
        args: serde_json::Map<String, serde_json::Value>,
    ) -> RemoteFuture<'_> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            Ok(
                serde_json::json!({"content":[],"isError":false,"structuredContent":{
                    "tool":name,"args":args,"validated_catalog_revision":self.revision,
                    "coverage":{"complete":false,"future_native_field":[1,2,3]},
                    "continuation":"opaque:original-clock-fixture"
                }}),
            )
        })
    }
}

type ClockTestSender = hyper::client::conn::http1::SendRequest<Full<Bytes>>;

/// A real CA-validated HTTPS exchange against the supplied native status owner.
/// Callers hold their inference lane separately; this helper owns only ingress.
/// Its only caller (`inference_owner_tests`) exists with `local-inference`.
#[cfg(feature = "local-inference")]
pub(crate) async fn live_https_status(
    backend: Arc<dyn RemoteTools>,
) -> anyhow::Result<serde_json::Value> {
    let endpoint = BoundEndpoint::bind(None)?;
    let port = endpoint.port()?;
    let mut f = fixture(port)?;
    Arc::get_mut(&mut f.gateway.0).unwrap().validate = Arc::new(move |_, _| {
        let backend = backend.clone();
        Box::pin(
            async move { Ok(Arc::new(move || Ok(backend.clone())) as AuthenticatedBackendFactory) },
        )
    });
    let stop = CancellationToken::new();
    let shutdown = stop.clone();
    let gateway = f.gateway.clone();
    let server = tokio::spawn(async move { gateway.run(endpoint, shutdown).await });
    let exchange = async {
        let (mut sender, connection) = clock_test_tcp(&f, port).await?;
        let response = clock_test_exchange(
            &f,
            &mut sender,
            "/api/v1/tools/status",
            serde_json::json!({}),
            None,
        )
        .await;
        drop(sender);
        tokio::time::timeout(Duration::from_secs(5), connection).await???;
        let (status, _, body) = response?;
        assert_eq!(status, StatusCode::OK, "native status over HTTPS: {body}");
        Ok::<_, anyhow::Error>(body)
    }
    .await;
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(5), server).await???;
    exchange
}

async fn clock_test_tcp(
    f: &Fixture,
    port: u16,
) -> anyhow::Result<(
    ClockTestSender,
    tokio::task::JoinHandle<Result<(), hyper::Error>>,
)> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(rustls::pki_types::CertificateDer::from(f.ca.clone()))?;
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let socket = tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let tls = connector
        .connect(
            rustls::pki_types::ServerName::try_from("127.0.0.1")?,
            socket,
        )
        .await?;
    let (sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(tls)).await?;
    Ok((sender, tokio::spawn(connection)))
}
async fn clock_test_exchange(
    f: &Fixture,
    sender: &mut ClockTestSender,
    path: &str,
    body: serde_json::Value,
    session: Option<&http::HeaderValue>,
) -> anyhow::Result<(StatusCode, Option<http::HeaderValue>, serde_json::Value)> {
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("host", &f.gateway.0.authority)
        .header("authorization", format!("Bearer {}", f.token))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-11-25")
        .header("x-airk-response-mode", "full");
    if let Some(session) = session {
        request = request.header("mcp-session-id", session);
    }
    let response = tokio::time::timeout(
        Duration::from_secs(5),
        sender.send_request(request.body(Full::new(Bytes::from(serde_json::to_vec(&body)?)))?),
    )
    .await??;
    let status = response.status();
    let session = response.headers().get("mcp-session-id").cloned();
    let bytes = tokio::time::timeout(Duration::from_secs(5), response.into_body().collect())
        .await??
        .to_bytes();
    let value = if bytes.is_empty() {
        serde_json::Value::Null
    } else if let Ok(value) = serde_json::from_slice(&bytes) {
        value
    } else {
        let text = std::str::from_utf8(&bytes)?;
        match text.lines().find_map(|line| line.strip_prefix("data:")) {
            Some(data) => serde_json::from_str(data.trim_start())?,
            None => serde_json::Value::String(text.to_owned()),
        }
    };
    Ok((status, session, value))
}

#[tokio::test]
async fn saved_catalog_clock_refresh_preserves_real_mcp_session_on_same_and_fresh_tcp()
-> anyhow::Result<()> {
    let held = BoundEndpoint::bind(None)?;
    let port = held.port()?;
    let mut f = fixture(port)?;
    let current = CatalogRevision::INITIAL.next()?;
    f.gateway
        .0
        .native_gate
        .observe_catalog_commit(CatalogRevision::INITIAL, current)?;
    let gate = f.gateway.0.native_gate.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed_calls = calls.clone();
    Arc::get_mut(&mut f.gateway.0).unwrap().validate = Arc::new(move |_, _| {
        // Like the native backend factory, capture the actual validated clock,
        // rather than the saved credential's older minimum revision.
        let revision = gate.catalog_clock().unwrap().revision;
        let calls = observed_calls.clone();
        Box::pin(async move {
            Ok(Arc::new(move || {
                Ok(Arc::new(CatalogClockEcho {
                    revision,
                    calls: calls.clone(),
                }) as Arc<dyn RemoteTools>)
            }) as AuthenticatedBackendFactory)
        })
    });
    let original = f
        .gateway
        .0
        .credentials
        .lock()
        .unwrap()
        .authenticate(&f.token, now_ms())?;
    let stop = CancellationToken::new();
    let gateway = f.gateway.clone();
    let shutdown = stop.clone();
    let owner = tokio::spawn(async move { gateway.run(held, shutdown).await });
    let (mut same_tcp, first_task) = clock_test_tcp(&f, port).await?;
    let (status, session, initialized) = clock_test_exchange(
        &f,
        &mut same_tcp,
        "/mcp",
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"original-clock-fixture","version":"1"}}}),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    let session = session.context("MCP initialization must create its original session")?;
    let (status, _, _) = clock_test_exchange(
        &f,
        &mut same_tcp,
        "/mcp",
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        Some(&session),
    )
    .await?;
    assert!(status.is_success());
    let (status, _, listed) = clock_test_exchange(
        &f,
        &mut same_tcp,
        "/mcp",
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        Some(&session),
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["result"]["tools"][0]["name"], "search");
    let args = serde_json::json!({"query":"original","response_mode":"full","future_input":{"retained":true}});
    let call = |id| serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"search","arguments":args}});
    let (status, _, before) =
        clock_test_exchange(&f, &mut same_tcp, "/mcp", call(3), Some(&session)).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        before["result"]["structuredContent"]["validated_catalog_revision"],
        serde_json::json!(current)
    );
    let (old_ticket, old_context) = {
        let clients = f.gateway.0.clients.lock().unwrap();
        let cached = clients.get(&f.id).unwrap();
        (cached.ticket, cached.read_delivery.clone())
    };
    let mut advanced = original.selection.clone();
    if let HttpSelection::Scoped { selection } = &mut advanced {
        selection.expected_catalog_revision = current;
    }
    f.gateway
        .0
        .credentials
        .lock()
        .unwrap()
        .enroll_default(f.id, advanced.clone(), now_ms())?;
    let authenticated = f
        .gateway
        .0
        .credentials
        .lock()
        .unwrap()
        .authenticate(&f.token, now_ms())?;
    assert_eq!(authenticated.connection_id, original.connection_id);
    assert_eq!(authenticated.generation, original.generation);
    assert_eq!(authenticated.expires_unix_ms, original.expires_unix_ms);
    assert_eq!(authenticated.selection, advanced);
    assert_eq!(
        read_client_token(&f._root.path().join("http"), f.id)?,
        f.token
    );
    // Both requests use the original initialized session. No reinitialization,
    // re-enrollment or replay may hide a cache-induced session replacement.
    let (status, _, same_reply) =
        clock_test_exchange(&f, &mut same_tcp, "/mcp", call(4), Some(&session)).await?;
    let same_status = status;
    let (mut fresh_tcp, second_task) = clock_test_tcp(&f, port).await?;
    let (status, _, fresh_reply) =
        clock_test_exchange(&f, &mut fresh_tcp, "/mcp", call(5), Some(&session)).await?;
    assert_eq!(
        (same_status, status),
        (StatusCode::OK, StatusCode::OK),
        "independent one-dispatch calls; same TCP: {same_reply}; fresh TCP: {fresh_reply}"
    );
    assert_eq!(same_reply["result"], before["result"]);
    assert_eq!(fresh_reply["result"], before["result"]);
    let (status, _, json_reply) = clock_test_exchange(
        &f,
        &mut fresh_tcp,
        "/api/v1/tools/search",
        args.clone(),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        json_reply, before["result"],
        "canonical JSON/MCP complete result parity"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        4,
        "no accepted read was replayed"
    );
    {
        let clients = f.gateway.0.clients.lock().unwrap();
        let cached = clients.get(&f.id).unwrap();
        assert_eq!(cached.ticket, old_ticket);
        assert!(Arc::ptr_eq(&cached.read_delivery, &old_context));
        assert_eq!(cached.selection, advanced);
    }
    // Actual native clock turnover still retires the old delivery generation.
    // A fresh JSON request creates a new current factory; old SDK sessions stay
    // fenced and do not transparently replay in that new generation.
    let next = current.next()?;
    f.gateway
        .0
        .native_gate
        .observe_catalog_commit(current, next)?;
    assert!(f.gateway.0.native_gate.check_issue(old_ticket).is_err());
    let (mut next_tcp, third_task) = clock_test_tcp(&f, port).await?;
    let (status, _, next_json) = clock_test_exchange(
        &f,
        &mut next_tcp,
        "/api/v1/tools/search",
        args.clone(),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        next_json["structuredContent"]["validated_catalog_revision"],
        serde_json::json!(next)
    );
    assert!(old_context.lock().unwrap().closed);
    let (status, _, unknown_session) =
        clock_test_exchange(&f, &mut next_tcp, "/mcp", call(6), Some(&session)).await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(unknown_session, "Not Found: Session not found");
    assert_eq!(calls.load(Ordering::SeqCst), 5);
    drop((same_tcp, fresh_tcp, next_tcp));
    for task in [first_task, second_task, third_task] {
        let _ = tokio::time::timeout(Duration::from_secs(5), task).await?;
    }
    stop.cancel();
    tokio::time::timeout(Duration::from_secs(5), owner).await???;
    Ok(())
}

#[tokio::test]
async fn delayed_old_authenticated_clock_cannot_replace_newer_cached_authority()
-> anyhow::Result<()> {
    let f = fixture(57421)?;
    let current = CatalogRevision::INITIAL.next()?;
    f.gateway
        .0
        .native_gate
        .observe_catalog_commit(CatalogRevision::INITIAL, current)?;
    // Hold the exact snapshot and delivery cell after authentication/validation,
    // but before publication into the cache, like a delayed async validator.
    let stale = f
        .gateway
        .0
        .credentials
        .lock()
        .unwrap()
        .authenticate(&f.token, now_ms())?;
    let delivery = ConnectionFence::new(f.gateway.0.native_gate.clone());
    delivery.capture(&stale.selection)?;
    let ticket = delivery.authenticated_ticket()?;
    let cell = {
        let mut registry = f.gateway.0.delivery_cells.lock().unwrap();
        let epoch = registry.epoch;
        registry.capture(epoch, &stale, ticket)?
    };
    delivery.bind_credential(cell.clone())?;
    let backend = (f.gateway.0.validate)(
        stale.clone(),
        ReadAttemptContext::new(
            Instant::now() + Duration::from_secs(5),
            CancellationToken::new(),
        ),
    )
    .await?;
    delivery.check()?;
    let mut advanced = stale.selection.clone();
    if let HttpSelection::Scoped { selection } = &mut advanced {
        selection.expected_catalog_revision = current;
    }
    {
        let mut store = f.gateway.0.credentials.lock().unwrap();
        store.enroll_default(f.id, advanced.clone(), now_ms())?;
        assert!(
            store
                .enroll_default(f.id, stale.selection.clone(), now_ms())
                .is_err(),
            "saved clock cannot regress"
        );
        let mut crossed = advanced.clone();
        if let HttpSelection::Scoped { selection } = &mut crossed {
            selection.grant_id = GrantId::from_identity(Incarnation::from_bytes([91; 16]));
        }
        assert!(
            store.enroll_default(f.id, crossed, now_ms()).is_err(),
            "a clock refresh cannot replace the original grant"
        );
    }
    let response = f
        .gateway
        .handle(
            request(&f, "/api/v1/tools/airk_search"),
            CancellationToken::new(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    response.into_body().collect().await?;
    let retained = f
        .gateway
        .0
        .clients
        .lock()
        .unwrap()
        .get(&f.id)
        .unwrap()
        .read_delivery
        .clone();
    let refused = match f
        .gateway
        .cache_validated_service(stale, backend, &delivery, cell, ticket)
    {
        Ok(_) => anyhow::bail!("delayed old authentication replaced the newer cached selection"),
        Err(response) => response,
    };
    assert_eq!(refused.status(), StatusCode::SERVICE_UNAVAILABLE);
    let refused_body: serde_json::Value =
        serde_json::from_slice(&refused.into_body().collect().await?.to_bytes())?;
    assert_eq!(refused_body["error"]["code"], "native_read_context_changed");
    {
        let clients = f.gateway.0.clients.lock().unwrap();
        let cached = clients.get(&f.id).unwrap();
        assert_eq!(cached.selection, advanced);
        assert_eq!(cached.ticket, ticket);
        assert!(Arc::ptr_eq(&cached.read_delivery, &retained));
        assert!(!retained.lock().unwrap().closed);
    }
    f.gateway.close().await;
    Ok(())
}
