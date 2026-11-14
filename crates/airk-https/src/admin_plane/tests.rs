// SPDX-License-Identifier: Apache-2.0
//! Admin plane state and handler rules with a manual clock and fixed random
//! bytes; no listener, no sleeps.
mod event_stream;
mod preparation;
mod session_permit;
mod ui_pilot_cases;
use super::*;
use airk_endpoint::admin_contract::{COOKIE_NAME, MAX_SESSIONS, session_cookie};
use http_body_util::Full;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    sync::atomic::{AtomicU8, AtomicU64, Ordering},
};

const PORT: u16 = 41234;
const ADMIN_LAUNCH: &str = include_str!("../../../../fixtures/split/i1/admin_launch_code.json");
const MANIFEST: &str = include_str!("../../../airk-cli/admin-assets/manifest.json");

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

#[test]
fn macos_own_socket_scan_proves_only_the_client_end() {
    use airk_endpoint::admin_contract::{PeerObservation, PeerProof, peer_proof};
    let client: std::net::SocketAddrV4 = "127.0.0.1:50000".parse().unwrap();
    let server: std::net::SocketAddrV4 = "127.0.0.1:57421".parse().unwrap();
    let other: std::net::SocketAddrV4 = "127.0.0.1:50001".parse().unwrap();
    let socket = |local, remote| peer::OwnSocket { local, remote };
    let service_end = socket(server, client);
    let client_end = socket(client, server);
    let found = |sockets: Vec<peer::OwnSocket>| peer::macos_found_among(sockets, client, server);
    assert_eq!(found(vec![service_end, client_end]), Some(true));
    // The same socket shared by two same-uid processes still proves it.
    assert_eq!(found(vec![client_end, client_end]), Some(true));
    // Only the service's own end: the browser socket is not the user's.
    assert_eq!(found(vec![service_end]), None);
    assert_eq!(
        found(vec![socket(other, server), socket(client, other)]),
        None
    );
    assert_eq!(found(Vec::new()), None);
    let proof = |found_among_service_user_sockets| {
        peer_proof(&PeerObservation::Macos {
            found_among_service_user_sockets,
        })
    };
    assert_eq!(proof(found(vec![client_end])), PeerProof::SameUser);
    assert_eq!(proof(found(vec![service_end])), PeerProof::Unavailable);
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
    // A mutation whose reply never arrives has an unknown outcome; it is
    // neither refused nor retryable under a new key.
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
    assert_eq!(
        (status, code_of(&reply).as_str()),
        (503, "service_request_outcome_unknown")
    );
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

/// The index the fake catalog check grants.
const GRANTED_INDEX: &str = "idx_01010101010101010101010101010101";

/// A plane whose dispatch records every method and answers from `script`
/// (`None` is a lost reply), and whose catalog check grants exactly
/// `GRANTED_INDEX` at the revision `catalog` holds, with identity `[0x5c; 32]`.
fn scripted_plane(
    script: Vec<Option<ControlResponse>>,
) -> (AdminPlane, Arc<Mutex<Vec<Value>>>, Arc<AtomicU64>) {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let script = Arc::new(Mutex::new(VecDeque::from(script)));
    let log = sent.clone();
    let dispatch: Dispatch = Arc::new(move |method: ControlMethod| {
        log.lock()
            .unwrap()
            .push(serde_json::to_value(&method).unwrap());
        let answer = script.lock().unwrap().pop_front().flatten();
        Box::pin(async move { answer.ok_or_else(|| anyhow::anyhow!("reply lost")) })
    });
    let catalog = Arc::new(AtomicU64::new(12));
    let current = catalog.clone();
    let check: ScopeCheck = Arc::new(move |selection: WindowScopeSelection| {
        let revision = current.load(Ordering::SeqCst);
        Box::pin(async move {
            let granted = selection.index_ids.len() == 1
                && selection.index_ids[0].to_string() == GRANTED_INDEX
                && selection.expected_catalog_revision.get() == revision;
            if granted {
                Ok((
                    Sha256Digest::from_bytes([0x5c; 32]),
                    selection.expected_catalog_revision,
                ))
            } else {
                Err("scoped_read_unavailable".to_owned())
            }
        })
    });
    let counter = Arc::new(AtomicU8::new(0));
    let plane = AdminPlane::with_sources(
        dispatch,
        Incarnation::from_bytes([1; 16]),
        Arc::new(|| 0),
        Arc::new(move || Some([counter.fetch_add(1, Ordering::SeqCst) + 1; 32])),
    )
    .with_scope_check(check);
    (plane, sent, catalog)
}

fn api_call(cookie: &str, csrf: &str, operation: &str, body: &str) -> Request<Full<Bytes>> {
    let cookie = format!("{COOKIE_NAME}={cookie}");
    request(
        "POST",
        &format!("/admin/api/v1/call/{operation}"),
        &[
            HOST,
            ORIGIN,
            SAME_ORIGIN,
            JSON,
            ("cookie", cookie.as_str()),
            ("x-airk-csrf", csrf),
        ],
        body,
    )
}

async fn envelope(plane: &AdminPlane, request: Request<Full<Bytes>>) -> (u16, Value) {
    // Existing workflow cases retain their semantic assertions while speaking
    // the explicit R17 protocol. Raw initial-ack/once-only observations live in
    // preparation.rs and call handle directly without this helper.
    let operation = request
        .uri()
        .path()
        .strip_prefix("/admin/api/v1/call/")
        .map(str::to_owned);
    let cookie = request
        .headers()
        .get("cookie")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix(&format!("{COOKIE_NAME}=")))
        .map(str::to_owned);
    let csrf = request
        .headers()
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let (status, _, body) = answer(plane, request, PeerProof::SameUser).await;
    let reply: Value = serde_json::from_slice(&body).unwrap();
    if status == 200
        && operation.as_deref().is_some_and(|operation| {
            matches!(
                operation,
                "jobs.retry"
                    | "catalog.policy_resume"
                    | "catalog.split_recover"
                    | "recovery.plan_create"
                    | "service.stop"
                    | "service.restart"
            )
        })
        && let Ok(ack) =
            serde_json::from_value::<contract::MutationPreparation>(reply["result"].clone())
    {
        let cookie = cookie.expect("workflow has admitted cookie");
        let csrf = csrf.expect("workflow has admitted CSRF");
        let operation = operation.unwrap();
        let retained = ack.preparation;
        let (status, _, bytes) = answer(
            plane,
            api_call(
                &cookie,
                &csrf,
                &operation,
                &json!({"prepared":retained}).to_string(),
            ),
            PeerProof::SameUser,
        )
        .await;
        let retained_reply: Value = serde_json::from_slice(&bytes).unwrap();
        if retained_reply["ok"] == true || retained_reply["error"]["class"] == "refused" {
            let (ack_status, _, ack_bytes) = answer(
                plane,
                api_call(
                    &cookie,
                    &csrf,
                    &operation,
                    &json!({"acknowledged":retained}).to_string(),
                ),
                PeerProof::SameUser,
            )
            .await;
            assert_eq!(
                ack_status,
                200,
                "explicit client retention acknowledgment: {}",
                String::from_utf8_lossy(&ack_bytes)
            );
        }
        return (status, retained_reply);
    }
    (status, reply)
}

/// The 32-slot custody test deliberately uses the post-admission call seam:
/// the separate HTTP budget is 16 and has its own actual-handler regression.
async fn custody_envelope(
    plane: &AdminPlane,
    cookie: &str,
    operation: &str,
    body: Value,
) -> (u16, Value) {
    let response = plane
        .call(
            operation,
            Some(cookie),
            Full::new(Bytes::from(body.to_string())),
        )
        .await
        .unwrap_or_else(refusal);
    let status = response.status().as_u16();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let reply: Value = serde_json::from_slice(&bytes).unwrap();
    if let Ok(ack) =
        serde_json::from_value::<contract::MutationPreparation>(reply["result"].clone())
    {
        let retained = ack.preparation;
        let response = plane
            .call(
                operation,
                Some(cookie),
                Full::new(Bytes::from(json!({"prepared":retained}).to_string())),
            )
            .await
            .unwrap_or_else(refusal);
        let status = response.status().as_u16();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        return (status, serde_json::from_slice(&bytes).unwrap());
    }
    (status, reply)
}

