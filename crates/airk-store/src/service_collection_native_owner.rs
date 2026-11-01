// SPDX-License-Identifier: Apache-2.0
//! Native lifecycle enrollment and sealed request-scoped private read authority.
use super::*;
use crate::service::registry::{
    IntegrityAuditState,
    owner_session::{EnrollmentBasis, OwnerSession, POLICY, SessionState},
};
use airk_core::service::WorkerIdentity;

#[cfg(windows)]
struct RootPin(crate::service_census_windows::NativeRoot);
#[cfg(target_os = "linux")]
struct RootPin {
    file: File,
    path: PathBuf,
    private_boundary: bool,
}
#[cfg(not(any(windows, target_os = "linux")))]
struct RootPin;
impl RootPin {
    fn open(path: &Path, private_boundary: bool) -> Result<Self> {
        #[cfg(windows)]
        {
            let _ = private_boundary;
            Ok(Self(crate::service_census_windows::NativeRoot::begin(
                path,
            )?))
        }
        #[cfg(target_os = "linux")]
        {
            let file = super::census::open_absolute(path)?;
            if private_boundary {
                let proof = crate::private_storage::native_boundary(
                    &file,
                    crate::private_storage::Kind::RootDirectory,
                )?
                .context("native root privacy is not established")?;
                ensure!(proof.recheck()?, "native root boundary changed");
            }
            Ok(Self {
                file,
                path: path.into(),
                private_boundary,
            })
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            let _ = (path, private_boundary);
            anyhow::bail!("native root evidence unavailable")
        }
    }
    fn require_under(&self, boundary: [u64; 3]) -> Result<()> {
        #[cfg(windows)]
        {
            self.0.require_creation_chain(boundary)
        }
        #[cfg(target_os = "linux")]
        {
            ensure!(
                self.id()?[0] == boundary[0],
                "native directory crosses filesystem"
            );
            Ok(())
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            let _ = boundary;
            anyhow::bail!("native boundary unsupported")
        }
    }
    fn id(&self) -> Result<[u64; 3]> {
        #[cfg(windows)]
        {
            self.0.identity()
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::MetadataExt;
            // A retained fd alone survives rename/replacement. Every owning
            // checkpoint must also prove that the original path still names it.
            let current = super::census::open_absolute(&self.path)?;
            let m = self.file.metadata()?;
            let fresh = current.metadata()?;
            ensure!(
                m.is_dir() && fresh.is_dir() && m.dev() == fresh.dev() && m.ino() == fresh.ino(),
                "native root path identity changed"
            );
            if self.private_boundary {
                for file in [&self.file, &current] {
                    let proof = crate::private_storage::native_boundary(
                        file,
                        crate::private_storage::Kind::RootDirectory,
                    )?
                    .context("native root privacy changed")?;
                    ensure!(
                        proof.identity() == [m.dev(), m.ino(), 0] && proof.recheck()?,
                        "native root boundary changed"
                    );
                }
            }
            Ok([m.dev(), m.ino(), 0])
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            anyhow::bail!("native root evidence unavailable")
        }
    }
}
fn digest(domain: &[u8], value: &impl Serialize) -> Result<Sha256Digest> {
    Ok(airk_core::identity::framed_digest(
        domain,
        &[&serde_json::to_vec(value)?],
    ))
}
/// Acquired before entering SQL: lifecycle->persistent writer->manifest->SQL.
struct LockedPublication {
    _selection: super::super::ExistingSelection,
    _writer: crate::ExistingWriterExclusion,
    root: RootPin,
    selected: RootPin,
    head: CurrentHead,
}
impl LockedPublication {
    fn revalidate(&self) -> Result<()> {
        let root = self.root.id()?;
        self.selected.id()?;
        self.selected.require_under(root)
    }
}
impl IntegrityOwner<'_> {
    fn locked_native(&self) -> Result<LockedPublication> {
        self.locked_native_inner(false)
    }
    fn locked_native_inner(&self, recovery: bool) -> Result<LockedPublication> {
        self.check()?;
        let preliminary = self.collection.select_existing_locked()?;
        let directory = preliminary.store.directory().canonicalize()?;
        drop(preliminary);
        let store = FileStore::new(&directory);
        let writer = store.existing_writer_exclusion()?;
        let selection = self.collection.select_existing_locked()?;
        ensure!(
            selection.store.directory().canonicalize()? == directory,
            "native selection changed before resume"
        );
        let root = RootPin::open(Path::new(&self.collection.owner.data_root), true)
            .map_err(|error| error.context(NativeBoundaryUnavailable))?;
        let selected = RootPin::open(&directory, false)
            .map_err(|error| error.context(NativeBoundaryUnavailable))?;
        let root_id = root.id()?;
        let selected_id = selected.id()?;
        selected.require_under(root_id)?;
        ensure!(
            root_id[0] == selected_id[0],
            "native selected directory crosses volume boundary"
        );
        let head = selection.store.existing_settled_head(&writer)?;
        ensure!(
            head.scope.root == self.collection.owner.root
                && head.scope.collection == self.collection.owner.collection,
            "foreign native selected scope"
        );
        self.native_no_damage(&head, recovery)?;
        root.id()?;
        selected.id()?;
        selected.require_under(root_id)?;
        Ok(LockedPublication {
            _selection: selection,
            _writer: writer,
            root,
            selected,
            head,
        })
    }
    fn native_no_damage(&self, head: &CurrentHead, recovery: bool) -> Result<()> {
        let fence = self.check()?;
        ensure!(
            !self.bound.native_integrity_revoked(),
            "native owner live integrity revoked"
        );
        self.native_known_damage(head, fence.0)?;
        ensure!(
            recovery
                || !matches!(
                    self.registry.integrity_audit_state(self.lease)?,
                    IntegrityAuditState::Unresolved { .. }
                ),
            "native collection audit is unresolved"
        );
        Ok(())
    }
    fn native_known_damage(&self, head: &CurrentHead, namespace: Incarnation) -> Result<()> {
        if let Some(state) = self.load(namespace)? {
            ensure!(
                state.known_damage.len() < MAX_DAMAGE
                    && !state
                        .known_damage
                        .iter()
                        .any(|d| d.incarnation == head.scope.incarnation),
                "native collection has known damage"
            );
        }
        let damage = self.registry.recorded_integrity_damage(self.lease)?;
        ensure!(
            damage.len() < MAX_DAMAGE
                && !damage
                    .iter()
                    .any(|d| d.incarnation == head.scope.incarnation),
            "native collection has durable known damage"
        );
        Ok(())
    }
}
/// Owns the exact native worker and supported root handles. Unsupported native
/// boundaries keep strict readers without constructing this capability.
pub struct NativeOwnerSession {
    collection: Collection,
    bound: BoundDataRoot,
    lease: std::sync::Arc<ProjectLease>,
    identity: WorkerIdentity,
    active: String,
    _root: std::sync::Arc<RootPin>,
    live: std::sync::Arc<std::sync::atomic::AtomicBool>,
    damage_started: std::sync::Arc<std::sync::atomic::AtomicBool>,
    failed_incarnation: std::sync::Arc<std::sync::Mutex<Option<Incarnation>>>,
}
impl Collection {
    /// Explicit positive capability admission. No source/object/FTS audit occurs;
    /// absent legacy metadata is tagged Unknown under native current-user trust.
    /// Native callers must provide their actual admitted lease and worker identity.
    pub fn arm_native_owner(
        &self,
        registry: &mut ServiceRegistry,
        lease: std::sync::Arc<ProjectLease>,
        bound: &BoundDataRoot,
        identity: WorkerIdentity,
    ) -> Result<NativeOwnerSession> {
        let owner = self.integrity_owner(registry, &lease, bound)?;
        let fence = owner.check()?;
        ensure!(
            identity.service == fence.0
                && identity.project == lease.identity().0
                && identity.fencing_generation == fence.1,
            "foreign native worker admission"
        );
        let previous = registry.load_owner_session(&lease)?;
        if previous
            .as_ref()
            .is_some_and(|(_, old)| old.state != SessionState::Clean || old.policy != POLICY)
        {
            return Err(anyhow::Error::new(NativeOwnerRecoveryRequired));
        }
        let locked = owner.locked_native()?;
        let root = digest(b"native-root/1", &locked.root.id()?)?;
        let selected = digest(b"native-selected/1", &locked.selected.id()?)?;
        let token = locked.head.token()?;
        if let Some((_, old)) = &previous
            && !(old.state == SessionState::Clean
                && old.policy == POLICY
                && old.root == root
                && old.selected == selected
                && old.head == token)
        {
            return Err(anyhow::Error::new(NativeOwnerRecoveryRequired));
        }
        let next = OwnerSession {
            version: 1,
            namespace: fence.0,
            project: identity.project,
            worker: identity.worker,
            epoch: fence.1,
            nonce: identity.worker,
            incarnation: token.incarnation,
            root,
            selected,
            policy: POLICY,
            head: token,
            state: SessionState::Active,
            basis: previous
                .as_ref()
                .map_or(EnrollmentBasis::LegacyMetadataUnknown, |(_, old)| {
                    old.basis.clone()
                }),
        };
        // All filesystem exclusion has already been acquired. Recheck exact
        // live registration/bound state before the SQL-only conditional commit.
        ensure!(
            owner.check()? == fence,
            "native owner changed during admission"
        );
        locked.revalidate()?;
        registry.compare_owner_session(
            &lease,
            previous.as_ref().map(|(raw, _)| raw.as_str()),
            &next,
        )?;
        let active = serde_json::to_string(&next)?;
        let LockedPublication { root, .. } = locked;
        Ok(NativeOwnerSession {
            collection: self.clone(),
            bound: bound.clone(),
            lease,
            identity,
            active,
            _root: std::sync::Arc::new(root),
            live: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            damage_started: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            failed_incarnation: std::sync::Arc::new(std::sync::Mutex::new(None)),
        })
    }
}
impl Collection {
    /// Exceptional inherited Active/Clean mismatch resolution; declared physical
    /// coverage must match the exact selected head and fixed enabled-component profile.
    pub fn recover_native_owner(
        &self,
        registry: &mut ServiceRegistry,
        lease: &ProjectLease,
        bound: &BoundDataRoot,
        identity: WorkerIdentity,
        job: Sha256Digest,
        proof: &super::verify::EnabledComponentAuditProof,
    ) -> Result<()> {
        let owner = self.integrity_owner(registry, lease, bound)?;
        let fence = owner.check()?;
        ensure!(
            identity.service == fence.0
                && identity.project == lease.identity().0
                && identity.fencing_generation == fence.1,
            "foreign native recovery owner"
        );
        let locked = owner.locked_native_inner(true)?;
        ensure!(
            proof.epoch().worker == identity.worker
                && proof.epoch().worker_epoch == fence.1
                && proof.epoch().root == digest(b"native-root/1", &locked.root.id()?)?
                && proof.epoch().selected == digest(b"native-selected/1", &locked.selected.id()?)?
                && proof.exact_head() == &locked.head,
            "native recovery physical components changed"
        );
        let previous = registry
            .load_owner_session(lease)?
            .context("exceptional owner session missing")?;
        let token = locked.head.token()?;
        let next = OwnerSession {
            version: 1,
            namespace: fence.0,
            project: identity.project,
            worker: identity.worker,
            epoch: fence.1,
            nonce: identity.worker,
            incarnation: token.incarnation,
            root: digest(b"native-root/1", &locked.root.id()?)?,
            selected: digest(b"native-selected/1", &locked.selected.id()?)?,
            policy: POLICY,
            head: token,
            state: SessionState::Clean,
            basis: EnrollmentBasis::PhysicalVerify,
        };
        ensure!(owner.check()? == fence, "native recovery owner changed");
        locked.revalidate()?;
        registry.compare_recovered_owner_session(lease, &previous.0, &next, job, proof)?;
        drop(locked);
        Ok(())
    }
}

