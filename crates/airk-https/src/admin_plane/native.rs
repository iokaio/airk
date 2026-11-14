// SPDX-License-Identifier: Apache-2.0
//! Same-user native entry to the existing Admin plane. No HTTP peer is invented.
use super::*;
use airk_core::native_admin::{
    MAX_BODY_BYTES, MAX_CHUNK_BYTES, MAX_DEADLINE_NS, MAX_RESPONSE_BYTES, NativeAdminChunk,
    NativeAdminSessionSecret,
};
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

/// Rechecked by the service after acquiring canonical admission locks.
pub type NativeDispatchCheck = Arc<dyn Fn() -> Result<(), &'static str> + Send + Sync>;
pub type NativeServiceLive = Arc<dyn Fn() -> bool + Send + Sync>;
pub type NativeDispatch = Arc<
    dyn Fn(
            ControlMethod,
            NativeDispatchCheck,
        ) -> Pin<Box<dyn Future<Output = anyhow::Result<ControlResponse>> + Send>>
        + Send
        + Sync,
>;

type RequestId = ([u8; 32], Incarnation);
#[derive(Default)]
pub(super) struct Requests {
    entries: std::collections::HashMap<RequestId, Entry>,
    charged: Arc<AtomicUsize>,
}
struct Reservation(Arc<AtomicUsize>);
impl Drop for Reservation {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Entry {
    deadline: u64,
    // 0: queued; 1: dispatch entered; 2/3: cancelled before/after dispatch.
    phase: Arc<AtomicU8>,
    body: Option<Reply>,
    reservation: Option<Arc<Reservation>>,
    session: Option<secrets::SessionPermit>,
}
struct Reply {
    status: u16,
    bytes: Vec<u8>,
    next: usize,
}
fn now() -> Option<u64> {
    airk_process::monotonic::now_ns().ok()
}
fn failure(code: &str) -> ControlResponse {
    ControlResponse::Failure { code: code.into() }
}
fn cancel(phase: &AtomicU8) {
    let _ = phase.fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
        Some(if state == 0 || state == 2 { 2 } else { 3 })
    });
}
impl Requests {
    pub(super) fn end(&mut self) {
        for entry in self.entries.values() {
            cancel(&entry.phase);
        }
        self.entries.clear();
    }
    fn purge(&mut self, now: u64) {
        self.entries.retain(|_, entry| {
            if entry.deadline <= now {
                cancel(&entry.phase);
                false
            } else {
                true
            }
        });
    }
    fn chunk(&mut self, key: &RequestId, offset: u64, now_ms: u64) -> ControlResponse {
        let Some(entry) = self.entries.get_mut(key) else {
            return failure("admin_native_outcome_unknown");
        };
        if now().is_none_or(|timestamp| timestamp >= entry.deadline) {
            cancel(&entry.phase);
            entry.body = None;
            entry.reservation = None;
            return failure("admin_native_outcome_unknown");
        }
        if entry.phase.load(Ordering::Acquire) >= 2 {
            return failure("admin_native_outcome_unknown");
        }
        if !entry
            .session
            .as_ref()
            .is_some_and(|permit| permit.valid(now_ms))
        {
            entry.body = None;
            entry.reservation = None;
            return failure("admin_session_invalid");
        }
        let Some(reply) = entry.body.as_mut() else {
            return failure("admin_native_outcome_unknown");
        };
        if offset != reply.next as u64 {
            return failure("admin_native_outcome_unknown");
        }
        let end = reply.bytes.len().min(reply.next + MAX_CHUNK_BYTES);
        let chunk = NativeAdminChunk {
            http_status: reply.status,
            total_bytes: reply.bytes.len() as u64,
            offset,
            data: reply.bytes[reply.next..end].to_vec(),
            done: end == reply.bytes.len(),
        };
        reply.next = end;
        if chunk.done {
            entry.body = None;
            entry.reservation = None;
        }
        ControlResponse::AdminNativeChunk { chunk }
    }
}
impl AdminPlane {
    fn native_valid(&self, session: &NativeAdminSessionSecret) -> bool {
        self.secrets()
            .sessions
            .facts(&[session.cookie()], &[session.csrf()], (self.clock)())
            .is_some_and(|facts| facts.same_instance && facts.token_matches)
    }

    /// Only the authenticated owner control endpoint may call this entry.
    /// It shares the browser plane's session count, expiry and A2 authority.
    pub fn native_open(&self, deadline_ns: u64) -> ControlResponse {
        if now().is_none_or(|now| now >= deadline_ns || deadline_ns - now > MAX_DEADLINE_NS) {
            return failure("admin_native_deadline");
        }
        let (Some(cookie), Some(csrf)) = ((self.random)(), (self.random)()) else {
            return failure("admin_launch_unavailable");
        };
        let minted = {
            let mut secrets = self.secrets();
            if now().is_none_or(|now| now >= deadline_ns) {
                return failure("admin_native_deadline");
            }
            secrets.sessions.create(&cookie, &csrf, (self.clock)())
        };
        match NativeAdminSessionSecret::new(minted.cookie, minted.csrf) {
            Ok(session) => ControlResponse::AdminNativeOpened {
                session,
                service_instance: self.instance,
            },
            Err(_) => failure("admin_launch_unavailable"),
        }
    }

