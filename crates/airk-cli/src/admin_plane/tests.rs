// SPDX-License-Identifier: Apache-2.0
//! Admin plane state and handler rules with a manual clock and fixed random
//! bytes; no listener, no sleeps.
use super::*;
use crate::admin_contract::{COOKIE_NAME, MAX_SESSIONS, session_cookie};
use http_body_util::Full;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

const PORT: u16 = 41234;
const ADMIN_LAUNCH: &str = include_str!("../../../../fixtures/split/i1/admin_launch_code.json");
const MANIFEST: &str = include_str!("../../admin-assets/manifest.json");

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A plane whose clock the test sets and whose random source yields
/// `[n; 32]` for n = 1, 2, 3, ….
fn plane(instance: u8) -> (AdminPlane, Arc<AtomicU64>) {
    let now = Arc::new(AtomicU64::new(0));
    let clock = now.clone();
    let counter = Arc::new(AtomicU8::new(0));
    let dispatch: Dispatch = Arc::new(|_| {
        Box::pin(async {
            anyhow::Result::<ControlResponse>::Err(anyhow::anyhow!("no service in unit tests"))
        })
    });
    let plane = AdminPlane::with_sources(
        dispatch,
        Incarnation::from_bytes([instance; 16]),
        Arc::new(move || clock.load(Ordering::SeqCst)),
        Arc::new(move || Some([counter.fetch_add(1, Ordering::SeqCst) + 1; 32])),
    );
    (plane, now)
}

fn launch(plane: &AdminPlane) -> String {
    match plane.launch_code(PORT) {
        ControlResponse::AdminLaunchCode {
            code,
            port,
            expires_in_ms,
        } => {
            assert_eq!((port, expires_in_ms), (PORT, 60_000));
            code.as_str().to_owned()
        }
        _ => panic!("A1 did not issue a code"),
    }
}

#[test]
fn secret_text_is_canonical_unpadded_base64url() {
    assert_eq!(
        secrets::secret_text(&[1; 32]),
        "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE"
    );
    assert_eq!(
        secrets::secret_text(&[0xff; 32]),
        "__________________________________________8"
    );
    for byte in [0u8, 7, 0x80, 0xfe] {
        assert!(contract::is_secret_text(&secrets::secret_text(&[byte; 32])));
    }
}

/// Replays `fixtures/split/i1/admin_launch_code.json`: issuance, stored
/// digests, single use, expiry, eviction and restart.
#[test]
fn launch_codes_follow_the_reviewed_transcripts() {
    let fixture: Value = serde_json::from_str(ADMIN_LAUNCH).unwrap();
    let instance = Incarnation::from_bytes([0xe1; 16]);
    let mut replayed = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let transcript = case["transcript"].as_array().unwrap();
        if transcript.iter().any(|step| {
            matches!(step["channel"].as_str(), Some("session" | "http"))
                || step.get("refused_before_dispatch").is_some()
        }) {
            continue;
        }
        let mut codes = secrets::LaunchCodes::new(instance);
        let mut issued = 0u8;
        let mut created = 0;
        for step in transcript {
            let now = step["at_ms"].as_u64().unwrap();
            if let Some(reply) = step.get("reply") {
                issued += 1;
                let (text, ttl) = codes.issue(&[issued; 32], now);
                assert_eq!(text, reply["code"].as_str().unwrap(), "{name}");
                assert_eq!(ttl, reply["expires_in_ms"].as_u64().unwrap(), "{name}");
            } else if let Some(stored) = step.get("service_stores") {
                let digests = codes.stored_digests();
                let (digest, issued_at) = digests.last().unwrap();
                assert_eq!(hex(digest), stored["sha256"].as_str().unwrap(), "{name}");
                assert_eq!(
                    *issued_at,
                    stored["issued_at_ms"].as_u64().unwrap(),
                    "{name}"
                );
            } else if step["harness"] == "service_restarts" {
                codes = secrets::LaunchCodes::new(Incarnation::from_bytes([0xe2; 16]));
            } else if let Some(presented) = step.get("consume") {
                let request = contract::ExchangeRequest {
                    code: presented.as_str().unwrap().to_owned(),
                };
                let stored = codes.consume(&request.code);
                let outcome = contract::exchange(&request, stored.as_ref(), now);
                match step["expect"]["result"].as_str().unwrap() {
                    "session_created" => {
                        assert_eq!(outcome, Ok(()), "{name} at {now}");
                        created += 1;
                    }
                    _ => assert_eq!(
                        outcome,
                        Err(AdminCode::AdminLaunchCodeInvalid),
                        "{name} at {now}"
                    ),
                }
            }
        }
        let expected = &case["expected"];
        if let Some(outstanding) = expected["outstanding"].as_u64() {
            assert_eq!(codes.outstanding() as u64, outstanding, "{name}");
        }
        if let Some(sessions) = expected["sessions_created"].as_u64() {
            assert_eq!(created, sessions, "{name}");
        }
        replayed += 1;
    }
    assert!(replayed >= 7, "{replayed}");
}