impl NativeOwnerSession {
    /// Private owning adapter only. The worker calls this after jobs and actual
    /// LocalTools admission shutdown succeed. This API is NOT a query receipt.
    pub fn settle_after_owned_drain(self, registry: &mut ServiceRegistry) -> Result<()> {
        ensure!(
            self.live.load(std::sync::atomic::Ordering::Acquire)
                && !self.bound.native_integrity_revoked(),
            "failed native query owner cannot settle clean"
        );
        let owner = self
            .collection
            .integrity_owner(registry, &self.lease, &self.bound)?;
        let fence = owner.check()?;
        ensure!(
            fence == (self.identity.service, self.identity.fencing_generation),
            "stale native drain epoch"
        );
        let locked = owner.locked_native()?;
        ensure!(
            self._root.id()? == locked.root.id()?,
            "native data root replaced during owned session"
        );
        let previous = registry
            .load_owner_session(&self.lease)?
            .context("native active session missing")?;
        ensure!(
            previous.0 == self.active && previous.1.state == SessionState::Active,
            "foreign native session drain"
        );
        let mut clean = previous.1;
        // Publications/rebuilds inside this exact Active owner are allowed.
        // Bind final current selection/head instead of freezing initial head.
        clean.root = digest(b"native-root/1", &locked.root.id()?)?;
        clean.selected = digest(b"native-selected/1", &locked.selected.id()?)?;
        clean.head = locked.head.token()?;
        clean.incarnation = clean.head.incarnation;
        clean.state = SessionState::Clean;
        clean.basis = EnrollmentBasis::OwnedDrain;
        ensure!(owner.check()? == fence, "native owner changed during drain");
        locked.revalidate()?;
        // The owning caller has joined all accepted work and closed read admission.
        // Recheck after filesystem/registry waits so an observed failure never
        // converts the durable crash-recovery marker into Clean.
        ensure!(
            self.live.load(std::sync::atomic::Ordering::Acquire)
                && !self.bound.native_integrity_revoked(),
            "native query owner failed during drain"
        );
        registry.compare_owner_session(&self.lease, Some(&self.active), &clean)?;
        drop(locked);
        Ok(())
    }
}

