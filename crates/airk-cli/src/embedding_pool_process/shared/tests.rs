// SPDX-License-Identifier: Apache-2.0
//! Real native object observations plus isolated Genesis catalog fixtures.
//! These do not qualify managed activation or the guarded child.
use super::*;
use airk_store::service::ServiceLease;
#[cfg(unix)]
use airk_store::service::registry::{ProviderBoot, SharedHelperContainment};
mod native;

const OLD: Incarnation = Incarnation::from_bytes([31; 16]);
const CURRENT: Incarnation = Incarnation::from_bytes([32; 16]);
const HELPER: Incarnation = Incarnation::from_bytes([33; 16]);

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    lease: ServiceLease,
}
impl Fixture {
    fn new() -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("runtime");
        crate::local_ipc::prepare_root(&root)?;
        let root = root.canonicalize()?;
        drop(crate::catalog_test_fixture::authority_one(&root, OLD)?);
        let lease = ServiceLease::acquire(&root)?;
        registry(&root)?.record_service_instance(&lease, OLD)?;
        Ok(Self {
            _directory: directory,
            root,
            lease,
        })
    }
    fn restart(&self) -> Result<()> {
        registry(&self.root)?.record_service_instance(&self.lease, CURRENT)
    }
}

#[test]
fn shared_recovery_requires_positive_coverage_and_exact_admission() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("legacy-runtime");
    crate::local_ipc::prepare_root(&root)?;
    drop(ServiceRegistry::open(&root, OLD)?);
    assert!(!crate::embedding_pool_bootstrap::shared_mode(&root)?);
    assert!(recover_predecessor_shared_helpers(&root, CURRENT).is_err());
    let fixture = Fixture::new()?;
    fixture.restart()?;
    assert!(
        recover_shared_helper(
            &fixture.root,
            SharedHelperIdentity {
                service: OLD,
                helper: HELPER
            }
        )
        .is_err()
    );
    let proof = recover_predecessor_shared_helpers(&fixture.root, CURRENT)?
        .context("positive empty coverage")?;
    assert!(proof.matches(registry(&fixture.root)?.incarnation()?, CURRENT));
    assert!(proof.witnesses().is_empty());
    assert!(!proof.matches(Incarnation::from_bytes([99; 16]), CURRENT));
    Ok(())
}

#[test]
fn maintenance_keeps_legacy_identity_and_refuses_inactive_shared_coverage() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("legacy");
    crate::local_ipc::prepare_root(&root)?;
    drop(ServiceRegistry::open(&root, OLD)?);
    let lease = ServiceLease::acquire(&root)?;
    assert!(quiesce_for_maintenance(&lease)?);
    assert!(
        ServiceRegistry::open_saved(&root)?
            .catalog_state()?
            .is_none()
    );
    let fixture = Fixture::new()?;
    assert!(quiesce_for_maintenance(&fixture.lease)?);
    assert_eq!(
        registry(&fixture.root)?.recorded_service_instance()?,
        Some(OLD)
    );
    let sql = rusqlite::Connection::open(fixture.root.join("service-1.4.sqlite3"))?;
    sql.execute("UPDATE index_catalog_meta SET activated=0", [])?;
    assert!(quiesce_for_maintenance(&fixture.lease).is_err());
    Ok(())
}

