// SPDX-License-Identifier: Apache-2.0
//! The service's in-memory window plane (contracts §2.5, split step 3f.2):
//! W1–W8 and M12. All state belongs to one `service_instance` and is lost on
//! restart. Time, identifiers, leases and the catalog scope check are
//! injected, so every rule is exercised under a manual clock.
//!
//! W4 grants carry an in-memory selection authority (selection authority
//! spec §3.1, step S1b): each stored grant draws a fresh nonce and registers
//! it live with the admission gate. Every path that drops a grant (sweep,
//! close, release, replace, service end) returns its nonce as a [`Dropped`]
//! value that the caller revokes after releasing the windows lock (§3.3,
//! §3.7, N4). The W4 check and the read re-check run with the lock released
//! ([`grant_unlocked`], [`read_unlocked`]).
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::CatalogRevision,
    scoped_reads::{SelectionOwnerKind, SelectionScopeRef},
    service::{
        AssistanceDraft, AssistanceWindow, ControlMethod, ControlResponse, WindowLease,
        WindowScopeSelection,
    },
};
use std::{collections::VecDeque, future::Future, sync::Mutex};

const MAX_WINDOWS: usize = 64;
const LABEL_BYTES: usize = 128;
const TTL_MS: u64 = 30_000;
const DRAFTS_PER_WINDOW: usize = 16;
const QUEUE_BYTES: usize = 1_048_576;
const DRAFT_BYTES: usize = 262_144;
const DRAIN_LIMIT: usize = 8;
const HANDOFFS_PER_WINDOW: usize = 32;
const HANDOFF_BYTES: usize = 65_536;
const HANDOFF_TTL_MS: u64 = 600_000;

const WINDOW_CAPACITY: &str = "window_capacity";
const WINDOW_LABEL_INVALID: &str = "window_label_invalid";
const WINDOW_UNKNOWN: &str = "window_unknown";
const WINDOW_LEASE_INVALID: &str = "window_lease_invalid";
const WINDOW_SCOPE_NOT_GRANTED: &str = "window_scope_not_granted";
const WINDOW_PLANE_UNAVAILABLE: &str = "window_plane_unavailable";
/// A stale, replaced or released grant, an unregistered nonce or a selection
/// without a project (§3.2, §3.5); an existing code.
const SCOPE_UNAVAILABLE: &str = "scoped_read_unavailable";
const ASSISTANCE_DRAFT_OVERSIZE: &str = "assistance_draft_oversize";
const ASSISTANCE_QUEUE_FULL: &str = "assistance_queue_full";
const ASSISTANCE_DRAIN_LIMIT_INVALID: &str = "assistance_drain_limit_invalid";
const ASSISTANCE_ACK_AHEAD: &str = "assistance_ack_ahead";
const HANDOFF_UNKNOWN: &str = "handoff_unknown";
const HANDOFF_TTL_INVALID: &str = "handoff_ttl_invalid";
const HANDOFF_OVERSIZE: &str = "handoff_oversize";
const HANDOFF_CAPACITY: &str = "handoff_capacity";

/// Which identifier the id source is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IdKind {
    Window,
    Draft,
    Handoff,
}

/// Milliseconds on the service's injected clock.
pub(super) type Clock = Box<dyn Fn() -> u64 + Send>;
pub(super) type IdSource = Box<dyn FnMut(IdKind) -> Incarnation + Send>;
pub(super) type LeaseSource = Box<dyn FnMut() -> WindowLease + Send>;
/// The catalog check for a W4 selection: its scope identity and the catalog
/// revision, or the failure code (for example `scoped_read_unavailable`).
pub(super) type ScopeValidator =
    Box<dyn Fn(&WindowScopeSelection) -> Result<(Sha256Digest, CatalogRevision), String> + Send>;
/// The outcome of that check, as the unlocked W4 and read paths receive it.
pub(super) type CheckResult = Result<(Sha256Digest, CatalogRevision), String>;

/// Draws a fresh selection grant nonce (128 random bits in the service).
pub(super) type NonceSource = Box<dyn FnMut() -> Incarnation + Send>;
/// Registers a stored grant's nonce live with the admission gate. Called
/// nested inside the windows lock (§3.7: owner, then gate; the gate never
/// calls back into the plane).
pub(super) type SelectionRegistrar = Box<dyn Fn(&SelectionScopeRef) -> Result<(), ()> + Send>;

/// A window's in-memory scope grant (U77), resolved by M3 `selection: Window`.
#[derive(Clone, Debug)]
pub(super) struct WindowGrant {
    pub(super) selection: WindowScopeSelection,
    pub(super) scope_identity: Sha256Digest,
    pub(super) catalog_revision: CatalogRevision,
    /// The in-memory authority admission keys on (§3.1). Its `Debug`
    /// redacts the nonce; it never reaches a reply or failure body (N3).
    pub(super) selection_ref: SelectionScopeRef,
}

/// The selection nonces of the grants one plane call dropped. The caller
/// revokes them (`NativeCatalogAdmission::revoke_selection`) after it has
/// released the windows lock (§3.3, §3.7, N4).
#[must_use = "revoke every dropped selection nonce with no lock held"]
#[derive(Default)]
pub(super) struct Dropped(Vec<Incarnation>);

impl Dropped {
    fn push(&mut self, grant: Option<WindowGrant>) {
        if let Some(grant) = grant {
            self.0.push(grant.selection_ref.grant_nonce);
        }
    }
    fn absorb(&mut self, other: Dropped) {
        self.0.extend(other.0);
    }
    pub(super) fn into_nonces(self) -> Vec<Incarnation> {
        self.0
    }
}

impl std::fmt::Debug for Dropped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Dropped({} nonces)", self.0.len())
    }
}

/// A W4 attempt resolved under the lock: the window and its grant
/// generation, re-checked when the result is stored (§3.7 step 3).
struct GrantAttempt {
    window_id: Incarnation,
    lease: WindowLease,
    sequence: u64,
}

/// A read's grant, resolved under the lock and re-confirmed after the check
/// (§3.2). Holds the lease only to locate the window again; never printed.
pub(super) struct ReadGrant {
    window_id: Incarnation,
    lease: WindowLease,
    sequence: u64,
    pub(super) grant: WindowGrant,
}

struct QueuedDraft {
    sequence: u64,
    draft_id: Incarnation,
    kind: String,
    draft: serde_json::Value,
    bytes: usize,
}

struct Handoff {
    id: Incarnation,
    value: serde_json::Value,
    expires_at_ms: u64,
}

struct Window {
    id: Incarnation,
    label: String,
    lease: WindowLease,
    renewed_at_ms: u64,
    next_sequence: u64,
    drafts: VecDeque<QueuedDraft>,
    draft_bytes: usize,
    handoffs: Vec<Handoff>,
    grant: Option<WindowGrant>,
    /// Bumped on every grant store, release and replace (§3.7).
    grant_sequence: u64,
}

impl Window {
    fn alive(&self, now: u64) -> bool {
        now < self.renewed_at_ms.saturating_add(TTL_MS)
    }
    fn full(&self) -> bool {
        self.drafts.len() >= DRAFTS_PER_WINDOW || self.draft_bytes >= QUEUE_BYTES
    }
}

type Outcome = Result<ControlResponse, String>;

/// All W state of one service instance.
pub(super) struct WindowPlane {
    service_instance: Incarnation,
    clock: Clock,
    ids: IdSource,
    leases: LeaseSource,
    validate_scope: ScopeValidator,
    nonces: NonceSource,
    /// `None` without a catalog gate: the nonce is then never live and every
    /// read under the grant is refused at admission.
    registrar: Option<SelectionRegistrar>,
    /// Open windows in opening order (W5 lists them in this order).
    windows: Vec<Window>,
}

