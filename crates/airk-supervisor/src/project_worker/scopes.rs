// SPDX-License-Identifier: Apache-2.0
//! Worker-local frozen authority. No registry/source observation occurs on reads.
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::IndexId,
    scoped_reads::ScopeReadRefused,
    service::{ConnectionGrant, ScopedReadRef, WorkerIdentity},
};
use airk_kernel::{
    Kernel,
    scoped_reads::{OpenReadAuthority, SelectionReadAuthority},
};
use anyhow::{Result, ensure};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Selection entries: 80 live owners plus 80 revoked
/// entries whose `RevokeScope` is still on its way (selection authority spec
/// §3.4, N6). The gate enforces the finer live/revoked split.
const MAX_SELECTION_SCOPES: usize = 160;
/// U105: open agent entries, counted apart: the gate's agent budget of live
/// plus revoked-awaiting-acknowledgement entries per worker.
const MAX_AGENT_SCOPES: usize = 2 * airk_catalog_control::catalog_admission::MAX_AGENT_SELECTIONS;
const MAX_PENDING: usize = 128;
const MAX_SESSIONS: usize = 128;
const MAX_LIFETIME_MS: u64 = 300_000;

/// The frozen authority a redeemed bearer opens a session with.
#[derive(Clone)]
pub(super) enum WorkerAuthority {
    Selection(SelectionReadAuthority),
    /// U105: one whole index of an open agent read, ledgered as a selection.
    Open(OpenReadAuthority),
}
/// The selection ref, index, backing and fingerprint of a selection-ledger
/// authority.
type SelectionParts = (
    airk_core::scoped_reads::SelectionScopeRef,
    IndexId,
    airk_core::service::ProjectBinding,
    Sha256Digest,
);
impl WorkerAuthority {
    fn selection_parts(&self) -> Result<SelectionParts> {
        match self {
            Self::Selection(authority) => authority.fingerprint().map(|fingerprint| {
                (
                    authority.selection,
                    authority.index_id,
                    authority.backing,
                    fingerprint,
                )
            }),
            Self::Open(authority) => authority.fingerprint().map(|fingerprint| {
                (
                    authority.selection,
                    authority.index_id,
                    authority.backing,
                    fingerprint,
                )
            }),
        }
    }
}

