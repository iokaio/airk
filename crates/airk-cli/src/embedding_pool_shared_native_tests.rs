// SPDX-License-Identifier: Apache-2.0
//! Explicit native qualification against the newly built verified executable
//! pair. These are opt-in and fail if the pair is not supplied; a metadata
//! fixture alone is never reported as guarded-helper qualification.
use super::*;
use std::time::{Duration, Instant};

fn real_main() -> Result<PathBuf> {
    std::env::var_os("AIRK_GUARDED_PAIR_MAIN")
        .map(PathBuf::from)
        .context("set AIRK_GUARDED_PAIR_MAIN to the newly built, verified airk executable pair")
}

#[test]
#[ignore = "requires newly built verified airk/airk-embed native pair"]
fn guarded_real_pair_starts_and_records_actual_native_completion() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut pool = Pool::start_fixture(&fixture.root, OLD, &real_main()?)?;
    assert!(matches!(
        pool.status()?,
        protocol::Observation::Status { .. }
    ));
    let owner = SharedHelperIdentity {
        service: OLD,
        helper: pool.identity(),
    };
    let row = registry(&fixture.root)?
        .shared_helper_drain(owner)?
        .context("actual native admission")?;
    assert_eq!(row.phase, SharedHelperPhase::Bound);
    match row.containment {
        SharedHelperContainment::Unix {
            guardian_pid,
            group,
            ..
        } => {
            assert!(guardian_pid.is_some());
            assert_eq!(group, Some(pool.process_id()));
        }
        SharedHelperContainment::Windows {
            created,
            leader_pid,
            ..
        } => {
            assert!(created);
            assert_eq!(leader_pid, Some(pool.process_id()));
        }
    }
    pool.terminate()?;
    assert!(
        pool.reap_witness()
            .context("actual group/job and reader completion")?
            .matches(OLD, owner.helper)
    );
    assert_eq!(
        registry(&fixture.root)?
            .shared_helper_drain(owner)?
            .context("durable native completion")?
            .phase,
        SharedHelperPhase::Drained
    );
    Ok(())
}

