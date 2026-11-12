// SPDX-License-Identifier: Apache-2.0
//! `airkctl session` (split step 3f.3): the per-window helper of an editor
//! window, session protocol 2 (contracts §3.7, §3.8, §2.5, §2.6; plan §6.5).
//!
//! Frames are length-prefixed JSON (`airk-ipc` codec) on the session's
//! stdin/stdout. Rules kept from protocol 1 (`airk service session`):
//! - `hello` comes first; anything else first is `session_protocol_mismatch`;
//! - every request id receives exactly one reply; at most [`MAX_IN_FLIGHT`]
//!   requests run at once, each on its own thread, and a further one is
//!   `session_busy`; a deadline or a `cancel` answers at once while the slot
//!   stays occupied until the thread has finished;
//! - replies are encoded under [`MAX_RESPONSE_FRAME_BYTES`]; a larger one is
//!   `session_response_oversize`, never truncated;
//! - on EOF, a codec failure, a closed output or owner loss the session
//!   accepts nothing more, closes its window (W3), emits `closing` when
//!   subscribed and exits once in-flight work ends or the drain bound passes.
//!
//! Protocol 2 adds the window plane: `hello` opens a window (W1) whose lease
//! lives only in this process's memory and on pipe frames (never in a session
//! frame, stdout, stderr or a log), renews it (W2) on the injected clock and
//! re-opens it after a restart with one `resync { service_restarted }`. Event
//! subscriptions poll `Events` and drain Assistance (W7) into a bounded buffer
//! (≤ [`MAX_BUFFERED_EVENTS`] events, ≤ [`MAX_BUFFERED_EVENT_BYTES`]); an
//! overflow drops the buffered events and emits one `resync { overflow }`.
//!
//! Seams: [`Clock`] (time and waking), [`Service`] (the I1 pipe), [`Invoker`]
//! (the in-process command line) and the owner probe; tests drive all four.
use crate::contract::{
    ErrorLine, EventTopic, ExitClass, I2_CONTRACT, MAX_BUFFERED_EVENT_BYTES, MAX_BUFFERED_EVENTS,
    MAX_IN_FLIGHT, MAX_INVOKE_ARG_BYTES, MAX_INVOKE_ARGS, MAX_INVOKE_DEADLINE_MS,
    MAX_INVOKE_STDERR_BYTES, MAX_INVOKE_STDIN_BYTES, MAX_REQUEST_FRAME_BYTES,
    MAX_RESPONSE_FRAME_BYTES, ResyncReason, SESSION_PROTOCOL, SessionCursor, SessionEvent,
    SessionEventFrame, SessionFailure, SessionMethod, SessionOutcome, SessionRequest,
    SessionResponse, WindowSelection,
};
use airk_core::{
    identity::Incarnation,
    index_catalog::CatalogRevision,
    service::{AssistanceDraft, ControlMethod, ControlResponse, WindowLease, WindowScopeSelection},
    service_events::{EventCursor, EventKind, EventPage, ServiceEvent},
};
use serde_json::Value;
use std::{
    collections::{HashMap, VecDeque},
    ffi::OsString,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// W7 drain size (contracts §2.5: `limit ≤ 8`).
const ASSISTANCE_DRAIN_LIMIT: usize = 8;
/// `Events` page size; the service accepts 1..=32.
const EVENT_PAGE_LIMIT: usize = 32;
/// Stack of each request thread: the command-line parser needs a full stack.
const REQUEST_STACK_BYTES: usize = 8 * 1024 * 1024;
/// Commands whose first word and a later word name a one-shot only (§3.8):
/// they return bearer tokens, connection grants or launch codes, are
/// pipe-only, or would nest an interactive loop.
const REFUSED_COMMANDS: &[(&str, &str)] = &[
    ("agents", "credential"),
    ("admin", "url"),
    ("admin", "open"),
    ("service", "uninstall"),
    ("tools", "shell"),
    // Relays any I1 method, so it would bypass the `control` refusals.
    ("service", "rpc"),
];
/// Global options of `airkctl` that take a separate value.
const VALUE_OPTIONS: &[&str] = &[
    "--output",
    "--service-runtime-dir",
    "--service-project",
    "--catalog-session",
    "--combined-session",
    "--project-ids",
    "--index-ids",
    "--member-availability",
    "--root",
    "--data-dir",
    "--collection",
];

/// Wakes the session loop (a manual clock calls it after advancing).
pub type Wake = Box<dyn Fn() + Send + Sync>;

/// The session's time source. Decision logic reads only this clock.
pub trait Clock: Send + Sync {
    /// Milliseconds on a monotonic scale.
    fn now_ms(&self) -> u64;
    /// The real time to block for `ms` of clock time; `None` blocks until the
    /// waker given to [`Clock::attach`] runs (a manual clock).
    fn real_wait(&self, ms: u64) -> Option<Duration>;
    /// Receive the session's waker; a manual clock calls it after advancing.
    fn attach(&self, wake: Wake);
}

/// The process's monotonic clock.
pub struct SystemClock(Instant);

impl Default for SystemClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        u64::try_from(self.0.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
    fn real_wait(&self, ms: u64) -> Option<Duration> {
        Some(Duration::from_millis(ms))
    }
    fn attach(&self, _wake: Wake) {}
}

/// Session timing; production uses [`Timing::default`].
#[derive(Clone)]
pub struct Timing {
    pub clock: Arc<dyn Clock>,
    /// Bound for in-flight work to end after shutdown begins.
    pub drain_ms: u64,
    /// Owner observation interval.
    pub owner_poll_ms: u64,
    /// W2 renewal interval (TTL 30 s, renewed every 10 s).
    pub renew_every_ms: u64,
    /// `Events` poll and W7 drain interval while subscribed.
    pub event_poll_ms: u64,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            clock: Arc::new(SystemClock::default()),
            drain_ms: 5_000,
            owner_poll_ms: 250,
            renew_every_ms: 10_000,
            event_poll_ms: 1_000,
        }
    }
}

/// Reports whether the session's owner (the editor host) has exited.
pub type OwnerProbe = Box<dyn Fn() -> bool + Send>;

/// The I1 pipe, as the session uses it. Every call except `instance` is
/// fenced to the service instance the session observed.
pub(crate) trait Service: Send + Sync {
    /// `Health`, unfenced: the running instance (identity checked by `airk-ipc`).
    fn instance(&self) -> Result<Incarnation, ErrorLine>;
    fn request(
        &self,
        method: ControlMethod,
        expected: Incarnation,
    ) -> Result<ControlResponse, ErrorLine>;
}

/// Runs one `airkctl` command line in process (the program name first).
pub(crate) trait Invoker: Send + Sync {
    fn run(&self, arguments: Vec<OsString>, io: &mut crate::cli::Io) -> u8;
}

type Connect = Arc<dyn Fn() -> Option<Arc<dyn Service>> + Send + Sync>;

pub struct Options {
    /// `--service-runtime-dir`; without it the install record's control
    /// directory is found at `hello`.
    pub control_root: Option<PathBuf>,
    pub version: String,
    pub executable_sha256: String,
    /// Capabilities of this pair, answered by `hello` once a service is located.
    pub capabilities: Vec<String>,
    /// W1 window label (≤ 128 bytes).
    pub label: String,
    /// W1 `owner_pid`; diagnostic only.
    pub owner_pid: u32,
    /// Exit when the owner exits ([`serve`] installs the platform probe).
    pub owner_watch: bool,
    /// `AIRK_MCP_RESPONSE_MODE=full` for invoked commands.
    pub full_responses: bool,
    pub timing: Timing,
    pub(crate) owner: Option<OwnerProbe>,
    pub(crate) connect: Option<Connect>,
    pub(crate) invoker: Option<Arc<dyn Invoker>>,
}