#[test]
fn guarded_mode_never_falls_back_for_unqualified_or_unknown_catalog_contracts() -> Result<()> {
    let fixture = Fixture::new()?;
    assert!(crate::embedding_pool_bootstrap::shared_mode(&fixture.root)?);
    let sql = rusqlite::Connection::open(fixture.root.join("service-1.4.sqlite3"))?;
    sql.execute("UPDATE index_catalog_meta SET activated=0", [])?;
    assert!(crate::embedding_pool_bootstrap::shared_mode(&fixture.root).is_err());
    sql.execute(
        "UPDATE index_catalog_meta SET activated=1,policy_contract=1",
        [],
    )?;
    assert!(crate::embedding_pool_bootstrap::shared_mode(&fixture.root).is_err());
    sql.execute(
        "UPDATE index_catalog_meta SET policy_contract=0,authority_contract=2",
        [],
    )?;
    assert!(crate::embedding_pool_bootstrap::shared_mode(&fixture.root).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn admitted_unbound_startup_retains_its_actual_pin_until_every_native_copy_closes() -> Result<()> {
    let fixture = Fixture::new()?;
    let owner = SharedHelperIdentity {
        service: OLD,
        helper: HELPER,
    };
    let pin = airk_store::service::SharedHelperLease::acquire(&fixture.root, owner)?;
    let (device, inode) = pin.identity()?;
    let inherited = pin.duplicate_native_pin()?;
    #[cfg(target_os = "macos")]
    let scope = "darwin-global".to_owned();
    #[cfg(target_os = "linux")]
    let scope = std::fs::read_link("/proc/self/ns/pid")?
        .to_str()
        .context("native fixture PID scope")?
        .to_owned();
    registry(&fixture.root)?.begin_shared_helper_drain(
        owner,
        SharedHelperContainment::Unix {
            guardian_pid: None,
            group: None,
            pin_device: device,
            pin_inode: inode,
        },
        ProviderBoot {
            boot_id: None,
            process_scope: scope,
            elapsed_ns: 0,
            monotonic_reset_proves_boot: false,
        },
    )?;
    fixture.restart()?;
    drop(pin);
    assert!(recover_shared_helper(&fixture.root, owner)?.is_none());
    assert!(recover_predecessor_shared_helpers(&fixture.root, CURRENT)?.is_none());
    drop(inherited);
    assert!(
        recover_shared_helper(&fixture.root, owner)?
            .context("covered no-permit startup ended")?
            .matches(OLD, HELPER)
    );
    // A terminal row records the native observation. The lock inode can no
    // longer authorize effects and is not reopened as new ownership proof.
    let path = fixture
        .root
        .join(airk_store::service::shared_helper_pin_name(owner));
    std::fs::rename(&path, path.with_extension("old"))?;
    std::fs::write(&path, b"foreign replacement preserved")?;
    assert!(
        recover_shared_helper(&fixture.root, owner)?
            .context("durable terminal proof")?
            .matches(OLD, HELPER)
    );
    assert_eq!(std::fs::read(path)?, b"foreign replacement preserved");
    let proof = recover_predecessor_shared_helpers(&fixture.root, CURRENT)?
        .context("captured actual native completion")?;
    assert_eq!(
        prune_predecessor_shared_helpers(&fixture.root, CURRENT, &fixture.lease, &proof)?,
        1
    );
    // Reusing the retained opaque native proof after a successful commit whose
    // observation was lost must be idempotent. Missing rows mint no new proof.
    assert_eq!(
        prune_predecessor_shared_helpers(&fixture.root, CURRENT, &fixture.lease, &proof)?,
        0
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn uncreated_collision_is_not_adopted_and_terminal_recovery_never_reopens_reused_name() -> Result<()>
{
    use std::os::windows::io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::JobObjects::{
        CreateJobObjectW, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        OpenJobObjectW, QueryInformationJobObject,
    };
    let fixture = Fixture::new()?;
    let mut catalog = registry(&fixture.root)?;
    let namespace = catalog.incarnation()?;
    let owner = SharedHelperIdentity {
        service: OLD,
        helper: HELPER,
    };
    let user = crate::local_ipc::current_user()?;
    let name = shared_helper_job_name(namespace, owner, &user)?;
    catalog.begin_shared_helper_drain(
        owner,
        SharedHelperContainment::Windows {
            job_name: name.clone(),
            user_sid: user.clone(),
            session: session()?,
            created: false,
            leader_pid: None,
        },
        boot(&user)?,
    )?;
    let job = crate::local_ipc::with_current_user_job_security(|attributes, _| {
        airk_process::ChildJob::unassigned_named(&name, attributes)
    })?;
    fixture.restart()?;
    assert!(
        recover_shared_helper(&fixture.root, owner)?
            .context("no-spawn native proof")?
            .matches(OLD, HELPER)
    );
    let wide = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    // SAFETY: live terminated name and inspection-only rights.
    let raw = unsafe { OpenJobObjectW(0x0004, 0, wide.as_ptr()) };
    ensure!(!raw.is_null(), std::io::Error::last_os_error());
    // SAFETY: successful native open transfers one owned handle.
    let inspection = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: retained job handle and correctly sized output.
    ensure!(
        unsafe {
            QueryInformationJobObject(
                inspection.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&mut limits as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
                std::ptr::null_mut(),
            )
        } != 0,
        std::io::Error::last_os_error()
    );
    assert_eq!(
        limits.BasicLimitInformation.ActiveProcessLimit, 32,
        "recovery must not adopt/drain a collided job"
    );
    drop((inspection, job));
    // This is a new native object under the old name with ordinary inherited
    // security, not the protected producer ACL. Terminal recovery must not open
    // it or attempt to repair/terminate it.
    let raw = unsafe { CreateJobObjectW(std::ptr::null(), wide.as_ptr()) };
    ensure!(!raw.is_null(), std::io::Error::last_os_error());
    let replacement = unsafe { OwnedHandle::from_raw_handle(raw) };
    assert!(crate::local_ipc::require_job_user(replacement.as_handle(), &user).is_err());
    assert!(
        recover_shared_helper(&fixture.root, owner)?
            .context("durable terminal proof")?
            .matches(OLD, HELPER)
    );
    let proof = recover_predecessor_shared_helpers(&fixture.root, CURRENT)?
        .context("captured native no-spawn proof")?;
    assert_eq!(
        prune_predecessor_shared_helpers(&fixture.root, CURRENT, &fixture.lease, &proof)?,
        1
    );
    assert_eq!(
        prune_predecessor_shared_helpers(&fixture.root, CURRENT, &fixture.lease, &proof)?,
        0
    );
    Ok(())
}
