// SPDX-License-Identifier: Apache-2.0
//! Native-held Admin custody. I2 sees only bounded chunks and public failures.
use super::*;
use airk_core::native_admin::{MAX_BODY_BYTES, NativeAdminChunk, NativeAdminSessionSecret};
use std::sync::atomic::AtomicBool;

#[derive(Default)]
pub(super) struct Admin {
    state: Mutex<State>,
    opening: Mutex<()>,
}
#[derive(Default)]
struct State {
    generation: u64,
    closing: bool,
    permanent: bool,
    cache: Option<Cached>,
    pending_close: Option<Cached>,
    requests: HashMap<Incarnation, Arc<Request>>,
}
#[derive(Clone)]
struct Cached {
    instance: Incarnation,
    secret: NativeAdminSessionSecret,
}
pub(super) struct Request {
    id: Incarnation,
    epoch: u64,
    generation: u64,
    deadline_ns: u64,
    cancelled: AtomicBool,
    dispatched: AtomicBool,
    meta: Mutex<Option<(u16, u64, u64)>>,
}
pub(super) struct Close {
    generation: u64,
    cached: Option<Cached>,
    pub(super) active: bool,
}
pub(super) enum Task {
    Call {
        request: Arc<Request>,
        operation: String,
        body: Box<serde_json::value::RawValue>,
    },
    Read {
        target: Incarnation,
        offset: u64,
    },
    Cancel {
        target: Incarnation,
        generation: u64,
    },
    Close(Close),
}
fn refused(code: &str) -> SessionOutcome {
    SessionOutcome::Refused {
        code: code.into(),
        retryable: false,
    }
}
fn unknown() -> SessionOutcome {
    failed(SessionFailure::AdminOutcomeUnknown)
}

impl Admin {
    fn session_ended(&self, generation: u64) {
        let mut state = self.state();
        if state.generation != generation {
            return;
        }
        for request in state.requests.values() {
            request.cancelled.store(true, Ordering::SeqCst);
        }
        state.requests.clear();
        state.cache = None;
        state.pending_close = None;
        state.generation = state.generation.wrapping_add(1);
        state.closing = true;
        state.permanent = true;
    }
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
    fn register(
        &self,
        id: Incarnation,
        epoch: u64,
        deadline_ns: u64,
    ) -> Result<Arc<Request>, Box<SessionOutcome>> {
        let mut state = self.state();
        if state.closing {
            return Err(Box::new(refused("admin_session_closing")));
        }
        if state.requests.contains_key(&id) || state.requests.len() >= MAX_IN_FLIGHT {
            return Err(Box::new(failed(SessionFailure::SessionBusy)));
        }
        let request = Arc::new(Request {
            id,
            epoch,
            generation: state.generation,
            deadline_ns,
            cancelled: AtomicBool::new(false),
            dispatched: AtomicBool::new(false),
            meta: Mutex::new(None),
        });
        state.requests.insert(id, request.clone());
        Ok(request)
    }
    fn request(&self, id: Incarnation) -> Option<Arc<Request>> {
        self.state().requests.get(&id).cloned()
    }
    fn remove(&self, id: Incarnation) {
        self.state().requests.remove(&id);
    }
    fn purge(&self, now: u64) {
        self.state().requests.retain(|_, request| {
            if request.deadline_ns <= now {
                request.cancelled.store(true, Ordering::SeqCst);
                false
            } else {
                true
            }
        });
    }
    pub(super) fn contains(&self, id: Incarnation) -> bool {
        self.state().requests.contains_key(&id)
    }
    pub(super) fn cancel(&self, id: Incarnation) {
        if let Some(request) = self.request(id) {
            request.cancelled.store(true, Ordering::SeqCst);
        }
    }
    pub(super) fn generation_current(&self, generation: u64) -> bool {
        let state = self.state();
        !state.closing && state.generation == generation
    }
    pub(super) fn invalidate(&self, permanent: bool) -> Close {
        let mut state = self.state();
        let active =
            state.cache.is_some() || state.pending_close.is_some() || !state.requests.is_empty();
        for request in state.requests.values() {
            request.cancelled.store(true, Ordering::SeqCst);
        }
        state.requests.clear();
        state.generation = state.generation.wrapping_add(1);
        state.closing = true;
        state.permanent |= permanent;
        let cached = state.cache.take().or_else(|| state.pending_close.clone());
        state.pending_close = cached.clone();
        Close {
            generation: state.generation,
            cached,
            active,
        }
    }
    pub(super) fn verified_context(&self) {
        let mut state = self.state();
        if !state.permanent {
            state.closing = false;
        }
    }
}

