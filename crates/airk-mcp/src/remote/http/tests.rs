// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::remote::RemoteFuture;
use rmcp::model::{CallToolResult, Tool};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};
type ObservedCalls = Arc<Mutex<Vec<(String, serde_json::Map<String, Value>)>>>;

struct Echo {
    calls: ObservedCalls,
    closed: Arc<AtomicUsize>,
    clients: Arc<Mutex<Vec<airk_core::usage::ClientFamily>>>,
}
impl RemoteTools for Echo {
    fn catalog(&self) -> Vec<Tool> {
        ["search","status","draft_plan","erase_everything"].into_iter().map(|name|serde_json::from_value(json!({"name":name,"description":"Original HTTP fixture","inputSchema":{"type":"object","properties":{"revision":{"type":"string"}},"additionalProperties":true},"outputSchema":{"type":"object","additionalProperties":true},"annotations":{"readOnlyHint":true}})).unwrap()).collect()
    }
    fn client(&self, client: airk_core::usage::ClientFamily) {
        self.clients.lock().unwrap().push(client);
    }
    fn close(&self) {
        self.closed.fetch_add(1, Ordering::SeqCst);
    }
    fn local_help(&self) -> bool {
        false
    }
    fn call(&self, name: String, args: serde_json::Map<String, Value>) -> RemoteFuture<'_> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push((name.clone(), args.clone()));
            if args.get("fixture_error") == Some(&Value::Bool(true)) {
                return Ok(serde_json::to_value(CallToolResult::structured_error(
                    json!({"status":"retry","code":"selection_busy","retryable":true,"action":"repeat_same_request","retry_after_ms":25,"future_receipt":{"retained":true}}),
                ))?);
            }
            let body = json!({"tool":name,"arguments":args,"publication":{"token":"fixture-exact-publication"},"coverage":{"complete":false,"future_native_field":[1,2,3]},"continuation":"opaque:fixture","future_native_data":{"preserved":true}});
            Ok(serde_json::to_value(CallToolResult::structured(body))?)
        })
    }
}
struct Fixture {
    service: HttpReadService,
    calls: ObservedCalls,
    closed: Arc<AtomicUsize>,
    clients: Arc<Mutex<Vec<airk_core::usage::ClientFamily>>>,
}
fn fixture(factory: ReadSessionFactory, budget: HttpSessionBudget) -> anyhow::Result<Fixture> {
    let calls = Arc::new(Mutex::new(vec![]));
    let closed = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let closures = closed.clone();
    let clients = Arc::new(Mutex::new(vec![]));
    let attribution = clients.clone();
    let backend: AuthenticatedBackendFactory = Arc::new(move || {
        Ok(Arc::new(Echo {
            calls: observed.clone(),
            closed: closures.clone(),
            clients: attribution.clone(),
        }))
    });
    let service = HttpReadService::new(
        factory,
        budget,
        backend,
        ReadSessionPreferences {
            response_mode: crate::response::ResponseMode::Terse,
            client: airk_core::usage::ClientFamily::Unknown,
        },
        "127.0.0.1:57421".into(),
        vec![],
        HttpReadLimits::default(),
    )?;
    Ok(Fixture {
        service,
        calls,
        closed,
        clients,
    })
}
fn request(method: Method, path: &str, body: Value) -> Request<Full<Bytes>> {
    Request::builder()
        .method(method)
        .uri(path)
        .header("host", "127.0.0.1:57421")
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(Full::new(Bytes::from(body.to_string())))
        .unwrap()
}
async fn value(response: HttpResponse) -> anyhow::Result<Value> {
    let bytes = tokio::time::timeout(Duration::from_secs(5), response.into_body().collect())
        .await??
        .to_bytes();
    if let Ok(value) = serde_json::from_slice(&bytes) {
        return Ok(value);
    }
    let text = std::str::from_utf8(&bytes)?;
    let data = text
        .lines()
        .filter_map(|line| {
            line.strip_prefix("data: ")
                .or_else(|| line.strip_prefix("data:"))
        })
        .find(|data| !data.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("expected MCP JSON or SSE data"))?;
    Ok(serde_json::from_str(data)?)
}

