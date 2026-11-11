// SPDX-License-Identifier: Apache-2.0
//! Worker-local frozen authority. No registry/source observation occurs on reads.
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::{CatalogRevision, GrantId, IndexId},
    scoped_reads::ScopeReadRefused,
    service::{ConnectionGrant, ScopedReadRef, WorkerIdentity},
};
use airk_kernel::{Kernel, scoped_reads::ScopedReadAuthority};
use anyhow::{Result, ensure};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_SCOPES: usize = 128;
const MAX_PENDING: usize = 128;
const MAX_SESSIONS: usize = 128;
const MAX_LIFETIME_MS: u64 = 300_000;

struct Installed {
    reference: ScopedReadRef,
    previous_lease: Option<ScopedReadRef>,
    authority: ScopedReadAuthority,
    expires: Instant,
    revoked: bool,
    sessions: BTreeMap<Incarnation, Kernel>,
}
impl Installed {
    fn live(&self, reference: ScopedReadRef, now: Instant) -> Result<()> {
        reference.validate()?;
        ensure!(
            self.reference == reference && !self.revoked && now < self.expires,
            ScopeReadRefused
        );
        Ok(())
    }
    fn revoke(&mut self) {
        self.revoked = true;
        self.revoke_readers();
    }
    fn revoke_readers(&self) {
        for kernel in self.sessions.values() {
            kernel.revoke_scoped_reads();
        }
    }
}