    /// Runs the very same operation admission, confirmation, key custody and
    /// result projection as HTTP. `dispatch` is the service's direct canonical
    /// handler, avoiding a nested owner-pipe admission while it owns this call.
    #[allow(clippy::too_many_arguments)]
    pub async fn native_call(
        &self,
        session: NativeAdminSessionSecret,
        request_id: Incarnation,
        operation: String,
        body: Box<serde_json::value::RawValue>,
        deadline_ns: u64,
        dispatch: NativeDispatch,
        live_service: NativeServiceLive,
    ) -> ControlResponse {
        let permit =
            self.secrets()
                .sessions
                .native_permit(session.cookie(), session.csrf(), (self.clock)());
        let Some(permit) = permit else {
            return self
                .native_reply(refusal(AdminCode::AdminSessionInvalid))
                .await;
        };
        if !live_service() {
            return self.native_reply(refusal(AdminCode::ServiceDraining)).await;
        }
        if body.get().len() > MAX_BODY_BYTES {
            return failure("admin_native_request_too_large");
        }
        let Some(timestamp) =
            now().filter(|now| *now < deadline_ns && deadline_ns - *now <= MAX_DEADLINE_NS)
        else {
            return failure("admin_native_deadline");
        };
        let key = (secrets::digest(session.cookie()), request_id);
        let phase = Arc::new(AtomicU8::new(0));
        let _reservation = {
            let mut requests = self.native.lock().unwrap_or_else(|e| e.into_inner());
            requests.purge(timestamp);
            // Completed/cancelled identifiers remain tombstones until their
            // original deadline: duplicate calls cannot re-execute an effect.
            if requests.entries.contains_key(&key) {
                return failure("admin_native_outcome_unknown");
            }
            if requests.entries.len() >= 128 || requests.charged.load(Ordering::Acquire) >= 2 {
                return failure("admin_native_busy");
            }
            requests.charged.fetch_add(1, Ordering::AcqRel);
            let reservation = Arc::new(Reservation(requests.charged.clone()));
            requests.entries.insert(
                key,
                Entry {
                    deadline: deadline_ns,
                    phase: phase.clone(),
                    body: None,
                    reservation: Some(reservation.clone()),
                    session: Some(permit.clone()),
                },
            );
            reservation
        };
        let clock = self.clock.clone();
        let admission_permit = permit.clone();
        let admission_clock = self.clock.clone();
        let dispatch_phase = phase.clone();
        let dispatch_live = live_service.clone();
        let check: NativeDispatchCheck = Arc::new(move || {
            if !dispatch_live() {
                return Err("service_draining");
            }
            if now().is_none_or(|now| now >= deadline_ns) {
                return Err("admin_native_deadline");
            }
            if dispatch_phase.load(Ordering::Acquire) >= 2 {
                return Err("admin_native_cancelled");
            }
            if !permit.valid(clock()) {
                return Err("admin_session_invalid");
            }
            if now().is_none_or(|now| now >= deadline_ns) {
                return Err("admin_native_deadline");
            }
            if dispatch_phase.load(Ordering::Acquire) >= 2 {
                return Err("admin_native_cancelled");
            }
            Ok(())
        });
        let dispatch_phase = phase.clone();
        let guarded: Dispatch = Arc::new(move |method| {
            let dispatch = dispatch.clone();
            let check = check.clone();
            let phase = dispatch_phase.clone();
            Box::pin(async move {
                check().map_err(anyhow::Error::msg)?;
                let prior = phase.compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
                anyhow::ensure!(matches!(prior, Ok(0) | Err(1)), "native Admin cancelled");
                dispatch(method, check).await
            })
        });
        let admission_phase = phase.clone();
        let view = Self {
            dispatch: guarded,
            instance: self.instance,
            clock: self.clock.clone(),
            random: self.random.clone(),
            secrets: self.secrets.clone(),
            native: self.native.clone(),
            scope_check: self.scope_check.clone(),
            trust_inspect: self.trust_inspect.clone(),
            selection: self.selection.clone(),
            selection_reader: self.selection_reader.clone(),
            native_admission: Some(Arc::new(move || {
                if !live_service() {
                    Err(AdminCode::ServiceDraining)
                } else if !admission_permit.valid(admission_clock()) {
                    Err(AdminCode::AdminSessionInvalid)
                } else if admission_phase.load(Ordering::Acquire) >= 2
                    || now().is_none_or(|now| now >= deadline_ns)
                {
                    Err(AdminCode::AdminInternal)
                } else {
                    Ok(())
                }
            })),
        };
        let response = match view
            .call(
                &operation,
                Some(session.cookie()),
                http_body_util::Full::new(Bytes::copy_from_slice(body.get().as_bytes())),
            )
            .await
        {
            Ok(response) => response,
            Err(code) => refusal(code),
        };
        let status = response.status().as_u16();
        if phase.load(Ordering::Acquire) >= 2 || now().is_none_or(|now| now >= deadline_ns) {
            self.native_finish_failed(&key);
            return failure("admin_native_outcome_unknown");
        }
        let Some(bytes) = collect(response).await else {
            self.native_finish_failed(&key);
            return failure("admin_native_outcome_unknown");
        };
        let mut requests = self.native.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = requests.entries.get_mut(&key) else {
            return failure("admin_native_outcome_unknown");
        };
        if phase.load(Ordering::Acquire) >= 2 || now().is_none_or(|now| now >= deadline_ns) {
            entry.reservation = None;
            return failure("admin_native_outcome_unknown");
        }
        if !entry
            .session
            .as_ref()
            .is_some_and(|permit| permit.valid((self.clock)()))
        {
            entry.body = None;
            entry.reservation = None;
            return failure("admin_session_invalid");
        }
        entry.body = Some(Reply {
            status,
            bytes,
            next: 0,
        });
        requests.chunk(&key, 0, (self.clock)())
    }

