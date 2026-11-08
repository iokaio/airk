// SPDX-License-Identifier: Apache-2.0
//! Bounded broker/admission policy. This is not an IPC server or an OS ownership lock.
//!
//! Adapters must authenticate peers and establish exclusive worker ownership before
//! registration. All transitions require exclusive access; no query payload is stored.
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{
        AdmissionError as Error, BrokerStatus, ConnectionGrant, MAX_FRAME_BYTES, MAX_GRANT_TTL_MS,
        PROTOCOL_VERSION, ProjectBinding, QueryAllowance, WorkerIdentity,
    },
};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug)]
pub struct BrokerLimits {
    pub reserved_status: bool,
    pub projects: usize,
    pub pending_grants: usize,
    pub connections_per_project: usize,
    pub queries_per_connection: usize,
    pub queries_per_caller: usize,
    pub total: QueryAllowance,
}

impl Default for BrokerLimits {
    fn default() -> Self {
        Self {
            reserved_status: false,
            projects: 1024,
            pending_grants: 4096,
            connections_per_project: 64,
            queries_per_connection: 4,
            queries_per_caller: 4,
            total: QueryAllowance {
                concurrent_queries: 32,
                response_bytes: 32 * MAX_FRAME_BYTES,
            },
        }
    }
}
impl BrokerLimits {
    /// Finite aggregate metadata allowance; each project receives its configured grant.
    pub fn service() -> Self {
        Self {
            reserved_status: true,
            connections_per_project: 1024,
            queries_per_caller: 65,
            total: QueryAllowance {
                concurrent_queries: 4096,
                response_bytes: 4096 * MAX_FRAME_BYTES,
            },
            ..Self::default()
        }
    }
}

struct Pending {
    grant: ConnectionGrant,
    expires: Instant,
    purpose: SessionPurpose,
}

struct Connection {
    caller: Incarnation,
    expires: Instant,
    active: usize,
    purpose: SessionPurpose,
}

/// Native adapter purpose, retained only in the existing authenticated broker
/// state. It is deliberately absent from ConnectionGrant and all wire inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionPurpose {
    Evidence,
    NativeAdministrativeRead,
}

struct Worker {
    identity: WorkerIdentity,
    allowance: QueryAllowance,
    expires: Instant,
    draining: bool,
    connections: BTreeMap<Incarnation, Connection>,
    active: BTreeMap<Incarnation, (Incarnation, usize, bool)>,
    response_bytes: usize,
}

/// In-process reference policy for broker and worker admission transitions.
/// Transport integration must keep query admission local to workers, distributing
/// finite allowances over the control channel rather than making per-query RPCs.
/// Counters remain charged until completion or supervisor-confirmed process exit.
pub struct Broker {
    service: Incarnation,
    limits: BrokerLimits,
    alive: bool,
    evidence_paused: bool,
    workers: BTreeMap<ProjectBinding, Worker>,
    pending: BTreeMap<Sha256Digest, Pending>,
}

pub fn new_instance_id() -> Result<Incarnation> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::InvalidRequest)?;
    Ok(Incarnation::from_bytes(bytes))
}

fn random_id() -> Result<Incarnation> {
    new_instance_id()
}

impl Broker {
    /// Never opens an index or inspects query payloads. Expired running capacity stays visible.
    pub fn status(&self, now: Instant) -> BrokerStatus {
        let mut status = BrokerStatus {
            service: self.service,
            accepting: self.alive && !self.evidence_paused,
            projects: self.workers.len(),
            draining_projects: 0,
            pending_connections: self.pending.values().filter(|p| now < p.expires).count(),
            allocated: QueryAllowance {
                concurrent_queries: 0,
                response_bytes: 0,
            },
            active_queries: 0,
            reserved_response_bytes: 0,
        };
        for worker in self.workers.values() {
            status.draining_projects += usize::from(worker.draining || now >= worker.expires);
            status.allocated.concurrent_queries += worker.allowance.concurrent_queries;
            status.allocated.response_bytes += worker.allowance.response_bytes;
            status.active_queries += worker.active.len();
            status.reserved_response_bytes += worker.response_bytes;
        }
        status
    }

