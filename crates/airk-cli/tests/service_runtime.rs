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

fn probe_stdio_bridge(root: &Path, project: airk_core::service::ProjectBinding) -> Result<()> {
    use std::io::{BufRead, Read, Write};
    let mut child = command()
        .arg("bridge")
        .arg("--runtime-dir")
        .arg(root)
        .arg("--project")
        .arg(project.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut input = child.stdin.take().context("bridge input missing")?;
    let output = child.stdout.take().context("bridge output missing")?;
    let mut children = Children(vec![child]);
    let (sender, receiver) = std::sync::mpsc::sync_channel(4);
    let reader = std::thread::spawn(move || {
        let mut output = std::io::BufReader::new(output);
        loop {
            let mut line = Vec::new();
            let result = (&mut output)
                .take(128 * 1024 + 1)
                .read_until(b'\n', &mut line);
            if !matches!(result,Ok(count) if count>0&&count<=128*1024) {
                break;
            }
            if let Ok(value) = serde_json::from_slice::<Value>(&line)
                && sender.try_send(value).is_err()
            {
                break;
            }
        }
    });
    let result = (|| -> Result<()> {
        let mut send = |value: &Value| -> Result<()> {
            serde_json::to_writer(&mut input, value)?;
            input.write_all(b"\n")?;
            input.flush()?;
            Ok(())
        };
        send(
            &serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"airk-original-test","version":"1"}}}),
        )?;
        let initialized = receiver.recv_timeout(Duration::from_secs(10))?;
        ensure!(
            initialized["id"] == 1 && initialized["result"]["serverInfo"]["name"] == "airk",
            "bridge initialization failed"
        );
        send(&serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
        send(&serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))?;
        let listed = receiver.recv_timeout(Duration::from_secs(10))?;
        let tools = listed["result"]["tools"]
            .as_array()
            .context("bridge tool list missing")?;
        ensure!(
            tools.len() == 15
                && tools
                    .iter()
                    .all(|tool| tool["annotations"]["readOnlyHint"] == true),
            "bridge exposed a management tool"
        );
        send(
            &serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"airk_search","arguments":{"query":"cobalt harbor","mode":"lexical","max_bytes":4096}}}),
        )?;
        let evidence = receiver.recv_timeout(Duration::from_secs(10))?;
        ensure!(
            evidence["id"] == 3 && evidence.to_string().contains("original.txt"),
            "stdio bridge did not preserve evidence"
        );
        Ok(())
    })();
    drop(input);
    children.0[0].kill()?;
    children.0[0].wait()?;
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("bridge reader panicked"))?;
    result
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
        request(Method::Pause {
            project: project.binding,
            paused: true,
        })
        .await?;
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