impl Worker {
    fn admin_nonchunk(&self, request: &Request, response: ControlResponse) -> Box<SessionOutcome> {
        if matches!(response, ControlResponse::Failure { code } if code == "admin_session_invalid")
        {
            self.shared.admin.session_ended(request.generation);
            Box::new(failed(SessionFailure::AdminSessionEndedOutcomeUnknown))
        } else {
            Box::new(unknown())
        }
    }
    fn admin_live(&self, request: &Request) -> Result<(), Box<SessionOutcome>> {
        if request.cancelled.load(Ordering::SeqCst) {
            return Err(Box::new(SessionOutcome::Cancelled));
        }
        if !self.shared.scope_open(request.epoch)
            || !self.shared.admin.generation_current(request.generation)
        {
            return Err(Box::new(unknown()));
        }
        let now = self
            .shared
            .clock
            .now_ns()
            .map_err(|_| refused("native_monotonic_unavailable"))?;
        if now >= request.deadline_ns {
            return Err(Box::new(failed(SessionFailure::SessionDeadlineExceeded)));
        }
        Ok(())
    }
    fn admin_cached(&self, instance: Incarnation) -> Option<Cached> {
        self.shared
            .admin
            .state()
            .cache
            .as_ref()
            .filter(|cached| cached.instance == instance)
            .cloned()
    }
    fn admin_session(
        &self,
        snapshot: &Snapshot,
        request: &Request,
    ) -> Result<Cached, Box<SessionOutcome>> {
        self.admin_live(request)?;
        let Some(instance) = snapshot.instance else {
            return Err(Box::new(refused("service_unavailable")));
        };
        let Some(service) = &self.service else {
            return Err(Box::new(refused("service_unavailable")));
        };
        let _opening = self
            .shared
            .admin
            .opening
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.admin_live(request)?;
        if let Some(cached) = self.admin_cached(instance) {
            return Ok(cached);
        }
        let response = service
            .request(
                ControlMethod::AdminNativeOpen {
                    deadline_ns: request.deadline_ns,
                },
                instance,
            )
            .map_err(|_| refused("admin_session_unavailable"))?;
        let ControlResponse::AdminNativeOpened {
            session,
            service_instance,
        } = response
        else {
            return Err(Box::new(refused("admin_session_open_refused")));
        };
        let cached = Cached {
            instance: service_instance,
            secret: session,
        };
        if service_instance != instance || self.admin_live(request).is_err() {
            let _ = service.request(
                ControlMethod::AdminNativeClose {
                    session: cached.secret,
                },
                service_instance,
            );
            return Err(Box::new(unknown()));
        }
        let mut state = self.shared.admin.state();
        if state.closing
            || state.generation != request.generation
            || request.cancelled.load(Ordering::SeqCst)
        {
            drop(state);
            let _ = service.request(
                ControlMethod::AdminNativeClose {
                    session: cached.secret,
                },
                instance,
            );
            return Err(Box::new(unknown()));
        }
        state.cache = Some(cached.clone());
        Ok(cached)
    }
    fn admin_chunk(
        &self,
        request: &Request,
        chunk: NativeAdminChunk,
        meta: &mut Option<(u16, u64, u64)>,
    ) -> Result<SessionOutcome, Box<SessionOutcome>> {
        self.admin_live(request)?;
        let end = chunk.validate().map_err(|_| unknown())?;
        let expected = meta.unwrap_or((chunk.http_status, chunk.total_bytes, 0));
        if (chunk.http_status, chunk.total_bytes, chunk.offset) != expected {
            return Err(Box::new(unknown()));
        }
        *meta = Some((chunk.http_status, chunk.total_bytes, end));
        if chunk.done {
            self.shared.admin.remove(request.id);
        }
        Ok(SessionOutcome::AdminChunk { chunk })
    }
    pub(super) fn admin_task(
        &self,
        snapshot: &Snapshot,
        task: Task,
        effects: &mut Effects,
    ) -> SessionOutcome {
        match task {
            Task::Call {
                request,
                operation,
                body,
            } => {
                effects.admin_generation = Some(request.generation);
                effects.admin_deadline_ns = Some(request.deadline_ns);
                let result = (|| {
                    let mut meta = request.meta.lock().unwrap_or_else(PoisonError::into_inner);
                    let cached = self.admin_session(snapshot, &request)?;
                    self.admin_live(&request)?;
                    let service = self
                        .service
                        .as_ref()
                        .ok_or_else(|| refused("service_unavailable"))?;
                    request.dispatched.store(true, Ordering::SeqCst);
                    let response = service
                        .request(
                            ControlMethod::AdminNativeCall {
                                session: cached.secret,
                                request_id: request.id,
                                operation,
                                body,
                                deadline_ns: request.deadline_ns,
                            },
                            cached.instance,
                        )
                        .map_err(|_| unknown())?;
                    let ControlResponse::AdminNativeChunk { chunk } = response else {
                        return Err(self.admin_nonchunk(&request, response));
                    };
                    self.admin_chunk(&request, chunk, &mut meta)
                })();
                match result {
                    Ok(outcome) => outcome,
                    Err(outcome) => {
                        if request.dispatched.load(Ordering::SeqCst) {
                            request.cancelled.store(true, Ordering::SeqCst);
                        } else {
                            self.shared.admin.remove(request.id);
                        }
                        *outcome
                    }
                }
            }
            Task::Read { target, offset } => {
                let Some(request) = self.shared.admin.request(target) else {
                    return refused("admin_response_unavailable");
                };
                effects.admin_generation = Some(request.generation);
                effects.admin_deadline_ns = Some(request.deadline_ns);
                let result = (|| {
                    let mut meta = request.meta.lock().unwrap_or_else(PoisonError::into_inner);
                    self.admin_live(&request)?;
                    if meta.is_none_or(|(_, _, next)| next != offset) {
                        return Err(Box::new(refused("admin_response_offset_mismatch")));
                    }
                    let instance = snapshot
                        .instance
                        .ok_or_else(|| refused("service_unavailable"))?;
                    let cached = self.admin_cached(instance).ok_or_else(unknown)?;
                    self.admin_live(&request)?;
                    let response = self
                        .service
                        .as_ref()
                        .ok_or_else(unknown)?
                        .request(
                            ControlMethod::AdminNativeRead {
                                session: cached.secret,
                                request_id: target,
                                offset,
                            },
                            instance,
                        )
                        .map_err(|_| unknown())?;
                    let ControlResponse::AdminNativeChunk { chunk } = response else {
                        return Err(self.admin_nonchunk(&request, response));
                    };
                    self.admin_chunk(&request, chunk, &mut meta)
                })();
                match result {
                    Ok(outcome) => outcome,
                    Err(outcome) => {
                        request.cancelled.store(true, Ordering::SeqCst);
                        *outcome
                    }
                }
            }
            Task::Cancel { target, generation } => {
                self.shared.admin.cancel(target);
                let Some(instance) = snapshot.instance else {
                    return SessionOutcome::Cancelled;
                };
                let cached = {
                    let state = self.shared.admin.state();
                    if state.generation != generation {
                        return SessionOutcome::Cancelled;
                    }
                    state
                        .cache
                        .as_ref()
                        .filter(|cached| cached.instance == instance)
                        .cloned()
                };
                let Some(cached) = cached else {
                    return SessionOutcome::Cancelled;
                };
                let Some(service) = &self.service else {
                    return unknown();
                };
                match service.request(
                    ControlMethod::AdminNativeCancel {
                        session: cached.secret,
                        request_id: target,
                    },
                    instance,
                ) {
                    Ok(ControlResponse::AdminNativeCancelled {}) => {
                        self.shared.admin.remove(target);
                        SessionOutcome::Cancelled
                    }
                    _ => unknown(),
                }
            }
            Task::Close(ticket) => self.finish_admin_close(ticket),
        }
    }
    pub(super) fn finish_admin_close(&self, ticket: Close) -> SessionOutcome {
        // A pending old open closes itself before releasing this barrier.
        let _opening = self
            .shared
            .admin
            .opening
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let success = match (ticket.cached, self.service.as_ref()) {
            (Some(cached), Some(service)) => match service.request(
                ControlMethod::AdminNativeClose {
                    session: cached.secret,
                },
                cached.instance,
            ) {
                Ok(ControlResponse::AdminNativeClosed {}) => true,
                Ok(ControlResponse::Failure { code }) if code == "admin_session_invalid" => true,
                _ => false,
            },
            (None, _) => true,
            _ => false,
        };
        if success {
            let mut state = self.shared.admin.state();
            if state.generation == ticket.generation {
                state.pending_close = None;
                if !state.permanent {
                    state.closing = false;
                }
            }
            SessionOutcome::AdminClosed {}
        } else {
            unknown()
        }
    }
}

