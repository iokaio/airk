// SPDX-License-Identifier: Apache-2.0
//! U105/U106(d) open agent selections, in memory. An open agent read is
//! admitted under an agent-owned selection nonce registered live with the
//! scoped-read admission gate. Nonces are leased per (connection, resolved
//! scope identity, catalog revision) and dropped on idle (300 s), a
//! connection's defaults change or revocation, and a catalog revision move.
//! Every drop returns its nonces as [`Dropped`] for the caller to revoke in
//! the gate after releasing the plane's lock (selection authority §3.3,
//! §3.7). Time, nonces and the gate registration are injected. The service's
//! open-read path (`open_reads`) leases; the dispatch drops on defaults
//! change and revocation, a periodic sweep drops idle leases and service
//! end closes the plane.
use airk_catalog_control::catalog_admission::MAX_AGENT_SELECTIONS;
use airk_core::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{CatalogRevision, IndexId, MAX_SELECTION},
    scoped_reads::{SelectionOwnerKind, SelectionScopeRef},
};
use std::collections::BTreeMap;

/// An unused lease is dropped after this long (contracts: idle 300 s).
pub(super) const IDLE_MS: u64 = 300_000;
/// One connection's concurrent resolved scopes; the least recently used is
/// dropped for a new one, so one agent cannot hold the whole budget.
pub(super) const MAX_LEASES_PER_CONNECTION: usize = 4;

pub(super) type Clock = Box<dyn Fn() -> u64 + Send>;
pub(super) type NonceSource = Box<dyn FnMut() -> Incarnation + Send>;
/// Registers a lease's nonce live with the gate, nested inside the plane's
/// lock (owner, then gate; the gate never calls back into the plane).
pub(super) type Registrar = Box<dyn Fn(&SelectionScopeRef) -> Result<(), ()> + Send>;

/// The selection nonces one plane call dropped; revoke each in the gate
/// (`NativeCatalogAdmission::revoke_selection`) with no plane lock held.
#[must_use = "revoke every dropped agent nonce with no lock held"]
#[derive(Default, PartialEq, Eq)]
pub(super) struct Dropped(Vec<Incarnation>);
impl Dropped {
    pub(super) fn into_nonces(self) -> Vec<Incarnation> {
        self.0
    }
    fn absorb(&mut self, other: Dropped) {
        self.0.extend(other.0);
    }
}
impl std::fmt::Debug for Dropped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Dropped({} nonces)", self.0.len())
    }
}

/// Why a lease was not granted. None of these reveal a nonce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LeaseRefused {
    /// The agent budget is full of live leases of other connections.
    Capacity,
    /// The requested revision is older than a live lease's: the caller
    /// resolved against a superseded catalog and must resolve again.
    Stale,
    /// The gate refused the registration.
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LeaseKey {
    connection: Incarnation,
    scope_identity: Sha256Digest,
    catalog_revision: CatalogRevision,
}
struct Lease {
    selection_ref: SelectionScopeRef,
    last_used_ms: u64,
}

/// The identity of an open read's resolved scope: its namespace, catalog
/// revision and the sorted, deduplicated index set it reads whole.
pub(super) fn open_scope_identity(
    namespace: Incarnation,
    catalog_revision: CatalogRevision,
    index_ids: &[IndexId],
) -> anyhow::Result<Sha256Digest> {
    let mut indexes = index_ids.to_vec();
    indexes.sort();
    indexes.dedup();
    anyhow::ensure!(
        !indexes.is_empty() && indexes.len() <= MAX_SELECTION,
        "open agent scope must name 1 to 32 indexes"
    );
    Ok(framed_digest(
        b"airk/open-agent-scope/1",
        &[
            namespace.as_bytes(),
            &catalog_revision.get().to_be_bytes(),
            &serde_json::to_vec(&indexes)?,
        ],
    ))
}

pub(super) struct AgentPlane {
    instance: Incarnation,
    clock: Clock,
    nonce: NonceSource,
    registrar: Registrar,
    leases: BTreeMap<LeaseKey, Lease>,
}

impl AgentPlane {
    pub(super) fn new(
        instance: Incarnation,
        clock: Clock,
        nonce: NonceSource,
        registrar: Registrar,
    ) -> Self {
        Self {
            instance,
            clock,
            nonce,
            registrar,
            leases: BTreeMap::new(),
        }
    }