fn error_code(reply: &Value) -> String {
    reply["error"]["code"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn grant_body(index: &str, revision: u64) -> String {
    json!({"admin_scope_grant": {"selection": {
        "namespace": "0a".repeat(16),
        "expected_catalog_revision": revision,
        "index_ids": [index],
        "project_ids": ["prj_02020202020202020202020202020202"],
    }}})
    .to_string()
}

fn search_body() -> String {
    json!({"tools_call": {"tool": "airk_search", "arguments": {"query": "q"},
        "selection": "admin", "response_mode": "terse", "usage_client": "unknown"}})
    .to_string()
}

#[tokio::test]
async fn mutations_answer_with_a_server_generated_request() {
    let (plane, sent, _) = scripted_plane(vec![Some(ControlResponse::ClientErrorRecorded {})]);
    let (cookie, csrf) = sign_in(&plane).await;
    let pause = json!({"pause": {"project": "ab".repeat(32), "paused": true}});
    let (status, reply) = envelope(
        &plane,
        api_call(&cookie, &csrf, "indexes.pause", &pause.to_string()),
    )
    .await;
    assert_eq!(status, 200, "{reply}");
    let reply: SuccessEnvelope = serde_json::from_value(reply).unwrap();
    reply.validate().unwrap();
    let request = reply.request.unwrap();
    // The launch code, cookie and token drew 1-3; the request nonce is 4.
    assert_eq!(request.nonce, "04".repeat(16));
    assert_eq!(request.state, contract::RequestState::Completed);
    assert_eq!(*sent.lock().unwrap(), [pause]);
    // A browser never supplies a key: refused before anything is sent.
    let lifecycle = json!({"lifecycle": {"action": "stop", "caller": "c1".repeat(16)}});
    let retry = json!({"retry": {"id": "3b".repeat(32),
        "key": {"namespace": "0a".repeat(16), "epoch": 1, "nonce": "c2".repeat(16)}}});
    for (operation, body) in [("service.stop", lifecycle), ("jobs.retry", retry)] {
        let (status, reply) = envelope(
            &plane,
            api_call(&cookie, &csrf, operation, &body.to_string()),
        )
        .await;
        assert_eq!(
            (status, error_code(&reply).as_str()),
            (400, "admin_malformed_request")
        );
    }
    assert_eq!(sent.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn keyed_mutations_fill_the_key_from_the_service_epoch() {
    let (plane, sent, _) = scripted_plane(vec![
        Some(ControlResponse::RequestEpoch {
            namespace: Incarnation::from_bytes([0x0a; 16]),
            epoch: 7,
        }),
        // A refusal under the key: no request in the envelope.
        Some(ControlResponse::Failure {
            code: "lifecycle_request_in_progress".into(),
        }),
    ]);
    let (cookie, csrf) = sign_in(&plane).await;
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "service.stop",
            r#"{"lifecycle":{"action":"stop"}}"#,
        ),
    )
    .await;
    assert_eq!(status, 409, "{reply}");
    assert_eq!(error_code(&reply), "lifecycle_request_in_progress");
    let sent = sent.lock().unwrap();
    assert_eq!(sent[0], json!("request_epoch"));
    // Caller 4 and nonce 5 come from the service's random source.
    assert_eq!(
        sent[1],
        json!({"lifecycle": {"action": "stop", "caller": "04".repeat(16),
            "key": {"namespace": "0a".repeat(16), "epoch": 7, "nonce": "05".repeat(16)}}})
    );
}

#[tokio::test]
async fn the_admin_grant_belongs_to_its_session() {
    let (plane, sent, catalog) = scripted_plane(Vec::new());
    let (cookie, csrf) = sign_in(&plane).await;
    let search = |cookie: &str, csrf: &str| api_call(cookie, csrf, "search.call", &search_body());
    // No grant yet: the search is refused, typed.
    let (status, reply) = envelope(&plane, search(&cookie, &csrf)).await;
    assert_eq!(
        (status, error_code(&reply).as_str()),
        (409, "admin_scope_not_granted")
    );
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
    assert_eq!(
        reply["result"],
        json!({"status": "admin_scope_granted", "scope_identity": "5c".repeat(32),
            "catalog_revision": 12, "expires_at_ms": SESSION_IDLE_MS})
    );
    // A failed grant leaves the previous one.
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "search.select_scope",
            &grant_body("idx_0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f", 12),
        ),
    )
    .await;
    assert_eq!(
        (status, error_code(&reply).as_str()),
        (409, "scoped_read_unavailable")
    );
    // Granted: the read under it waits for the in-memory grant admission.
    let (status, reply) = envelope(&plane, search(&cookie, &csrf)).await;
    assert_eq!(
        (status, error_code(&reply).as_str()),
        (503, "not_yet_served")
    );
    // A catalog change since the grant makes it stale.
    catalog.store(13, Ordering::SeqCst);
    let (status, reply) = envelope(&plane, search(&cookie, &csrf)).await;
    assert_eq!(
        (status, error_code(&reply).as_str()),
        (409, "scoped_read_unavailable")
    );
    catalog.store(12, Ordering::SeqCst);
    // Another session has no grant.
    let (other_cookie, other_csrf) = sign_in(&plane).await;
    let (status, reply) = envelope(&plane, search(&other_cookie, &other_csrf)).await;
    assert_eq!(
        (status, error_code(&reply).as_str()),
        (409, "admin_scope_not_granted")
    );
    // The browser never names a project, catalog grant or window instead.
    let project = json!({"tools_call": {"tool": "airk_search", "arguments": {},
        "selection": {"project": {"binding": "ab".repeat(32)}},
        "response_mode": "terse", "usage_client": "unknown"}});
    let (status, _) = envelope(
        &plane,
        api_call(&cookie, &csrf, "search.call", &project.to_string()),
    )
    .await;
    assert_eq!(status, 400);
    // Logout drops the session and its grant. Nothing reached the pipe.
    let session_cookie = format!("{COOKIE_NAME}={cookie}");
    let logout = request(
        "POST",
        "/admin/api/v1/session/logout",
        &[
            HOST,
            ORIGIN,
            SAME_ORIGIN,
            JSON,
            ("cookie", session_cookie.as_str()),
            ("x-airk-csrf", csrf.as_str()),
        ],
        "{}",
    );
    assert_eq!(answer(&plane, logout, PeerProof::SameUser).await.0, 200);
    assert!(plane.secrets().sessions.grant(&cookie, 0).is_none());
    assert!(sent.lock().unwrap().is_empty());
}

#[test]
fn grants_end_with_their_session_and_never_cross_instances() {
    let instance = Incarnation::from_bytes([1; 16]);
    let mut sessions = secrets::Sessions::new(instance);
    let minted = sessions.create(&[1; 32], &[2; 32], 0);
    let grant = |instance| secrets::Grant {
        selection: WindowScopeSelection {
            namespace: Incarnation::from_bytes([0x0a; 16]),
            expected_catalog_revision: CatalogRevision::try_from(12).unwrap(),
            index_ids: vec![GRANTED_INDEX.parse().unwrap()],
            project_ids: Vec::new(),
        },
        scope_identity: Sha256Digest::from_bytes([0x5c; 32]),
        catalog_revision: CatalogRevision::try_from(12).unwrap(),
        instance,
        selection_ref: None,
    };
    // A grant checked by another instance is never stored.
    assert_eq!(
        sessions.set_grant(&minted.cookie, grant(Incarnation::from_bytes([2; 16])), 1),
        None
    );
    assert!(sessions.grant(&minted.cookie, 1).is_none());
    assert_eq!(
        sessions.set_grant(&minted.cookie, grant(instance), 1),
        Some(SESSION_IDLE_MS)
    );
    assert!(sessions.grant(&minted.cookie, 2).is_some());
    // Idle expiry ends the session and its grant.
    assert!(
        sessions
            .grant(&minted.cookie, 1 + SESSION_IDLE_MS)
            .is_none()
    );
    // An unknown session has nothing to bind to.
    let unknown = secrets::secret_text(&[9; 32]);
    assert_eq!(sessions.set_grant(&unknown, grant(instance), 2), None);
    // A restarted service (a new instance) knows no session or grant.
    let mut restarted = secrets::Sessions::new(Incarnation::from_bytes([2; 16]));
    assert!(restarted.grant(&minted.cookie, 2).is_none());
}

/// The reviewed mapping (contracts §4.3 amendment), written out by hand.
#[test]
fn rows_without_a_control_method_follow_the_reviewed_mapping() {
    use operations::{MAPPINGS, Served, served};
    let expected: [(&str, Served); 4] = [
        (
            "HttpConnectionInfo",
            Served::Equivalent("AgentConnectionInfo"),
        ),
        (
            "HttpConnectionOwnership",
            Served::Equivalent("AgentConnectionOwnership"),
        ),
        ("LogsExport", Served::Equivalent("Logs")),
        ("HttpTrust.Inspect", Served::TrustInspect),
    ];
    assert_eq!(MAPPINGS, expected);
    // A name is a `ControlMethod` variant when decoding it fails for a
    // reason other than an unknown variant.
    let is_variant = |method: &str| {
        let variant = operations::snake(method.split('.').next().unwrap());
        let mut object = serde_json::Map::new();
        object.insert(variant.clone(), json!({}));
        [Value::String(variant), Value::Object(object)]
            .into_iter()
            .any(|body| match serde_json::from_value::<ControlMethod>(body) {
                Ok(_) => true,
                Err(error) => !error.to_string().contains("unknown variant"),
            })
    };
    let table = operations::table().unwrap();
    for row in &table.operations {
        if !row.surfaces.contains(&contract::Surface::Admin) {
            continue;
        }
        match served(&row.method) {
            Served::Direct => assert!(is_variant(&row.method), "{}", row.operation),
            Served::Equivalent(method) => {
                assert!(!is_variant(&row.method), "{}", row.operation);
                assert!(is_variant(method), "{}", row.operation);
            }
            Served::TrustInspect => {
                assert!(!is_variant(&row.method), "{}", row.operation)
            }
        }
    }
}

#[tokio::test]
async fn mapped_rows_are_served_by_their_mapping() {
    let (plane, sent, _) = scripted_plane(vec![Some(ControlResponse::Failure {
        code: "agent_connection_unknown".into(),
    })]);
    let (cookie, csrf) = sign_in(&plane).await;
    // agents.connection is AgentConnectionInfo.
    let info = json!({"agent_connection_info": {"connection_id": "1c".repeat(16)}});
    let (status, _) = envelope(
        &plane,
        api_call(&cookie, &csrf, "agents.connection", &info.to_string()),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(*sent.lock().unwrap(), [info]);
    // Trust inspection takes exactly its body; this plane has no inspector.
    let (status, _) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "https.trust_status",
            r#"{"http_trust":{"action":"install"}}"#,
        ),
    )
    .await;
    assert_eq!(status, 400);
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "https.trust_status",
            r#"{"http_trust":{"action":"inspect"}}"#,
        ),
    )
    .await;
    assert_eq!(
        (status, error_code(&reply).as_str()),
        (503, "not_yet_served")
    );
    assert_eq!(sent.lock().unwrap().len(), 1);
}

fn epoch(epoch: u64) -> ControlResponse {
    ControlResponse::RequestEpoch {
        namespace: Incarnation::from_bytes([0x0a; 16]),
        epoch,
    }
}