    pub fn new(service: Incarnation, limits: BrokerLimits) -> Result<Self> {
        if limits.projects == 0
            || limits.projects > 1024
            || limits.pending_grants == 0
            || limits.pending_grants > 4096
            || limits.connections_per_project == 0
            || limits.connections_per_project > 1024
            || limits.queries_per_connection == 0
            || limits.queries_per_connection > 64
            || limits.queries_per_caller == 0
            || limits.queries_per_caller > 65
            || limits.total.concurrent_queries == 0
            || limits.total.concurrent_queries > 4096
            || limits.total.response_bytes == 0
            || limits.total.response_bytes > 4096 * MAX_FRAME_BYTES
        {
            return Err(Error::InvalidRequest);
        }
        Ok(Self {
            service,
            limits,
            alive: true,
            evidence_paused: false,
            workers: BTreeMap::new(),
            pending: BTreeMap::new(),
        })
    }

    fn require_alive(&self) -> Result<()> {
        if self.alive {
            Ok(())
        } else {
            Err(Error::OwnershipLost)
        }
    }

    /// Preserve all accepted charges when lowering capacity; new work waits for them to drain.
    pub fn configure_queries(
        &mut self,
        identity: WorkerIdentity,
        concurrency: usize,
        now: Instant,
    ) -> Result<()> {
        airk_core::service_resources::validate_query_concurrency(concurrency)
            .map_err(|_| Error::InvalidRequest)?;
        self.worker(identity, now)?;
        let queries = concurrency + 1; // Independent status/control headroom.
        let allocated = self
            .workers
            .iter()
            .filter(|(p, _)| **p != identity.project)
            .map(|(_, w)| w.allowance.concurrent_queries)
            .sum::<usize>();
        if allocated + queries > self.limits.total.concurrent_queries
            || (allocated + queries) * MAX_FRAME_BYTES > self.limits.total.response_bytes
        {
            return Err(Error::Busy);
        }
        self.workers
            .get_mut(&identity.project)
            .ok_or(Error::UnknownWorker)?
            .allowance = QueryAllowance {
            concurrent_queries: queries,
            response_bytes: queries * MAX_FRAME_BYTES,
        };
        Ok(())
    }

    /// Call only after the supervisor proves OS ownership and classifies pending recovery.
    /// Replacing an identity requires a strictly newer fencing generation and drained work.
    pub fn register(
        &mut self,
        identity: WorkerIdentity,
        allowance: QueryAllowance,
        now: Instant,
        lifetime: Duration,
    ) -> Result<()> {
        self.require_alive()?;
        if identity.service != self.service || identity.fencing_generation == 0 {
            return Err(Error::StaleIdentity);
        }
        if allowance.concurrent_queries == 0
            || allowance.response_bytes == 0
            || lifetime.is_zero()
            || lifetime > Duration::from_secs(300)
        {
            return Err(Error::InvalidRequest);
        }
        let expires = now.checked_add(lifetime).ok_or(Error::InvalidRequest)?;
        if let Some(old) = self.workers.get(&identity.project) {
            if identity.fencing_generation <= old.identity.fencing_generation
                || identity.worker == old.identity.worker
            {
                return Err(Error::StaleIdentity);
            }
            if !old.draining || !old.active.is_empty() {
                return Err(Error::Busy);
            }
        } else if self.workers.len() >= self.limits.projects {
            return Err(Error::Busy);
        }
        let (queries, bytes) = self
            .workers
            .iter()
            .filter(|(project, _)| **project != identity.project)
            .fold((0usize, 0usize), |(q, b), (_, w)| {
                (
                    q.saturating_add(w.allowance.concurrent_queries),
                    b.saturating_add(w.allowance.response_bytes),
                )
            });
        if allowance.concurrent_queries
            > self.limits.total.concurrent_queries.saturating_sub(queries)
            || allowance.response_bytes > self.limits.total.response_bytes.saturating_sub(bytes)
        {
            return Err(Error::Busy);
        }
        self.pending
            .retain(|_, p| p.grant.identity.project != identity.project);
        self.workers.insert(
            identity.project,
            Worker {
                identity,
                allowance,
                expires,
                draining: false,
                connections: BTreeMap::new(),
                active: BTreeMap::new(),
                response_bytes: 0,
            },
        );
        Ok(())
    }