impl Collection {
    pub fn begin_native_recovery(
        &self,
        registry: &ServiceRegistry,
        lease: &ProjectLease,
        bound: &BoundDataRoot,
        identity: WorkerIdentity,
        job: Sha256Digest,
    ) -> Result<super::verify::RecoveryEpoch> {
        let owner = self.integrity_owner(registry, lease, bound)?;
        let fence = owner.check()?;
        ensure!(
            identity.service == fence.0
                && identity.project == lease.identity().0
                && identity.fencing_generation == fence.1,
            "foreign native recovery owner"
        );
        let locked = owner.locked_native_inner(true)?;
        let previous = registry
            .load_owner_session(lease)?
            .context("exceptional owner session missing")?;
        let context = super::verify::RecoveryContext {
            namespace: fence.0,
            project: identity.project,
            worker: identity.worker,
            worker_epoch: fence.1,
            job,
            inherited: digest(b"native-recovery-owner/1", &previous.0)?,
            head: locked.head.token()?,
            root: digest(b"native-root/1", &locked.root.id()?)?,
            selected: digest(b"native-selected/1", &locked.selected.id()?)?,
            profile: super::verify::NATIVE_RECOVERY_PROFILE,
        };
        registry.require_recovery_epoch(lease, &context)?;
        locked.revalidate()?;
        Ok(super::verify::RecoveryEpoch { context })
    }
}

