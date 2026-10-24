// SPDX-License-Identifier: Apache-2.0
//! Isolated native crash fixtures: never signal a process found by name/PID scan.
#![cfg(unix)]
use airk_core::{
    identity::Incarnation,
    service::{ProjectRegistration, WorkerIdentity},
};
use airk_store::service::{ProjectLease, registry::ServiceRegistry};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
const SERVICE: Incarnation = Incarnation::from_bytes([91; 16]);
fn wait(mut condition: impl FnMut() -> bool) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(20);
    while !condition() {
        ensure!(Instant::now() < until, "guardian fixture deadline");
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}
struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_))) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
fn fixture_command(name: &str) -> Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    command.args(["--ignored", "--exact", name, "--nocapture"]);
    Ok(command)
}

#[test]
fn guardian_keeps_direct_pipes_and_drains_after_worker_death() -> Result<()> {
    let mut command = fixture_command("guardian_scenarios_fixture")?;
    command.env("AIRK_GUARDIAN_MAIN", env!("CARGO_BIN_EXE_airk"));
    let mut child = Owned(command.spawn()?);
    let until = Instant::now() + Duration::from_secs(90);
    while !matches!(child.0.try_wait(), Ok(Some(_))) {
        ensure!(
            Instant::now() < until,
            "isolated guardian scenario deadline"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    ensure!(
        child.0.wait()?.success(),
        "isolated guardian fixture failed"
    );
    Ok(())
}

#[test]
#[ignore = "isolated subprocess entry"]
fn guardian_scenarios_fixture() -> Result<()> {
    if std::env::var_os("AIRK_GUARDIAN_MAIN").is_none() {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    // SAFETY: this dedicated subprocess owns all of the fixture descendants.
    ensure!(
        unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) } == 0,
        "subreaper fixture unavailable"
    );
    for scenario in ["normal", "worker-killed"] {
        run_scenario(scenario)?;
    }
    #[cfg(target_os = "linux")]
    run_scenario("guardian-killed")?;
    Ok(())
}
fn reap_orphans(protected: Option<u32>) {
    #[cfg(target_os = "linux")]
    {
        use std::{collections::BTreeSet, io::Read};
        let mut children = BTreeSet::new();
        if let Ok(tasks) = fs::read_dir("/proc/self/task") {
            for task in tasks.take(1024).flatten() {
                let Ok(file) = fs::File::open(task.path().join("children")) else {
                    continue;
                };
                let mut bytes = Vec::new();
                if file.take(8193).read_to_end(&mut bytes).is_err() || bytes.len() > 8192 {
                    continue;
                }
                let Ok(text) = std::str::from_utf8(&bytes) else {
                    continue;
                };
                children.extend(
                    text.split_whitespace()
                        .filter_map(|s| s.parse::<i32>().ok())
                        .filter(|pid| *pid > 0 && Some(*pid as u32) != protected)
                        .take(128),
                );
            }
        }
        for pid in children {
            // SAFETY: read-only wait for this isolated process's own children.
            // Its actively owned service is excluded, even after an unexpected exit.
            unsafe {
                libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG);
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = protected;
}
fn run_scenario(scenario: &str) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let runtime = directory.path().join("runtime");
    airk_cli::local_ipc::prepare_root(&runtime)?;
    let source = directory.path().join("source");
    fs::create_dir(&source)?;
    fs::write(
        source.join("unchanged.txt"),
        b"source fixture stays read-only\n",
    )?;
    let data = directory.path().join("derived");
    let registration = ProjectRegistration::new(
        source.to_str().context("source UTF8")?.into(),
        data.to_str().context("data UTF8")?.into(),
        "fixture".into(),
    )?;
    let mut registry = ServiceRegistry::open(&runtime, SERVICE)?;
    registry.register(&registration)?;
    let ready = directory.path().join("ready");
    let stop = directory.path().join("stop");
    let executable = if scenario == "guardian-killed" {
        let wrapper = directory.path().join("guardian-fixture");
        let source = directory.path().join("guardian-fixture.rs");
        fs::write(
            &source,
            r#"
use std::{env,fs,process::{Command,Stdio},time::{Duration,Instant}};
fn main() {
    let mut child=Command::new(env::var_os("AIRK_GUARDIAN_MAIN").unwrap())
        .args(env::args_os().skip(1)).stdin(Stdio::inherit()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    let until=Instant::now()+Duration::from_secs(20);
    while !std::path::Path::new(&env::var_os("AIRK_GUARDIAN_READY").unwrap()).exists(){
        assert!(Instant::now()<until);assert!(child.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(10));
    }
    child.kill().unwrap();child.wait().unwrap();
    fs::write(env::var_os("AIRK_GUARDIAN_CRASHED").unwrap(),[]).unwrap();
}
"#,
        )?;
        ensure!(
            Command::new("rustc")
                .arg("--edition=2024")
                .arg(&source)
                .arg("-o")
                .arg(&wrapper)
                .status()?
                .success(),
            "compile owned guardian-crash fixture"
        );
        wrapper
    } else {
        PathBuf::from(std::env::var_os("AIRK_GUARDIAN_MAIN").context("main executable")?)
    };
    let mut command = fixture_command("provider_worker_fixture")?;
    command
        .env("AIRK_GUARDIAN_RUNTIME", &runtime)
        .env("AIRK_GUARDIAN_PROJECT", registration.binding.to_string())
        .env("AIRK_GUARDIAN_EXECUTABLE", executable)
        .env("AIRK_GUARDIAN_READY", &ready)
        .env("AIRK_GUARDIAN_STOP", &stop)
        .env("AIRK_GUARDIAN_CRASHED", directory.path().join("crashed"))
        .env(
            "AIRK_GUARDIAN_SLEEP",
            if scenario == "guardian-killed" {
                "8"
            } else {
                "30"
            },
        );
    let mut worker = Owned(command.spawn()?);
    wait(|| ready.exists() || matches!(worker.0.try_wait(), Ok(Some(_))))?;
    ensure!(
        ready.exists(),
        "worker failed before direct provider response"
    );
    let rows = registry.provider_drains()?;
    ensure!(
        rows.len() == 1 && rows[0].group.is_some(),
        "provider not durably bound"
    );
    ensure!(
        ProjectLease::acquire(&runtime, registration.binding).is_err(),
        "worker lease was released early"
    );
    if scenario == "normal" {
        fs::write(stop, [])?;
        wait(|| matches!(worker.0.try_wait(), Ok(Some(_))))?;
        ensure!(worker.0.wait()?.success(), "normal worker failed");
    } else {
        if scenario == "guardian-killed" {
            wait(|| directory.path().join("crashed").exists())?;
            ensure!(
                !registry.provider_drains()?.is_empty(),
                "guardian crash discarded durable ownership"
            );
        }
        worker.0.kill()?;
        worker.0.wait()?;
    }
    if scenario != "guardian-killed" {
        wait(|| {
            reap_orphans(None);
            registry.provider_drains().is_ok_and(|rows| rows.is_empty())
                && ProjectLease::acquire(&runtime, registration.binding).is_ok()
        })?;
    }
    if scenario == "guardian-killed" {
        // No process is signaled during recovery: the bounded provider expires itself.
        // Starting a service exercises durable group-absence recovery before admission.
        let mut service = Owned(
            Command::new(std::env::var_os("AIRK_GUARDIAN_MAIN").context("main executable")?)
                .args(["serve", "--runtime-dir"])
                .arg(&runtime)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        let client = tokio::runtime::Runtime::new()?;
        wait(
            || matches!(client.block_on(airk_cli::service_runtime::request(&runtime,airk_core::service::ControlMethod::Resources)),Ok(airk_core::service::ControlResponse::Resources{observation,..}) if observation.pending_provider_drains>0&&!observation.provider_drain_error),
        )?;
        ensure!(
            !registry.provider_drains()?.is_empty(),
            "restart released a live provider charge"
        );
        wait(|| {
            reap_orphans(Some(service.0.id()));
            registry.provider_drains().is_ok_and(|rows| rows.is_empty())
        })?;
        service.0.kill()?;
        service.0.wait()?;
        reap_orphans(None);
    }
    ensure!(
        fs::read(source.join("unchanged.txt"))? == b"source fixture stays read-only\n",
        "source fixture changed"
    );
    Ok(())
}

#[test]
#[ignore = "isolated subprocess entry"]
fn provider_worker_fixture() -> Result<()> {
    let Some(runtime) = std::env::var_os("AIRK_GUARDIAN_RUNTIME") else {
        return Ok(());
    };
    let runtime = PathBuf::from(runtime);
    let project = std::env::var("AIRK_GUARDIAN_PROJECT")?.parse()?;
    let mut registry = ServiceRegistry::open(&runtime, SERVICE)?;
    let mut lease = ProjectLease::acquire(&runtime, project)?;
    let generation = registry.admit_worker(&mut lease)?;
    let identity = WorkerIdentity {
        service: SERVICE,
        worker: Incarnation::from_bytes([92; 16]),
        project,
        fencing_generation: generation,
    };
    let executable =
        PathBuf::from(std::env::var_os("AIRK_GUARDIAN_EXECUTABLE").context("guardian executable")?);
    airk_cli::unix_guardian::install(
        &runtime,
        registry.incarnation()?,
        identity,
        &lease,
        &executable,
    )?;
    // The production worker uses a process-static lease for the same reason.
    let _lease = Box::leak(Box::new(lease));
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            "read line; sleep \"$AIRK_GUARDIAN_SLEEP\" & printf 'direct:%s\\n' \"$line\"; wait",
        ])
        .current_dir(Path::new("/"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = airk_process::GroupChild::spawn_provider(&mut command, None)?;
    let mut input = child.stdin.take().context("provider stdin")?;
    input.write_all(b"pipe bytes\n")?;
    drop(input);
    let mut output = BufReader::new(child.stdout.take().context("provider stdout")?);
    let mut line = String::new();
    output.read_line(&mut line)?;
    ensure!(
        line == "direct:pipe bytes\n",
        "provider pipe framing changed"
    );
    fs::write(
        PathBuf::from(std::env::var_os("AIRK_GUARDIAN_READY").context("ready path")?),
        [],
    )?;
    let stop = PathBuf::from(std::env::var_os("AIRK_GUARDIAN_STOP").context("stop path")?);
    wait(|| stop.exists())?;
    child.terminate_and_wait(Duration::from_secs(2))?;
    Ok(())
}