impl Options {
    pub fn new(version: impl Into<String>, executable_sha256: impl Into<String>) -> Self {
        Self {
            control_root: None,
            version: version.into(),
            executable_sha256: executable_sha256.into(),
            capabilities: Vec::new(),
            label: "airkctl session".into(),
            owner_pid: std::process::id(),
            owner_watch: false,
            full_responses: false,
            timing: Timing::default(),
            owner: None,
            connect: None,
            invoker: None,
        }
    }
}

/// Serve the process's stdin/stdout until EOF, codec failure or owner loss.
pub fn serve(mut options: Options) -> u8 {
    if options.owner_watch && options.owner.is_none() {
        options.owner = owner_probe();
        if options.owner.is_none() {
            // Owner loss could not be observed; refuse rather than outlive it.
            return crate::contract::ExitClass::Internal.code();
        }
    }
    // Frames move to private, non-inheritable duplicates; the standard
    // handles then read the null device and write to stderr, so no stray
    // print or child (a detached service start) can corrupt a frame.
    match airk_process::private_stdio::private_streams() {
        Ok((input, output)) => serve_streams(input, output, options),
        Err(_) => crate::contract::ExitClass::Internal.code(),
    }
}

/// The owner (the editor's extension host) as `airk service session`
/// observed it (`airk_process::client_owner`).
fn owner_probe() -> Option<OwnerProbe> {
    let observer = airk_process::client_owner::observe().ok()?;
    Some(Box::new(move || observer.exited()))
}

/// Serve framed requests from `input` and write replies and events to
/// `output`. Returns the process exit code.
pub fn serve_streams<R, W>(input: R, output: W, options: Options) -> u8
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    let Options {
        control_root,
        version,
        executable_sha256,
        capabilities,
        label,
        owner_pid,
        owner_watch: _,
        full_responses,
        timing,
        owner,
        connect,
        invoker,
    } = options;
    let (sender, receiver) = mpsc::channel();
    let waker = sender.clone();
    timing.clock.attach(Box::new(move || {
        let _ = waker.send(Event::Wake);
    }));
    let frames = sender.clone();
    let mut input = input;
    let reader = std::thread::Builder::new()
        .name("airkctl-session-input".into())
        .spawn(move || {
            loop {
                let frame = airk_ipc::frame::read_frame_limited::<_, Value>(
                    &mut input,
                    MAX_REQUEST_FRAME_BYTES,
                );
                let stop = !matches!(frame, Ok(Some(_)));
                if frames.send(Event::Frame(frame)).is_err() || stop {
                    break;
                }
            }
        });
    let outbox = Arc::new(Outbox::default());
    let (writes, closed) = (outbox.clone(), sender.clone());
    let writer = std::thread::Builder::new()
        .name("airkctl-session-output".into())
        .spawn(move || write_loop(&writes, output, &closed));
    if reader.is_err() || writer.is_err() {
        return ExitClass::Internal.code();
    }
    let connect = connect.unwrap_or_else(|| {
        let (root, version) = (control_root.clone(), version.clone());
        Arc::new(move || locate(root.clone(), &version))
    });
    let now = timing.clock.now_ms();
    let session = Session {
        shared: Arc::new(Shared {
            version,
            executable_sha256,
            capabilities,
            label,
            owner_pid,
            full_responses,
            explicit_root: control_root,
            invoker: invoker.unwrap_or_else(|| Arc::new(CommandLine)),
            epoch: AtomicU64::new(0),
        }),
        owner_at: owner.as_ref().map(|_| now + timing.owner_poll_ms),
        owner,
        timing,
        connect,
        sender,
        outbox,
        entries: HashMap::new(),
        greeted: false,
        service: None,
        instance: None,
        window: None,
        scope: None,
        delivered: 0,
        acked: None,
        resync_sent: false,
        subscription: None,
        maintaining: false,
        suspect: false,
        renew_at: u64::MAX,
        poll_at: u64::MAX,
        closing: None,
        window_closing: false,
    };
    session.run(&receiver)
}

/// The installed service: `--service-runtime-dir`, else the install record.
fn locate(root: Option<PathBuf>, version: &str) -> Option<Arc<dyn Service>> {
    let root = match root {
        Some(root) => root,
        None => match crate::install::discover() {
            crate::install::Discovery::Verified(verified) => {
                PathBuf::from(&verified.record.locations.state)
            }
            _ => return None,
        },
    };
    Some(Arc::new(Pipe {
        root,
        version: version.to_owned(),
    }))
}

struct Pipe {
    root: PathBuf,
    version: String,
}

impl Service for Pipe {
    fn instance(&self) -> Result<Incarnation, ErrorLine> {
        match crate::service::request(&self.root, ControlMethod::Health, &self.version)? {
            ControlResponse::Health { observation } => Ok(observation.instance),
            other => Err(crate::service::refusal(&other)),
        }
    }
    fn request(
        &self,
        method: ControlMethod,
        expected: Incarnation,
    ) -> Result<ControlResponse, ErrorLine> {
        crate::service::request_fenced(&self.root, method, &self.version, Some(expected))
    }
}

struct CommandLine;

impl Invoker for CommandLine {
    fn run(&self, arguments: Vec<OsString>, io: &mut crate::cli::Io) -> u8 {
        crate::cli::run(arguments, io)
    }
}

enum Event {
    Frame(std::io::Result<Option<Value>>),
    Wake,
    Finished {
        id: Incarnation,
        outcome: Box<SessionOutcome>,
        effects: Box<Effects>,
    },
    Maintained(Box<Effects>),
    WindowClosed,
    OutputClosed,
}

/// Frames waiting for the output stream, in order. Only events count toward
/// the event budget; replies are never dropped.
#[derive(Default)]
struct Outbox {
    queue: Mutex<Queue>,
    changed: Condvar,
}

#[derive(Default)]
struct Queue {
    frames: VecDeque<(Vec<u8>, bool)>,
    events: usize,
    event_bytes: usize,
    writing: bool,
    closed: bool,
    broken: bool,
}

impl Outbox {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn reply(&self, bytes: Vec<u8>) {
        let mut queue = self.lock();
        if !queue.broken {
            queue.frames.push_back((bytes, false));
            self.changed.notify_all();
        }
    }

    /// Queue one event. On overflow the buffered events are dropped and one
    /// `resync { overflow }` takes their place; returns whether that happened.
    fn event(&self, bytes: Vec<u8>) -> bool {
        let mut queue = self.lock();
        if queue.broken {
            return false;
        }
        let overflow = queue.events + 1 > MAX_BUFFERED_EVENTS
            || queue.event_bytes + bytes.len() > MAX_BUFFERED_EVENT_BYTES;
        let bytes = if overflow {
            queue.frames.retain(|(_, event)| !event);
            queue.events = 0;
            queue.event_bytes = 0;
            match encode_event(SessionEvent::Resync {
                reason: ResyncReason::Overflow,
            }) {
                Some(resync) => resync,
                None => return true,
            }
        } else {
            bytes
        };
        queue.events += 1;
        queue.event_bytes += bytes.len();
        queue.frames.push_back((bytes, true));
        self.changed.notify_all();
        overflow
    }

    fn close(&self) {
        self.lock().closed = true;
        self.changed.notify_all();
    }