impl WindowPlane {
    pub(super) fn new(
        service_instance: Incarnation,
        clock: Clock,
        ids: IdSource,
        leases: LeaseSource,
        validate_scope: ScopeValidator,
    ) -> Self {
        let mut drawn = 0u64;
        Self {
            service_instance,
            clock,
            ids,
            leases,
            validate_scope,
            // Deterministic and never registered: the service replaces it
            // through `with_selection_authority`.
            nonces: Box::new(move || {
                drawn += 1;
                let mut bytes = [0x5e; 16];
                bytes[1..9].copy_from_slice(&drawn.to_be_bytes());
                Incarnation::from_bytes(bytes)
            }),
            registrar: None,
            windows: Vec::new(),
        }
    }

    /// The service's nonce source and, with a catalog gate, its live-set
    /// registration (§3.1, §3.4).
    pub(super) fn with_selection_authority(
        mut self,
        nonces: NonceSource,
        registrar: Option<SelectionRegistrar>,
    ) -> Self {
        self.nonces = nonces;
        self.registrar = registrar;
        self
    }

    /// The window-plane methods without their dropped nonces (the fixture
    /// replay and the tests use this).
    #[cfg(test)]
    pub(super) fn handle(&mut self, method: &ControlMethod) -> Option<ControlResponse> {
        self.dispatch(method).map(|(response, _dropped)| response)
    }

    /// Answers the ten window-plane methods; `None` for every other method.
    /// W4 here runs the check inline; the service routes W4 through
    /// [`grant_unlocked`] instead, so the check never runs under its lock.
    pub(super) fn dispatch(
        &mut self,
        method: &ControlMethod,
    ) -> Option<(ControlResponse, Dropped)> {
        use ControlMethod as M;
        if !matches!(
            method,
            M::WindowOpen { .. }
                | M::WindowRenew { .. }
                | M::WindowClose { .. }
                | M::WindowScopeGrant { .. }
                | M::WindowScopeRelease { .. }
                | M::AssistanceWindows {}
                | M::AssistanceSubmit { .. }
                | M::AssistanceDrain { .. }
                | M::HandoffPublish { .. }
                | M::ExplorerHandoff { .. }
        ) {
            return None;
        }
        let now = (self.clock)();
        let mut dropped = self.sweep(now);
        let outcome = match method {
            M::WindowOpen {
                label,
                owner_pid: _,
            } => self.open(label, now),
            M::WindowRenew { window_id, lease } => self.renew(window_id, lease, now),
            M::WindowClose { window_id, lease } => self.close(window_id, lease, now, &mut dropped),
            M::WindowScopeGrant {
                window_id,
                lease,
                selection,
            } => self.grant(window_id, lease, selection, now, &mut dropped),
            M::WindowScopeRelease { window_id, lease } => {
                self.release(window_id, lease, now, &mut dropped)
            }
            M::AssistanceWindows {} => Ok(self.list()),
            M::AssistanceSubmit {
                window_id,
                kind,
                draft,
                submitter: _,
            } => self.submit(window_id, kind, draft, now),
            M::AssistanceDrain {
                window_id,
                lease,
                ack_through,
                limit,
            } => self.drain(window_id, lease, *ack_through, *limit, now),
            M::HandoffPublish {
                window_id,
                lease,
                handoff,
                ttl_ms,
            } => self.publish(window_id, lease, handoff, *ttl_ms, now),
            M::ExplorerHandoff { handoff_id } => self.read_handoff(handoff_id),
            _ => return None,
        };
        Some((
            outcome.unwrap_or_else(|code| ControlResponse::Failure { code }),
            dropped,
        ))
    }

