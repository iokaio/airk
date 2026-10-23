// SPDX-License-Identifier: Apache-2.0
//! Native process election/control uses its own private fixture, never an installed service.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Children(Vec<Child>);
impl Drop for Children {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn command() -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new(env!("CARGO_BIN_EXE_airk"));
        command.creation_flags(0x08000000);
        command
    }
    #[cfg(not(windows))]
    {
        Command::new(env!("CARGO_BIN_EXE_airk"))
    }
}

fn start(root: &Path) -> Result<Child> {
    static SEQUENCE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let log = root
        .parent()
        .context("fixture parent missing")?
        .join(format!("service-child-{sequence}.stderr"));
    let log = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(log)?;
    Ok(command()
        .arg("serve")
        .arg("--runtime-dir")
        .arg(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()?)
}

fn control(root: &Path, action: &str) -> Result<Value> {
    let result = command()
        .args(["service", action, "--runtime-dir"])
        .arg(root)
        .output()?;
    ensure!(
        result.status.success(),
        "service control failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(serde_json::from_slice(&result.stdout)?)
}

fn wait_ready(root: &Path) -> Result<Value> {
    // Debug ELF images include large DWARF sections; full identity hashing is part of startup.
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Ok(response) = control(root, "health") {
            return Ok(response);
        }
        if Instant::now() >= deadline {
            use std::io::Read;
            let mut detail = String::new();
            for entry in fs::read_dir(root.parent().context("fixture parent missing")?)?.take(16) {
                let entry = entry?;
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("service-child-")
                {
                    fs::File::open(entry.path())?
                        .take(4096)
                        .read_to_string(&mut detail)?;
                }
            }
            anyhow::bail!("native service failed to become ready: {detail}");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn wait_exit(child: &mut Child) -> Result<bool> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.success());
        }
        ensure!(Instant::now() < deadline, "native service failed to drain");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn exercise_disappearing_stalled_and_malformed_clients(root: &Path) -> Result<()> {
    use airk_cli::local_ipc;
    use airk_core::service::{ControlMethod, ControlRequest, ControlResponse};
    use tokio::io::AsyncWriteExt;
    tokio::runtime::Runtime::new()?.block_on(async {
        let address = local_ipc::endpoint(root, "control")?;
        for _ in 0..3 {
            drop(local_ipc::connect(&address).await?);
        }
        let mut malformed = local_ipc::connect(&address).await?;
        malformed.write_all(&u32::MAX.to_be_bytes()).await?;
        drop(malformed);
        let mut incompatible = local_ipc::connect(&address).await?;
        local_ipc::write_message(
            &mut incompatible,
            &ControlRequest {
                protocol: u32::MAX,
                method: ControlMethod::Health,
            },
            Duration::from_secs(5),
        )
        .await?;
        let response: Option<ControlResponse> =
            local_ipc::read_message(&mut incompatible, Duration::from_secs(5)).await?;
        ensure!(
            matches!(response, Some(ControlResponse::Incompatible { .. })),
            "service accepted incompatible protocol"
        );
        let _stalled = local_ipc::connect(&address).await?;
        ensure!(
            control(root, "health")?["status"] == "health",
            "stalled client blocked health"
        );
        Ok(())
    })
}

fn management_survives_restart_and_busy_registry_does_not_block_health(
    root: &Path,
    owner: &Path,
) -> Result<airk_core::service::JobRecord> {
    use airk_core::service::{
        ControlMethod as Method, ControlResponse as Response, JobKind, RequestKey,
    };
    let source = owner.join("source");
    let data = owner.join("state");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    fs::write(source.join("original.txt"), b"source bytes remain original")?;
    let project = airk_kernel::service_projects::registration(&source, &data, "fixture")?;
    let caller = airk_kernel::service_broker::new_instance_id()?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let request = |method| airk_cli::service_runtime::request(root, method);
        ensure!(
            matches!(
                request(Method::Register {
                    project: project.clone()
                })
                .await?,
                Response::Registered { .. }
            ),
            "registration failed"
        );
        let second = owner.join("source-two");
        fs::create_dir(&second)?;
        let collision = airk_kernel::service_projects::registration(&second, &data, "fixture")?;
        ensure!(
            matches!(
                request(Method::Register { project: collision }).await?,
                Response::Failure { .. }
            ),
            "conflicting storage accepted"
        );
        let Response::RequestEpoch { namespace, epoch } = request(Method::RequestEpoch).await?
        else {
            anyhow::bail!("missing request epoch")
        };
        let key = RequestKey {
            namespace,
            epoch,
            nonce: caller,
        };
        let Response::Job { job } = request(Method::Enqueue {
            project: project.binding,
            caller,
            key: key.clone(),
            kind: JobKind::Index,
        })
        .await?
        else {
            anyhow::bail!("job was not accepted")
        };
        let Response::Job { job: again } = request(Method::Enqueue {
            project: project.binding,
            caller,
            key,
            kind: JobKind::Index,
        })
        .await?
        else {
            anyhow::bail!("receipt retry failed")
        };
        ensure!(job.id == again.id, "duplicate request repeated work");
        // Lock only the metadata ledger; health must still answer without waiting for SQLite.
        let lock = rusqlite::Connection::open(root.join("service-1.4.sqlite3"))?;
        lock.execute_batch("BEGIN EXCLUSIVE")?;
        let blocked = request(Method::Projects {
            after: None,
            revision: None,
            limit: 8,
        });
        let health = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let started = Instant::now();
            ensure!(
                matches!(request(Method::Health).await?, Response::Health { .. }),
                "health failed while registry locked"
            );
            ensure!(
                started.elapsed() < Duration::from_secs(1),
                "health waited behind registry"
            );
            Ok::<_, anyhow::Error>(())
        };
        let (blocked, health) = tokio::join!(blocked, health);
        health?;
        ensure!(
            matches!(blocked?, Response::Failure { .. }),
            "locked ledger did not return bounded failure"
        );
        lock.execute_batch("ROLLBACK")?;
        ensure!(
            fs::read_dir(&data)?.next().is_none(),
            "management opened collection state"
        );
        ensure!(
            fs::read(source.join("original.txt"))? == b"source bytes remain original",
            "source changed"
        );
        Ok(job)
    })
}