#[test]
fn sessions_expire_evict_and_bind_to_their_instance() {
    let mut sessions = secrets::Sessions::new(Incarnation::from_bytes([1; 16]));
    let minted = sessions.create(&[1; 32], &[2; 32], 0);
    let facts = sessions
        .facts(&[minted.cookie.as_str()], &[minted.csrf.as_str()], 1)
        .unwrap();
    assert!(facts.same_instance && facts.token_matches);
    // A wrong, missing or doubled token is a session without a matching token.
    let wrong = secrets::secret_text(&[3; 32]);
    assert!(
        !sessions
            .facts(&[minted.cookie.as_str()], &[wrong.as_str()], 1)
            .unwrap()
            .token_matches
    );
    assert!(
        !sessions
            .facts(&[minted.cookie.as_str()], &[], 1)
            .unwrap()
            .token_matches
    );
    // Zero or several admin cookies name no session.
    assert!(sessions.facts(&[], &[minted.csrf.as_str()], 1).is_none());
    assert!(
        sessions
            .facts(
                &[minted.cookie.as_str(), minted.cookie.as_str()],
                &[minted.csrf.as_str()],
                1
            )
            .is_none()
    );
    // Idle expiry, renewed only by `touch`.
    sessions.touch(&minted.cookie, SESSION_IDLE_MS - 1);
    assert!(
        sessions
            .facts(&[minted.cookie.as_str()], &[], 2 * SESSION_IDLE_MS - 2)
            .is_some()
    );
    assert!(
        sessions
            .facts(&[minted.cookie.as_str()], &[], 2 * SESSION_IDLE_MS - 1)
            .is_none()
    );
    // Absolute expiry, whatever the activity.
    let late = sessions.create(&[4; 32], &[5; 32], 0);
    let mut now = 0;
    while now + SESSION_IDLE_MS / 2 < SESSION_ABSOLUTE_MS {
        now += SESSION_IDLE_MS / 2;
        sessions.touch(&late.cookie, now);
    }
    assert!(sessions.facts(&[late.cookie.as_str()], &[], now).is_some());
    assert!(
        sessions
            .facts(&[late.cookie.as_str()], &[], SESSION_ABSOLUTE_MS)
            .is_none()
    );
    // At most 16; the oldest is evicted.
    let mut sessions = secrets::Sessions::new(Incarnation::from_bytes([1; 16]));
    let first = sessions.create(&[10; 32], &[11; 32], 0);
    for index in 0..MAX_SESSIONS as u8 {
        sessions.create(&[20 + index; 32], &[60 + index; 32], 1);
    }
    assert_eq!(sessions.len(), MAX_SESSIONS);
    assert!(sessions.facts(&[first.cookie.as_str()], &[], 2).is_none());
    // Logout removes exactly that session.
    let last = secrets::secret_text(&[20 + MAX_SESSIONS as u8 - 1; 32]);
    assert!(sessions.remove(&last));
    assert!(!sessions.remove(&last));
    assert_eq!(sessions.len(), MAX_SESSIONS - 1);
}

