// SPDX-License-Identifier: Apache-2.0
//! `airk service session`: the one temporary native helper of an editor window.
//!
//! The session is a client, never the service. It answers framed
//! [`airk_host::session_protocol`] requests on its own stdin/stdout, relays
//! metadata-only control requests exactly like `service console`, and runs
//! allowlisted one-shot CLI commands in-process so that a window starts one
//! `airk` process instead of one per operation. It exits on stdin EOF (after a
//! bounded drain), on a codec failure, on a closed response stream, or when its
//! owner (the extension host) exits.
//!
//! Lifetime and safety rules:
//! - every request id receives exactly one response. A `cancel` is answered
//!   `cancelled` at once (`session_invalid_request` for a target that is not in
//!   flight) and needs no request slot; its target answers `cancelled` once it
//!   has stopped, or at its deadline if it does not stop;
//! - at most [`MAX_IN_FLIGHT`] requests run at once, each on its own
//!   large-stack thread; a command classified [`Admission::Exclusive`]
//!   (installer and other mutating commands) runs alone among exclusive ones;
//! - an elapsed deadline or a cancel answers immediately and cancels the
//!   command cooperatively (its future is dropped at its next await); its slot
//!   stays occupied until the thread has actually finished, so work stays bounded;
//! - on EOF, a codec failure or owner loss (the extension host exited), the
//!   session accepts nothing more, cancels everything except exclusive work that
//!   has already started, lets that work finish within its own deadline (at most
//!   [`MAX_INVOKE_DEADLINE_MS`]) and then exits, even if the work has not stopped;
//! - the frame streams are private duplicates: the process's standard input and
//!   output are redirected (to the null device and to stderr) before any request
//!   runs, so no stray print or inherited child handle can corrupt a frame.
use crate::{cli_io::CliIo, service_ipc, session_protocol::*};
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::ControlMethod,
};
use anyhow::{Context as _, Result};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

/// Shutdown timing; production uses [`Timing::default`].
#[derive(Clone, Copy)]
pub(crate) struct Timing {
    /// Bound for cancelled (non-exclusive) work to stop after shutdown begins.
    pub(crate) drain: Duration,
    /// Owner observation interval (the same cadence as `client_owner::watch`).
    pub(crate) owner_poll: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            drain: Duration::from_secs(5),
            owner_poll: Duration::from_millis(250),
        }
    }
}

/// Reports whether the session's owner has exited.
pub(crate) type OwnerProbe = Box<dyn Fn() -> bool + Send>;

/// When the session exits after shutdown began at `now`: after `drain` for
/// cancelled work, extended to the deadline of every exclusive command that
/// had already started, never beyond the latest such deadline.
fn drain_until(
    now: Instant,
    drain: Duration,
    running_exclusive_deadlines: impl IntoIterator<Item = Instant>,
) -> Instant {
    running_exclusive_deadlines
        .into_iter()
        .fold(now + drain, Instant::max)
}

/// A shutting-down session exits once nothing is in flight or its drain
/// deadline has passed, whether or not the remaining work has stopped.
fn may_exit(now: Instant, until: Instant, in_flight: usize) -> bool {
    in_flight == 0 || now >= until
}

/// How an admitted command may run beside others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// Read-only; runs concurrently up to [`MAX_IN_FLIGHT`].
    Concurrent,
    /// Mutating or installer work; runs alone among exclusive commands and, once
    /// started, may finish within its deadline after stdin EOF.
    Exclusive,
}

/// One admitted command body. It runs on the request's own thread against the
/// captured streams and stops cooperatively when the token is cancelled.
pub type Job = Box<dyn FnOnce(&CliIo, &CancellationToken) -> Result<()> + Send>;

pub enum Prepared {
    /// Parsing alone produced the complete output (help, version, usage errors).
    Complete {
        success: bool,
        stdout: String,
        stderr: String,
    },
    Run {
        admission: Admission,
        job: Job,
    },
}