    /// The live lease of this connection's resolved scope at this revision,
    /// or a new one. Idle leases and leases of older revisions are dropped
    /// first; the returned [`Dropped`] must be revoked even on refusal.
    pub(super) fn lease(
        &mut self,
        connection: Incarnation,
        scope_identity: Sha256Digest,
        catalog_revision: CatalogRevision,
    ) -> (Result<SelectionScopeRef, LeaseRefused>, Dropped) {
        let now = (self.clock)();
        let mut dropped = self.sweep_at(now);
        if self
            .leases
            .keys()
            .any(|key| key.catalog_revision > catalog_revision)
        {
            return (Err(LeaseRefused::Stale), dropped);
        }
        dropped.absorb(self.revision_moved(catalog_revision));
        let key = LeaseKey {
            connection,
            scope_identity,
            catalog_revision,
        };
        if let Some(lease) = self.leases.get_mut(&key) {
            lease.last_used_ms = now;
            return (Ok(lease.selection_ref), dropped);
        }
        let mine = self
            .leases
            .iter()
            .filter(|(key, _)| key.connection == connection)
            .min_by_key(|(key, lease)| (lease.last_used_ms, **key))
            .map(|(key, _)| *key);
        let count = self
            .leases
            .keys()
            .filter(|key| key.connection == connection)
            .count();
        if count >= MAX_LEASES_PER_CONNECTION
            && let Some(oldest) = mine
            && let Some(lease) = self.leases.remove(&oldest)
        {
            dropped.0.push(lease.selection_ref.grant_nonce);
        }
        if self.leases.len() >= MAX_AGENT_SELECTIONS {
            return (Err(LeaseRefused::Capacity), dropped);
        }
        let selection_ref = SelectionScopeRef {
            owner: SelectionOwnerKind::Agent,
            grant_nonce: (self.nonce)(),
            service_instance: self.instance,
            scope_identity,
            catalog_revision,
        };
        if (self.registrar)(&selection_ref).is_err() {
            return (Err(LeaseRefused::Unavailable), dropped);
        }
        self.leases.insert(
            key,
            Lease {
                selection_ref,
                last_used_ms: now,
            },
        );
        (Ok(selection_ref), dropped)
    }

    /// Drop every lease unused for [`IDLE_MS`].
    pub(super) fn sweep(&mut self) -> Dropped {
        let now = (self.clock)();
        self.sweep_at(now)
    }
    fn sweep_at(&mut self, now: u64) -> Dropped {
        self.drop_where(|_, lease| now.saturating_sub(lease.last_used_ms) >= IDLE_MS)
    }

    /// The catalog moved to `current`: leases of any other revision can no
    /// longer be admitted (admission is exact-revision) and are dropped.
    pub(super) fn revision_moved(&mut self, current: CatalogRevision) -> Dropped {
        self.drop_where(|key, _| key.catalog_revision != current)
    }

    /// A connection's default scope or agent choice changed, or the
    /// connection was revoked: every lease of it is dropped.
    pub(super) fn revoke_connection(&mut self, connection: Incarnation) -> Dropped {
        self.drop_where(|key, _| key.connection == connection)
    }

    /// Service end: every lease.
    pub(super) fn close(&mut self) -> Dropped {
        self.drop_where(|_, _| true)
    }

    fn drop_where(&mut self, mut drop: impl FnMut(&LeaseKey, &Lease) -> bool) -> Dropped {
        let mut dropped = Dropped::default();
        self.leases.retain(|key, lease| {
            let remove = drop(key, lease);
            if remove {
                dropped.0.push(lease.selection_ref.grant_nonce);
            }
            !remove
        });
        dropped
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.leases.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    };

    struct Harness {
        plane: AgentPlane,
        now: Arc<AtomicU64>,
        registered: Arc<Mutex<Vec<SelectionScopeRef>>>,
    }
    fn harness(refuse: bool) -> Harness {
        let now = Arc::new(AtomicU64::new(1_000));
        let registered = Arc::new(Mutex::new(Vec::new()));
        let clock = Arc::clone(&now);
        let seen = Arc::clone(&registered);
        let mut next = 0u128;
        let plane = AgentPlane::new(
            Incarnation::from_bytes([0xe1; 16]),
            Box::new(move || clock.load(Ordering::SeqCst)),
            Box::new(move || {
                next += 1;
                Incarnation::from_bytes(next.to_be_bytes())
            }),
            Box::new(move |selection| {
                if refuse {
                    return Err(());
                }
                seen.lock().unwrap().push(*selection);
                Ok(())
            }),
        );
        Harness {
            plane,
            now,
            registered,
        }
    }
    fn connection(byte: u8) -> Incarnation {
        Incarnation::from_bytes([byte; 16])
    }
    fn scope(byte: u8) -> Sha256Digest {
        Sha256Digest::from_bytes([byte; 32])
    }
    fn revision(value: u64) -> CatalogRevision {
        value.try_into().unwrap()
    }

    #[test]
    fn a_lease_is_reused_for_the_same_connection_scope_and_revision() {
        let mut h = harness(false);
        let (first, dropped) = h.plane.lease(connection(1), scope(1), revision(3));
        let first = first.unwrap();
        assert_eq!(dropped, Dropped::default());
        assert_eq!(first.owner, SelectionOwnerKind::Agent);
        assert_eq!(first.scope_identity, scope(1));
        assert_eq!(first.catalog_revision, revision(3));
        let (again, _) = h.plane.lease(connection(1), scope(1), revision(3));
        assert_eq!(again.unwrap(), first);
        // Another connection with the same scope has its own nonce.
        let (other, _) = h.plane.lease(connection(2), scope(1), revision(3));
        assert_ne!(other.unwrap().grant_nonce, first.grant_nonce);
        assert_eq!(h.registered.lock().unwrap().len(), 2);
    }

