// SPDX-License-Identifier: Apache-2.0
//! Inert native admission ledger. The existing actors own durable intent and IPC.
//! Every method is synchronous; no registry, source, transport or worker wait is
//! performed while the private mutex is held. Tickets are metadata, not bearers.
// Remove this inert-only allowance when the native coordinator is wired.
#![allow(dead_code)]
use airk_core::{
    catalog_management::{CatalogOperation, CatalogRequest},
    identity::{Incarnation, Sha256Digest},
    index_catalog::{
        CatalogRevision, GrantId, IndexId, MAX_SELECTION, QueryProjectId, encoded_size,
    },
    scoped_reads::ScopeReadRefused,
    service::{RequestKey, ScopedReadRef, WorkerIdentity},
};
use airk_kernel::scoped_reads::ScopedReadAuthority;
pub(crate) use airk_store::service::registry::catalog::operations::FenceTarget;
use airk_store::service::registry::catalog::operations::{
    FenceConfirmation, FenceConfirmationMethod, FenceIntent, FencePlan as StoreFencePlan,
    FencedReceipt,
};
use anyhow::{Result, ensure};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const MAX_SCOPES: usize = 128;
const MAX_WORKERS: usize = 128;
const MAX_PLAN_BYTES: usize = 16 * 1024;
const MAX_LIFETIME: Duration = Duration::from_secs(300);

