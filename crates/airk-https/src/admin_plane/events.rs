// SPDX-License-Identifier: Apache-2.0
//! Bounded Admin NDJSON observations through the existing canonical Events read.
use super::*;
use airk_core::service_events::EventCursor;
use contract::{EventStreamRequest, StreamLine, StreamResync};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use tokio::sync::mpsc;

pub(super) type Wait = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

struct Permit {
    secrets: Arc<Mutex<Secrets>>,
    cookie: String,
    lease: Arc<AtomicU8>,
    selection: Option<SelectionAuthority>,
}

fn guarded<'a>(
    secrets: &'a Arc<Mutex<Secrets>>,
    selection: Option<&'a SelectionAuthority>,
) -> SecretsGuard<'a> {
    SecretsGuard {
        guard: Some(secrets.lock().unwrap_or_else(|error| error.into_inner())),
        selection,
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut secrets = self
            .secrets
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        secrets.active_streams = secrets.active_streams.saturating_sub(1);
        secrets.sessions.release_stream(&self.cookie, &self.lease);
    }
}

struct StreamBody {
    receiver: mpsc::Receiver<Bytes>,
    queued: Arc<AtomicUsize>,
    terminal: Arc<Mutex<Option<StreamResync>>>,
    permit: Option<Permit>,
    clock: Clock,
    task: tokio::task::AbortHandle,
    stage: u8,
}

fn line(line: StreamLine) -> Bytes {
    let mut bytes = serde_json::to_vec(&line).expect("bounded stream contract serializes");
    bytes.push(b'\n');
    Bytes::from(bytes)
}

impl Body for StreamBody {
    type Data = Bytes;
    type Error = Infallible;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        if self.stage == 3 {
            return Poll::Ready(None);
        }
        if self.stage == 0
            && let Some(permit) = &self.permit
        {
            guarded(&permit.secrets, permit.selection.as_ref())
                .sessions
                .stream_live(&permit.cookie, (self.clock)());
            let reason = match permit.lease.load(Ordering::SeqCst) {
                1 => Some(StreamResync::SessionExpired),
                2 => Some(StreamResync::SessionEnded),
                3 => Some(StreamResync::ServiceRestarted),
                _ => None,
            };
            if let Some(reason) = reason {
                *self
                    .terminal
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = Some(reason);
                self.task.abort();
                self.receiver.close();
                self.stage = 1;
            }
        }
        if self.stage == 0 {
            match self.receiver.poll_recv(cx) {
                Poll::Ready(Some(bytes)) => {
                    self.queued.fetch_sub(bytes.len(), Ordering::SeqCst);
                    return Poll::Ready(Some(Ok(Frame::data(bytes))));
                }
                Poll::Pending => return Poll::Pending,
                Poll::Ready(None) => self.stage = 1,
            }
        }
        if self.stage == 1 {
            self.stage = 2;
            let reason = self
                .terminal
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .take();
            if let Some(reason) = reason {
                return Poll::Ready(Some(Ok(Frame::data(line(StreamLine::Resync { reason })))));
            }
        }
        self.stage = 3;
        self.permit.take();
        Poll::Ready(Some(Ok(Frame::data(line(StreamLine::Closing {})))))
    }
    fn is_end_stream(&self) -> bool {
        self.stage == 3
    }
}

impl Drop for StreamBody {
    fn drop(&mut self) {
        self.task.abort();
    }
}

pub(super) fn open(
    plane: &AdminPlane,
    cookie: &str,
    request: EventStreamRequest,
) -> Result<HttpResponse, AdminCode> {
    plane.require_native_admission()?;
    let cursor = request
        .cursor
        .map(|cursor| {
            cursor
                .parse::<Incarnation>()
                .map_err(|_| AdminCode::AdminMalformedRequest)
        })
        .transpose()?;
    let (lease, cursor, lost, wait) = {
        let mut secrets = plane.secrets();
        if secrets.active_streams >= contract::MAX_STREAMS {
            return Err(AdminCode::AdminStreamLimit);
        }
        let (lease, cursor, lost) =
            secrets
                .sessions
                .reserve_stream(cookie, cursor, (plane.clock)())?;
        secrets.active_streams += 1;
        (lease, cursor, lost, secrets.event_wait.clone())
    };
    let permit = Permit {
        secrets: plane.secrets.clone(),
        cookie: cookie.to_owned(),
        lease: lease.clone(),
        selection: plane.selection.clone(),
    };
    let queued = Arc::new(AtomicUsize::new(0));
    let terminal = Arc::new(Mutex::new(None));
    let (sender, receiver) = mpsc::channel(contract::STREAM_BUFFER_EVENTS);
    let wait = wait.unwrap_or_else(|| {
        Arc::new(|| {
            Box::pin(tokio::time::sleep(Duration::from_millis(
                contract::EVENT_POLL_MS,
            )))
        })
    });
    let source = Source {
        dispatch: plane.dispatch.clone(),
        clock: plane.clock.clone(),
        random: plane.random.clone(),
        secrets: plane.secrets.clone(),
        cookie: cookie.to_owned(),
        lease,
        queued: queued.clone(),
        terminal: terminal.clone(),
        native_admission: plane.native_admission.clone(),
        wait,
        selection: plane.selection.clone(),
    };
    let task = tokio::spawn(async move {
        source.run(sender, cursor, lost).await;
    });
    let body = StreamBody {
        receiver,
        queued,
        terminal,
        permit: Some(permit),
        clock: plane.clock.clone(),
        task: task.abort_handle(),
        stage: 0,
    };
    let mut builder = Response::builder().status(200);
    for (name, value) in contract::response_headers(ResponseKind::Events) {
        builder = builder.header(name, value);
    }
    builder
        .body(body.boxed())
        .map_err(|_| AdminCode::AdminInternal)
}