const SET: &str = "prj_02020202020202020202020202020202";

#[tokio::test]
async fn reviews_keep_the_request_identity_in_the_session() {
    // The preview's and the receipt's native answers stand in as successes.
    let (plane, sent, _) = scripted_plane(vec![
        Some(epoch(7)),
        Some(epoch(1)),
        Some(epoch(2)),
        Some(epoch(3)),
    ]);
    let (cookie, csrf) = sign_in(&plane).await;
    let create = json!({"operation": "create_project", "id": SET, "name": "Docs"});
    let preview = json!({"catalog_preview": {"request": {
        "expected_catalog_revision": 3, "operation": create}}});
    let (status, reply) = envelope(
        &plane,
        api_call(&cookie, &csrf, "catalog.preview", &preview.to_string()),
    )
    .await;
    assert_eq!(status, 200, "{reply}");
    let reply: SuccessEnvelope = serde_json::from_value(reply).unwrap();
    reply.validate().unwrap();
    // The launch code, cookie and token drew 1-3; caller 4, nonce 5, token 6.
    assert_eq!(reply.review.as_deref(), Some("06".repeat(16).as_str()));
    let identity = json!({"caller": "04".repeat(16),
        "key": {"namespace": "0a".repeat(16), "epoch": 7, "nonce": "05".repeat(16)}});
    assert_eq!(
        sent.lock().unwrap()[1]["catalog_preview"]["request"]["key"],
        identity["key"]
    );
    // Apply under the token: the session's identity, nothing from the browser.
    let apply = json!({"method": {"catalog_apply": {"request": {
        "expected_catalog_revision": 3, "operation": create},
        "preview_digest": "3c".repeat(32)}}, "review": "06".repeat(16)});
    let (status, reply) = envelope(
        &plane,
        api_call(&cookie, &csrf, "catalog.apply", &apply.to_string()),
    )
    .await;
    assert_eq!(status, 200, "{reply}");
    let applied = sent.lock().unwrap()[2]["catalog_apply"]["request"].clone();
    assert_eq!(applied["caller"], identity["caller"]);
    assert_eq!(applied["key"], identity["key"]);
    // The receipt reads the same identity.
    let receipt = json!({"method": {"catalog_receipt": {}}, "review": "06".repeat(16)});
    let (status, _) = envelope(
        &plane,
        api_call(&cookie, &csrf, "catalog.receipt", &receipt.to_string()),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        sent.lock().unwrap()[3],
        json!({"catalog_receipt": identity})
    );
    // Refused before anything is sent: an unknown or missing token, a
    // browser-named key, a bare body on a review row.
    let unknown = json!({"method": {"catalog_receipt": {}}, "review": "07".repeat(16)});
    let missing = json!({"method": {"catalog_receipt": {}}});
    let named = json!({"method": {"catalog_receipt": {"caller": "04".repeat(16)}},
        "review": "06".repeat(16)});
    for (body, code) in [
        (unknown, "admin_review_unknown"),
        (missing, "admin_review_unknown"),
        (named, "admin_malformed_request"),
        (json!({"catalog_receipt": {}}), "admin_malformed_request"),
    ] {
        let (status, reply) = envelope(
            &plane,
            api_call(&cookie, &csrf, "catalog.receipt", &body.to_string()),
        )
        .await;
        assert_eq!((status, error_code(&reply).as_str()), (400, code), "{body}");
    }
    assert_eq!(sent.lock().unwrap().len(), 4);
    // Another session never sees this session's token.
    let (other_cookie, other_csrf) = sign_in(&plane).await;
    let receipt = json!({"method": {"catalog_receipt": {}}, "review": "06".repeat(16)});
    let (status, reply) = envelope(
        &plane,
        api_call(
            &other_cookie,
            &other_csrf,
            "catalog.receipt",
            &receipt.to_string(),
        ),
    )
    .await;
    assert_eq!(
        (status, error_code(&reply).as_str()),
        (400, "admin_review_unknown")
    );
    assert_eq!(sent.lock().unwrap().len(), 4);
}

#[tokio::test]
async fn destructive_calls_need_the_typed_target_name() {
    let (plane, sent, _) = scripted_plane(vec![
        Some(ControlResponse::ClientErrorRecorded {}),
        // The delete's preview (epoch, answer), then its target read: the
        // catalog no longer finds the index set, so nothing can be confirmed.
        Some(epoch(7)),
        Some(epoch(1)),
        Some(ControlResponse::Failure {
            code: "catalog_entity_unknown".into(),
        }),
    ]);
    let (cookie, csrf) = sign_in(&plane).await;
    let id = "3b".repeat(32);
    let execute = |text: Option<&str>| {
        let mut body = json!({"method": {"plan_execute": {"id": id, "preview": "3c".repeat(32),
            "allow_fallback": false}}});
        if let Some(text) = text {
            body["confirm_text"] = json!(text);
        }
        body.to_string()
    };
    let long = "x".repeat(257);
    for (text, status, code) in [
        (None, 400, "admin_confirmation_mismatch"),
        (Some("3b3b3b3b3b3c"), 400, "admin_confirmation_mismatch"),
        (Some(long.as_str()), 400, "admin_malformed_request"),
    ] {
        let (got, reply) = envelope(
            &plane,
            api_call(&cookie, &csrf, "recovery.plan_execute", &execute(text)),
        )
        .await;
        assert_eq!((got, error_code(&reply).as_str()), (status, code));
    }
    assert!(sent.lock().unwrap().is_empty());
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "recovery.plan_execute",
            &execute(Some("3b3b3b3b3b3b")),
        ),
    )
    .await;
    assert_eq!(status, 200, "{reply}");
    assert_eq!(sent.lock().unwrap().len(), 1);
    // A catalog delete reads the target first; a missing target refuses.
    let operation = json!({"operation": "delete_project", "id": SET,
        "expected_definition_revision": 1, "expected_scope_revision": 1,
        "expected_defaults_revision": 1});
    let preview = json!({"catalog_preview": {"request": {
        "expected_catalog_revision": 3, "operation": operation}}});
    let (status, reply) = envelope(
        &plane,
        api_call(&cookie, &csrf, "catalog.preview", &preview.to_string()),
    )
    .await;
    assert_eq!(status, 200, "{reply}");
    // Plan execution drew nonce 4; the preview caller 5, nonce 6, token 7.
    assert_eq!(reply["review"], json!("07".repeat(16)));
    let delete = json!({"method": {"catalog_apply": {"request": {
        "expected_catalog_revision": 3, "operation": operation},
        "preview_digest": "3c".repeat(32)}}, "review": "07".repeat(16), "confirm_text": "Docs"});
    let (status, reply) = envelope(
        &plane,
        api_call(&cookie, &csrf, "catalog.apply", &delete.to_string()),
    )
    .await;
    assert_eq!(
        (status, error_code(&reply).as_str()),
        (400, "admin_confirmation_mismatch")
    );
    let sent = sent.lock().unwrap();
    assert_eq!(sent.len(), 4);
    assert_eq!(
        sent[3],
        json!({"catalog_detail": {"entity": {"kind": "project", "id": SET},
            "expected_catalog_revision": 3}})
    );
}

#[test]
fn redaction_removes_only_the_issued_identity() {
    let key = json!({"namespace": "0a".repeat(16), "epoch": 7, "nonce": "05".repeat(16)});
    let other = json!({"namespace": "0a".repeat(16), "epoch": 7, "nonce": "09".repeat(16)});
    let mut value = json!({"draft": {"request": {"caller": "04".repeat(16), "key": key,
        "name": "Docs"}, "review_digest": "3c".repeat(32)},
        "history": [{"caller": "08".repeat(16), "key": other}]});
    let caller = mutations::redact(&mut value, &key);
    assert_eq!(caller, Some(json!("04".repeat(16))));
    assert_eq!(
        value,
        json!({"draft": {"request": {"name": "Docs"}, "review_digest": "3c".repeat(32)},
            "history": [{"caller": "08".repeat(16), "key": other}]})
    );
}

const NATIVE_OUTCOMES: &str = include_str!("../../../../fixtures/split/i3/native_outcomes.json");

fn r04_fixture() -> Value {
    let fixture: Value = serde_json::from_str(NATIVE_OUTCOMES).unwrap();
    assert_eq!(fixture["schema"], "airk.split.admin.native-outcomes/1");
    fixture
}

fn r04_case(name: &str) -> Value {
    r04_fixture()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap_or_else(|| panic!("missing reviewed native outcome {name}"))
        .clone()
}

fn r04_response(value: &Value) -> ControlResponse {
    let response: ControlResponse = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&response).unwrap(), *value);
    response
}

fn r04_assert_outcome(status: u16, reply: &Value, expected: &Value) {
    assert_eq!(status, expected["http"].as_u64().unwrap() as u16, "{reply}");
    assert_eq!(reply["ok"], expected["ok"], "{reply}");
    assert!(
        reply.get("review").is_none(),
        "an outcome is not a new review: {reply}"
    );
    if expected["ok"] == true {
        assert_eq!(reply["result"], expected["native"]);
        assert_eq!(reply["request"]["state"], expected["request_state"]);
        assert_eq!(reply["request"]["nonce"], expected["request_nonce"]);
        let decoded: SuccessEnvelope = serde_json::from_value(reply.clone()).unwrap();
        decoded.validate().unwrap();
    } else {
        assert!(
            reply.get("request").is_none(),
            "unknown/refused never completes: {reply}"
        );
        assert!(reply.get("result").is_none());
        assert_eq!(reply["error"]["code"], expected["code"]);
        assert_eq!(reply["error"]["class"], expected["class"]);
        assert_eq!(reply["error"]["retryable"], expected["retryable"]);
        assert_eq!(reply["error"]["native"], expected["native"]);
        let decoded: FailureEnvelope = serde_json::from_value(reply.clone()).unwrap();
        decoded.validate().unwrap();
    }
}