pub(super) struct Scopes {
    identity: WorkerIdentity,
    required: bool,
    // Exact fingerprint tombstones and revision high-water last until worker exit. Capacity
    // refusal is preferable to resurrecting an evicted revoked authority.
    installed: BTreeMap<(GrantId, IndexId, Sha256Digest), Installed>,
    highwater: BTreeMap<(GrantId, IndexId), CatalogRevision>,
    last_lease_generation: u64,
    pending: BTreeMap<Sha256Digest, (ConnectionGrant, Instant)>,
}
impl Scopes {
    pub fn new(identity: WorkerIdentity, required: bool) -> Self {
        Self {
            identity,
            required,
            installed: BTreeMap::new(),
            highwater: BTreeMap::new(),
            last_lease_generation: 0,
            pending: BTreeMap::new(),
        }
    }
    pub fn legacy_allowed(&self) -> bool {
        !self.required
    }
    pub fn install(
        &mut self,
        identity: WorkerIdentity,
        authority: ScopedReadAuthority,
        lease_generation: u64,
        expires_unix_ms: u64,
    ) -> Result<ScopedReadRef> {
        ensure!(
            identity == self.identity && authority.backing == identity.project,
            ScopeReadRefused
        );
        self.install_lease(
            authority,
            lease_generation,
            Self::native_expiry(expires_unix_ms)?,
            None,
        )
    }
    fn native_expiry(expires_unix_ms: u64) -> Result<Instant> {
        let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
        let remaining = u128::from(expires_unix_ms)
            .checked_sub(now_ms)
            .ok_or(ScopeReadRefused)?;
        ensure!(
            remaining > 0 && remaining <= u128::from(MAX_LIFETIME_MS),
            ScopeReadRefused
        );
        Ok(Instant::now() + Duration::from_millis(remaining as u64))
    }
    pub fn renew_scope_lease(
        &mut self,
        identity: WorkerIdentity,
        authority: ScopedReadAuthority,
        previous: ScopedReadRef,
        lease_generation: u64,
        expires_unix_ms: u64,
    ) -> Result<ScopedReadRef> {
        ensure!(identity == self.identity, ScopeReadRefused);
        self.install_lease(
            authority,
            lease_generation,
            Self::native_expiry(expires_unix_ms)?,
            Some(previous),
        )
    }
    #[cfg(test)]
    fn install_until(
        &mut self,
        authority: ScopedReadAuthority,
        expires: Instant,
    ) -> Result<ScopedReadRef> {
        let fingerprint = authority.fingerprint()?;
        let generation = self
            .installed
            .get(&(authority.grant.id, authority.index_id, fingerprint))
            .map_or(
                self.last_lease_generation
                    .checked_add(1)
                    .ok_or(ScopeReadRefused)?,
                |entry| entry.reference.lease_generation,
            );
        self.install_lease(authority, generation, expires, None)
    }
    fn install_lease(
        &mut self,
        authority: ScopedReadAuthority,
        lease_generation: u64,
        expires: Instant,
        previous: Option<ScopedReadRef>,
    ) -> Result<ScopedReadRef> {
        ensure!(
            authority.backing == self.identity.project && expires > Instant::now(),
            ScopeReadRefused
        );
        let reference = ScopedReadRef {
            schema: 1,
            lease_generation,
            index_id: authority.index_id,
            grant_id: authority.grant.id,
            grant_revision: authority.grant.revision,
            fingerprint: authority.fingerprint()?,
        };
        reference.validate()?;
        let family = (reference.grant_id, reference.index_id);
        ensure!(
            self.highwater
                .get(&family)
                .is_none_or(|revision| reference.grant_revision >= *revision),
            ScopeReadRefused
        );
        let key = (
            reference.grant_id,
            reference.index_id,
            reference.fingerprint,
        );
        if let Some(existing) = self.installed.get_mut(&key) {
            if existing.reference == reference {
                ensure!(
                    previous.is_none() || previous == existing.previous_lease,
                    ScopeReadRefused
                );
                existing.live(reference, Instant::now())?;
                // Private native reinstallation may narrow expiry, never renew a
                // cached scope merely because another frontend requested it.
                existing.expires = existing.expires.min(expires);
                return Ok(reference);
            }
        }
        ensure!(
            lease_generation > self.last_lease_generation
                && lease_generation.checked_add(1).is_some(),
            ScopeReadRefused
        );
        if let Some(entry) = self.installed.get_mut(&key) {
            ensure!(
                previous == Some(entry.reference)
                    && !entry.revoked
                    && entry.expires <= Instant::now()
                    && reference.grant_revision == entry.reference.grant_revision,
                ScopeReadRefused
            );
            let old = entry.reference;
            entry.revoke_readers();
            entry.sessions.clear();
            entry.previous_lease = Some(old);
            entry.reference = reference;
            entry.expires = expires;
            self.pending
                .retain(|_, (grant, _)| grant.scope != Some(old));
            self.last_lease_generation = lease_generation;
            return Ok(reference);
        }
        ensure!(previous.is_none(), ScopeReadRefused);
        ensure!(self.installed.len() < MAX_SCOPES, ScopeReadRefused);
        if self
            .highwater
            .get(&family)
            .is_some_and(|revision| reference.grant_revision > *revision)
        {
            for entry in self
                .installed
                .values_mut()
                .filter(|entry| (entry.reference.grant_id, entry.reference.index_id) == family)
            {
                entry.revoke();
            }
            self.pending.retain(|_, (grant, _)| {
                grant
                    .scope
                    .is_none_or(|scope| (scope.grant_id, scope.index_id) != family)
            });
        }
        self.highwater.insert(family, reference.grant_revision);
        self.last_lease_generation = lease_generation;
        self.installed.insert(
            key,
            Installed {
                reference,
                previous_lease: None,
                authority,
                expires,
                revoked: false,
                sessions: BTreeMap::new(),
            },
        );
        Ok(reference)
    }
    fn entry(&self, reference: ScopedReadRef) -> Result<&Installed> {
        ensure!(
            self.highwater
                .get(&(reference.grant_id, reference.index_id))
                == Some(&reference.grant_revision),
            ScopeReadRefused
        );
        let entry = self
            .installed
            .get(&(
                reference.grant_id,
                reference.index_id,
                reference.fingerprint,
            ))
            .ok_or(ScopeReadRefused)?;
        entry.live(reference, Instant::now())?;
        Ok(entry)
    }
    pub fn check(&self, reference: Option<ScopedReadRef>) -> Result<()> {
        match reference {
            Some(scope) => {
                self.entry(scope)?;
            }
            None => ensure!(self.legacy_allowed(), ScopeReadRefused),
        }
        Ok(())
    }
    pub fn bind(&mut self, grant: &mut ConnectionGrant, scope: ScopedReadRef) -> Result<()> {
        let installed_expiry = self.entry(scope)?.expires;
        ensure!(
            grant.identity == self.identity && grant.scope.is_none(),
            ScopeReadRefused
        );
        ensure!(
            (1..=airk_core::service::MAX_GRANT_TTL_MS).contains(&grant.ttl_ms),
            ScopeReadRefused
        );
        self.pending
            .retain(|_, (_, expires)| *expires > Instant::now());
        ensure!(
            self.pending.len() < MAX_PENDING && !self.pending.contains_key(&grant.token),
            ScopeReadRefused
        );
        grant.scope = Some(scope);
        self.pending.insert(
            grant.token,
            (
                grant.clone(),
                installed_expiry.min(Instant::now() + Duration::from_millis(grant.ttl_ms)),
            ),
        );
        Ok(())
    }
    pub fn redeem(&mut self, grant: &ConnectionGrant) -> Result<Option<ScopedReadAuthority>> {
        ensure!(grant.identity == self.identity, ScopeReadRefused);
        match grant.scope {
            Some(scope) => {
                self.entry(scope)?;
                let (bound, expires) = self.pending.remove(&grant.token).ok_or(ScopeReadRefused)?;
                ensure!(
                    bound.scope == grant.scope
                        && bound.identity == grant.identity
                        && bound.caller == grant.caller
                        && bound.protocol == grant.protocol
                        && bound.ttl_ms == grant.ttl_ms
                        && expires > Instant::now(),
                    ScopeReadRefused
                );
                // ClientFamily is the existing mutable accounting label; it is
                // preserved into LocalTools, never treated as bearer authority.
                Ok(Some(self.entry(scope)?.authority.clone()))
            }
            None => {
                ensure!(
                    !self.pending.contains_key(&grant.token) && self.legacy_allowed(),
                    ScopeReadRefused
                );
                Ok(None)
            }
        }
    }
    pub fn attach(
        &mut self,
        scope: ScopedReadRef,
        session: Incarnation,
        kernel: Kernel,
    ) -> Result<()> {
        if self.entry(scope).is_err() {
            kernel.revoke_scoped_reads();
            return Err(ScopeReadRefused.into());
        }
        let entry = self
            .installed
            .get_mut(&(scope.grant_id, scope.index_id, scope.fingerprint))
            .ok_or(ScopeReadRefused)?;
        ensure!(
            entry.sessions.len() < MAX_SESSIONS && !entry.sessions.contains_key(&session),
            ScopeReadRefused
        );
        entry.sessions.insert(session, kernel);
        Ok(())
    }
    pub fn detach(&mut self, scope: Option<ScopedReadRef>, session: Incarnation) {
        if let Some(scope) = scope
            && let Some(entry) =
                self.installed
                    .get_mut(&(scope.grant_id, scope.index_id, scope.fingerprint))
            && entry.reference == scope
            && let Some(kernel) = entry.sessions.remove(&session)
        {
            kernel.revoke_scoped_reads();
        }
    }
    pub fn revoke(&mut self, identity: WorkerIdentity, scope: ScopedReadRef) -> Result<()> {
        ensure!(identity == self.identity, ScopeReadRefused);
        scope.validate()?;
        let entry = self
            .installed
            .get_mut(&(scope.grant_id, scope.index_id, scope.fingerprint))
            .ok_or(ScopeReadRefused)?;
        ensure!(
            entry.reference == scope || entry.previous_lease == Some(scope),
            ScopeReadRefused
        );
        entry.revoke();
        self.pending.retain(|_, (grant, _)| {
            grant.scope.is_none_or(|bound| {
                (bound.grant_id, bound.index_id, bound.fingerprint)
                    != (scope.grant_id, scope.index_id, scope.fingerprint)
            })
        });
        Ok(())
    }
    pub fn close(&mut self) {
        for entry in self.installed.values_mut() {
            entry.revoke();
        }
        self.pending.clear();
    }
    pub fn expire(&mut self) {
        let now = Instant::now();
        for entry in self.installed.values_mut() {
            if now >= entry.expires {
                // Transport expiry fences this lease, not logical authority.
                // Only fresh native validation may install a later lease.
                entry.revoke_readers();
            }
        }
        self.pending.retain(|_, (_, expires)| now < *expires);
    }
}