struct Source {
    dispatch: Dispatch,
    clock: Clock,
    random: Random,
    secrets: Arc<Mutex<Secrets>>,
    cookie: String,
    lease: Arc<AtomicU8>,
    queued: Arc<AtomicUsize>,
    terminal: Arc<Mutex<Option<StreamResync>>>,
    native_admission: Option<Arc<dyn Fn() -> Result<(), AdminCode> + Send + Sync>>,
    wait: Wait,
    selection: Option<SelectionAuthority>,
}

impl Source {
    fn stop(&self, reason: StreamResync) {
        *self
            .terminal
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(reason);
    }
    fn live(&self) -> bool {
        guarded(&self.secrets, self.selection.as_ref())
            .sessions
            .stream_live(&self.cookie, (self.clock)());
        if self
            .native_admission
            .as_ref()
            .is_some_and(|admit| admit().is_err())
        {
            self.lease.store(2, Ordering::SeqCst);
        }
        self.lease.load(Ordering::SeqCst) == 0
    }
    fn send(&self, sender: &mpsc::Sender<Bytes>, value: StreamLine) -> bool {
        let bytes = line(value);
        let len = bytes.len();
        let queued = self.queued.fetch_add(len, Ordering::SeqCst);
        if queued.saturating_add(len) > contract::STREAM_BUFFER_BYTES {
            self.queued.fetch_sub(len, Ordering::SeqCst);
            self.stop(StreamResync::Overflow);
            return false;
        }
        if sender.try_send(bytes).is_err() {
            self.queued.fetch_sub(len, Ordering::SeqCst);
            self.stop(StreamResync::Overflow);
            return false;
        }
        true
    }
    async fn run(self, sender: mpsc::Sender<Bytes>, mut cursor: Option<EventCursor>, lost: bool) {
        if lost {
            self.stop(StreamResync::NativeGap);
            return;
        }
        let started = (self.clock)();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(contract::STREAM_SECONDS);
        loop {
            if !self.live()
                || (self.clock)().saturating_sub(started) >= contract::STREAM_SECONDS * 1000
            {
                return;
            }
            let method = ControlMethod::Events {
                project: None,
                cursor: cursor.clone(),
                limit: contract::NATIVE_EVENT_PAGE_LIMIT,
            };
            let response = match tokio::time::timeout_at(deadline, (self.dispatch)(method)).await {
                Ok(Ok(response)) => response,
                Ok(Err(_)) => {
                    self.stop(StreamResync::NativeGap);
                    return;
                }
                Err(_) => return,
            };
            if !self.live() {
                return;
            }
            let page = match response {
                ControlResponse::Events { page } => page,
                ControlResponse::Failure { code } if code == "event_scope_changed" => {
                    self.stop(StreamResync::ServiceRestarted);
                    return;
                }
                _ => {
                    self.stop(StreamResync::NativeGap);
                    return;
                }
            };
            if cursor
                .as_ref()
                .is_some_and(|cursor| cursor.namespace != page.namespace)
            {
                self.stop(StreamResync::ServiceRestarted);
                return;
            }
            if page.events.len() > contract::NATIVE_EVENT_PAGE_LIMIT
                || page.tail.namespace != page.namespace
                || page.tail.project.is_some()
                || page.tail.after != page.snapshot_through
                || page.tail.through.is_some()
                || page.next.as_ref().is_some_and(|next| {
                    next.namespace != page.namespace
                        || next.project.is_some()
                        || next.through != Some(page.snapshot_through)
                        || page
                            .events
                            .last()
                            .is_none_or(|event| event.sequence != next.after)
                })
            {
                self.stop(StreamResync::NativeGap);
                return;
            }
            if page.retained_prefix_lost {
                if !self.send(
                    &sender,
                    StreamLine::Resync {
                        reason: StreamResync::NativeGap,
                    },
                ) {
                    return;
                }
                cursor = Some(page.tail);
            } else {
                let mut after = cursor.as_ref().map_or(0, |cursor| cursor.after);
                for event in page.events {
                    if event.sequence <= after
                        || (after != 0 && event.sequence != after.saturating_add(1))
                        || event.sequence > page.snapshot_through
                    {
                        self.stop(StreamResync::NativeGap);
                        return;
                    }
                    after = event.sequence;
                    if !self.live() {
                        return;
                    }
                    // This observer is the channel's sole producer. Refuse
                    // overflow before registering a cursor for an event that
                    // cannot enter the bounded delivery queue.
                    if sender.capacity() == 0 {
                        self.stop(StreamResync::Overflow);
                        return;
                    }
                    let token = match mutations::identity(&self.random) {
                        Ok(token) => token,
                        Err(_) => {
                            self.stop(StreamResync::NativeGap);
                            return;
                        }
                    };
                    let position = EventCursor {
                        namespace: page.namespace,
                        project: None,
                        after,
                        through: None,
                    };
                    if guarded(&self.secrets, self.selection.as_ref())
                        .sessions
                        .store_event_cursor(&self.cookie, token, position, (self.clock)())
                        .is_err()
                    {
                        return;
                    }
                    let event = match serde_json::to_value(event) {
                        Ok(event) => event,
                        Err(_) => {
                            self.stop(StreamResync::NativeGap);
                            return;
                        }
                    };
                    if !self.send(
                        &sender,
                        StreamLine::Event {
                            cursor: token.to_string(),
                            event,
                        },
                    ) {
                        return;
                    }
                }
                if let Some(next) = page.next {
                    cursor = Some(next);
                    continue;
                }
                cursor = Some(page.tail);
            }
            if tokio::time::timeout_at(deadline, (self.wait)())
                .await
                .is_err()
            {
                return;
            }
        }
    }
}
