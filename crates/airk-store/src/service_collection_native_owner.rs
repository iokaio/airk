// SPDX-License-Identifier: Apache-2.0
//! Bounded native lifecycle enrollment only; ordinary readers remain strict.
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
        let root = RootPin::open(Path::new(&self.collection.owner.data_root), true)?;
        let selected = RootPin::open(&directory, false)?;
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
        if let Some(state) = self.load(fence.0)? {
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
}
/// Owns exact native worker and supported root handles, never a census/read bypass.
/// No instance is installed by normal strict worker construction.
pub struct NativeOwnerSession {
    collection: Collection,
    bound: BoundDataRoot,
    lease: std::sync::Arc<ProjectLease>,
    identity: WorkerIdentity,
    active: String,
    _root: RootPin,
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
        let locked = owner.locked_native()?;
        let root = digest(b"native-root/1", &locked.root.id()?)?;
        let selected = digest(b"native-selected/1", &locked.selected.id()?)?;
        let token = locked.head.token()?;
        let previous = registry.load_owner_session(&lease)?;
        if let Some((_, old)) = &previous {
            ensure!(
                old.state == SessionState::Clean
                    && old.policy == POLICY
                    && old.root == root
                    && old.selected == selected
                    && old.head == token,
                "native owner requires exceptional physical verification"
            );
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
            _root: root,
        })
    }
}
impl Collection {
    /// Exceptional inherited Active/Clean mismatch resolution; declared physical
    /// coverage must match the exact selected head. F13 must extend this proof
    /// predicate before projection fields can ever use relaxed reads.
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