/// An inherited session or changed policy/head needs explicit physical recovery.
#[derive(Debug)]
pub struct NativeOwnerRecoveryRequired;
impl std::fmt::Display for NativeOwnerRecoveryRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native owner requires exceptional physical verification")
    }
}
impl std::error::Error for NativeOwnerRecoveryRequired {}

/// A lifecycle transition requires strict reads; it is not evidence damage and
/// must not poison the owning session when it interrupts a selected-page read.
#[derive(Debug)]
struct PrivateMaintenanceUnavailable;
impl std::fmt::Display for PrivateMaintenanceUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("private query maintenance was stopped, paused, or blocked")
    }
}
impl std::error::Error for PrivateMaintenanceUnavailable {}

/// Request-scoped, nonserializable authority minted only by an active native
/// owner. It never owns a database snapshot and cannot outlive that owner.
pub(crate) struct PrivateQueryAuthority {
    collection: Collection,
    bound: BoundDataRoot,
    lease: std::sync::Arc<ProjectLease>,
    identity: WorkerIdentity,
    active: String,
    root: std::sync::Arc<RootPin>,
    selected: RootPin,
    directory: PathBuf,
    head: crate::current::CurrentToken,
    live: std::sync::Arc<std::sync::atomic::AtomicBool>,
    damage_started: std::sync::Arc<std::sync::atomic::AtomicBool>,
    failed_incarnation: std::sync::Arc<std::sync::Mutex<Option<Incarnation>>>,
}
impl PrivateQueryAuthority {
    pub(crate) fn recheck(&self, read: &CurrentRead) -> Result<()> {
        use std::sync::atomic::Ordering;
        ensure!(
            self.live.load(Ordering::Acquire),
            "native query owner has drained or failed"
        );
        ensure!(
            !self.bound.native_integrity_revoked(),
            "native query integrity revoked"
        );
        ensure!(
            read.head().token()? == self.head
                && read.native_directory().canonicalize()? == self.directory,
            "foreign private request publication"
        );
        let root = self.root.id()?;
        self.selected.id()?;
        self.selected.require_under(root)?;
        let registry = ServiceRegistry::open_saved(self.bound.native_control_root())?;
        let owner = self
            .collection
            .integrity_owner(&registry, &self.lease, &self.bound)?;
        ensure!(
            owner.check()? == (self.identity.service, self.identity.fencing_generation),
            "private query worker epoch changed"
        );
        let active = registry
            .load_owner_session(&self.lease)?
            .context("native query owner missing")?;
        ensure!(
            active.0 == self.active
                && active.1.state == SessionState::Active
                && active.1.policy == POLICY,
            "private query owner changed"
        );
        owner.native_no_damage(read.head(), false)?;
        if !registry.private_query_maintenance_enabled(&self.lease)? {
            return Err(anyhow::Error::new(PrivateMaintenanceUnavailable));
        }
        let selection = self.collection.select_existing_locked()?;
        ensure!(
            selection.store.directory().canonicalize()? == self.directory,
            "private query selected incarnation changed"
        );
        Ok(())
    }
    pub(crate) fn observe_failure(&self, read: &CurrentRead, error: &anyhow::Error) -> Result<()> {
        // Contention is not uncertain evidence and must not poison a healthy owner.
        if error
            .downcast_ref::<crate::current::CurrentReadContention>()
            .is_some()
            || error
                .downcast_ref::<crate::current::CurrentOpenContention>()
                .is_some()
            || error
                .downcast_ref::<crate::current::CurrentRequestExpired>()
                .is_some()
            || error
                .downcast_ref::<crate::service::collection::SelectionContention>()
                .is_some()
            || error
                .downcast_ref::<PrivateMaintenanceUnavailable>()
                .is_some()
        {
            return Ok(());
        }
        self.live.store(false, std::sync::atomic::Ordering::Release);
        // A poisoned failure identity can only refuse later retirement; do not
        // let it prevent the independent durable damage attempts below.
        if let Ok(mut failed) = self.failed_incarnation.lock() {
            failed.get_or_insert(read.head().scope.incarnation);
        }
        if error
            .downcast_ref::<crate::current::PrivateIntegrityFailure>()
            .is_some()
            && !self
                .damage_started
                .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            // The live veto precedes both independent persistence attempts.
            self.bound.revoke_native_integrity();
            // A failed writable open must not suppress the independent local
            // receipt attempt. Saved read-only fencing can still establish owner
            // identity; its control write then truthfully fails in record_damage.
            let registry = ServiceRegistry::open_existing_for_integrity_damage(
                self.bound.native_control_root(),
            )
            .or_else(|_| ServiceRegistry::open_saved(self.bound.native_control_root()))?;
            let owner = self
                .collection
                .integrity_owner(&registry, &self.lease, &self.bound)?;
            let component =
                airk_core::identity::framed_digest(b"private-query-integrity-failure/1", &[]);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis();
            owner.record_damage(read, component, u64::try_from(now)?)?;
        }
        // Other errors keep durable Active. They never become known corruption.
        Ok(())
    }
}
impl NativeOwnerSession {
    /// Failure retirement never writes Clean and never grants relaxed reads.
    pub fn failed(&self) -> bool {
        !self.live.load(std::sync::atomic::Ordering::Acquire)
            || self.bound.native_integrity_revoked()
    }