    /// The grant M3 `selection: Window { window_id, lease }` reads under.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "read by the M3 window selection in step 3f.4")
    )]
    pub(super) fn scope(
        &self,
        window_id: &Incarnation,
        lease: &WindowLease,
    ) -> Result<WindowGrant, String> {
        let index = locate(&self.windows, window_id, lease, (self.clock)())?;
        self.windows[index]
            .grant
            .clone()
            .ok_or_else(|| WINDOW_SCOPE_NOT_GRANTED.into())
    }

    /// Expiry acts as close (drops queue, handoffs and grant); expired
    /// handoffs are dropped. Returns the expired windows' grant nonces.
    fn sweep(&mut self, now: u64) -> Dropped {
        let mut dropped = Dropped::default();
        self.windows.retain_mut(|window| {
            let alive = window.alive(now);
            if !alive {
                dropped.push(window.grant.take());
            }
            alive
        });
        for window in &mut self.windows {
            window
                .handoffs
                .retain(|handoff| now < handoff.expires_at_ms);
        }
        dropped
    }

    /// Service end: every grant's nonce, revoked before the workers drain
    /// (§3.3).
    pub(super) fn end(&mut self) -> Dropped {
        let mut dropped = Dropped::default();
        for window in &mut self.windows {
            window.grant_sequence = window.grant_sequence.wrapping_add(1);
            dropped.push(window.grant.take());
        }
        dropped
    }

    fn open(&mut self, label: &str, now: u64) -> Outcome {
        if label.len() > LABEL_BYTES {
            return Err(WINDOW_LABEL_INVALID.into());
        }
        if self.windows.len() >= MAX_WINDOWS {
            return Err(WINDOW_CAPACITY.into());
        }
        let id = (self.ids)(IdKind::Window);
        let lease = (self.leases)();
        self.windows.push(Window {
            id,
            label: label.to_owned(),
            lease: lease.clone(),
            renewed_at_ms: now,
            next_sequence: 1,
            drafts: VecDeque::new(),
            draft_bytes: 0,
            handoffs: Vec::new(),
            grant: None,
            grant_sequence: 0,
        });
        Ok(ControlResponse::WindowOpened {
            window_id: id,
            lease,
            ttl_ms: TTL_MS,
            service_instance: self.service_instance,
        })
    }

    fn renew(&mut self, id: &Incarnation, lease: &WindowLease, now: u64) -> Outcome {
        let index = locate(&self.windows, id, lease, now)?;
        self.windows[index].renewed_at_ms = now;
        Ok(ControlResponse::WindowRenewed { ttl_ms: TTL_MS })
    }

    fn close(
        &mut self,
        id: &Incarnation,
        lease: &WindowLease,
        now: u64,
        dropped: &mut Dropped,
    ) -> Outcome {
        let index = locate(&self.windows, id, lease, now)?;
        dropped.push(self.windows.remove(index).grant);
        Ok(ControlResponse::WindowClosed {})
    }

    /// The inline W4 (fixture replay and tests): the steps of
    /// [`grant_unlocked`] with nothing released between them.
    fn grant(
        &mut self,
        id: &Incarnation,
        lease: &WindowLease,
        selection: &WindowScopeSelection,
        now: u64,
        dropped: &mut Dropped,
    ) -> Outcome {
        let attempt = self.grant_attempt(id, lease, now)?;
        let checked = (self.validate_scope)(selection);
        self.store_grant(&attempt, selection, checked, now, dropped)
    }

    fn grant_attempt(
        &self,
        id: &Incarnation,
        lease: &WindowLease,
        now: u64,
    ) -> Result<GrantAttempt, String> {
        let index = locate(&self.windows, id, lease, now)?;
        Ok(GrantAttempt {
            window_id: *id,
            lease: lease.clone(),
            sequence: self.windows[index].grant_sequence,
        })
    }

    /// §3.7 step 3: the window must still be alive under the same lease
    /// (`window_unknown`) with no grant stored or released since the attempt
    /// began (`scoped_read_unavailable`: the last writer under the lock
    /// won). Then a fresh nonce is drawn and registered live (gate, nested),
    /// the replaced nonce collected and the grant stored. A successful grant
    /// renews the window, so the grant's `expires_at_ms` is the window's
    /// expiry. A failed grant leaves the previous one.
    fn store_grant(
        &mut self,
        attempt: &GrantAttempt,
        selection: &WindowScopeSelection,
        checked: CheckResult,
        now: u64,
        dropped: &mut Dropped,
    ) -> Outcome {
        let index = locate(&self.windows, &attempt.window_id, &attempt.lease, now)?;
        let (scope_identity, catalog_revision) = checked?;
        if self.windows[index].grant_sequence != attempt.sequence {
            return Err(SCOPE_UNAVAILABLE.into());
        }
        let selection_ref = SelectionScopeRef {
            owner: SelectionOwnerKind::Window,
            grant_nonce: (self.nonces)(),
            service_instance: self.service_instance,
            scope_identity,
            catalog_revision,
        };
        if let Some(register) = &self.registrar {
            register(&selection_ref).map_err(|()| SCOPE_UNAVAILABLE.to_owned())?;
        }
        let window = &mut self.windows[index];
        window.grant_sequence = window.grant_sequence.wrapping_add(1);
        window.renewed_at_ms = now;
        dropped.push(window.grant.replace(WindowGrant {
            selection: selection.clone(),
            scope_identity,
            catalog_revision,
            selection_ref,
        }));
        Ok(ControlResponse::WindowScopeGranted {
            scope_identity,
            catalog_revision,
            expires_at_ms: now.saturating_add(TTL_MS),
        })
    }

    fn release(
        &mut self,
        id: &Incarnation,
        lease: &WindowLease,
        now: u64,
        dropped: &mut Dropped,
    ) -> Outcome {
        let index = locate(&self.windows, id, lease, now)?;
        let window = &mut self.windows[index];
        window.grant_sequence = window.grant_sequence.wrapping_add(1);
        dropped.push(window.grant.take());
        Ok(ControlResponse::WindowScopeReleased {})
    }

    /// §3.2 step 1: the window's grant and its generation, copied under the
    /// lock.
    fn read_attempt(
        &self,
        id: &Incarnation,
        lease: &WindowLease,
        now: u64,
    ) -> Result<ReadGrant, String> {
        let index = locate(&self.windows, id, lease, now)?;
        let window = &self.windows[index];
        let grant = window.grant.clone().ok_or(WINDOW_SCOPE_NOT_GRANTED)?;
        Ok(ReadGrant {
            window_id: *id,
            lease: lease.clone(),
            sequence: window.grant_sequence,
            grant,
        })
    }

    /// After the check: the window is still alive (`window_unknown`) and its
    /// grant was neither replaced nor released (`scoped_read_unavailable`).
    fn read_confirm(&self, read: &ReadGrant, now: u64) -> Result<(), String> {
        let index = locate(&self.windows, &read.window_id, &read.lease, now)?;
        if self.windows[index].grant_sequence == read.sequence {
            Ok(())
        } else {
            Err(SCOPE_UNAVAILABLE.into())
        }
    }

    /// U99 Q5: a successful read under the grant renews the window, unless
    /// the window or its grant changed meanwhile.
    pub(super) fn renew_after_read(&mut self, read: &ReadGrant) -> bool {
        let now = (self.clock)();
        match locate(&self.windows, &read.window_id, &read.lease, now) {
            Ok(index) if self.windows[index].grant_sequence == read.sequence => {
                self.windows[index].renewed_at_ms = now;
                true
            }
            _ => false,
        }
    }

    fn list(&self) -> ControlResponse {
        ControlResponse::AssistanceWindows {
            windows: self
                .windows
                .iter()
                .map(|window| AssistanceWindow {
                    window_id: window.id,
                    label: window.label.clone(),
                    accepting: !window.full(),
                    queued: window.drafts.len(),
                })
                .collect(),
        }
    }

    fn submit(
        &mut self,
        id: &Incarnation,
        kind: &str,
        draft: &serde_json::Value,
        now: u64,
    ) -> Outcome {
        let window = self
            .windows
            .iter_mut()
            .find(|window| window.id == *id && window.alive(now))
            .ok_or(WINDOW_UNKNOWN)?;
        let bytes = compact_len(draft);
        if bytes > DRAFT_BYTES {
            return Err(ASSISTANCE_DRAFT_OVERSIZE.into());
        }
        if window.drafts.len() >= DRAFTS_PER_WINDOW || window.draft_bytes + bytes > QUEUE_BYTES {
            return Err(ASSISTANCE_QUEUE_FULL.into());
        }
        let draft_id = (self.ids)(IdKind::Draft);
        let sequence = window.next_sequence;
        window.next_sequence += 1;
        window.draft_bytes += bytes;
        window.drafts.push_back(QueuedDraft {
            sequence,
            draft_id,
            kind: kind.to_owned(),
            draft: draft.clone(),
            bytes,
        });
        Ok(ControlResponse::AssistanceQueued {
            draft_id,
            queued: window.drafts.len(),
        })
    }

    /// W7: acknowledge through `ack_through`, then return (without removing)
    /// the oldest `limit` unacknowledged drafts.
    fn drain(
        &mut self,
        id: &Incarnation,
        lease: &WindowLease,
        ack_through: Option<u64>,
        limit: usize,
        now: u64,
    ) -> Outcome {
        let index = locate(&self.windows, id, lease, now)?;
        if !(1..=DRAIN_LIMIT).contains(&limit) {
            return Err(ASSISTANCE_DRAIN_LIMIT_INVALID.into());
        }
        let window = &mut self.windows[index];
        if let Some(through) = ack_through {
            if through >= window.next_sequence {
                return Err(ASSISTANCE_ACK_AHEAD.into());
            }
            while window
                .drafts
                .front()
                .is_some_and(|draft| draft.sequence <= through)
            {
                if let Some(draft) = window.drafts.pop_front() {
                    window.draft_bytes -= draft.bytes;
                }
            }
        }
        let drafts = window
            .drafts
            .iter()
            .take(limit)
            .map(|draft| AssistanceDraft {
                sequence: draft.sequence,
                draft_id: draft.draft_id,
                kind: draft.kind.clone(),
                draft: draft.draft.clone(),
            })
            .collect();
        let next = window.drafts.get(limit).map(|draft| draft.sequence);
        Ok(ControlResponse::AssistanceDrained { drafts, next })
    }

    fn publish(
        &mut self,
        id: &Incarnation,
        lease: &WindowLease,
        handoff: &serde_json::Value,
        ttl_ms: u64,
        now: u64,
    ) -> Outcome {
        let index = locate(&self.windows, id, lease, now)?;
        if !(1..=HANDOFF_TTL_MS).contains(&ttl_ms) {
            return Err(HANDOFF_TTL_INVALID.into());
        }
        if compact_len(handoff) > HANDOFF_BYTES {
            return Err(HANDOFF_OVERSIZE.into());
        }
        if self.windows[index].handoffs.len() >= HANDOFFS_PER_WINDOW {
            return Err(HANDOFF_CAPACITY.into());
        }
        let handoff_id = (self.ids)(IdKind::Handoff);
        let expires_at_ms = now.saturating_add(ttl_ms);
        self.windows[index].handoffs.push(Handoff {
            id: handoff_id,
            value: handoff.clone(),
            expires_at_ms,
        });
        Ok(ControlResponse::HandoffPublished {
            handoff_id,
            expires_at_ms,
        })
    }

    /// M12: any window's unexpired handoff; reads do not consume.
    fn read_handoff(&self, id: &Incarnation) -> Outcome {
        self.windows
            .iter()
            .flat_map(|window| &window.handoffs)
            .find(|handoff| handoff.id == *id)
            .map(|handoff| ControlResponse::ExplorerHandoff {
                handoff: handoff.value.clone(),
            })
            .ok_or_else(|| HANDOFF_UNKNOWN.into())
    }
}

