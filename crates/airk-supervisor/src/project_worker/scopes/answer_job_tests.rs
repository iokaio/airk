// SPDX-License-Identifier: Apache-2.0
//! A service-owned Answer job's read authority against its maker's session
//! (final plan section 12.4.2, step L2c; review of 24184339, finding H1).
//! These use the real `Scopes` ledger and real scoped kernels over a small
//! index: `detach` is exactly what `SessionFence::drop` does when a query
//! connection ends.
use super::*;
use airk_core::{index_catalog::*, scoped_reads::SourceDescriptor};

struct Fixture {
    kernel: Kernel,
    identity: WorkerIdentity,
    authority: ScopedReadAuthority,
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
    let authority = ScopedReadAuthority {
        schema: 1,
        provenance: airk_core::grant_provenance::GrantProvenance::UserReviewedProjectGrant,
        backing: project,
        index_id,
        project_ids: vec![project_id],
        grant: ReviewedGrant {
            id: GrantId::from_identity(Incarnation::from_bytes([7; 16])),
            host: Sha256Digest::from_bytes([8; 32]),
            revision: CatalogRevision::INITIAL,
            revoked: false,
            memberships: vec![GrantedMembership {
                membership: Membership {
                    project_id,
                    index_id,
                    revision: CatalogRevision::INITIAL,
                    scope: vec![ScopeAtom {
                        source_root_id: source,
                        kind: PathScope::Subtree,
                        path: "one".into(),
                    }],
                },
                source_revision: CatalogRevision::INITIAL,
                policy_epoch: 0,
                policy_digest: None,
            }],
        },
        sources: vec![SourceDescriptor {
            source_root_id: source,
            original_root: kernel.root().to_string_lossy().into_owned(),
        }],
    };
    Ok(Fixture {
        kernel,
        identity,
        authority,
        _owner: owner,
    })
}

fn lifetime() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

/// H1: at 3134e544 the job's kernel was the maker's session kernel (a
/// clone), so the session's teardown revoked the job's reads; this test then
/// failed (`l2c-h1-repro`). The job now has its own kernel, registered with
/// the grant's installed lease.
#[test]
fn an_answer_job_reads_on_after_its_makers_session_ends() -> Result<()> {
    let f = fixture()?;
    let mut scopes = Scopes::new(f.identity, true);
    let scope = scopes.install_lease(f.authority.clone(), 1, lifetime(), None)?;
    let session = Incarnation::from_bytes([17; 16]);
    let maker = f.kernel.with_scoped_reads(f.authority.clone())?;
    scopes.attach(scope, session, maker.clone())?;
    let job = maker.answer_job_kernel()?;
    let id = scopes.attach_job(scope, job.clone())?;
    assert!(job.file_info("one/file.rs").is_ok());
    // The maker's connection ends: `SessionFence::drop`.
    scopes.detach(Some(scope), session);
    assert!(maker.file_info("one/file.rs").is_err());
    assert!(
        job.file_info("one/file.rs").is_ok(),
        "H1: the maker's session teardown revoked the job's reads"
    );
    // A revoked maker cannot make a job any more.
    assert!(maker.answer_job_kernel().is_err());
    // The job ends: its kernel is removed and revoked.
    scopes.detach_job(scope, id);
    assert!(job.file_info("one/file.rs").is_err());
    Ok(())
}

/// What fences the grant fences its jobs: a revocation, a newer grant
/// revision, the lease's expiry and renewal, and the owner's close. A
/// revoked grant also cannot redeem a new session, so nothing more can be
/// delivered under it.
#[test]
fn an_answer_job_ends_with_its_grant_its_lease_and_its_owner() -> Result<()> {
    let f = fixture()?;
    let fresh_job = |scopes: &mut Scopes, scope| -> Result<Kernel> {
        let maker = f.kernel.with_scoped_reads(f.authority.clone())?;
        let job = maker.answer_job_kernel()?;
        scopes.attach_job(scope, job.clone())?;
        assert!(job.file_info("one/file.rs").is_ok());
        Ok(job)
    };
    // Revocation.
    let mut scopes = Scopes::new(f.identity, true);
    let scope = scopes.install_lease(f.authority.clone(), 1, lifetime(), None)?;
    let job = fresh_job(&mut scopes, scope)?;
    scopes.revoke(f.identity, scope)?;
    assert!(job.file_info("one/file.rs").is_err());
    assert!(scopes.check(Some(scope)).is_err());
    assert!(
        scopes
            .attach_job(scope, f.kernel.with_scoped_reads(f.authority.clone())?)
            .is_err()
    );
    // A newer grant revision.
    let mut scopes = Scopes::new(f.identity, true);
    let scope = scopes.install_lease(f.authority.clone(), 1, lifetime(), None)?;
    let job = fresh_job(&mut scopes, scope)?;
    let mut newer = f.authority.clone();
    newer.grant.revision = newer.grant.revision.next()?;
    scopes.install_lease(newer, 2, lifetime(), None)?;
    assert!(job.file_info("one/file.rs").is_err());
    // Lease expiry, also for a job still queued: its kernel is revoked, so
    // its dispatch is refused for that invocation alone.
    let mut scopes = Scopes::new(f.identity, true);
    let scope = scopes.install_lease(f.authority.clone(), 1, lifetime(), None)?;
    let job = fresh_job(&mut scopes, scope)?;
    for entry in scopes.installed.values_mut() {
        entry.expires = Instant::now() - Duration::from_secs(1);
    }
    scopes.expire();
    assert!(job.scoped_read_fingerprint().is_err());
    // Renewal of the expired lease fences the superseded lease's job too.
    let mut scopes = Scopes::new(f.identity, true);
    let scope = scopes.install_lease(f.authority.clone(), 1, lifetime(), None)?;
    let job = fresh_job(&mut scopes, scope)?;
    for entry in scopes.installed.values_mut() {
        entry.expires = Instant::now() - Duration::from_secs(1);
    }
    scopes.install_lease(f.authority.clone(), 2, lifetime(), Some(scope))?;
    assert!(job.file_info("one/file.rs").is_err());
    // The owner's close.
    let mut scopes = Scopes::new(f.identity, true);
    let scope = scopes.install_lease(f.authority.clone(), 1, lifetime(), None)?;
    let job = fresh_job(&mut scopes, scope)?;
    scopes.close();
    assert!(job.file_info("one/file.rs").is_err());
    // At most `MAX_JOBS` a lease.
    let mut scopes = Scopes::new(f.identity, true);
    let scope = scopes.install_lease(f.authority.clone(), 1, lifetime(), None)?;
    for _ in 0..MAX_JOBS {
        scopes.attach_job(scope, f.kernel.with_scoped_reads(f.authority.clone())?)?;
    }
    let over = f.kernel.with_scoped_reads(f.authority.clone())?;
    assert!(scopes.attach_job(scope, over.clone()).is_err());
    assert!(over.file_info("one/file.rs").is_err());
    Ok(())
}