    /// Retire only the failed in-memory capability whose inherited Active row
    /// was actually replaced by this sealed physical recovery proof. This does
    /// not reset its live flag or the shared known-damage veto.
    pub fn require_recovered_owner(
        &self,
        registry: &ServiceRegistry,
        proof: &super::verify::EnabledComponentAuditProof,
    ) -> Result<()> {
        ensure!(
            self.failed() && !self.bound.native_integrity_revoked(),
            "known-damage or healthy native owner cannot be retired by uncertain-read recovery"
        );
        let owner = self
            .collection
            .integrity_owner(registry, &self.lease, &self.bound)?;
        let fence = owner.check()?;
        ensure!(
            fence == (self.identity.service, self.identity.fencing_generation)
                && proof.epoch().worker == self.identity.worker
                && proof.epoch().worker_epoch == self.identity.fencing_generation
                && proof.epoch().inherited == digest(b"native-recovery-owner/1", &self.active)?,
            "physical recovery belongs to another failed native owner"
        );
        let locked = owner.locked_native()?;
        ensure!(
            self._root.id()? == locked.root.id()?
                && proof.exact_head() == &locked.head
                && proof.epoch().root == digest(b"native-root/1", &locked.root.id()?)?
                && proof.epoch().selected == digest(b"native-selected/1", &locked.selected.id()?)?,
            "physical recovery publication or root changed before capability retirement"
        );
        let recovered = registry
            .load_owner_session(&self.lease)?
            .context("recovered native owner missing")?
            .1;
        ensure!(
            recovered.state == SessionState::Clean
                && recovered.policy == POLICY
                && recovered.worker == self.identity.worker
                && recovered.epoch == fence.1
                && recovered.head == locked.head.token()?
                && recovered.basis == EnrollmentBasis::PhysicalVerify,
            "physical recovery did not commit matching Clean owner state"
        );
        locked.revalidate()?;
        Ok(())
    }

