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

struct Pending {
    grant: ConnectionGrant,
    expires: Instant,
}

struct Connection {
    caller: Incarnation,
    expires: Instant,
    active: usize,
}

struct Worker {
    identity: WorkerIdentity,
    allowance: QueryAllowance,
    expires: Instant,
    draining: bool,
    connections: BTreeMap<Incarnation, Connection>,
    active: BTreeMap<Incarnation, (Incarnation, usize)>,
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
    workers: BTreeMap<ProjectBinding, Worker>,
    pending: BTreeMap<Sha256Digest, Pending>,
}

fn random_id() -> Result<Incarnation> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| Error::InvalidRequest)?;
    Ok(Incarnation::from_bytes(bytes))
}

impl Broker {
    /// Never opens an index or inspects query payloads. Expired running capacity stays visible.
    pub fn status(&self, now: Instant) -> BrokerStatus {
        let mut status = BrokerStatus {
            service: self.service,
            accepting: self.alive,
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
            || limits.queries_per_caller > 64
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

    /// Issues access only; does not enqueue indexing or allocate additional execution capacity.
    pub fn issue(
        &mut self,
        identity: WorkerIdentity,
        caller: Incarnation,
        now: Instant,
        ttl: Duration,
    ) -> Result<ConnectionGrant> {
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
            protocol: PROTOCOL_VERSION,
            identity,
            caller,
            token,
            ttl_ms: ttl.as_millis() as u64,
        };
        self.pending.insert(
            token,
            Pending {
                grant: grant.clone(),
                expires: now.checked_add(ttl).ok_or(Error::InvalidRequest)?,
            },
        );
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
            },
        );
        self.pending.remove(&grant.token);
        Ok(session)
    }

    /// Local worker admission policy: no query text, vectors or evidence cross this interface.
    pub fn begin_query(
        &mut self,
        identity: WorkerIdentity,
        session: Incarnation,
        response_bytes: usize,
        now: Instant,
    ) -> Result<Incarnation> {
        let worker = self.worker(identity, now)?;
        let connection = worker
            .connections
            .get(&session)
            .ok_or(Error::InvalidGrant)?;
        if now >= connection.expires {
            return Err(Error::Expired);
        }
        if response_bytes == 0 || response_bytes > MAX_FRAME_BYTES {
            return Err(Error::InvalidRequest);
        }
        if worker.active.len() >= worker.allowance.concurrent_queries
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
        worker.active.insert(request, (session, response_bytes));
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
        let (session, bytes) = worker
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