/// The CLI side of the session: parsing, the exhaustive allowlist and the exact
/// one-shot failure rendering live with the command tree in the binary.
pub trait Invoker: Send + Sync + 'static {
    /// Parse and classify `argv` (program name excluded); `None` refuses it.
    /// Called on the request's own thread with the CLI stack size.
    fn prepare(&self, argv: Vec<String>) -> Option<Prepared>;
    /// Write a failed command's diagnostics exactly as the one-shot process would.
    fn report_failure(&self, error: &anyhow::Error, io: &CliIo);
}

pub struct Options {
    pub parent_watch: bool,
    /// Stack for each request thread (the CLI parser needs its full stack).
    pub stack_bytes: usize,
}

/// The session's own image, as answered by `hello` and used as the control
/// relay's engine pin.
pub struct Identity {
    pub version: String,
    pub executable_sha256: String,
}

/// Serve the process's stdin/stdout until EOF, codec failure or owner loss.
pub fn serve(invoker: Arc<dyn Invoker>, options: Options) -> Result<()> {
    let executable = std::env::current_exe()?;
    let identity = Identity {
        version: env!("CARGO_PKG_VERSION").into(),
        executable_sha256: airk_host::runtime_pair::digest_file(&executable)?,
    };
    let (input, output) = airk_process::private_stdio::private_streams()?;
    let owner = if options.parent_watch {
        let observer = airk_process::client_owner::observe()?;
        Some(Box::new(move || observer.exited()) as OwnerProbe)
    } else {
        None
    };
    serve_streams(
        invoker,
        input,
        output,
        identity,
        options.stack_bytes,
        Timing::default(),
        owner,
    )
}

enum Event {
    Frame(std::io::Result<Option<Value>>),
    OwnerLost,
    Finished {
        id: Incarnation,
        outcome: Box<SessionOutcome>,
    },
}

struct Entry {
    cancel: CancellationToken,
    deadline: Option<Instant>,
    /// Set by the request thread once exclusive work has actually started.
    exclusive_running: Arc<AtomicBool>,
    cancel_requested: bool,
    answered: bool,
}

/// Exclusive commands run one at a time. Waiters wake on release or cancel.
#[derive(Default)]
struct Gate {
    busy: Mutex<bool>,
    changed: Condvar,
}

struct GateGuard<'a>(&'a Gate);
impl Drop for GateGuard<'_> {
    fn drop(&mut self) {
        *self
            .0
            .busy
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
        self.0.changed.notify_all();
    }
}

impl Gate {
    fn enter(&self, cancel: &CancellationToken) -> Option<GateGuard<'_>> {
        let mut busy = self
            .busy
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if cancel.is_cancelled() {
                return None;
            }
            if !*busy {
                *busy = true;
                return Some(GateGuard(self));
            }
            busy = self
                .changed
                .wait(busy)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
    fn wake(&self) {
        // Take the lock so a waiter between its check and wait cannot miss this.
        let _held = self
            .busy
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.changed.notify_all();
    }
}

/// What one request thread owns.
struct Context {
    identity: Arc<Identity>,
    invoker: Arc<dyn Invoker>,
    gate: Arc<Gate>,
    cancel: CancellationToken,
    exclusive_running: Arc<AtomicBool>,
}

struct Session<W: Write> {
    invoker: Arc<dyn Invoker>,
    identity: Arc<Identity>,
    output: Option<W>,
    events: mpsc::Sender<Event>,
    entries: HashMap<Incarnation, Entry>,
    gate: Arc<Gate>,
    stack_bytes: usize,
    greeted: bool,
}