    fn worker(&self, identity: WorkerIdentity, now: Instant) -> Result<&Worker> {
        self.require_alive()?;
        let worker = self
            .workers
            .get(&identity.project)
            .ok_or(Error::UnknownWorker)?;
        if worker.identity != identity {
            return Err(Error::StaleIdentity);
        }
        if worker.draining {
            return Err(Error::Draining);
        }
        if now >= worker.expires {
            return Err(Error::Expired);
        }
        Ok(worker)
    }

    /// Extend an existing allowance only over the live supervisor channel. Renewal
    /// never revives an expired/draining owner or changes running-work charges.
    pub fn renew(
        &mut self,
        identity: WorkerIdentity,
        now: Instant,
        lifetime: Duration,
    ) -> Result<()> {
        if lifetime.is_zero() || lifetime > Duration::from_secs(300) {
            return Err(Error::InvalidRequest);
        }
        self.worker(identity, now)?;
        let expires = now.checked_add(lifetime).ok_or(Error::InvalidRequest)?;
        let worker = self
            .workers
            .get_mut(&identity.project)
            .ok_or(Error::UnknownWorker)?;
        worker.expires = worker.expires.max(expires);
        Ok(())
    }

    /// Issues access only; does not enqueue indexing or allocate additional execution capacity.
    pub fn issue(
        &mut self,
        identity: WorkerIdentity,
        caller: Incarnation,
        now: Instant,
        ttl: Duration,
    ) -> Result<ConnectionGrant> {
        if self.evidence_paused {
            return Err(Error::Busy);
        }
        self.worker(identity, now)?;
        if ttl.is_zero() || ttl > Duration::from_millis(MAX_GRANT_TTL_MS) || ttl.as_millis() == 0 {
            return Err(Error::InvalidRequest);
        }
        self.pending.retain(|_, p| now < p.expires);
        if self.pending.len() >= self.limits.pending_grants {
            return Err(Error::Busy);
        }
        let mut token = [0; 32];
        getrandom::fill(&mut token).map_err(|_| Error::InvalidRequest)?;
        let token = Sha256Digest::from_bytes(token);
        if self.pending.contains_key(&token) {
            return Err(Error::Busy);
        }
        let grant = ConnectionGrant {
            scope: None,
            protocol: PROTOCOL_VERSION,
            identity,
            caller,
            token,
            ttl_ms: ttl.as_millis() as u64,
            client: airk_core::usage::ClientFamily::Unknown,
        };
        self.pending.insert(
            token,
            Pending {
                grant: grant.clone(),
                expires: now.checked_add(ttl).ok_or(Error::InvalidRequest)?,
                purpose: SessionPurpose::Evidence,
            },
        );
        Ok(grant)
    }

    /// Private native-control adapter only. The runtime consumes this grant and
    /// returns a NativeRead value; it must never export the credential publicly.
    pub fn issue_native_admin(
        &mut self,
        identity: WorkerIdentity,
        caller: Incarnation,
        now: Instant,
        ttl: Duration,
    ) -> Result<ConnectionGrant> {
        let grant = self.issue(identity, caller, now, ttl)?;
        self.pending
            .get_mut(&grant.token)
            .ok_or(Error::InvalidGrant)?
            .purpose = SessionPurpose::NativeAdministrativeRead;
        Ok(grant)
    }

