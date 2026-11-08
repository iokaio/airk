// SPDX-License-Identifier: Apache-2.0
//! Real worker controls/query owners and real tiny-model inference; only Ready is gated.
use super::common as preparation;
use super::*;
use airk_cli::{
    embedding_pool_protocol as pool, local_ipc, project_protocol as worker, service_ipc,
};
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{ControlMethod, ControlResponse, JobKind, ProjectRegistration, WorkerIdentity},
};
use airk_retrieval::embedding_worker::Request;
use std::{
    process::ChildStdin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
};

fn child_diagnostic(child: &mut Child, log: &Path, stage: &str) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let exit = match child.try_wait() {
        Ok(value) => format!("{value:?}"),
        Err(error) => format!("unavailable: {error}"),
    };
    let tail = (|| -> std::io::Result<String> {
        let mut file = fs::File::open(log)?;
        let length = file.metadata()?.len();
        file.seek(SeekFrom::Start(length.saturating_sub(4096)))?;
        let mut bytes = Vec::new();
        file.take(4096).read_to_end(&mut bytes)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    })();
    format!(
        "{stage}; pid={}; exit={exit}; stderr_tail={tail:?}",
        child.id()
    )
}
struct ControlledChild {
    child: Child,
    stderr: std::path::PathBuf,
    input: Option<ChildStdin>,
    replies: mpsc::Receiver<worker::Reply>,
    reader: Option<thread::JoinHandle<()>>,
    identity: WorkerIdentity,
    role: String,
}
impl Drop for ControlledChild {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
impl ControlledChild {
    fn start(
        runtime: &Path,
        project: &ProjectRegistration,
        service: Incarnation,
        pending: bool,
        stderr: &Path,
    ) -> Result<Self> {
        let log = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stderr)?;
        let child = command()
            .arg("project-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log))
            .spawn()?;
        let (send, replies) = mpsc::sync_channel(8);
        let placeholder = WorkerIdentity {
            service,
            worker: service,
            project: project.binding,
            fencing_generation: 0,
        };
        let mut owned = Self {
            child,
            stderr: stderr.into(),
            input: None,
            replies,
            reader: None,
            identity: placeholder,
            role: String::new(),
        };
        owned.input = Some(owned.child.stdin.take().context("worker stdin")?);
        let mut output = owned.child.stdout.take().context("worker stdout")?;
        owned.reader = Some(
            thread::Builder::new()
                .name("held-worker-replies".into())
                .spawn(move || {
                    while let Ok(Some(reply)) =
                        service_ipc::read_frame::<_, worker::Reply>(&mut output)
                    {
                        if send.try_send(reply).is_err() {
                            break;
                        }
                    }
                })?,
        );
        service_ipc::write_frame(
            owned.input.as_mut().unwrap(),
            &worker::Init {
                engine: Sha256Digest::from_bytes([0; 32]),
                runtime: runtime.into(),
                project: project.binding,
                service,
                worker: airk_kernel::service_broker::new_instance_id()?,
            },
        )?;
        let worker::Reply::Ready { identity, role } =
            owned.receive(Duration::from_secs(15), "worker initial Ready")?
        else {
            anyhow::bail!(
                "{}",
                child_diagnostic(&mut owned.child, &owned.stderr, "worker Ready missing")
            )
        };
        owned.identity = identity;
        owned.role = role;
        owned.exchange(
            &worker::Command::Activate {
                identity,
                allowance: airk_core::service::QueryAllowance {
                    concurrent_queries: 1,
                    response_bytes: airk_core::service::MAX_FRAME_BYTES,
                },
                pool: None,
                pool_pending: pending,
            },
            |r| matches!(r, worker::Reply::Activated),
        )?;
        Ok(owned)
    }
    fn receive(&mut self, timeout: Duration, stage: &str) -> Result<worker::Reply> {
        self.replies
            .recv_timeout(timeout)
            .with_context(|| child_diagnostic(&mut self.child, &self.stderr, stage))
    }
    fn exchange(
        &mut self,
        request: &worker::Command,
        check: impl FnOnce(&worker::Reply) -> bool,
    ) -> Result<worker::Reply> {
        service_ipc::write_frame(self.input.as_mut().context("closed worker input")?, request)
            .with_context(|| {
                child_diagnostic(&mut self.child, &self.stderr, "worker control write")
            })?;
        let reply = self.receive(Duration::from_secs(5), "worker control reply")?;
        ensure!(
            check(&reply),
            "{}",
            child_diagnostic(
                &mut self.child,
                &self.stderr,
                "unexpected worker control reply"
            )
        );
        Ok(reply)
    }
    fn renew(&mut self) -> Result<()> {
        self.exchange(
            &worker::Command::Renew {
                identity: self.identity,
            },
            |r| matches!(r, worker::Reply::Renewed),
        )?;
        Ok(())
    }
    fn close_input(&mut self) {
        self.input.take();
    }
    fn wait_clean(&mut self) -> Result<()> {
        let until = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(status) = self.child.try_wait()? {
                ensure!(
                    status.success(),
                    "{}",
                    child_diagnostic(&mut self.child, &self.stderr, "worker failed exit")
                );
                return Ok(());
            }
            ensure!(
                Instant::now() < until,
                "{}",
                child_diagnostic(
                    &mut self.child,
                    &self.stderr,
                    "worker drain exceeded fixture watchdog"
                )
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}
async fn submit(
    runtime: &Path,
    child: &mut ControlledChild,
    mode: &str,
) -> Result<tokio::task::JoinHandle<Result<worker::QueryResult>>> {
    let caller = local_ipc::caller_identity()?;
    let worker::Reply::Grant { grant } = child
        .exchange(&worker::Command::Grant { caller }, |r| {
            matches!(r, worker::Reply::Grant { .. })
        })?
    else {
        unreachable!()
    };
    let mut stream = tokio::time::timeout(
        Duration::from_secs(5),
        local_ipc::connect(&local_ipc::endpoint(runtime, &child.role)?),
    )
    .await??;
    local_ipc::write_message(&mut stream, &grant, Duration::from_secs(5)).await?;
    let accepted: worker::Accepted = local_ipc::read_message(&mut stream, Duration::from_secs(5))
        .await?
        .context("query handshake EOF")?;
    ensure!(
        accepted.identity == child.identity,
        "query identity crossed"
    );
    local_ipc::write_message(
        &mut stream,
        &worker::Query {
            read_attempt: None,
            frontend: None,
            tool: "airk_search".into(),
            arguments:
                serde_json::json!({"query":"authenticate login","mode":mode,"response_mode":"full"})
                    .as_object()
                    .unwrap()
                    .clone(),
            native: None,
            status_receipt: Default::default(),
        },
        Duration::from_secs(5),
    )
    .await?;
    Ok(tokio::spawn(async move {
        local_ipc::read_message(&mut stream, Duration::from_secs(15))
            .await?
            .context("query response EOF")
    }))
}
fn body(reply: &worker::QueryResult) -> Result<Value> {
    Ok(serde_json::from_str(
        reply.result["content"][0]["text"]
            .as_str()
            .context("query text missing")?,
    )?)
}

struct RealHelper {
    child: Child,
    stderr: std::path::PathBuf,
    input: Option<ChildStdin>,
    replies: mpsc::Receiver<pool::Observation>,
    reader: Option<thread::JoinHandle<()>>,
}
impl Drop for RealHelper {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
impl RealHelper {
    fn start(
        runtime: &Path,
        service: Incarnation,
        identity: Incarnation,
        stderr: &Path,
    ) -> Result<Self> {
        let log = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stderr)?;
        let mut command = Command::new(env!("CARGO_BIN_EXE_airk-embed"));
        command
            .arg("pool")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let child = command.spawn()?;
        let (send, replies) = mpsc::sync_channel(8);
        let mut owned = Self {
            child,
            stderr: stderr.into(),
            input: None,
            replies,
            reader: None,
        };
        owned.input = Some(owned.child.stdin.take().context("helper stdin")?);
        let mut output = owned.child.stdout.take().context("helper stdout")?;
        owned.reader = Some(
            thread::Builder::new()
                .name("held-helper-replies".into())
                .spawn(move || {
                    while let Ok(Some(reply)) =
                        service_ipc::read_frame::<_, pool::Observation>(&mut output)
                    {
                        if send.try_send(reply).is_err() {
                            break;
                        }
                    }
                })?,
        );
        owned.control(&pool::Init {
            runtime: runtime.into(),
            service,
            pool: identity,
            inference_threads: 1,
        })?;
        let ready = owned.receive("helper initial Ready")?;
        ensure!(
            matches!(ready,pool::Observation::Ready{pool,role} if pool==identity&&role==pool::role(identity)),
            "{}",
            child_diagnostic(&mut owned.child, &owned.stderr, "helper Ready crossed")
        );
        Ok(owned)
    }
    fn receive(&mut self, stage: &str) -> Result<pool::Observation> {
        self.replies
            .recv_timeout(Duration::from_secs(5))
            .with_context(|| child_diagnostic(&mut self.child, &self.stderr, stage))
    }
    fn control(&mut self, value: &impl serde::Serialize) -> Result<()> {
        service_ipc::write_frame(self.input.as_mut().context("helper input closed")?, value)
            .with_context(|| {
                child_diagnostic(&mut self.child, &self.stderr, "helper control write")
            })
    }
    fn register(&mut self, grant: pool::Grant) -> Result<()> {
        self.control(&pool::Control::Register {
            grant,
            worker_identity: None,
        })?;
        ensure!(
            matches!(
                self.receive("helper Register reply")?,
                pool::Observation::Registered
            ),
            "{}",
            child_diagnostic(&mut self.child, &self.stderr, "helper grant refused")
        );
        Ok(())
    }
    fn no_model_loaded(&mut self) -> Result<()> {
        self.control(&pool::Control::Status)?;
        ensure!(
            matches!(
                self.receive("helper Status reply")?,
                pool::Observation::Status { loads: 0, .. }
            ),
            "{}",
            child_diagnostic(
                &mut self.child,
                &self.stderr,
                "model opened before Ready release"
            )
        );
        Ok(())
    }
}
struct ProxyTask(Option<tokio::task::JoinHandle<Result<()>>>);
impl Drop for ProxyTask {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}
async fn proxy(
    runtime: &Path,
    advertised: pool::Grant,
    real: pool::Grant,
    entered: tokio::sync::oneshot::Sender<()>,
    release: tokio::sync::oneshot::Receiver<()>,
    operations: Arc<AtomicUsize>,
    embeds: Arc<AtomicUsize>,
) -> Result<ProxyTask> {
    let runtime = runtime.to_path_buf();
    let role = pool::role(advertised.pool);
    let mut listener = local_ipc::Listener::bind(&local_ipc::endpoint(&runtime, &role)?)?;
    let task = tokio::spawn(async move {
        let mut front = tokio::time::timeout(Duration::from_secs(10), listener.accept()).await??;
        let received: pool::Grant = local_ipc::read_message(&mut front, Duration::from_secs(5))
            .await?
            .context("proxy grant EOF")?;
        ensure!(
            serde_json::to_vec(&received)? == serde_json::to_vec(&advertised)?,
            "proxy advertised grant crossed"
        );
        let mut back = tokio::time::timeout(
            Duration::from_secs(5),
            local_ipc::connect(&local_ipc::endpoint(&runtime, &pool::role(real.pool))?),
        )
        .await??;
        local_ipc::write_message(&mut back, &real, Duration::from_secs(5)).await?;
        let ready: pool::Observation = local_ipc::read_message(&mut back, Duration::from_secs(5))
            .await?
            .context("real helper Ready EOF")?;
        ensure!(
            matches!(ready,pool::Observation::Ready{pool,role} if pool==real.pool&&role==pool::role(real.pool)),
            "real helper Ready crossed"
        );
        let _ = entered.send(());
        // No operation is forwarded until explicit Ready release. EOF can settle
        // this test-owned proxy without publishing authority after owner retirement.
        tokio::select! {
            released=release=>{released.context("Ready release abandoned")?;},
            early=local_ipc::read_message::<_,pool::Envelope>(&mut front,Duration::from_secs(10))=>{ensure!(early?.is_none(),"operation arrived before Ready");return Ok(())},
        }
        local_ipc::write_message(
            &mut front,
            &pool::Observation::Ready {
                pool: advertised.pool,
                role: pool::role(advertised.pool),
            },
            Duration::from_secs(5),
        )
        .await?;
        while let Some(envelope) = local_ipc::read_message_limited::<_, pool::Envelope>(
            &mut front,
            Duration::from_secs(10),
            airk_retrieval::embedding_worker::MAX_FRAME_BYTES,
        )
        .await?
        {
            ensure!(
                envelope.project == advertised.project.binding
                    && envelope.worker == advertised.worker,
                "proxy operation crossed worker"
            );
            operations.fetch_add(1, Ordering::SeqCst);
            if matches!(&envelope.operation,pool::Operation::Call{request} if matches!(request.as_ref(),Request::Embed{..}))
            {
                embeds.fetch_add(1, Ordering::SeqCst);
            }
            local_ipc::write_message_limited(
                &mut back,
                &envelope,
                Duration::from_secs(5),
                airk_retrieval::embedding_worker::MAX_FRAME_BYTES,
            )
            .await?;
            let result: pool::ResultEnvelope = local_ipc::read_message_limited(
                &mut back,
                Duration::from_secs(10),
                airk_retrieval::embedding_worker::MAX_FRAME_BYTES,
            )
            .await?
            .context("real inference EOF")?;
            ensure!(
                result.project == envelope.project
                    && result.worker == envelope.worker
                    && result.session == envelope.session
                    && result.request_id == envelope.request_id
                    && result.input_digest == envelope.input_digest,
                "real inference correlation crossed"
            );
            local_ipc::write_message_limited(
                &mut front,
                &result,
                Duration::from_secs(5),
                airk_retrieval::embedding_worker::MAX_FRAME_BYTES,
            )
            .await?;
        }
        Ok(())
    });
    Ok(ProxyTask(Some(task)))
}

#[test]
fn real_semantic_wait_keeps_worker_controls_and_other_project_reads_responsive_then_eof_drains()
-> Result<()> {
    use sha2::{Digest, Sha256};
    for retire in [false, true] {
        let owner = tempfile::tempdir()?;
        let source = owner.path().join("source");
        let data = owner.path().join("data");
        let model = owner.path().join("tiny-model");
        let other_source = owner.path().join("other-source");
        let other_data = owner.path().join("other-data");
        for directory in [&source, &data, &model, &other_source, &other_data] {
            fs::create_dir(directory)?;
        }
        let original = b"authenticate login\n";
        fs::write(source.join("memo.txt"), original)?;
        fs::write(other_source.join("memo.txt"), original)?;
        let vocabulary = br#"{"authenticate":[1.0,0.0],"login":[0.0,1.0]}"#;
        fs::write(model.join("vocabulary.json"), vocabulary)?;
        fs::write(
            model.join("model.json"),
            serde_json::to_vec(
                &serde_json::json!({"schema":1,"name":"original held authority fixture","license":"Apache-2.0","sha256":format!("{:x}",Sha256::digest(vocabulary)),"dimensions":2}),
            )?,
        )?;
        let prepared = preparation::Service::with_helper(&source, &data, "semantic");
        ensure!(
            matches!(prepared.request(ControlMethod::ConfigureWatch{project:prepared.project.binding,enabled:false}),ControlResponse::Watch{observation,..} if !observation.enabled),
            "preparation watch was not disabled"
        );
        prepared.configure(airk_cli::service_configuration::Draft::InstallModel {
            directory: model.clone(),
        });
        prepared.job(JobKind::Index);
        let saved = prepared.wait_semantic(1)?;
        let control = prepared.control.clone();
        let project = prepared.project.clone();
        let ControlResponse::Health { observation } = prepared.request(ControlMethod::Health)
        else {
            anyhow::bail!("preparation health missing")
        };
        let service = observation.instance;
        let registry = airk_store::service::registry::ServiceRegistry::open_saved(&control)?;
        airk_store::service::data_scope::bind(&other_data, &control, registry.incarnation()?)?;
        drop(registry);
        let other =
            airk_kernel::service_projects::registration(&other_source, &other_data, "lexical")?;
        let lexical = airk_kernel::Kernel::open_service_collection(
            &other_source,
            Some(other_data.clone()),
            "lexical",
        )?;
        lexical.configure_vectors(false)?;
        lexical.index()?;
        drop(lexical);
        ensure!(
            matches!(
                prepared.request(ControlMethod::Register {
                    project: other.clone()
                }),
                ControlResponse::Registered { .. }
            ),
            "other fixture registration refused"
        );
        ensure!(
            matches!(
                prepared.request(ControlMethod::Shutdown),
                ControlResponse::Stopping
            ),
            "preparation shutdown refused"
        );
        let until = Instant::now() + Duration::from_secs(15);
        let _lease = loop {
            match airk_store::service::ServiceLease::acquire(&control) {
                Ok(lease) => break lease,
                Err(_) if Instant::now() < until => thread::sleep(Duration::from_millis(10)),
                Err(error) => return Err(error),
            }
        };
        let mut first = ControlledChild::start(
            &control,
            &project,
            service,
            true,
            &owner.path().join("semantic-worker.stderr"),
        )?;
        let mut second = ControlledChild::start(
            &control,
            &other,
            service,
            false,
            &owner.path().join("lexical-worker.stderr"),
        )?;
        let real_id = airk_kernel::service_broker::new_instance_id()?;
        let advertised_id = airk_kernel::service_broker::new_instance_id()?;
        let mut helper = RealHelper::start(
            &control,
            service,
            real_id,
            &owner.path().join("real-helper.stderr"),
        )?;
        let advertised = pool::Grant {
            service,
            pool: advertised_id,
            project: project.clone(),
            worker: first.identity.worker,
            token: Sha256Digest::from_bytes([73; 32]),
        };
        let real = pool::Grant {
            pool: real_id,
            ..advertised.clone()
        };
        helper.register(real.clone())?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let operations = Arc::new(AtomicUsize::new(0));
            let embeds = Arc::new(AtomicUsize::new(0));
            let (entered, held) = tokio::sync::oneshot::channel();
            let (release, wait) = tokio::sync::oneshot::channel();
            let mut proxy = proxy(
                &control,
                advertised.clone(),
                real,
                entered,
                wait,
                operations.clone(),
                embeds.clone(),
            )
            .await?;
            let mut semantic = submit(&control, &mut first, "vector").await?;
            // Exact semantic frame is submitted, no warm-up/read retry. This is
            // not an instruction-level witness for the private None+pending wait.
            first.renew()?;
            let lexical = submit(&control, &mut second, "lexical").await?;
            let lexical = tokio::time::timeout(Duration::from_secs(5), lexical).await???;
            ensure!(
                lexical.result["isError"] != true
                    && !body(&lexical)?["hits"]
                        .as_array()
                        .context("lexical hits missing")?
                        .is_empty(),
                "other project lexical blocked/failed"
            );
            ensure!(
                !semantic.is_finished(),
                "first semantic request completed before authority publication"
            );
            let access = pool::Access {
                runtime: control.clone(),
                role: pool::role(advertised_id),
                grant: advertised,
                in_process: false,
            };
            first.exchange(
                &worker::Command::ConfigurePool {
                    identity: first.identity,
                    pool: Some(Box::new(access)),
                    pool_pending: false,
                },
                |r| matches!(r, worker::Reply::PoolConfigured),
            )?;
            tokio::time::timeout(Duration::from_secs(5), held).await??;
            first.renew()?;
            helper.no_model_loaded()?;
            ensure!(
                operations.load(Ordering::SeqCst) == 0 && embeds.load(Ordering::SeqCst) == 0,
                "model operation preceded Ready"
            );
            let busy = submit(&control, &mut first, "lexical").await?;
            let busy = tokio::time::timeout(Duration::from_secs(5), busy).await???;
            ensure!(
                busy.result["isError"] == true
                    && body(&busy)?["error"]["code"] == "direct_query_admission",
                "held semantic permit not observed"
            );
            if retire {
                first.close_input();
                first.wait_clean()?;
                let outcome = tokio::time::timeout(Duration::from_secs(2), &mut semantic).await?;
                let refused = match outcome {
                    Err(_) => true,
                    Ok(Err(_)) => true,
                    Ok(Ok(reply)) => reply.result["isError"] == true,
                };
                ensure!(refused, "retired semantic request falsely succeeded");
                drop(release);
                ensure!(
                    operations.load(Ordering::SeqCst) == 0,
                    "retirement submitted model work"
                );
            } else {
                release
                    .send(())
                    .map_err(|_| anyhow::anyhow!("Ready gate closed"))?;
                let exact =
                    tokio::time::timeout(Duration::from_secs(10), &mut semantic).await???;
                ensure!(
                    exact.result["isError"] != true,
                    "first semantic query failed without retry: {}",
                    exact.result
                );
                let value = body(&exact)?;
                ensure!(
                    value["publication"]["token"] == saved["publication"]["token"]
                        && !value["hits"]
                            .as_array()
                            .context("semantic hits missing")?
                            .is_empty(),
                    "real semantic publication/results changed"
                );
                ensure!(
                    embeds.load(Ordering::SeqCst) > 0,
                    "real helper inference was not observed"
                );
                first.close_input();
                first.wait_clean()?;
            }
            if let Some(task) = proxy.0.as_mut() {
                tokio::time::timeout(Duration::from_secs(10), task).await???;
            }
            proxy.0.take();
            second.close_input();
            second.wait_clean()?;
            Ok::<_, anyhow::Error>(())
        })?;
        ensure!(
            fs::read(source.join("memo.txt"))? == original
                && fs::read(other_source.join("memo.txt"))? == original,
            "original source changed"
        );
    }
    Ok(())
}