/// W4 with the catalog check outside the windows lock (§3.7): resolve under
/// the lock, unlock, `check`, lock again and store only if the window and
/// its grant generation are unchanged. Returns the reply and every nonce the
/// sweeps and the replacement dropped, for revocation after this returns.
pub(super) async fn grant_unlocked<F, Fut>(
    plane: &Mutex<WindowPlane>,
    id: &Incarnation,
    lease: &WindowLease,
    selection: &WindowScopeSelection,
    check: F,
) -> (ControlResponse, Dropped)
where
    F: FnOnce(WindowScopeSelection) -> Fut,
    Fut: Future<Output = CheckResult>,
{
    let mut dropped = Dropped::default();
    let attempt = match plane.lock() {
        Ok(mut windows) => {
            let now = (windows.clock)();
            dropped.absorb(windows.sweep(now));
            windows.grant_attempt(id, lease, now)
        }
        Err(_) => Err(WINDOW_PLANE_UNAVAILABLE.to_owned()),
    };
    let outcome = match attempt {
        Ok(attempt) => {
            let checked = check(selection.clone()).await;
            match plane.lock() {
                Ok(mut windows) => {
                    let now = (windows.clock)();
                    dropped.absorb(windows.sweep(now));
                    windows.store_grant(&attempt, selection, checked, now, &mut dropped)
                }
                Err(_) => Err(WINDOW_PLANE_UNAVAILABLE.to_owned()),
            }
        }
        Err(code) => Err(code),
    };
    (
        outcome.unwrap_or_else(|code| ControlResponse::Failure { code }),
        dropped,
    )
}

/// §3.2 steps 1-2 for M3 `selection: Window`: sweep (expired grants are
/// returned for revocation), resolve the grant under the lock, refuse a
/// selection without a project (§3.5), run the full `check` with the lock
/// released (U99 Q3) and require exactly the stored identity and revision,
/// then lock again: a window closed meanwhile is `window_unknown`, a grant
/// replaced or released meanwhile `scoped_read_unavailable`.
pub(super) async fn read_unlocked<F, Fut>(
    plane: &Mutex<WindowPlane>,
    id: &Incarnation,
    lease: &WindowLease,
    check: F,
) -> (Result<ReadGrant, String>, Dropped)
where
    F: FnOnce(WindowScopeSelection) -> Fut,
    Fut: Future<Output = CheckResult>,
{
    let mut dropped = Dropped::default();
    let read = match plane.lock() {
        Ok(mut windows) => {
            let now = (windows.clock)();
            dropped.absorb(windows.sweep(now));
            windows.read_attempt(id, lease, now)
        }
        Err(_) => Err(WINDOW_PLANE_UNAVAILABLE.to_owned()),
    };
    let read = match read {
        Ok(read) if read.grant.selection.project_ids.is_empty() => {
            return (Err(SCOPE_UNAVAILABLE.into()), dropped);
        }
        Ok(read) => read,
        Err(code) => return (Err(code), dropped),
    };
    match check(read.grant.selection.clone()).await {
        Ok(checked) if checked == (read.grant.scope_identity, read.grant.catalog_revision) => {}
        _ => return (Err(SCOPE_UNAVAILABLE.into()), dropped),
    }
    let confirmed = match plane.lock() {
        Ok(mut windows) => {
            let now = (windows.clock)();
            dropped.absorb(windows.sweep(now));
            windows.read_confirm(&read, now)
        }
        Err(_) => Err(WINDOW_PLANE_UNAVAILABLE.to_owned()),
    };
    (confirmed.map(|()| read), dropped)
}

/// The live window `id` owned by `lease`. Unknown, closed or expired →
/// `window_unknown`; another lease → `window_lease_invalid`.
fn locate(
    windows: &[Window],
    id: &Incarnation,
    lease: &WindowLease,
    now: u64,
) -> Result<usize, String> {
    let index = windows
        .iter()
        .position(|window| window.id == *id && window.alive(now))
        .ok_or(WINDOW_UNKNOWN)?;
    if leases_equal(&windows[index].lease, lease) {
        Ok(index)
    } else {
        Err(WINDOW_LEASE_INVALID.into())
    }
}