    /// Adapter supplies the authenticated caller and actual worker identity, not wire assertions.
    pub fn redeem(
        &mut self,
        grant: &ConnectionGrant,
        actual_worker: WorkerIdentity,
        actual_caller: Incarnation,
        now: Instant,
        authorization_lifetime: Duration,
    ) -> Result<Incarnation> {
        self.redeem_with_purpose(
            grant,
            actual_worker,
            actual_caller,
            now,
            authorization_lifetime,
        )
        .map(|(session, _)| session)
    }
    /// Return only the purpose sealed in the original private issuance, never a
    /// caller-supplied role flag. Adapters still authorize each permitted operation.
    pub fn redeem_with_purpose(
        &mut self,
        grant: &ConnectionGrant,
        actual_worker: WorkerIdentity,
        actual_caller: Incarnation,
        now: Instant,
        authorization_lifetime: Duration,
    ) -> Result<(Incarnation, SessionPurpose)> {
        self.worker(actual_worker, now)?;
        if authorization_lifetime.is_zero() || authorization_lifetime > Duration::from_secs(300) {
            return Err(Error::InvalidRequest);
        }
        let pending = self.pending.get(&grant.token).ok_or(Error::InvalidGrant)?;
        if now >= pending.expires {
            self.pending.remove(&grant.token);
            return Err(Error::Expired);
        }
        if grant.protocol != PROTOCOL_VERSION
            || grant.identity != actual_worker
            || grant.caller != actual_caller
            || pending.grant.identity != actual_worker
            || pending.grant.caller != actual_caller
            || pending.grant.ttl_ms != grant.ttl_ms
        {
            return Err(Error::InvalidGrant);
        }
        let purpose = pending.purpose;
        if purpose == SessionPurpose::NativeAdministrativeRead && grant.scope.is_some() {
            return Err(Error::InvalidGrant);
        }
        let worker = self
            .workers
            .get_mut(&actual_worker.project)
            .ok_or(Error::UnknownWorker)?;
        worker
            .connections
            .retain(|_, c| now < c.expires || c.active > 0);
        if worker.connections.len() >= self.limits.connections_per_project {
            return Err(Error::Busy);
        }
        let session = random_id()?;
        if worker.connections.contains_key(&session) {
            return Err(Error::Busy);
        }
        let expires = now
            .checked_add(authorization_lifetime)
            .ok_or(Error::InvalidRequest)?;
        worker.connections.insert(
            session,
            Connection {
                caller: actual_caller,
                expires,
                active: 0,
                purpose,
            },
        );
        self.pending.remove(&grant.token);
        Ok((session, purpose))
    }

    pub fn session_purpose(
        &self,
        identity: WorkerIdentity,
        session: Incarnation,
    ) -> Result<SessionPurpose> {
        let worker = self
            .workers
            .get(&identity.project)
            .ok_or(Error::UnknownWorker)?;
        if worker.identity != identity {
            return Err(Error::StaleIdentity);
        }
        Ok(worker
            .connections
            .get(&session)
            .ok_or(Error::InvalidGrant)?
            .purpose)
    }

    /// Local worker admission policy: no query text, vectors or evidence cross this interface.
    pub fn begin_query(
        &mut self,
        identity: WorkerIdentity,
        session: Incarnation,
        response_bytes: usize,
        now: Instant,
    ) -> Result<Incarnation> {
        self.begin_query_with_control(identity, session, response_bytes, now, false)
    }
    pub fn begin_query_with_control(
        &mut self,
        identity: WorkerIdentity,
        session: Incarnation,
        response_bytes: usize,
        now: Instant,
        control: bool,
    ) -> Result<Incarnation> {
        if self.evidence_paused {
            return Err(Error::Busy);
        }
        let worker = self.worker(identity, now)?;
        let connection = worker
            .connections
            .get(&session)
            .ok_or(Error::InvalidGrant)?;
        if control && connection.purpose == SessionPurpose::NativeAdministrativeRead {
            return Err(Error::InvalidRequest);
        }
        if now >= connection.expires {
            return Err(Error::Expired);
        }
        if response_bytes == 0 || response_bytes > MAX_FRAME_BYTES {
            return Err(Error::InvalidRequest);
        }
        if (!control
            && self.limits.reserved_status
            && worker
                .active
                .values()
                .filter(|(_, _, control)| !control)
                .count()
                >= worker.allowance.concurrent_queries.saturating_sub(1))
            || worker.active.len() >= worker.allowance.concurrent_queries
            || connection.active >= self.limits.queries_per_connection
            || worker
                .connections
                .values()
                .filter(|c| c.caller == connection.caller)
                .map(|c| c.active)
                .sum::<usize>()
                >= self.limits.queries_per_caller
            || response_bytes
                > worker
                    .allowance
                    .response_bytes
                    .saturating_sub(worker.response_bytes)
        {
            return Err(Error::Busy);
        }
        let request = random_id()?;
        let worker = self
            .workers
            .get_mut(&identity.project)
            .ok_or(Error::UnknownWorker)?;
        if worker.active.contains_key(&request) {
            return Err(Error::Busy);
        }
        worker
            .connections
            .get_mut(&session)
            .ok_or(Error::InvalidGrant)?
            .active += 1;
        worker
            .active
            .insert(request, (session, response_bytes, control));
        worker.response_bytes += response_bytes;
        Ok(request)
    }