async fn r04_through_handler(name: &str) {
    let fixture = r04_fixture();
    let case = r04_case(name);
    let reviewed = case["route"] == "review";
    let mut script = Vec::new();
    if reviewed {
        script.push(Some(epoch(7)));
        script.push(Some(r04_response(&fixture["review"]["response"])));
    }
    script.push(Some(r04_response(&case["response"])));
    let (plane, sent, _) = scripted_plane(script);
    let (cookie, csrf) = sign_in(&plane).await;
    if reviewed {
        let (status, preview) = envelope(
            &plane,
            api_call(
                &cookie,
                &csrf,
                "catalog.preview",
                &fixture["review"]["body"].to_string(),
            ),
        )
        .await;
        assert_eq!(status, 200, "{preview}");
        assert_eq!(preview["review"], "06".repeat(16));
    }
    let body = if reviewed {
        &fixture["review"]["apply"]
    } else {
        &case["body"]
    };
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            case["operation"].as_str().unwrap(),
            &body.to_string(),
        ),
    )
    .await;
    if reviewed {
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 3, "no replay of a native terminal answer");
        assert_eq!(
            sent[2]["catalog_apply"]["request"]["caller"],
            "04".repeat(16)
        );
        assert_eq!(
            sent[2]["catalog_apply"]["request"]["key"],
            json!({"namespace": "0a".repeat(16), "epoch": 7, "nonce": "05".repeat(16)})
        );
    } else {
        assert_eq!(*sent.lock().unwrap(), [case["body"].clone()]);
    }
    r04_assert_outcome(status, &reply, &case["expect"]);
}

#[test]
fn r04_native_outcome_fixture_has_the_independent_case_roster() {
    let mut fixture = r04_fixture();
    // Opt-in fixture emission uses the owning Rust identity primitive and
    // native request serialization. It does not derive resolution/redaction
    // expectations from the adapter under test or qualify persisted lineage.
    if std::env::var_os("AIRK_WRITE_ADMIN_NATIVE_OUTCOMES").is_some_and(|value| value == "1") {
        for case in fixture["recovery_receipts"].as_array_mut().unwrap() {
            let native: ControlResponse = serde_json::from_value(case["response"].clone()).unwrap();
            if let ControlResponse::CatalogSplitRecoveryReceipt {
                receipt: Some(receipt),
                ..
            } = native
            {
                let digest = airk_core::identity::framed_digest(
                    b"airk/catalog-split-recovery/1",
                    &[&serde_json::to_vec(&receipt.request).unwrap()],
                )
                .to_string();
                case["response"]["receipt"]["request_digest"] = json!(digest);
                case["expect"]["native"]["receipt"]["request_digest"] = json!(digest);
            }
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/split/i3/native_outcomes.json");
        std::fs::write(path, serde_json::to_string_pretty(&fixture).unwrap() + "\n").unwrap();
    }
    let names: Vec<&str> = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "catalog_unknown",
            "source_resolution_unknown",
            "source_change_busy",
            "job_accepted",
            "catalog_completed"
        ]
    );
    let receipt_cases = fixture["recovery_receipts"].as_array().unwrap();
    let names: Vec<&str> = receipt_cases
        .iter()
        .map(|case| case["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "policy_known_accepted",
            "policy_receipt_missing",
            "split_already_complete",
            "split_still_unknown",
            "split_receipt_missing"
        ]
    );
    let identity = secrets::Review {
        token: "5f".repeat(16).parse().unwrap(),
        caller: Some(json!("04".repeat(16))),
        key: json!({"namespace": "0a".repeat(16), "epoch": 7, "nonce": "05".repeat(16)}),
        purpose: secrets::ReviewPurpose::Ordinary,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    for case in receipt_cases {
        let response = r04_response(&case["response"]);
        match &response {
            ControlResponse::CatalogPolicyResumeReceipt {
                index_id,
                operation_id,
                receipt: Some(receipt),
                ..
            } => {
                receipt.configuration.request.validate().unwrap();
                assert_eq!(receipt.job.kind, airk_core::service::JobKind::Configure);
                let airk_core::service_configuration::ProjectConfiguration::ApplySourcePolicy {
                    policy,
                    operation_id: accepted_operation,
                } = &receipt.configuration.request.configuration
                else {
                    panic!("policy resume fixture must retain its actual native action");
                };
                assert_eq!(policy.index_id, *index_id);
                assert_eq!(*accepted_operation, *operation_id);
                assert_eq!(receipt.job.project, policy.owner_binding);
            }
            ControlResponse::CatalogSplitRecoveryReceipt {
                operation_id,
                caller,
                key,
                receipt: Some(receipt),
                ..
            } => {
                assert_eq!(
                    receipt.request_digest,
                    airk_core::identity::framed_digest(
                        b"airk/catalog-split-recovery/1",
                        &[&serde_json::to_vec(&receipt.request).unwrap()]
                    )
                );
                let held = &case["request"]["catalog_split_recover"]["request"];
                assert_eq!(receipt.request.operation_id, *operation_id);
                assert_eq!(receipt.request.caller, *caller);
                assert_eq!(receipt.request.key, *key);
                assert_eq!(
                    serde_json::to_value(receipt.request.operation_id).unwrap(),
                    held["operation_id"]
                );
                assert_eq!(
                    serde_json::to_value(receipt.request.request_digest).unwrap(),
                    held["request_digest"]
                );
                assert_eq!(
                    serde_json::to_value(receipt.request.expected_receipt_digest).unwrap(),
                    held["expected_receipt_digest"]
                );
                assert_eq!(
                    serde_json::to_value(receipt.request.action).unwrap(),
                    held["action"]
                );
                assert_eq!(receipt.request.expected_finalization_revision, None);
                if let airk_core::catalog_split::SplitRecoveryOutcome::Unknown { caller, key } =
                    &receipt.outcome
                {
                    assert_eq!(*caller, receipt.request.caller);
                    assert_eq!(*key, receipt.request.key);
                }
                if let airk_core::catalog_split::SplitRecoveryOutcome::AlreadyComplete {
                    receipt_digest,
                } = &receipt.outcome
                {
                    assert_eq!(*receipt_digest, receipt.request.expected_receipt_digest);
                }
            }
            ControlResponse::CatalogPolicyResumeReceipt { receipt: None, .. }
            | ControlResponse::CatalogSplitRecoveryReceipt { receipt: None, .. } => {}
            _ => panic!("unexpected recovery receipt fixture {}", case["name"]),
        }
        let result = native_request(
            case["operation"].as_str().unwrap(),
            response,
            None,
            Some(&identity),
        );
        assert_eq!(result.status().as_u16(), 200);
        let body = runtime
            .block_on(result.into_body().collect())
            .unwrap()
            .to_bytes();
        let envelope: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            envelope["result"], case["expect"]["native"],
            "{}",
            case["name"]
        );
        assert!(envelope.get("request").is_none());
        assert!(envelope.get("review").is_none());
        assert!(case["expect"]["resolved"].is_boolean());
    }
}

#[tokio::test]
async fn r04_catalog_unknown_is_not_a_completed_admin_mutation() {
    r04_through_handler("catalog_unknown").await;
}

#[tokio::test]
async fn r04_source_resolution_unknown_is_not_a_completed_admin_mutation() {
    r04_through_handler("source_resolution_unknown").await;
}

#[tokio::test]
async fn r04_source_change_busy_is_not_a_completed_admin_mutation() {
    r04_through_handler("source_change_busy").await;
}

#[tokio::test]
async fn r04_pending_job_keeps_accepted_admin_progress() {
    r04_through_handler("job_accepted").await;
}

#[tokio::test]
async fn r04_completed_catalog_mutation_redacts_the_restored_review_identity() {
    r04_through_handler("catalog_completed").await;
}

#[tokio::test]
async fn r04_generated_unknown_redacts_identity_without_issuing_a_review() {
    let fixture = r04_fixture();
    let case = r04_case("catalog_unknown");
    let (plane, sent, _) =
        scripted_plane(vec![Some(epoch(7)), Some(r04_response(&case["response"]))]);
    let (cookie, csrf) = sign_in(&plane).await;
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "catalog.preview",
            &fixture["review"]["body"].to_string(),
        ),
    )
    .await;
    assert_eq!(sent.lock().unwrap().len(), 2);
    r04_assert_outcome(status, &reply, &case["expect"]);
}

#[tokio::test]
async fn r04_unknown_apply_keeps_the_existing_receipt_identity_without_replay() {
    let fixture = r04_fixture();
    let case = r04_case("catalog_unknown");
    let (plane, sent, _) = scripted_plane(vec![
        Some(epoch(7)),
        Some(r04_response(&fixture["review"]["response"])),
        Some(r04_response(&case["response"])),
        Some(ControlResponse::CatalogReceipt {
            receipt: None,
            fence: None,
        }),
    ]);
    let (cookie, csrf) = sign_in(&plane).await;
    let (status, preview) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "catalog.preview",
            &fixture["review"]["body"].to_string(),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(preview["review"], "06".repeat(16));
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "catalog.apply",
            &fixture["review"]["apply"].to_string(),
        ),
    )
    .await;
    r04_assert_outcome(status, &reply, &case["expect"]);
    let receipt = json!({"method": {"catalog_receipt": {}}, "review": "06".repeat(16)});
    let (status, reply) = envelope(
        &plane,
        api_call(&cookie, &csrf, "catalog.receipt", &receipt.to_string()),
    )
    .await;
    assert_eq!(status, 200, "{reply}");
    assert_eq!(
        reply["result"],
        json!({"status": "catalog_receipt", "receipt": null})
    );
    assert_eq!(
        sent.lock().unwrap().len(),
        4,
        "receipt lookup does not replay apply"
    );
    assert_eq!(
        sent.lock().unwrap()[3],
        json!({"catalog_receipt": {
            "caller": "04".repeat(16),
            "key": {"namespace": "0a".repeat(16), "epoch": 7, "nonce": "05".repeat(16)}
        }})
    );
}