impl Session {
    pub(super) fn admin_call_request(
        &mut self,
        id: Incarnation,
        operation: String,
        body: Box<serde_json::value::RawValue>,
        deadline_ms: u64,
    ) {
        if operation.is_empty()
            || operation.len() > 128
            || body.get().len() > MAX_BODY_BYTES
            || !(1..=MAX_INVOKE_DEADLINE_MS).contains(&deadline_ms)
        {
            return self.respond(id, failed(SessionFailure::SessionInvalidRequest));
        }
        if self.entries.contains_key(&id) || self.entries.len() >= MAX_IN_FLIGHT {
            return self.respond(id, failed(SessionFailure::SessionBusy));
        }
        let deadline = match self.received.ns.and_then(|received| {
            let now = self.shared.clock.now_ns().ok()?;
            self.shared.admin.purge(now);
            deadline_ms
                .checked_mul(1_000_000)
                .and_then(|duration| received.checked_add(duration))
        }) {
            Some(deadline) => deadline,
            None => return self.respond(id, refused("native_monotonic_unavailable")),
        };
        if self
            .shared
            .clock
            .now_ns()
            .ok()
            .is_none_or(|now| now >= deadline)
        {
            return self.respond(id, failed(SessionFailure::SessionDeadlineExceeded));
        }
        let request =
            match self
                .shared
                .admin
                .register(id, self.shared.epoch.load(Ordering::SeqCst), deadline)
            {
                Ok(request) => request,
                Err(outcome) => return self.respond(id, *outcome),
            };
        self.start(
            id,
            Some(deadline_ms),
            Work::Admin(Task::Call {
                request,
                operation,
                body,
            }),
        );
        if !self.entries.contains_key(&id) {
            self.shared.admin.cancel(id);
            self.shared.admin.remove(id);
        }
    }
    pub(super) fn admin_cancel_request(&mut self, id: Incarnation, target: Incarnation) {
        if self.entries.contains_key(&id) {
            return self.respond(id, failed(SessionFailure::SessionInvalidRequest));
        }
        if self.entries.len() >= MAX_IN_FLIGHT {
            return self.respond(id, failed(SessionFailure::SessionBusy));
        }
        let Some(request) = self.shared.admin.request(target) else {
            return self.respond(id, SessionOutcome::Cancelled);
        };
        let generation = request.generation;
        self.shared.admin.cancel(target);
        if let Some(entry) = self.entries.get_mut(&target) {
            entry.cancel_requested = true;
        }
        self.start(id, None, Work::Admin(Task::Cancel { target, generation }));
    }
    pub(super) fn close_admin_owner(&mut self, permanent: bool) {
        let ticket = self.shared.admin.invalidate(permanent);
        if !ticket.active {
            return;
        }
        let worker = self.worker();
        let sender = self.sender.clone();
        if std::thread::Builder::new()
            .name("airkctl-admin-close".into())
            .spawn(move || {
                let _ = worker.finish_admin_close(ticket);
                let _ = sender.send(Event::AdminClosed);
            })
            .is_ok()
        {
            self.admin_closing += 1;
        }
    }
    pub(super) fn admin_delivery_current(&self, effects: &Effects) -> bool {
        effects
            .admin_generation
            .is_some_and(|generation| self.shared.admin.generation_current(generation))
            && self.shared.scope_open(effects.epoch)
            && effects
                .admin_deadline_ns
                .is_some_and(|deadline| self.shared.clock.now_ns().is_ok_and(|now| now < deadline))
    }
}