#[derive(Clone)]
pub(crate) struct NativeCatalogAdmission(Arc<Mutex<State>>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IssueTicket {
    service: Incarnation,
    namespace: Incarnation,
    revision: CatalogRevision,
    generation: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FenceKey {
    // Supplied by the canonical Store operation-key helper; never invented by
    // this adapter or allocated afresh on a retry.
    pub operation_id: Incarnation,
    pub caller: Incarnation,
    pub key: RequestKey,
    pub request_digest: Sha256Digest,
    pub expected_revision: CatalogRevision,
}
impl FenceKey {
    pub fn new(request: &CatalogRequest) -> Result<Self> {
        request.validate()?;
        Ok(Self {
            operation_id: airk_store::service::registry::catalog::operations::catalog_operation_id(
                request.caller,
                &request.key,
            )?,
            caller: request.caller,
            key: request.key.clone(),
            request_digest: request.identity()?,
            expected_revision: request.expected_catalog_revision,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FenceTicket {
    key: FenceKey,
    generation: u64,
    service: Incarnation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InstallTicket {
    issue: IssueTicket,
    nonce: u64,
    worker: WorkerIdentity,
    scope: ScopedReadRef,
    previous: Option<ScopedReadRef>,
    requires_install: bool,
    expires: Instant,
}
impl InstallTicket {
    pub fn scope(&self) -> ScopedReadRef {
        self.scope
    }
    pub fn worker(&self) -> WorkerIdentity {
        self.worker
    }
    pub fn previous(&self) -> Option<ScopedReadRef> {
        self.previous
    }
    pub fn requires_install(&self) -> bool {
        self.requires_install
    }
    pub fn expires(&self) -> Instant {
        self.expires
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ScopeTarget {
    Grant(GrantId),
    Membership {
        project_id: QueryProjectId,
        index_id: IndexId,
    },
    Project(QueryProjectId),
    /// Requires confirmed reap of every registered worker. No in-process reopen;
    /// authority promotion starts a fresh service after durable maintenance.
    LegacyPromotion,
}
impl ScopeTarget {
    pub fn for_operation(operation: &CatalogOperation) -> Result<Self> {
        Ok(match operation {
            CatalogOperation::DeleteProject { id, .. } => Self::Project(*id),
            CatalogOperation::SetMembership { membership, .. } => Self::Membership {
                project_id: membership.project_id,
                index_id: membership.index_id,
            },
            CatalogOperation::RemoveMembership {
                project_id,
                index_id,
                ..
            } => Self::Membership {
                project_id: *project_id,
                index_id: *index_id,
            },
            CatalogOperation::RevokeGrant { id, .. } => Self::Grant(*id),
            _ => return Err(ScopeReadRefused.into()),
        })
    }
}
#[derive(Clone, Debug)]
pub(crate) struct FencePlan {
    pub ticket: FenceTicket,
    pub targets: Vec<FenceTarget>,
    pub drain_workers: Vec<WorkerIdentity>,
    pub durable: StoreFencePlan,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Installing,
    Installed,
    Revoked,
    Expired,
}
#[derive(Clone)]
struct Installed {
    worker: WorkerIdentity,
    scope: ScopedReadRef,
    projects: Vec<QueryProjectId>,
    expires: Instant,
    nonce: u64,
    phase: Phase,
    pending_previous: Option<ScopedReadRef>,
}
impl Installed {
    fn matches(&self, target: ScopeTarget) -> bool {
        match target {
            ScopeTarget::Grant(id) => self.scope.grant_id == id,
            ScopeTarget::Membership {
                project_id,
                index_id,
            } => self.scope.index_id == index_id && self.projects.contains(&project_id),
            ScopeTarget::Project(id) => self.projects.contains(&id),
            ScopeTarget::LegacyPromotion => true,
        }
    }
}
struct PendingFence {
    ticket: FenceTicket,
    targets: Vec<FenceTarget>,
    acknowledgements: Vec<FenceConfirmation>,
    drain_workers: Vec<WorkerIdentity>,
    target: ScopeTarget,
    durable: StoreFencePlan,
    effects_started: bool,
}
impl PendingFence {
    fn confirmed(&self, target: &FenceTarget) -> bool {
        self.acknowledgements
            .iter()
            .any(|confirmation| confirmation.target == *target)
    }
    fn confirmations(&self) -> Vec<FenceConfirmation> {
        self.targets
            .iter()
            .filter_map(|target| {
                self.acknowledgements
                    .iter()
                    .find(|confirmation| confirmation.target == *target)
                    .cloned()
            })
            .collect()
    }
}
struct State {
    service: Incarnation,
    namespace: Incarnation,
    revision: CatalogRevision,
    authority_contract: u32,
    generation: u64,
    next_nonce: u64,
    workers: BTreeMap<Incarnation, WorkerIdentity>,
    retired_workers: BTreeMap<Incarnation, WorkerIdentity>,
    installed: BTreeMap<(Incarnation, GrantId, IndexId, Sha256Digest), Installed>,
    highwater: BTreeMap<(Incarnation, GrantId, IndexId), CatalogRevision>,
    lease_highwater: BTreeMap<Incarnation, u64>,
    pending: Option<PendingFence>,
}
impl State {
    fn ticket(&self) -> IssueTicket {
        IssueTicket {
            service: self.service,
            namespace: self.namespace,
            revision: self.revision,
            generation: self.generation,
        }
    }
    fn check_issue(&self, ticket: IssueTicket) -> Result<()> {
        ensure!(
            self.authority_contract == 1 && self.pending.is_none() && ticket == self.ticket(),
            ScopeReadRefused
        );
        Ok(())
    }
    fn worker(&self, identity: WorkerIdentity) -> Result<()> {
        ensure!(
            self.workers.get(&identity.worker) == Some(&identity),
            ScopeReadRefused
        );
        Ok(())
    }
    fn fence(&self, ticket: &FenceTicket) -> Result<&PendingFence> {
        let pending = self.pending.as_ref().ok_or(ScopeReadRefused)?;
        ensure!(
            &pending.ticket == ticket
                && ticket.service == self.service
                && ticket.generation == self.generation,
            ScopeReadRefused
        );
        Ok(pending)
    }
    fn reserve_numbers(&mut self, worker: Incarnation) -> Result<(u64, u64)> {
        let nonce = self.next_nonce;
        let next_nonce = nonce.checked_add(1).ok_or(ScopeReadRefused)?;
        let generation = self
            .lease_highwater
            .get(&worker)
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(ScopeReadRefused)?;
        ensure!(generation.checked_add(1).is_some(), ScopeReadRefused);
        self.next_nonce = next_nonce;
        self.lease_highwater.insert(worker, generation);
        Ok((nonce, generation))
    }
}

impl NativeCatalogAdmission {
    pub fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn require_service(&self, service: Incarnation) -> Result<()> {
        ensure!(
            self.0.lock().map_err(|_| ScopeReadRefused)?.service == service,
            ScopeReadRefused
        );
        Ok(())
    }
    pub fn worker_was_reaped(&self, worker: WorkerIdentity) -> Result<bool> {
        let state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        Ok(state.retired_workers.get(&worker.worker) == Some(&worker))
    }
    pub fn new(
        service: Incarnation,
        namespace: Incarnation,
        revision: CatalogRevision,
        authority_contract: u32,
    ) -> Result<Self> {
        ensure!(authority_contract <= 1, ScopeReadRefused);
        Ok(Self(Arc::new(Mutex::new(State {
            service,
            namespace,
            revision,
            authority_contract,
            generation: 1,
            next_nonce: 1,
            workers: BTreeMap::new(),
            retired_workers: BTreeMap::new(),
            installed: BTreeMap::new(),
            highwater: BTreeMap::new(),
            lease_highwater: BTreeMap::new(),
            pending: None,
        }))))
    }
    pub fn register_worker(&self, identity: WorkerIdentity) -> Result<()> {
        let mut state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        ensure!(
            state.pending.is_none()
                && identity.service == state.service
                && identity.fencing_generation > 0,
            ScopeReadRefused
        );
        if let Some(existing) = state.workers.get(&identity.worker) {
            ensure!(*existing == identity, ScopeReadRefused);
            return Ok(());
        }
        ensure!(
            !state.retired_workers.contains_key(&identity.worker)
                && state.workers.len() + state.retired_workers.len() < MAX_WORKERS,
            ScopeReadRefused
        );
        state.workers.insert(identity.worker, identity);
        Ok(())
    }
    pub fn begin_issue(
        &self,
        namespace: Incarnation,
        revision: CatalogRevision,
    ) -> Result<IssueTicket> {
        let state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        ensure!(
            state.namespace == namespace && state.revision == revision,
            ScopeReadRefused
        );
        let ticket = state.ticket();
        state.check_issue(ticket)?;
        Ok(ticket)
    }
    pub fn check_issue(&self, ticket: IssueTicket) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| ScopeReadRefused)?
            .check_issue(ticket)
    }
    pub fn check_legacy_admission(&self) -> Result<()> {
        let state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        ensure!(
            state.authority_contract == 0 && state.pending.is_none(),
            ScopeReadRefused
        );
        Ok(())
    }
    pub fn reserve_install(
        &self,
        issue: IssueTicket,
        worker: WorkerIdentity,
        authority: &ScopedReadAuthority,
        expires: Instant,
    ) -> Result<InstallTicket> {
        self.reserve_lease(issue, worker, authority, expires, None)
    }
    /// Native-only decision after the actor freshly ran the canonical builder.
    /// An uncertain renewal is replayed with its original exact previous lease.
    pub fn reserve_validated_scope(
        &self,
        issue: IssueTicket,
        worker: WorkerIdentity,
        authority: &ScopedReadAuthority,
        expires: Instant,
    ) -> Result<InstallTicket> {
        let fingerprint = authority.fingerprint()?;
        let previous = {
            let state = self.0.lock().map_err(|_| ScopeReadRefused)?;
            state.check_issue(issue)?;
            state.worker(worker)?;
            state
                .installed
                .get(&(
                    worker.worker,
                    authority.grant.id,
                    authority.index_id,
                    fingerprint,
                ))
                .and_then(|entry| {
                    if entry.phase == Phase::Installing {
                        entry.pending_previous
                    } else if matches!(entry.phase, Phase::Installed | Phase::Expired)
                        && entry.expires <= Instant::now()
                    {
                        Some(entry.scope)
                    } else {
                        None
                    }
                })
        };
        self.reserve_lease(issue, worker, authority, expires, previous)
    }
    /// Caller has freshly revalidated this immutable payload against the native
    /// catalog. Expiry alone never authorizes renewal or clears logical revoke.
    pub fn renew_expired_lease(
        &self,
        issue: IssueTicket,
        worker: WorkerIdentity,
        authority: &ScopedReadAuthority,
        previous: ScopedReadRef,
        expires: Instant,
    ) -> Result<InstallTicket> {
        self.reserve_lease(issue, worker, authority, expires, Some(previous))
    }
    fn reserve_lease(
        &self,
        issue: IssueTicket,
        worker: WorkerIdentity,
        authority: &ScopedReadAuthority,
        expires: Instant,
        previous: Option<ScopedReadRef>,
    ) -> Result<InstallTicket> {
        // Validate and measure immutable payload before taking the gate mutex.
        let mut scope = ScopedReadRef {
            schema: 1,
            lease_generation: 0,
            index_id: authority.index_id,
            grant_id: authority.grant.id,
            grant_revision: authority.grant.revision,
            fingerprint: authority.fingerprint()?,
        };
        ensure!(
            authority.backing == worker.project && authority.project_ids.len() <= MAX_SELECTION,
            ScopeReadRefused
        );
        let mut state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        state.check_issue(issue)?;
        state.worker(worker)?;
        ensure!(worker.service == state.service, ScopeReadRefused);
        let now = Instant::now();
        ensure!(
            expires > now && expires.duration_since(now) <= MAX_LIFETIME,
            ScopeReadRefused
        );
        let family = (worker.worker, scope.grant_id, scope.index_id);
        ensure!(
            state
                .highwater
                .get(&family)
                .is_none_or(|revision| scope.grant_revision >= *revision),
            ScopeReadRefused
        );
        let key = (
            worker.worker,
            scope.grant_id,
            scope.index_id,
            scope.fingerprint,
        );
        if let Some(existing) = state.installed.get(&key).cloned() {
            ensure!(
                existing.worker == worker
                    && existing.scope.grant_revision == scope.grant_revision
                    && existing.phase != Phase::Revoked,
                ScopeReadRefused
            );
            if existing.expires > now {
                ensure!(previous == existing.pending_previous, ScopeReadRefused);
                let requires_install =
                    existing.phase == Phase::Installing || expires < existing.expires;
                let entry = state.installed.get_mut(&key).ok_or(ScopeReadRefused)?;
                entry.expires = entry.expires.min(expires);
                if requires_install {
                    entry.phase = Phase::Installing;
                }
                return Ok(InstallTicket {
                    issue,
                    nonce: existing.nonce,
                    worker,
                    scope: existing.scope,
                    previous: existing.pending_previous,
                    requires_install,
                    expires: entry.expires,
                });
            }
            ensure!(
                matches!(existing.phase, Phase::Installed | Phase::Expired)
                    && previous == Some(existing.scope)
                    && existing.pending_previous.is_none(),
                ScopeReadRefused
            );
            let (nonce, generation) = state.reserve_numbers(worker.worker)?;
            scope.lease_generation = generation;
            state.installed.insert(
                key,
                Installed {
                    worker,
                    scope,
                    projects: existing.projects,
                    expires,
                    nonce,
                    phase: Phase::Installing,
                    pending_previous: Some(existing.scope),
                },
            );
            return Ok(InstallTicket {
                issue,
                nonce,
                worker,
                scope,
                previous: Some(existing.scope),
                requires_install: true,
                expires,
            });
        }
        ensure!(
            previous.is_none() && state.installed.len() < MAX_SCOPES,
            ScopeReadRefused
        );
        if state
            .highwater
            .get(&family)
            .is_some_and(|revision| scope.grant_revision > *revision)
        {
            ensure!(
                state
                    .installed
                    .values()
                    .filter(|entry| (
                        entry.worker.worker,
                        entry.scope.grant_id,
                        entry.scope.index_id
                    ) == family)
                    .all(|entry| entry.phase == Phase::Revoked),
                ScopeReadRefused
            );
        }
        let (nonce, generation) = state.reserve_numbers(worker.worker)?;
        scope.lease_generation = generation;
        state.highwater.insert(family, scope.grant_revision);
        state.installed.insert(
            key,
            Installed {
                worker,
                scope,
                projects: authority.project_ids.clone(),
                expires,
                nonce,
                phase: Phase::Installing,
                pending_previous: None,
            },
        );
        Ok(InstallTicket {
            issue,
            nonce,
            worker,
            scope,
            previous: None,
            requires_install: true,
            expires,
        })
    }
    pub fn acknowledge_install(
        &self,
        ticket: InstallTicket,
        worker: WorkerIdentity,
        scope: ScopedReadRef,
    ) -> Result<()> {
        let mut state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        state.check_issue(ticket.issue)?;
        state.worker(worker)?;
        ensure!(
            worker == ticket.worker && scope == ticket.scope,
            ScopeReadRefused
        );
        let entry = state
            .installed
            .get_mut(&(
                worker.worker,
                scope.grant_id,
                scope.index_id,
                scope.fingerprint,
            ))
            .ok_or(ScopeReadRefused)?;
        ensure!(
            entry.worker == worker
                && entry.scope == scope
                && entry.nonce == ticket.nonce
                && entry.expires == ticket.expires
                && matches!(entry.phase, Phase::Installing | Phase::Installed)
                && Instant::now() < entry.expires,
            ScopeReadRefused
        );
        entry.phase = Phase::Installed;
        // Only the exact private ACK proves replacement fenced the old lease.
        entry.pending_previous = None;
        Ok(())
    }
    pub fn check_scope_issue(
        &self,
        ticket: IssueTicket,
        worker: WorkerIdentity,
        scope: ScopedReadRef,
    ) -> Result<()> {
        self.with_scope_issue(ticket, worker, scope, || ())
    }
    /// The synchronous effect may poll a bounded transport write. It must never
    /// wait or perform RPC/registry I/O. The guard spans the actual poll so an
    /// admission close cannot be acknowledged between its check and that poll.
    pub fn with_scope_issue<T>(
        &self,
        ticket: IssueTicket,
        worker: WorkerIdentity,
        scope: ScopedReadRef,
        effect: impl FnOnce() -> T,
    ) -> Result<T> {
        let state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        state.check_issue(ticket)?;
        state.worker(worker)?;
        scope.validate()?;
        let entry = state
            .installed
            .get(&(
                worker.worker,
                scope.grant_id,
                scope.index_id,
                scope.fingerprint,
            ))
            .ok_or(ScopeReadRefused)?;
        ensure!(
            entry.worker == worker
                && entry.scope == scope
                && entry.phase == Phase::Installed
                && state
                    .highwater
                    .get(&(worker.worker, scope.grant_id, scope.index_id))
                    == Some(&scope.grant_revision)
                && Instant::now() < entry.expires,
            ScopeReadRefused
        );
        Ok(effect())
    }
    pub fn close_request_admission(&self, request: &CatalogRequest) -> Result<FencePlan> {
        self.close_admission(
            FenceKey::new(request)?,
            ScopeTarget::for_operation(&request.operation)?,
        )
    }
    pub fn close_promotion_admission(&self, key: FenceKey) -> Result<FencePlan> {
        self.close_admission(key, ScopeTarget::LegacyPromotion)
    }
    fn close_admission(&self, key: FenceKey, target: ScopeTarget) -> Result<FencePlan> {
        let mut state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        if let Some(pending) = &state.pending {
            ensure!(
                pending.ticket.key == key && pending.target == target,
                ScopeReadRefused
            );
            return Ok(FencePlan {
                ticket: pending.ticket.clone(),
                targets: pending.targets.clone(),
                drain_workers: pending.drain_workers.clone(),
                durable: pending.durable.clone(),
            });
        }
        ensure!(
            key.key.namespace == state.namespace && key.expected_revision == state.revision,
            ScopeReadRefused
        );
        let generation = state.generation.checked_add(1).ok_or(ScopeReadRefused)?;
        let targets: Vec<_> = state
            .installed
            .values()
            .filter(|entry| entry.matches(target) && entry.phase != Phase::Revoked)
            .flat_map(|entry| {
                std::iter::once(entry.scope)
                    .chain(entry.pending_previous)
                    .map(|scope| FenceTarget {
                        worker: entry.worker,
                        scope,
                    })
            })
            .collect();
        let durable = StoreFencePlan::new(state.service, generation, targets)?;
        let targets = durable.targets.clone();
        let drain_workers: Vec<_> = if target == ScopeTarget::LegacyPromotion {
            state.workers.values().copied().collect()
        } else {
            Vec::new()
        };
        encoded_size(&(&targets, &drain_workers), MAX_PLAN_BYTES)?;
        let ticket = FenceTicket {
            key,
            generation,
            service: state.service,
        };
        state.generation = generation;
        state.pending = Some(PendingFence {
            ticket: ticket.clone(),
            targets: targets.clone(),
            acknowledgements: Vec::new(),
            drain_workers: drain_workers.clone(),
            target,
            durable: durable.clone(),
            effects_started: false,
        });
        Ok(FencePlan {
            ticket,
            targets,
            drain_workers,
            durable,
        })
    }
    pub fn record_revoked(
        &self,
        ticket: &FenceTicket,
        worker: WorkerIdentity,
        scope: ScopedReadRef,
    ) -> Result<()> {
        let mut state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        let target = FenceTarget { worker, scope };
        ensure!(
            state.fence(ticket)?.targets.contains(&target),
            ScopeReadRefused
        );
        if state.fence(ticket)?.confirmed(&target) {
            return Ok(());
        }
        state.worker(worker)?;
        let entry = state
            .installed
            .get_mut(&(
                worker.worker,
                scope.grant_id,
                scope.index_id,
                scope.fingerprint,
            ))
            .ok_or(ScopeReadRefused)?;
        ensure!(
            entry.worker == worker
                && (entry.scope == scope || entry.pending_previous == Some(scope)),
            ScopeReadRefused
        );
        entry.phase = Phase::Revoked;
        let pending = state.pending.as_mut().ok_or(ScopeReadRefused)?;
        pending.effects_started = true;
        if !pending.confirmed(&target) {
            pending.acknowledgements.push(FenceConfirmation {
                target,
                method: FenceConfirmationMethod::NativeScopeAck,
            });
        }
        Ok(())
    }
    pub fn require_commit_ready(&self, ticket: &FenceTicket) -> Result<Vec<FenceTarget>> {
        let state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        let pending = state.fence(ticket)?;
        ensure!(
            pending.drain_workers.is_empty()
                && pending
                    .targets
                    .iter()
                    .all(|target| pending.confirmed(target)),
            ScopeReadRefused
        );
        Ok(pending.targets.clone())
    }
    /// Complete native confirmation methods, in canonical target order.
    pub fn confirmations(&self, ticket: &FenceTicket) -> Result<Vec<FenceConfirmation>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| ScopeReadRefused)?
            .fence(ticket)?
            .confirmations())
    }
    /// Called only by the native supervisor after actual child/descendant/lease
    /// reap. Pipe EOF, elapsed expiry and a numerical PID are never sufficient.
    pub fn observe_worker_reaped(&self, worker: WorkerIdentity) -> Result<()> {
        let mut state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        if let Some(previous) = state.retired_workers.get(&worker.worker) {
            ensure!(*previous == worker, ScopeReadRefused);
            return Ok(());
        }
        state.worker(worker)?;
        state.workers.remove(&worker.worker);
        state.retired_workers.insert(worker.worker, worker);
        state.installed.retain(|_, entry| entry.worker != worker);
        state
            .highwater
            .retain(|(owner, _, _), _| *owner != worker.worker);
        state.lease_highwater.remove(&worker.worker);
        if let Some(pending) = state.pending.as_mut() {
            if pending.targets.iter().any(|target| target.worker == worker)
                || pending.drain_workers.contains(&worker)
            {
                pending.effects_started = true;
            }
            pending.drain_workers.retain(|expected| *expected != worker);
            for target in &pending.targets {
                if target.worker == worker && !pending.confirmed(target) {
                    pending.acknowledgements.push(FenceConfirmation {
                        target: target.clone(),
                        method: FenceConfirmationMethod::NativeWorkerReap,
                    });
                }
            }
        }
        Ok(())
    }
    pub fn expire(&self) -> Result<()> {
        let mut state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        let now = Instant::now();
        for entry in state.installed.values_mut() {
            if entry.phase != Phase::Revoked && now >= entry.expires {
                entry.phase = Phase::Expired;
            }
        }
        Ok(())
    }
    pub fn observe_catalog_commit(
        &self,
        previous: CatalogRevision,
        next: CatalogRevision,
    ) -> Result<()> {
        let mut state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        ensure!(
            state.pending.is_none() && state.revision == previous && previous.next()? == next,
            ScopeReadRefused
        );
        let generation = state.generation.checked_add(1).ok_or(ScopeReadRefused)?;
        state.revision = next;
        state.generation = generation;
        Ok(())
    }
    pub fn finish_commit(
        &self,
        ticket: &FenceTicket,
        request: &CatalogRequest,
        preview_digest: Sha256Digest,
        prepared_digest: Sha256Digest,
        receipt: &FencedReceipt,
    ) -> Result<()> {
        let mut state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        let pending = state.fence(ticket)?;
        ensure!(
            pending.target != ScopeTarget::LegacyPromotion
                && pending.target == ScopeTarget::for_operation(&request.operation)?
                && FenceKey::new(request)? == ticket.key
                && pending.drain_workers.is_empty()
                && pending
                    .targets
                    .iter()
                    .all(|target| pending.confirmed(target)),
            ScopeReadRefused
        );
        receipt.validate_for(request, preview_digest, prepared_digest, &pending.durable)?;
        ensure!(
            receipt.proof.confirmations == pending.confirmations(),
            ScopeReadRefused
        );
        ensure!(
            receipt.metadata.operation_id == ticket.key.operation_id
                && receipt.metadata.catalog_revision == state.revision.next()?,
            ScopeReadRefused
        );
        let generation = state.generation.checked_add(1).ok_or(ScopeReadRefused)?;
        state.revision = receipt.metadata.catalog_revision;
        state.generation = generation;
        state.pending = None;
        Ok(())
    }
    /// Reconstitute a durable, still-closed operation before publishing native
    /// admission. Unresolved predecessor scopes may only settle via exact native
    /// acknowledgement or supervisor-confirmed actual reap, never by new grants.
    pub fn restore_pending_fence(&self, intent: &FenceIntent) -> Result<FencePlan> {
        intent.validate()?;
        let mut state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        ensure!(
            state.pending.is_none()
                && state.installed.is_empty()
                && state.workers.is_empty()
                && state.retired_workers.is_empty()
                && intent.request.key.namespace == state.namespace
                && intent.request.expected_catalog_revision == state.revision,
            ScopeReadRefused
        );
        let generation = state.generation.checked_add(1).ok_or(ScopeReadRefused)?;
        let key = FenceKey::new(&intent.request)?;
        ensure!(key.operation_id == intent.operation_id, ScopeReadRefused);
        let target = ScopeTarget::for_operation(&intent.request.operation)?;
        let ticket = FenceTicket {
            key,
            generation,
            service: state.service,
        };
        let mut workers = BTreeMap::new();
        let mut installed: BTreeMap<(Incarnation, GrantId, IndexId, Sha256Digest), Installed> =
            BTreeMap::new();
        let mut highwater = BTreeMap::new();
        let mut lease_highwater = BTreeMap::new();
        for target in &intent.plan.targets {
            if let Some(previous) = workers.insert(target.worker.worker, target.worker) {
                ensure!(previous == target.worker, ScopeReadRefused);
            }
            let family = (
                target.worker.worker,
                target.scope.grant_id,
                target.scope.index_id,
            );
            highwater
                .entry(family)
                .and_modify(|revision: &mut CatalogRevision| {
                    *revision = (*revision).max(target.scope.grant_revision)
                })
                .or_insert(target.scope.grant_revision);
            lease_highwater
                .entry(target.worker.worker)
                .and_modify(|generation: &mut u64| {
                    *generation = (*generation).max(target.scope.lease_generation)
                })
                .or_insert(target.scope.lease_generation);
            let key = (
                target.worker.worker,
                target.scope.grant_id,
                target.scope.index_id,
                target.scope.fingerprint,
            );
            if let Some(entry) = installed.get_mut(&key) {
                ensure!(
                    entry.worker == target.worker
                        && entry.pending_previous.is_none()
                        && entry.scope.grant_revision == target.scope.grant_revision,
                    ScopeReadRefused
                );
                if target.scope.lease_generation > entry.scope.lease_generation {
                    entry.pending_previous = Some(entry.scope);
                    entry.scope = target.scope;
                } else {
                    entry.pending_previous = Some(target.scope);
                }
            } else {
                installed.insert(
                    key,
                    Installed {
                        worker: target.worker,
                        scope: target.scope,
                        projects: Vec::new(),
                        expires: Instant::now(),
                        nonce: 0,
                        phase: Phase::Expired,
                        pending_previous: None,
                    },
                );
            }
        }
        for entry in installed.values_mut() {
            if intent.confirmations.iter().any(|confirmation| {
                confirmation.target
                    == FenceTarget {
                        worker: entry.worker,
                        scope: entry.scope,
                    }
            }) {
                entry.phase = Phase::Revoked;
            }
        }
        ensure!(workers.len() <= MAX_WORKERS, ScopeReadRefused);
        state.workers = workers;
        state.installed = installed;
        state.highwater = highwater;
        state.lease_highwater = lease_highwater;
        state.generation = generation;
        state.pending = Some(PendingFence {
            ticket: ticket.clone(),
            targets: intent.plan.targets.clone(),
            acknowledgements: intent.confirmations.clone(),
            drain_workers: Vec::new(),
            target,
            durable: intent.plan.clone(),
            effects_started: !intent.confirmations.is_empty(),
        });
        // Persisted exact acknowledgements stay valid; no unacknowledged target
        // receives an invented live installation or a reusable bearer.
        Ok(FencePlan {
            ticket,
            targets: intent.plan.targets.clone(),
            drain_workers: Vec::new(),
            durable: intent.plan.clone(),
        })
    }
    pub fn matching_plan(&self, request: &CatalogRequest) -> Result<Option<FencePlan>> {
        let key = FenceKey::new(request)?;
        let state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        Ok(state
            .pending
            .as_ref()
            .filter(|pending| pending.ticket.key == key)
            .map(|pending| FencePlan {
                ticket: pending.ticket.clone(),
                targets: pending.targets.clone(),
                drain_workers: pending.drain_workers.clone(),
                durable: pending.durable.clone(),
            }))
    }
    pub fn begin_revocations(&self, ticket: &FenceTicket) -> Result<()> {
        let mut state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        state.fence(ticket)?;
        state
            .pending
            .as_mut()
            .ok_or(ScopeReadRefused)?
            .effects_started = true;
        Ok(())
    }
    pub fn validate_target(&self, ticket: &FenceTicket, target: &FenceTarget) -> Result<()> {
        ensure!(
            self.0
                .lock()
                .map_err(|_| ScopeReadRefused)?
                .fence(ticket)?
                .targets
                .contains(target),
            ScopeReadRefused
        );
        Ok(())
    }
    pub fn abort_known_unprepared(
        &self,
        ticket: &FenceTicket,
        proof: &crate::catalog_coordinator::KnownUnprepared,
    ) -> Result<()> {
        let mut state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        let pending = state.fence(ticket)?;
        ensure!(
            proof.matches(&ticket.key)
                && !pending.effects_started
                && pending.acknowledgements.is_empty()
                && state.revision == ticket.key.expected_revision,
            ScopeReadRefused
        );
        let generation = state.generation.checked_add(1).ok_or(ScopeReadRefused)?;
        state.generation = generation;
        state.pending = None;
        Ok(())
    }
    pub fn reconcile_terminal(
        &self,
        request: &CatalogRequest,
        receipt: &FencedReceipt,
    ) -> Result<()> {
        let plan = StoreFencePlan::new(
            receipt.proof.service,
            receipt.proof.gate_generation,
            receipt
                .proof
                .confirmations
                .iter()
                .map(|confirmation| confirmation.target.clone())
                .collect(),
        )?;
        receipt.validate_for(
            request,
            receipt.preview_digest,
            receipt.prepared_digest,
            &plan,
        )?;
        if let Some(pending) = self.matching_plan(request)? {
            return self.finish_commit(
                &pending.ticket,
                request,
                receipt.preview_digest,
                receipt.prepared_digest,
                receipt,
            );
        }
        let state = self.0.lock().map_err(|_| ScopeReadRefused)?;
        ensure!(
            request.key.namespace == state.namespace
                && state.revision >= receipt.metadata.catalog_revision,
            ScopeReadRefused
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "catalog_admission_tests.rs"]
mod tests;