#[test]
fn supervised_jobs_and_direct_queries_preserve_grants_and_stop_on_service_loss() -> Result<()> {
    use airk_cli::{local_ipc, project_client::Connection, project_protocol};
    use airk_core::service::{
        ControlMethod as Method, ControlResponse as Response, JobKind, JobState, RequestKey,
    };
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("state");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    let text = b"Native supervised project retrieval fixture: cobalt harbor.";
    fs::write(source.join("original.txt"), text)?;
    airk_kernel::Kernel::open_collection(&source, Some(data.clone()), "fixture")?
        .configure_vectors(false)?;
    let project = airk_kernel::service_projects::registration(&source, &data, "fixture")?;
    let mut children = Children(vec![start(&root)?]);
    wait_ready(&root)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let request = |method| airk_cli::service_runtime::request(&root, method);
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
        let Response::RequestEpoch { namespace, epoch } = request(Method::RequestEpoch).await?
        else {
            anyhow::bail!("missing request epoch")
        };
        let caller = airk_kernel::service_broker::new_instance_id()?;
        let key = RequestKey {
            namespace,
            epoch,
            nonce: caller,
        };
        let Response::Job { job } = request(Method::Enqueue {
            project: project.binding,
            caller,
            key,
            kind: JobKind::Index,
        })
        .await?
        else {
            anyhow::bail!("index job not accepted")
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let Response::Job { job: observation } = request(Method::Job { id: job.id }).await?
            else {
                anyhow::bail!("job inspection failed")
            };
            if observation.state == JobState::Succeeded {
                break;
            }
            ensure!(
                observation.state != JobState::Failed,
                "native index job failed: {:?}",
                observation.outcome
            );
            ensure!(Instant::now() < deadline, "native job did not finish");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let Response::Connection { role, grant } = request(Method::Connect {
            project: project.binding,
        })
        .await?
        else {
            anyhow::bail!("broker did not return grant")
        };
        let address = local_ipc::endpoint(&root, &role)?;
        let mut first = local_ipc::connect(&address).await?;
        local_ipc::write_message(&mut first, &grant, Duration::from_secs(5)).await?;
        let accepted: Option<project_protocol::Accepted> =
            local_ipc::read_message(&mut first, Duration::from_secs(5)).await?;
        ensure!(
            accepted.is_some_and(|accepted| accepted.identity == grant.identity),
            "direct grant failed"
        );
        let mut replay = local_ipc::connect(&address).await?;
        local_ipc::write_message(&mut replay, &grant, Duration::from_secs(5)).await?;
        ensure!(
            local_ipc::read_message::<_, project_protocol::Accepted>(
                &mut replay,
                Duration::from_secs(5)
            )
            .await
            .ok()
            .flatten()
            .is_none(),
            "replayed grant admitted"
        );
        drop(first);
        drop(replay);
        ensure!(
            airk_store::service::ProjectLease::acquire(&root, project.binding).is_err(),
            "worker lost its ownership lock"
        );
        let mut connection = Connection::connect(&root, project.binding).await?;
        let arguments =
            serde_json::json!({"query":"cobalt harbor","mode":"lexical","max_bytes":4096})
                .as_object()
                .context("invalid fixture query")?
                .clone();
        let result = connection
            .call("airk_search".into(), arguments.clone())
            .await?;
        ensure!(
            result.get("isError").and_then(Value::as_bool) != Some(true),
            "direct query returned error: {result}"
        );
        ensure!(
            result.to_string().contains("original.txt")
                && result.to_string().contains("cobalt harbor"),
            "direct response lost original citation"
        );
        probe_stdio_bridge(&root, project.binding)?;
        // Retrieval-only connections expose neither management nor mutation tools.
        let result = connection
            .call("service_shutdown".into(), Default::default())
            .await?;
        ensure!(
            result.get("isError").and_then(Value::as_bool) == Some(true),
            "worker exposed management"
        );
        ensure!(
            matches!(request(Method::Health).await?, Response::Health { .. }),
            "unauthorized tool stopped service"
        );
        children.0[0].kill()?;
        children.0[0].wait()?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        ensure!(
            connection
                .call("airk_search".into(), arguments)
                .await
                .is_err(),
            "worker continued admission after service loss"
        );
        drop(connection);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(lease) = airk_store::service::ProjectLease::acquire(&root, project.binding) {
                drop(lease);
                break;
            }
            ensure!(
                Instant::now() < deadline,
                "orphan worker did not drain and release its lock"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        ensure!(
            fs::read(source.join("original.txt"))? == text,
            "source changed"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    Ok(())
}

#[test]
fn foreign_registry_state_is_preserved_and_does_not_disable_native_health() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    airk_cli::local_ipc::prepare_root(&root)?;
    let path = root.join("service-1.4.sqlite3");
    let original = b"foreign metadata fixture must not be initialized or deleted";
    fs::write(&path, original)?;
    let mut children = Children(vec![start(&root)?]);
    wait_ready(&root)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        use airk_core::service::{ControlMethod,ControlResponse};
        ensure!(matches!(airk_cli::service_runtime::request(&root,ControlMethod::RequestEpoch).await?,ControlResponse::Failure{code} if code=="registry_unavailable"),"foreign registry was admitted");
        Ok::<_,anyhow::Error>(())
    })?;
    ensure!(
        control(&root, "health")?["status"] == "health",
        "registry failure blocked health"
    );
    control(&root, "shutdown")?;
    ensure!(
        wait_exit(&mut children.0[0])?,
        "failed registry prevented service shutdown"
    );
    ensure!(fs::read(path)? == original, "foreign registry changed");
    Ok(())
}

#[test]
fn restarted_service_defers_allocation_until_predecessor_ownership_releases() -> Result<()> {
    use airk_core::service::{ControlMethod as Method, ControlResponse as Response};
    use airk_store::service::{EmbeddingLease, ProjectLease, registry::ServiceRegistry};
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("state");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    airk_cli::local_ipc::prepare_root(&root)?;
    let project = airk_kernel::service_projects::registration(&source, &data, "fixture")?;
    let mut registry =
        ServiceRegistry::open(&root, airk_kernel::service_broker::new_instance_id()?)?;
    registry.register(&project)?;
    let mut predecessor = ProjectLease::acquire(&root, project.binding)?;
    registry.admit_worker(&mut predecessor)?;
    let embedding_predecessor = EmbeddingLease::acquire(&root)?;
    drop(registry);
    let mut children = Children(vec![start(&root)?]);
    wait_ready(&root)?;
    tokio::runtime::Runtime::new()?.block_on(async {
        let response=airk_cli::service_runtime::request(&root,Method::Connect{project:project.binding}).await?;
        ensure!(matches!(response,Response::Failure{code} if code=="previous_workers_draining"),"replacement ignored predecessor ownership");
        ensure!(matches!(airk_cli::service_runtime::request(&root,Method::Health).await?,Response::Health{..}),"predecessor drain blocked health");
        drop(predecessor);
        tokio::time::sleep(Duration::from_millis(500)).await;
        let response=airk_cli::service_runtime::request(&root,Method::Connect{project:project.binding}).await?;
        ensure!(matches!(response,Response::Failure{code} if code=="previous_workers_draining"),"replacement ignored surviving inference ownership");
        drop(embedding_predecessor);
        tokio::time::sleep(Duration::from_millis(500)).await;
        let response=airk_cli::service_runtime::request(&root,Method::Connect{project:project.binding}).await?;
        ensure!(matches!(response,Response::Connection{grant,..} if grant.identity.fencing_generation==2),"new worker was not admitted after predecessor exit");
        Ok::<_,anyhow::Error>(())
    })?;
    control(&root, "shutdown")?;
    ensure!(wait_exit(&mut children.0[0])?, "replacement did not drain");
    Ok(())
}