#[test]
fn every_manifest_entry_matches_its_embedded_bytes() {
    let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
    assert_eq!(manifest["schema"], "airk-admin-assets/1");
    let files = manifest["files"].as_object().unwrap();
    assert!(files.contains_key(assets::SHELL));
    assert_eq!(files.len(), assets::ADMIN_ASSETS.len());
    for (name, digest, content_type, bytes) in assets::ADMIN_ASSETS {
        let entry = &files[*name];
        assert_eq!(entry["sha256"], *digest, "{name}");
        assert_eq!(entry["content_type"], *content_type, "{name}");
        assert_eq!(hex(&Sha256::digest(bytes)), *digest, "{name}");
        assert!(contract::is_asset_name(name), "{name}");
    }
}

#[test]
fn the_reviewed_table_validates_and_names_control_methods() {
    let table = operations::table().unwrap();
    assert!(table.admin_row("service.health").is_some());
    assert!(table.admin_row("no.such_operation").is_none());
    for row in &table.operations {
        assert!(
            row.surfaces.contains(&contract::Surface::Admin)
                == table.admin_row(&row.operation).is_some()
        );
    }
    let health = serde_json::to_value(ControlMethod::Health).unwrap();
    assert!(operations::names("Health", &health));
    assert!(!operations::names("Resources", &health));
    let status = json!({"http_issuer_rotation": {"action": "status"}});
    assert!(operations::names("HttpIssuerRotation.Status", &status));
    assert!(!operations::names("HttpIssuerRotation.Commit", &status));
    let launch = serde_json::to_value(ControlMethod::AdminLaunchCode {}).unwrap();
    assert!(table.never_over_http(&launch));
    assert!(!table.never_over_http(&health));
}

#[test]
fn table_parsers_find_the_peer_end_of_the_connection() {
    let client: std::net::SocketAddrV4 = "127.0.0.1:50000".parse().unwrap();
    let server: std::net::SocketAddrV4 = "127.0.0.1:57421".parse().unwrap();
    // Two rows: the service's end (pid 7) and the client's end (pid 9).
    let row = |local: std::net::SocketAddrV4, remote: std::net::SocketAddrV4, pid: u32| {
        let mut bytes = Vec::new();
        bytes.extend(5u32.to_ne_bytes());
        bytes.extend(local.ip().octets());
        bytes.extend(local.port().to_be_bytes());
        bytes.extend([0, 0]);
        bytes.extend(remote.ip().octets());
        bytes.extend(remote.port().to_be_bytes());
        bytes.extend([0, 0]);
        bytes.extend(pid.to_ne_bytes());
        bytes
    };
    let mut table = 2u32.to_ne_bytes().to_vec();
    table.extend(row(server, client, 7));
    table.extend(row(client, server, 9));
    assert_eq!(peer::windows_owner_pid(&table, client, server), Some(9));
    assert_eq!(peer::windows_owner_pid(&table, server, client), Some(7));
    assert_eq!(peer::windows_owner_pid(&table[..30], client, server), None);
    let other: std::net::SocketAddrV4 = "127.0.0.1:50001".parse().unwrap();
    assert_eq!(peer::windows_owner_pid(&table, other, server), None);
    let address = |end: std::net::SocketAddrV4| {
        format!(
            "{:08X}:{:04X}",
            u32::from_ne_bytes(end.ip().octets()),
            end.port()
        )
    };
    let proc_table = format!(
        "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: {} {} 01 00000000:00000000 00:00000000 00000000  1000        0 1 1\n   1: {} {} 01 00000000:00000000 00:00000000 00000000  1001        0 2 1\n",
        address(server),
        address(client),
        address(client),
        address(server)
    );
    assert_eq!(
        peer::linux_peer_uid(&proc_table, client, server),
        Some(1001)
    );
    assert_eq!(
        peer::linux_peer_uid(&proc_table, server, client),
        Some(1000)
    );
    assert_eq!(peer::linux_peer_uid(&proc_table, other, server), None);
}

