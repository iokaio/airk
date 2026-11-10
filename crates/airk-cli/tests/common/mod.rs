// SPDX-License-Identifier: Apache-2.0
//! Owned native-service fixtures. Raw owner/admin fixtures exercise component
//! transports; reviewed fixtures use a real private managed installation and
//! canonical catalog setup. Neither qualifies release signatures or predecessors.
#![allow(dead_code)]
pub mod http;
pub mod read_retry;
mod reviewed;
use airk_core::service::{
    ControlMethod as Method, ControlResponse as Response, JobKind, JobOutcome, JobRecord,
    ProjectRegistration, RequestKey,
};
use anyhow::Context;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, OnceLock, Weak},
    time::{Duration, Instant},
};

type Scope = (PathBuf, PathBuf, String);
static FIXTURES: OnceLock<Mutex<HashMap<Scope, Weak<Service>>>> = OnceLock::new();

fn diagnostic(stage: &str) {
    if std::env::var_os("AIRK_PROCESS_DIAGNOSTICS").is_some() {
        eprintln!("airk-service fixture: {stage}");
    }
}

pub struct Service {
    child: Mutex<Option<Child>>,
    reviewed: Option<reviewed::ReviewedService>,
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

/// Failure-only evidence from this test's owned native runtime. Preserve the
/// complete canonical result; raw details stay local and are filtered by job.
pub fn job_failure_diagnostics(control: &Path, job: &JobRecord) -> serde_json::Value {
    struct BoundedCapture(Vec<u8>);
    impl std::io::Write for BoundedCapture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.0.len() + bytes.len() > (1 << 20) {
                return Err(std::io::Error::other(
                    "fixture raw diagnostic capture exceeds 1 MiB",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let observe = |method| match &runtime {
        Ok(runtime) => {
            match runtime.block_on(airk_cli::service_runtime::request(control, method)) {
                Ok(response) => serde_json::to_value(response).unwrap(),
                Err(error) => serde_json::json!({"diagnostic_rpc_error": format!("{error:#}")}),
            }
        }
        Err(error) => serde_json::json!({"diagnostic_runtime_error": error.to_string()}),
    };
    let configuration = observe(Method::ConfigurationResult { id: job.id });
    let worker = observe(Method::Worker {
        project: job.project,
    });
    let resources = observe(Method::Resources);
    let job_id = serde_json::to_value(job.id).unwrap();
    let mut exact_raw = None;
    let mut capture_error = None;
    for attempt in 0..3 {
        let mut capture = BoundedCapture(Vec::new());
        capture_error = airk_cli::service_logs::export_raw_to(control, &mut capture)
            .err()
            .map(|error| error.to_string());
        // Each raw record is independently bounded by the native producer.
        // Never print another job's paths/values or an unparsed partial record.
        for line in capture.0.split(|byte| *byte == b'\n') {
            if line.len() <= 64 << 10
                && let Ok(value) = serde_json::from_slice::<serde_json::Value>(line)
                && value["failure"]["job"] == job_id
            {
                exact_raw = Some(value);
            }
        }
        if exact_raw.is_some() || capture_error.is_some() || attempt == 2 {
            break;
        }
        // Worker status/log delivery can follow the terminal metadata receipt.
        std::thread::sleep(Duration::from_millis(50));
    }
    serde_json::json!({
        "configuration_result": configuration,
        "worker": worker,
        "resources": resources,
        "raw_job_error": exact_raw,
        "raw_capture_error": capture_error,
    })
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

/// Uses the same sealed private payload and actual native Genesis activation as
/// reviewed evidence fixtures, without constructing a background fixture owner.
pub fn installed_runtime_fixture(owner: &Path) -> anyhow::Result<(PathBuf, PathBuf, PathBuf)> {
    reviewed::installed_runtime(owner)
}

pub fn setup_fixture_project(
    control: &Path,
    root: &Path,
    data: &Path,
    label: &str,
) -> anyhow::Result<ProjectRegistration> {
    reviewed::setup_project(control, root, data, label)
}

impl Service {
    /// Fresh canonical catalog fixture, approved only by its exact local digest.
    /// Existing indexed data must use an explicit retained-state test journey.
    pub fn reviewed(root: &Path, data: &Path, collection: &str) -> Arc<Self> {
        let mut owner = tempfile::tempdir().unwrap();
        // Activation can own a service even if a later setup assertion fails.
        // Only exact lifecycle settlement below authorizes directory cleanup.
        owner.disable_cleanup(true);
        let managed =
            reviewed::start(root, data, collection, owner.path()).unwrap_or_else(|error| {
                panic!(
                    "reviewed fixture failed; retained {}: {error:#}",
                    owner.path().display()
                )
            });
        let requested = (
            root.canonicalize().unwrap(),
            data.canonicalize().unwrap(),
            collection.into(),
        );
        let service = Arc::new(Self {
            child: Mutex::new(None),
            control: managed.control.clone(),
            project: managed.project.clone(),
            reviewed: Some(managed),
            owner,
        });
        let mut fixtures = FIXTURES.get_or_init(Default::default).lock().unwrap();
        fixtures.insert(requested, Arc::downgrade(&service));
        fixtures.insert(service.scope(), Arc::downgrade(&service));
        service
    }
    pub fn shutdown_reviewed(&self) -> anyhow::Result<()> {
        reviewed::shutdown(
            self.reviewed
                .as_ref()
                .context("shutdown requires reviewed fixture")?,
        )
    }

    pub fn with_stopped_owner<T>(
        &self,
        callback: impl FnOnce(&ProjectRegistration) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        reviewed::with_stopped_owner(
            self.reviewed
                .as_ref()
                .context("offline policy requires reviewed fixture")?,
            callback,
        )
    }

    pub fn http_selection(&self) -> airk_core::service_http::HttpSelection {
        self.reviewed
            .as_ref()
            .expect("HTTP evidence requires an explicit reviewed fixture")
            .selection
            .clone()
    }
    pub fn install_dir(&self) -> &Path {
        &self
            .reviewed
            .as_ref()
            .expect("installed root requires reviewed fixture")
            .install
    }
    pub fn retire_index(&self) -> anyhow::Result<serde_json::Value> {
        reviewed::retire_index(
            self.reviewed
                .as_ref()
                .context("retirement requires reviewed fixture")?,
        )
    }
    pub fn installed_command(&self, root: &Path, data: &Path, collection: &str) -> Command {
        let managed = self
            .reviewed
            .as_ref()
            .expect("installed command requires reviewed fixture");
        let mut command = Command::new(&managed.executable);
        reviewed::git_environment(&mut command, managed.install.parent().unwrap());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
            .arg("--service-install-dir")
            .arg(&managed.install)
            .arg("--root")
            .arg(root)
            .arg("--data-dir")
            .arg(data)
            .args(["--collection", collection]);
        command
    }
    pub fn additional_project(
        &self,
        root: &Path,
        data: &Path,
        collection: &str,
    ) -> anyhow::Result<ProjectRegistration> {
        reviewed::additional_project(
            self.reviewed
                .as_ref()
                .context("additional canonical Index requires reviewed fixture")?,
            root,
            data,
            collection,
        )
    }
    pub fn start(root: &Path, data: &Path, collection: &str) -> Arc<Self> {
        let owner = tempfile::tempdir().unwrap();
        Self::start_owned(root, data, collection, owner)
    }
    pub fn in_owner(
        root: &Path,
        data: &Path,
        collection: &str,
        owner: tempfile::TempDir,
    ) -> Arc<Self> {
        Self::start_owned(root, data, collection, owner)
    }
    fn start_owned(
        root: &Path,
        data: &Path,
        collection: &str,
        owner: tempfile::TempDir,
    ) -> Arc<Self> {
        let control = owner.path().join("control");
        let project = airk_kernel::service_projects::registration(root, data, collection).unwrap();
        let log = std::fs::File::create(owner.path().join("stderr")).unwrap();
        let diagnostics = std::env::var_os("AIRK_PROCESS_DIAGNOSTICS").is_some();
        let mut command = native_command();
        diagnostic("service process starting");
        command
            .arg("serve")
            .arg("--runtime-dir")
            .arg(&control)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(if diagnostics {
                Stdio::piped()
            } else {
                Stdio::from(log)
            });
        let mut child = command.spawn().unwrap();
        if diagnostics && let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if line.starts_with("airk-process:") {
                        eprintln!("airk-service child: {line}");
                    }
                }
            });
        }
        diagnostic("service process started; probing health");
        // Install the guard immediately so failed assertions still reap the owned child.
        let service = Arc::new(Self {
            child: Mutex::new(Some(child)),
            reviewed: None,
            owner,
            control,
            project,
        });
        let until = Instant::now() + Duration::from_secs(60);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let mut health_probe_logged = false;
        loop {
            if !health_probe_logged {
                diagnostic("first health request started");
                health_probe_logged = true;
            }
            if matches!(
                runtime.block_on(airk_cli::service_runtime::request(
                    &service.control,
                    Method::Health
                )),
                Ok(Response::Health { .. })
            ) {
                diagnostic("health request returned ready");
                break;
            }
            assert!(
                Instant::now() < until,
                "native fixture startup failed: {}",
                std::fs::read_to_string(service.owner.path().join("stderr")).unwrap_or_default()
            );
            assert!(
                service
                    .child
                    .lock()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .try_wait()
                    .unwrap()
                    .is_none(),
                "fixture service exited"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
        diagnostic("registration request started");
        let registered = service.request(Method::Register {
            project: service.project.clone(),
        });
        diagnostic("registration request returned");
        assert!(
            matches!(&registered, Response::Registered { project } if project.registration == service.project)
                || matches!(&registered, Response::Failure { code } if code == "registry_response_unknown_retry_with_key"),
            "fixture registration failed: {}; stderr: {}",
            serde_json::to_string(&registered).unwrap(),
            std::fs::read_to_string(service.owner.path().join("stderr")).unwrap_or_default()
        );
        // An explicit unknown response does not prove whether registration
        // committed. Observe its exact binding; never replay the mutation.
        loop {
            let observed = service.request(Method::Projects {
                after: None,
                revision: None,
                limit: 32,
            });
            if matches!(&observed, Response::Projects { page } if page.projects.iter().any(|project| project.registration == service.project))
            {
                break;
            }
            assert!(
                Instant::now() < until
                    && matches!(&observed, Response::Failure { code } if matches!(code.as_str(), "registry_busy" | "registry_response_unknown_retry_with_key")),
                "fixture registration was not observed: {}; initial response: {}",
                serde_json::to_string(&observed).unwrap(),
                serde_json::to_string(&registered).unwrap()
            );
            std::thread::sleep(Duration::from_millis(25));
        }
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
    pub fn request_until(&self, method: Method, until: Instant) -> anyhow::Result<Response> {
        anyhow::ensure!(
            Instant::now() < until,
            "fixture metadata deadline exhausted"
        );
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                tokio::time::timeout_at(
                    until.into(),
                    airk_cli::service_runtime::request(&self.control, method),
                )
                .await
                .context("fixture metadata deadline exhausted")?
            })
    }
    // Fixture setup may outlive one bounded metadata response. Only these
    // read-only operations and receipt-keyed mutations may be repeated, with
    // the complete original request retained across every attempt. Tests of
    // failure responses keep using request() directly.
    pub fn metadata(&self, method: Method, until: Instant) -> Response {
        assert!(matches!(
            &method,
            Method::RequestEpoch
                | Method::Job { .. }
                | Method::Plan { .. }
                | Method::Enqueue { .. }
                | Method::ConfigureProject { .. }
                | Method::PlanCreate { .. }
                | Method::Progress { .. }
                | Method::Watch { .. }
                | Method::Inspection { .. }
        ));
        loop {
            assert!(
                Instant::now() < until,
                "fixture metadata deadline exhausted"
            );
            let response = self
                .request_until(method.clone(), until)
                .expect("fixture metadata request failed");
            if matches!(&response, Response::Failure { code } if matches!(code.as_str(), "registry_busy" | "registry_response_unknown_retry_with_key"))
                && Instant::now() + Duration::from_millis(25) < until
            {
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
            return response;
        }
    }
    pub fn key(&self) -> RequestKey {
        let response = self.metadata(
            Method::RequestEpoch,
            Instant::now() + Duration::from_secs(60),
        );
        let Response::RequestEpoch { namespace, epoch } = response else {
            panic!(
                "request epoch unavailable: {}",
                serde_json::to_string(&response).unwrap()
            )
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
        self.wait_outcome_until(id, Instant::now() + Duration::from_secs(90))
    }
    fn wait_outcome_until(
        &self,
        id: airk_core::identity::Sha256Digest,
        until: Instant,
    ) -> JobRecord {
        loop {
            let response = self.metadata(Method::Job { id }, until);
            let Response::Job { job } = response else {
                panic!(
                    "job receipt unavailable: {}",
                    serde_json::to_string(&response).unwrap()
                )
            };
            if job.outcome.is_some() {
                if matches!(job.outcome, Some(JobOutcome::Failed { .. })) {
                    eprintln!(
                        "native failed-job evidence: {}",
                        job_failure_diagnostics(&self.control, &job)
                    );
                }
                return job;
            }
            if Instant::now() >= until {
                panic!("native job timed out at original observation deadline: {job:?}");
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    pub fn job(&self, kind: JobKind) -> JobRecord {
        let request = Method::Enqueue {
            project: self.project.binding,
            caller: airk_kernel::service_broker::new_instance_id().unwrap(),
            key: self.key(),
            kind,
        };
        let response = self.metadata(request, Instant::now() + Duration::from_secs(60));
        let Response::Job { job } = response else {
            panic!(
                "job not accepted: {}",
                serde_json::to_string(&response).unwrap()
            )
        };
        self.wait(job.id)
    }
    // A successful text Index receipt can precede its automatically scheduled
    // semantic publication. Baseline fixtures observe that native work; they
    // never enqueue a second mutation or disable the selected model.
    pub fn background_pending_until(
        &self,
        project: airk_core::service::ProjectBinding,
        until: Instant,
    ) -> anyhow::Result<bool> {
        // Progress exposes accepted jobs, not the native owner's durable future
        // phase flags. After its idle sample and a completed source watermark,
        // inspect those existing canonical flags before pinning a baseline.
        // This is strictly read-only fixture observation, never job injection.
        let registry = airk_store::service::registry::ServiceRegistry::open_saved_before(
            &self.control,
            Some(until),
        )?;
        if registry.project_background_pending(project)? {
            return Ok(true);
        }
        let mut after = None;
        loop {
            anyhow::ensure!(
                Instant::now() < until,
                "native background observation deadline exhausted"
            );
            // Include a just-claimed Running job whose first cached Progress
            // sample has not arrived. Paused pending jobs remain in Progress's
            // complete queued list, checked by the caller's native barrier.
            let page = registry.pending_project_page(after, 32)?;
            anyhow::ensure!(
                Instant::now() < until,
                "native background observation deadline exhausted"
            );
            for seen in &page {
                if *seen == project {
                    return Ok(true);
                }
                if *seen > project {
                    return Ok(false);
                }
            }
            if page.len() < 32 {
                return Ok(false);
            }
            let next = *page.last().context("native background cursor missing")?;
            anyhow::ensure!(
                after.is_none_or(|old| next > old),
                "native background cursor did not advance"
            );
            after = Some(next);
        }
    }
    pub fn settle_reviewed_index(
        &self,
        root: &Path,
        data: &Path,
        until: Instant,
    ) -> anyhow::Result<()> {
        if self.reviewed.is_none() {
            return Ok(());
        }
        loop {
            anyhow::ensure!(Instant::now() < until, "reviewed Index work did not settle");
            let progress = self.metadata(
                Method::Progress {
                    project: self.project.binding,
                },
                until,
            );
            let Response::Progress {
                project,
                observation,
                pending_job,
                queued_jobs,
                queue_truncated,
                ..
            } = &progress
            else {
                anyhow::bail!(
                    "native Index progress unavailable: {}",
                    serde_json::to_string(&progress)?
                );
            };
            anyhow::ensure!(
                *project == self.project.binding,
                "Index progress crossed project"
            );
            anyhow::ensure!(
                pending_job
                    .as_ref()
                    .is_none_or(|job| job.project == self.project.binding)
                    && queued_jobs
                        .iter()
                        .all(|job| job.project == self.project.binding)
                    && observation
                        .as_ref()
                        .is_none_or(|value| value.job.project == self.project.binding),
                "native Index progress contains crossed job"
            );
            let idle = pending_job.is_none()
                && queued_jobs.is_empty()
                && *queue_truncated == Some(false)
                && observation.as_ref().is_none_or(|value| {
                    !matches!(
                        value.job.state,
                        airk_core::service::JobState::Pending
                            | airk_core::service::JobState::Running
                    )
                });
            let watch = self.metadata(
                Method::Watch {
                    project: self.project.binding,
                },
                until,
            );
            let Response::Watch {
                project,
                observation: watch,
            } = watch
            else {
                anyhow::bail!(
                    "native Index watch unavailable: {}",
                    serde_json::to_string(&watch)?
                );
            };
            anyhow::ensure!(
                project == self.project.binding,
                "Index watch crossed project"
            );
            watch.validate()?;
            anyhow::ensure!(
                watch.failure.is_none() && !watch.automatic_retries_exhausted,
                "native Index source watch failed: {watch:?}"
            );
            // IndexSetup Ready seals the catalog/storage transaction; its initial
            // source job can still be pending. Observe that job's native state
            // before evidence exists instead of retrying a nonretryable read.
            let inspection = self.metadata(
                Method::Inspection {
                    project: self.project.binding,
                },
                until,
            );
            let Response::Inspection {
                project,
                observation,
                ..
            } = &inspection
            else {
                anyhow::bail!(
                    "native Index inspection unavailable: {}",
                    serde_json::to_string(&inspection)?
                );
            };
            anyhow::ensure!(
                *project == self.project.binding
                    && observation
                        .as_ref()
                        .is_none_or(|value| value.snapshot.project == self.project.binding),
                "Index inspection crossed project"
            );
            let indexed = observation.as_ref().is_some_and(|value| {
                value.snapshot.index_state == airk_core::service_inspection::IndexState::Ready
                    && value.snapshot.publication.is_some()
            });
            if !idle || watch.completed_revision != watch.requested_revision || !indexed {
                anyhow::ensure!(
                    Instant::now() + Duration::from_millis(25) < until,
                    "native initial Index did not settle: progress={},watch={watch:?},inspection={}",
                    serde_json::to_string(&progress)?,
                    serde_json::to_string(&inspection)?
                );
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
            if self.background_pending_until(self.project.binding, until)? {
                anyhow::ensure!(
                    Instant::now() + Duration::from_millis(25) < until,
                    "native background phase did not settle"
                );
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
            let envelope = call_until(
                root,
                data,
                &self.project.collection,
                &["tools", "call", "airk_diagnostics"],
                until,
            )?;
            anyhow::ensure!(envelope["isError"] != true, "{envelope}");
            let status: serde_json::Value = serde_json::from_str(
                envelope["content"][0]["text"]
                    .as_str()
                    .context("Index diagnostic content missing")?,
            )?;
            if !read_retry::publication_pending(
                &status,
                &self.project.collection,
                &self.project.binding.to_string(),
            )? {
                let semantic: airk_core::readiness::SemanticReadiness =
                    serde_json::from_value(status["semantic_readiness"].clone())?;
                anyhow::ensure!(
                    semantic.state != airk_core::readiness::SemanticState::Failed,
                    "native Index semantic work failed: {status}"
                );
                if idle
                    && watch.completed_revision == watch.requested_revision
                    && matches!(
                        semantic.state,
                        airk_core::readiness::SemanticState::Ready
                            | airk_core::readiness::SemanticState::Disabled
                    )
                {
                    return Ok(());
                }
            }
            anyhow::ensure!(
                Instant::now() + Duration::from_millis(25) < until,
                "native Index settlement deadline exhausted: {status}"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    #[cfg(feature = "local-inference")]
    pub fn wait_semantic(&self, artifacts: u64) -> anyhow::Result<serde_json::Value> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let connection = if let Some(managed) = &self.reviewed {
                    let airk_core::service_http::HttpSelection::Scoped { selection } =
                        &managed.selection
                    else {
                        anyhow::bail!("fresh semantic fixture requires its exact scoped selection");
                    };
                    let refreshed = airk_cli::service_frontend::resolve_scoped(
                        None,
                        Some(&self.control),
                        selection,
                    )
                    .await?;
                    anyhow::ensure!(
                        refreshed.project.registration == self.project,
                        "semantic fixture scope crossed native backing"
                    );
                    airk_cli::project_client::scoped_tools(
                        self.control.clone(),
                        self.project.binding,
                        refreshed.selection,
                    )?
                } else {
                    airk_cli::project_client::tools(self.control.clone(), self.project.binding)
                };
                let deadline = Instant::now() + Duration::from_secs(90);
                loop {
                    let envelope = tokio::time::timeout_at(
                        deadline.into(),
                        connection.call(
                            "airk_diagnostics".into(),
                            serde_json::json!({"response_mode":"full"})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                    )
                    .await??;
                    anyhow::ensure!(
                        Instant::now() < deadline,
                        "semantic observation deadline exhausted"
                    );
                    if let Some(retry) = read_retry::within(&envelope, deadline) {
                        tokio::time::sleep(retry.delay).await;
                        continue;
                    }
                    anyhow::ensure!(envelope["isError"] != true, "{envelope}");
                    let status: serde_json::Value =
                        serde_json::from_str(envelope["content"][0]["text"].as_str().unwrap())?;
                    if !read_retry::publication_pending(
                        &status,
                        &self.project.collection,
                        &self.project.binding.to_string(),
                    )? {
                        let readiness: airk_core::readiness::SemanticReadiness =
                            serde_json::from_value(status["semantic_readiness"].clone())?;
                        anyhow::ensure!(
                            readiness.state != airk_core::readiness::SemanticState::Failed,
                            "semantic phase failed"
                        );
                        if readiness.state == airk_core::readiness::SemanticState::Ready
                            && readiness.completed_artifacts == artifacts
                        {
                            return Ok(status);
                        }
                    }
                    anyhow::ensure!(
                        Instant::now() < deadline,
                        "semantic work did not complete: {}",
                        read_retry::status_summary(&status)
                    );
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
    }
    pub fn configure(&self, draft: airk_cli::service_configuration::Draft) {
        self.job(JobKind::Inspect);
        // The service authenticates the native engine image. The test harness
        // has a different image, so exercise the actual CLI preparation path
        // rather than weakening that identity check for in-process fixtures.
        let mut child = native_command()
            .args(["service", "prepare-configuration", "--runtime-dir"])
            .arg(&self.control)
            .arg("--project")
            .arg(self.project.binding.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&draft).unwrap())
            .unwrap();
        let prepared = child.wait_with_output().unwrap();
        assert!(
            prepared.status.success(),
            "native configuration preparation failed: stdout={} stderr={}",
            String::from_utf8_lossy(&prepared.stdout),
            String::from_utf8_lossy(&prepared.stderr)
        );
        let request = serde_json::from_slice(&prepared.stdout).unwrap();
        let request = Method::ConfigureProject {
            project: self.project.binding,
            caller: airk_kernel::service_broker::new_instance_id().unwrap(),
            key: self.key(),
            request: Box::new(request),
        };
        let response = self.metadata(request, Instant::now() + Duration::from_secs(60));
        let Response::Job { job } = response else {
            panic!(
                "configuration not accepted: {}",
                serde_json::to_string(&response).unwrap()
            )
        };
        self.wait(job.id);
    }

    pub fn repair(&self, operation: airk_core::service_maintenance::RepairOperation) {
        use airk_core::service_maintenance::{PlanPhase, RepairRequest, RepairSelection};
        let request = Method::PlanCreate {
            caller: airk_kernel::service_broker::new_instance_id().unwrap(),
            key: self.key(),
            request: RepairRequest {
                selection: RepairSelection::Selected(vec![self.project.binding]),
                operation,
            },
        };
        let response = self.metadata(request, Instant::now() + Duration::from_secs(60));
        let Response::Plan { plan } = response else {
            panic!(
                "repair preview not accepted: {}",
                serde_json::to_string(&response).unwrap()
            )
        };
        let until = Instant::now() + Duration::from_secs(90);
        let preview = loop {
            let Response::Plan { plan } = self.metadata(Method::Plan { id: plan.id }, until) else {
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
            let Response::Plan { plan } = self.metadata(Method::Plan { id: preview.id }, until)
            else {
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
        let retain_http = self.control.join("http-fixture-failure.json").exists();
        if retain_http {
            self.owner.disable_cleanup(true);
        }
        if let Some(managed) = &self.reviewed {
            match reviewed::shutdown(managed) {
                Ok(()) if !retain_http => self.owner.disable_cleanup(false),
                Ok(()) => eprintln!(
                    "HTTP fixture failure evidence retained at {}",
                    self.owner.path().display()
                ),
                Err(error) => eprintln!(
                    "reviewed fixture cleanup uncertain; retained {}: {error:#}",
                    self.owner.path().display()
                ),
            }
            return;
        }
        if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            let _ = runtime.block_on(airk_cli::service_runtime::request(
                &self.control,
                Method::Shutdown,
            ));
        }
        let child = self.child.get_mut().unwrap().as_mut().unwrap();
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

fn fixture(root: &Path, data: &Path, collection: &str) -> Option<Arc<Service>> {
    let scope = (
        root.canonicalize().unwrap_or_else(|_| root.into()),
        data.canonicalize().unwrap_or_else(|_| data.into()),
        collection.into(),
    );
    FIXTURES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .get(&scope)
        .and_then(Weak::upgrade)
}

pub fn http_fixture(root: &Path, data: &Path, collection: &str) -> Arc<Service> {
    let service = fixture(root, data, collection).expect("HTTP fixture binding missing");
    assert!(
        service.reviewed.is_some(),
        "HTTP evidence requires an explicit reviewed fixture"
    );
    service
}

pub fn command(root: &Path, data: &Path, collection: &str) -> Command {
    let service = fixture(root, data, collection);
    let mut command = if let Some(managed) = service.as_ref().and_then(|s| s.reviewed.as_ref()) {
        let mut command = Command::new(&managed.executable);
        reviewed::git_environment(
            &mut command,
            managed
                .install
                .parent()
                .expect("reviewed fixture owner missing"),
        );
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        command
    } else {
        native_command()
    };
    if let Some(service) = &service {
        command
            .arg("--root")
            .arg(&service.project.root)
            .arg("--data-dir")
            .arg(&service.project.data_root)
            .args([
                "--collection",
                &service.project.collection,
                "--service-runtime-dir",
            ])
            .arg(&service.control)
            .arg("--service-project")
            .arg(service.project.binding.to_string());
    } else {
        command
            .arg("--root")
            .arg(root)
            .arg("--data-dir")
            .arg(data)
            .args(["--collection", collection])
            .arg("--service-install-dir")
            .arg(data.join("not-an-installed-runtime"));
    }
    command
        .env("AIRK_MCP_RESPONSE_MODE", "full")
        .env("AIRK_USAGE_MODE", "disabled");
    command
}

pub fn http_selection(
    root: &Path,
    data: &Path,
    collection: &str,
) -> anyhow::Result<airk_core::service_http::HttpSelection> {
    let service = fixture(root, data, collection).context("HTTP fixture binding missing")?;
    let managed = service
        .reviewed
        .as_ref()
        .context("HTTP read requires an explicit reviewed fixture")?;
    Ok(managed.selection.clone())
}

pub fn reviewed_http_selection(
    control: &Path,
    project: &ProjectRegistration,
) -> airk_core::service_http::HttpSelection {
    reviewed::reviewed_http_selection(control, project)
        .expect("canonical reviewed HTTP selection failed")
}

pub fn scoped_command(root: &Path, data: &Path, collection: &str) -> Command {
    let service = fixture(root, data, collection).expect("scoped read has no owned fixture");
    let managed = service
        .reviewed
        .as_ref()
        .expect("scoped read requires explicit reviewed fixture");
    let airk_core::service_http::HttpSelection::Scoped { selection } = &managed.selection else {
        panic!("fresh fixture selection is not scoped")
    };
    let mut command = Command::new(&managed.executable);
    reviewed::git_environment(
        &mut command,
        managed
            .install
            .parent()
            .expect("reviewed fixture owner missing"),
    );
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command
        .arg("--service-runtime-dir")
        .arg(&service.control)
        .arg("--catalog-session")
        .arg(serde_json::to_string(selection).unwrap())
        .env("AIRK_MCP_RESPONSE_MODE", "full")
        .env("AIRK_USAGE_MODE", "disabled");
    command
}

/// Evidence reads use the exact reviewed grant; owner mutations retain their
/// independent native control arguments. Legacy fixtures remain explicit.
pub fn evidence_command(root: &Path, data: &Path, collection: &str) -> Command {
    if fixture(root, data, collection).is_some_and(|service| service.reviewed.is_some()) {
        scoped_command(root, data, collection)
    } else {
        command(root, data, collection)
    }
}

fn evidence_arguments(args: &[&str]) -> bool {
    args.starts_with(&["tools", "call"])
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
        )
}

pub fn scoped_call(
    root: &Path,
    data: &Path,
    collection: &str,
    args: &[&str],
) -> anyhow::Result<serde_json::Value> {
    let output = read_retry::output(scoped_command(root, data, collection).args(args))?;
    anyhow::ensure!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

pub fn call(
    root: &Path,
    data: &Path,
    collection: &str,
    args: &[&str],
) -> anyhow::Result<serde_json::Value> {
    call_until(
        root,
        data,
        collection,
        args,
        Instant::now() + Duration::from_secs(60),
    )
}

pub fn published_status(
    root: &Path,
    data: &Path,
    collection: &str,
) -> anyhow::Result<serde_json::Value> {
    let project = airk_kernel::service_projects::registration(root, data, collection)?
        .binding
        .to_string();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let response = call_until(
            root,
            data,
            collection,
            &["tools", "call", "airk_diagnostics"],
            deadline,
        )?;
        let value = serde_json::from_str(
            response["content"][0]["text"]
                .as_str()
                .context("diagnostic status payload missing")?,
        )?;
        anyhow::ensure!(
            Instant::now() < deadline,
            "publication observation deadline exhausted"
        );
        if !read_retry::publication_pending(&value, collection, &project)? {
            return Ok(value);
        }
        anyhow::ensure!(
            Instant::now() + Duration::from_millis(25) < deadline,
            "published status unavailable at deadline; cause unknown: {}",
            read_retry::status_summary(&value)
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn call_until(
    root: &Path,
    data: &Path,
    collection: &str,
    args: &[&str],
    deadline: Instant,
) -> anyhow::Result<serde_json::Value> {
    let mut invocation_started = false;
    let mut invocation_returned = false;
    let read = evidence_arguments(args);
    let output = loop {
        if !invocation_started {
            diagnostic("first native CLI invocation started");
            invocation_started = true;
        }
        let mut native = if read {
            evidence_command(root, data, collection)
        } else {
            command(root, data, collection)
        };
        let output = read_retry::once_until(native.args(args), deadline)?;
        if !invocation_returned {
            diagnostic("native CLI invocation returned");
            invocation_returned = true;
        }
        // Evidence commands are read-only. A typed rejection repeats these exact
        // arguments (including revision/cursor); never retry an accepted mutation.
        let delay = if read {
            read_retry::from_output(&output, deadline).map(|retry| retry.delay)
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
        let job = service.wait_outcome_until(
            serde_json::from_value(value["job"]["id"].clone())?,
            deadline,
        );
        anyhow::ensure!(
            matches!(job.outcome, Some(JobOutcome::Succeeded { .. })),
            "native index failed: {job:?}"
        );
        service.settle_reviewed_index(root, data, deadline)?;
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