    /// Wait until every queued frame is written, the output broke, or
    /// `timeout` passes (`None`: no bound, for manual clocks).
    fn flushed(&self, timeout: Option<Duration>) {
        let until = timeout.map(|timeout| Instant::now() + timeout);
        let mut queue = self.lock();
        while (!queue.frames.is_empty() || queue.writing) && !queue.broken {
            queue = match until {
                None => self
                    .changed
                    .wait(queue)
                    .unwrap_or_else(PoisonError::into_inner),
                Some(until) => {
                    let now = Instant::now();
                    if now >= until {
                        return;
                    }
                    self.changed
                        .wait_timeout(queue, until - now)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                }
            };
        }
    }
}

fn write_loop<W: Write>(outbox: &Outbox, mut output: W, closed: &mpsc::Sender<Event>) {
    loop {
        let bytes = {
            let mut queue = outbox.lock();
            loop {
                if let Some((bytes, event)) = queue.frames.pop_front() {
                    if event {
                        queue.events -= 1;
                        queue.event_bytes -= bytes.len();
                    }
                    queue.writing = true;
                    break bytes;
                }
                if queue.closed {
                    return;
                }
                queue = outbox
                    .changed
                    .wait(queue)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };
        let written = output.write_all(&bytes).and_then(|()| output.flush());
        let mut queue = outbox.lock();
        queue.writing = false;
        if written.is_err() {
            queue.broken = true;
            queue.frames.clear();
            queue.events = 0;
            queue.event_bytes = 0;
            drop(queue);
            outbox.changed.notify_all();
            let _ = closed.send(Event::OutputClosed);
            return;
        }
        drop(queue);
        outbox.changed.notify_all();
    }
}

fn encode<T: serde::Serialize>(value: &T, limit: usize) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    airk_ipc::frame::write_frame_limited(&mut bytes, value, limit)
        .ok()
        .map(|()| bytes)
}

fn encode_event(event: SessionEvent) -> Option<Vec<u8>> {
    encode(&SessionEventFrame { event }, MAX_BUFFERED_EVENT_BYTES)
}

fn failed(code: SessionFailure) -> SessionOutcome {
    SessionOutcome::Failed {
        code,
        retryable: code.retryable(),
    }
}

/// The refusal of a read under a stale, replaced or released W4 grant.
const SCOPE_UNAVAILABLE: &str = "scoped_read_unavailable";

/// The window-scope read routing of a snapshot holding a W4 grant.
fn window_route(snapshot: &Snapshot) -> Option<crate::cli::WindowRoute> {
    let (Some(instance), Some(window), Some(_)) =
        (snapshot.instance, &snapshot.window, &snapshot.scope)
    else {
        return None;
    };
    Some(crate::cli::WindowRoute {
        window_id: window.id,
        lease: window.lease.clone(),
        instance,
        refused: None,
    })
}

/// Codes after which the window or the instance the session holds is gone.
fn restart_code(code: &str) -> bool {
    matches!(
        code,
        "service_instance_changed" | "window_unknown" | "window_lease_invalid"
    )
}

/// The session's window: the lease never leaves this process except on pipe
/// frames to the owning service.
#[derive(Clone)]
struct Window {
    id: Incarnation,
    lease: WindowLease,
}

/// What a worker sees of the window plane when it starts.
#[derive(Clone)]
struct Snapshot {
    epoch: u64,
    instance: Option<Incarnation>,
    window: Option<Window>,
    acked: Option<u64>,
    /// The held W4 grant's selection.
    scope: Option<WindowScopeSelection>,
}

/// What a worker observed, applied by the session loop. Everything except
/// `reopened` is discarded when the plane changed (another `epoch`) meanwhile.
#[derive(Default)]
struct Effects {
    epoch: u64,
    /// A fenced call reported another instance or a lost window.
    restart_suspected: bool,
    /// Upkeep found another instance or window (`None`: none could be opened).
    reopened: Option<(Incarnation, Option<Window>)>,
    /// Renewal confirmed the held window.
    renewed: bool,
    scoped: Option<bool>,
    /// With `scoped: Some(true)`: the selection now granted.
    granted: Option<WindowScopeSelection>,
    acked: Option<u64>,
    drafts: Vec<AssistanceDraft>,
    subscribed: Option<(Vec<EventTopic>, EventCursor)>,
    events: Vec<ServiceEvent>,
    cursor: Option<EventCursor>,
    gap: bool,
}

enum Work {
    Invoke {
        argv: Vec<String>,
        stdin: Option<String>,
    },
    Control(ControlMethod),
    Subscribe {
        topics: Vec<EventTopic>,
        native: Option<EventCursor>,
    },
    Scope(WindowScopeSelection),
    ScopeRelease,
    Ack(u64),
    Handoff {
        handoff: Value,
        ttl_ms: u64,
    },
}

enum Relay {
    Forward,
    Refuse,
    Brokerage,
}

/// The `control` relay carries metadata only (§3.7, §3.8, §2.5): no
/// connection grant, upgrade checkpoint, enrollment or lease-bearing method.
fn relay_rule(method: &ControlMethod) -> Relay {
    use ControlMethod as M;
    match method {
        M::Connect { .. } | M::ConnectScoped { .. } => Relay::Brokerage,
        M::PrepareUpgradeCheckpoint { .. }
        | M::ObserveUpgradeCheckpoint { .. }
        | M::UpgradeRecoveryProbe { .. }
        | M::ReleaseUpgradeAdmission { .. }
        | M::HttpEnroll { .. }
        | M::WindowOpen { .. }
        | M::WindowRenew { .. }
        | M::WindowClose { .. }
        | M::WindowScopeGrant { .. }
        | M::WindowScopeRelease { .. }
        | M::AssistanceDrain { .. }
        | M::HandoffPublish { .. }
        // A1 is owner pipe only (contracts §2.7, §3.8); A2 belongs to an
        // admin session, which a window session never is.
        | M::AdminLaunchCode {}
        | M::AdminScopeGrant { .. } => Relay::Refuse,
        _ => Relay::Forward,
    }
}

/// Whether `argv` (program name excluded) names a one-shot-only command.
fn refused_command(argv: &[String]) -> bool {
    let mut words = Vec::new();
    let mut arguments = argv.iter();
    while let Some(argument) = arguments.next() {
        if argument == "--" {
            words.extend(arguments.by_ref().map(String::as_str));
        } else if argument.starts_with('-') {
            if VALUE_OPTIONS.contains(&argument.as_str()) {
                arguments.next();
            }
        } else {
            words.push(argument.as_str());
        }
    }
    match words.split_first() {
        Some((&"session", _)) => true,
        Some((group, rest)) => REFUSED_COMMANDS
            .iter()
            .any(|(first, second)| first == group && rest.contains(second)),
        None => false,
    }
}

fn scope_selection(selection: WindowSelection) -> Option<WindowScopeSelection> {
    Some(WindowScopeSelection {
        namespace: selection.namespace,
        expected_catalog_revision: CatalogRevision::try_from(selection.expected_catalog_revision)
            .ok()?,
        index_ids: selection
            .index_ids
            .iter()
            .map(|id| id.parse().ok())
            .collect::<Option<_>>()?,
        project_ids: selection
            .project_ids
            .iter()
            .map(|id| id.parse().ok())
            .collect::<Option<_>>()?,
    })
}

fn topic_of(kind: EventKind) -> EventTopic {
    match kind {
        EventKind::JobPending
        | EventKind::JobRunning
        | EventKind::JobSucceeded
        | EventKind::JobFailed
        | EventKind::JobCanceled
        | EventKind::CancellationRequested
        | EventKind::JobRetryRequested => EventTopic::Jobs,
        EventKind::Registered
        | EventKind::Paused
        | EventKind::Resumed
        | EventKind::WatchEnabled
        | EventKind::WatchDisabled => EventTopic::Status,
    }
}

