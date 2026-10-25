// SPDX-License-Identifier: Apache-2.0
//! Owned native-service fixtures; no installed runtime or user state is touched.
#![allow(dead_code)]
use airk_core::service::{
    ControlMethod as Method, ControlResponse as Response, JobKind, JobOutcome, JobRecord,
    ProjectRegistration, RequestKey,
};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, OnceLock, Weak},
    time::{Duration, Instant},
};

type Scope = (PathBuf, PathBuf, String);
static FIXTURES: OnceLock<Mutex<HashMap<Scope, Weak<Service>>>> = OnceLock::new();

pub struct Service {
    child: Mutex<Child>,
    owner: tempfile::TempDir,
    pub control: PathBuf,
    pub project: ProjectRegistration,
}

pub fn rpc(control: &Path, method: Method) -> Response {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(airk_cli::service_runtime::request(control, method))
        .unwrap()
}

pub fn native_command() -> Command {
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

impl Service {
    pub fn start(root: &Path, data: &Path, collection: &str) -> Arc<Self> {
        Self::start_inner(root, data, collection, false)
    }
    #[cfg(feature = "embedding-worker")]
    pub fn with_helper(root: &Path, data: &Path, collection: &str) -> Arc<Self> {
        Self::start_inner(root, data, collection, true)
    }
    fn start_inner(root: &Path, data: &Path, collection: &str, helper: bool) -> Arc<Self> {
        let owner = tempfile::tempdir().unwrap();
        Self::start_owned(root, data, collection, helper, owner)
    }
    pub fn in_owner(
        root: &Path,
        data: &Path,
        collection: &str,
        owner: tempfile::TempDir,
    ) -> Arc<Self> {
        Self::start_owned(root, data, collection, false, owner)
    }
    fn start_owned(
        root: &Path,
        data: &Path,
        collection: &str,
        helper: bool,
        owner: tempfile::TempDir,
    ) -> Arc<Self> {
        let control = owner.path().join("control");
        let project = airk_kernel::service_projects::registration(root, data, collection).unwrap();
        let log = std::fs::File::create(owner.path().join("stderr")).unwrap();
        let mut command = native_command();
        if helper {
            #[cfg(feature = "embedding-worker")]
            {
                let payload = owner.path().join("pair");
                std::fs::create_dir(&payload).unwrap();
                let main = airk_cli::runtime_install::main_name();
                let embed = Path::new(main).with_file_name(if cfg!(windows) {
                    "airk-embed.exe"
                } else {
                    "airk-embed"
                });
                std::fs::create_dir_all(payload.join(main).parent().unwrap()).unwrap();
                std::fs::copy(env!("CARGO_BIN_EXE_airk"), payload.join(main)).unwrap();
                std::fs::copy(env!("CARGO_BIN_EXE_airk-embed"), payload.join(&embed)).unwrap();
                let pair = serde_json::json!({"schema":1,"version":env!("CARGO_PKG_VERSION"),"os":std::env::consts::OS,"arch":std::env::consts::ARCH,
                    "airk_sha256":airk_cli::runtime_pair::digest_file(&payload.join(main)).unwrap(),
                    "embed_sha256":airk_cli::runtime_pair::digest_file(&payload.join(&embed)).unwrap()});
                std::fs::write(
                    payload.join("airk-runtime.json"),
                    serde_json::to_vec(&pair).unwrap(),
                )
                .unwrap();
                command = Command::new(payload.join(main));
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x08000000);
                }
            }
            #[cfg(not(feature = "embedding-worker"))]
            panic!("helper fixture requires embedding-worker feature");
        }
        let child = command
            .arg("serve")
            .arg("--runtime-dir")
            .arg(&control)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .unwrap();
        // Install the guard immediately so failed assertions still reap the owned child.
        let service = Arc::new(Self {
            child: Mutex::new(child),
            owner,
            control,
            project,
        });
        let until = Instant::now() + Duration::from_secs(60);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        loop {
            if matches!(
                runtime.block_on(airk_cli::service_runtime::request(
                    &service.control,
                    Method::Health
                )),
                Ok(Response::Health { .. })
            ) {
                break;
            }
            assert!(
                Instant::now() < until,
                "native fixture startup failed: {}",
                std::fs::read_to_string(service.owner.path().join("stderr")).unwrap_or_default()
            );
            assert!(
                service.child.lock().unwrap().try_wait().unwrap().is_none(),
                "fixture service exited"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(matches!(
            service.request(Method::Register {
                project: service.project.clone()
            }),
            Response::Registered { .. }
        ));
        FIXTURES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .insert(service.scope(), Arc::downgrade(&service));
        service
    }

    fn scope(&self) -> Scope {
        (
            PathBuf::from(&self.project.root),
            PathBuf::from(&self.project.data_root),
            self.project.collection.clone(),
        )
    }
    pub fn request(&self, method: Method) -> Response {
        rpc(&self.control, method)
    }
    pub fn key(&self) -> RequestKey {
        let Response::RequestEpoch { namespace, epoch } = self.request(Method::RequestEpoch) else {
            panic!("request epoch unavailable")
        };
        RequestKey {
            namespace,
            epoch,
            nonce: airk_kernel::service_broker::new_instance_id().unwrap(),
        }
    }
    pub fn wait(&self, id: airk_core::identity::Sha256Digest) -> JobRecord {
        let job = self.wait_outcome(id);
        assert!(
            matches!(job.outcome, Some(JobOutcome::Succeeded { .. })),
            "native job failed: {job:?}"
        );
        job
    }
    pub fn wait_outcome(&self, id: airk_core::identity::Sha256Digest) -> JobRecord {
        let until = Instant::now() + Duration::from_secs(90);
        loop {
            let Response::Job { job } = self.request(Method::Job { id }) else {
                panic!("job receipt unavailable")
            };
            if job.outcome.is_some() {
                return job;
            }
            assert!(Instant::now() < until, "native job timed out: {job:?}");
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    pub fn job(&self, kind: JobKind) -> JobRecord {
        let Response::Job { job } = self.request(Method::Enqueue {
            project: self.project.binding,
            caller: airk_kernel::service_broker::new_instance_id().unwrap(),
            key: self.key(),
            kind,
        }) else {
            panic!("job not accepted")
        };
        self.wait(job.id)
    }
    #[cfg(feature = "embedding-worker")]
    pub fn wait_semantic(&self, artifacts: u64) -> anyhow::Result<serde_json::Value> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let connection =
                    airk_cli::project_client::tools(self.control.clone(), self.project.binding);
                let deadline = Instant::now() + Duration::from_secs(90);
                loop {
                    let envelope = connection
                        .call(
                            "airk_status".into(),
                            serde_json::json!({"response_mode":"full"})
                                .as_object()
                                .unwrap()
                                .clone(),
                        )
                        .await?;
                    anyhow::ensure!(envelope["isError"] != true, "{envelope}");
                    let status: serde_json::Value =
                        serde_json::from_str(envelope["content"][0]["text"].as_str().unwrap())?;
                    let readiness = &status["semantic_readiness"];
                    if readiness["state"] == "ready"
                        && readiness["completed_artifacts"] == artifacts
                    {
                        return Ok(status);
                    }
                    anyhow::ensure!(
                        Instant::now() < deadline,
                        "semantic work did not complete: {status}"
                    );
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
    }
    pub fn configure(&self, draft: airk_cli::service_configuration::Draft) {
        self.job(JobKind::Inspect);
        let request = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(airk_cli::service_configuration::prepare(
                &self.control,
                self.project.binding,
                draft,
            ))
            .unwrap();
        let Response::Job { job } = self.request(Method::ConfigureProject {
            project: self.project.binding,
            caller: airk_kernel::service_broker::new_instance_id().unwrap(),
            key: self.key(),
            request: Box::new(request),
        }) else {
            panic!("configuration not accepted")
        };
        self.wait(job.id);
    }

    pub fn repair(&self, operation: airk_core::service_maintenance::RepairOperation) {
        use airk_core::service_maintenance::{PlanPhase, RepairRequest, RepairSelection};
        let Response::Plan { plan } = self.request(Method::PlanCreate {
            caller: airk_kernel::service_broker::new_instance_id().unwrap(),
            key: self.key(),
            request: RepairRequest {
                selection: RepairSelection::Selected(vec![self.project.binding]),
                operation,
            },
        }) else {
            panic!("repair preview not accepted")
        };
        let until = Instant::now() + Duration::from_secs(90);
        let preview = loop {
            let Response::Plan { plan } = self.request(Method::Plan { id: plan.id }) else {
                panic!("preview lost")
            };
            if plan.phase == PlanPhase::Ready {
                break plan;
            }
            assert!(Instant::now() < until, "preview did not settle: {plan:?}");
            std::thread::sleep(Duration::from_millis(25));
        };
        assert!(matches!(
            self.request(Method::PlanExecute {
                id: preview.id,
                preview: preview.preview_digest.unwrap(),
                allow_fallback: false
            }),
            Response::Plan { .. }
        ));
        loop {
            let Response::Plan { plan } = self.request(Method::Plan { id: preview.id }) else {
                panic!("repair receipt lost")
            };
            if plan.phase == PlanPhase::Finished {
                assert_eq!(
                    plan.outcome,
                    Some(airk_core::service_maintenance::PlanOutcome::Succeeded),
                    "{plan:?}"
                );
                break;
            }
            assert!(Instant::now() < until, "repair did not settle: {plan:?}");
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

pub fn bind_data(control: &Path, data: &Path) -> anyhow::Result<()> {
    airk_cli::local_ipc::prepare_root(control)?;
    let registry = airk_store::service::registry::ServiceRegistry::open(
        control,
        airk_kernel::service_broker::new_instance_id()?,
    )?;
    airk_store::service::data_scope::bind(data, control, registry.incarnation()?)?;
    Ok(())
}

impl Drop for Service {
    fn drop(&mut self) {
        if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            let _ = runtime.block_on(airk_cli::service_runtime::request(
                &self.control,
                Method::Shutdown,
            ));
        }
        let child = self.child.get_mut().unwrap();
        let until = Instant::now() + Duration::from_secs(15);
        loop {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            if Instant::now() >= until {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

pub fn command(root: &Path, data: &Path, collection: &str) -> Command {
    let mut command = native_command();
    command
        .arg("--root")
        .arg(root)
        .arg("--data-dir")
        .arg(data)
        .args(["--collection", collection])
        .env("AIRK_MCP_RESPONSE_MODE", "full")
        .env("AIRK_USAGE_MODE", "disabled");
    let scope = (
        root.canonicalize().unwrap_or_else(|_| root.into()),
        data.canonicalize().unwrap_or_else(|_| data.into()),
        collection.into(),
    );
    let service = FIXTURES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .get(&scope)
        .and_then(Weak::upgrade);
    if let Some(service) = service {
        command
            .arg("--service-runtime-dir")
            .arg(&service.control)
            .arg("--service-project")
            .arg(service.project.binding.to_string());
    } else {
        // Metadata-only help must work without attempting the real user installation.
        command
            .arg("--service-install-dir")
            .arg(data.join("not-an-installed-runtime"));
    }
    command
}

pub fn call(
    root: &Path,
    data: &Path,
    collection: &str,
    args: &[&str],
) -> anyhow::Result<serde_json::Value> {
    let deadline = Instant::now() + Duration::from_secs(60);
    let output = loop {
        let output = command(root, data, collection).args(args).output()?;
        // Evidence commands are read-only. A typed rejection repeats these exact
        // arguments (including revision/cursor); never retry an accepted mutation.
        let read = args.starts_with(&["tools", "call"])
            || matches!(
                args.first(),
                Some(
                    &("status"
                        | "search"
                        | "files"
                        | "read"
                        | "info"
                        | "outline"
                        | "symbol"
                        | "imports"
                        | "facts"
                        | "refs"
                        | "context"
                        | "changes"
                        | "code-facts"
                        | "relationships")
                )
            );
        let delay = if read {
            serde_json::from_slice::<serde_json::Value>(&output.stdout)
                .ok()
                .filter(|envelope| envelope["isError"] == true)
                .and_then(|envelope| {
                    serde_json::from_str::<serde_json::Value>(
                        envelope["content"][0]["text"].as_str()?,
                    )
                    .ok()
                })
                .filter(|hint| {
                    (hint["status"] == "retry"
                        && hint["retryable"] == true
                        && hint["action"] == "repeat_same_request"
                        && matches!(
                            hint["code"].as_str(),
                            Some("publication_busy" | "selection_busy")
                        ))
                        || (matches!(
                            hint["error"]["code"].as_str(),
                            Some("direct_query_admission" | "direct_query_setup")
                        ) && hint["error"]["reason"] == "busy"
                            && hint["error"]["query_submitted"] == false
                            && hint["error"]["retryable"] == true)
                })
                .and_then(|hint| {
                    hint["retry_after_ms"]
                        .as_u64()
                        .or_else(|| hint["error"]["retry_after_ms"].as_u64())
                })
                .filter(|delay| (1..=1000).contains(delay))
                .map(Duration::from_millis)
        } else {
            None
        };
        if let Some(delay) = delay
            && Instant::now() + delay < deadline
        {
            std::thread::sleep(delay);
            continue;
        }
        break output;
    };
    if !output.status.success() {
        let scope = (
            root.canonicalize()?,
            data.canonicalize()?,
            collection.into(),
        );
        let service = FIXTURES
            .get()
            .and_then(|fixtures| fixtures.lock().ok()?.get(&scope)?.upgrade());
        if let Some(service) = service {
            use std::io::Read;
            if let Ok(file) = std::fs::File::open(service.owner.path().join("stderr")) {
                let mut text = String::new();
                if file.take(16384).read_to_string(&mut text).is_ok() {
                    eprintln!("native fixture child diagnostic: {text}");
                }
            }
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            for method in [
                Method::Resources,
                Method::Workers {
                    projects: vec![service.project.binding],
                },
            ] {
                let observation =
                    runtime.block_on(airk_cli::service_runtime::request(&service.control, method));
                if let Ok(observation) = observation {
                    eprintln!(
                        "native fixture failure observation: {}",
                        serde_json::to_string(&observation)?
                    );
                }
            }
        }
    }
    anyhow::ensure!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    if args == ["index"] {
        let scope = (
            root.canonicalize()?,
            data.canonicalize()?,
            collection.into(),
        );
        let service = FIXTURES
            .get()
            .unwrap()
            .lock()
            .unwrap()
            .get(&scope)
            .and_then(Weak::upgrade)
            .unwrap();
        service.wait(serde_json::from_value(value["job"]["id"].clone())?);
    }
    if args.first() != Some(&"tools") && value.get("content").is_some() {
        anyhow::ensure!(value["isError"] != true, "{value}");
        return Ok(serde_json::from_str(
            value["content"][0]["text"].as_str().unwrap(),
        )?);
    }
    Ok(value)
}

pub fn configure_cli(
    service: &Service,
    root: &Path,
    data: &Path,
    collection: &str,
    draft: serde_json::Value,
) -> anyhow::Result<JobRecord> {
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(&serde_json::to_vec(&draft)?)?;
    let output = command(root, data, collection)
        .arg("configure")
        .arg("--file")
        .arg(file.path())
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    Ok(service.wait_outcome(serde_json::from_value(value["job"]["id"].clone())?))
}