/// Compares without an early exit on the first differing byte.
fn leases_equal(left: &WindowLease, right: &WindowLease) -> bool {
    let (left, right) = (left.as_str().as_bytes(), right.as_str().as_bytes());
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

/// The compact JSON serialization length, without materializing it.
fn compact_len(value: &serde_json::Value) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    match serde_json::to_writer(&mut count, value) {
        Ok(()) => count.0,
        Err(_) => usize::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{service::ControlRequest, usage::ClientFamily};
    use serde_json::{Value, json};
    use std::sync::{
        Arc,
        atomic::{AtomicU8, AtomicU64, Ordering},
    };

    const FIXTURE: &str = include_str!("../../../../fixtures/split/i1/window_plane.json");
    const INSTANCE_1: &str = "e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1";
    const INSTANCE_2: &str = "e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2e2";

    fn synthetic(tag: u8, number: u64) -> [u8; 16] {
        let mut bytes = [tag; 16];
        bytes[1..9].copy_from_slice(&number.to_be_bytes());
        bytes
    }

    /// Sources seeded with the fixture's identifiers: windows `a0`, `b0`
    /// (instance e1) or `a2` (e2), leases `1a`, `1b` or `2a`, drafts
    /// `0xd0 + n`, the first handoff `4e`, grants `5c` then `5d`. Further
    /// identifiers are synthetic and unique.
    fn fixture_plane(instance: &str, clock: &Arc<AtomicU64>) -> WindowPlane {
        let (windows, leases): (Vec<u8>, Vec<u8>) = if instance == INSTANCE_2 {
            (vec![0xa2], vec![0x2a])
        } else {
            (vec![0xa0, 0xb0], vec![0x1a, 0x1b])
        };
        let (mut window_count, mut draft_count, mut handoff_count) = (0u64, 0u8, 0u64);
        let ids: IdSource = Box::new(move |kind| match kind {
            IdKind::Window => {
                window_count += 1;
                windows
                    .get(window_count as usize - 1)
                    .map(|byte| Incarnation::from_bytes([*byte; 16]))
                    .unwrap_or_else(|| Incarnation::from_bytes(synthetic(0x70, window_count)))
            }
            IdKind::Draft => {
                draft_count = draft_count.wrapping_add(1);
                Incarnation::from_bytes([0xd0u8.wrapping_add(draft_count); 16])
            }
            IdKind::Handoff => {
                handoff_count += 1;
                if handoff_count == 1 {
                    Incarnation::from_bytes([0x4e; 16])
                } else {
                    Incarnation::from_bytes(synthetic(0x40, handoff_count))
                }
            }
        });
        let mut lease_count = 0u64;
        let lease_source: LeaseSource = Box::new(move || {
            lease_count += 1;
            match leases.get(lease_count as usize - 1) {
                Some(byte) => WindowLease::from_bytes([*byte; 32]),
                None => {
                    let mut bytes = [0x60; 32];
                    bytes[..16].copy_from_slice(&synthetic(0x60, lease_count));
                    WindowLease::from_bytes(bytes)
                }
            }
        });
        let grants = AtomicU8::new(0x5c);
        let validate: ScopeValidator = Box::new(move |selection| {
            let indexes = serde_json::to_string(&selection.index_ids).unwrap();
            if selection.expected_catalog_revision.get() != 12 || indexes.contains("0f0f") {
                return Err("scoped_read_unavailable".into());
            }
            let byte = grants.fetch_add(1, Ordering::SeqCst);
            Ok((
                Sha256Digest::from_bytes([byte; 32]),
                selection.expected_catalog_revision,
            ))
        });
        let time = Arc::clone(clock);
        WindowPlane::new(
            instance.parse().unwrap(),
            Box::new(move || time.load(Ordering::SeqCst)),
            ids,
            lease_source,
            validate,
        )
    }

    /// Replaces `{"$fill_bytes": N}` with a string whose compact JSON is N bytes.
    fn expand(value: Value) -> Value {
        match value {
            Value::Object(map) => {
                if let (1, Some(size)) = (map.len(), map.get("$fill_bytes").and_then(Value::as_u64))
                {
                    return Value::String("x".repeat(size as usize - 2));
                }
                Value::Object(map.into_iter().map(|(k, v)| (k, expand(v))).collect())
            }
            Value::Array(items) => Value::Array(items.into_iter().map(expand).collect()),
            other => other,
        }
    }

    fn call(plane: &mut WindowPlane, method: ControlMethod) -> Value {
        serde_json::to_value(plane.handle(&method).expect("a window-plane method")).unwrap()
    }

    fn open(plane: &mut WindowPlane, label: &str) -> (Incarnation, WindowLease) {
        let reply = call(
            plane,
            ControlMethod::WindowOpen {
                label: label.into(),
                owner_pid: 1,
            },
        );
        assert_eq!(reply["status"], "window_opened", "{reply}");
        (
            serde_json::from_value(reply["window_id"].clone()).unwrap(),
            serde_json::from_value(reply["lease"].clone()).unwrap(),
        )
    }

    fn renew(plane: &mut WindowPlane, window: (Incarnation, WindowLease)) -> Value {
        call(
            plane,
            ControlMethod::WindowRenew {
                window_id: window.0,
                lease: window.1,
            },
        )
    }

    fn submit(plane: &mut WindowPlane, window_id: Incarnation, draft: Value) -> Value {
        call(
            plane,
            ControlMethod::AssistanceSubmit {
                window_id,
                kind: "search".into(),
                draft,
                submitter: ClientFamily::Unknown,
            },
        )
    }

    fn drain(
        plane: &mut WindowPlane,
        window: &(Incarnation, WindowLease),
        ack: Option<u64>,
        limit: usize,
    ) -> Value {
        call(
            plane,
            ControlMethod::AssistanceDrain {
                window_id: window.0,
                lease: window.1.clone(),
                ack_through: ack,
                limit,
            },
        )
    }

    fn publish(
        plane: &mut WindowPlane,
        window: &(Incarnation, WindowLease),
        handoff: Value,
        ttl_ms: u64,
    ) -> Value {
        call(
            plane,
            ControlMethod::HandoffPublish {
                window_id: window.0,
                lease: window.1.clone(),
                handoff,
                ttl_ms,
            },
        )
    }

    fn failure(code: &str) -> Value {
        json!({"status": "failure", "code": code})
    }

    fn new_plane() -> (Arc<AtomicU64>, WindowPlane) {
        let clock = Arc::new(AtomicU64::new(0));
        let plane = fixture_plane(INSTANCE_1, &clock);
        (clock, plane)
    }

    #[test]
    fn limits_match_the_fixture() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let limits = &fixture["limits"];
        let expected = [
            ("windows", MAX_WINDOWS as u64),
            ("label_bytes", LABEL_BYTES as u64),
            ("ttl_ms", TTL_MS),
            ("drafts_per_window", DRAFTS_PER_WINDOW as u64),
            ("queue_bytes", QUEUE_BYTES as u64),
            ("draft_bytes", DRAFT_BYTES as u64),
            ("drain_limit", DRAIN_LIMIT as u64),
            ("handoffs_per_window", HANDOFFS_PER_WINDOW as u64),
            ("handoff_bytes", HANDOFF_BYTES as u64),
            ("handoff_ttl_ms", HANDOFF_TTL_MS),
        ];
        for (name, value) in expected {
            assert_eq!(limits[name].as_u64(), Some(value), "{name}");
        }
    }

    /// Replays every pipe and `/mcp` frame of `window_plane.json` under its
    /// `at_ms` clock and checks each reply byte-for-byte as JSON. Harness steps
    /// are reproduced: bulk opens and publishes, session renewals and restarts.
    #[test]
    fn window_plane_replays_every_fixture_case() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let mut total = 0;
        for case in fixture["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let clock = Arc::new(AtomicU64::new(0));
            let mut plane = fixture_plane(INSTANCE_1, &clock);
            let mut renewals: Option<(u64, u64)> = None;
            let mut last_opened: Option<(Incarnation, WindowLease)> = None;
            let mut pending: Option<Value> = None;
            let mut asserted = 0;
            for step in case["transcript"].as_array().unwrap() {
                let at = step["at_ms"].as_u64().unwrap();
                if let (Some((every, next)), Some(window)) =
                    (renewals.as_mut(), last_opened.as_ref())
                {
                    while *next <= at {
                        clock.store(*next, Ordering::SeqCst);
                        let reply = renew(&mut plane, window.clone());
                        assert_eq!(
                            reply["status"], "window_renewed",
                            "{name} renewal at {next}"
                        );
                        *next += *every;
                    }
                }
                clock.store(at, Ordering::SeqCst);
                if let Some(what) = step["harness"].as_str() {
                    match what {
                        "windows_opened" => {
                            while plane.windows.len() < step["count"].as_u64().unwrap() as usize {
                                open(&mut plane, "harness");
                            }
                        }
                        "handoffs_published" => {
                            let id = step["window_id"].as_str().unwrap();
                            let lease = &fixture["secrets"]["lease_by_window"][id];
                            let window: (Incarnation, WindowLease) = (
                                id.parse().unwrap(),
                                serde_json::from_value(lease.clone()).unwrap(),
                            );
                            let count = step["count"].as_u64().unwrap() as usize;
                            for _ in 0..count {
                                let reply = publish(&mut plane, &window, json!({}), 1000);
                                assert_eq!(reply["status"], "handoff_published", "{name}");
                            }
                        }
                        "session_renews" => {
                            let every = step["every_ms"].as_u64().unwrap();
                            renewals = Some((every, at + every));
                        }
                        "service_restarts" => {
                            let instance = step["service_instance"].as_str().unwrap();
                            plane = fixture_plane(instance, &clock);
                            last_opened = None;
                        }
                        _ => {}
                    }
                    continue;
                }
                let channel = step["channel"].as_str().unwrap();
                if channel != "pipe" && channel != "mcp" {
                    continue;
                }
                if let Some(send) = step.get("send") {
                    pending = None;
                    if send.to_string().contains("\"elided\"") {
                        continue;
                    }
                    let send = expand(send.clone());
                    let method = if channel == "pipe" {
                        serde_json::from_value::<ControlRequest>(send)
                            .unwrap()
                            .method
                    } else {
                        serde_json::from_value::<ControlMethod>(send["method"].clone()).unwrap()
                    };
                    pending = plane
                        .handle(&method)
                        .map(|reply| serde_json::to_value(reply).unwrap());
                    if let Some(reply) = pending
                        .as_ref()
                        .filter(|reply| reply["status"] == "window_opened")
                    {
                        last_opened = Some((
                            serde_json::from_value(reply["window_id"].clone()).unwrap(),
                            serde_json::from_value(reply["lease"].clone()).unwrap(),
                        ));
                    }
                } else if let (Some(reply), Some(actual)) = (step.get("reply"), pending.take()) {
                    assert_eq!(actual, expand(reply.clone()), "{name} at {at}");
                    asserted += 1;
                }
            }
            assert!(asserted > 0, "{name}: no reply asserted");
            total += asserted;
        }
        assert!(total > 60, "{total}");
    }

    #[test]
    fn non_window_methods_are_not_handled() {
        let (_, mut plane) = new_plane();
        assert!(plane.handle(&ControlMethod::ModelCatalog {}).is_none());
    }

    #[test]
    fn label_bound_counts_bytes_not_characters() {
        let (_, mut plane) = new_plane();
        open(&mut plane, &"é".repeat(64));
        let reply = call(
            &mut plane,
            ControlMethod::WindowOpen {
                label: format!("{}x", "é".repeat(64)),
                owner_pid: 1,
            },
        );
        assert_eq!(reply, failure(WINDOW_LABEL_INVALID));
        open(&mut plane, "");
    }

    #[test]
    fn closed_and_expired_windows_free_capacity() {
        let (clock, mut plane) = new_plane();
        let first = open(&mut plane, "first");
        clock.store(5, Ordering::SeqCst);
        let mut windows = vec![first.clone()];
        while plane.windows.len() < MAX_WINDOWS {
            windows.push(open(&mut plane, "later"));
        }
        let refused = call(
            &mut plane,
            ControlMethod::WindowOpen {
                label: "x".into(),
                owner_pid: 1,
            },
        );
        assert_eq!(refused, failure(WINDOW_CAPACITY));
        let last = windows.pop().unwrap();
        let closed = call(
            &mut plane,
            ControlMethod::WindowClose {
                window_id: last.0,
                lease: last.1.clone(),
            },
        );
        assert_eq!(closed, json!({"status": "window_closed"}));
        open(&mut plane, "reopened");
        clock.store(TTL_MS, Ordering::SeqCst);
        // The window opened at 0 expired; the others opened at 5 are live.
        assert_eq!(renew(&mut plane, first), failure(WINDOW_UNKNOWN));
        open(&mut plane, "after expiry");
        assert_eq!(renew(&mut plane, last), failure(WINDOW_UNKNOWN));
    }

    #[test]
    fn sequences_are_never_reused_and_ack_frees_the_queue() {
        let (_, mut plane) = new_plane();
        let window = open(&mut plane, "a");
        for _ in 0..DRAFTS_PER_WINDOW {
            assert_eq!(
                submit(&mut plane, window.0, json!(1))["status"],
                "assistance_queued"
            );
        }
        assert_eq!(
            submit(&mut plane, window.0, json!(1)),
            failure(ASSISTANCE_QUEUE_FULL)
        );
        let first = drain(&mut plane, &window, Some(0), 8);
        assert_eq!(first["next"], 9);
        let acked = drain(&mut plane, &window, Some(16), 1);
        assert_eq!(
            acked,
            json!({"status": "assistance_drained", "drafts": [], "next": null})
        );
        assert_eq!(plane.windows[0].draft_bytes, 0);
        assert_eq!(submit(&mut plane, window.0, json!(1))["queued"], 1);
        let redrained = drain(&mut plane, &window, Some(16), 1);
        assert_eq!(redrained["drafts"][0]["sequence"], 17);
        // Acknowledging an already removed range again is idempotent.
        assert_eq!(
            drain(&mut plane, &window, Some(3), 1)["drafts"][0]["sequence"],
            17
        );
        assert_eq!(
            drain(&mut plane, &window, Some(18), 1),
            failure(ASSISTANCE_ACK_AHEAD)
        );
    }

    #[test]
    fn drain_limit_zero_is_invalid_and_does_not_acknowledge() {
        let (_, mut plane) = new_plane();
        let window = open(&mut plane, "a");
        submit(&mut plane, window.0, json!({"q": 1}));
        assert_eq!(
            drain(&mut plane, &window, Some(1), 0),
            failure(ASSISTANCE_DRAIN_LIMIT_INVALID)
        );
        assert_eq!(
            drain(&mut plane, &window, None, 1)["drafts"][0]["sequence"],
            1
        );
    }

    #[test]
    fn submit_and_drain_refuse_unknown_and_foreign_windows() {
        let (_, mut plane) = new_plane();
        let window = open(&mut plane, "a");
        let other = open(&mut plane, "b");
        let unknown = Incarnation::from_bytes([0x99; 16]);
        assert_eq!(
            submit(&mut plane, unknown, json!(1)),
            failure(WINDOW_UNKNOWN)
        );
        assert_eq!(
            drain(&mut plane, &(window.0, other.1), None, 8),
            failure(WINDOW_LEASE_INVALID)
        );
    }

    #[test]
    fn handoff_bounds_and_expiry() {
        let (clock, mut plane) = new_plane();
        let window = open(&mut plane, "a");
        assert_eq!(
            publish(&mut plane, &window, json!({}), 0),
            failure(HANDOFF_TTL_INVALID)
        );
        let exact = Value::String("x".repeat(HANDOFF_BYTES - 2));
        let published = publish(&mut plane, &window, exact, 10);
        assert_eq!(published["expires_at_ms"], 10);
        for _ in 1..HANDOFFS_PER_WINDOW {
            publish(&mut plane, &window, json!({}), 20_000);
        }
        assert_eq!(
            publish(&mut plane, &window, json!({}), 1),
            failure(HANDOFF_CAPACITY)
        );
        clock.store(10, Ordering::SeqCst);
        let id: Incarnation = serde_json::from_value(published["handoff_id"].clone()).unwrap();
        assert_eq!(
            call(
                &mut plane,
                ControlMethod::ExplorerHandoff { handoff_id: id }
            ),
            failure(HANDOFF_UNKNOWN)
        );
        assert_eq!(
            publish(&mut plane, &window, json!({}), 1)["status"],
            "handoff_published"
        );
    }

    #[test]
    fn window_expiry_drops_its_handoffs() {
        let (clock, mut plane) = new_plane();
        let window = open(&mut plane, "a");
        let published = publish(&mut plane, &window, json!({"k": 1}), HANDOFF_TTL_MS);
        let id: Incarnation = serde_json::from_value(published["handoff_id"].clone()).unwrap();
        let read = call(
            &mut plane,
            ControlMethod::ExplorerHandoff { handoff_id: id },
        );
        assert_eq!(
            read,
            json!({"status": "explorer_handoff", "handoff": {"k": 1}})
        );
        clock.store(TTL_MS, Ordering::SeqCst);
        assert_eq!(
            call(
                &mut plane,
                ControlMethod::ExplorerHandoff { handoff_id: id }
            ),
            failure(HANDOFF_UNKNOWN)
        );
    }

    fn selection(index: &str) -> WindowScopeSelection {
        serde_json::from_value(json!({
            "namespace": "0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a",
            "expected_catalog_revision": 12,
            "index_ids": [index],
            "project_ids": ["prj_02020202020202020202020202020202"],
        }))
        .unwrap()
    }

    fn grant(plane: &mut WindowPlane, window: &(Incarnation, WindowLease), index: &str) -> Value {
        call(
            plane,
            ControlMethod::WindowScopeGrant {
                window_id: window.0,
                lease: window.1.clone(),
                selection: selection(index),
            },
        )
    }

    #[test]
    fn scope_requires_lease_grant_and_live_window() {
        let (clock, mut plane) = new_plane();
        let window = open(&mut plane, "a");
        let other = open(&mut plane, "b");
        assert_eq!(
            plane.scope(&window.0, &window.1).unwrap_err(),
            WINDOW_SCOPE_NOT_GRANTED
        );
        let granted = grant(&mut plane, &window, "idx_01010101010101010101010101010101");
        assert_eq!(granted["status"], "window_scope_granted");
        let scope = plane.scope(&window.0, &window.1).unwrap();
        assert_eq!(
            scope.selection,
            selection("idx_01010101010101010101010101010101")
        );
        assert_eq!(scope.scope_identity, Sha256Digest::from_bytes([0x5c; 32]));
        assert_eq!(
            plane.scope(&window.0, &other.1).unwrap_err(),
            WINDOW_LEASE_INVALID
        );
        // A failed grant leaves the previous grant in place.
        let failed = grant(&mut plane, &window, "idx_0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f");
        assert_eq!(failed, failure("scoped_read_unavailable"));
        assert_eq!(
            plane.scope(&window.0, &window.1).unwrap().scope_identity,
            scope.scope_identity
        );
        clock.store(TTL_MS, Ordering::SeqCst);
        assert_eq!(
            plane.scope(&window.0, &window.1).unwrap_err(),
            WINDOW_UNKNOWN
        );
    }

    #[test]
    fn lease_never_appears_in_failures_or_debug() {
        let (_, mut plane) = new_plane();
        let window = open(&mut plane, "a");
        let other = open(&mut plane, "b");
        let refused = renew(&mut plane, (window.0, other.1.clone()));
        assert!(!refused.to_string().contains(other.1.as_str()));
        assert!(!refused.to_string().contains(window.1.as_str()));
        let granted = grant(&mut plane, &window, "idx_01010101010101010101010101010101");
        assert_eq!(granted["status"], "window_scope_granted");
        let debug = format!("{:?}", plane.scope(&window.0, &window.1).unwrap());
        assert!(!debug.contains(window.1.as_str()));
        assert!(leases_equal(&window.1, &window.1.clone()));
        assert!(!leases_equal(&window.1, &other.1));
    }

    // Selection authority step S1b (spec §3.2, §3.3, §3.7, N3, N4).

    const INDEX: &str = "idx_01010101010101010101010101010101";

    /// A plane whose registrar records every registered nonce and refuses
    /// while `refuse` is set.
    fn registering_plane(
        clock: &Arc<AtomicU64>,
    ) -> (Arc<Mutex<Vec<Incarnation>>>, Arc<AtomicU8>, WindowPlane) {
        let registered = Arc::new(Mutex::new(Vec::new()));
        let refuse = Arc::new(AtomicU8::new(0));
        let (log, gate) = (Arc::clone(&registered), Arc::clone(&refuse));
        let mut drawn = 0u8;
        let plane = fixture_plane(INSTANCE_1, clock).with_selection_authority(
            Box::new(move || {
                drawn += 1;
                Incarnation::from_bytes([0x90 + drawn; 16])
            }),
            Some(Box::new(move |selection: &SelectionScopeRef| {
                if gate.load(Ordering::SeqCst) != 0 {
                    return Err(());
                }
                log.lock().unwrap().push(selection.grant_nonce);
                Ok(())
            })),
        );
        (registered, refuse, plane)
    }

    fn nonce(plane: &WindowPlane, window: &(Incarnation, WindowLease)) -> Incarnation {
        plane
            .scope(&window.0, &window.1)
            .unwrap()
            .selection_ref
            .grant_nonce
    }

    fn dispatched(plane: &mut WindowPlane, method: ControlMethod) -> (Value, Vec<Incarnation>) {
        let (reply, dropped) = plane.dispatch(&method).expect("a window-plane method");
        (serde_json::to_value(reply).unwrap(), dropped.into_nonces())
    }

    #[test]
    fn a_stored_grant_registers_a_fresh_nonce_and_a_refused_registration_keeps_the_old_one() {
        let clock = Arc::new(AtomicU64::new(0));
        let (registered, refuse, mut plane) = registering_plane(&clock);
        let window = open(&mut plane, "a");
        assert_eq!(
            grant(&mut plane, &window, INDEX)["status"],
            "window_scope_granted"
        );
        let first = nonce(&plane, &window);
        assert_eq!(first, Incarnation::from_bytes([0x91; 16]));
        let scope = plane.scope(&window.0, &window.1).unwrap();
        assert_eq!(
            scope.selection_ref,
            SelectionScopeRef {
                owner: SelectionOwnerKind::Window,
                grant_nonce: first,
                service_instance: INSTANCE_1.parse().unwrap(),
                scope_identity: Sha256Digest::from_bytes([0x5c; 32]),
                catalog_revision: CatalogRevision::try_from(12u64).unwrap(),
            }
        );
        refuse.store(1, Ordering::SeqCst);
        assert_eq!(
            grant(&mut plane, &window, INDEX),
            failure("scoped_read_unavailable")
        );
        assert_eq!(nonce(&plane, &window), first, "the previous grant stays");
        assert_eq!(*registered.lock().unwrap(), vec![first]);
    }

    #[test]
    fn every_path_that_drops_a_grant_returns_its_nonce() {
        let clock = Arc::new(AtomicU64::new(0));
        let (_, _, mut plane) = registering_plane(&clock);
        let window = open(&mut plane, "a");
        let selection = selection(INDEX);
        let grant_method = |window: &(Incarnation, WindowLease)| ControlMethod::WindowScopeGrant {
            window_id: window.0,
            lease: window.1.clone(),
            selection: selection.clone(),
        };
        // A new grant replaces the old one: the old nonce is dropped.
        let (_, none) = dispatched(&mut plane, grant_method(&window));
        assert!(none.is_empty());
        let first = nonce(&plane, &window);
        let (reply, replaced) = dispatched(&mut plane, grant_method(&window));
        assert_eq!(reply["status"], "window_scope_granted");
        assert_eq!(replaced, vec![first]);
        // W4b release.
        let second = nonce(&plane, &window);
        let release = ControlMethod::WindowScopeRelease {
            window_id: window.0,
            lease: window.1.clone(),
        };
        assert_eq!(dispatched(&mut plane, release.clone()).1, vec![second]);
        assert!(dispatched(&mut plane, release).1.is_empty(), "idempotent");
        // W3 close.
        dispatched(&mut plane, grant_method(&window));
        let third = nonce(&plane, &window);
        let close = ControlMethod::WindowClose {
            window_id: window.0,
            lease: window.1.clone(),
        };
        assert_eq!(dispatched(&mut plane, close).1, vec![third]);
        // Expiry, swept by any window-plane method.
        let expiring = open(&mut plane, "b");
        dispatched(&mut plane, grant_method(&expiring));
        let fourth = nonce(&plane, &expiring);
        clock.store(TTL_MS, Ordering::SeqCst);
        let (_, swept) = dispatched(&mut plane, ControlMethod::AssistanceWindows {});
        assert_eq!(swept, vec![fourth]);
        // Service end.
        let last = open(&mut plane, "c");
        dispatched(&mut plane, grant_method(&last));
        let fifth = nonce(&plane, &last);
        assert_eq!(plane.end().into_nonces(), vec![fifth]);
        assert!(plane.end().into_nonces().is_empty());
    }

    fn checked(
        identity: u8,
    ) -> impl FnOnce(WindowScopeSelection) -> std::future::Ready<CheckResult> {
        move |selection| {
            std::future::ready(Ok((
                Sha256Digest::from_bytes([identity; 32]),
                selection.expected_catalog_revision,
            )))
        }
    }

    #[tokio::test]
    async fn the_w4_check_runs_with_the_windows_lock_released() {
        let clock = Arc::new(AtomicU64::new(0));
        let (_, _, mut plane) = registering_plane(&clock);
        let window = open(&mut plane, "a");
        let plane = Mutex::new(plane);
        let (reply, dropped) = grant_unlocked(
            &plane,
            &window.0,
            &window.1,
            &selection(INDEX),
            |selection| {
                assert!(plane.try_lock().is_ok(), "the check runs unlocked");
                checked(0x77)(selection)
            },
        )
        .await;
        assert!(dropped.into_nonces().is_empty());
        assert_eq!(
            serde_json::to_value(reply).unwrap(),
            json!({"status": "window_scope_granted", "scope_identity": "77".repeat(32),
                "catalog_revision": 12, "expires_at_ms": TTL_MS})
        );
        let (read, _) = read_unlocked(&plane, &window.0, &window.1, |selection| {
            assert!(plane.try_lock().is_ok(), "the read re-check runs unlocked");
            checked(0x77)(selection)
        })
        .await;
        assert_eq!(
            read.ok().unwrap().grant.scope_identity,
            Sha256Digest::from_bytes([0x77; 32])
        );
    }

    #[tokio::test]
    async fn a_window_closed_during_the_check_is_window_unknown() {
        let clock = Arc::new(AtomicU64::new(0));
        let (_, _, mut plane) = registering_plane(&clock);
        let window = open(&mut plane, "a");
        let plane = Mutex::new(plane);
        let close = ControlMethod::WindowClose {
            window_id: window.0,
            lease: window.1.clone(),
        };
        let (reply, _) = grant_unlocked(
            &plane,
            &window.0,
            &window.1,
            &selection(INDEX),
            |selection| {
                let _ = plane.lock().unwrap().dispatch(&close);
                checked(0x77)(selection)
            },
        )
        .await;
        assert_eq!(
            serde_json::to_value(reply).unwrap(),
            failure(WINDOW_UNKNOWN)
        );
        let reopened = open(&mut plane.lock().unwrap(), "b");
        assert_eq!(
            grant(&mut plane.lock().unwrap(), &reopened, INDEX)["status"],
            "window_scope_granted"
        );
        let identity = plane
            .lock()
            .unwrap()
            .scope(&reopened.0, &reopened.1)
            .unwrap();
        let close = ControlMethod::WindowClose {
            window_id: reopened.0,
            lease: reopened.1.clone(),
        };
        let (read, dropped) = read_unlocked(&plane, &reopened.0, &reopened.1, |selection| {
            let (_, closed) = plane.lock().unwrap().dispatch(&close).unwrap();
            assert_eq!(
                closed.into_nonces(),
                vec![identity.selection_ref.grant_nonce]
            );
            std::future::ready(Ok((
                identity.scope_identity,
                selection.expected_catalog_revision,
            )))
        })
        .await;
        assert_eq!(read.err().unwrap(), WINDOW_UNKNOWN);
        assert!(dropped.into_nonces().is_empty());
    }

    #[tokio::test]
    async fn a_concurrent_regrant_during_the_check_is_scoped_read_unavailable() {
        let clock = Arc::new(AtomicU64::new(0));
        let (_, _, mut plane) = registering_plane(&clock);
        let window = open(&mut plane, "a");
        let plane = Mutex::new(plane);
        // W4 racing W4: the one that stored under the lock first wins.
        let (reply, dropped) = grant_unlocked(
            &plane,
            &window.0,
            &window.1,
            &selection(INDEX),
            |selection| {
                assert_eq!(
                    grant(&mut plane.lock().unwrap(), &window, INDEX)["status"],
                    "window_scope_granted"
                );
                checked(0x77)(selection)
            },
        )
        .await;
        assert_eq!(
            serde_json::to_value(reply).unwrap(),
            failure("scoped_read_unavailable")
        );
        assert!(dropped.into_nonces().is_empty());
        let winner = plane.lock().unwrap().scope(&window.0, &window.1).unwrap();
        assert_eq!(winner.scope_identity, Sha256Digest::from_bytes([0x5c; 32]));
        // A read racing a re-grant.
        let (read, _) = read_unlocked(&plane, &window.0, &window.1, |selection| {
            assert_eq!(
                grant(&mut plane.lock().unwrap(), &window, INDEX)["status"],
                "window_scope_granted"
            );
            std::future::ready(Ok((
                winner.scope_identity,
                selection.expected_catalog_revision,
            )))
        })
        .await;
        assert_eq!(read.err().unwrap(), "scoped_read_unavailable");
    }

    #[tokio::test]
    async fn a_read_requires_a_project_and_exactly_the_stored_identity() {
        let clock = Arc::new(AtomicU64::new(0));
        let (_, _, mut plane) = registering_plane(&clock);
        let window = open(&mut plane, "a");
        let mut no_project = selection(INDEX);
        no_project.project_ids.clear();
        let method = ControlMethod::WindowScopeGrant {
            window_id: window.0,
            lease: window.1.clone(),
            selection: no_project,
        };
        // §3.5: granted, but its reads are refused before any check.
        assert_eq!(
            dispatched(&mut plane, method).0["status"],
            "window_scope_granted"
        );
        let plane = Mutex::new(plane);
        let (read, _) = read_unlocked(&plane, &window.0, &window.1, |_| -> std::future::Ready<_> {
            panic!("no check without a project")
        })
        .await;
        assert_eq!(read.err().unwrap(), "scoped_read_unavailable");
        assert_eq!(
            grant(&mut plane.lock().unwrap(), &window, INDEX)["status"],
            "window_scope_granted"
        );
        // Any other identity or revision (a catalog change) refuses.
        let (read, _) = read_unlocked(&plane, &window.0, &window.1, checked(0x77)).await;
        assert_eq!(read.err().unwrap(), "scoped_read_unavailable");
        let (read, _) = read_unlocked(&plane, &window.0, &window.1, |_| {
            std::future::ready(Err("scoped_read_unavailable".to_owned()))
        })
        .await;
        assert_eq!(read.err().unwrap(), "scoped_read_unavailable");
        let other = (window.0, WindowLease::from_bytes([0x33; 32]));
        let (read, _) = read_unlocked(&plane, &other.0, &other.1, checked(0x5d)).await;
        assert_eq!(read.err().unwrap(), WINDOW_LEASE_INVALID);
    }

    #[tokio::test]
    async fn expiry_swept_at_a_window_read_returns_the_grant_nonce() {
        let clock = Arc::new(AtomicU64::new(0));
        let (_, _, mut plane) = registering_plane(&clock);
        let window = open(&mut plane, "a");
        grant(&mut plane, &window, INDEX);
        let held = nonce(&plane, &window);
        clock.store(TTL_MS, Ordering::SeqCst);
        let plane = Mutex::new(plane);
        let (read, dropped) = read_unlocked(&plane, &window.0, &window.1, checked(0x5c)).await;
        assert_eq!(read.err().unwrap(), WINDOW_UNKNOWN);
        assert_eq!(dropped.into_nonces(), vec![held]);
    }

    #[tokio::test]
    async fn a_successful_read_renews_the_window_unless_its_grant_changed() {
        let clock = Arc::new(AtomicU64::new(0));
        let (_, _, mut plane) = registering_plane(&clock);
        let window = open(&mut plane, "a");
        grant(&mut plane, &window, INDEX);
        let plane = Mutex::new(plane);
        clock.store(TTL_MS - 1, Ordering::SeqCst);
        let (read, _) = read_unlocked(&plane, &window.0, &window.1, checked(0x5c)).await;
        let read = read.ok().unwrap();
        assert!(plane.lock().unwrap().renew_after_read(&read));
        clock.store(2 * TTL_MS - 2, Ordering::SeqCst);
        assert_eq!(
            renew(&mut plane.lock().unwrap(), window.clone())["status"],
            "window_renewed",
            "the read renewed the window (U99 Q5)"
        );
        let release = ControlMethod::WindowScopeRelease {
            window_id: window.0,
            lease: window.1.clone(),
        };
        let _ = plane.lock().unwrap().dispatch(&release);
        assert!(!plane.lock().unwrap().renew_after_read(&read));
    }

    #[test]
    fn the_grant_nonce_never_reaches_a_reply_failure_or_debug() {
        let clock = Arc::new(AtomicU64::new(0));
        let (_, refuse, mut plane) = registering_plane(&clock);
        let window = open(&mut plane, "a");
        let granted = grant(&mut plane, &window, INDEX);
        let held = nonce(&plane, &window).to_string();
        refuse.store(1, Ordering::SeqCst);
        let refused = grant(&mut plane, &window, INDEX);
        let debug = format!(
            "{:?} {:?}",
            plane.scope(&window.0, &window.1).unwrap(),
            plane.end()
        );
        for text in [granted.to_string(), refused.to_string(), debug] {
            assert!(!text.contains(&held), "{text}");
            assert!(!text.contains(window.1.as_str()), "{text}");
        }
    }
}