pub(crate) fn serve_streams<R, W>(
    invoker: Arc<dyn Invoker>,
    mut input: R,
    output: W,
    identity: Identity,
    stack_bytes: usize,
    timing: Timing,
    owner: Option<OwnerProbe>,
) -> Result<()>
where
    R: Read + Send + 'static,
    W: Write,
{
    let (events, receiver) = mpsc::channel();
    let frames = events.clone();
    std::thread::Builder::new()
        .name("airk-session-input".into())
        .spawn(move || {
            loop {
                let frame = service_ipc::read_frame_limited::<_, Value>(
                    &mut input,
                    MAX_REQUEST_FRAME_BYTES,
                );
                let stop = !matches!(frame, Ok(Some(_)));
                if frames.send(Event::Frame(frame)).is_err() || stop {
                    break;
                }
            }
        })
        .context("start session input reader")?;
    // Stops the owner observer when this session returns.
    struct Stop(Arc<AtomicBool>);
    impl Drop for Stop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let stop = Stop(Arc::default());
    if let Some(owner) = owner {
        let (lost, stopped) = (events.clone(), stop.0.clone());
        std::thread::Builder::new()
            .name("airk-session-owner".into())
            .spawn(move || {
                while !stopped.load(Ordering::SeqCst) {
                    if owner() {
                        let _ = lost.send(Event::OwnerLost);
                        return;
                    }
                    std::thread::sleep(timing.owner_poll);
                }
            })
            .context("start session owner observer")?;
    }
    let mut session = Session {
        invoker,
        identity: Arc::new(identity),
        output: Some(output),
        events,
        entries: HashMap::new(),
        gate: Arc::default(),
        stack_bytes,
        greeted: false,
    };
    let mut closing: Option<Instant> = None;
    loop {
        if let Some(until) = closing
            && may_exit(Instant::now(), until, session.entries.len())
        {
            drop(stop);
            return Ok(());
        }
        let now = Instant::now();
        let next = session
            .entries
            .values()
            .filter(|entry| !entry.answered)
            .filter_map(|entry| entry.deadline)
            .chain(closing)
            .min();
        let event = match next {
            Some(at) => match receiver.recv_timeout(at.saturating_duration_since(now)) {
                Ok(event) => Some(event),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            },
            None => match receiver.recv() {
                Ok(event) => Some(event),
                Err(_) => return Ok(()),
            },
        };
        match event {
            None => session.expire(Instant::now()),
            Some(Event::Finished { id, outcome }) => {
                if let Some(entry) = session.entries.remove(&id)
                    && !entry.answered
                {
                    let outcome = if entry.cancel_requested {
                        SessionOutcome::Cancelled
                    } else {
                        *outcome
                    };
                    session.respond(id, outcome);
                }
            }
            Some(Event::Frame(Ok(Some(value)))) => {
                if closing.is_none() {
                    session.request(value);
                }
            }
            Some(Event::Frame(Ok(None) | Err(_)) | Event::OwnerLost) => {
                // EOF, a codec failure or owner loss: accept nothing more and drain.
                if closing.is_none() {
                    closing = Some(session.close(timing.drain));
                }
            }
        }
        if session.output.is_none() && closing.is_none() {
            // The response stream is gone; nothing more can be delivered.
            closing = Some(session.close(timing.drain));
        }
    }
}

impl<W: Write> Session<W> {
    fn respond(&mut self, id: Incarnation, outcome: SessionOutcome) {
        let Some(output) = self.output.as_mut() else {
            return;
        };
        let mut bytes = Vec::new();
        let response = SessionResponse { id, outcome };
        if service_ipc::write_frame_limited(&mut bytes, &response, MAX_RESPONSE_FRAME_BYTES)
            .is_err()
        {
            bytes.clear();
            let refused = SessionResponse {
                id,
                outcome: failed(SessionFailure::SessionResponseOversize),
            };
            if service_ipc::write_frame_limited(&mut bytes, &refused, MAX_RESPONSE_FRAME_BYTES)
                .is_err()
            {
                self.output = None;
                return;
            }
        }
        if output
            .write_all(&bytes)
            .and_then(|()| output.flush())
            .is_err()
        {
            self.output = None;
        }
    }

