// SPDX-License-Identifier: Apache-2.0
//! A service-owned Answer job's read authority against its maker's session
//! (final plan section 12.4.2, step L2c; review of 24184339, finding H1).
//! After grant removal (local trust plan phase B) a job holds the read
//! authority its caller redeemed: a W4/A2 selection or a U105 open read,
//! installed in the selection ledger. These use the real `Scopes` ledger and
//! real scoped kernels over a small index: `detach` is exactly what
//! `SessionFence::drop` does when a query connection ends.
use super::*;
use airk_core::{
    index_catalog::*,
    scoped_reads::{SelectionOwnerKind, SelectionScopeRef, SourceDescriptor},
};

struct Fixture {
    kernel: Kernel,
    identity: WorkerIdentity,
    selection: SelectionReadAuthority,
    open: OpenReadAuthority,
    _owner: tempfile::TempDir,
}

fn fixture() -> Result<Fixture> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("source");
    std::fs::create_dir_all(root.join("one"))?;
    std::fs::write(root.join("one/file.rs"), "pub struct ScopedMarker;\n")?;
    let kernel = Kernel::open(&root, Some(owner.path().join("data")))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let project = airk_core::service::ProjectRegistration::new(
        kernel.root().to_string_lossy().into_owned(),
        kernel.data_dir().to_string_lossy().into_owned(),
        kernel.collection().into(),
    )?
    .binding;
    let identity = WorkerIdentity {
        service: Incarnation::from_bytes([1; 16]),
        worker: Incarnation::from_bytes([2; 16]),
        project,
        fencing_generation: 1,
    };
    let index_id = IndexId::from_identity(Incarnation::from_bytes([4; 16]));
    let project_id = QueryProjectId::from_identity(Incarnation::from_bytes([5; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([6; 16]));
    let sources = vec![SourceDescriptor {
        source_root_id: source,
        original_root: kernel.root().to_string_lossy().into_owned(),
    }];
    let reference = |owner, nonce| SelectionScopeRef {
        owner,
        grant_nonce: Incarnation::from_bytes([nonce; 16]),
        service_instance: identity.service,
        scope_identity: Sha256Digest::from_bytes([0x33; 32]),
        catalog_revision: CatalogRevision::INITIAL,
    };
    let selection = SelectionReadAuthority {
        schema: 1,
        backing: project,
        index_id,
        project_ids: vec![project_id],
        selection: reference(SelectionOwnerKind::Window, 0x5c),
        atoms: vec![(
            project_id,
            ScopeAtom {
                source_root_id: source,
                kind: PathScope::Subtree,
                path: "one".into(),
            },
        )],
        sources: sources.clone(),
        membership_revisions: vec![(project_id, CatalogRevision::INITIAL)],
        source_revision: CatalogRevision::INITIAL,
        policy_epoch: 0,
        policy_digest: None,
    };
    let open = OpenReadAuthority {
        schema: 1,
        backing: project,
        index_id,
        selection: reference(SelectionOwnerKind::Agent, 0x6a),
        sources,
        source_revision: CatalogRevision::INITIAL,
        policy_epoch: 0,
        policy_digest: None,
    };
    Ok(Fixture {
        kernel,
        identity,
        selection,
        open,
        _owner: owner,
    })
}

fn lifetime() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

/// The two kinds of read authority a job's maker can hold, each installed in
/// the selection ledger and read through its own scoped kernel.
#[derive(Clone, Copy, Debug)]
enum Kind {
    Selection,
    Open,
}
const KINDS: [Kind; 2] = [Kind::Selection, Kind::Open];

impl Fixture {
    fn install(&self, scopes: &mut Scopes, kind: Kind, generation: u64) -> Result<ScopedReadRef> {
        self.install_after(scopes, kind, generation, None)
    }
    fn install_after(
        &self,
        scopes: &mut Scopes,
        kind: Kind,
        generation: u64,
        previous: Option<ScopedReadRef>,
    ) -> Result<ScopedReadRef> {
        let authority = match kind {
            Kind::Selection => WorkerAuthority::Selection(self.selection.clone()),
            Kind::Open => WorkerAuthority::Open(self.open.clone()),
        };
        scopes.install_ledgered_lease(authority, generation, lifetime(), previous)
    }
    fn maker(&self, kind: Kind) -> Result<Kernel> {
        match kind {
            Kind::Selection => self.kernel.with_selection_reads(self.selection.clone()),
            Kind::Open => self.kernel.with_open_reads(self.open.clone()),
        }
    }
}

/// H1: at 3134e544 the job's kernel was the maker's session kernel (a
/// clone), so the session's teardown revoked the job's reads; this test then
/// failed (`l2c-h1-repro`). The job now has its own kernel, registered on the
/// lease of the selection or open read its maker redeemed.
#[test]
fn an_answer_job_reads_on_after_its_makers_session_ends() -> Result<()> {
    let f = fixture()?;
    for kind in KINDS {
        let mut scopes = Scopes::new(f.identity, true);
        let scope = f.install(&mut scopes, kind, 1)?;
        let session = Incarnation::from_bytes([17; 16]);
        let maker = f.maker(kind)?;
        scopes.attach(scope, session, maker.clone())?;
        let job = maker.answer_job_kernel()?;
        let id = scopes.attach_job(scope, job.clone())?;
        assert!(job.file_info("one/file.rs").is_ok(), "{kind:?}");
        // The job holds its maker's authority, not a wider one.
        assert_eq!(
            job.scoped_read_fingerprint()?,
            maker.scoped_read_fingerprint()?,
            "{kind:?}"
        );
        // The maker's connection ends: `SessionFence::drop`.
        scopes.detach(Some(scope), session);
        assert!(maker.file_info("one/file.rs").is_err(), "{kind:?}");
        assert!(
            job.file_info("one/file.rs").is_ok(),
            "H1 ({kind:?}): the maker's session teardown revoked the job's reads"
        );
        // A revoked maker cannot make a job any more.
        assert!(maker.answer_job_kernel().is_err(), "{kind:?}");
        // The job ends: its kernel is removed and revoked.
        scopes.detach_job(scope, id);
        assert!(job.file_info("one/file.rs").is_err(), "{kind:?}");
    }
    Ok(())
}

/// What fences the selection or open read fences its jobs: its revocation
/// (`RevokeScope`, sent when the window, Admin or agent selection that owns
/// it ends), the lease's expiry and renewal, and the owner's close. A revoked
/// selection also cannot redeem a new session or take a new job, so nothing
/// more can be delivered under it.
#[test]
fn an_answer_job_ends_with_its_selection_its_lease_and_its_owner() -> Result<()> {
    let f = fixture()?;
    let fresh_job = |scopes: &mut Scopes, scope, kind| -> Result<Kernel> {
        let job = f.maker(kind)?.answer_job_kernel()?;
        scopes.attach_job(scope, job.clone())?;
        assert!(job.file_info("one/file.rs").is_ok(), "{kind:?}");
        Ok(job)
    };
    for kind in KINDS {
        // Revocation of the selection.
        let mut scopes = Scopes::new(f.identity, true);
        let scope = f.install(&mut scopes, kind, 1)?;
        let job = fresh_job(&mut scopes, scope, kind)?;
        scopes.revoke(f.identity, scope)?;
        assert!(job.file_info("one/file.rs").is_err(), "{kind:?}");
        assert!(scopes.check(Some(scope)).is_err(), "{kind:?}");
        let late = f.maker(kind)?;
        assert!(scopes.attach_job(scope, late.clone()).is_err(), "{kind:?}");
        assert!(late.file_info("one/file.rs").is_err(), "{kind:?}");
        // Lease expiry, also for a job still queued: its kernel is revoked,
        // so its dispatch is refused for that invocation alone.
        let mut scopes = Scopes::new(f.identity, true);
        let scope = f.install(&mut scopes, kind, 1)?;
        let job = fresh_job(&mut scopes, scope, kind)?;
        for entry in scopes.selections.values_mut() {
            entry.expires = Instant::now() - Duration::from_secs(1);
        }
        scopes.expire();
        assert!(job.scoped_read_fingerprint().is_err(), "{kind:?}");
        // Renewal of the expired lease fences the superseded lease's job too.
        let mut scopes = Scopes::new(f.identity, true);
        let scope = f.install(&mut scopes, kind, 1)?;
        let job = fresh_job(&mut scopes, scope, kind)?;
        for entry in scopes.selections.values_mut() {
            entry.expires = Instant::now() - Duration::from_secs(1);
        }
        let renewed = f.install_after(&mut scopes, kind, 2, Some(scope))?;
        assert!(job.file_info("one/file.rs").is_err(), "{kind:?}");
        // The superseded lease takes no new job; the renewed one does.
        assert!(
            scopes.attach_job(scope, f.maker(kind)?).is_err(),
            "{kind:?}"
        );
        fresh_job(&mut scopes, renewed, kind)?;
        // The owner's close.
        let mut scopes = Scopes::new(f.identity, true);
        let scope = f.install(&mut scopes, kind, 1)?;
        let job = fresh_job(&mut scopes, scope, kind)?;
        scopes.close();
        assert!(job.file_info("one/file.rs").is_err(), "{kind:?}");
        // At most `MAX_JOBS` a lease.
        let mut scopes = Scopes::new(f.identity, true);
        let scope = f.install(&mut scopes, kind, 1)?;
        for _ in 0..MAX_JOBS {
            scopes.attach_job(scope, f.maker(kind)?)?;
        }
        let over = f.maker(kind)?;
        assert!(scopes.attach_job(scope, over.clone()).is_err(), "{kind:?}");
        assert!(over.file_info("one/file.rs").is_err(), "{kind:?}");
    }
    Ok(())
}

/// One selection's revocation ends only its own jobs: a job under another
/// live selection of the same index reads on.
#[test]
fn a_revoked_selection_ends_only_its_own_jobs() -> Result<()> {
    let f = fixture()?;
    let mut scopes = Scopes::new(f.identity, true);
    let selection = f.install(&mut scopes, Kind::Selection, 1)?;
    let open = f.install(&mut scopes, Kind::Open, 2)?;
    let selection_job = f.maker(Kind::Selection)?.answer_job_kernel()?;
    let open_job = f.maker(Kind::Open)?.answer_job_kernel()?;
    scopes.attach_job(selection, selection_job.clone())?;
    scopes.attach_job(open, open_job.clone())?;
    scopes.revoke(f.identity, selection)?;
    assert!(selection_job.file_info("one/file.rs").is_err());
    assert!(open_job.file_info("one/file.rs").is_ok());
    Ok(())
}