pub(super) struct SessionFence {
    pub scopes: std::sync::Arc<std::sync::Mutex<Scopes>>,
    pub reference: Option<ScopedReadRef>,
    pub session: Incarnation,
}
impl Drop for SessionFence {
    fn drop(&mut self) {
        if let Ok(mut scopes) = self.scopes.lock() {
            scopes.detach(self.reference, self.session);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{index_catalog::*, scoped_reads::SourceDescriptor};

    fn fixture(root: &str) -> (WorkerIdentity, ScopedReadAuthority) {
        let identity = WorkerIdentity {
            service: Incarnation::from_bytes([1; 16]),
            worker: Incarnation::from_bytes([2; 16]),
            project: Sha256Digest::from_bytes([3; 32]),
            fencing_generation: 1,
        };
        let index_id = IndexId::from_identity(Incarnation::from_bytes([4; 16]));
        let project = QueryProjectId::from_identity(Incarnation::from_bytes([5; 16]));
        let source = SourceRootId::from_identity(Incarnation::from_bytes([6; 16]));
        let authority = ScopedReadAuthority {
            schema: 1,
            backing: identity.project,
            index_id,
            project_ids: vec![project],
            grant: ReviewedGrant {
                id: GrantId::from_identity(Incarnation::from_bytes([7; 16])),
                host: Sha256Digest::from_bytes([8; 32]),
                revision: CatalogRevision::INITIAL,
                revoked: false,
                memberships: vec![GrantedMembership {
                    membership: Membership {
                        project_id: project,
                        index_id,
                        revision: CatalogRevision::INITIAL,
                        scope: vec![ScopeAtom {
                            source_root_id: source,
                            kind: PathScope::Subtree,
                            path: String::new(),
                        }],
                    },
                    source_revision: CatalogRevision::INITIAL,
                    policy_epoch: 0,
                    policy_digest: None,
                }],
            },
            sources: vec![SourceDescriptor {
                source_root_id: source,
                original_root: root.into(),
            }],
        };
        (identity, authority)
    }
    fn grant(identity: WorkerIdentity) -> ConnectionGrant {
        ConnectionGrant {
            protocol: airk_core::service::PROTOCOL_VERSION,
            identity,
            caller: Incarnation::from_bytes([9; 16]),
            token: Sha256Digest::from_bytes([10; 32]),
            ttl_ms: 15_000,
            client: airk_core::usage::ClientFamily::Unknown,
            scope: None,
        }
    }
    fn lifetime() -> Instant {
        Instant::now() + Duration::from_secs(60)
    }

    #[test]
    fn scope_install_redemption_requires_exact_payload_and_one_time_bearer() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (identity, authority) = fixture(root.path().to_str().unwrap());
        let mut scopes = Scopes::new(identity, true);
        assert!(scopes.redeem(&grant(identity)).is_err());
        let scope = scopes.install_until(authority.clone(), lifetime())?;
        let mut issued = grant(identity);
        scopes.bind(&mut issued, scope)?;
        let mut stripped = issued.clone();
        stripped.scope = None;
        assert!(scopes.redeem(&stripped).is_err());
        let mut altered = issued.clone();
        altered.scope.as_mut().unwrap().fingerprint = Sha256Digest::from_bytes([11; 32]);
        assert!(scopes.redeem(&altered).is_err());
        let redeemed = scopes.redeem(&issued)?.unwrap();
        assert_eq!(redeemed.fingerprint()?, authority.fingerprint()?);
        assert!(scopes.redeem(&issued).is_err());
        let original_expiry = scopes.entry(scope)?.expires;
        assert_eq!(scopes.install_until(authority, lifetime())?, scope);
        assert_eq!(scopes.entry(scope)?.expires, original_expiry);
        Ok(())
    }

    #[test]
    fn narrowing_and_revocation_never_resurrect_older_revision_or_foreign_worker() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (identity, mut authority) = fixture(root.path().to_str().unwrap());
        let mut scopes = Scopes::new(identity, true);
        let old = scopes.install_until(authority.clone(), lifetime())?;
        let mut pending = grant(identity);
        scopes.bind(&mut pending, old)?;
        let mut foreign = identity;
        foreign.fencing_generation += 1;
        assert!(scopes.revoke(foreign, old).is_err());
        authority.grant.revision = authority.grant.revision.next()?;
        authority.grant.memberships[0].membership.scope[0].path = "public".into();
        let current = scopes.install_until(authority.clone(), lifetime())?;
        assert!(scopes.check(Some(old)).is_err());
        assert!(scopes.redeem(&pending).is_err());
        scopes.revoke(identity, current)?;
        scopes.revoke(identity, current)?;
        assert!(scopes.check(Some(current)).is_err());
        assert!(scopes.install_until(authority.clone(), lifetime()).is_err());
        authority.grant.revision = CatalogRevision::INITIAL;
        assert!(scopes.install_until(authority, lifetime()).is_err());
        Ok(())
    }