#[test]
fn racing_launchers_elect_one_service_and_restart_changes_instance_without_project_access()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let missing = owner.path().join("missing");
    assert!(control(&missing, "health").is_err());
    assert!(!missing.exists(), "health must not initialize a runtime");
    let mut children = Children(Vec::new());
    for _ in 0..3 {
        children.0.push(start(&root)?);
    }
    let first = wait_ready(&root)?;
    assert_eq!(first["status"], "health");
    assert_eq!(first["observation"]["policy"]["projects"], 0);
    assert_eq!(first["observation"]["version"], env!("CARGO_PKG_VERSION"));
    exercise_disappearing_stalled_and_malformed_clients(&root)?;
    let job =
        management_survives_restart_and_busy_registry_does_not_block_health(&root, owner.path())?;
    assert_eq!(control(&root, "shutdown")?["status"], "stopping");
    let mut successes = 0;
    for child in &mut children.0 {
        successes += usize::from(wait_exit(child)?);
    }
    assert_eq!(
        successes, 1,
        "only the elected service should serve and exit normally"
    );
    children.0.push(start(&root)?);
    let second = wait_ready(&root)?;
    assert_ne!(
        first["observation"]["instance"],
        second["observation"]["instance"]
    );
    tokio::runtime::Runtime::new()?.block_on(async {
        let response = airk_cli::service_runtime::request(&root,airk_core::service::ControlMethod::Job { id:job.id }).await?;
        ensure!(matches!(response,airk_core::service::ControlResponse::Job { job:found } if found.id==job.id),"restart lost accepted receipt");
        Ok::<_,anyhow::Error>(())
    })?;
    control(&root, "shutdown")?;
    ensure!(
        wait_exit(children.0.last_mut().context("service child missing")?)?,
        "restarted service failed"
    );
    assert!(root.join("instance-1.4.lock").is_file());
    assert!(
        !root.join("collections-1.2").exists(),
        "health must not open collection state"
    );
    Ok(())
}