    /// Answer every elapsed deadline and cancel its command cooperatively.
    fn expire(&mut self, now: Instant) {
        let expired: Vec<Incarnation> = self
            .entries
            .iter()
            .filter(|(_, entry)| !entry.answered && entry.deadline.is_some_and(|at| at <= now))
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            let mut cancelled = false;
            if let Some(entry) = self.entries.get_mut(&id) {
                entry.answered = true;
                cancelled = entry.cancel_requested;
                entry.cancel.cancel();
            }
            self.gate.wake();
            // A cancelled command that has not stopped by its deadline answers
            // its cancellation; its effects are unknown either way.
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

    /// Stop accepting work: cancel everything except exclusive work that has
    /// already started, which may finish within its own deadline. Returns the
    /// drain deadline.
    fn close(&mut self, drain: Duration) -> Instant {
        let mut running = Vec::new();
        for entry in self.entries.values_mut() {
            match entry.deadline {
                Some(deadline) if entry.exclusive_running.load(Ordering::SeqCst) => {
                    running.push(deadline);
                }
                _ => entry.cancel.cancel(),
            }
        }
        self.gate.wake();
        drain_until(Instant::now(), drain, running)
    }

    fn request(&mut self, value: Value) {
        let id = value
            .get("id")
            .cloned()
            .and_then(|id| serde_json::from_value::<Incarnation>(id).ok());
        let Ok(request) = serde_json::from_value::<SessionRequest>(value) else {
            match id {
                Some(id) => self.respond(id, failed(SessionFailure::SessionInvalidRequest)),
                // Without an id nothing can be answered: a codec failure.
                None => self.output = None,
            }
            return;
        };
        let id = request.id;
        match request.request {
            SessionMethod::Hello { protocol } => {
                if protocol != SESSION_PROTOCOL {
                    return self.respond(id, failed(SessionFailure::SessionProtocolMismatch));
                }
                self.greeted = true;
                let outcome = SessionOutcome::Hello {
                    protocol: SESSION_PROTOCOL,
                    version: self.identity.version.clone(),
                    executable_sha256: self.identity.executable_sha256.clone(),
                };
                self.respond(id, outcome);
            }
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
                entry.cancel.cancel();
                self.gate.wake();
                self.respond(id, SessionOutcome::Cancelled);
            }
            SessionMethod::Control {
                runtime_dir,
                runtime_sha256,
                method,
            } => {
                let pin = match runtime_sha256.as_deref().map(str::parse::<Sha256Digest>) {
                    None => None,
                    Some(Ok(digest)) => Some(digest.to_string()),
                    Some(Err(_)) => {
                        return self.respond(id, failed(SessionFailure::SessionInvalidRequest));
                    }
                };
                if runtime_dir.is_empty() || runtime_dir.len() > MAX_INVOKE_ARG_BYTES {
                    return self.respond(id, failed(SessionFailure::SessionInvalidRequest));
                }
                self.start(id, None, move |context| {
                    control(context, PathBuf::from(runtime_dir), pin, method)
                });
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
                let deadline = Instant::now() + Duration::from_millis(deadline_ms);
                self.start(id, Some(deadline), move |context| {
                    invoke(context, argv, stdin)
                });
            }
        }
    }

    fn start(
        &mut self,
        id: Incarnation,
        deadline: Option<Instant>,
        work: impl FnOnce(&Context) -> SessionOutcome + Send + 'static,
    ) {
        if self.entries.contains_key(&id) {
            // Ids name exactly one in-flight request; a reuse is malformed.
            return self.respond(id, failed(SessionFailure::SessionInvalidRequest));
        }
        if self.entries.len() >= MAX_IN_FLIGHT {
            return self.respond(id, failed(SessionFailure::SessionBusy));
        }
        let context = Context {
            identity: self.identity.clone(),
            invoker: self.invoker.clone(),
            gate: self.gate.clone(),
            cancel: CancellationToken::new(),
            exclusive_running: Arc::default(),
        };
        let (cancel, exclusive_running) =
            (context.cancel.clone(), context.exclusive_running.clone());
        let events = self.events.clone();
        let spawned = std::thread::Builder::new()
            .name("airk-session-request".into())
            .stack_size(self.stack_bytes)
            .spawn(move || {
                let outcome = work(&context);
                let _ = events.send(Event::Finished {
                    id,
                    outcome: Box::new(outcome),
                });
            });
        if spawned.is_err() {
            return self.respond(id, failed(SessionFailure::SessionBusy));
        }
        self.entries.insert(
            id,
            Entry {
                cancel,
                deadline,
                exclusive_running,
                cancel_requested: false,
                answered: false,
            },
        );
    }
}