fn r04_recovery_body(split: bool) -> Value {
    if split {
        json!({"catalog_split_recover": {"request": {
            "operation_id": "2b".repeat(16), "request_digest": "3c".repeat(32),
            "expected_receipt_digest": "3d".repeat(32), "action": "resume"
        }}})
    } else {
        json!({"catalog_policy_resume": {"index_id": GRANTED_INDEX, "operation_id": "2b".repeat(16)}})
    }
}

fn r04_recovery_operation(split: bool) -> &'static str {
    if split {
        "catalog.split_recover"
    } else {
        "catalog.policy_resume"
    }
}

fn r04_recovery_read(split: bool, token: &str) -> Value {
    let method = if split {
        json!({"catalog_split_recovery_receipt": {"operation_id": "2b".repeat(16)}})
    } else {
        json!({"catalog_policy_resume_receipt": {"index_id": GRANTED_INDEX, "operation_id": "2b".repeat(16)}})
    };
    json!({"method": method, "review": token})
}

fn r04_unknown_metadata() -> ControlResponse {
    // finalize_catalog_response emits this actual native answer when an
    // accepted catalog mutation cannot finish live reconciliation.
    ControlResponse::Failure {
        code: "catalog_metadata_saved_outcome_unknown".into(),
    }
}

fn r04_recovery_reply(split: bool) -> ControlResponse {
    let key = json!({"namespace": "0a".repeat(16), "epoch": 7, "nonce": "05".repeat(16)});
    let body = if split {
        json!({"status": "catalog_split_recovery_receipt", "operation_id": "2b".repeat(16),
            "caller": "04".repeat(16), "key": key, "expired": false, "receipt": null, "snapshot": null})
    } else {
        json!({"status": "catalog_policy_resume_receipt", "index_id": GRANTED_INDEX,
            "operation_id": "2b".repeat(16), "caller": "04".repeat(16), "key": key,
            "expired": false, "receipt": null})
    };
    r04_response(&body)
}

fn r04_known_recovery_reply(split: bool) -> ControlResponse {
    let fixture = r04_fixture();
    let name = if split {
        "split_already_complete"
    } else {
        "policy_known_accepted"
    };
    r04_response(
        &fixture["recovery_receipts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap()["response"],
    )
}

fn r04_policy_success() -> ControlResponse {
    let value = json!({"status": "catalog_policy_resumed", "index_id": GRANTED_INDEX,
        "accepted_job_id": "3b".repeat(32), "status_basis": "current_policy",
        "snapshot": {"schema": 1, "operation_id": "2b".repeat(16),
            "policy": {"schema": "airk.source-policy/1", "index_id": GRANTED_INDEX,
                "owner_binding": "ab".repeat(32), "namespace": "collections-1.4", "collection": "docs", "epoch": 1,
                "roots": [{"id": "src_01010101010101010101010101010101", "canonical_path": "/fiction/docs", "aliases": [], "owner": true}],
                "positive_includes": null, "denies": []},
            "job": "3b".repeat(32), "application": null, "application_job": null, "last_error": null},
        "job": {"id": "3b".repeat(32), "project": "ab".repeat(32), "kind": "configure", "state": "pending",
            "fencing_generation": 0, "attempts": 0, "outcome": null}});
    let response = r04_response(&value);
    let ControlResponse::CatalogPolicyResumed { snapshot, .. } = &response else {
        unreachable!()
    };
    snapshot.validate().unwrap();
    response
}

fn r04_recovery_plane(
    script: Vec<Option<ControlResponse>>,
) -> (AdminPlane, Arc<Mutex<Vec<Value>>>, Arc<AtomicU64>) {
    let (mut plane, sent, _) = scripted_plane(script);
    let clock = Arc::new(AtomicU64::new(0));
    let observed = clock.clone();
    plane.clock = Arc::new(move || observed.load(Ordering::SeqCst));
    (plane, sent, clock)
}

fn r04_handle(status: u16, reply: &Value, native: bool) -> String {
    assert_eq!(status, 503, "{reply}");
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["error"]["class"], "unavailable");
    assert_eq!(reply["error"]["retryable"], false);
    assert!(reply.get("request").is_none());
    assert!(reply.get("result").is_none());
    if native {
        assert_eq!(
            reply["error"]["code"],
            "catalog_metadata_saved_outcome_unknown"
        );
        assert_eq!(
            reply["error"]["native"],
            json!({"status": "failure", "code": "catalog_metadata_saved_outcome_unknown"})
        );
    } else {
        assert_eq!(reply["error"]["code"], "service_request_outcome_unknown");
        assert!(
            reply["error"].get("native").is_none(),
            "lost replies invent no native evidence"
        );
    }
    let token = reply["review"]
        .as_str()
        .expect("unknown generated mutation retains its receipt identity");
    assert_eq!(token.len(), 32);
    let decoded: FailureEnvelope = serde_json::from_value(reply.clone()).unwrap();
    decoded.validate().unwrap();
    token.to_owned()
}

async fn r04_unknown_generated_pair(split: bool, lost: bool) {
    let mut script = vec![Some(epoch(7))];
    if lost {
        script.extend([None, None]);
    } else {
        script.push(Some(r04_unknown_metadata()));
    }
    script.push(Some(r04_known_recovery_reply(split)));
    let (plane, sent, _) = r04_recovery_plane(script);
    let (cookie, csrf) = sign_in(&plane).await;
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            r04_recovery_operation(split),
            &r04_recovery_body(split).to_string(),
        ),
    )
    .await;
    let token = r04_handle(status, &reply, !lost);
    assert_eq!(plane.secrets().sessions.review_count(&cookie), 1);
    let sent_before = sent.lock().unwrap().len();
    if lost {
        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 3);
        assert_eq!(
            sent[1], sent[2],
            "native resend retains the exact original identity"
        );
    }
    let other_receipt = json!({"recover":token});
    let other_operation = r04_recovery_operation(!split);
    let (status, reply) = envelope(
        &plane,
        api_call(&cookie, &csrf, other_operation, &other_receipt.to_string()),
    )
    .await;
    assert_eq!(
        (status, error_code(&reply).as_str()),
        (400, "admin_review_unknown")
    );
    let mut mutation = r04_fixture()["review"]["apply"].clone();
    mutation["review"] = json!(token);
    let (status, reply) = envelope(
        &plane,
        api_call(&cookie, &csrf, "catalog.apply", &mutation.to_string()),
    )
    .await;
    assert_eq!(
        (status, error_code(&reply).as_str()),
        (400, "admin_review_unknown")
    );
    let (foreign_cookie, foreign_csrf) = sign_in(&plane).await;
    let receipt = json!({"recover":token});
    let receipt_operation = r04_recovery_operation(split);
    let (status, reply) = envelope(
        &plane,
        api_call(
            &foreign_cookie,
            &foreign_csrf,
            receipt_operation,
            &receipt.to_string(),
        ),
    )
    .await;
    assert_eq!(
        (status, error_code(&reply).as_str()),
        (400, "admin_review_unknown")
    );
    assert_eq!(
        sent.lock().unwrap().len(),
        sent_before,
        "invalid custody restores nothing to native dispatch"
    );
    let (status, reply) = envelope(
        &plane,
        api_call(&cookie, &csrf, receipt_operation, &receipt.to_string()),
    )
    .await;
    assert_eq!(status, 200, "{reply}");
    assert_eq!(reply["result"]["operation_id"], "2b".repeat(16));
    assert_eq!(reply["result"]["expired"], false);
    assert!(reply["result"].get("caller").is_none());
    assert!(reply["result"].get("key").is_none());
    assert!(
        reply.get("review").is_none(),
        "a receipt lookup issues no new mutation authority"
    );
    let sent = sent.lock().unwrap();
    assert_eq!(
        sent.len(),
        sent_before + 1,
        "only the receipt was sent after the unknown outcome"
    );
    let variant = if split {
        "catalog_split_recovery_receipt"
    } else {
        "catalog_policy_resume_receipt"
    };
    assert_eq!(sent.last().unwrap()[variant]["caller"], "04".repeat(16));
    assert_eq!(
        sent.last().unwrap()[variant]["key"],
        json!({"namespace": "0a".repeat(16), "epoch": 7, "nonce": "05".repeat(16)})
    );
}

#[tokio::test]
async fn r04_policy_native_unknown_can_only_read_its_original_receipt() {
    r04_unknown_generated_pair(false, false).await;
}

#[tokio::test]
async fn r04_split_native_unknown_can_only_read_its_original_receipt() {
    r04_unknown_generated_pair(true, false).await;
}

#[tokio::test]
async fn r04_policy_double_lost_replies_keep_the_original_receipt_identity() {
    r04_unknown_generated_pair(false, true).await;
}

#[tokio::test]
async fn r04_split_double_lost_replies_keep_the_original_receipt_identity() {
    r04_unknown_generated_pair(true, true).await;
}