    /// Complete only after execution/output buffers and any revision lease are released.
    /// Remains available after expiry, drain or service loss so capacity is not leaked.
    pub fn finish_query(&mut self, identity: WorkerIdentity, request: Incarnation) -> Result<()> {
        let worker = self
            .workers
            .get_mut(&identity.project)
            .ok_or(Error::UnknownWorker)?;
        if worker.identity != identity {
            return Err(Error::StaleIdentity);
        }
        let (session, bytes, _) = worker
            .active
            .remove(&request)
            .ok_or(Error::UnknownRequest)?;
        worker.response_bytes -= bytes;
        worker
            .connections
            .get_mut(&session)
            .ok_or(Error::InvalidGrant)?
            .active -= 1;
        Ok(())
    }

    /// Disconnect cancels admission but never releases a still-running request's capacity.
    pub fn disconnect(
        &mut self,
        identity: WorkerIdentity,
        session: Incarnation,
        now: Instant,
    ) -> Result<()> {
        let worker = self
            .workers
            .get_mut(&identity.project)
            .ok_or(Error::UnknownWorker)?;
        if worker.identity != identity {
            return Err(Error::StaleIdentity);
        }
        let connection = worker
            .connections
            .get_mut(&session)
            .ok_or(Error::InvalidGrant)?;
        connection.expires = now;
        Ok(())
    }

    pub fn drain(&mut self, identity: WorkerIdentity) -> Result<()> {
        let worker = self
            .workers
            .get_mut(&identity.project)
            .ok_or(Error::UnknownWorker)?;
        if worker.identity != identity {
            return Err(Error::StaleIdentity);
        }
        worker.draining = true;
        self.pending.retain(|_, p| p.grant.identity != identity);
        Ok(())
    }

    /// After process exit is proven by the supervisor, release outstanding memory/execution.
    /// Keep the identity tombstone so an old fencing generation cannot be reintroduced.
    pub fn process_exited(&mut self, identity: WorkerIdentity) -> Result<()> {
        self.drain(identity)?;
        let worker = self
            .workers
            .get_mut(&identity.project)
            .ok_or(Error::UnknownWorker)?;
        worker.active.clear();
        worker.connections.clear();
        worker.response_bytes = 0;
        worker.allowance = QueryAllowance {
            concurrent_queries: 0,
            response_bytes: 0,
        };
        Ok(())
    }

    /// Quiesce evidence without revoking accepted indexing/model continuations
    /// or worker renewal. This flag is intentionally irreversible for an owner.
    pub fn pause_evidence(&mut self) {
        self.evidence_paused = true;
        self.pending.clear();
    }