struct Controller(std::process::Child);
impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "requires newly built verified airk/airk-embed native pair"]
fn abrupt_service_exit_recovers_real_guarded_helper_before_predecessor_proof() -> Result<()> {
    let main = real_main()?;
    let Fixture {
        _directory,
        root,
        lease,
    } = Fixture::new()?;
    drop(lease);
    let ready = _directory.path().join("native-ready");
    let mut controller = Controller(
        std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "embedding_pool_process::shared::tests::native::guarded_shared_controller_fixture",
                "--ignored",
            ])
            .env("AIRK_GUARDED_PAIR_MAIN", main)
            .env("AIRK_GUARDED_CONTROL_ROOT", &root)
            .env("AIRK_GUARDED_CONTROL_READY", &ready)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .spawn()?,
    );
    let until = Instant::now() + Duration::from_secs(30);
    while !ready.try_exists()? {
        ensure!(
            Instant::now() < until && controller.0.try_wait()?.is_none(),
            "guarded controller did not reach native ready"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let before = registry(&root)?.shared_helper_drains()?;
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].phase, SharedHelperPhase::Bound);
    controller.0.kill()?;
    controller.0.wait()?;
    let lease = ServiceLease::acquire(&root)?;
    registry(&root)?.record_service_instance(&lease, CURRENT)?;
    let until = Instant::now() + Duration::from_secs(30);
    let proof = loop {
        if let Some(proof) = recover_predecessor_shared_helpers(&root, CURRENT)? {
            break proof;
        }
        ensure!(
            Instant::now() < until,
            "actual old guarded resources did not drain"
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(proof.witnesses().len(), 1);
    assert!(proof.witnesses()[0].matches(OLD, before[0].owner.helper));
    // This exclusion supplements actual native container proof above. It is
    // never used as the proof by itself.
    let _embedding = airk_store::service::EmbeddingLease::acquire(&root)?;
    assert_eq!(
        prune_predecessor_shared_helpers(&root, CURRENT, &lease, &proof)?,
        1
    );
    assert!(registry(&root)?.shared_helper_drains()?.is_empty());
    Ok(())
}

#[test]
#[ignore = "isolated native controller entry; selected by its owning test only"]
fn guarded_shared_controller_fixture() -> Result<()> {
    let root =
        PathBuf::from(std::env::var_os("AIRK_GUARDED_CONTROL_ROOT").context("controller root")?);
    let ready =
        PathBuf::from(std::env::var_os("AIRK_GUARDED_CONTROL_READY").context("controller ready")?);
    let lease = ServiceLease::acquire(&root)?;
    registry(&root)?.record_service_instance(&lease, OLD)?;
    let mut pool = Pool::start_fixture(&root, OLD, &real_main()?)?;
    assert!(matches!(
        pool.status()?,
        protocol::Observation::Status { .. }
    ));
    std::fs::write(ready, b"actual guarded helper ready")?;
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[cfg(unix)]
#[test]
#[ignore = "requires newly built verified Unix airk/airk-embed native pair"]
fn actual_guardian_death_contains_helper_and_descendant_while_pool_input_is_open() -> Result<()> {
    use std::{io, os::unix::process::CommandExt};
    let fixture = Fixture::new()?;
    let mut pool = Pool::start_fixture(&fixture.root, OLD, &real_main()?)?;
    let owner = SharedHelperIdentity {
        service: OLD,
        helper: pool.identity(),
    };
    let row = registry(&fixture.root)?
        .shared_helper_drain(owner)?
        .context("live guarded admission")?;
    let SharedHelperContainment::Unix {
        guardian_pid: Some(guardian),
        group: Some(group),
        ..
    } = row.containment
    else {
        anyhow::bail!("actual Unix guarded binding missing");
    };
    let ready = fixture._directory.path().join("group-descendant.ready");
    let resource = fixture._directory.path().join("group-descendant.lock");
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .args([
            "--exact",
            "embedding_pool_process::shared::tests::native::guarded_group_descendant_fixture",
            "--ignored",
        ])
        .env("AIRK_GUARDED_DESCENDANT_READY", &ready)
        .env("AIRK_GUARDED_DESCENDANT_RESOURCE", &resource)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit());
    // This isolated child joins the exact live helper group. It holds a real
    // resource and is owned/reaped by this test, never found by a process scan.
    command.process_group(i32::try_from(group)?);
    let mut descendant = Controller(command.spawn()?);
    let until = Instant::now() + Duration::from_secs(20);
    while !ready.try_exists()? {
        ensure!(
            Instant::now() < until && descendant.0.try_wait()?.is_none(),
            "actual grouped descendant did not start"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&resource)?;
    assert!(matches!(
        held.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(
        pool.input.is_some(),
        "test requires still-open Pool control input"
    );
    // Guardian is this exact test's unreaped native child. WNOHANG confirms
    // child ownership without releasing a live/reusable PID; no other thread
    // waits on this owner. Only this owned fixture guardian is killed.
    ensure!(
        unsafe {
            libc::waitpid(
                i32::try_from(guardian)?,
                std::ptr::null_mut(),
                libc::WNOHANG,
            )
        } == 0,
        "native guardian was not our live owned child"
    );
    ensure!(
        unsafe { libc::kill(i32::try_from(guardian)?, libc::SIGKILL) } == 0,
        io::Error::last_os_error()
    );
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Err(error) = pool.exit_status() {
            if pool.containment_failed(&error) {
                break;
            }
        }
        ensure!(
            Instant::now() < until,
            "native guardian failure was not identified"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        pool.input.is_some(),
        "guardian loss must be handled even while control input stays open"
    );
    assert!(
        pool.reap_witness().is_none(),
        "guardian death alone is not provider completion"
    );
    // Keep Pool input open until the helper's EXISTING timer observes its
    // private guardian peer loss and contains the actual whole group. Closing
    // stdin first would merely exercise graceful EOF instead of this failure.
    while descendant.0.try_wait()?.is_none() {
        assert!(pool.input.is_some());
        assert!(pool.reap_witness().is_none());
        ensure!(
            Instant::now() < until,
            "guardian loss did not contain the actual helper descendant"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    loop {
        if pool.terminate().is_ok() {
            break;
        }
        ensure!(
            Instant::now() < until,
            "exact helper group did not drain after guardian death"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        pool.reap_witness()
            .context("actual group/pin/reader proof")?
            .matches(OLD, owner.helper)
    );
    assert!(
        pool.exit_status().is_err(),
        "native containment proof must not invent an unobserved helper exit status"
    );
    held.try_lock()?;
    assert_eq!(
        registry(&fixture.root)?
            .shared_helper_drain(owner)?
            .context("canonical native completion")?
            .phase,
        SharedHelperPhase::Drained
    );
    Ok(())
}

#[cfg(unix)]
#[test]
#[ignore = "isolated actual native group member entry"]
fn guarded_group_descendant_fixture() -> Result<()> {
    let ready = PathBuf::from(
        std::env::var_os("AIRK_GUARDED_DESCENDANT_READY").context("descendant ready")?,
    );
    let resource = PathBuf::from(
        std::env::var_os("AIRK_GUARDED_DESCENDANT_RESOURCE").context("descendant resource")?,
    );
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(resource)?;
    held.try_lock()?;
    std::fs::write(ready, b"actual group member resource held")?;
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