#[tokio::test]
async fn r04_recovery_capacity_refuses_before_another_generated_effect() {
    let mut script = Vec::new();
    for _ in 0..=contract::MAX_REVIEWS_PER_SESSION {
        script.extend([Some(epoch(7)), Some(r04_unknown_metadata())]);
    }
    let (plane, sent, _) = r04_recovery_plane(script);
    let (cookie, csrf) = sign_in(&plane).await;
    for _ in 0..contract::MAX_REVIEWS_PER_SESSION {
        let (status, reply) = envelope(
            &plane,
            api_call(
                &cookie,
                &csrf,
                "catalog.policy_resume",
                &r04_recovery_body(false).to_string(),
            ),
        )
        .await;
        r04_handle(status, &reply, true);
    }
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "catalog.policy_resume",
            &r04_recovery_body(false).to_string(),
        ),
    )
    .await;
    assert_eq!(
        (status, error_code(&reply).as_str()),
        (409, "admin_review_capacity")
    );
    assert!(reply.get("review").is_none());
    assert_eq!(
        plane.secrets().sessions.review_count(&cookie),
        contract::MAX_REVIEWS_PER_SESSION
    );
    assert_eq!(
        sent.lock()
            .unwrap()
            .iter()
            .filter(|method| method.get("catalog_policy_resume").is_some())
            .count(),
        contract::MAX_REVIEWS_PER_SESSION
    );
}

#[tokio::test]
async fn r04_competing_pending_reservations_refuse_before_the_extra_effect() {
    let effects = Arc::new(AtomicU64::new(0));
    let observed = effects.clone();
    let (entered, mut entered_rx) = tokio::sync::mpsc::unbounded_channel();
    let (release, waiting) = tokio::sync::watch::channel(false);
    let dispatch: Dispatch = Arc::new(move |method| {
        if matches!(method, ControlMethod::RequestEpoch) {
            return Box::pin(async { Ok(epoch(7)) });
        }
        assert!(matches!(method, ControlMethod::CatalogPolicyResume { .. }));
        let n = observed.fetch_add(1, Ordering::SeqCst);
        let mut waiting = waiting.clone();
        let entered = entered.clone();
        Box::pin(async move {
            if n < contract::MAX_REVIEWS_PER_SESSION as u64 {
                entered.send(()).unwrap();
                waiting.wait_for(|released| *released).await.unwrap();
            }
            Ok(r04_unknown_metadata())
        })
    });
    let sequence = Arc::new(AtomicU8::new(0));
    let plane = Arc::new(AdminPlane::with_sources(
        dispatch,
        Incarnation::from_bytes([1; 16]),
        Arc::new(|| 0),
        Arc::new(move || Some([sequence.fetch_add(1, Ordering::SeqCst) + 1; 32])),
    ));
    let (cookie, _) = sign_in(&plane).await;
    let mut tasks = Vec::new();
    for _ in 0..contract::MAX_REVIEWS_PER_SESSION {
        let plane = plane.clone();
        let cookie = cookie.clone();
        tasks.push(tokio::spawn(async move {
            custody_envelope(
                &plane,
                &cookie,
                "catalog.policy_resume",
                r04_recovery_body(false),
            )
            .await
        }));
    }
    for _ in 0..contract::MAX_REVIEWS_PER_SESSION {
        tokio::time::timeout(std::time::Duration::from_secs(5), entered_rx.recv())
            .await
            .unwrap()
            .unwrap();
    }
    let held = plane.secrets().sessions.review_count(&cookie);
    let extra = custody_envelope(
        &plane,
        &cookie,
        "catalog.policy_resume",
        r04_recovery_body(false),
    )
    .await;
    release.send(true).unwrap();
    let mut replies = Vec::new();
    for task in tasks {
        replies.push(task.await.unwrap());
    }
    assert_eq!(
        held,
        contract::MAX_REVIEWS_PER_SESSION,
        "each in-flight generated call already owns its custody slot"
    );
    assert_eq!(
        effects.load(Ordering::SeqCst),
        contract::MAX_REVIEWS_PER_SESSION as u64
    );
    assert_eq!(
        (extra.0, error_code(&extra.1).as_str()),
        (409, "admin_review_capacity")
    );
    for (status, reply) in replies {
        r04_handle(status, &reply, true);
    }
}

#[tokio::test]
async fn r04_ordinary_preview_pressure_never_evicts_pinned_recovery() {
    let fixture = r04_fixture();
    let mut script = vec![Some(epoch(7)), Some(r04_unknown_metadata())];
    for _ in 0..=contract::MAX_REVIEWS_PER_SESSION {
        script.extend([
            Some(epoch(7)),
            Some(r04_response(&fixture["review"]["response"])),
        ]);
    }
    script.push(Some(r04_known_recovery_reply(false)));
    let (plane, sent, _) = r04_recovery_plane(script);
    let (cookie, csrf) = sign_in(&plane).await;
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "catalog.policy_resume",
            &r04_recovery_body(false).to_string(),
        ),
    )
    .await;
    let token = r04_handle(status, &reply, true);
    for _ in 0..=contract::MAX_REVIEWS_PER_SESSION {
        let (status, preview) = envelope(
            &plane,
            api_call(
                &cookie,
                &csrf,
                "catalog.preview",
                &fixture["review"]["body"].to_string(),
            ),
        )
        .await;
        assert_eq!(
            status, 200,
            "ordinary previews still evict ordinary previews: {preview}"
        );
    }
    let receipt = json!({"recover":token});
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "catalog.policy_resume",
            &receipt.to_string(),
        ),
    )
    .await;
    assert_eq!(
        status, 200,
        "pinned original recovery survives ordinary review pressure: {reply}"
    );
    assert_eq!(
        plane.secrets().sessions.review_count(&cookie),
        contract::MAX_REVIEWS_PER_SESSION
    );
    assert!(
        sent.lock()
            .unwrap()
            .last()
            .unwrap()
            .get("catalog_policy_resume_receipt")
            .is_some()
    );
}

#[tokio::test]
async fn r04_normal_success_releases_reservation_for_ordinary_eviction() {
    let fixture = r04_fixture();
    let mut script = Vec::new();
    for _ in 0..contract::MAX_REVIEWS_PER_SESSION {
        script.extend([Some(epoch(7)), Some(r04_policy_success())]);
    }
    script.extend([
        Some(epoch(7)),
        Some(r04_response(&fixture["review"]["response"])),
    ]);
    let (plane, sent, _) = r04_recovery_plane(script);
    let (cookie, csrf) = sign_in(&plane).await;
    let mut first = None;
    for _ in 0..contract::MAX_REVIEWS_PER_SESSION {
        let (status, reply) = envelope(
            &plane,
            api_call(
                &cookie,
                &csrf,
                "catalog.policy_resume",
                &r04_recovery_body(false).to_string(),
            ),
        )
        .await;
        assert_eq!(status, 200, "{reply}");
        assert_eq!(reply["request"]["state"], "completed");
        let token = reply["review"].as_str().unwrap().to_owned();
        first.get_or_insert(token);
    }
    let (status, preview) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "catalog.preview",
            &fixture["review"]["body"].to_string(),
        ),
    )
    .await;
    assert_eq!(
        status, 200,
        "normal success must not leave its custody pinned: {preview}"
    );
    let old_receipt = r04_recovery_read(false, first.as_deref().unwrap());
    let before = sent.lock().unwrap().len();
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "catalog.policy_resume_receipt",
            &old_receipt.to_string(),
        ),
    )
    .await;
    assert_eq!(
        (status, error_code(&reply).as_str()),
        (400, "admin_review_unknown")
    );
    assert_eq!(sent.lock().unwrap().len(), before);
    assert_eq!(
        plane.secrets().sessions.review_count(&cookie),
        contract::MAX_REVIEWS_PER_SESSION
    );
}

#[tokio::test]
async fn r04_no_effect_refusal_releases_its_reserved_review_slot() {
    let mut script = Vec::new();
    for _ in 0..contract::MAX_REVIEWS_PER_SESSION {
        script.extend([
            Some(epoch(7)),
            Some(ControlResponse::Failure {
                code: "key_conflict".into(),
            }),
        ]);
    }
    script.extend([Some(epoch(7)), Some(r04_unknown_metadata())]);
    let (plane, _, _) = r04_recovery_plane(script);
    let (cookie, csrf) = sign_in(&plane).await;
    for _ in 0..contract::MAX_REVIEWS_PER_SESSION {
        let (status, reply) = envelope(
            &plane,
            api_call(
                &cookie,
                &csrf,
                "catalog.policy_resume",
                &r04_recovery_body(false).to_string(),
            ),
        )
        .await;
        assert_eq!((status, error_code(&reply).as_str()), (409, "key_conflict"));
        assert!(reply.get("review").is_none());
        assert_eq!(plane.secrets().sessions.review_count(&cookie), 0);
    }
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "catalog.policy_resume",
            &r04_recovery_body(false).to_string(),
        ),
    )
    .await;
    r04_handle(status, &reply, true);
    assert_eq!(plane.secrets().sessions.review_count(&cookie), 1);
}

#[tokio::test]
async fn r04_recovery_handle_lifetime_follows_its_session_and_service() {
    for end in ["expiry", "logout", "service_end", "restart"] {
        let (plane, sent, now) =
            r04_recovery_plane(vec![Some(epoch(7)), Some(r04_unknown_metadata())]);
        let (cookie, csrf) = sign_in(&plane).await;
        let (status, reply) = envelope(
            &plane,
            api_call(
                &cookie,
                &csrf,
                "catalog.policy_resume",
                &r04_recovery_body(false).to_string(),
            ),
        )
        .await;
        let token = r04_handle(status, &reply, true);
        match end {
            "expiry" => {
                now.store(SESSION_IDLE_MS, Ordering::SeqCst);
            }
            "logout" => {
                assert_eq!(envelope(&plane, logout(&cookie, &csrf)).await.0, 200);
            }
            "service_end" | "restart" => plane.end(),
            _ => unreachable!(),
        }
        let receipt = r04_recovery_read(false, &token);
        let (status, reply) = if end == "restart" {
            let (restarted, _) = self::plane(2);
            envelope(
                &restarted,
                api_call(
                    &cookie,
                    &csrf,
                    "catalog.policy_resume_receipt",
                    &receipt.to_string(),
                ),
            )
            .await
        } else {
            envelope(
                &plane,
                api_call(
                    &cookie,
                    &csrf,
                    "catalog.policy_resume_receipt",
                    &receipt.to_string(),
                ),
            )
            .await
        };
        assert_eq!(
            (status, error_code(&reply).as_str()),
            (401, "admin_session_invalid"),
            "{end}"
        );
        assert_eq!(plane.secrets().sessions.review_count(&cookie), 0, "{end}");
        assert_eq!(
            sent.lock().unwrap().len(),
            2,
            "ended custody sends no receipt: {end}"
        );
    }
}