/// Captured stdout: bounded at the response budget; beyond it the write
/// fails (the command reports an internal error) and the reply is
/// `session_response_oversize`.
struct Bounded {
    bytes: Vec<u8>,
    overflow: bool,
}

impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.overflow || bytes.len() > MAX_RESPONSE_FRAME_BYTES - self.bytes.len() {
            self.overflow = true;
            return Err(std::io::Error::other("session response budget exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Captured stderr: only the last [`MAX_INVOKE_STDERR_BYTES`] are kept.
#[derive(Default)]
struct Tail(Vec<u8>);

impl Write for Tail {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(bytes);
        if self.0.len() > 2 * MAX_INVOKE_STDERR_BYTES {
            self.0.drain(..self.0.len() - MAX_INVOKE_STDERR_BYTES);
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Tail {
    /// The retained tail, starting on a character boundary.
    fn finish(mut self) -> String {
        if self.0.len() > MAX_INVOKE_STDERR_BYTES {
            self.0.drain(..self.0.len() - MAX_INVOKE_STDERR_BYTES);
        }
        let start = self
            .0
            .iter()
            .position(|byte| byte & 0xc0 != 0x80)
            .unwrap_or(self.0.len());
        String::from_utf8_lossy(&self.0[start..]).into_owned()
    }
}

/// Move a final `--output json` error line (§3.3) from stderr into `error`.
fn split_error_line(stderr: String) -> (String, Option<ErrorLine>) {
    let body = stderr.strip_suffix('\n').unwrap_or(&stderr);
    let start = body.rfind('\n').map_or(0, |index| index + 1);
    match serde_json::from_str::<ErrorLine>(&body[start..]) {
        Ok(line) if line.validate().is_ok() => (stderr[..start].to_owned(), Some(line)),
        _ => (stderr, None),
    }
}

struct Shared {
    version: String,
    executable_sha256: String,
    capabilities: Vec<String>,
    label: String,
    owner_pid: u32,
    full_responses: bool,
    explicit_root: Option<PathBuf>,
    invoker: Arc<dyn Invoker>,
    /// The plane epoch, readable by workers to tell a restart happened.
    epoch: AtomicU64,
}

/// What one worker thread owns.
#[derive(Clone)]
struct Worker {
    shared: Arc<Shared>,
    service: Option<Arc<dyn Service>>,
}

enum Upkeep {
    Same,
    Unreachable,
    Reopened(Incarnation, Option<Window>),
}

fn open(shared: &Shared, service: &dyn Service, instance: Incarnation) -> Option<Window> {
    let method = ControlMethod::WindowOpen {
        label: shared.label.clone(),
        owner_pid: shared.owner_pid,
    };
    match service.request(method, instance) {
        Ok(ControlResponse::WindowOpened {
            window_id, lease, ..
        }) => Some(Window {
            id: window_id,
            lease,
        }),
        _ => None,
    }
}

fn renew(window: &Window) -> ControlMethod {
    ControlMethod::WindowRenew {
        window_id: window.id,
        lease: window.lease.clone(),
    }
}

impl Worker {
    fn run(&self, snapshot: &Snapshot, work: Work) -> (SessionOutcome, Effects) {
        let mut effects = Effects {
            epoch: snapshot.epoch,
            ..Effects::default()
        };
        let outcome = match work {
            Work::Invoke { argv, stdin } => self.invoke(snapshot, argv, stdin, &mut effects),
            Work::Control(method) => self.control(snapshot, method, &mut effects),
            Work::Subscribe { topics, native } => {
                self.subscribe(snapshot, topics, native, &mut effects)
            }
            Work::Scope(selection) => match self.plane(snapshot, &mut effects, |window| {
                ControlMethod::WindowScopeGrant {
                    window_id: window.id,
                    lease: window.lease.clone(),
                    selection: selection.clone(),
                }
            }) {
                Ok(ControlResponse::WindowScopeGranted {
                    scope_identity,
                    catalog_revision,
                    expires_at_ms,
                }) => {
                    effects.scoped = Some(true);
                    effects.granted = Some(selection);
                    SessionOutcome::Scope {
                        scope_identity: scope_identity.to_string(),
                        catalog_revision: catalog_revision.get(),
                        expires_at_ms,
                    }
                }
                Ok(_) => unexpected(),
                Err(refusal) => refusal.outcome(),
            },
            Work::ScopeRelease => {
                if snapshot.window.is_none() {
                    // Nothing is granted without a window; release is idempotent.
                    effects.scoped = Some(false);
                    SessionOutcome::ScopeReleased
                } else {
                    match self.plane(snapshot, &mut effects, |window| {
                        ControlMethod::WindowScopeRelease {
                            window_id: window.id,
                            lease: window.lease.clone(),
                        }
                    }) {
                        Ok(ControlResponse::WindowScopeReleased {}) => {
                            effects.scoped = Some(false);
                            SessionOutcome::ScopeReleased
                        }
                        Ok(_) => unexpected(),
                        Err(refusal) => refusal.outcome(),
                    }
                }
            }
            Work::Ack(through) => match self.plane(snapshot, &mut effects, |window| {
                ControlMethod::AssistanceDrain {
                    window_id: window.id,
                    lease: window.lease.clone(),
                    ack_through: Some(through),
                    limit: ASSISTANCE_DRAIN_LIMIT,
                }
            }) {
                Ok(ControlResponse::AssistanceDrained { drafts, .. }) => {
                    effects.acked = Some(through);
                    effects.drafts = drafts;
                    SessionOutcome::Acknowledged
                }
                Ok(_) => unexpected(),
                Err(refusal) => refusal.outcome(),
            },
            Work::Handoff { handoff, ttl_ms } => {
                match self.plane(snapshot, &mut effects, |window| {
                    ControlMethod::HandoffPublish {
                        window_id: window.id,
                        lease: window.lease.clone(),
                        handoff,
                        ttl_ms,
                    }
                }) {
                    Ok(ControlResponse::HandoffPublished {
                        handoff_id,
                        expires_at_ms,
                    }) => SessionOutcome::Handoff {
                        handoff_id: handoff_id.to_string(),
                        expires_at_ms,
                    },
                    Ok(_) => unexpected(),
                    Err(refusal) => refusal.outcome(),
                }
            }
        };
        (outcome, effects)
    }

    /// One lease-bearing call; a service refusal becomes `refused`.
    fn plane(
        &self,
        snapshot: &Snapshot,
        effects: &mut Effects,
        method: impl FnOnce(&Window) -> ControlMethod,
    ) -> Result<ControlResponse, Refusal> {
        let (Some(service), Some(instance), Some(window)) =
            (&self.service, snapshot.instance, &snapshot.window)
        else {
            return Err(Refusal {
                code: "window_unknown".into(),
                retryable: true,
            });
        };
        match service.request(method(window), instance) {
            Ok(ControlResponse::Failure { code }) => {
                let restart = restart_code(&code);
                effects.restart_suspected |= restart;
                Err(Refusal {
                    retryable: restart || code == "registry_busy",
                    code,
                })
            }
            Ok(response) => Ok(response),
            Err(line) => Err(Refusal {
                code: line.code,
                retryable: line.retryable,
            }),
        }
    }

    fn control(
        &self,
        snapshot: &Snapshot,
        method: ControlMethod,
        effects: &mut Effects,
    ) -> SessionOutcome {
        let failure = |code: &str| SessionOutcome::Control {
            response: ControlResponse::Failure { code: code.into() },
        };
        let (Some(service), Some(instance)) = (&self.service, snapshot.instance) else {
            return failure("service_unavailable");
        };
        match service.request(method, instance) {
            // Metadata only: a connection grant never crosses the relay.
            Ok(ControlResponse::Connection { .. }) => failure("console_brokerage_forbidden"),
            Ok(response) => {
                if let ControlResponse::Failure { code } = &response {
                    effects.restart_suspected |= code == "service_instance_changed";
                }
                SessionOutcome::Control { response }
            }
            Err(line) => failure(&line.code),
        }
    }

    fn events(
        &self,
        service: &dyn Service,
        instance: Incarnation,
        cursor: Option<EventCursor>,
    ) -> Result<EventPage, String> {
        let method = ControlMethod::Events {
            project: None,
            cursor,
            limit: EVENT_PAGE_LIMIT,
        };
        match service.request(method, instance) {
            Ok(ControlResponse::Events { page }) => Ok(page),
            Ok(ControlResponse::Failure { code }) => Err(code),
            Ok(_) => Err("unexpected_response".into()),
            Err(line) => Err(line.code),
        }
    }

    /// Poll from `cursor`, or start at the current tail without history.
    fn poll(
        &self,
        service: &dyn Service,
        instance: Incarnation,
        cursor: Option<EventCursor>,
        effects: &mut Effects,
    ) -> Result<(), String> {
        let resumed = cursor.is_some();
        match self.events(service, instance, cursor) {
            Ok(page) if resumed => {
                effects.gap |= page.retained_prefix_lost;
                effects.events = page.events;
                effects.cursor = Some(page.next.unwrap_or(page.tail));
                Ok(())
            }
            Ok(page) => {
                effects.cursor = Some(page.tail);
                Ok(())
            }
            Err(code)
                if resumed
                    && matches!(
                        code.as_str(),
                        "event_cursor_expired" | "event_scope_changed"
                    ) =>
            {
                effects.gap = true;
                self.poll(service, instance, None, effects)
            }
            Err(code) => {
                effects.restart_suspected |= restart_code(&code);
                Err(code)
            }
        }
    }

    fn subscribe(
        &self,
        snapshot: &Snapshot,
        topics: Vec<EventTopic>,
        native: Option<EventCursor>,
        effects: &mut Effects,
    ) -> SessionOutcome {
        let (Some(service), Some(instance), Some(window)) =
            (&self.service, snapshot.instance, &snapshot.window)
        else {
            return SessionOutcome::Refused {
                code: "window_unknown".into(),
                retryable: true,
            };
        };
        if let Err(code) = self.poll(service.as_ref(), instance, native.clone(), effects) {
            return SessionOutcome::Refused {
                retryable: restart_code(&code) || code == "service_unavailable",
                code,
            };
        }
        let Some(start) = native.or_else(|| effects.cursor.clone()) else {
            return unexpected();
        };
        effects.subscribed = Some((topics, start.clone()));
        SessionOutcome::Subscribed {
            cursor: SessionCursor {
                service_instance: instance,
                window_id: window.id,
                native: start,
            },
        }
    }

    fn invoke(
        &self,
        snapshot: &Snapshot,
        argv: Vec<String>,
        stdin: Option<String>,
        effects: &mut Effects,
    ) -> SessionOutcome {
        let mut arguments: Vec<OsString> = vec!["airkctl".into()];
        if let Some(root) = &self.shared.explicit_root
            && !argv.iter().any(|argument| {
                argument == "--service-runtime-dir"
                    || argument.starts_with("--service-runtime-dir=")
            })
        {
            arguments.push("--service-runtime-dir".into());
            arguments.push(root.clone().into_os_string());
        }
        arguments.extend(argv.into_iter().map(OsString::from));
        let stdin = stdin.unwrap_or_default();
        // With a W4 grant, a read that names no explicit selection calls M3
        // with `ToolSelection::Window`, fenced to the snapshot's instance
        // (window_plane.json `scope_select_in_memory_grant`).
        let mut route = window_route(snapshot);
        let mut ran = self.command(arguments.clone(), &stdin, route.as_mut());
        // U99 Q4: on `scoped_read_unavailable` caused by a catalog revision
        // change, re-select once at the current revision and retry the read
        // once. The retry's outcome, refused or not, is returned unchanged.
        if route
            .as_ref()
            .is_some_and(|route| route.refused.as_deref() == Some(SCOPE_UNAVAILABLE))
            && self.reselect(snapshot, effects)
        {
            let mut retry = window_route(snapshot);
            ran = self.command(arguments, &stdin, retry.as_mut());
        }
        let (exit_code, stdout, stderr) = ran;
        if stdout.overflow {
            return failed(SessionFailure::SessionResponseOversize);
        }
        // TODO(3f.3): `error` is filled only when the caller passed
        // `--output json` (the error line is then stderr's last line). A text
        // run has no structured line; filling `error` for it needs a cli.rs
        // seam that hands the `ErrorLine` to the caller as well.
        let (stderr, error) = split_error_line(stderr.finish());
        let class = ExitClass::from_code(exit_code).unwrap_or(ExitClass::Internal);
        if let Some(line) = &error
            && restart_code(&line.code)
        {
            effects.restart_suspected = true;
            return failed(SessionFailure::ServiceRestarted);
        }
        let restarted = self.shared.epoch.load(Ordering::SeqCst) != snapshot.epoch;
        if restarted && matches!(class, ExitClass::Refused | ExitClass::ServiceMissing) {
            // A read refused across a restart; mutations keep their own class
            // (an unknown outcome is recovered by receipt, never replayed).
            return failed(SessionFailure::ServiceRestarted);
        }
        SessionOutcome::Invoked {
            exit_code,
            class,
            stdout: String::from_utf8_lossy(&stdout.bytes).into_owned(),
            stderr,
            error,
        }
    }

    /// One in-process command line with captured streams.
    fn command(
        &self,
        arguments: Vec<OsString>,
        stdin: &str,
        window: Option<&mut crate::cli::WindowRoute>,
    ) -> (u8, Bounded, Tail) {
        let mut input = std::io::Cursor::new(stdin.as_bytes());
        let mut stdout = Bounded {
            bytes: Vec::new(),
            overflow: false,
        };
        let mut stderr = Tail::default();
        let exit_code = {
            let mut io = crate::cli::Io {
                stdin: &mut input,
                stdout: &mut stdout,
                stderr: &mut stderr,
                full_responses: self.shared.full_responses,
                window,
            };
            let invoker = &self.shared.invoker;
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                invoker.run(arguments, &mut io)
            }))
            .unwrap_or(ExitClass::Internal.code())
        };
        (exit_code, stdout, stderr)
    }

    /// U99 Q4: whether the held grant's catalog moved to another revision
    /// in the same namespace, and a re-select of the same indexes and
    /// projects at the current revision was granted. Called at most once per
    /// read; a refused re-select leaves the first outcome in place.
    fn reselect(&self, snapshot: &Snapshot, effects: &mut Effects) -> bool {
        let (Some(service), Some(instance), Some(held)) =
            (&self.service, snapshot.instance, &snapshot.scope)
        else {
            return false;
        };
        let Some(index) = held.index_ids.first() else {
            return false;
        };
        let observed = service.request(
            ControlMethod::CatalogDetail {
                entity: airk_core::catalog_management::CatalogEntity::Index { id: *index },
                expected_catalog_revision: None,
            },
            instance,
        );
        let current = match observed {
            Ok(ControlResponse::CatalogDetail { detail, .. })
                if detail.namespace == held.namespace
                    && detail.revision != held.expected_catalog_revision =>
            {
                WindowScopeSelection {
                    expected_catalog_revision: detail.revision,
                    ..held.clone()
                }
            }
            _ => return false,
        };
        match self.plane(snapshot, effects, |window| {
            ControlMethod::WindowScopeGrant {
                window_id: window.id,
                lease: window.lease.clone(),
                selection: current.clone(),
            }
        }) {
            Ok(ControlResponse::WindowScopeGranted { .. }) => {
                effects.scoped = Some(true);
                effects.granted = Some(current);
                true
            }
            _ => false,
        }
    }

    /// Renewal, restart detection, Assistance drain and the event poll.
    fn maintain(&self, snapshot: &Snapshot, plan: &Plan) -> Effects {
        let mut effects = Effects {
            epoch: snapshot.epoch,
            ..Effects::default()
        };
        let Some(service) = &self.service else {
            return effects;
        };
        if plan.renew || plan.suspect || snapshot.window.is_none() {
            match self.upkeep(service.as_ref(), snapshot, plan) {
                Upkeep::Same => effects.renewed = true,
                Upkeep::Unreachable => return effects,
                Upkeep::Reopened(instance, window) => {
                    effects.reopened = Some((instance, window));
                    return effects;
                }
            }
        }
        let (Some(instance), Some(window)) = (snapshot.instance, &snapshot.window) else {
            return effects;
        };
        if plan.drain {
            let method = ControlMethod::AssistanceDrain {
                window_id: window.id,
                lease: window.lease.clone(),
                ack_through: snapshot.acked,
                limit: ASSISTANCE_DRAIN_LIMIT,
            };
            match service.request(method, instance) {
                Ok(ControlResponse::AssistanceDrained { drafts, .. }) => effects.drafts = drafts,
                Ok(ControlResponse::Failure { code }) => {
                    effects.restart_suspected |= restart_code(&code);
                }
                _ => {}
            }
        }
        if plan.poll {
            let _ = self.poll(
                service.as_ref(),
                instance,
                plan.cursor.clone(),
                &mut effects,
            );
        }
        effects
    }

    /// Keep the window: renew it, or find the new instance and open another.
    fn upkeep(&self, service: &dyn Service, snapshot: &Snapshot, plan: &Plan) -> Upkeep {
        let mut instance = snapshot.instance;
        if plan.suspect || instance.is_none() || snapshot.window.is_none() {
            match service.instance() {
                Ok(observed) => instance = Some(observed),
                Err(_) => return Upkeep::Unreachable,
            }
        }
        let Some(mut current) = instance else {
            return Upkeep::Unreachable;
        };
        if let Some(window) = &snapshot.window {
            // Renewal also verifies a suspected window or instance loss.
            let mut answer = service.request(renew(window), current);
            if matches!(&answer, Ok(ControlResponse::Failure { code }) if code == "service_instance_changed")
            {
                match service.instance() {
                    Ok(observed) => current = observed,
                    Err(_) => return Upkeep::Unreachable,
                }
                answer = service.request(renew(window), current);
            }
            match answer {
                Ok(ControlResponse::WindowRenewed { .. }) if Some(current) == snapshot.instance => {
                    return Upkeep::Same;
                }
                // Renewed under another instance cannot happen; treat it as a
                // new window so stale state is dropped.
                Ok(ControlResponse::WindowRenewed { .. }) => {
                    return Upkeep::Reopened(current, Some(window.clone()));
                }
                Ok(ControlResponse::Failure { code })
                    if matches!(code.as_str(), "window_unknown" | "window_lease_invalid") => {}
                _ => return Upkeep::Unreachable,
            }
        }
        Upkeep::Reopened(current, open(&self.shared, service, current))
    }
}

fn unexpected() -> SessionOutcome {
    SessionOutcome::Refused {
        code: "unexpected_response".into(),
        retryable: false,
    }
}

struct Plan {
    renew: bool,
    suspect: bool,
    drain: bool,
    poll: bool,
    cursor: Option<EventCursor>,
}

struct Entry {
    deadline_ms: Option<u64>,
    cancel_requested: bool,
    answered: bool,
}

struct Subscription {
    topics: Vec<EventTopic>,
    native: Option<EventCursor>,
}

struct Session {
    shared: Arc<Shared>,
    timing: Timing,
    owner: Option<OwnerProbe>,
    owner_at: Option<u64>,
    connect: Connect,
    sender: mpsc::Sender<Event>,
    outbox: Arc<Outbox>,
    entries: HashMap<Incarnation, Entry>,
    greeted: bool,
    service: Option<Arc<dyn Service>>,
    instance: Option<Incarnation>,
    window: Option<Window>,
    /// The selection of the held W4 grant: reads with no explicit selection
    /// are routed to it, and it is re-selected once on revision drift (U99
    /// Q4).
    scope: Option<WindowScopeSelection>,
    /// Highest Assistance sequence emitted; repeats are suppressed.
    delivered: u64,
    acked: Option<u64>,
    /// A `resync { service_restarted }` was emitted for the current loss.
    resync_sent: bool,
    subscription: Option<Subscription>,
    maintaining: bool,
    suspect: bool,
    renew_at: u64,
    poll_at: u64,
    closing: Option<u64>,
    window_closing: bool,
}

impl Session {
    fn now(&self) -> u64 {
        self.timing.clock.now_ms()
    }

    fn run(mut self, receiver: &mpsc::Receiver<Event>) -> u8 {
        loop {
            let now = self.now();
            if let Some(until) = self.closing
                && ((self.entries.is_empty() && !self.window_closing) || now >= until)
            {
                self.outbox.close();
                let rest = until.saturating_sub(self.now());
                self.outbox.flushed(self.timing.clock.real_wait(rest));
                return 0;
            }
            self.timers(now);
            let wait = self
                .next_due()
                .map(|at| self.timing.clock.real_wait(at.saturating_sub(now)));
            let event = match wait {
                Some(Some(wait)) => match receiver.recv_timeout(wait) {
                    Ok(event) => Some(event),
                    Err(mpsc::RecvTimeoutError::Timeout) => None,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return 0,
                },
                Some(None) | None => match receiver.recv() {
                    Ok(event) => Some(event),
                    Err(_) => return 0,
                },
            };
            match event {
                None | Some(Event::Wake) => {}
                Some(Event::Frame(Ok(Some(value)))) => {
                    if self.closing.is_none() {
                        self.request(value);
                    }
                }
                Some(Event::Frame(_) | Event::OutputClosed) => self.begin_close(),
                Some(Event::Finished {
                    id,
                    outcome,
                    effects,
                }) => {
                    let accepted = match self.entries.remove(&id) {
                        Some(entry) if !entry.answered => {
                            let outcome = if entry.cancel_requested {
                                SessionOutcome::Cancelled
                            } else {
                                *outcome
                            };
                            let accepted = !entry.cancel_requested;
                            self.respond(id, outcome);
                            accepted
                        }
                        _ => false,
                    };
                    self.apply(*effects, accepted);
                }
                Some(Event::Maintained(effects)) => {
                    self.maintaining = false;
                    self.apply(*effects, false);
                }
                Some(Event::WindowClosed) => self.window_closing = false,
            }
        }
    }

    fn next_due(&self) -> Option<u64> {
        let deadlines = self
            .entries
            .values()
            .filter(|entry| !entry.answered)
            .filter_map(|entry| entry.deadline_ms);
        let mut due = deadlines.chain(self.closing).chain(self.owner_at).min();
        if self.closing.is_none() && self.greeted && self.service.is_some() && !self.maintaining {
            let poll = self.subscription.as_ref().map(|_| self.poll_at);
            due = due.into_iter().chain(Some(self.renew_at)).chain(poll).min();
        }
        due
    }

    fn timers(&mut self, now: u64) {
        if self.closing.is_none()
            && let Some(at) = self.owner_at
            && now >= at
        {
            self.owner_at = Some(now + self.timing.owner_poll_ms);
            if self.owner.as_ref().is_some_and(|probe| probe()) {
                self.begin_close();
            }
        }
        self.expire(now);
        if self.closing.is_some() || !self.greeted || self.service.is_none() || self.maintaining {
            return;
        }
        let renew = now >= self.renew_at;
        let poll = self.subscription.is_some() && now >= self.poll_at;
        if !(renew || poll || self.suspect) {
            return;
        }
        if renew {
            self.renew_at = now + self.timing.renew_every_ms;
        }
        if poll {
            self.poll_at = now + self.timing.event_poll_ms;
        }
        let cursor = self
            .subscription
            .as_ref()
            .and_then(|subscription| subscription.native.clone());
        let plan = Plan {
            renew,
            suspect: std::mem::take(&mut self.suspect),
            drain: poll && self.subscribed(EventTopic::Assistance),
            poll: poll
                && (cursor.is_none()
                    || self.subscribed(EventTopic::Status)
                    || self.subscribed(EventTopic::Jobs)),
            cursor,
        };
        let (worker, snapshot, sender) = (self.worker(), self.snapshot(), self.sender.clone());
        self.maintaining = std::thread::Builder::new()
            .name("airkctl-session-window".into())
            .spawn(move || {
                let effects = worker.maintain(&snapshot, &plan);
                let _ = sender.send(Event::Maintained(Box::new(effects)));
            })
            .is_ok();
    }

    fn worker(&self) -> Worker {
        Worker {
            shared: self.shared.clone(),
            service: self.service.clone(),
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            epoch: self.shared.epoch.load(Ordering::SeqCst),
            instance: self.instance,
            window: self.window.clone(),
            acked: self.acked,
            scope: self.scope.clone(),
        }
    }

    fn subscribed(&self, topic: EventTopic) -> bool {
        self.subscription
            .as_ref()
            .is_some_and(|subscription| subscription.topics.contains(&topic))
    }

    fn respond(&mut self, id: Incarnation, outcome: SessionOutcome) {
        let bytes =
            encode(&SessionResponse { id, outcome }, MAX_RESPONSE_FRAME_BYTES).or_else(|| {
                let outcome = failed(SessionFailure::SessionResponseOversize);
                encode(&SessionResponse { id, outcome }, MAX_RESPONSE_FRAME_BYTES)
            });
        if let Some(bytes) = bytes {
            self.outbox.reply(bytes);
        }
    }

    /// Queue one event; returns whether the buffer overflowed.
    fn emit(&mut self, event: SessionEvent) -> bool {
        let overflow = encode_event(event).is_some_and(|bytes| self.outbox.event(bytes));
        if overflow {
            // Re-drain from the last acknowledgement so no draft is lost.
            self.delivered = self.acked.unwrap_or(0);
        }
        overflow
    }

    /// The window's grant and Assistance state end with its window or instance.
    fn reset_plane(&mut self) {
        self.shared.epoch.fetch_add(1, Ordering::SeqCst);
        self.scope = None;
        self.delivered = 0;
        self.acked = None;
        if let Some(subscription) = self.subscription.as_mut() {
            subscription.native = None;
        }
    }

    fn apply(&mut self, effects: Effects, accepted: bool) {
        if let Some((instance, window)) = effects.reopened {
            let changed = self.instance != Some(instance)
                || self.window.as_ref().map(|window| window.id)
                    != window.as_ref().map(|window| window.id);
            if changed {
                self.reset_plane();
                self.instance = Some(instance);
                self.window = window;
                if !self.resync_sent {
                    self.emit(SessionEvent::Resync {
                        reason: ResyncReason::ServiceRestarted,
                    });
                }
            }
            self.resync_sent = false;
            return;
        }
        if effects.epoch != self.shared.epoch.load(Ordering::SeqCst) {
            return;
        }
        if effects.restart_suspected {
            self.suspect = true;
            if !self.resync_sent {
                self.reset_plane();
                self.resync_sent = true;
                self.emit(SessionEvent::Resync {
                    reason: ResyncReason::ServiceRestarted,
                });
            }
            return;
        }
        if effects.renewed {
            self.resync_sent = false;
        }
        match effects.scoped {
            Some(true) => self.scope = effects.granted,
            Some(false) => self.scope = None,
            None => {}
        }
        if let Some(acked) = effects.acked {
            self.acked = Some(self.acked.map_or(acked, |held| held.max(acked)));
        }
        if accepted && let Some((topics, start)) = effects.subscribed {
            self.subscription = Some(Subscription {
                topics,
                native: Some(start),
            });
            self.poll_at = self.now();
        }
        let Some(subscription) = self.subscription.as_mut() else {
            return;
        };
        let template = subscription
            .native
            .clone()
            .or_else(|| effects.cursor.clone());
        if let Some(cursor) = effects.cursor {
            subscription.native = Some(cursor);
        }
        let (Some(instance), Some(window_id), Some(template)) = (
            self.instance,
            self.window.as_ref().map(|window| window.id),
            template,
        ) else {
            return;
        };
        let scoped = |native: EventCursor| SessionCursor {
            service_instance: instance,
            window_id,
            native,
        };
        if effects.gap
            && self.emit(SessionEvent::Resync {
                reason: ResyncReason::NativeGap,
            })
        {
            return;
        }
        for event in effects.events {
            let topic = topic_of(event.kind);
            if !self.subscribed(topic) {
                continue;
            }
            let cursor = scoped(EventCursor {
                namespace: template.namespace,
                project: template.project,
                after: event.sequence,
                through: None,
            });
            let Ok(value) = serde_json::to_value(&event) else {
                continue;
            };
            let event = match topic {
                EventTopic::Jobs => SessionEvent::Job { cursor, job: value },
                _ => SessionEvent::Status {
                    cursor,
                    status: value,
                },
            };
            if self.emit(event) {
                return;
            }
        }
        if !self.subscribed(EventTopic::Assistance) {
            return;
        }
        let native = self
            .subscription
            .as_ref()
            .and_then(|subscription| subscription.native.clone())
            .unwrap_or(template);
        for draft in effects.drafts {
            if draft.sequence <= self.delivered {
                continue;
            }
            let sequence = draft.sequence;
            let Ok(value) = serde_json::to_value(&draft) else {
                continue;
            };
            let event = SessionEvent::AssistanceDraft {
                cursor: scoped(native.clone()),
                draft: value,
            };
            if self.emit(event) {
                return;
            }
            self.delivered = sequence;
        }
    }

    fn expire(&mut self, now: u64) {
        let expired: Vec<Incarnation> = self
            .entries
            .iter()
            .filter(|(_, entry)| !entry.answered && entry.deadline_ms.is_some_and(|at| at <= now))
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            let mut cancelled = false;
            if let Some(entry) = self.entries.get_mut(&id) {
                entry.answered = true;
                cancelled = entry.cancel_requested;
            }
            self.respond(
                id,
                if cancelled {
                    SessionOutcome::Cancelled
                } else {
                    failed(SessionFailure::SessionDeadlineExceeded)
                },
            );
        }
    }

    /// Accept nothing more, emit `closing`, close the window (W3) and let
    /// in-flight work end within the drain bound. `cli::run` has no
    /// cooperative cancellation, so in-flight commands are abandoned at the
    /// bound; accepted mutations continue under service ownership.
    fn begin_close(&mut self) {
        if self.closing.is_some() {
            return;
        }
        self.closing = Some(self.now() + self.timing.drain_ms);
        if self.subscription.take().is_some() {
            self.emit(SessionEvent::Closing);
        }
        if let (Some(service), Some(instance), Some(window)) =
            (self.service.clone(), self.instance, self.window.take())
        {
            let sender = self.sender.clone();
            self.window_closing = std::thread::Builder::new()
                .name("airkctl-session-close".into())
                .spawn(move || {
                    let method = ControlMethod::WindowClose {
                        window_id: window.id,
                        lease: window.lease,
                    };
                    let _ = service.request(method, instance);
                    let _ = sender.send(Event::WindowClosed);
                })
                .is_ok();
        }
    }

    fn hello(&mut self, id: Incarnation, protocol: u32, contracts: &[u32]) {
        if self.greeted {
            return self.respond(id, failed(SessionFailure::SessionInvalidRequest));
        }
        if protocol != SESSION_PROTOCOL || !contracts.contains(&I2_CONTRACT) {
            return self.respond(id, failed(SessionFailure::SessionProtocolMismatch));
        }
        self.greeted = true;
        // Located once per session; the pipe calls run before the reply so the
        // window is known to every later request.
        self.service = (self.connect)();
        if let Some(service) = self.service.clone()
            && let Ok(instance) = service.instance()
        {
            self.instance = Some(instance);
            self.window = open(&self.shared, service.as_ref(), instance);
        }
        let capabilities = if self.service.is_some() {
            self.shared.capabilities.clone()
        } else {
            Vec::new()
        };
        let outcome = SessionOutcome::Hello {
            protocol: SESSION_PROTOCOL,
            version: self.shared.version.clone(),
            i2_contract: I2_CONTRACT,
            capabilities,
            executable_sha256: self.shared.executable_sha256.clone(),
            service_instance: self.instance,
            window_id: self.window.as_ref().map(|window| window.id),
        };
        self.respond(id, outcome);
        self.renew_at = self.now() + self.timing.renew_every_ms;
    }

    fn request(&mut self, value: Value) {
        let id = value
            .get("id")
            .cloned()
            .and_then(|id| serde_json::from_value::<Incarnation>(id).ok());
        let other_protocol = value
            .pointer("/request/hello/protocol")
            .and_then(Value::as_u64)
            .is_some_and(|protocol| protocol != u64::from(SESSION_PROTOCOL));
        let Ok(request) = serde_json::from_value::<SessionRequest>(value) else {
            match id {
                Some(id) if other_protocol => {
                    self.respond(id, failed(SessionFailure::SessionProtocolMismatch));
                }
                Some(id) => self.respond(id, failed(SessionFailure::SessionInvalidRequest)),
                // Without an id nothing can be answered: a codec failure.
                None => self.begin_close(),
            }
            return;
        };
        let id = request.id;
        match request.request {
            SessionMethod::Hello {
                protocol,
                client: _,
                i2_contracts,
            } => self.hello(id, protocol, &i2_contracts),
            _ if !self.greeted => {
                self.respond(id, failed(SessionFailure::SessionProtocolMismatch));
            }
            SessionMethod::Cancel { target } => {
                let Some(entry) = self
                    .entries
                    .get_mut(&target)
                    .filter(|entry| !entry.answered)
                else {
                    return self.respond(id, failed(SessionFailure::SessionInvalidRequest));
                };
                entry.cancel_requested = true;
                self.respond(id, SessionOutcome::Cancelled);
            }
            SessionMethod::Invoke {
                argv,
                stdin,
                deadline_ms,
            } => {
                let valid = argv.len() <= MAX_INVOKE_ARGS
                    && argv.iter().all(|arg| arg.len() <= MAX_INVOKE_ARG_BYTES)
                    && stdin
                        .as_ref()
                        .is_none_or(|stdin| stdin.len() <= MAX_INVOKE_STDIN_BYTES)
                    && (1..=MAX_INVOKE_DEADLINE_MS).contains(&deadline_ms);
                if !valid {
                    return self.respond(id, failed(SessionFailure::SessionInvalidRequest));
                }
                if refused_command(&argv) {
                    return self.respond(id, failed(SessionFailure::SessionCommandRefused));
                }
                self.start(id, Some(deadline_ms), Work::Invoke { argv, stdin });
            }
            SessionMethod::Control { method } => match relay_rule(&method) {
                Relay::Forward => self.start(id, None, Work::Control(method)),
                Relay::Refuse => self.respond(id, failed(SessionFailure::SessionCommandRefused)),
                Relay::Brokerage => self.respond(
                    id,
                    SessionOutcome::Control {
                        response: ControlResponse::Failure {
                            code: "console_brokerage_forbidden".into(),
                        },
                    },
                ),
            },
            SessionMethod::Subscribe { topics, cursor } => {
                if let Some(cursor) = &cursor
                    && (Some(cursor.service_instance) != self.instance
                        || Some(cursor.window_id) != self.window.as_ref().map(|window| window.id))
                {
                    return self.respond(id, failed(SessionFailure::EventCursorScopeMismatch));
                }
                let native = cursor.map(|cursor| cursor.native);
                self.start(id, None, Work::Subscribe { topics, native });
            }
            SessionMethod::Unsubscribe {} => {
                self.subscription = None;
                self.respond(id, SessionOutcome::Unsubscribed);
            }
            SessionMethod::ScopeSelect { selection } => match scope_selection(selection) {
                Some(selection) => self.start(id, None, Work::Scope(selection)),
                None => self.respond(id, failed(SessionFailure::SessionInvalidRequest)),
            },
            SessionMethod::ScopeRelease {} => self.start(id, None, Work::ScopeRelease),
            SessionMethod::AssistanceAck { through } => self.start(id, None, Work::Ack(through)),
            SessionMethod::HandoffPublish { handoff, ttl_ms } => {
                self.start(id, None, Work::Handoff { handoff, ttl_ms });
            }
        }
    }

    fn start(&mut self, id: Incarnation, deadline_ms: Option<u64>, work: Work) {
        if self.entries.contains_key(&id) {
            // Ids name exactly one in-flight request; a reuse is malformed.
            return self.respond(id, failed(SessionFailure::SessionInvalidRequest));
        }
        if self.entries.len() >= MAX_IN_FLIGHT {
            return self.respond(id, failed(SessionFailure::SessionBusy));
        }
        let (worker, snapshot, sender) = (self.worker(), self.snapshot(), self.sender.clone());
        let spawned = std::thread::Builder::new()
            .name("airkctl-session-request".into())
            .stack_size(REQUEST_STACK_BYTES)
            .spawn(move || {
                let (outcome, effects) = worker.run(&snapshot, work);
                let _ = sender.send(Event::Finished {
                    id,
                    outcome: Box::new(outcome),
                    effects: Box::new(effects),
                });
            });
        if spawned.is_err() {
            return self.respond(id, failed(SessionFailure::SessionBusy));
        }
        let now = self.now();
        self.entries.insert(
            id,
            Entry {
                deadline_ms: deadline_ms.map(|deadline| now + deadline),
                cancel_requested: false,
                answered: false,
            },
        );
    }
}

/// A window-plane refusal, kept small; it becomes `SessionOutcome::Refused`.
struct Refusal {
    code: String,
    retryable: bool,
}

impl Refusal {
    fn outcome(self) -> SessionOutcome {
        SessionOutcome::Refused {
            code: self.code,
            retryable: self.retryable,
        }
    }
}

#[cfg(test)]
mod tests;