    /// An explicitly accepted replacement may restore strict reads in the same
    /// worker. The failed capability is retired only after its exact failed
    /// incarnation has been replaced by the owned committed rebuild receipt.
    pub fn require_owned_replacement(
        &self,
        replaced: Incarnation,
        token: crate::current::CurrentToken,
    ) -> Result<()> {
        ensure!(
            self.failed(),
            "healthy native owner does not require retirement"
        );
        let failed = self
            .failed_incarnation
            .lock()
            .map_err(|_| anyhow::anyhow!("failed native incarnation unavailable"))?;
        ensure!(
            replaced != token.incarnation
                && failed.is_none_or(|failed| failed != token.incarnation),
            "replacement retained the failed incarnation"
        );
        ensure!(
            self.collection
                .manifest()?
                .retired
                .iter()
                .any(|old| old.incarnation == Some(replaced)),
            "replacement did not retire the actual prior incarnation"
        );
        drop(failed);
        self._root.id()?;
        let registry = ServiceRegistry::open_saved(self.bound.native_control_root())?;
        let owner = self
            .collection
            .integrity_owner(&registry, &self.lease, &self.bound)?;
        ensure!(
            owner.check()? == (self.identity.service, self.identity.fencing_generation),
            "replacement worker epoch changed"
        );
        let receipt = self
            .collection
            .last_rebuild_receipt()?
            .context("owned replacement receipt missing")?;
        ensure!(
            receipt.token == token,
            "replacement receipt differs from committed head"
        );
        let job = registry.job(receipt.job)?;
        ensure!(
            job.project == self.identity.project
                && matches!(
                    job.kind,
                    airk_core::service::JobKind::Rebuild | airk_core::service::JobKind::Repair
                )
                && matches!(
                    job.state,
                    airk_core::service::JobState::Running | airk_core::service::JobState::Succeeded
                )
                && job.fencing_generation == self.identity.fencing_generation,
            "replacement lacks actual accepted native job"
        );
        let selection = self.collection.select_existing_locked()?;
        let scope = selection
            .store
            .current_owned_scope()?
            .context("replacement scope missing")?;
        ensure!(
            scope.incarnation == token.incarnation,
            "replacement selection changed"
        );
        let read = selection.store.current_read(&scope)?;
        ensure!(
            read.token()? == token,
            "replacement publication changed before retirement"
        );
        owner.native_known_damage(read.head(), self.identity.service)?;
        let active = registry
            .load_owner_session(&self.lease)?
            .context("failed native owner missing")?;
        ensure!(
            active.0 == self.active && active.1.state == SessionState::Active,
            "failed native owner changed before retirement"
        );
        // No session transition, damage deletion, audit-veto clearance, or new
        // private authority is performed. New readers start from strict APIs.
        Ok(())
    }