    #[test]
    fn idle_defaults_revocation_and_revision_moves_drop_nonces() {
        let mut h = harness(false);
        let a = h
            .plane
            .lease(connection(1), scope(1), revision(3))
            .0
            .unwrap();
        let b = h
            .plane
            .lease(connection(2), scope(2), revision(3))
            .0
            .unwrap();
        // Idle: a used again at +299 s survives; b, unused for 300 s, drops.
        h.now.fetch_add(IDLE_MS - 1, Ordering::SeqCst);
        let (renewed, dropped) = h.plane.lease(connection(1), scope(1), revision(3));
        assert_eq!(renewed.unwrap(), a);
        assert_eq!(dropped, Dropped::default());
        h.now.fetch_add(1, Ordering::SeqCst);
        assert_eq!(h.plane.sweep().into_nonces(), vec![b.grant_nonce]);
        // Defaults change or connection revocation: that connection only.
        let c = h
            .plane
            .lease(connection(2), scope(2), revision(3))
            .0
            .unwrap();
        assert_eq!(
            h.plane.revoke_connection(connection(1)).into_nonces(),
            vec![a.grant_nonce]
        );
        assert_eq!(h.plane.len(), 1);
        // Revision move: a lease at the new revision drops the old one, and
        // an older revision is refused as stale afterwards.
        let (moved, dropped) = h.plane.lease(connection(2), scope(2), revision(4));
        assert_ne!(moved.unwrap().grant_nonce, c.grant_nonce);
        assert_eq!(dropped.into_nonces(), vec![c.grant_nonce]);
        let (stale, dropped) = h.plane.lease(connection(1), scope(1), revision(3));
        assert_eq!(stale, Err(LeaseRefused::Stale));
        assert_eq!(dropped, Dropped::default());
        assert_eq!(h.plane.close().into_nonces().len(), 1);
        assert_eq!(h.plane.len(), 0);
    }

    #[test]
    fn the_agent_budget_bounds_leases_and_one_connection_is_capped() {
        let mut h = harness(false);
        // One connection keeps at most four scopes, least recently used out.
        let mut own = Vec::new();
        for n in 0..MAX_LEASES_PER_CONNECTION as u8 {
            own.push(
                h.plane
                    .lease(connection(1), scope(n), revision(1))
                    .0
                    .unwrap(),
            );
            h.now.fetch_add(1, Ordering::SeqCst);
        }
        let (fifth, dropped) = h.plane.lease(connection(1), scope(9), revision(1));
        fifth.unwrap();
        assert_eq!(dropped.into_nonces(), vec![own[0].grant_nonce]);
        // Fill the budget with other connections; the next one is refused
        // without registering, and an idle sweep frees the slots.
        let mut n = 2u8;
        while h.plane.len() < MAX_AGENT_SELECTIONS {
            h.plane
                .lease(connection(n), scope(0), revision(1))
                .0
                .unwrap();
            n += 1;
        }
        let registered = h.registered.lock().unwrap().len();
        let (full, _) = h.plane.lease(connection(200), scope(0), revision(1));
        assert_eq!(full, Err(LeaseRefused::Capacity));
        assert_eq!(h.registered.lock().unwrap().len(), registered);
        h.now.fetch_add(IDLE_MS, Ordering::SeqCst);
        let (freed, dropped) = h.plane.lease(connection(200), scope(0), revision(1));
        freed.unwrap();
        assert_eq!(dropped.into_nonces().len(), MAX_AGENT_SELECTIONS);
        assert_eq!(h.plane.len(), 1);
    }

    #[test]
    fn a_gate_refusal_stores_nothing() {
        let mut h = harness(true);
        let (refused, _) = h.plane.lease(connection(1), scope(1), revision(1));
        assert_eq!(refused, Err(LeaseRefused::Unavailable));
        assert_eq!(h.plane.len(), 0);
    }

    #[test]
    fn the_scope_identity_names_the_sorted_index_set_and_revision() -> anyhow::Result<()> {
        let namespace = connection(7);
        let a = IndexId::from_identity(connection(1));
        let b = IndexId::from_identity(connection(2));
        let one = open_scope_identity(namespace, revision(2), &[a, b])?;
        assert_eq!(
            one,
            open_scope_identity(namespace, revision(2), &[b, a, b])?
        );
        assert_ne!(one, open_scope_identity(namespace, revision(3), &[a, b])?);
        assert_ne!(one, open_scope_identity(namespace, revision(2), &[a])?);
        assert!(open_scope_identity(namespace, revision(2), &[]).is_err());
        Ok(())
    }
}
