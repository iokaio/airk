// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::native_admin::{NativeAdminChunk, NativeAdminSessionSecret};

struct BridgeService {
    log: Mutex<Vec<&'static str>>,
    changed: Condvar,
    gate: Gate,
    body: Mutex<Option<String>>,
    bytes: Vec<u8>,
    read_refusal: Mutex<Option<&'static str>>,
}
impl BridgeService {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            log: Mutex::new(Vec::new()),
            changed: Condvar::new(),
            gate: Gate::default(),
            body: Mutex::new(None),
            bytes: serde_json::to_vec(&json!({"future_native":["é中",true,1],"ok":true})).unwrap(),
            read_refusal: Mutex::new(None),
        })
    }
    fn record(&self, name: &'static str) {
        self.log.lock().unwrap().push(name);
        self.changed.notify_all();
    }
    fn wait(&self, name: &str) {
        let log = self.log.lock().unwrap();
        let (_log, timeout) = self
            .changed
            .wait_timeout_while(log, OBSERVE, |log| !log.contains(&name))
            .unwrap();
        assert!(!timeout.timed_out(), "missing native {name}");
    }
    fn secret() -> NativeAdminSessionSecret {
        NativeAdminSessionSecret::new("A".repeat(43), "E".repeat(43)).unwrap()
    }
    fn chunk(&self, offset: usize) -> ControlResponse {
        let end = if offset == 0 {
            self.bytes.len() / 2
        } else {
            self.bytes.len()
        };
        ControlResponse::AdminNativeChunk {
            chunk: NativeAdminChunk {
                http_status: 200,
                total_bytes: self.bytes.len() as u64,
                offset: offset as u64,
                data: self.bytes[offset..end].to_vec(),
                done: end == self.bytes.len(),
            },
        }
    }
}
impl Service for BridgeService {
    fn instance(&self) -> Result<Incarnation, ErrorLine> {
        Ok(E1.parse().unwrap())
    }
    fn request(
        &self,
        method: ControlMethod,
        expected: Incarnation,
    ) -> Result<ControlResponse, ErrorLine> {
        assert_eq!(expected, E1.parse().unwrap());
        Ok(match method {
            ControlMethod::WindowOpen { .. } => ControlResponse::WindowOpened {
                window_id: id(2).parse().unwrap(),
                service_instance: expected,
                lease: WindowLease::from_bytes([3; 32]),
                ttl_ms: 30000,
            },
            ControlMethod::WindowClose { .. } => ControlResponse::WindowClosed {},
            ControlMethod::WindowRenew { .. } => ControlResponse::WindowRenewed { ttl_ms: 30000 },
            ControlMethod::AdminNativeOpen { deadline_ns } => {
                assert!(deadline_ns > 0);
                self.record("open");
                self.gate.pass();
                ControlResponse::AdminNativeOpened {
                    session: Self::secret(),
                    service_instance: expected,
                }
            }
            ControlMethod::AdminNativeCall {
                session,
                body,
                request_id,
                deadline_ns,
                ..
            } => {
                assert_eq!(session, Self::secret());
                assert_eq!(request_id, id(10).parse().unwrap());
                assert!(deadline_ns > 0);
                *self.body.lock().unwrap() = Some(body.get().to_owned());
                self.record("call");
                self.chunk(0)
            }
            ControlMethod::AdminNativeRead {
                session, offset, ..
            } => {
                assert_eq!(session, Self::secret());
                self.record("read");
                if let Some(code) = *self.read_refusal.lock().unwrap() {
                    return Ok(ControlResponse::Failure { code: code.into() });
                }
                self.chunk(offset as usize)
            }
            ControlMethod::AdminNativeCancel { session, .. } => {
                assert_eq!(session, Self::secret());
                self.record("cancel");
                ControlResponse::AdminNativeCancelled {}
            }
            ControlMethod::AdminNativeClose { session } => {
                assert_eq!(session, Self::secret());
                self.record("close");
                ControlResponse::AdminNativeClosed {}
            }
            // Simulate an unexpected private reply on an allowed metadata read.
            ControlMethod::Health => ControlResponse::AdminNativeChunk {
                chunk: NativeAdminChunk {
                    http_status: 200,
                    total_bytes: 43,
                    offset: 0,
                    data: "A".repeat(43).into_bytes(),
                    done: true,
                },
            },
            _ => {
                return Err(ErrorLine::new(
                    ExitClass::Refused,
                    "unexpected_fixture_method",
                    false,
                    "unexpected fixture method",
                ));
            }
        })
    }
}
fn client(clock: &Arc<ManualClock>, service: &Arc<BridgeService>) -> Client {
    let mut settings = options(clock, None, unused);
    let service = service.clone();
    settings.capabilities.push("admin-management/1".into());
    settings.connect = Some(Arc::new(move || Some(service.clone() as Arc<dyn Service>)));
    let mut client = Client::start(settings);
    assert_eq!(client.ask(&hello())["outcome"]["status"], "hello");
    client
}
fn call(deadline: u64) -> Value {
    frame(
        10,
        json!({"admin_call":{"operation":"usage.document","body":{"future_body":"retained"},"deadline_ms":deadline}}),
    )
}