fn failed(code: SessionFailure) -> SessionOutcome {
    SessionOutcome::Failed {
        code,
        retryable: code.retryable(),
    }
}

fn control(
    context: &Context,
    runtime_dir: PathBuf,
    pin: Option<String>,
    method: ControlMethod,
) -> SessionOutcome {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        // Resource exhaustion before any request was sent.
        return failed(SessionFailure::SessionBusy);
    };
    let response = runtime.block_on(async {
        tokio::select! {
            response = crate::console_client::relay(
                &runtime_dir,
                method,
                &context.identity.executable_sha256,
                pin.as_deref(),
            ) => Some(response),
            () = context.cancel.cancelled() => None,
        }
    });
    runtime.shutdown_background();
    match response {
        Some(response) => SessionOutcome::Control { response },
        None => SessionOutcome::Cancelled,
    }
}

fn invoke(context: &Context, argv: Vec<String>, stdin: Option<String>) -> SessionOutcome {
    let cancel = &context.cancel;
    let Some(prepared) = context.invoker.prepare(argv) else {
        return failed(SessionFailure::SessionCommandRefused);
    };
    let (admission, job) = match prepared {
        Prepared::Complete {
            success,
            stdout,
            stderr,
        } => {
            return SessionOutcome::Invoked {
                success,
                stdout,
                stderr: tail(stderr.into_bytes()),
            };
        }
        Prepared::Run { admission, job } => (admission, job),
    };
    let _exclusive = match admission {
        Admission::Exclusive => match context.gate.enter(cancel) {
            Some(guard) => {
                // Published before the cancel check below, so a concurrent EOF
                // drain either sees running work or has already cancelled it.
                context.exclusive_running.store(true, Ordering::SeqCst);
                Some(guard)
            }
            None => return SessionOutcome::Cancelled,
        },
        Admission::Concurrent => None,
    };
    if cancel.is_cancelled() {
        return SessionOutcome::Cancelled;
    }
    // The capture bounds retained memory at the frame budget; whether the
    // response fits is judged on the encoded frame in `respond` (never truncated).
    let io = CliIo::captured(
        stdin.map(String::into_bytes),
        MAX_RESPONSE_FRAME_BYTES,
        MAX_INVOKE_STDERR_BYTES,
    );
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job(&io, cancel)))
        .unwrap_or_else(|_| Err(anyhow::anyhow!("AIRK CLI thread panicked")));
    if cancel.is_cancelled() {
        return SessionOutcome::Cancelled;
    }
    let success = match &result {
        Ok(()) => true,
        Err(error) => {
            context.invoker.report_failure(error, &io);
            false
        }
    };
    let output = io.finish();
    if output.stdout_overflow {
        return failed(SessionFailure::SessionResponseOversize);
    }
    SessionOutcome::Invoked {
        success,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: tail(output.stderr),
    }
}

/// The retained stderr tail, starting on a character boundary.
fn tail(mut bytes: Vec<u8>) -> String {
    if bytes.len() > MAX_INVOKE_STDERR_BYTES {
        bytes.drain(..bytes.len() - MAX_INVOKE_STDERR_BYTES);
    }
    let start = bytes
        .iter()
        .position(|byte| byte & 0xc0 != 0x80)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[start..]).into_owned()
}

#[cfg(test)]
mod tests;