struct Installed {
    reference: ScopedReadRef,
    previous_lease: Option<ScopedReadRef>,
    authority: WorkerAuthority,
    expires: Instant,
    revoked: bool,
    sessions: BTreeMap<Incarnation, Kernel>,
}
/// Where a ref is installed: the selection ledger by the selection's nonce.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Slot(Incarnation, IndexId, Sha256Digest);
impl Slot {
    fn of(reference: &ScopedReadRef) -> Self {
        Self(
            reference.selection.grant_nonce,
            reference.index_id,
            reference.fingerprint,
        )
    }
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
    // In-memory selections by nonce. An entry is pruned on its RevokeScope: a
    // nonce is never reused and is no longer live in the gate, so it cannot
    // be resurrected (§3.4).
    selections: BTreeMap<(Incarnation, IndexId, Sha256Digest), Installed>,
    last_lease_generation: u64,
    pending: BTreeMap<Sha256Digest, (ConnectionGrant, Instant)>,
}
impl Scopes {
    pub fn new(identity: WorkerIdentity, required: bool) -> Self {
        Self {
            identity,
            required,
            selections: BTreeMap::new(),
            last_lease_generation: 0,
            pending: BTreeMap::new(),
        }
    }
    fn slot(&self, Slot(nonce, index, fingerprint): Slot) -> Option<&Installed> {
        self.selections.get(&(nonce, index, fingerprint))
    }
    fn slot_mut(&mut self, Slot(nonce, index, fingerprint): Slot) -> Option<&mut Installed> {
        self.selections.get_mut(&(nonce, index, fingerprint))
    }
    pub fn install_selection(
        &mut self,
        identity: WorkerIdentity,
        authority: SelectionReadAuthority,
        previous: Option<ScopedReadRef>,
        lease_generation: u64,
        expires_unix_ms: u64,
    ) -> Result<ScopedReadRef> {
        ensure!(
            identity == self.identity && authority.backing == identity.project,
            ScopeReadRefused
        );
        self.install_selection_lease(
            authority,
            lease_generation,
            Self::native_expiry(expires_unix_ms)?,
            previous,
        )
    }
    /// U105: install an open agent read's whole-index authority in the
    /// selection ledger, under the same lease, renewal and revocation rules.
    pub fn install_open(
        &mut self,
        identity: WorkerIdentity,
        authority: OpenReadAuthority,
        previous: Option<ScopedReadRef>,
        lease_generation: u64,
        expires_unix_ms: u64,
    ) -> Result<ScopedReadRef> {
        ensure!(
            identity == self.identity
                && authority.backing == identity.project
                && authority.selection.owner == airk_core::scoped_reads::SelectionOwnerKind::Agent,
            ScopeReadRefused
        );
        self.install_ledgered_lease(
            WorkerAuthority::Open(authority),
            lease_generation,
            Self::native_expiry(expires_unix_ms)?,
            previous,
        )
    }
    fn install_selection_lease(
        &mut self,
        authority: SelectionReadAuthority,
        lease_generation: u64,
        expires: Instant,
        previous: Option<ScopedReadRef>,
    ) -> Result<ScopedReadRef> {
        self.install_ledgered_lease(
            WorkerAuthority::Selection(authority),
            lease_generation,
            expires,
            previous,
        )
    }
    /// The selection ledger's install, renewal and capacity rules, shared by
    /// W4/A2 selections and U105 open agent reads (counted apart).
    fn install_ledgered_lease(
        &mut self,
        authority: WorkerAuthority,
        lease_generation: u64,
        expires: Instant,
        previous: Option<ScopedReadRef>,
    ) -> Result<ScopedReadRef> {
        let (selection, index_id, backing, fingerprint) = authority.selection_parts()?;
        ensure!(
            backing == self.identity.project
                && selection.service_instance == self.identity.service
                && expires > Instant::now(),
            ScopeReadRefused
        );
        let reference = ScopedReadRef {
            schema: 1,
            lease_generation,
            index_id,
            selection,
            fingerprint,
        };
        reference.validate()?;
        let key = (
            selection.grant_nonce,
            reference.index_id,
            reference.fingerprint,
        );
        if let Some(existing) = self.selections.get_mut(&key)
            && existing.reference == reference
        {
            ensure!(
                previous.is_none() || previous == existing.previous_lease,
                ScopeReadRefused
            );
            existing.live(reference, Instant::now())?;
            existing.expires = existing.expires.min(expires);
            return Ok(reference);
        }
        ensure!(
            lease_generation > self.last_lease_generation
                && lease_generation.checked_add(1).is_some(),
            ScopeReadRefused
        );
        if let Some(entry) = self.selections.get_mut(&key) {
            // Renewal of the same live selection's expired lease only.
            ensure!(
                previous == Some(entry.reference)
                    && !entry.revoked
                    && entry.expires <= Instant::now(),
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
        ensure!(
            previous.is_none() && self.has_selection_capacity(selection.owner),
            ScopeReadRefused
        );
        self.last_lease_generation = lease_generation;
        self.selections.insert(
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
    fn has_selection_capacity(&self, owner: airk_core::scoped_reads::SelectionOwnerKind) -> bool {
        use airk_core::scoped_reads::SelectionOwnerKind::Agent;
        let agent = owner == Agent;
        let same = self
            .selections
            .values()
            .filter(|entry| (entry.reference.selection.owner == Agent) == agent)
            .count();
        same < if agent {
            MAX_AGENT_SCOPES
        } else {
            MAX_SELECTION_SCOPES
        }
    }
    pub fn legacy_allowed(&self) -> bool {
        !self.required
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
    fn entry(&self, reference: ScopedReadRef) -> Result<&Installed> {
        let entry = self.slot(Slot::of(&reference)).ok_or(ScopeReadRefused)?;
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
    pub fn redeem_authority(&mut self, grant: &ConnectionGrant) -> Result<Option<WorkerAuthority>> {
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
        let entry = self.slot_mut(Slot::of(&scope)).ok_or(ScopeReadRefused)?;
        ensure!(
            entry.sessions.len() < MAX_SESSIONS && !entry.sessions.contains_key(&session),
            ScopeReadRefused
        );
        entry.sessions.insert(session, kernel);
        Ok(())
    }
    pub fn detach(&mut self, scope: Option<ScopedReadRef>, session: Incarnation) {
        if let Some(scope) = scope
            && let Some(entry) = self.slot_mut(Slot::of(&scope))
            && entry.reference == scope
            && let Some(kernel) = entry.sessions.remove(&session)
        {
            kernel.revoke_scoped_reads();
        }
    }
    pub fn revoke(&mut self, identity: WorkerIdentity, scope: ScopedReadRef) -> Result<()> {
        ensure!(identity == self.identity, ScopeReadRefused);
        scope.validate()?;
        self.revoke_selection(Slot::of(&scope), scope)
    }
    /// Revoke and prune a selection lease (§3.3, §3.4). Idempotent: a lease
    /// that is not installed (never installed, or already pruned) has nothing
    /// to revoke, so the host's acknowledgement can prune its own entry.
    fn revoke_selection(&mut self, slot: Slot, scope: ScopedReadRef) -> Result<()> {
        let Slot(nonce, index, fingerprint) = slot;
        let key = (nonce, index, fingerprint);
        if let Some(entry) = self.selections.get_mut(&key) {
            if entry.reference == scope {
                entry.revoke();
                self.selections.remove(&key);
            } else if entry.previous_lease == Some(scope) {
                // The superseded lease's readers were fenced at renewal.
                entry.previous_lease = None;
            }
        }
        self.pending
            .retain(|_, (grant, _)| grant.scope.is_none_or(|bound| Slot::of(&bound) != slot));
        Ok(())
    }
    pub fn close(&mut self) {
        for entry in self.selections.values_mut() {
            entry.revoke();
        }
        self.pending.clear();
    }
    pub fn expire(&mut self) {
        let now = Instant::now();
        for entry in self.selections.values_mut() {
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

    fn fixture(root: &str) -> (WorkerIdentity, SelectionReadAuthority) {
        selection_fixture(root, 0x5c)
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
    fn selection_redemption_requires_exact_payload_and_one_time_bearer() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (identity, authority) = fixture(root.path().to_str().unwrap());
        let mut scopes = Scopes::new(identity, true);
        assert!(scopes.redeem_authority(&grant(identity)).is_err());
        let scope = scopes.install_selection_lease(authority.clone(), 1, lifetime(), None)?;
        let mut issued = grant(identity);
        scopes.bind(&mut issued, scope)?;
        let mut stripped = issued.clone();
        stripped.scope = None;
        assert!(scopes.redeem_authority(&stripped).is_err());
        let mut altered = issued.clone();
        altered.scope.as_mut().unwrap().fingerprint = Sha256Digest::from_bytes([11; 32]);
        assert!(scopes.redeem_authority(&altered).is_err());
        let Some(WorkerAuthority::Selection(redeemed)) = scopes.redeem_authority(&issued)? else {
            anyhow::bail!("selection authority missing");
        };
        assert_eq!(redeemed.fingerprint()?, authority.fingerprint()?);
        assert!(scopes.redeem_authority(&issued).is_err());
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
        let scope = scopes.install_selection_lease(authority, 1, lifetime(), None)?;
        let mut issued = grant(identity);
        scopes.bind(&mut issued, scope)?;
        let Some(WorkerAuthority::Selection(authority)) = scopes.redeem_authority(&issued)? else {
            anyhow::bail!("selection authority missing");
        };
        let mut base = airk_mcp::local::LocalTools::new(kernel).await?;
        let mut session = base
            .selection_session_with_client(authority, airk_core::usage::ClientFamily::Unknown)
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

    fn selection_fixture(root: &str, nonce: u8) -> (WorkerIdentity, SelectionReadAuthority) {
        let identity = WorkerIdentity {
            service: Incarnation::from_bytes([1; 16]),
            worker: Incarnation::from_bytes([2; 16]),
            project: Sha256Digest::from_bytes([3; 32]),
            fencing_generation: 1,
        };
        let index_id = IndexId::from_identity(Incarnation::from_bytes([4; 16]));
        let project = QueryProjectId::from_identity(Incarnation::from_bytes([5; 16]));
        let source = SourceRootId::from_identity(Incarnation::from_bytes([6; 16]));
        let authority = SelectionReadAuthority {
            schema: 1,
            backing: identity.project,
            index_id,
            project_ids: vec![project],
            selection: airk_core::scoped_reads::SelectionScopeRef {
                owner: airk_core::scoped_reads::SelectionOwnerKind::Window,
                grant_nonce: Incarnation::from_bytes([nonce; 16]),
                service_instance: identity.service,
                scope_identity: Sha256Digest::from_bytes([0x33; 32]),
                catalog_revision: CatalogRevision::INITIAL,
            },
            atoms: vec![(
                project,
                ScopeAtom {
                    source_root_id: source,
                    kind: PathScope::Subtree,
                    path: String::new(),
                },
            )],
            sources: vec![SourceDescriptor {
                source_root_id: source,
                original_root: root.into(),
            }],
            membership_revisions: vec![(project, CatalogRevision::INITIAL)],
            source_revision: CatalogRevision::INITIAL,
            policy_epoch: 0,
            policy_digest: None,
        };
        (identity, authority)
    }

    #[test]
    fn selection_scope_is_ledgered_apart_and_pruned_by_its_revocation() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (identity, authority) = selection_fixture(root.path().to_str().unwrap(), 0x5c);
        let mut scopes = Scopes::new(identity, true);
        let scope = scopes.install_selection_lease(authority.clone(), 1, lifetime(), None)?;
        assert_eq!(scope.selection, authority.selection);
        assert_eq!(scopes.selections.len(), 1);
        let mut issued = grant(identity);
        scopes.bind(&mut issued, scope)?;
        assert!(matches!(
            scopes.redeem_authority(&issued)?,
            Some(WorkerAuthority::Selection(_))
        ));
        let mut pending = grant(identity);
        pending.token = Sha256Digest::from_bytes([21; 32]);
        scopes.bind(&mut pending, scope)?;
        scopes.revoke(identity, scope)?;
        // Pruned, not tombstoned; the bound bearer died with it.
        assert_eq!(scopes.selections.len(), 0);
        assert!(scopes.check(Some(scope)).is_err());
        assert!(scopes.redeem_authority(&pending).is_err());
        // A repeated or unknown selection revocation settles idempotently.
        scopes.revoke(identity, scope)?;
        // Another service's selection is never installed.
        let mut foreign = authority;
        foreign.selection.service_instance = Incarnation::from_bytes([15; 16]);
        assert!(
            scopes
                .install_selection_lease(foreign, 2, lifetime(), None)
                .is_err()
        );
        Ok(())
    }

    fn open_fixture(root: &str, nonce: u8) -> (WorkerIdentity, OpenReadAuthority) {
        let (identity, selection) = selection_fixture(root, nonce);
        let mut scope = selection.selection;
        scope.owner = airk_core::scoped_reads::SelectionOwnerKind::Agent;
        let authority = OpenReadAuthority {
            schema: 1,
            backing: identity.project,
            index_id: selection.index_id,
            selection: scope,
            sources: selection.sources,
            source_revision: CatalogRevision::INITIAL,
            policy_epoch: 0,
            policy_digest: None,
        };
        (identity, authority)
    }

    /// U105: an open agent read is ledgered as a selection, redeems only
    /// through the authority path, is pruned by its revocation, and is
    /// counted apart from W4/A2 selections.
    #[test]
    fn open_scope_is_a_selection_entry_with_its_own_capacity() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().to_str().unwrap();
        let (identity, authority) = open_fixture(path, 0x6a);
        let mut scopes = Scopes::new(identity, true);
        let expires =
            u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())? + 60_000;
        let mut window = authority.clone();
        window.selection.owner = airk_core::scoped_reads::SelectionOwnerKind::Window;
        assert!(
            scopes
                .install_open(identity, window, None, 1, expires)
                .is_err()
        );
        let scope = scopes.install_open(identity, authority.clone(), None, 1, expires)?;
        assert_eq!(scope.selection, authority.selection);
        assert_eq!(scopes.selections.len(), 1);
        let mut issued = grant(identity);
        scopes.bind(&mut issued, scope)?;
        assert!(matches!(
            scopes.redeem_authority(&issued)?,
            Some(WorkerAuthority::Open(_))
        ));
        scopes.revoke(identity, scope)?;
        assert_eq!(scopes.selections.len(), 0);
        // Agent entries fill only their own class.
        for n in 0..MAX_AGENT_SCOPES as u8 {
            let (_, open) = open_fixture(path, n);
            scopes.install_ledgered_lease(
                WorkerAuthority::Open(open),
                u64::from(n) + 2,
                lifetime(),
                None,
            )?;
        }
        let (_, overflow) = open_fixture(path, 0xf0);
        assert!(
            scopes
                .install_ledgered_lease(WorkerAuthority::Open(overflow), 300, lifetime(), None)
                .is_err()
        );
        let (_, selection) = selection_fixture(path, 0xf1);
        scopes.install_selection_lease(selection, 301, lifetime(), None)?;
        Ok(())
    }

    #[test]
    fn selection_capacity_is_bounded() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (identity, _) = fixture(root.path().to_str().unwrap());
        let mut scopes = Scopes::new(identity, true);
        for n in 0..160u8 {
            let (_, authority) = selection_fixture(root.path().to_str().unwrap(), n);
            scopes.install_selection_lease(authority, u64::from(n) + 1, lifetime(), None)?;
        }
        let (_, overflow) = selection_fixture(root.path().to_str().unwrap(), 200);
        assert!(
            scopes
                .install_selection_lease(overflow, 500, lifetime(), None)
                .is_err()
        );
        assert_eq!(scopes.selections.len(), 160);
        Ok(())
    }
}