#[tokio::test]
async fn r04_actual_setup_apply_and_receipt_preserve_data_but_not_session_identity() {
    use airk_core::catalog_setup::{
        IndexSetupDraft, IndexSetupReceipt, IndexSetupRequest, SetupSource, SetupStage,
        SetupStorageAction, setup_index_id, setup_source_ids,
    };
    use airk_core::service::{CatalogOutcomeObservation, ProjectRegistration, RequestKey};
    let registration = ProjectRegistration::new(
        "/fiction/docs".into(),
        "/fiction/index".into(),
        "docs".into(),
    )
    .unwrap();
    let request = IndexSetupRequest {
        // Setup's native caller is not the unused caller draw of a
        // selection.key-only generate rule; it must come from the reply.
        caller: "c1".repeat(16).parse().unwrap(),
        key: RequestKey {
            namespace: "0a".repeat(16).parse().unwrap(),
            epoch: 7,
            nonce: "05".repeat(16).parse().unwrap(),
        },
        expected_catalog_revision: CatalogRevision::try_from(3).unwrap(),
        name: "Docs".into(),
        registration: registration.clone(),
        sources: vec![SetupSource {
            canonical_path: "/fiction/docs".into(),
            aliases: vec![],
            owner: true,
        }],
        setup_watch: false,
    };
    let request_digest = request.identity().unwrap();
    let draft = IndexSetupDraft::new(
        request.clone(),
        SetupStorageAction::CreateMissingLeaf,
        Sha256Digest::from_bytes([0x3d; 32]),
    )
    .unwrap();
    let receipt = IndexSetupReceipt {
        schema: 1,
        operation_id: airk_core::catalog_management::catalog_operation_id(
            request.caller,
            &request.key,
        )
        .unwrap(),
        request: request.clone(),
        request_digest,
        index_id: setup_index_id(&request).unwrap(),
        source_ids: setup_source_ids(&request).unwrap(),
        stage: SetupStage::Reserved,
        catalog_revision: CatalogRevision::try_from(4).unwrap(),
        source_revision: CatalogRevision::INITIAL,
        preparation: None,
        completion: None,
        legacy_adoption: None,
    };
    receipt.validate().unwrap();
    // Independently state the complete public projection. Native fixture
    // builders above establish valid digested identities, not redaction rules.
    let public_request = json!({"expected_catalog_revision": 3, "name": "Docs",
        "registration": {"binding": registration.binding, "root": "/fiction/docs", "data_root": "/fiction/index", "collection": "docs"},
        "sources": [{"canonical_path": "/fiction/docs", "aliases": [], "owner": true}], "setup_watch": false});
    let public_receipt = json!({"schema": 1, "operation_id": receipt.operation_id,
        "request": public_request, "request_digest": request_digest, "index_id": receipt.index_id,
        "source_ids": receipt.source_ids, "stage": "reserved", "catalog_revision": 4, "source_revision": 1,
        "preparation": null, "completion": null});
    let apply = json!({"method": {"catalog_index_setup_apply": {"draft": {
        "schema": 1, "request": public_request, "storage_action": "create_missing_leaf", "review_digest": "3d".repeat(32)
    }}}, "review": "06".repeat(16)});
    let (plane, sent, _) = r04_recovery_plane(vec![
        Some(epoch(7)),
        Some(ControlResponse::CatalogIndexSetupDraft {
            draft,
            request_digest,
        }),
        Some(ControlResponse::CatalogOutcomeUnknown {
            code: "catalog_index_setup_outcome_unknown".into(),
            caller: request.caller,
            key: request.key.clone(),
            request_digest,
            observation: Some(CatalogOutcomeObservation::IndexSetup {
                receipt: Box::new(receipt.clone()),
            }),
        }),
        Some(ControlResponse::CatalogIndexSetupReceipt {
            receipt: Some(receipt),
        }),
    ]);
    let (cookie, csrf) = sign_in(&plane).await;
    let selection = json!({"catalog_index_setup_draft": {"selection": {
        "expected_catalog_revision": 3, "name": "Docs", "primary_root": "/fiction/docs", "data_root": "/fiction/index",
        "collection": "docs", "additional_roots": [], "setup_watch": false
    }}});
    let (status, reviewed) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "catalog.index_setup_draft",
            &selection.to_string(),
        ),
    )
    .await;
    assert_eq!(status, 200, "{reviewed}");
    assert_eq!(reviewed["review"], "06".repeat(16));
    assert_eq!(
        reviewed["result"],
        json!({"status": "catalog_index_setup_draft", "request_digest": request_digest,
        "draft": {"schema": 1, "request": public_request, "storage_action": "create_missing_leaf", "review_digest": "3d".repeat(32)}})
    );
    let (status, reply) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "catalog.index_setup_apply",
            &apply.to_string(),
        ),
    )
    .await;
    assert_eq!(status, 503, "{reply}");
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["error"]["class"], "unavailable");
    assert_eq!(reply["error"]["retryable"], false);
    assert!(reply.get("review").is_none());
    assert!(reply.get("request").is_none());
    assert_eq!(
        reply["error"]["native"],
        json!({"status": "catalog_outcome_unknown",
        "code": "catalog_index_setup_outcome_unknown", "request_digest": request_digest,
        "observation": {"kind": "index_setup", "receipt": public_receipt}})
    );
    let lookup = json!({"method": {"catalog_index_setup_receipt": {}}, "review": "06".repeat(16)});
    let (status, saved) = envelope(
        &plane,
        api_call(
            &cookie,
            &csrf,
            "catalog.index_setup_receipt",
            &lookup.to_string(),
        ),
    )
    .await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(
        saved["result"],
        json!({"status": "catalog_index_setup_receipt", "receipt": public_receipt})
    );
    let sent = sent.lock().unwrap();
    assert_eq!(
        sent.len(),
        4,
        "unknown setup reads its exact receipt; it never replays apply"
    );
    assert_eq!(
        sent[2]["catalog_index_setup_apply"]["draft"]["request"]["caller"],
        "c1".repeat(16)
    );
    assert_eq!(
        sent[2]["catalog_index_setup_apply"]["draft"]["request"]["key"],
        json!({"namespace": "0a".repeat(16), "epoch": 7, "nonce": "05".repeat(16)})
    );
    assert_eq!(
        sent[3],
        json!({"catalog_index_setup_receipt": {"caller": "c1".repeat(16),
        "key": {"namespace": "0a".repeat(16), "epoch": 7, "nonce": "05".repeat(16)}}})
    );
}

#[tokio::test]
async fn r04_observed_cancellation_releases_read_custody_but_pins_uncertain_mutation() {
    let fixture = r04_fixture();
    let mut observations = Vec::new();
    for read in [true, false] {
        let (entered, mut entered_rx) = tokio::sync::mpsc::unbounded_channel();
        let (release, waiting) = tokio::sync::watch::channel(false);
        let calls = Arc::new(AtomicU64::new(0));
        let observed = calls.clone();
        let dispatch: Dispatch = Arc::new(move |method| {
            if matches!(method, ControlMethod::RequestEpoch) {
                return Box::pin(async { Ok(epoch(7)) });
            }
            assert!(if read {
                matches!(method, ControlMethod::CatalogPreview { .. })
            } else {
                matches!(method, ControlMethod::CatalogPolicyResume { .. })
            });
            observed.fetch_add(1, Ordering::SeqCst);
            let entered = entered.clone();
            let mut waiting = waiting.clone();
            Box::pin(async move {
                entered.send(()).unwrap();
                waiting.wait_for(|released| *released).await.unwrap();
                Ok(r04_unknown_metadata())
            })
        });
        let sequence = Arc::new(AtomicU8::new(0));
        let plane = Arc::new(AdminPlane::with_sources(
            dispatch,
            Incarnation::from_bytes([1; 16]),
            Arc::new(|| 0),
            Arc::new(move || Some([sequence.fetch_add(1, Ordering::SeqCst) + 1; 32])),
        ));
        let (cookie, csrf) = sign_in(&plane).await;
        let body = if read {
            fixture["review"]["body"].clone()
        } else {
            r04_recovery_body(false)
        };
        let request = api_call(
            &cookie,
            &csrf,
            if read {
                "catalog.preview"
            } else {
                "catalog.policy_resume"
            },
            &body.to_string(),
        );
        let running = plane.clone();
        let task = tokio::spawn(async move { envelope(&running, request).await });
        tokio::time::timeout(std::time::Duration::from_secs(5), entered_rx.recv())
            .await
            .unwrap()
            .unwrap();
        let before = plane.secrets().sessions.review_count(&cookie);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        release.send(true).unwrap();
        let after = plane.secrets().sessions.review_count(&cookie);
        observations.push((read, before, after, calls.load(Ordering::SeqCst)));
    }
    assert_eq!(
        observations,
        [(true, 1, 0, 1), (false, 1, 1, 1)],
        "only a canceled read proves its reserved mutation identity was unused"
    );
}

