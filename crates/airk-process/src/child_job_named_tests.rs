// SPDX-License-Identifier: Apache-2.0
//! Native kernel tests, not Store admission or CLI user-ACL qualification.
use super::*;
use anyhow::Context;
use std::{
    io::{BufRead, Write},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use windows_sys::Win32::System::{
    JobObjects::IsProcessInJob,
    Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        QueryFullProcessImageNameW,
    },
};

fn wait_ready_pid(
    path: &Path,
    owner: &mut impl FnMut() -> io::Result<Option<ExitStatus>>,
) -> Result<u32> {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        match std::fs::read_to_string(path) {
            Ok(value) if value.len() <= 32 => {
                if let Ok(pid) = value.parse() {
                    return Ok(pid);
                }
            }
            Ok(_) => anyhow::bail!("native fixture readiness exceeds bound"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        ensure!(
            Instant::now() < until && owner()?.is_none(),
            "owned native fixture readiness deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn retain_process(pid: u32) -> Result<OwnedHandle> {
    ensure!(pid > 1, "invalid actual native process identity");
    // SAFETY: capture an actual fixture process while readiness proves it live.
    let raw = unsafe {
        OpenProcess(
            PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        )
    };
    ensure!(!raw.is_null(), io::Error::last_os_error());
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}
fn wait_exited(process: &OwnedHandle) -> Result<()> {
    // SAFETY: retained actual process handle, never a PID-based signal.
    ensure!(
        unsafe { WaitForSingleObject(process.as_raw_handle(), 5000) } == WAIT_OBJECT_0,
        "actual owned native process did not terminate"
    );
    Ok(())
}
fn wait_resource_unlock(file: &File) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(()),
            Err(std::fs::TryLockError::WouldBlock) => {
                // LockFileEx explicitly permits delayed OS unlock after the
                // process terminates. Still require this ACTUAL resource to
                // become available; a missing Job name is never the proof.
                // https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex
                ensure!(
                    Instant::now() < until,
                    "terminated descendant still holds its actual resource lock"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error.into()),
        }
    }
}
fn inspect_members(job: &ChildJob, pids: &[u32]) -> Result<Vec<OwnedHandle>> {
    let mut retained = Vec::new();
    for &pid in pids {
        let process = retain_process(pid)?;
        let mut associated = 0;
        // SAFETY: actual retained process and owned native job handles.
        ensure!(
            unsafe {
                IsProcessInJob(
                    process.as_raw_handle(),
                    job.0.as_raw_handle(),
                    &mut associated,
                )
            } != 0
                && associated != 0,
            "queried process does not belong to this exact job"
        );
        let mut image = [0u16; 32768];
        let mut length = image.len() as u32;
        // Diagnostics identify extra native members (e.g. a console host)
        // without assuming that the only two possible members are our roles.
        if unsafe {
            QueryFullProcessImageNameW(process.as_raw_handle(), 0, image.as_mut_ptr(), &mut length)
        } != 0
        {
            let image = String::from_utf16_lossy(&image[..length as usize]);
            eprintln!(
                "actual owned job member {pid}: {}",
                Path::new(&image)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            );
        }
        retained.push(process);
    }
    Ok(retained)
}

fn attributes() -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 0,
    }
}
fn unique_name(suffix: &str) -> String {
    format!(
        "Global\\AIRK-native-test-{}-{}-{suffix}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}
struct Controller(Child);
impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn read_line(input: &mut impl BufRead) -> Result<String> {
    let mut value = String::new();
    ensure!(
        input.read_line(&mut value)? > 0 && value.len() <= 4096,
        "invalid native fixture frame"
    );
    Ok(value.trim_end_matches(['\r', '\n']).to_owned())
}
fn selected(role: &str) -> bool {
    std::env::args().skip(1).any(|arg| arg == role)
}

#[test]
fn global_native_names_reject_local_foreign_and_oversized_names() {
    for value in [
        "Local\\AIRK-native",
        "Global\\other\\object",
        "Global\\",
        "Global\\name\0other",
    ] {
        assert!(name(value).is_err());
    }
    assert!(name(&format!("Global\\{}", "a".repeat(241))).is_err());
}

#[test]
fn named_collision_and_unverified_open_do_not_become_cleanup_owners() -> Result<()> {
    let value = unique_name("collision");
    let job = ChildJob::unassigned_named(&value, &attributes())?;
    assert!(ChildJob::unassigned_named(&value, &attributes()).is_err());
    assert!(
        ChildJob::open_named(&value, |_| anyhow::bail!(
            "injected foreign native identity"
        ))
        .is_err()
    );
    let reopened = ChildJob::open_named(&value, |_| Ok(()))?;
    assert!(matches!(reopened, NamedJobOpen::Owned(_)));
    drop(reopened);
    drop(job);
    assert!(matches!(
        ChildJob::open_named(&value, |_| panic!(
            "missing object has no handle to validate"
        ))?,
        NamedJobOpen::Missing
    ));
    Ok(())
}

#[test]
fn owned_job_membership_query_observes_actual_child_and_descendant_without_draining() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let lease = directory.path().join("members.lock");
    let job = ChildJob::unassigned_named(&unique_name("members"), &attributes())?;
    assert!(job.member_pids()?.is_empty());
    let mut child = job.spawn_guarded(&std::env::current_exe()?, "namedjobfixture")?;
    writeln!(
        child.stdin.as_mut().context("fixture input")?,
        "{}",
        lease.display()
    )?;
    child.stdin.as_mut().context("fixture input")?.flush()?;
    let descendant = wait_ready_pid(
        Path::new(&format!("{}.ready", lease.display())),
        &mut || child.try_wait(),
    )?;
    let members = job.member_pids()?;
    assert_ne!(child.id(), descendant);
    assert!(members.contains(&child.id()));
    assert!(
        members.contains(&descendant),
        "actual resource-owning descendant is missing from its job"
    );
    let observed = inspect_members(&job, &members)?;
    let guard = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lease)?;
    assert!(matches!(
        guard.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(
        child.try_wait()?.is_none(),
        "query must not terminate native members"
    );
    job.terminate_and_wait(Duration::from_secs(2))?;
    child.wait()?;
    for process in observed {
        wait_exited(&process)?;
    }
    wait_resource_unlock(&guard)?;
    assert!(job.member_pids()?.is_empty());
    Ok(())
}

#[test]
fn abrupt_controller_exit_cannot_leave_atomic_job_children_or_descendants() -> Result<()> {
    for stage in ["created", "spawned", "permitted"] {
        let root = tempfile::tempdir()?;
        let value = unique_name(stage);
        let ready = root.path().join("controller-ready");
        let lease = root.path().join("descendant.lock");
        let mut command = Command::new(std::env::current_exe()?);
        use std::os::windows::process::CommandExt;
        command
            .arg("namedjobcontrollerfixture")
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut controller = Controller(command.spawn()?);
        let input = controller.0.stdin.as_mut().unwrap();
        writeln!(
            input,
            "{value}\n{stage}\n{}\n{}",
            ready.display(),
            lease.display()
        )?;
        input.flush()?;
        let pid = wait_ready_pid(&ready, &mut || controller.0.try_wait())?;
        let process = if pid > 1 {
            Some(retain_process(pid)?)
        } else {
            None
        };
        let descendant = if stage == "permitted" {
            let pid = wait_ready_pid(
                Path::new(&format!("{}.ready", lease.display())),
                &mut || controller.0.try_wait(),
            )?;
            Some(retain_process(pid)?)
        } else {
            None
        };
        let guard = if stage == "permitted" {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&lease)?;
            assert!(matches!(
                file.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ));
            // A collision/refused-owner observation must not kill live work.
            assert!(ChildJob::unassigned_named(&value, &attributes()).is_err());
            assert!(ChildJob::open_named(&value, |_| anyhow::bail!("foreign owner")).is_err());
            assert!(matches!(
                file.try_lock(),
                Err(std::fs::TryLockError::WouldBlock)
            ));
            Some(file)
        } else {
            assert!(!lease.exists());
            None
        };
        controller.0.kill()?;
        controller.0.wait()?;
        match ChildJob::open_named(&value, |_| Ok(()))? {
            NamedJobOpen::Owned(job) => job.terminate_and_wait(Duration::from_secs(5))?,
            NamedJobOpen::Missing => {}
        }
        if let Some(process) = process {
            wait_exited(&process)?;
        }
        if let Some(descendant) = descendant {
            wait_exited(&descendant)?;
        }
        if let Some(guard) = guard {
            wait_resource_unlock(&guard)?;
        }
        assert!(matches!(
            ChildJob::open_named(&value, |_| Ok(()))?,
            NamedJobOpen::Missing
        ));
    }
    Ok(())
}

#[test]
fn namedjobcontrollerfixture() -> Result<()> {
    if !selected("namedjobcontrollerfixture") {
        return Ok(());
    }
    let mut input = io::BufReader::new(io::stdin());
    let value = read_line(&mut input)?;
    let stage = read_line(&mut input)?;
    let ready = read_line(&mut input)?;
    let lease = read_line(&mut input)?;
    let job = ChildJob::unassigned_named(&value, &attributes())?;
    let mut child = if stage == "created" {
        None
    } else {
        let child = job.spawn_guarded(&std::env::current_exe()?, "namedjobfixture")?;
        ensure!(
            child.assigned_to(&job)?,
            "atomic native job assignment missing"
        );
        Some(child)
    };
    if stage == "permitted" {
        let child = child.as_mut().unwrap();
        writeln!(child.stdin.as_mut().unwrap(), "{lease}")?;
        child.stdin.as_mut().unwrap().flush()?;
        let until = Instant::now() + Duration::from_secs(10);
        while !Path::new(&format!("{lease}.ready")).try_exists()? {
            ensure!(
                Instant::now() < until && child.try_wait()?.is_none(),
                "native descendant did not acquire resource"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    std::fs::write(
        ready,
        child.as_ref().map_or(0, AtomicJobChild::id).to_string(),
    )?;
    // The parent kills this real process. No Rust destructor runs to help the
    // kernel's last-job-handle contract or hide a startup assignment gap.
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[test]
fn namedjobfixture() -> Result<()> {
    if !selected("namedjobfixture") {
        return Ok(());
    }
    let lease = read_line(&mut io::BufReader::new(io::stdin()))?;
    let mut command = Command::new(std::env::current_exe()?);
    use std::os::windows::process::CommandExt;
    command
        .arg("namedjobdescendantfixture")
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = Controller(command.spawn()?);
    writeln!(child.0.stdin.as_mut().unwrap(), "{lease}")?;
    child.0.stdin.as_mut().unwrap().flush()?;
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}

#[test]
fn namedjobdescendantfixture() -> Result<()> {
    if !selected("namedjobdescendantfixture") {
        return Ok(());
    }
    let path = read_line(&mut io::BufReader::new(io::stdin()))?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.try_lock()?;
    std::fs::write(format!("{path}.ready"), std::process::id().to_string())?;
    loop {
        std::thread::sleep(Duration::from_secs(1));
    }
}
