// SPDX-License-Identifier: Apache-2.0
//! Session protocol 2 against the reviewed frames
//! (`fixtures/wire/airkctl_session_frames.json`) and the session's pipe frames
//! in the window-plane transcripts (`fixtures/split/i1/window_plane.json`).
//! Time is a manual clock; tests wait only for observed frames or pipe calls
//! under a deadline, never for a fixed interval.
use super::*;
use serde_json::json;
use std::sync::mpsc::{Receiver, Sender, channel};

const FRAMES: &str = include_str!("../../../../../fixtures/wire/airkctl_session_frames.json");
const PLANE: &str = include_str!("../../../../../fixtures/split/i1/window_plane.json");
/// Observation deadline for a frame or pipe call; never a sleep.
const OBSERVE: Duration = Duration::from_secs(30);
const E1: &str = "e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1";
const E2: &str = "e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2";

fn id(byte: u8) -> String {
    format!("{byte:02x}").repeat(16)
}

fn frame(byte: u8, request: Value) -> Value {
    json!({"id": id(byte), "request": request})
}

/// A fixture case's `json`, by list and name.
fn case(list: &str, name: &str) -> Value {
    let fixture: Value = serde_json::from_str(FRAMES).unwrap();
    fixture[list]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap_or_else(|| panic!("{list}: {name}"))["json"]
        .clone()
}