    #[test]
    fn tombstones_stay_bounded_and_pending_bearer_cannot_outlive_scope() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (identity, authority) = fixture(root.path().to_str().unwrap());
        let mut scopes = Scopes::new(identity, true);
        for n in 0..MAX_SCOPES {
            let mut next = authority.clone();
            next.grant.id = GrantId::from_identity(Incarnation::from_bytes([n as u8; 16]));
            let scope = scopes.install_until(next, lifetime())?;
            scopes.revoke(identity, scope)?;
        }
        let mut extra = authority.clone();
        extra.grant.id = GrantId::from_identity(Incarnation::from_bytes([255; 16]));
        assert!(scopes.install_until(extra, lifetime()).is_err());
        assert_eq!(scopes.installed.len(), MAX_SCOPES);
        let mut scopes = Scopes::new(identity, true);
        let expiry = Instant::now() + Duration::from_secs(2);
        let scope = scopes.install_until(authority, expiry)?;
        let mut issued = grant(identity);
        scopes.bind(&mut issued, scope)?;
        assert_eq!(scopes.pending.get(&issued.token).unwrap().1, expiry);
        let mut unsupported = scope;
        unsupported.schema = 2;
        assert!(scopes.check(Some(unsupported)).is_err());
        let mut foreign = issued.clone();
        foreign.identity.service = Incarnation::from_bytes([15; 16]);
        assert!(scopes.redeem(&foreign).is_err());
        scopes.close();
        assert!(scopes.redeem(&issued).is_err());
        Ok(())
    }

    #[test]
    fn expiry_contract_and_worker_binding_refuse_without_legacy_expansion() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (identity, mut authority) = fixture(root.path().to_str().unwrap());
        let mut scopes = Scopes::new(identity, true);
        assert!(scopes.install(identity, authority.clone(), 1, 0).is_err());
        let mut foreign = identity;
        foreign.worker = Incarnation::from_bytes([12; 16]);
        assert!(
            scopes
                .install(foreign, authority.clone(), 1, u64::MAX)
                .is_err()
        );
        authority.schema = 2;
        assert!(scopes.install_until(authority.clone(), lifetime()).is_err());
        authority.schema = 1;
        authority.backing = Sha256Digest::from_bytes([13; 32]);
        assert!(scopes.install_until(authority, lifetime()).is_err());
        assert!(scopes.check(None).is_err());
        assert!(Scopes::new(identity, false).check(None).is_ok());
        Ok(())
    }

    #[test]
    fn parallel_subsets_renew_expired_lease_without_regrant_and_advance_revokes_all_readers()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("source");
        for name in ["one", "two"] {
            std::fs::create_dir_all(root.join(name))?;
            std::fs::write(
                root.join(name).join("file.rs"),
                "pub struct ScopedMarker;\n",
            )?;
        }
        let kernel = Kernel::open(&root, Some(owner.path().join("data")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let (mut identity, mut one) = fixture(kernel.root().to_str().unwrap());
        identity.project = airk_core::service::ProjectRegistration::new(
            kernel.root().to_string_lossy().into_owned(),
            kernel.data_dir().to_string_lossy().into_owned(),
            kernel.collection().into(),
        )?
        .binding;
        one.backing = identity.project;
        one.grant.memberships[0].membership.scope[0].path = "one".into();
        let mut second = one.grant.memberships[0].clone();
        second.membership.project_id =
            QueryProjectId::from_identity(Incarnation::from_bytes([16; 16]));
        second.membership.scope[0].path = "two".into();
        one.grant.memberships.push(second.clone());
        let mut two = one.clone();
        two.project_ids = vec![second.membership.project_id];
        let mut scopes = Scopes::new(identity, true);
        let first = scopes.install_lease(one.clone(), 1, lifetime(), None)?;
        let second = scopes.install_lease(two.clone(), 2, lifetime(), None)?;
        assert_ne!(first.fingerprint, second.fingerprint);
        let first_reader = kernel.with_scoped_reads(one.clone())?;
        let second_reader = kernel.with_scoped_reads(two.clone())?;
        scopes.attach(
            first,
            Incarnation::from_bytes([17; 16]),
            first_reader.clone(),
        )?;
        scopes.attach(
            second,
            Incarnation::from_bytes([18; 16]),
            second_reader.clone(),
        )?;
        assert!(first_reader.file_info("one/file.rs").is_ok());
        assert!(second_reader.file_info("two/file.rs").is_ok());
        scopes.revoke(identity, first)?;
        assert!(first_reader.file_info("one/file.rs").is_err());
        assert!(second_reader.file_info("two/file.rs").is_ok());
        assert!(
            scopes
                .install_lease(one.clone(), 3, lifetime(), Some(first))
                .is_err()
        );
        let mut old_bearer = grant(identity);
        old_bearer.token = Sha256Digest::from_bytes([19; 32]);
        scopes.bind(&mut old_bearer, second)?;
        scopes
            .installed
            .get_mut(&(second.grant_id, second.index_id, second.fingerprint))
            .unwrap()
            .expires = Instant::now() - Duration::from_secs(1);
        scopes.expire();
        assert!(second_reader.file_info("two/file.rs").is_err());
        assert!(
            scopes
                .install_lease(two.clone(), 2, lifetime(), None)
                .is_err()
        );
        let renewed = scopes.install_lease(two.clone(), 3, lifetime(), Some(second))?;
        assert_eq!(renewed.fingerprint, second.fingerprint);
        assert_eq!(renewed.grant_revision, second.grant_revision);
        assert_eq!(
            scopes.installed.len(),
            2,
            "lease renewal accumulated tombstones"
        );
        let expiry = scopes.entry(renewed)?.expires;
        assert_eq!(
            scopes.install_lease(two.clone(), 3, lifetime(), Some(second))?,
            renewed
        );
        assert_eq!(scopes.entry(renewed)?.expires, expiry);
        assert!(scopes.redeem(&old_bearer).is_err());
        assert!(scopes.check(Some(second)).is_err());
        let renewed_reader = kernel.with_scoped_reads(two.clone())?;
        scopes.attach(
            renewed,
            Incarnation::from_bytes([20; 16]),
            renewed_reader.clone(),
        )?;
        assert!(renewed_reader.file_info("two/file.rs").is_ok());
        let mut fresh_bearer = grant(identity);
        scopes.bind(&mut fresh_bearer, renewed)?;
        let mut newer = two.clone();
        newer.grant.revision = newer.grant.revision.next()?;
        assert!(
            scopes
                .install_lease(newer.clone(), u64::MAX, lifetime(), None)
                .is_err()
        );
        scopes.check(Some(renewed))?;
        scopes.install_lease(newer, 4, lifetime(), None)?;
        assert!(renewed_reader.file_info("two/file.rs").is_err());
        assert!(scopes.redeem(&fresh_bearer).is_err());
        assert!(scopes.check(Some(first)).is_err());
        assert!(scopes.check(Some(renewed)).is_err());
        assert!(scopes.install_lease(two, 5, lifetime(), None).is_err());
        Ok(())
    }

    #[test]
    fn repeated_native_expired_lease_renewal_is_bounded_and_explicit_revoke_is_permanent()
    -> Result<()> {
        let root = tempfile::tempdir()?;
        let (identity, authority) = fixture(root.path().to_str().unwrap());
        let mut scopes = Scopes::new(identity, true);
        let mut current = scopes.install_lease(authority.clone(), 1, lifetime(), None)?;
        for generation in 2..22 {
            scopes.installed.values_mut().next().unwrap().expires =
                Instant::now() - Duration::from_secs(1);
            scopes.expire();
            let previous = current;
            current =
                scopes.install_lease(authority.clone(), generation, lifetime(), Some(previous))?;
            assert!(scopes.check(Some(previous)).is_err());
            assert_eq!(scopes.installed.len(), 1);
            assert_eq!(scopes.highwater.len(), 1);
        }
        let known_previous = scopes
            .installed
            .values()
            .next()
            .unwrap()
            .previous_lease
            .unwrap();
        let mut unknown = known_previous;
        unknown.lease_generation = 999;
        assert!(scopes.revoke(identity, unknown).is_err());
        scopes.revoke(identity, known_previous)?;
        assert!(scopes.check(Some(current)).is_err());
        scopes.installed.values_mut().next().unwrap().expires =
            Instant::now() - Duration::from_secs(1);
        assert!(
            scopes
                .install_lease(authority, 22, lifetime(), Some(current))
                .is_err()
        );
        Ok(())
    }

    #[tokio::test]
    async fn installed_session_fence_revokes_actual_localtools_and_retained_kernel_reader()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("visible.rs"), "pub struct Visible;\n")?;
        let kernel = Kernel::open(&root, Some(owner.path().join("data")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let (mut identity, mut authority) = fixture(kernel.root().to_str().unwrap());
        identity.project = airk_core::service::ProjectRegistration::new(
            kernel.root().to_string_lossy().into_owned(),
            kernel.data_dir().to_string_lossy().into_owned(),
            kernel.collection().into(),
        )?
        .binding;
        authority.backing = identity.project;
        let mut scopes = Scopes::new(identity, true);
        let scope = scopes.install_until(authority, lifetime())?;
        let mut issued = grant(identity);
        scopes.bind(&mut issued, scope)?;
        let authority = scopes.redeem(&issued)?.unwrap();
        let mut base = airk_mcp::local::LocalTools::new(kernel).await?;
        let mut session = base
            .scoped_session_with_client(authority, airk_core::usage::ClientFamily::Unknown)
            .await?;
        let reader = session.scoped_read_revocation_handle();
        assert!(reader.file_info("visible.rs").is_ok());
        let session_id = Incarnation::from_bytes([14; 16]);
        scopes.attach(scope, session_id, reader.clone())?;
        let before = session
            .call(
                "airk_file_info".into(),
                serde_json::json!({"path":"visible.rs"})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .await?;
        assert_ne!(before.is_error, Some(true));
        // A completed/serialized reply is still fenced at native delivery.
        let serialized = serde_json::to_vec(&before)?;
        scopes.revoke(identity, scope)?;
        assert!(!serialized.is_empty());
        assert!(scopes.check(Some(scope)).is_err());
        let (mut sender, mut receiver) = tokio::io::duplex(1024);
        assert!(
            super::super::deliver_frame(&mut sender, &serialized, || scopes.check(Some(scope)))
                .await
                .is_err()
        );
        drop(sender);
        use tokio::io::AsyncReadExt;
        let mut delivered = Vec::new();
        receiver.read_to_end(&mut delivered).await?;
        assert!(
            delivered.is_empty(),
            "serialized protected response escaped its delivery fence"
        );
        assert!(reader.file_info("visible.rs").is_err());
        let after = session
            .call(
                "airk_file_info".into(),
                serde_json::json!({"path":"visible.rs"})
                    .as_object()
                    .unwrap()
                    .clone(),
            )
            .await?;
        assert_eq!(after.is_error, Some(true));
        scopes.detach(Some(scope), session_id);
        session.close().await?;
        base.close().await?;
        Ok(())
    }
}