#[tokio::test]
async fn json_api_retains_canonical_catalog_arguments_results_and_refuses_annotation_only_authority()
-> anyhow::Result<()> {
    let f = fixture(ReadSessionFactory::default(), HttpSessionBudget::new(128)?)?;
    let catalog = value(
        f.service
            .handle(request(Method::GET, "/api/v1/tools", Value::Null))
            .await,
    )
    .await?;
    assert_eq!(catalog["contract_version"], 1);
    let tools = catalog["tools"].as_array().unwrap();
    assert_eq!(
        tools
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["search", "status"]
    );
    assert_eq!(
        tools[0]["outputSchema"],
        json!({"type":"object","additionalProperties":true})
    );
    let args = json!({"revision":"exact:original","cursor":"cursor:original","response_mode":"full","max_bytes":8192});
    let response = f
        .service
        .handle(request(Method::POST, "/api/v1/tools/search", args.clone()))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let actual = value(response).await?;
    let expected = super::super::call_observed(
        &Echo {
            calls: Arc::new(Mutex::new(vec![])),
            closed: Arc::new(AtomicUsize::new(0)),
            clients: Arc::new(Mutex::new(vec![])),
        },
        "search".into(),
        args.as_object().unwrap().clone(),
    )
    .await?;
    assert_eq!(actual, expected);
    assert_eq!(
        actual["structuredContent"]["coverage"]["future_native_field"],
        json!([1, 2, 3])
    );
    for name in ["draft_plan", "erase_everything", "shutdown", "../search"] {
        assert_eq!(
            f.service
                .handle(request(
                    Method::POST,
                    &format!("/api/v1/tools/{name}"),
                    json!({})
                ))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(f.calls.lock().unwrap().len(), 1);
    let alias = value(
        f.service
            .handle(request(Method::POST, "/api/v1/tools/airk_search", args))
            .await,
    )
    .await?;
    assert_eq!(
        alias, expected,
        "published native alias dispatches the advertised canonical read handler"
    );
    assert_eq!(f.calls.lock().unwrap().len(), 2);
    f.service.close().await;
    Ok(())
}

#[tokio::test]
async fn json_api_framing_host_origin_limits_and_methods_refuse_before_canonical_execution()
-> anyhow::Result<()> {
    let f = fixture(ReadSessionFactory::default(), HttpSessionBudget::new(128)?)?;
    for (request, status) in [
        (
            request(Method::POST, "/api/v1/tools/search", json!([])),
            StatusCode::BAD_REQUEST,
        ),
        (
            request(Method::GET, "/api/v1/tools/search", json!({})),
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        (
            request(Method::POST, "/api/v1/tools/search?root=/other", json!({})),
            StatusCode::BAD_REQUEST,
        ),
        (
            request(
                Method::POST,
                "/api/v1/tools/search",
                json!({"query":"x".repeat(65536)}),
            ),
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
    ] {
        assert_eq!(f.service.handle(request).await.status(), status);
    }
    let mut hostile = request(Method::POST, "/api/v1/tools/search", json!({}));
    hostile
        .headers_mut()
        .insert("origin", "https://hostile.example".parse()?);
    assert_eq!(
        f.service.handle(hostile).await.status(),
        StatusCode::FORBIDDEN
    );
    let mut crossed = request(Method::POST, "/api/v1/tools/search", json!({}));
    crossed
        .headers_mut()
        .insert("host", "localhost:57421".parse()?);
    assert_eq!(
        f.service.handle(crossed).await.status(),
        StatusCode::FORBIDDEN
    );
    assert!(f.calls.lock().unwrap().is_empty());
    f.service.close().await;
    Ok(())
}

#[tokio::test]
async fn standard_streamable_http_negotiates_lists_calls_and_keeps_credentials_sessions_separate()
-> anyhow::Result<()> {
    let factory = ReadSessionFactory::default();
    let budget = HttpSessionBudget::new(128)?;
    let a = fixture(factory.clone(), budget.clone())?;
    let b = fixture(factory, budget)?;
    let initialize = request(
        Method::POST,
        "/mcp",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}}}),
    );
    let response = a.service.handle(initialize).await;
    assert_eq!(response.status(), StatusCode::OK);
    let session = response
        .headers()
        .get("mcp-session-id")
        .expect("legacy MCP session header")
        .clone();
    let initialized = value(response).await?;
    assert_eq!(initialized["result"]["protocolVersion"], "2025-11-25");
    let mut ready = request(
        Method::POST,
        "/mcp",
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    ready
        .headers_mut()
        .insert("mcp-session-id", session.clone());
    assert!(a.service.handle(ready).await.status().is_success());
    let mut listed = request(
        Method::POST,
        "/mcp",
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    );
    listed
        .headers_mut()
        .insert("mcp-session-id", session.clone());
    let listed = value(a.service.handle(listed).await).await?;
    assert_eq!(
        listed["result"]["tools"].as_array().unwrap().len(),
        4,
        "MCP retains the existing canonical catalog including reviewed draft proposals"
    );
    let mut call = request(
        Method::POST,
        "/mcp",
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search","arguments":{"revision":"fixed","response_mode":"full"}}}),
    );
    call.headers_mut().insert("mcp-session-id", session.clone());
    let result = value(a.service.handle(call).await).await?;
    assert_eq!(
        result["result"]["structuredContent"]["arguments"]["revision"],
        "fixed"
    );
    let mut crossed = request(
        Method::POST,
        "/mcp",
        json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":{}}),
    );
    crossed
        .headers_mut()
        .insert("mcp-session-id", session.clone());
    assert_eq!(
        b.service.handle(crossed).await.status(),
        StatusCode::NOT_FOUND
    );
    let mut deleted = request(Method::DELETE, "/mcp", Value::Null);
    deleted.headers_mut().insert("mcp-session-id", session);
    assert!(a.service.handle(deleted).await.status().is_success());
    a.service.close().await;
    assert_eq!(
        b.service
            .handle(request(Method::POST, "/api/v1/tools/search", json!({})))
            .await
            .status(),
        StatusCode::OK,
        "closing one credential/session cannot close service-global query admission"
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        while a.closed.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    b.service.close().await;
    Ok(())
}

#[tokio::test]
async fn session_budget_is_global_idle_bounded_and_delete_releases_only_its_own_reservation()
-> anyhow::Result<()> {
    let budget = HttpSessionBudget::new(1)?;
    let limits = HttpReadLimits {
        idle_timeout: Duration::from_millis(30),
        ..Default::default()
    };
    let a = BoundedSessions::new(budget.clone(), limits, CancellationToken::new());
    let b = BoundedSessions::new(budget, limits, CancellationToken::new());
    let (id, transport) = a.create_session().await?;
    assert!(b.create_session().await.is_err());
    a.close_session(&id).await?;
    drop(transport);
    let (id, transport) = b.create_session().await?;
    assert!(b.has_session(&id).await?);
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(!b.has_session(&id).await?);
    drop(transport);
    let (_id, transport) = a.create_session().await?;
    drop(transport);
    a.shutdown().await;
    b.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn complete_native_refusals_remain_http_200_and_the_transport_never_adds_a_retry()
-> anyhow::Result<()> {
    let f = fixture(ReadSessionFactory::default(), HttpSessionBudget::new(128)?)?;
    let response = f
        .service
        .handle(request(
            Method::POST,
            "/api/v1/tools/search",
            json!({"fixture_error":true}),
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = value(response).await?;
    assert_eq!(body["isError"], true);
    assert_eq!(body["structuredContent"]["code"], "selection_busy");
    assert_eq!(
        body["structuredContent"]["future_receipt"],
        json!({"retained":true})
    );
    assert_eq!(f.calls.lock().unwrap().len(), 1);
    f.service.close().await;
    Ok(())
}

#[tokio::test]
async fn global_capacity_and_reserved_status_survive_abandoned_http_callers() -> anyhow::Result<()>
{
    struct Blocked {
        started: Arc<AtomicUsize>,
        finished: Arc<AtomicUsize>,
        release: Arc<Semaphore>,
    }
    impl RemoteTools for Blocked {
        fn query_concurrency(&self) -> usize {
            4
        }
        fn call(&self, name: String, _args: serde_json::Map<String, Value>) -> RemoteFuture<'_> {
            Box::pin(async move {
                if name == "airk_status" {
                    return Ok(serde_json::to_value(CallToolResult::structured(
                        json!({"health":"ready"}),
                    ))?);
                }
                self.started.fetch_add(1, Ordering::SeqCst);
                let _permit = self.release.acquire().await?;
                self.finished.fetch_add(1, Ordering::SeqCst);
                Ok(serde_json::to_value(CallToolResult::success(vec![]))?)
            })
        }
    }
    let global = ReadSessionFactory::default();
    let client = global.client();
    let other = global.client();
    let preferences = ReadSessionPreferences {
        response_mode: crate::response::ResponseMode::Full,
        client: airk_core::usage::ClientFamily::Unknown,
    };
    let started = Arc::new(AtomicUsize::new(0));
    let finished = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(Semaphore::new(0));
    let backend = || {
        Arc::new(Blocked {
            started: started.clone(),
            finished: finished.clone(),
            release: release.clone(),
        }) as Arc<dyn RemoteTools>
    };

    struct CheckpointGuard(Arc<AtomicUsize>);
    impl Drop for CheckpointGuard {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let guard_drops = Arc::new(AtomicUsize::new(0));
    let mut calls = vec![];
    for _ in 0..4 {
        let session = client.session(backend(), preferences);
        let context = crate::read_attempt::ReadAttemptContext::new(
            std::time::Instant::now() + Duration::from_secs(5),
            CancellationToken::new(),
        )
        .with_lifetime_guard(Arc::new(CheckpointGuard(guard_drops.clone())));
        calls.push(tokio::spawn(async move {
            session
                .call_read_with_context("airk_search".into(), Default::default(), context)
                .await
        }));
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        while started.load(Ordering::SeqCst) != 4 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert_eq!(global.admission.calls.available_permits(), 0);
    calls.remove(0).abort();
    tokio::task::yield_now().await;
    assert_eq!(
        global.admission.calls.available_permits(),
        0,
        "abandoning a caller cannot release accepted native capacity"
    );
    assert_eq!(
        guard_drops.load(Ordering::SeqCst),
        0,
        "accepted tasks retain checkpoint guards even when default backends drop their context and a caller aborts"
    );
    let status = other.session(backend(), preferences);
    let reply = tokio::time::timeout(
        Duration::from_secs(1),
        status.call_read(
            "airk_status".into(),
            Default::default(),
            CancellationToken::new(),
        ),
    )
    .await?
    .unwrap();
    assert_eq!(reply.structured_content.unwrap()["health"], "ready");
    release.add_permits(4);
    for call in calls {
        call.await?.unwrap();
    }
    tokio::time::timeout(Duration::from_secs(2), async {
        while finished.load(Ordering::SeqCst) != 4 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    tokio::time::timeout(Duration::from_secs(2), async {
        while guard_drops.load(Ordering::SeqCst) != 4 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    assert_eq!(global.admission.calls.available_permits(), 4);
    Ok(())
}

#[tokio::test]
async fn concurrent_http_headers_are_request_local_canonical_arguments_win_and_completed_read_does_not_cancel_output()
-> anyhow::Result<()> {
    let f = fixture(ReadSessionFactory::default(), HttpSessionBudget::new(128)?)?;
    let response=f.service.handle(request(Method::POST,"/mcp",json!({"jsonrpc":"2.0","id":10,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"codex","version":"1"}}}))).await;
    let session = response.headers().get("mcp-session-id").unwrap().clone();
    value(response).await?;
    let mut ready = request(
        Method::POST,
        "/mcp",
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    ready
        .headers_mut()
        .insert("mcp-session-id", session.clone());
    f.service.handle(ready).await;
    let make = |id, mode: &str, args: Value| {
        let mut call = request(
            Method::POST,
            "/mcp",
            json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"search","arguments":args}}),
        );
        call.headers_mut().insert("mcp-session-id", session.clone());
        call.headers_mut()
            .insert("x-airk-response-mode", mode.parse().unwrap());
        call
    };
    let first = make(11, "terse", json!({"revision":"terse-request"}));
    let second = make(12, "full", json!({"revision":"full-request"}));
    let a = crate::read_attempt::ReadAttemptContext::new(
        std::time::Instant::now() + Duration::from_secs(5),
        CancellationToken::new(),
    );
    let b = crate::read_attempt::ReadAttemptContext::new(
        std::time::Instant::now() + Duration::from_secs(5),
        CancellationToken::new(),
    );
    let (first, second) = tokio::join!(
        f.service.handle_with_context(first, a.clone()),
        f.service.handle_with_context(second, b.clone())
    );
    let first = value(first).await?;
    let second = value(second).await?;
    assert_eq!(
        first["result"]["structuredContent"]["arguments"]["response_mode"],
        "terse"
    );
    assert_eq!(
        second["result"]["structuredContent"]["arguments"]["response_mode"],
        "full"
    );
    assert!(
        !a.cancellation().is_cancelled() && !b.cancellation().is_cancelled(),
        "successful MCP handler leaves the original output lifetime live"
    );
    let explicit = value(
        f.service
            .handle(make(13, "terse", json!({"response_mode":"full"})))
            .await,
    )
    .await?;
    assert_eq!(
        explicit["result"]["structuredContent"]["arguments"]["response_mode"],
        "full"
    );
    assert!(
        f.clients
            .lock()
            .unwrap()
            .iter()
            .all(|family| *family == airk_core::usage::ClientFamily::from_label("codex"))
    );
    assert_eq!(f.clients.lock().unwrap().len(), 3);
    assert_eq!(
        f.service
            .handle(make(14, "invalid", json!({})))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let mut raw = request(Method::POST, "/api/v1/tools/search", json!({}));
    raw.headers_mut()
        .insert("x-airk-response-mode", "terse".parse()?);
    assert_eq!(
        f.service.handle(raw).await.status(),
        StatusCode::BAD_REQUEST
    );
    f.service.close().await;
    Ok(())
}

#[tokio::test]
async fn lower_http_response_override_still_bounds_complete_encoded_envelopes() -> anyhow::Result<()>
{
    let body = json!({"evidence":"x".repeat(airk_core::MAX_RESPONSE_BYTES / 2)});
    let result = crate::remote::complete_public_envelope(CallToolResult::success(vec![
        rmcp::model::ContentBlock::text(body.to_string()),
    ]));
    let expected = serde_json::to_vec(&result)?;
    assert!(expected.len() > airk_core::MAX_RESPONSE_BYTES);
    let accepted = json(StatusCode::OK, &result, expected.len());
    assert_eq!(accepted.status(), StatusCode::OK);
    assert_eq!(
        accepted.into_body().collect().await?.to_bytes().as_ref(),
        expected.as_slice()
    );
    let refused = json(StatusCode::OK, &result, airk_core::MAX_RESPONSE_BYTES);
    assert_eq!(refused.status(), StatusCode::BAD_GATEWAY);
    let error: Value = serde_json::from_slice(&refused.into_body().collect().await?.to_bytes())?;
    assert_eq!(error["error"]["code"], "canonical_response_limit");
    Ok(())
}