fn transcript(name: &str) -> Vec<Value> {
    let plane: Value = serde_json::from_str(PLANE).unwrap();
    let case = plane["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == name)
        .unwrap_or_else(|| panic!("window plane case {name}"));
    case["transcript"].as_array().unwrap().clone()
}

/// `{"$fill_bytes": N}` is a value whose compact JSON is exactly N bytes.
fn expand(value: &Value) -> Value {
    match value {
        Value::Object(map) => match map.get("$fill_bytes").and_then(Value::as_u64) {
            Some(bytes) => Value::String("x".repeat(bytes as usize - 2)),
            None => Value::Object(map.iter().map(|(k, v)| (k.clone(), expand(v))).collect()),
        },
        Value::Array(items) => Value::Array(items.iter().map(expand).collect()),
        other => other.clone(),
    }
}

/// The first `take` session-originated pipe requests of `name` naming one of
/// `methods`, each with the reply that follows it.
fn pipe(name: &str, methods: &[&str], take: usize) -> Vec<(Value, Value)> {
    let steps = transcript(name);
    let script: Vec<_> = steps
        .iter()
        .enumerate()
        .filter(|(_, step)| step["channel"] == "pipe" && step.get("send").is_some())
        .filter(|(_, step)| {
            methods
                .iter()
                .any(|m| step["send"]["method"].get(*m).is_some())
        })
        .take(take)
        .map(|(index, step)| (expand(&step["send"]), expand(&steps[index + 1]["reply"])))
        .collect();
    assert_eq!(script.len(), take, "{name}: selected pipe steps");
    script
}

/// The session frames `name` sends or answers, in order.
fn session_steps(name: &str, key: &str) -> Vec<Value> {
    transcript(name)
        .into_iter()
        .filter(|step| step["channel"] == "session" && step.get(key).is_some())
        .map(|step| step[key].clone())
        .collect()
}

#[derive(Default)]
struct ManualClock {
    now: AtomicU64,
    wake: Mutex<Option<Wake>>,
}

impl Clock for ManualClock {
    fn now_ms(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
    fn real_wait(&self, _ms: u64) -> Option<Duration> {
        None
    }
    fn attach(&self, wake: Wake) {
        *self.wake.lock().unwrap() = Some(wake);
    }
}

impl ManualClock {
    fn advance(&self, ms: u64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
        if let Some(wake) = self.wake.lock().unwrap().as_ref() {
            wake();
        }
    }
}

/// A gate the test opens and closes; closed blocks the holder.
#[derive(Clone, Default)]
struct Gate(Arc<(Mutex<bool>, Condvar)>);

impl Gate {
    fn set(&self, closed: bool) {
        *self.0.0.lock().unwrap() = closed;
        self.0.1.notify_all();
    }
    fn pass(&self) {
        let mut closed = self.0.0.lock().unwrap();
        while *closed {
            closed = self.0.1.wait(closed).unwrap();
        }
    }
}

struct Input {
    frames: Receiver<Vec<u8>>,
    pending: Vec<u8>,
    at: usize,
}

impl Read for Input {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        while self.at == self.pending.len() {
            match self.frames.recv() {
                Ok(bytes) => (self.pending, self.at) = (bytes, 0),
                Err(_) => return Ok(0),
            }
        }
        let count = buffer.len().min(self.pending.len() - self.at);
        buffer[..count].copy_from_slice(&self.pending[self.at..self.at + count]);
        self.at += count;
        Ok(count)
    }
}

struct Output {
    bytes: Sender<Vec<u8>>,
    gate: Gate,
}

impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.gate.pass();
        self.bytes
            .send(bytes.to_vec())
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Client {
    input: Option<Sender<Vec<u8>>>,
    output: Receiver<Vec<u8>>,
    buffer: Vec<u8>,
    exit: Receiver<u8>,
    gate: Gate,
    text: String,
}

impl Client {
    fn start(options: Options) -> Self {
        let (input, frames) = channel();
        let (bytes, output) = channel();
        let (code, exit) = channel();
        let gate = Gate::default();
        let writer = Output {
            bytes,
            gate: gate.clone(),
        };
        std::thread::spawn(move || {
            let reader = Input {
                frames,
                pending: Vec::new(),
                at: 0,
            };
            let _ = code.send(serve_streams(reader, writer, options));
        });
        Self {
            input: Some(input),
            output,
            buffer: Vec::new(),
            exit,
            gate,
            text: String::new(),
        }
    }

    fn raw(&self, bytes: Vec<u8>) {
        self.input.as_ref().unwrap().send(bytes).unwrap();
    }

    fn send(&self, frame: &Value) {
        let mut bytes = Vec::new();
        airk_ipc::frame::write_frame_limited(&mut bytes, frame, MAX_RESPONSE_FRAME_BYTES).unwrap();
        self.raw(bytes);
    }

    /// The next frame, or `None` once the session closed its output.
    fn try_next(&mut self) -> Option<Value> {
        loop {
            if self.buffer.len() >= 4 {
                let length = u32::from_be_bytes(self.buffer[..4].try_into().unwrap()) as usize;
                if self.buffer.len() >= 4 + length {
                    let body: Vec<u8> = self.buffer.drain(..4 + length).skip(4).collect();
                    self.text.push_str(&String::from_utf8_lossy(&body));
                    return Some(serde_json::from_slice(&body).unwrap());
                }
            }
            match self.output.recv_timeout(OBSERVE) {
                Ok(chunk) => self.buffer.extend(chunk),
                Err(mpsc::RecvTimeoutError::Disconnected) => return None,
                Err(mpsc::RecvTimeoutError::Timeout) => panic!("no session frame observed"),
            }
        }
    }

    fn next(&mut self) -> Value {
        self.try_next().expect("session frame")
    }

    fn ask(&mut self, frame: &Value) -> Value {
        self.send(frame);
        self.next()
    }

    /// Close stdin; returns the remaining frames and the exit code.
    fn eof(mut self) -> (Vec<Value>, u8, String) {
        self.input = None;
        let mut rest = Vec::new();
        while let Some(frame) = self.try_next() {
            rest.push(frame);
        }
        let code = self.exit.recv_timeout(OBSERVE).expect("session exit");
        (rest, code, self.text)
    }
}

/// The pipe as the session sees it: scripted session requests, every call
/// recorded. `Events` answers an empty page and is not recorded (the
/// transcripts elide event cursors).
struct FakeService {
    instance: Mutex<Incarnation>,
    script: Mutex<VecDeque<Value>>,
    replies: Mutex<VecDeque<Value>>,
    calls: Mutex<Vec<Value>>,
    called: Condvar,
}

impl FakeService {
    fn new(instance: &str, script: Vec<(Value, Value)>) -> Arc<Self> {
        let (sends, replies) = script.into_iter().unzip();
        Arc::new(Self {
            instance: Mutex::new(instance.parse().unwrap()),
            script: Mutex::new(sends),
            replies: Mutex::new(replies),
            calls: Mutex::new(Vec::new()),
            called: Condvar::new(),
        })
    }

    fn wait_calls(&self, count: usize) {
        let calls = self.calls.lock().unwrap();
        let (calls, timeout) = self
            .called
            .wait_timeout_while(calls, OBSERVE, |calls| calls.len() < count)
            .unwrap();
        assert!(!timeout.timed_out(), "pipe calls observed: {calls:?}");
    }

    /// Every scripted request was sent, exactly and in order.
    fn finished(&self, expected: &[(Value, Value)]) {
        let calls = self.calls.lock().unwrap();
        let sends: Vec<_> = expected.iter().map(|(send, _)| send.clone()).collect();
        assert_eq!(*calls, sends);
    }
}

impl Service for FakeService {
    fn instance(&self) -> Result<Incarnation, ErrorLine> {
        Ok(*self.instance.lock().unwrap())
    }

    fn request(
        &self,
        method: ControlMethod,
        expected: Incarnation,
    ) -> Result<ControlResponse, ErrorLine> {
        if let ControlMethod::Events { .. } = method {
            let namespace: Incarnation = "0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c0c".parse().unwrap();
            let tail = EventCursor {
                namespace,
                project: None,
                after: 0,
                through: None,
            };
            let page = EventPage {
                namespace,
                oldest_sequence: None,
                snapshot_through: 0,
                retained_prefix_lost: false,
                events: Vec::new(),
                next: None,
                tail,
            };
            return Ok(ControlResponse::Events { page });
        }
        let sent = json!({"protocol": 1, "expected_instance": expected,
            "method": serde_json::to_value(&method).unwrap()});
        let scripted = self.script.lock().unwrap().pop_front();
        let reply = self.replies.lock().unwrap().pop_front();
        self.calls.lock().unwrap().push(sent.clone());
        self.called.notify_all();
        match (scripted, reply) {
            (Some(scripted), Some(reply)) if scripted == sent => {
                Ok(serde_json::from_value(reply).unwrap())
            }
            _ => Err(ErrorLine::new(
                ExitClass::Refused,
                "unscripted_pipe_request",
                false,
                "unscripted",
            )),
        }
    }
}

struct Script<F>(F);

impl<F: Fn(&[String], &mut crate::cli::Io) -> u8 + Send + Sync> Invoker for Script<F> {
    fn run(&self, arguments: Vec<OsString>, io: &mut crate::cli::Io) -> u8 {
        let arguments: Vec<String> = arguments
            .into_iter()
            .map(|argument| argument.into_string().unwrap())
            .collect();
        assert_eq!(arguments[0], "airkctl", "the program name comes first");
        (self.0)(&arguments[1..], io)
    }
}

fn options(
    clock: &Arc<ManualClock>,
    service: Option<Arc<FakeService>>,
    invoker: impl Fn(&[String], &mut crate::cli::Io) -> u8 + Send + Sync + 'static,
) -> Options {
    let mut options = Options::new(
        "1.9.0",
        "1111111111111111111111111111111111111111111111111111111111111111",
    );
    options.capabilities = vec!["session/2".into(), "tools/1".into()];
    options.label = "workspace A".into();
    options.owner_pid = 4242;
    options.timing = Timing {
        clock: clock.clone(),
        drain_ms: 5_000,
        owner_poll_ms: 250,
        renew_every_ms: 10_000,
        event_poll_ms: 1_000,
    };
    options.connect = Some(Arc::new(move || {
        service.clone().map(|service| service as Arc<dyn Service>)
    }));
    options.invoker = Some(Arc::new(Script(invoker)));
    options
}

fn unused(_: &[String], _: &mut crate::cli::Io) -> u8 {
    panic!("no command may run")
}

fn hello() -> Value {
    case("requests", "hello names the client and its I2 contracts")
}

fn failure(byte: u8, code: &str, retryable: bool) -> Value {
    json!({"id": id(byte), "outcome": {"status": "failed", "code": code, "retryable": retryable}})
}

#[test]
fn hello_comes_first_and_answers_without_an_installation() {
    let clock = Arc::new(ManualClock::default());
    let mut client = Client::start(options(&clock, None, unused));
    let early = case("requests", "invoke without stdin");
    assert_eq!(
        client.ask(&early),
        failure(3, "session_protocol_mismatch", false)
    );
    let old = case(
        "invalid_requests",
        "protocol 1 hello lacks the client and contracts",
    );
    assert_eq!(
        client.ask(&old),
        failure(1, "session_protocol_mismatch", false)
    );
    let answer = case("responses", "hello before any installation is found");
    assert_eq!(client.ask(&hello()), answer);
    assert_eq!(
        client.ask(&hello()),
        failure(1, "session_invalid_request", false)
    );
    let relay = case("requests", "control relay names no control directory");
    assert_eq!(
        client.ask(&relay),
        case("responses", "control failure relay")
    );
    let unsubscribe = case("requests", "unsubscribe");
    assert_eq!(client.ask(&unsubscribe), case("responses", "unsubscribed"));
    let (rest, code, _) = client.eof();
    assert!(rest.is_empty());
    assert_eq!(code, 0);
}

#[test]
fn invalid_frames_are_refused_and_a_frame_without_an_id_closes() {
    let clock = Arc::new(ManualClock::default());
    let mut client = Client::start(options(&clock, None, unused));
    client.ask(&hello());
    let invalid: Vec<Value> = serde_json::from_str::<Value>(FRAMES).unwrap()["invalid_requests"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| {
            !matches!(
                case["name"].as_str(),
                Some("protocol 1 hello lacks the client and contracts" | "non-canonical id")
            )
        })
        .map(|case| case["json"].clone())
        .collect();
    assert_eq!(invalid.len(), 9, "selected invalid requests");
    for frame in &invalid {
        let byte = u8::from_str_radix(&frame["id"].as_str().unwrap()[..2], 16).unwrap();
        assert_eq!(
            client.ask(frame),
            failure(byte, "session_invalid_request", false)
        );
    }
    client.send(&case("invalid_requests", "non-canonical id"));
    let (rest, code, _) = client.eof();
    assert!(rest.is_empty(), "nothing is answered after a codec failure");
    assert_eq!(code, 0);
}

#[test]
fn an_oversize_request_frame_closes_the_session() {
    let clock = Arc::new(ManualClock::default());
    let mut client = Client::start(options(&clock, None, unused));
    client.ask(&hello());
    client.raw(
        ((MAX_REQUEST_FRAME_BYTES + 1) as u32)
            .to_be_bytes()
            .to_vec(),
    );
    let (rest, code, _) = client.eof();
    assert!(rest.is_empty());
    assert_eq!(code, 0);
}

#[test]
fn one_shot_commands_and_budget_violations_are_refused_before_running() {
    let clock = Arc::new(ManualClock::default());
    let mut client = Client::start(options(&clock, None, unused));
    client.ask(&hello());
    let refused = case(
        "responses",
        "token-returning command is refused in a session",
    );
    for argv in [
        json!(["agents", "credential"]),
        json!(["--output", "json", "admin", "url"]),
        json!(["service", "uninstall"]),
        json!(["session"]),
        json!(["tools", "shell"]),
        json!(["--service-runtime-dir", "C:\\x", "service", "rpc", "health"]),
    ] {
        let invoke = frame(9, json!({"invoke": {"argv": argv, "deadline_ms": 1000}}));
        assert_eq!(client.ask(&invoke), refused, "{argv}");
    }
    let too_many: Vec<String> = vec!["x".into(); MAX_INVOKE_ARGS + 1];
    let too_long = vec!["x".repeat(MAX_INVOKE_ARG_BYTES + 1)];
    for invoke in [
        json!({"argv": too_many, "deadline_ms": 1000}),
        json!({"argv": too_long, "deadline_ms": 1000}),
        json!({"argv": ["version"], "stdin": "x".repeat(MAX_INVOKE_STDIN_BYTES + 1), "deadline_ms": 1000}),
        json!({"argv": ["version"], "deadline_ms": 0}),
        json!({"argv": ["version"], "deadline_ms": MAX_INVOKE_DEADLINE_MS + 1}),
    ] {
        let reply = client.ask(&frame(9, json!({"invoke": invoke})));
        assert_eq!(reply, failure(9, "session_invalid_request", false));
    }
    assert_eq!(client.eof().1, 0);
}

#[test]
fn invoked_carries_the_three_channels_and_the_error_line() {
    let clock = Arc::new(ManualClock::default());
    let mut client = Client::start(options(&clock, None, |argv, io| {
        if argv[0] == "search" {
            io.stdout.write_all(b"{\"selected\":null}\n").unwrap();
            return 0;
        }
        let mut stdin = String::new();
        io.stdin.read_to_string(&mut stdin).unwrap();
        assert_eq!(stdin, "{\"draft\":1}");
        let line = ErrorLine::new(
            ExitClass::UnsupportedCapability,
            "not_yet_served",
            false,
            "This command is not served by this AIRK Service version.",
        );
        writeln!(io.stderr, "{}", serde_json::to_string(&line).unwrap()).unwrap();
        5
    }));
    client.ask(&hello());
    let search = case("requests", "invoke without stdin");
    assert_eq!(client.ask(&search), case("responses", "invoked success"));
    let prepare = case("requests", "invoke with stdin");
    assert_eq!(
        client.ask(&prepare),
        case(
            "responses",
            "invoked unsupported capability carries its error line"
        )
    );
    assert_eq!(client.eof().1, 0);
}

#[test]
fn a_response_over_the_budget_is_refused_never_truncated() {
    let clock = Arc::new(ManualClock::default());
    let mut client = Client::start(options(&clock, None, |_, io| {
        let written = io
            .stdout
            .write_all(&vec![b'x'; MAX_RESPONSE_FRAME_BYTES + 1]);
        u8::from(written.is_err()) * 8
    }));
    client.ask(&hello());
    let invoke = frame(
        3,
        json!({"invoke": {"argv": ["search"], "deadline_ms": 1000}}),
    );
    assert_eq!(
        client.ask(&invoke),
        failure(3, "session_response_oversize", false)
    );
    assert_eq!(client.eof().1, 0);
}

#[test]
fn busy_cancel_and_deadline_answer_each_request_once() {
    let clock = Arc::new(ManualClock::default());
    let release = Gate::default();
    release.set(true);
    let held = release.clone();
    let mut client = Client::start(options(&clock, None, move |_, _| {
        held.pass();
        0
    }));
    client.ask(&hello());
    let running = [3, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17];
    for byte in running {
        client.send(&frame(
            byte,
            json!({"invoke": {"argv": ["search"], "deadline_ms": 60000}}),
        ));
    }
    let ninth = frame(
        5,
        json!({"invoke": {"argv": ["search"], "deadline_ms": 60000}}),
    );
    assert_eq!(client.ask(&ninth), case("responses", "busy is retryable"));
    let unknown = frame(0x20, json!({"cancel": {"target": id(0x30)}}));
    assert_eq!(
        client.ask(&unknown),
        failure(0x20, "session_invalid_request", false)
    );
    let cancel = frame(0x21, json!({"cancel": {"target": id(3)}}));
    assert_eq!(
        client.ask(&cancel),
        json!({"id": id(0x21), "outcome": {"status": "cancelled"}})
    );
    clock.advance(60_000);
    let mut answers: Vec<Value> = (0..running.len()).map(|_| client.next()).collect();
    answers.sort_by_key(|answer| answer["id"].as_str().unwrap().to_owned());
    assert_eq!(answers[0], case("responses", "cancelled"));
    for (answer, byte) in answers[1..].iter().zip(&running[1..]) {
        assert_eq!(*answer, failure(*byte, "session_deadline_exceeded", false));
    }
    release.set(false);
    let (rest, code, _) = client.eof();
    assert!(
        rest.is_empty(),
        "a finished command is never answered twice"
    );
    assert_eq!(code, 0);
}

fn close_a0() -> Vec<(Value, Value)> {
    pipe("grant_revoked_on_session_close", &["window_close"], 1)
}

#[test]
fn hello_opens_a_window_renews_it_and_closes_it_without_leaking_the_lease() {
    let clock = Arc::new(ManualClock::default());
    let mut script = pipe("open_lease_held_by_session", &["window_open"], 1);
    script.extend(pipe("renew_and_expiry", &["window_renew"], 1));
    script.extend(close_a0());
    let service = FakeService::new(E1, script.clone());
    let mut options = options(&clock, Some(service.clone()), unused);
    options.capabilities = vec!["window-scope/1".into(), "assistance/1".into()];
    options.executable_sha256 = "3".repeat(64);
    let mut client = Client::start(options);
    let sends = session_steps("open_lease_held_by_session", "send");
    let replies = session_steps("open_lease_held_by_session", "reply");
    assert_eq!(client.ask(&sends[0]), replies[0]);
    clock.advance(9_999);
    clock.advance(1);
    service.wait_calls(2);
    let (rest, code, text) = client.eof();
    assert!(rest.is_empty());
    assert_eq!(code, 0);
    service.finished(&script);
    assert!(
        !text.contains(&"1a".repeat(32)),
        "the lease never reaches a session frame"
    );
}

#[test]
fn scope_grant_release_and_the_control_relay_follow_the_window_plane() {
    let clock = Arc::new(ManualClock::default());
    let mut script = pipe("open_lease_held_by_session", &["window_open"], 1);
    script.extend(pipe(
        "scope_select_in_memory_grant",
        &["window_scope_grant"],
        2,
    ));
    script.extend(pipe("scope_release", &["window_scope_release"], 2));
    script.extend(pipe(
        "relay_refuses_lease_methods",
        &["assistance_windows"],
        1,
    ));
    script.extend(close_a0());
    let service = FakeService::new(E1, script.clone());
    let mut client = Client::start(options(&clock, Some(service.clone()), unused));
    client.ask(&hello());

    let select = session_steps("scope_select_in_memory_grant", "send");
    let granted = session_steps("scope_select_in_memory_grant", "reply");
    let reply = client.ask(&select[0]);
    for field in ["status", "scope_identity", "catalog_revision"] {
        assert_eq!(
            reply["outcome"][field], granted[0]["outcome"][field],
            "{field}"
        );
    }
    assert_eq!(reply["outcome"]["expires_at_ms"], 30001);
    let mut stale = select[0].clone();
    stale["id"] = json!(id(0x0a));
    stale["request"]["scope_select"]["selection"]["index_ids"] =
        json!(["idx_0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f"]);
    assert_eq!(
        client.ask(&stale),
        case("responses", "stale catalog revision refused by the service")
    );

    let release = session_steps("scope_release", "send");
    let released = session_steps("scope_release", "reply");
    assert_eq!(client.ask(&release[0]), released[0]);
    assert_eq!(
        client.ask(&release[0]),
        released[0],
        "release is idempotent"
    );

    let relay_sends = session_steps("relay_refuses_lease_methods", "send");
    let relay_replies = session_steps("relay_refuses_lease_methods", "reply");
    // Seven lease methods refused before the pipe, then W5 relayed.
    assert_eq!(relay_sends.len(), 8);
    assert_eq!(relay_replies.len(), relay_sends.len());
    for (send, reply) in relay_sends.iter().zip(&relay_replies) {
        assert_eq!(client.ask(send), *reply);
    }
    // §3.7: the relay refuses evidence brokerage before the pipe.
    let brokerage = frame(
        0x0c,
        json!({"control": {"method": {"connect": {
        "project": "c".repeat(64)}}}}),
    );
    assert_eq!(
        client.ask(&brokerage),
        json!({"id": id(0x0c), "outcome": {"status": "control", "response": {
            "status": "failure", "code": "console_brokerage_forbidden"}}})
    );
    let (_, code, text) = client.eof();
    assert_eq!(code, 0);
    service.finished(&script);
    assert!(!text.contains(&"1a".repeat(32)));
}

#[test]
fn assistance_drafts_are_delivered_once_and_acknowledged() {
    let clock = Arc::new(ManualClock::default());
    let mut script = pipe("open_lease_held_by_session", &["window_open"], 1);
    script.extend(pipe("drain_and_ack", &["assistance_drain"], 5));
    script.extend(close_a0());
    let service = FakeService::new(E1, script.clone());
    let mut client = Client::start(options(&clock, Some(service.clone()), unused));
    client.ask(&hello());
    let sends = session_steps("drain_and_ack", "send");
    let events = session_steps("drain_and_ack", "event");
    let subscribed = client.ask(&sends[0]);
    assert_eq!(subscribed["outcome"]["status"], "subscribed");
    assert_eq!(subscribed["outcome"]["cursor"]["service_instance"], E1);
    assert_eq!(subscribed["outcome"]["cursor"]["window_id"], id(0xa0));
    let delivered = |client: &mut Client, expected: &[Value]| {
        for event in expected {
            let observed = client.next();
            assert_eq!(observed["event"]["kind"], "assistance_draft");
            assert_eq!(observed["event"]["draft"], event["draft"]);
            assert_eq!(observed["event"]["cursor"]["window_id"], id(0xa0));
        }
    };
    delivered(&mut client, &events[..8]);
    let acknowledged = session_steps("drain_and_ack", "reply");
    assert_eq!(client.ask(&sends[1]), acknowledged[1]);
    delivered(&mut client, &events[8..]);
    clock.advance(1_000);
    service.wait_calls(4);
    let ahead = frame(0x0d, json!({"assistance_ack": {"through": 12}}));
    assert_eq!(
        client.ask(&ahead),
        case("responses", "acknowledgement ahead of delivery refused")
    );
    let through = frame(0x0d, json!({"assistance_ack": {"through": 10}}));
    assert_eq!(
        client.ask(&through),
        case("responses", "drafts acknowledged")
    );
    let (rest, code, _) = client.eof();
    assert_eq!(
        rest,
        vec![case("events", "closing")],
        "repeats are suppressed"
    );
    assert_eq!(code, 0);
    service.finished(&script);
}

#[test]
fn an_overflowing_event_buffer_resyncs_once_and_redrains_from_the_acknowledgement() {
    let clock = Arc::new(ManualClock::default());
    let mut script = pipe("open_lease_held_by_session", &["window_open"], 1);
    let drains = pipe("session_buffer_overflow_resync", &["assistance_drain"], 2);
    script.push(drains[0].clone());
    // The re-drain answers the same sequences; small bodies keep this step
    // from overflowing again while the test reads (sizes are pinned above).
    let (send, mut reply) = drains[1].clone();
    for draft in reply["drafts"].as_array_mut().unwrap() {
        draft["draft"] = json!("x");
    }
    script.push((send, reply));
    script.extend(close_a0());
    let service = FakeService::new(E1, script.clone());
    let mut client = Client::start(options(&clock, Some(service.clone()), unused));
    client.ask(&hello());
    client.gate.set(true);
    client.send(&session_steps("session_buffer_overflow_resync", "send")[0]);
    service.wait_calls(2);
    // A maintenance pass starts only after the previous one was applied.
    clock.advance(1_000);
    service.wait_calls(3);
    client.gate.set(false);
    assert_eq!(client.next()["outcome"]["status"], "subscribed");
    assert_eq!(client.next(), case("events", "overflow resync"));
    for sequence in 1..=4 {
        let event = client.next();
        assert_eq!(event["event"]["kind"], "assistance_draft");
        assert_eq!(event["event"]["draft"]["sequence"], sequence);
    }
    let (rest, code, _) = client.eof();
    assert_eq!(rest, vec![case("events", "closing")]);
    assert_eq!(code, 0);
    service.finished(&script);
}

#[test]
fn handoffs_publish_through_the_window_and_foreign_cursors_are_refused() {
    let clock = Arc::new(ManualClock::default());
    let mut script = pipe("open_lease_held_by_session", &["window_open"], 1);
    script.extend(pipe("handoff_publish_and_read", &["handoff_publish"], 1));
    script.extend(close_a0());
    let service = FakeService::new(E1, script.clone());
    let mut client = Client::start(options(&clock, Some(service.clone()), unused));
    client.ask(&hello());
    let sends = session_steps("handoff_publish_and_read", "send");
    let replies = session_steps("handoff_publish_and_read", "reply");
    assert_eq!(client.ask(&sends[0]), replies[0]);
    let foreign = case("requests", "subscribe resuming a scoped cursor");
    assert_eq!(
        client.ask(&foreign),
        case("responses", "cursor from another window is refused")
    );
    assert_eq!(client.eof().1, 0);
    service.finished(&script);
}

#[test]
fn a_service_restart_fails_the_read_resyncs_once_and_reopens_the_window() {
    let clock = Arc::new(ManualClock::default());
    let restart = "grant_revoked_on_service_restart";
    let mut script = pipe(restart, &["window_open", "window_scope_grant"], 2);
    script.extend(
        pipe(restart, &["window_renew", "window_open"], 3)
            .into_iter()
            .skip(1),
    );
    let lease = "2a".repeat(32);
    script.push((
        json!({"protocol": 1, "expected_instance": E2, "method": {"window_renew": {
            "window_id": id(0xa2), "lease": lease}}}),
        json!({"status": "window_renewed", "ttl_ms": 30000}),
    ));
    script.push((
        json!({"protocol": 1, "expected_instance": E2, "method": {"window_close": {
            "window_id": id(0xa2), "lease": lease}}}),
        json!({"status": "window_closed"}),
    ));
    let service = FakeService::new(E1, script.clone());
    let mut client = Client::start(options(&clock, Some(service.clone()), |_, io| {
        let line = ErrorLine::new(ExitClass::Refused, "service_instance_changed", true, "x");
        writeln!(io.stderr, "{}", serde_json::to_string(&line).unwrap()).unwrap();
        1
    }));
    client.ask(&hello());
    let select = session_steps("scope_select_in_memory_grant", "send");
    assert_eq!(client.ask(&select[0])["outcome"]["status"], "scope");
    *service.instance.lock().unwrap() = E2.parse().unwrap();
    let sends = session_steps(restart, "send");
    let replies = session_steps(restart, "reply");
    let events = session_steps(restart, "event");
    assert_eq!(client.ask(&sends[0]), replies[0]);
    assert_eq!(client.next(), json!({"event": events[0]}));
    service.wait_calls(4);
    clock.advance(10_000);
    service.wait_calls(5);
    let (rest, code, text) = client.eof();
    assert!(rest.is_empty(), "one resync per restart: {rest:?}");
    assert_eq!(code, 0);
    service.finished(&script);
    assert!(!text.contains(&lease) && !text.contains(&"1a".repeat(32)));
}