    async fn native_reply(&self, response: HttpResponse) -> ControlResponse {
        let status = response.status().as_u16();
        let Some(bytes) = collect(response).await else {
            return failure("admin_native_outcome_unknown");
        };
        ControlResponse::AdminNativeChunk {
            chunk: NativeAdminChunk {
                http_status: status,
                total_bytes: bytes.len() as u64,
                offset: 0,
                data: bytes,
                done: true,
            },
        }
    }
    fn native_finish_failed(&self, key: &RequestId) {
        if let Some(entry) = self
            .native
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entries
            .get_mut(key)
        {
            entry.body = None;
            entry.reservation = None;
        }
    }
    pub fn native_read(
        &self,
        session: NativeAdminSessionSecret,
        request_id: Incarnation,
        offset: u64,
    ) -> ControlResponse {
        if !self.native_valid(&session) {
            return failure("admin_session_invalid");
        }
        let Some(timestamp) = now() else {
            return failure("admin_native_deadline");
        };
        let mut requests = self.native.lock().unwrap_or_else(|e| e.into_inner());
        requests.purge(timestamp);
        requests.chunk(
            &(secrets::digest(session.cookie()), request_id),
            offset,
            (self.clock)(),
        )
    }
    pub fn native_cancel(
        &self,
        session: NativeAdminSessionSecret,
        request_id: Incarnation,
    ) -> ControlResponse {
        if !self.native_valid(&session) {
            return failure("admin_session_invalid");
        }
        let Some(timestamp) = now() else {
            return failure("admin_native_deadline");
        };
        let mut requests = self.native.lock().unwrap_or_else(|e| e.into_inner());
        requests.purge(timestamp);
        let key = (secrets::digest(session.cookie()), request_id);
        if let Some(entry) = requests.entries.get_mut(&key) {
            cancel(&entry.phase);
            entry.body = None;
            entry.reservation = None;
        } else if requests.entries.len() < 128 {
            // The I2 client deadline is at most 120 s. Cancellation can race
            // owner-pipe delivery, so keep a no-dispatch tombstone for that span.
            requests.entries.insert(
                key,
                Entry {
                    deadline: timestamp.saturating_add(MAX_DEADLINE_NS),
                    phase: Arc::new(AtomicU8::new(2)),
                    body: None,
                    reservation: None,
                    session: None,
                },
            );
        } else {
            return failure("admin_native_busy");
        }
        ControlResponse::AdminNativeCancelled {}
    }
    pub fn native_close(&self, session: NativeAdminSessionSecret) -> ControlResponse {
        if !self.native_valid(&session) {
            return failure("admin_session_invalid");
        }
        self.secrets().sessions.remove(session.cookie());
        let cookie = secrets::digest(session.cookie());
        self.native
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entries
            .retain(|(owner, _), entry| {
                if owner == &cookie {
                    cancel(&entry.phase);
                    false
                } else {
                    true
                }
            });
        ControlResponse::AdminNativeClosed {}
    }
}
async fn collect(response: HttpResponse) -> Option<Vec<u8>> {
    let mut body = response.into_body();
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        if let Ok(data) = frame.ok()?.into_data() {
            if bytes.len().checked_add(data.len())? > MAX_RESPONSE_BYTES as usize {
                return None;
            }
            bytes.extend_from_slice(&data);
        }
    }
    (!bytes.is_empty()).then_some(bytes)
}

#[cfg(test)]
mod tests;