    /// Attach before handing the exact fresh request reader to consumers.
    pub fn attach_read(
        &self,
        read: std::sync::Arc<CurrentRead>,
    ) -> Result<std::sync::Arc<CurrentRead>> {
        ensure!(
            read.private_query.get().is_none(),
            "private reader already attached"
        );
        ensure!(
            self.live.load(std::sync::atomic::Ordering::Acquire)
                && !self.bound.native_integrity_revoked(),
            "failed native query owner cannot admit a reader"
        );
        let registry = ServiceRegistry::open_saved(self.bound.native_control_root())?;
        let owner = self
            .collection
            .integrity_owner(&registry, &self.lease, &self.bound)?;
        owner.native_no_damage(read.head(), false)?;
        if !registry.private_query_maintenance_enabled(&self.lease)? {
            return Ok(read);
        }
        // A prepared/applying snapshot stays strict until its owning writer settles.
        if read.settled_native_head()?.is_none() {
            return Ok(read);
        }
        let directory = read.native_directory().canonicalize()?;
        let selected = RootPin::open(&directory, false)?;
        let authority = std::sync::Arc::new(PrivateQueryAuthority {
            collection: self.collection.clone(),
            bound: self.bound.clone(),
            lease: self.lease.clone(),
            identity: self.identity,
            active: self.active.clone(),
            root: self._root.clone(),
            selected,
            directory,
            head: read.head().token()?,
            live: self.live.clone(),
            damage_started: self.damage_started.clone(),
            failed_incarnation: self.failed_incarnation.clone(),
        });
        authority.recheck(&read)?;
        read.private_query
            .set(authority)
            .map_err(|_| anyhow::anyhow!("private reader already attached"))?;
        Ok(read)
    }
}
impl Drop for NativeOwnerSession {
    fn drop(&mut self) {
        self.live.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// Platform boundary unavailable; callers may retain all strict read checks.
#[derive(Debug)]
pub struct NativeBoundaryUnavailable;
impl std::fmt::Display for NativeBoundaryUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native private boundary is unavailable; strict reads required")
    }
}
impl std::error::Error for NativeBoundaryUnavailable {}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    // Native-created storage under actual protected ancestry; no positive chmod
    // fixture. The negative mutation below changes only this disposable tree.
    fn native_root() -> Result<tempfile::TempDir> {
        let base = std::env::var_os("AIRK_PRIVATE_CENSUS_BASE")
            .context("protected native qualification base required")?;
        let root = tempfile::Builder::new()
            .prefix("airk-native-pin-")
            .tempdir_in(base)?;
        let control = root.path().join("control");
        fs::create_dir(&control)?;
        crate::service::data_scope::bind(
            &root.path().join("data"),
            &control,
            Incarnation::from_bytes([143; 16]),
        )?;
        Ok(root)
    }

    #[test]
    #[ignore = "requires AIRK_PRIVATE_CENSUS_BASE under protected local user ancestry"]
    fn native_root_pin_rejects_path_replacement_and_selected_directory_replacement() -> Result<()> {
        let fixture = native_root()?;
        let data = fixture.path().join("data");
        let pin = RootPin::open(&data, true)?;
        let selected = data.join("selected");
        fs::create_dir(&selected)?;
        let selection = RootPin::open(&selected, false)?;
        fs::rename(&selected, data.join("old-selected"))?;
        fs::create_dir(&selected)?;
        assert!(selection.id().is_err());
        fs::rename(&data, fixture.path().join("old-data"))?;
        fs::create_dir(&data)?;
        assert!(pin.id().is_err());
        Ok(())
    }

    #[test]
    #[ignore = "requires AIRK_PRIVATE_CENSUS_BASE under protected local user ancestry"]
    fn native_root_pin_rechecks_current_inode_privacy() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let fixture = native_root()?;
        let data = fixture.path().join("data");
        let pin = RootPin::open(&data, true)?;
        pin.id()?;
        fs::set_permissions(&data, fs::Permissions::from_mode(0o750))?;
        assert!(pin.id().is_err());
        Ok(())
    }
}