/// A fake gate side of the A2 read authority: records every registered ref
/// and every revoked nonce; `accept` decides registration.
#[allow(clippy::type_complexity)]
fn recorded_authority() -> (
    SelectionAuthority,
    Arc<Mutex<Vec<SelectionScopeRef>>>,
    Arc<Mutex<Vec<Incarnation>>>,
    Arc<std::sync::atomic::AtomicBool>,
) {
    let registered = Arc::new(Mutex::new(Vec::new()));
    let revoked = Arc::new(Mutex::new(Vec::new()));
    let accept = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let (register_log, revoke_log, accepting) =
        (registered.clone(), revoked.clone(), accept.clone());
    let authority = SelectionAuthority {
        register: Arc::new(move |selection: &SelectionScopeRef| {
            register_log.lock().unwrap().push(*selection);
            accepting.load(Ordering::SeqCst)
        }),
        revoke: Arc::new(move |nonces: Vec<Incarnation>| {
            revoke_log.lock().unwrap().extend(nonces);
        }),
    };
    (authority, registered, revoked, accept)
}

fn logout(cookie: &str, csrf: &str) -> Request<Full<Bytes>> {
    let session_cookie = format!("{COOKIE_NAME}={cookie}");
    request(
        "POST",
        "/admin/api/v1/session/logout",
        &[
            HOST,
            ORIGIN,
            SAME_ORIGIN,
            JSON,
            ("cookie", session_cookie.as_str()),
            ("x-airk-csrf", csrf),
        ],
        "{}",
    )
}

/// Selection authority §3.1, §3.3 (N4) for A2: each stored grant registers
/// a fresh live nonce nested in the plane's lock; replacing it, logging out
/// and service end revoke the dropped nonce once the lock is released; a
/// failed check registers and revokes nothing.
#[tokio::test]
async fn admin_grants_hold_a_live_nonce_until_their_session_drops_them() {
    let (plane, sent, _) = scripted_plane(Vec::new());
    let (authority, registered, revoked, _) = recorded_authority();
    let plane = plane.with_selection_authority(authority);
    // The launch code, cookie and token draw random 1-3.
    let (cookie, csrf) = sign_in(&plane).await;
    let select = |cookie: &str, csrf: &str, index: &str| {
        api_call(cookie, csrf, "search.select_scope", &grant_body(index, 12))
    };
    let (status, reply) = envelope(&plane, select(&cookie, &csrf, GRANTED_INDEX)).await;
    assert_eq!(status, 200, "{reply}");
    let first = SelectionScopeRef {
        owner: SelectionOwnerKind::Admin,
        grant_nonce: Incarnation::from_bytes([4; 16]),
        service_instance: Incarnation::from_bytes([1; 16]),
        scope_identity: Sha256Digest::from_bytes([0x5c; 32]),
        catalog_revision: CatalogRevision::try_from(12).unwrap(),
    };
    assert_eq!(*registered.lock().unwrap(), [first]);
    assert!(revoked.lock().unwrap().is_empty());
    // The nonce never reaches the browser.
    assert!(!reply.to_string().contains(&"04".repeat(16)), "{reply}");
    // A re-selection gets a new nonce and revokes the replaced one.
    let (status, _) = envelope(&plane, select(&cookie, &csrf, GRANTED_INDEX)).await;
    assert_eq!(status, 200);
    let second = SelectionScopeRef {
        grant_nonce: Incarnation::from_bytes([5; 16]),
        ..first
    };
    assert_eq!(*registered.lock().unwrap(), [first, second]);
    assert_eq!(*revoked.lock().unwrap(), [first.grant_nonce]);
    // A failed check keeps the current grant: nothing registered or revoked.
    let other = "idx_0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f";
    let (status, _) = envelope(&plane, select(&cookie, &csrf, other)).await;
    assert_eq!(status, 409);
    assert_eq!(registered.lock().unwrap().len(), 2);
    assert_eq!(revoked.lock().unwrap().len(), 1);
    // Logout revokes the session's grant.
    assert_eq!(
        answer(&plane, logout(&cookie, &csrf), PeerProof::SameUser)
            .await
            .0,
        200
    );
    assert_eq!(
        *revoked.lock().unwrap(),
        [first.grant_nonce, second.grant_nonce]
    );
    // Service end revokes every remaining grant.
    let (cookie, csrf) = sign_in(&plane).await;
    let (status, _) = envelope(&plane, select(&cookie, &csrf, GRANTED_INDEX)).await;
    assert_eq!(status, 200);
    let third = registered.lock().unwrap()[2];
    plane.end();
    assert_eq!(revoked.lock().unwrap()[2..], [third.grant_nonce]);
    assert_eq!(plane.sessions(), 0);
    assert!(sent.lock().unwrap().is_empty());
}

/// A nonce the gate refuses is never stored: the grant answers
/// `scoped_read_unavailable` and the session stays without a grant.
#[tokio::test]
async fn an_admin_grant_the_gate_refuses_is_not_stored() {
    let (plane, _, _) = scripted_plane(Vec::new());
    let (authority, registered, revoked, accept) = recorded_authority();
    accept.store(false, Ordering::SeqCst);
    let plane = plane.with_selection_authority(authority);
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
    assert_eq!(
        (status, error_code(&reply).as_str()),
        (409, "scoped_read_unavailable")
    );
    assert_eq!(registered.lock().unwrap().len(), 1);
    assert!(revoked.lock().unwrap().is_empty());
    assert!(plane.secrets().sessions.grant(&cookie, 0).is_none());
}

/// N4 at the session store: replacement, a store that found no session,
/// idle expiry, eviction, logout and service end each hand back the nonce
/// of the grant they dropped, once.
#[test]
fn every_dropped_admin_grant_hands_back_its_nonce() {
    let instance = Incarnation::from_bytes([1; 16]);
    let grant = |nonce: u8| secrets::Grant {
        selection: WindowScopeSelection {
            namespace: Incarnation::from_bytes([0x0a; 16]),
            expected_catalog_revision: CatalogRevision::try_from(12).unwrap(),
            index_ids: vec![GRANTED_INDEX.parse().unwrap()],
            project_ids: Vec::new(),
        },
        scope_identity: Sha256Digest::from_bytes([0x5c; 32]),
        catalog_revision: CatalogRevision::try_from(12).unwrap(),
        instance,
        selection_ref: Some(SelectionScopeRef {
            owner: SelectionOwnerKind::Admin,
            grant_nonce: Incarnation::from_bytes([nonce; 16]),
            service_instance: instance,
            scope_identity: Sha256Digest::from_bytes([0x5c; 32]),
            catalog_revision: CatalogRevision::try_from(12).unwrap(),
        }),
    };
    let nonce = |n: u8| Incarnation::from_bytes([n; 16]);
    let mut sessions = secrets::Sessions::new(instance);
    let first = sessions.create(&[1; 32], &[2; 32], 0);
    assert!(sessions.set_grant(&first.cookie, grant(10), 0).is_some());
    assert!(sessions.take_dropped().is_empty());
    assert!(sessions.set_grant(&first.cookie, grant(11), 0).is_some());
    assert_eq!(sessions.take_dropped(), [nonce(10)]);
    let unknown = secrets::secret_text(&[9; 32]);
    assert_eq!(sessions.set_grant(&unknown, grant(12), 0), None);
    assert_eq!(sessions.take_dropped(), [nonce(12)]);
    // Idle expiry, observed by the next session operation.
    assert!(sessions.grant(&first.cookie, SESSION_IDLE_MS).is_none());
    assert_eq!(sessions.take_dropped(), [nonce(11)]);
    // Eviction of the oldest session at the bound.
    let oldest = sessions.create(&[3; 32], &[4; 32], 1);
    assert!(sessions.set_grant(&oldest.cookie, grant(13), 1).is_some());
    for index in 1..MAX_SESSIONS {
        let index = u8::try_from(index).unwrap();
        sessions.create(&[20 + index; 32], &[60 + index; 32], 1);
    }
    assert!(sessions.take_dropped().is_empty());
    let last = sessions.create(&[90; 32], &[91; 32], 1);
    assert_eq!(sessions.take_dropped(), [nonce(13)]);
    assert!(sessions.set_grant(&last.cookie, grant(14), 1).is_some());
    assert!(sessions.remove(&last.cookie));
    assert_eq!(sessions.take_dropped(), [nonce(14)]);
    let kept = secrets::secret_text(&[21; 32]);
    assert!(sessions.set_grant(&kept, grant(15), 1).is_some());
    sessions.end();
    assert_eq!(sessions.take_dropped(), [nonce(15)]);
    assert_eq!(sessions.len(), 0);
}

#[tokio::test]
async fn invalid_original_read_parent_observes_no_scope_or_backend() {
    let (plane, sent, _) = scripted_plane(Vec::new());
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
    let observations = Arc::new(AtomicU64::new(0));
    let counted = observations.clone();
    let plane = plane.with_scope_check(Arc::new(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err("invalid original parent reached scope observation".to_owned()) })
    }));
    let now = airk_process::monotonic::now_ns().unwrap();
    for schema in [1, 2] {
        let result = plane
            .read(
                "search.call",
                Some(&cookie),
                ControlMethod::ToolsCall {
                    tool: "airk_answer".into(),
                    arguments: serde_json::Map::new(),
                    selection: ToolSelection::Admin,
                    response_mode: airk_core::service::ToolResponseMode::Full,
                    usage_client: airk_core::usage::ClientFamily::Cli,
                    read_attempt: Some(airk_core::read_attempt::ReadAttemptBudget {
                        schema,
                        request_id: Incarnation::from_bytes([8; 16]),
                        clock: if cfg!(windows) {
                            airk_core::read_attempt::ReadClock::WindowsPerformanceCounter
                        } else {
                            airk_core::read_attempt::ReadClock::UnixMonotonic
                        },
                        expires_monotonic_ns: now,
                        remaining_ns: 2_000_000_000,
                        native_retry_expires_monotonic_ns: None,
                    }),
                },
                None,
            )
            .await;
        assert!(result.is_ok(), "typed canonical read failure response");
    }
    assert_eq!(observations.load(Ordering::SeqCst), 0);
    assert!(sent.lock().unwrap().is_empty());
}