    pub fn ownership_lost(&mut self) {
        self.alive = false;
        self.pending.clear();
        for worker in self.workers.values_mut() {
            worker.draining = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_admin_purpose_is_sealed_in_issuance_and_not_a_wire_role() {
        let (mut broker, id, now) = setup();
        let caller = Incarnation::from_bytes([7; 16]);
        let ordinary = broker
            .issue(id, caller, now, Duration::from_secs(5))
            .unwrap();
        let (session, purpose) = broker
            .redeem_with_purpose(&ordinary, id, caller, now, Duration::from_secs(30))
            .unwrap();
        assert_eq!(purpose, SessionPurpose::Evidence);
        assert_eq!(
            broker.session_purpose(id, session).unwrap(),
            SessionPurpose::Evidence
        );
        let admin = broker
            .issue_native_admin(id, caller, now, Duration::from_secs(5))
            .unwrap();
        let wire = serde_json::to_value(&admin).unwrap();
        assert!(wire.get("purpose").is_none());
        let (session, purpose) = broker
            .redeem_with_purpose(&admin, id, caller, now, Duration::from_secs(30))
            .unwrap();
        assert_eq!(purpose, SessionPurpose::NativeAdministrativeRead);
        assert_eq!(broker.session_purpose(id, session).unwrap(), purpose);
        assert_eq!(
            broker.redeem_with_purpose(&admin, id, caller, now, Duration::from_secs(30)),
            Err(Error::InvalidGrant)
        );
    }

    #[test]
    fn administrative_reads_share_all_sixteen_slots_and_cannot_steal_status_or_release_on_disconnect()
     {
        let id = identity(1, 1);
        let now = Instant::now();
        let caller = Incarnation::from_bytes([7; 16]);
        let mut broker = Broker::new(id.service, BrokerLimits::service()).unwrap();
        broker
            .register(
                id,
                QueryAllowance {
                    concurrent_queries: 17,
                    response_bytes: 17 * MAX_FRAME_BYTES,
                },
                now,
                Duration::from_secs(60),
            )
            .unwrap();
        let mut running = Vec::new();
        for _ in 0..16 {
            let grant = broker
                .issue_native_admin(id, caller, now, Duration::from_secs(5))
                .unwrap();
            let (session, purpose) = broker
                .redeem_with_purpose(&grant, id, caller, now, Duration::from_secs(30))
                .unwrap();
            assert_eq!(purpose, SessionPurpose::NativeAdministrativeRead);
            let request = broker.begin_query(id, session, 1024, now).unwrap();
            running.push((session, request));
        }
        let grant = broker
            .issue_native_admin(id, caller, now, Duration::from_secs(5))
            .unwrap();
        let (waiting, _) = broker
            .redeem_with_purpose(&grant, id, caller, now, Duration::from_secs(30))
            .unwrap();
        assert_eq!(broker.begin_query(id, waiting, 1024, now), Err(Error::Busy));
        assert_eq!(
            broker.begin_query_with_control(id, waiting, 1024, now, true),
            Err(Error::InvalidRequest)
        );
        let evidence = connect(&mut broker, id, now);
        let status = broker
            .begin_query_with_control(id, evidence, 1024, now, true)
            .unwrap();
        assert_eq!(broker.status(now).active_queries, 17);
        broker.disconnect(id, running[0].0, now).unwrap();
        assert_eq!(
            broker.status(now).active_queries,
            17,
            "a dropped parent/peer does not finish native work"
        );
        assert_eq!(broker.begin_query(id, waiting, 1024, now), Err(Error::Busy));
        broker.finish_query(id, running.remove(0).1).unwrap();
        let accepted = broker.begin_query(id, waiting, 1024, now).unwrap();
        for (_, request) in running {
            broker.finish_query(id, request).unwrap();
        }
        broker.finish_query(id, status).unwrap();
        broker.finish_query(id, accepted).unwrap();
        assert_eq!(broker.status(now).active_queries, 0);
    }

    fn identity(project: u8, generation: u64) -> WorkerIdentity {
        WorkerIdentity {
            service: Incarnation::from_bytes([1; 16]),
            worker: Incarnation::from_bytes([generation as u8; 16]),
            project: Sha256Digest::from_bytes([project; 32]),
            fencing_generation: generation,
        }
    }

    fn setup() -> (Broker, WorkerIdentity, Instant) {
        let id = identity(1, 1);
        let mut broker = Broker::new(id.service, BrokerLimits::default()).unwrap();
        let now = Instant::now();
        broker
            .register(
                id,
                QueryAllowance {
                    concurrent_queries: 2,
                    response_bytes: 1024,
                },
                now,
                Duration::from_secs(60),
            )
            .unwrap();
        (broker, id, now)
    }

    fn connect(broker: &mut Broker, id: WorkerIdentity, now: Instant) -> Incarnation {
        let caller = Incarnation::from_bytes([7; 16]);
        let grant = broker
            .issue(id, caller, now, Duration::from_secs(5))
            .unwrap();
        broker
            .redeem(&grant, id, caller, now, Duration::from_secs(30))
            .unwrap()
    }

    #[test]
    fn service_capacity_matches_channels_and_lowering_keeps_accepted_charges() {
        for concurrency in [4, 8, 16, 32, 64] {
            let id = identity(1, 1);
            let now = Instant::now();
            let mut broker = Broker::new(id.service, BrokerLimits::service()).unwrap();
            broker
                .register(
                    id,
                    QueryAllowance {
                        concurrent_queries: concurrency + 1,
                        response_bytes: (concurrency + 1) * MAX_FRAME_BYTES,
                    },
                    now,
                    Duration::from_secs(60),
                )
                .unwrap();
            let sessions: Vec<_> = (0..concurrency + 2)
                .map(|_| connect(&mut broker, id, now))
                .collect();
            let mut active: Vec<_> = sessions
                .iter()
                .take(concurrency + 1)
                .enumerate()
                .map(|(index, session)| {
                    broker
                        .begin_query_with_control(
                            id,
                            *session,
                            MAX_FRAME_BYTES,
                            now,
                            index == concurrency,
                        )
                        .unwrap()
                })
                .collect();
            assert_eq!(broker.status(now).active_queries, concurrency + 1);
            assert_eq!(
                broker.begin_query(id, sessions[concurrency + 1], MAX_FRAME_BYTES, now),
                Err(Error::Busy)
            );
            broker.configure_queries(id, 4, now).unwrap();
            assert_eq!(broker.status(now).active_queries, concurrency + 1);
            assert_eq!(
                broker.status(now).reserved_response_bytes,
                (concurrency + 1) * MAX_FRAME_BYTES
            );
            while active.len() >= 5 {
                broker.finish_query(id, active.remove(0)).unwrap();
            }
            let request = broker
                .begin_query(id, sessions[concurrency + 1], MAX_FRAME_BYTES, now)
                .unwrap();
            broker.finish_query(id, request).unwrap();
            for request in active {
                broker.finish_query(id, request).unwrap();
            }
            broker.configure_queries(id, 64, now).unwrap();
            assert_eq!(broker.status(now).allocated.concurrent_queries, 65);
        }
    }

    #[test]
    fn renewal_preserves_running_charges_and_cannot_revive_expired_or_draining_owners() {
        let (mut broker, id, now) = setup();
        let session = connect(&mut broker, id, now);
        let request = broker.begin_query(id, session, 768, now).unwrap();
        let later = now + Duration::from_secs(50);
        broker.renew(id, later, Duration::from_secs(120)).unwrap();
        let observation = broker.status(now + Duration::from_secs(61));
        assert_eq!(
            (
                observation.active_queries,
                observation.reserved_response_bytes,
                observation.draining_projects
            ),
            (1, 768, 0)
        );
        broker.finish_query(id, request).unwrap();
        broker.drain(id).unwrap();
        assert_eq!(
            broker.renew(id, later, Duration::from_secs(120)),
            Err(Error::Draining)
        );
        let (mut broker, id, now) = setup();
        assert_eq!(
            broker.renew(id, now + Duration::from_secs(61), Duration::from_secs(120)),
            Err(Error::Expired)
        );
    }

    #[test]
    fn grant_is_single_use_and_bound_to_peer_worker_and_protocol() {
        let (mut broker, id, now) = setup();
        let caller = Incarnation::from_bytes([7; 16]);
        let grant = broker
            .issue(id, caller, now, Duration::from_secs(5))
            .unwrap();
        assert!(!format!("{grant:?}").contains(&grant.token.to_string()));
        assert_eq!(
            broker.redeem(&grant, id, id.service, now, Duration::from_secs(1)),
            Err(Error::InvalidGrant)
        );
        let mut forged = grant.clone();
        forged.protocol += 1;
        assert_eq!(
            broker.redeem(&forged, id, caller, now, Duration::from_secs(1)),
            Err(Error::InvalidGrant)
        );
        let session = broker
            .redeem(&grant, id, caller, now, Duration::from_secs(1))
            .unwrap();
        assert_eq!(
            broker.redeem(&grant, id, caller, now, Duration::from_secs(1)),
            Err(Error::InvalidGrant)
        );
        assert_eq!(
            broker.begin_query(identity(2, 1), session, 512, now),
            Err(Error::UnknownWorker)
        );
    }

    #[test]
    fn response_and_concurrency_limits_span_all_connections() {
        let (mut broker, id, now) = setup();
        let first = connect(&mut broker, id, now);
        let second = connect(&mut broker, id, now);
        let query = broker.begin_query(id, first, 768, now).unwrap();
        assert_eq!(broker.begin_query(id, second, 512, now), Err(Error::Busy));
        let another = broker.begin_query(id, second, 256, now).unwrap();
        assert_eq!(broker.begin_query(id, second, 1, now), Err(Error::Busy));
        broker.finish_query(id, query).unwrap();
        assert_eq!(broker.finish_query(id, query), Err(Error::UnknownRequest));
        broker.finish_query(id, another).unwrap();
        assert!(broker.begin_query(id, second, 1024, now).is_ok());
    }

    #[test]
    fn opening_connections_cannot_bypass_authenticated_caller_limit() {
        let (mut broker, id, now) = setup();
        broker.limits.queries_per_caller = 1;
        let first = connect(&mut broker, id, now);
        let second = connect(&mut broker, id, now);
        let request = broker.begin_query(id, first, 128, now).unwrap();
        assert_eq!(broker.begin_query(id, second, 128, now), Err(Error::Busy));
        // Disconnect still charges the caller while its execution drains.
        broker.disconnect(id, first, now).unwrap();
        assert_eq!(broker.begin_query(id, second, 128, now), Err(Error::Busy));
        let other_caller = Incarnation::from_bytes([8; 16]);
        let grant = broker
            .issue(id, other_caller, now, Duration::from_secs(1))
            .unwrap();
        let third = broker
            .redeem(&grant, id, other_caller, now, Duration::from_secs(1))
            .unwrap();
        let other = broker.begin_query(id, third, 128, now).unwrap();
        broker.finish_query(id, request).unwrap();
        assert!(broker.begin_query(id, second, 128, now).is_ok());
        broker.finish_query(id, other).unwrap();
    }

    #[test]
    fn expiry_and_disconnect_do_not_free_running_capacity() {
        let (mut broker, id, now) = setup();
        let session = connect(&mut broker, id, now);
        let request = broker.begin_query(id, session, 1024, now).unwrap();
        broker.disconnect(id, session, now).unwrap();
        assert_eq!(broker.begin_query(id, session, 1, now), Err(Error::Expired));
        let later = now + Duration::from_secs(61);
        assert_eq!(
            broker.begin_query(id, session, 1, later),
            Err(Error::Expired)
        );
        broker.drain(id).unwrap();
        assert_eq!(
            broker.register(
                identity(1, 2),
                QueryAllowance {
                    concurrent_queries: 1,
                    response_bytes: 1024
                },
                later,
                Duration::from_secs(60)
            ),
            Err(Error::Busy)
        );
        broker.finish_query(id, request).unwrap();
        broker
            .register(
                identity(1, 2),
                QueryAllowance {
                    concurrent_queries: 1,
                    response_bytes: 1024,
                },
                later,
                Duration::from_secs(60),
            )
            .unwrap();
        assert_eq!(
            broker.begin_query(id, session, 1, later),
            Err(Error::StaleIdentity)
        );
    }

    #[test]
    fn service_loss_stops_admission_but_allows_completion() {
        let (mut broker, id, now) = setup();
        let session = connect(&mut broker, id, now);
        let request = broker.begin_query(id, session, 512, now).unwrap();
        broker.ownership_lost();
        let status = broker.status(now);
        assert!(!status.accepting);
        assert_eq!(status.active_queries, 1);
        assert_eq!(status.reserved_response_bytes, 512);
        assert_eq!(
            broker.begin_query(id, session, 512, now),
            Err(Error::OwnershipLost)
        );
        broker.finish_query(id, request).unwrap();
        assert_eq!(broker.status(now).active_queries, 0);
        assert_eq!(
            broker
                .issue(id, session, now, Duration::from_secs(1))
                .unwrap_err(),
            Error::OwnershipLost
        );
    }

    #[test]
    fn expired_allowance_stays_charged_until_exit_and_epochs_cannot_repeat() {
        let (mut broker, id, now) = setup();
        broker.limits.total.concurrent_queries = 2;
        let later = now + Duration::from_secs(61);
        let other = identity(2, 1);
        let allowance = QueryAllowance {
            concurrent_queries: 1,
            response_bytes: 512,
        };
        assert_eq!(
            broker.register(other, allowance, later, Duration::from_secs(60)),
            Err(Error::Busy)
        );
        broker.process_exited(id).unwrap();
        broker
            .register(other, allowance, later, Duration::from_secs(60))
            .unwrap();
        assert_eq!(
            broker.register(id, allowance, later, Duration::from_secs(60)),
            Err(Error::StaleIdentity)
        );
    }

    #[test]
    fn pending_credentials_are_bounded_and_expire() {
        let (mut broker, id, now) = setup();
        broker.limits.pending_grants = 1;
        let grant = broker
            .issue(id, id.service, now, Duration::from_secs(1))
            .unwrap();
        assert_eq!(
            broker
                .issue(id, id.service, now, Duration::from_secs(1))
                .unwrap_err(),
            Error::Busy
        );
        let later = now + Duration::from_secs(1);
        assert_eq!(
            broker.redeem(&grant, id, id.service, later, Duration::from_secs(1)),
            Err(Error::Expired)
        );
        assert!(
            broker
                .issue(id, id.service, later, Duration::from_secs(1))
                .is_ok()
        );
    }
}