fn request(
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Request<Full<Bytes>> {
    let mut builder = Request::builder().method(method).uri(target);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    builder
        .body(Full::new(Bytes::from(body.to_owned())))
        .unwrap()
}

const HOST: (&str, &str) = ("host", "127.0.0.1:41234");
const ORIGIN: (&str, &str) = ("origin", "https://127.0.0.1:41234");
const SAME_ORIGIN: (&str, &str) = ("sec-fetch-site", "same-origin");
const JSON: (&str, &str) = ("content-type", "application/json");

async fn answer(
    plane: &AdminPlane,
    request: Request<Full<Bytes>>,
    proof: PeerProof,
) -> (u16, http::HeaderMap, Bytes) {
    let response = plane
        .handle(request, Some(ConnectionPeer::proven(proof)), PORT, false)
        .await;
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, body)
}

fn code_of(body: &Bytes) -> String {
    let value: Value = serde_json::from_slice(body).unwrap();
    value["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// Exchange a fresh code; returns (cookie value, anti-forgery token).
async fn sign_in(plane: &AdminPlane) -> (String, String) {
    let code = launch(plane);
    let body = json!({ "code": code }).to_string();
    let (status, headers, reply) = answer(
        plane,
        request(
            "POST",
            "/admin/api/v1/session",
            &[HOST, ORIGIN, SAME_ORIGIN, JSON],
            &body,
        ),
        PeerProof::SameUser,
    )
    .await;
    assert_eq!(status, 200, "{}", String::from_utf8_lossy(&reply));
    let set_cookie = headers["set-cookie"].to_str().unwrap().to_owned();
    let cookie = set_cookie
        .strip_prefix(&format!("{COOKIE_NAME}="))
        .and_then(|rest| rest.split(';').next())
        .unwrap()
        .to_owned();
    assert_eq!(set_cookie, session_cookie(&cookie));
    let envelope: Value = serde_json::from_slice(&reply).unwrap();
    assert_eq!(envelope["operation"], "session.exchange");
    let result: ExchangeResponse = serde_json::from_value(envelope["result"].clone()).unwrap();
    result.validate().unwrap();
    assert_eq!(result.service_instance, plane.instance.to_string());
    (cookie, result.csrf)
}

#[tokio::test]
async fn static_routes_need_no_session_and_carry_the_contract_headers() {
    let (plane, _) = plane(1);
    let (status, headers, body) = answer(
        &plane,
        request("GET", "/admin/", &[HOST], ""),
        PeerProof::Unavailable,
    )
    .await;
    assert_eq!(status, 200);
    for (name, value) in contract::response_headers(ResponseKind::Shell) {
        assert_eq!(headers[name], value, "{name}");
    }
    assert_eq!(&body[..], assets::find(assets::SHELL).unwrap().bytes);
    let (status, _, launch_page) = answer(
        &plane,
        request(
            "GET",
            "/admin/launch",
            &[HOST, ("sec-fetch-site", "cross-site")],
            "",
        ),
        PeerProof::Unavailable,
    )
    .await;
    assert_eq!((status, launch_page), (200, body));
    let (status, headers, _) = answer(
        &plane,
        request("GET", "/admin", &[HOST], ""),
        PeerProof::Unavailable,
    )
    .await;
    assert_eq!(
        (status, headers["location"].to_str().unwrap()),
        (308, "/admin/")
    );
    let (status, _, body) = answer(
        &plane,
        request("GET", "/admin/assets/missing.js", &[HOST], ""),
        PeerProof::Unavailable,
    )
    .await;
    assert_eq!((status, code_of(&body).as_str()), (404, "admin_not_found"));
    // Planes stay separate: a bearer is refused even on a static route.
    let (status, _, body) = answer(
        &plane,
        request("GET", "/admin/", &[HOST, ("authorization", "Bearer x")], ""),
        PeerProof::Unavailable,
    )
    .await;
    assert_eq!(
        (status, code_of(&body).as_str()),
        (401, "admin_bearer_refused")
    );
    // No launch code was issued by any of this.
    assert_eq!(plane.outstanding_codes(), 0);
}

#[tokio::test]
async fn the_exchange_is_single_use_and_requires_a_same_user_peer() {
    let (plane, now) = plane(1);
    let code = launch(&plane);
    let body = json!({ "code": code }).to_string();
    let exchange = || {
        request(
            "POST",
            "/admin/api/v1/session",
            &[HOST, ORIGIN, SAME_ORIGIN, JSON],
            &body,
        )
    };
    // A foreign or unproven peer is refused before the code is consumed.
    for proof in [PeerProof::OtherUser, PeerProof::Unavailable] {
        let (status, _, reply) = answer(&plane, exchange(), proof).await;
        assert_eq!(
            (status, code_of(&reply).as_str()),
            (403, "admin_peer_refused")
        );
    }
    assert_eq!(plane.outstanding_codes(), 1);
    let (status, _, _) = answer(&plane, exchange(), PeerProof::SameUser).await;
    assert_eq!(status, 200);
    assert_eq!((plane.outstanding_codes(), plane.sessions()), (0, 1));
    let (status, _, reply) = answer(&plane, exchange(), PeerProof::SameUser).await;
    assert_eq!(
        (status, code_of(&reply).as_str()),
        (401, "admin_launch_code_invalid")
    );
    // Expiry at 60 s on the injected clock.
    let late = launch(&plane);
    now.store(60_000, Ordering::SeqCst);
    let body = json!({ "code": late }).to_string();
    let (status, _, reply) = answer(
        &plane,
        request(
            "POST",
            "/admin/api/v1/session",
            &[HOST, ORIGIN, SAME_ORIGIN, JSON],
            &body,
        ),
        PeerProof::SameUser,
    )
    .await;
    assert_eq!(
        (status, code_of(&reply).as_str()),
        (401, "admin_launch_code_invalid")
    );
    // A code from another instance (a restart) is unknown here.
    let (other, _) = self::plane(2);
    let foreign = launch(&other);
    let body = json!({ "code": foreign }).to_string();
    let (status, _, _) = answer(
        &plane,
        request(
            "POST",
            "/admin/api/v1/session",
            &[HOST, ORIGIN, SAME_ORIGIN, JSON],
            &body,
        ),
        PeerProof::SameUser,
    )
    .await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn calls_need_the_cookie_the_token_and_a_reviewed_operation() {
    let (plane, _) = plane(1);
    let (cookie, csrf) = sign_in(&plane).await;
    let cookie_header = format!("{COOKIE_NAME}={cookie}");
    let call = |target: &str, csrf: Option<&str>, body: &str| {
        let mut headers = vec![
            HOST,
            ORIGIN,
            SAME_ORIGIN,
            JSON,
            ("cookie", cookie_header.as_str()),
        ];
        if let Some(csrf) = csrf {
            headers.push(("x-airk-csrf", csrf));
        }
        request("POST", target, &headers, body)
    };
    let (status, _, reply) = answer(
        &plane,
        call("/admin/api/v1/call/service.health", None, "\"health\""),
        PeerProof::SameUser,
    )
    .await;
    assert_eq!(
        (status, code_of(&reply).as_str()),
        (401, "admin_session_invalid")
    );
    let (status, _, reply) = answer(
        &plane,
        call(
            "/admin/api/v1/call/no.such_operation",
            Some(csrf.as_str()),
            "\"health\"",
        ),
        PeerProof::SameUser,
    )
    .await;
    assert_eq!(
        (status, code_of(&reply).as_str()),
        (404, "admin_operation_unknown")
    );
    // The body must decode as the row's own method.
    let (status, _, reply) = answer(
        &plane,
        call(
            "/admin/api/v1/call/service.health",
            Some(csrf.as_str()),
            "\"resources\"",
        ),
        PeerProof::SameUser,
    )
    .await;
    assert_eq!(
        (status, code_of(&reply).as_str()),
        (400, "admin_malformed_request")
    );
    let (status, _, reply) = answer(
        &plane,
        call(
            "/admin/api/v1/call/service.health",
            Some(csrf.as_str()),
            "{\"admin_launch_code\":{}}",
        ),
        PeerProof::SameUser,
    )
    .await;
    assert_eq!(
        (status, code_of(&reply).as_str()),
        (404, "admin_operation_unknown")
    );
    // Mutations wait for the journaled admin path.
    let revoke = json!({"http_revoke": {"connection_id": "1c".repeat(16)}}).to_string();
    let (status, _, reply) = answer(
        &plane,
        call(
            "/admin/api/v1/call/agents.revoke",
            Some(csrf.as_str()),
            &revoke,
        ),
        PeerProof::SameUser,
    )
    .await;
    assert_eq!((status, code_of(&reply).as_str()), (503, "not_yet_served"));
    // A valid cookie and token from a foreign peer are refused (H1).
    let (status, _, reply) = answer(
        &plane,
        call(
            "/admin/api/v1/call/service.health",
            Some(csrf.as_str()),
            "\"health\"",
        ),
        PeerProof::OtherUser,
    )
    .await;
    assert_eq!(
        (status, code_of(&reply).as_str()),
        (403, "admin_peer_refused")
    );
    // Logout clears the cookie and ends the session.
    let (status, headers, _) = answer(
        &plane,
        call("/admin/api/v1/session/logout", Some(csrf.as_str()), "{}"),
        PeerProof::SameUser,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(headers["set-cookie"], CLEAR_SESSION_COOKIE);
    assert_eq!(plane.sessions(), 0);
    let (status, _, reply) = answer(
        &plane,
        call(
            "/admin/api/v1/call/service.health",
            Some(csrf.as_str()),
            "\"health\"",
        ),
        PeerProof::SameUser,
    )
    .await;
    assert_eq!(
        (status, code_of(&reply).as_str()),
        (401, "admin_session_invalid")
    );
    // A session of another instance is unknown after a restart.
    let (restarted, _) = self::plane(2);
    let (status, _, _) = answer(
        &restarted,
        call(
            "/admin/api/v1/call/service.health",
            Some(csrf.as_str()),
            "\"health\"",
        ),
        PeerProof::SameUser,
    )
    .await;
    assert_eq!(status, 401);
}

#[test]
fn native_answers_keep_the_complete_response() {
    let read = |response: HttpResponse| async move {
        let status = response.status().as_u16();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice::<Value>(&body).unwrap())
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let (status, envelope) = runtime.block_on(read(native(
        "agents.verify",
        ControlResponse::Failure {
            code: "registry_busy".into(),
        },
    )));
    assert_eq!(status, 409);
    assert_eq!(
        envelope["error"],
        json!({"code": "registry_busy", "class": "refused", "retryable": true,
            "native": {"status": "failure", "code": "registry_busy"}})
    );
    let envelope: FailureEnvelope = serde_json::from_value(envelope).unwrap();
    envelope.validate().unwrap();
    let (status, envelope) = runtime.block_on(read(native(
        "agents.verify",
        ControlResponse::AgentRefused {
            message: "native HTTPS endpoint is not available".into(),
        },
    )));
    assert_eq!(
        (status, envelope["error"]["code"].as_str()),
        (409, Some("agent_refused"))
    );
    let (status, envelope) = runtime.block_on(read(native(
        "service.health",
        ControlResponse::ClientErrorRecorded {},
    )));
    assert_eq!(status, 200);
    assert_eq!(
        envelope["result"],
        json!({"status": "client_error_recorded"})
    );
    let envelope: SuccessEnvelope = serde_json::from_value(envelope).unwrap();
    envelope.validate().unwrap();
}