#[test]
fn native_admin_session_chunks_preserve_raw_body_and_never_expose_secret_frames() {
    let clock = Arc::new(ManualClock::default());
    let service = BridgeService::new();
    let mut client = client(&clock, &service);
    let empty = "{  \"future_body\" : \"é中\" }";
    let body = format!(
        "{{ {} \"future_body\" : \"é中\" }}",
        " ".repeat(airk_core::native_admin::MAX_BODY_BYTES - empty.len())
    );
    assert_eq!(body.len(), 65_536);
    let raw = format!(
        r#"{{"id":"{}","request":{{"admin_call":{{"operation":"usage.document","body":{},"deadline_ms":1000}}}}}}"#,
        id(10),
        body
    );
    let mut bytes = (raw.len() as u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(raw.as_bytes());
    client.raw(bytes);
    let first = client.next();
    assert_eq!(first["outcome"]["status"], "admin_chunk");
    assert_eq!(service.body.lock().unwrap().as_deref(), Some(body.as_str()));
    let chunk: NativeAdminChunk =
        serde_json::from_value(first["outcome"]["chunk"].clone()).unwrap();
    let offset = chunk.validate().unwrap();
    let last = client.ask(&frame(
        11,
        json!({"admin_read":{"target":id(10),"offset":offset}}),
    ));
    let mut assembly = airk_core::native_admin::NativeAdminAssembly::default();
    assert!(assembly.push(&chunk).unwrap().is_none());
    let last: NativeAdminChunk = serde_json::from_value(last["outcome"]["chunk"].clone()).unwrap();
    assert_eq!(
        assembly.push(&last).unwrap().unwrap(),
        serde_json::from_slice::<Value>(&service.bytes).unwrap()
    );
    let guarded = client.ask(&frame(12, json!({"control":{"method":"health"}})));
    assert_eq!(
        guarded["outcome"]["response"],
        json!({"status":"failure","code":"console_private_response_forbidden"})
    );
    assert_eq!(
        client.ask(&frame(13, json!({"admin_close":{}})))["outcome"]["status"],
        "admin_closed"
    );
    let (_, exit, text) = client.eof();
    assert_eq!(exit, 0);
    for secret in ["A".repeat(43), "E".repeat(43), "03".repeat(32)] {
        assert!(!text.contains(&secret));
    }
    assert_eq!(
        *service.log.lock().unwrap(),
        vec!["open", "call", "read", "close"]
    );
}

#[test]
fn native_admin_cancel_during_open_closes_late_session_without_dispatch() {
    let clock = Arc::new(ManualClock::default());
    let service = BridgeService::new();
    service.gate.set(true);
    let mut client = client(&clock, &service);
    client.send(&call(1000));
    service.wait("open");
    assert_eq!(
        client.ask(&frame(11, json!({"admin_cancel":{"target":id(10)}})))["outcome"]["status"],
        "cancelled"
    );
    service.gate.set(false);
    assert_eq!(client.next()["outcome"]["status"], "cancelled");
    service.wait("close");
    let (_, exit, text) = client.eof();
    assert_eq!(exit, 0);
    assert!(!text.contains(&"A".repeat(43)));
    assert_eq!(*service.log.lock().unwrap(), vec!["open", "close"]);
    assert!(service.body.lock().unwrap().is_none());
}

#[test]
fn native_admin_original_deadline_during_open_prevents_first_dispatch() {
    let clock = Arc::new(ManualClock::default());
    let service = BridgeService::new();
    service.gate.set(true);
    let mut client = client(&clock, &service);
    client.send(&call(5));
    service.wait("open");
    clock.advance(6);
    assert_eq!(
        client.next(),
        failure(10, "session_deadline_exceeded", false)
    );
    service.gate.set(false);
    service.wait("close");
    let (_, exit, _) = client.eof();
    assert_eq!(exit, 0);
    assert_eq!(*service.log.lock().unwrap(), vec!["open", "close"]);
    assert!(service.body.lock().unwrap().is_none());
}

#[test]
fn native_admin_midstream_auth_end_preserves_unknown_effects_and_prevents_reopen() {
    for code in [
        "admin_session_invalid",
        "admin_internal",
        "service_draining",
    ] {
        let clock = Arc::new(ManualClock::default());
        let service = BridgeService::new();
        *service.read_refusal.lock().unwrap() = Some(code);
        let mut client = client(&clock, &service);
        let first = client.ask(&call(1000));
        assert_eq!(first["outcome"]["chunk"]["http_status"], 200);
        let chunk: NativeAdminChunk =
            serde_json::from_value(first["outcome"]["chunk"].clone()).unwrap();
        let next = client.ask(&frame(
            11,
            json!({"admin_read":{"target":id(10),"offset":chunk.validate().unwrap()}}),
        ));
        assert_eq!(
            next,
            failure(
                11,
                if code == "admin_session_invalid" {
                    "admin_session_ended_outcome_unknown"
                } else {
                    "admin_outcome_unknown"
                },
                false
            )
        );
        if code == "admin_session_invalid" {
            let refused = client.ask(&frame(
                12,
                json!({"admin_call":{"operation":"usage.document","body":{},"deadline_ms":1000}}),
            ));
            assert_eq!(refused["outcome"]["code"], "admin_session_closing");
            assert_eq!(*service.log.lock().unwrap(), vec!["open", "call", "read"]);
        }
        let (_, exit, text) = client.eof();
        assert_eq!(exit, 0);
        assert!(!text.contains(&"A".repeat(43)) && !text.contains(&"E".repeat(43)));
    }
}