#[test]
fn late_child_with_buffered_initialization_cannot_open_collection_without_activation() -> Result<()>
{
    use airk_cli::{project_protocol, service_ipc};
    use airk_core::service::{JobKind, JobState, RequestKey};
    use airk_store::service::registry::ServiceRegistry;
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("state");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    fs::write(source.join("original.txt"), b"late child fixture")?;
    airk_cli::local_ipc::prepare_root(&root)?;
    let project = airk_kernel::service_projects::registration(&source, &data, "fixture")?;
    let instance = airk_kernel::service_broker::new_instance_id()?;
    let mut registry = ServiceRegistry::open(&root, instance)?;
    registry.register(&project)?;
    let (namespace, epoch) = registry.request_epoch()?;
    let job = registry.enqueue(
        project.binding,
        instance,
        &RequestKey {
            namespace,
            epoch,
            nonce: instance,
        },
        JobKind::Index,
    )?;
    let mut child = command()
        .arg("project-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let mut input = child.stdin.take().context("private input missing")?;
    let mut children = Children(vec![child]);
    service_ipc::write_frame(
        &mut input,
        &project_protocol::Init {
            runtime: root.clone(),
            project: project.binding,
            service: instance,
            worker: airk_kernel::service_broker::new_instance_id()?,
        },
    )?;
    drop(input);
    ensure!(
        !wait_exit(&mut children.0[0])?,
        "unactivated child completed normally"
    );
    ensure!(
        fs::read_dir(data)?.next().is_none(),
        "unactivated child opened collection storage"
    );
    ensure!(
        registry.job(job.id)?.state == JobState::Pending,
        "unactivated child claimed a job"
    );
    ensure!(
        fs::read(source.join("original.txt"))? == b"late child fixture",
        "source changed"
    );
    Ok(())
}

#[test]
fn private_worker_channel_renews_only_the_live_scoped_generation() -> Result<()> {
    use airk_cli::{
        project_protocol::{self, Command as Control, Reply},
        service_ipc,
    };
    use airk_store::service::registry::ServiceRegistry;
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    let source = owner.path().join("source");
    let data = owner.path().join("state");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    airk_cli::local_ipc::prepare_root(&root)?;
    let service = airk_kernel::service_broker::new_instance_id()?;
    let project = airk_kernel::service_projects::registration(&source, &data, "fixture")?;
    ServiceRegistry::open(&root, service)?.register(&project)?;
    let mut child = command()
        .arg("project-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut input = child.stdin.take().context("worker input missing")?;
    let mut output = child.stdout.take().context("worker output missing")?;
    let mut children = Children(vec![child]);
    let (sender, receiver) = std::sync::mpsc::sync_channel(4);
    let reader = std::thread::spawn(move || {
        while let Ok(Some(reply)) = service_ipc::read_frame::<_, Reply>(&mut output) {
            if sender.try_send(reply).is_err() {
                break;
            }
        }
    });
    let result = (|| -> Result<()> {
        service_ipc::write_frame(
            &mut input,
            &project_protocol::Init {
                runtime: root.clone(),
                project: project.binding,
                service,
                worker: airk_kernel::service_broker::new_instance_id()?,
            },
        )?;
        let Reply::Ready { identity, .. } = receiver.recv_timeout(Duration::from_secs(10))? else {
            anyhow::bail!("missing worker identity")
        };
        service_ipc::write_frame(
            &mut input,
            &Control::Activate {
                identity,
                allowance: airk_core::service::QueryAllowance {
                    concurrent_queries: 4,
                    response_bytes: 4 * airk_core::service::MAX_FRAME_BYTES,
                },
                pool: None,
            },
        )?;
        ensure!(
            matches!(
                receiver.recv_timeout(Duration::from_secs(10))?,
                Reply::Activated
            ),
            "worker activation failed"
        );
        service_ipc::write_frame(&mut input, &Control::Renew { identity })?;
        ensure!(
            matches!(
                receiver.recv_timeout(Duration::from_secs(5))?,
                Reply::Renewed
            ),
            "live worker did not renew"
        );
        let forged = airk_core::service::WorkerIdentity {
            fencing_generation: identity.fencing_generation + 1,
            ..identity
        };
        service_ipc::write_frame(&mut input, &Control::Renew { identity: forged })?;
        ensure!(
            matches!(
                receiver.recv_timeout(Duration::from_secs(5))?,
                Reply::Failed
            ),
            "worker accepted a foreign generation renewal"
        );
        Ok(())
    })();
    drop(input);
    if result.is_err() {
        let _ = children.0[0].kill();
    }
    let exited = wait_exit(&mut children.0[0])?;
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("private worker reader failed"))?;
    result?;
    ensure!(exited, "worker did not drain after control loss");
    Ok(())
}
