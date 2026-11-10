// SPDX-License-Identifier: Apache-2.0
//! Opt-in native qualification support. This owner delegates to the production
//! control and drain paths; it never supplies a synthetic revocation/reap proof.
use super::*;
use crate::{
    catalog_admission::{FencePlan, FenceTicket, NativeCatalogAdmission},
    catalog_authority::NativeCatalogAuthoritySelection,
    catalog_coordinator::CatalogControl,
};
use airk_core::catalog_fences::{FenceTarget, FencedReceipt};
use std::{cell::RefCell, rc::Rc};

/// One test controller owns the same child handles, resource managers and
/// private exchanges as Supervisor. The coordinator borrows this existing owner.
struct NativeOwner {
    root: PathBuf,
    instance: Incarnation,
    gate: NativeCatalogAdmission,
    bound: Option<NativeCatalogAdmission>,
    workers: BTreeMap<ProjectBinding, ChildWorker>,
    registry: Option<ServiceRegistry>,
    pool: Option<Pool>,
    #[cfg(feature = "local-inference")]
    _factory: Option<crate::service_inference::InferenceFactory>,
    recovery: PoolRecovery,
    resources: airk_kernel::service_resources::Resources,
    drains: WorkerResourceDrains,
    policy: Mutex<Broker>,
    log: crate::service_logs::Log,
    _log_guard: crate::service_logs::Guard,
    backoff: BTreeMap<ProjectBinding, (Instant, u32)>,
    // Release service exclusion only after children/helper/resource owners drop.
    _lease: Arc<airk_store::service::ServiceLease>,
}

impl NativeOwner {
    fn execute(&mut self, operation: catalog::Operation) -> Result<catalog::Response> {
        catalog::execute(
            &mut self.bound,
            self.gate.clone(),
            operation,
            &mut self.workers,
            &mut self.registry,
            &self.root,
            self.instance,
            true,
            &mut self.backoff,
            WorkerAdmission {
                log: &self.log,
                policy: &self.policy,
                pool: &mut self.pool,
                resources: &mut self.resources,
                recovery: &mut self.recovery,
                drains: &mut self.drains,
            },
        )
    }

    /// Explicit fixture image selection is necessary because the controller is
    /// a test image. All actual host admission and provider delivery are retained.
    fn spawn_image(
        &mut self,
        registration: airk_core::service::ProjectRegistration,
        main: &Path,
    ) -> Result<WorkerIdentity> {
        self.gate.require_worker_start(registration.binding)?;
        ensure!(
            self.resources.may_start_resident(),
            "native resident admission refused"
        );
        let worker = ChildWorker::spawn_image(
            &self.root,
            registration.binding,
            self.instance,
            false,
            main,
            crate::runtime_pair::engine_identity_at(main)?,
        )?;
        let identity = worker.identity.context("native worker identity missing")?;
        let allowance = QueryAllowance {
            concurrent_queries: 2,
            response_bytes: 2 * MAX_FRAME_BYTES,
        };
        self.policy
            .lock()
            .map_err(|_| anyhow::anyhow!("native policy poisoned"))?
            .register(
                identity,
                allowance,
                Instant::now(),
                Duration::from_secs(300),
            )?;
        self.resources.resident_started(identity)?;
        ensure!(
            !self.workers.contains_key(&identity.project),
            "native owner replaced"
        );
        self.workers.insert(identity.project, worker);
        let activated = (|| -> Result<()> {
            let worker = self
                .workers
                .get_mut(&identity.project)
                .context("native owner lost")?;
            let pool = self.pool.as_mut().context("native helper absent")?;
            // Register can have a wire effect even when its acknowledgement fails.
            worker.record_provider_delivery(
                Some(pool.identity()),
                crate::service_inference::in_process(pool),
            )?;
            let access = pool.access_worker(identity, registration)?;
            worker.activate(identity, allowance, Some(access), false)?;
            self.gate.register_worker(identity)?;
            worker.catalog_gate = Some(self.gate.clone());
            Ok(())
        })();
        if let Err(error) = activated {
            // Preserve the owning child/history on any fallible reservation.
            let _ = self.begin_exit(identity.project);
            return Err(error);
        }
        Ok(identity)
    }

    fn begin_exit(&mut self, project: ProjectBinding) -> Result<()> {
        begin_worker_exit(&mut self.workers, project, &mut self.pool, &mut self.drains)
    }

    fn poll_exits(&mut self) -> Result<()> {
        poll_worker_exits(
            &mut self.drains,
            &mut self.pool,
            &mut self.recovery,
            &mut self.resources,
            &self.policy,
            WorkerOwnerContext {
                root: &self.root,
                instance: self.instance,
                gate: Some(&self.gate),
            },
        )
    }

    fn charges(&self) -> Result<(usize, usize, usize, usize, usize, usize)> {
        let resources = self.resources.snapshot();
        let broker = self
            .policy
            .lock()
            .map_err(|_| anyhow::anyhow!("native policy poisoned"))?
            .status(Instant::now());
        // Semantic reads retain resident/query ownership, not an invented job
        // allowance. Broker project counts include retained fencing tombstones;
        // allocated slots/bytes separately prove release of their charges.
        Ok((
            resources.resident_workers,
            broker.projects,
            resources.running_jobs,
            broker.active_queries,
            broker.allocated.concurrent_queries,
            broker.allocated.response_bytes,
        ))
    }
}

#[derive(Clone)]
struct NativeControl {
    owner: Rc<RefCell<NativeOwner>>,
    gate: NativeCatalogAdmission,
    source: Option<crate::source_supervisor::SourceWake>,
}

impl CatalogControl for NativeControl {
    fn fence_source_policy(&self, project: ProjectBinding, operation: Incarnation) -> Result<()> {
        self.source
            .as_ref()
            .context("native fixture source owner absent")?
            .fence_policy(project, operation)
    }
    fn release_source_policy(&self, project: ProjectBinding, operation: Incarnation) -> Result<()> {
        self.source
            .as_ref()
            .context("native fixture source owner absent")?
            .release_policy(project, operation)
    }
    fn uses_gate(&self, gate: &NativeCatalogAdmission) -> bool {
        self.gate.same_instance(gate)
    }
    fn revoke(&self, ticket: &FenceTicket, target: &FenceTarget) -> Result<()> {
        ensure!(
            matches!(
                self.owner
                    .borrow_mut()
                    .execute(catalog::Operation::Revoke {
                        ticket: ticket.clone(),
                        target: target.clone(),
                    })?,
                catalog::Response::Revoked
            ),
            "unexpected native revoke reply"
        );
        Ok(())
    }
    fn confirm_reaped(&self, worker: WorkerIdentity) -> Result<bool> {
        match self
            .owner
            .borrow_mut()
            .execute(catalog::Operation::Reaped { worker })?
        {
            catalog::Response::Reaped(value) => Ok(value),
            _ => anyhow::bail!("unexpected native reap reply"),
        }
    }
    fn capture_workers(&self, ticket: &FenceTicket, backing: ProjectBinding) -> Result<FencePlan> {
        match self
            .owner
            .borrow_mut()
            .execute(catalog::Operation::CaptureWorkers {
                ticket: ticket.clone(),
                backing,
            })? {
            catalog::Response::Captured(plan) => Ok(*plan),
            _ => anyhow::bail!("unexpected native census reply"),
        }
    }
    fn drain_worker(&self, ticket: &FenceTicket, worker: WorkerIdentity) -> Result<()> {
        ensure!(
            matches!(
                self.owner
                    .borrow_mut()
                    .execute(catalog::Operation::DrainWorker {
                        ticket: ticket.clone(),
                        worker,
                    })?,
                catalog::Response::Drained
            ),
            "unexpected native drain reply"
        );
        Ok(())
    }
    fn finish_worker_drains(&self, receipt: &FencedReceipt) -> Result<()> {
        ensure!(
            matches!(
                self.owner
                    .borrow_mut()
                    .execute(catalog::Operation::FinishWorkerDrains {
                        receipt: Box::new(receipt.clone()),
                    })?,
                catalog::Response::Drained
            ),
            "unexpected native drain completion reply"
        );
        Ok(())
    }
}

/// Read brokerage crosses the actual guarded byte delivery boundary before this
/// test client can receive or redeem the native bearer. Nothing logs it.
async fn connect_scope(
    owner: &Rc<RefCell<NativeOwner>>,
    selection: NativeCatalogAuthoritySelection,
) -> Result<local_ipc::Stream> {
    let catalog::Response::Connection(delivery) =
        owner.borrow_mut().execute(catalog::Operation::Connect {
            selection: catalog::ConnectionSelection::Reviewed(selection),
            lifetime: Duration::from_secs(120),
        })?
    else {
        anyhow::bail!("native scope brokerage response missing")
    };
    let (mut writer, mut reader) = tokio::io::duplex(MAX_FRAME_BYTES + 4);
    (*delivery)
        .deliver(&mut writer, Duration::from_secs(5))
        .await?;
    drop(writer);
    let mut bytes = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut bytes).await?;
    let Some(ControlResponse::Connection { role, grant }) =
        service_ipc::read_frame(&mut bytes.as_slice())?
    else {
        anyhow::bail!("guarded native brokerage bytes missing")
    };
    ensure!(
        grant.scope.is_some(),
        "authority-one read expanded to unscoped"
    );
    let root = owner.borrow().root.clone();
    let mut stream = local_ipc::connect(&local_ipc::endpoint(&root, &role)?).await?;
    local_ipc::write_message(&mut stream, &grant, Duration::from_secs(5)).await?;
    let accepted: protocol::Accepted = local_ipc::read_message(&mut stream, Duration::from_secs(5))
        .await?
        .context("native scoped redemption EOF")?;
    ensure!(
        accepted.identity == grant.identity,
        "native scoped redemption crossed worker"
    );
    Ok(stream)
}

async fn search_scope(
    stream: &mut local_ipc::Stream,
    mode: &str,
    query: &str,
) -> Result<serde_json::Value> {
    local_ipc::write_message(
        stream,
        &protocol::Query {
            read_attempt: None,
            frontend: None,
            tool: "airk_search".into(),
            arguments: serde_json::json!({"query":query, "mode":mode, "response_mode":"full"})
                .as_object()
                .context("native search object missing")?
                .clone(),
            native: None,
            status_receipt: Default::default(),
        },
        Duration::from_secs(5),
    )
    .await?;
    let result: protocol::QueryResult = local_ipc::read_message(stream, Duration::from_secs(20))
        .await?
        .context("native scoped search EOF")?;
    ensure!(
        result.result["isError"] != true,
        "native scoped query refused: {}",
        result.result
    );
    let body: serde_json::Value = serde_json::from_str(
        result.result["content"][0]["text"]
            .as_str()
            .context("native scoped query body missing")?,
    )?;
    ensure!(
        body["hits"].as_array().is_some_and(|hits| !hits.is_empty()),
        "native scoped hits missing"
    );
    Ok(body)
}

impl NativeOwner {
    fn run_job(&mut self, job: airk_core::service::JobRecord) -> Result<()> {
        self.resources
            .pending(job.project, job.id, job.kind, Instant::now())?;
        let allowance = self
            .resources
            .next(Instant::now())?
            .context("actual native job resource admission unavailable")?;
        ensure!(
            allowance.job == job.id && allowance.identity.project == job.project,
            "native resource scheduler selected a different job"
        );
        self.workers
            .get_mut(&job.project)
            .context("native job worker absent")?
            .run_job(allowance.clone())?;
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let observation = self
                .workers
                .get_mut(&job.project)
                .context("native job owner disappeared")?
                .work_status(None, false, None)?;
            if let Some(failure) = observation.failure {
                anyhow::bail!(
                    "native fixture job failed: {}",
                    serde_json::to_value(failure)?
                )
            }
            if observation.finished == Some(allowance.id) {
                self.resources.finished(allowance.identity, allowance.id)?;
                let saved = self
                    .registry
                    .as_ref()
                    .context("native registry absent")?
                    .job(job.id)?;
                ensure!(
                    matches!(
                        saved.outcome,
                        Some(airk_core::service::JobOutcome::Succeeded { .. })
                    ),
                    "native fixture job did not succeed: {saved:?}"
                );
                return Ok(());
            }
            ensure!(
                Instant::now() < deadline,
                "actual native job did not finish"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

mod engine_gate;
mod restart;

struct NativeIndex {
    definition: airk_core::index_catalog::IndexId,
    project: airk_core::index_catalog::QueryProjectId,
    registration: airk_core::service::ProjectRegistration,
    original: Vec<u8>,
}

struct NativeFixture {
    // Resume any accepted engine work before native controller ownership drops.
    gate: engine_gate::EngineGate,
    control: NativeControl,
    registry: ServiceRegistry,
    indices: Vec<NativeIndex>,
    main: PathBuf,
    grant: Option<airk_core::index_catalog::GrantId>,
    _directory: tempfile::TempDir,
}

fn verify_fixture_image(main: &Path, native: bool) -> Result<()> {
    ensure!(
        crate::runtime_pair::single_executable(main)? == native,
        "fixture runtime execution kind does not match the selected provider"
    );
    if native {
        ensure!(
            crate::runtime_pair::verify_installed_pair(main, env!("CARGO_PKG_VERSION"))? == main,
            "modern runtime verification resolved a different executable"
        );
    } else {
        crate::runtime_pair::verify_helper(main)?;
    }
    Ok(())
}

impl NativeFixture {
    fn key(&mut self) -> Result<airk_core::service::RequestKey> {
        let (namespace, epoch) = self.registry.request_epoch()?;
        Ok(airk_core::service::RequestKey {
            namespace,
            epoch,
            nonce: new_instance_id()?,
        })
    }

    fn request(
        &mut self,
        operation: airk_core::catalog_management::CatalogOperation,
    ) -> Result<airk_core::catalog_management::CatalogRequest> {
        Ok(airk_core::catalog_management::CatalogRequest {
            caller: local_ipc::caller_identity()?,
            key: self.key()?,
            expected_catalog_revision: self
                .registry
                .catalog_state()?
                .context("native catalog absent")?
                .revision,
            operation,
        })
    }

    fn apply(&mut self, operation: airk_core::catalog_management::CatalogOperation) -> Result<()> {
        let request = self.request(operation)?;
        let preview = self.registry.catalog_preview(&request)?.identity()?;
        if request.operation.requires_authority_fence() {
            crate::catalog_coordinator::CatalogCoordinator::new(
                self.control.gate.clone(),
                self.control.clone(),
            )?
            .apply_fenced(&mut self.registry, &request, preview)?;
        } else {
            self.registry.catalog_apply(&request, preview)?;
            crate::catalog_coordinator::refresh_catalog_clock(&self.control.gate, &self.registry)?;
        }
        Ok(())
    }

    fn setup_index(&mut self, base: &Path, name: &str) -> Result<()> {
        use airk_core::{catalog_setup::IndexSetupSelection, service::ControlMethod};
        let source = base.join(format!("source-{name}"));
        std::fs::create_dir(&source)?;
        let original = b"# Cobalt harbor\nCobalt harbor navigation memo.\n".to_vec();
        std::fs::write(source.join("memo.md"), &original)?;
        let state = self
            .registry
            .catalog_state()?
            .context("native setup catalog absent")?;
        let method = ControlMethod::CatalogIndexSetupDraft {
            selection: IndexSetupSelection {
                key: self.key()?,
                expected_catalog_revision: state.revision,
                name: name.into(),
                primary_root: source.to_str().context("source is not UTF-8")?.into(),
                data_root: base
                    .join(format!("data-{name}"))
                    .to_str()
                    .context("data is not UTF-8")?
                    .into(),
                collection: name.into(),
                additional_roots: vec![],
                setup_watch: false,
            },
        };
        let root = self.control.owner.borrow().root.clone();
        let Some(ControlResponse::CatalogIndexSetupDraft { draft, .. }) =
            crate::catalog_index_setup_control::execute(&mut self.registry, &root, &method)?
        else {
            anyhow::bail!("canonical native setup draft unavailable")
        };
        let registration = draft.request.registration.clone();
        let Some(ControlResponse::CatalogIndexSetupApplied { receipt }) =
            crate::catalog_index_setup_control::execute(
                &mut self.registry,
                &root,
                &ControlMethod::CatalogIndexSetupApply { draft },
            )?
        else {
            anyhow::bail!("canonical native setup did not complete")
        };
        ensure!(
            receipt.stage == airk_core::catalog_setup::SetupStage::Ready,
            "native setup is not ready"
        );
        // Canonical empty publication, no source scan/model provision. This
        // pre-worker initialization avoids pretending a missing SQLite file is
        // an indexed collection; subsequent publication is an actual worker job.
        airk_kernel::Kernel::open_service_collection(
            Path::new(&registration.root),
            Some(PathBuf::from(&registration.data_root)),
            &registration.collection,
        )?
        .initialize()?;
        crate::catalog_coordinator::refresh_catalog_clock(&self.control.gate, &self.registry)?;
        let project = airk_core::index_catalog::QueryProjectId::from_identity(new_instance_id()?);
        self.apply(
            airk_core::catalog_management::CatalogOperation::CreateProject {
                id: project,
                name: format!("Reviewed {name}"),
            },
        )?;
        self.apply(
            airk_core::catalog_management::CatalogOperation::SetMembership {
                membership: airk_core::index_catalog::Membership {
                    project_id: project,
                    index_id: receipt.index_id,
                    revision: airk_core::index_catalog::CatalogRevision::INITIAL,
                    scope: vec![airk_core::index_catalog::ScopeAtom {
                        source_root_id: receipt.source_ids[0],
                        kind: airk_core::index_catalog::PathScope::Subtree,
                        path: String::new(),
                    }],
                },
                expected_scope_revision: airk_core::index_catalog::CatalogRevision::INITIAL,
                expected_membership_revision: None,
            },
        )?;
        self.indices.push(NativeIndex {
            definition: receipt.index_id,
            project,
            registration,
            original,
        });
        Ok(())
    }

    fn configure_model_and_publish(&mut self, index: usize, model: &Path) -> Result<()> {
        use airk_core::{service::JobKind, service_configuration::ConfigurationRequest};
        let project = self.indices[index].registration.binding;
        let key = self.key()?;
        let job = self.registry.enqueue(
            project,
            local_ipc::caller_identity()?,
            &key,
            JobKind::Inspect,
        )?;
        self.control.owner.borrow_mut().run_job(job)?;
        let observed = self
            .registry
            .inspection(project)?
            .context("actual native inspection missing")?
            .snapshot;
        let request = ConfigurationRequest {
            prepare: false,
            incarnation: observed.incarnation,
            settings: observed
                .settings_digest
                .context("actual native settings digest missing")?,
            configuration: executor()?
                .block_on(prepare_fixture_model(model, |request| async {
                    // The opaque capability outlives the synchronous owner borrow.
                    // No RefCell guard may cross connect/read's await points.
                    let capability = {
                        let response = self
                            .control
                            .owner
                            .borrow_mut()
                            .execute(catalog::Operation::AdministrativeRead { project })?;
                        let catalog::Response::AdministrativeRead(capability) = response else {
                            anyhow::bail!("native fixture administrative capability missing")
                        };
                        capability
                    };
                    (*capability)
                        .connect(airk_core::usage::ClientFamily::Unknown)
                        .await?
                        .read(request)
                        .await
                }))
                .context("fixture setup: native model input preparation")?,
        };
        request.validate()?;
        let key = self.key()?;
        let job = self.registry.enqueue_configuration(
            project,
            local_ipc::caller_identity()?,
            &key,
            &request,
            crate::runtime_pair::engine_identity_at(&self.main)?,
        )?;
        self.control
            .owner
            .borrow_mut()
            .run_job(job)
            .context("fixture setup: model configuration")?;
        for kind in [JobKind::Index, JobKind::Vectors, JobKind::Verify] {
            let key = self.key()?;
            let job = self
                .registry
                .enqueue(project, local_ipc::caller_identity()?, &key, kind)?;
            self.control
                .owner
                .borrow_mut()
                .run_job(job)
                .with_context(|| format!("fixture setup: {kind:?}"))?;
        }
        Ok(())
    }

    fn review_grant(&mut self) -> Result<()> {
        let projects = self.indices.iter().map(|index| index.project).collect();
        let selection = airk_core::catalog_host::GrantDraftSelection {
            key: self.key()?,
            expected_catalog_revision: self
                .registry
                .catalog_state()?
                .context("native catalog absent")?
                .revision,
            project_ids: projects,
            index_ids: None,
        };
        let context = crate::catalog_host_context::NativeHostContext::load(
            &mut self.registry,
            airk_core::identity::Sha256Digest::from_bytes([71; 32]),
            Some(selection.expected_catalog_revision),
        )?
        .for_review(&selection)?;
        let draft = crate::catalog_grant_draft::build_reviewed_grant_draft(
            &mut self.registry,
            &context,
            &selection,
        )?;
        let airk_core::catalog_management::CatalogOperation::CreateGrant { grant } =
            &draft.request.operation
        else {
            anyhow::bail!("native review returned a different operation")
        };
        let id = grant.id;
        self.registry
            .catalog_apply(&draft.request, draft.preview_digest)?;
        crate::catalog_coordinator::refresh_catalog_clock(&self.control.gate, &self.registry)?;
        self.grant = Some(id);
        Ok(())
    }

    fn selection(&self, index: usize) -> Result<NativeCatalogAuthoritySelection> {
        let state = self
            .registry
            .catalog_state()?
            .context("native catalog absent")?;
        let index = &self.indices[index];
        Ok(NativeCatalogAuthoritySelection {
            namespace: state.namespace,
            host: crate::catalog_host_context::current_native_host(state.namespace)?,
            catalog_revision: state.revision,
            grant_id: self.grant.context("native reviewed grant missing")?,
            grant_revision: airk_core::index_catalog::CatalogRevision::INITIAL,
            index_id: index.definition,
            project_ids: vec![index.project],
            backing: index.registration.binding,
        })
    }

    fn source_bytes_unchanged(&self) -> Result<()> {
        for index in &self.indices {
            ensure!(
                std::fs::read(Path::new(&index.registration.root).join("memo.md"))?
                    == index.original,
                "native qualification modified source bytes"
            );
        }
        Ok(())
    }
}

impl NativeFixture {
    fn build() -> Result<Self> {
        let staged = PathBuf::from(std::env::var_os("AIRK_TEST_GUARDED_RUNTIME").context(
            "AIRK_TEST_GUARDED_RUNTIME must pin a separately staged guarded qualification pair",
        )?)
        .canonicalize()?;
        let main = staged
            .join(crate::install_files::main_name())
            .canonicalize()?;
        Self::build_with_image(main, false)
    }

    #[cfg(feature = "local-inference")]
    fn build_modern() -> Result<Self> {
        let main = PathBuf::from(
            std::env::var_os("AIRK_TEST_BINARY")
                .context("AIRK_TEST_BINARY must pin a matched modern native runtime")?,
        )
        .canonicalize()?;
        let runtime = crate::runtime_pair::payload_root(&main)?;
        let expected = std::env::var("AIRK_TEST_NATIVE_SOURCE_SHA256").context(
            "pin AIRK_TEST_NATIVE_SOURCE_SHA256 to the verified candidate compiled source",
        )?;
        let seal = std::env::var("AIRK_TEST_INSTALL_MANIFEST_SHA256").context(
            "pin AIRK_TEST_INSTALL_MANIFEST_SHA256 to the independently verified candidate seal",
        )?;
        modern_checkpoint_tests::verify_modern_fixture_inputs(&main, runtime, &expected, &seal)?;
        Self::build_with_image(main, true)
    }

    fn build_with_image(main: PathBuf, native: bool) -> Result<Self> {
        #[cfg(not(feature = "local-inference"))]
        ensure!(!native, "native fixture requires local-inference");
        verify_fixture_image(&main, native)?;
        let mut directory = tempfile::Builder::new()
            .prefix("airk-guarded-workers-")
            .tempdir_in(std::env::temp_dir().canonicalize()?)?;
        if native {
            directory.disable_cleanup(true);
            eprintln!(
                "modern checkpoint fixture retained: {}",
                directory.path().display()
            );
        }
        let base = directory.path().canonicalize()?;
        let root = base.join("control");
        let instance = new_instance_id()?;
        // Complete compiled native component activation, not managed promotion.
        let mut registry = crate::catalog_test_fixture::authority_one(&root, instance)?;
        let lease = Arc::new(airk_store::service::ServiceLease::acquire(&root)?);
        crate::install_layout::record_control_root(&lease)?;
        registry.record_service_instance(&lease, instance)?;
        let state = registry
            .catalog_state()?
            .context("native fixture catalog absent")?;
        let gate = NativeCatalogAdmission::new(instance, state.namespace, state.revision, 1)?;
        let latch = engine_gate::EngineGate::bind(&root)?;
        #[cfg(feature = "local-inference")]
        let mut factory = if native {
            Some(crate::service_inference::InferenceFactory::new(
                lease.clone(),
                instance,
            )?)
        } else {
            None
        };
        #[cfg(feature = "local-inference")]
        let pool = match factory.as_mut() {
            Some(factory) => factory.start(1)?,
            None => Pool::start_fixture(&root, instance, &main)?,
        };
        #[cfg(not(feature = "local-inference"))]
        let pool = Pool::start_fixture(&root, instance, &main)?;
        let (log, guard) = crate::service_logs::Log::start(root.clone(), instance);
        let native = NativeOwner {
            root: root.clone(),
            instance,
            gate: gate.clone(),
            bound: None,
            workers: BTreeMap::new(),
            registry: Some(ServiceRegistry::open(&root, instance)?),
            pool: Some(pool),
            #[cfg(feature = "local-inference")]
            _factory: factory,
            recovery: PoolRecovery::new(Instant::now()),
            resources: airk_kernel::service_resources::Resources::detect(
                airk_core::service_resources::ResourceConfiguration {
                    memory_bytes: Some(4 << 30),
                    query_concurrency: 4,
                    ..Default::default()
                },
            )?,
            drains: WorkerResourceDrains::default(),
            policy: Mutex::new(Broker::new(
                instance,
                airk_kernel::service_broker::BrokerLimits::service(),
            )?),
            log,
            _log_guard: guard,
            backoff: BTreeMap::new(),
            _lease: lease,
        };
        let owner = Rc::new(RefCell::new(native));
        ensure!(
            matches!(
                owner.borrow_mut().execute(catalog::Operation::Attach)?,
                catalog::Response::Attached
            ),
            "actual native controller failed to attach"
        );
        let control = NativeControl {
            owner,
            gate,
            source: None,
        };
        let mut fixture = Self {
            gate: latch,
            control,
            registry,
            indices: vec![],
            main,
            grant: None,
            _directory: directory,
        };
        fixture.setup_index(&base, "a")?;
        fixture.setup_index(&base, "b")?;
        let model = base.join("model");
        std::fs::create_dir(&model)?;
        let vocabulary = br#"{"cobalt":[1.0,0.0],"harbor":[0.0,1.0]}"#;
        std::fs::write(model.join("vocabulary.json"), vocabulary)?;
        use sha2::{Digest, Sha256};
        std::fs::write(
            model.join("model.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema":1,"name":"guarded native workers","license":"Apache-2.0",
                "sha256":format!("{:x}",Sha256::digest(vocabulary)),"dimensions":2,
            }))?,
        )?;
        for index in 0..2 {
            fixture
                .control
                .owner
                .borrow_mut()
                .spawn_image(fixture.indices[index].registration.clone(), &fixture.main)?;
            fixture.configure_model_and_publish(index, &model)?;
        }
        fixture.review_grant()?;
        Ok(fixture)
    }
}

/// The observer is the native administrative read owner, never a local hasher.
async fn prepare_fixture_model<F, Fut>(
    model: &Path,
    observe: F,
) -> Result<airk_core::service_configuration::ProjectConfiguration>
where
    F: FnOnce(airk_core::native_reads::NativeRead) -> Fut,
    Fut: std::future::Future<Output = Result<serde_json::Value>>,
{
    use airk_core::service_configuration::{InputFile, ProjectConfiguration};
    let manifest = model.join("model.json");
    let path = manifest.to_str().context("model input must be UTF-8")?;
    ensure!(
        manifest.is_absolute() && path.len() <= 4096 && !path.chars().any(char::is_control),
        "invalid bounded native model input"
    );
    let result = observe(airk_core::native_reads::NativeRead::PrepareInputFile {
        path: path.into(),
        max_source_bytes: 16_384,
    })
    .await?;
    let result = crate::usage_client::require_native_success(result).map_err(|error| {
        let body = error
            .downcast_ref::<crate::usage_client::NativeReadFailure>()
            .map(|failure| failure.body().to_string())
            .unwrap_or_default();
        error.context(format!("fixture native model preparation refused: {body}"))
    })?;
    let pin: InputFile = serde_json::from_value(result)
        .context("native model preparation omitted its complete pin")?;
    pin.validate()?;
    let configuration = ProjectConfiguration::InstallModel {
        directory: Path::new(&pin.path)
            .parent()
            .context("native model manifest has no directory")?
            .to_str()
            .context("native model directory must be UTF-8")?
            .into(),
        manifest_sha256: pin.sha256,
    };
    configuration.validate()?;
    Ok(configuration)
}

#[test]
fn fixture_model_preparation_preserves_native_request_and_pin() -> Result<()> {
    use airk_core::service_configuration::{InputFile, ProjectConfiguration};
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    // Neither spelling exists: the fixture must leave source access to native.
    let original = base.join("alias").join("..").join("model");
    let pin = InputFile {
        path: base.join("canonical/model.json").to_str().unwrap().into(),
        sha256: airk_core::identity::Sha256Digest::from_bytes([43; 32]),
    };
    let mut calls = 0;
    let configuration = executor()?.block_on(prepare_fixture_model(&original, |request| {
        calls += 1;
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            serde_json::to_value(airk_core::native_reads::NativeRead::PrepareInputFile {
                path: original.join("model.json").to_str().unwrap().into(),
                max_source_bytes: 16_384,
            })
            .unwrap()
        );
        std::future::ready(Ok(serde_json::to_value(&pin).unwrap()))
    }))?;
    assert_eq!(calls, 1);
    assert_eq!(
        configuration,
        ProjectConfiguration::InstallModel {
            directory: base.join("canonical").to_str().unwrap().into(),
            manifest_sha256: pin.sha256,
        }
    );
    Ok(())
}

#[test]
fn fixture_model_preparation_retains_refusals_and_rejects_invalid_pins() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let model = directory.path().canonicalize()?.join("never-opened");
    let runtime = executor()?;
    let refusal = serde_json::json!({"isError": true, "error": {"code": "source_policy"},
        "retry": {"allowed": false}, "evidence": ["original spelling refused"]});
    let error = runtime
        .block_on(prepare_fixture_model(&model, |_| {
            std::future::ready(Ok(refusal.clone()))
        }))
        .unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<crate::usage_client::NativeReadFailure>()
            .context("lost native refusal type")?
            .body(),
        &refusal
    );
    assert!(format!("{error:#}").contains(&refusal.to_string()));
    let error = runtime
        .block_on(prepare_fixture_model(&model, |_| {
            std::future::ready(Err(anyhow::anyhow!("transport observation failed")))
        }))
        .unwrap_err();
    assert_eq!(error.to_string(), "transport observation failed");
    for pin in [
        serde_json::json!({}),
        serde_json::json!({"path":"", "sha256":"a".repeat(64)}),
        serde_json::json!({"path":model.join("model.json"), "sha256":"invalid"}),
        serde_json::json!({"path":model.join("model.json"), "sha256":"a".repeat(64), "extra":true}),
    ] {
        assert!(
            runtime
                .block_on(prepare_fixture_model(&model, |_| {
                    std::future::ready(Ok(pin.clone()))
                }))
                .is_err(),
            "accepted invalid native pin: {pin}"
        );
    }
    Ok(())
}

fn executor() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?)
}

#[test]
#[ignore = "requires separately pinned AIRK_TEST_GUARDED_RUNTIME; actual guarded native pair only"]
fn ordinary_exit_keeps_real_provider_work_and_sibling_ownership_until_engine_release() -> Result<()>
{
    let runtime = executor()?;
    let mut fixture = NativeFixture::build()?;
    runtime.block_on(async {
        let mut a = connect_scope(&fixture.control.owner, fixture.selection(0)?).await?;
        let mut b = connect_scope(&fixture.control.owner, fixture.selection(1)?).await?;
        require_semantic_ready(&mut a).await?;
        require_semantic_ready(&mut b).await?;
        search_scope(&mut a, "vector", "cobalt harbor").await?;
        search_scope(&mut b, "vector", "cobalt harbor").await?;
        let (a_identity, b_identity, helper, loads) = {
            let mut owner = fixture.control.owner.borrow_mut();
            let a = owner.workers[&fixture.indices[0].registration.binding].identity.context("A identity missing")?;
            let b = owner.workers[&fixture.indices[1].registration.binding].identity.context("B identity missing")?;
            let pool = owner.pool.as_mut().context("native helper absent")?;
            let crate::embedding_pool_protocol::Observation::Status { loads, .. } = pool.status()? else {
                anyhow::bail!("native helper status absent")
            };
            (a, b, pool.identity(), loads)
        };
        let before = fixture.control.owner.borrow().charges()?;
        assert_eq!(before, (2, 2, 0, 0, 4, 4 * MAX_FRAME_BYTES), "fixture must not invent a job-work/query charge for resident reads");
        let nonce = fixture.gate.arm(a_identity, helper)?;
        let accepted = tokio::spawn(async move { search_scope(&mut a, "vector", "harbor cobalt cobalt").await });
        let entered = fixture.gate.entered(nonce)?;
        ensure!(entered.worker == a_identity && entered.helper == helper, "actual compute crossed owner");
        fixture.control.owner.borrow_mut().begin_exit(a_identity.project)?;
        fixture.control.owner.borrow_mut().poll_exits()?;
        ensure!(fixture.control.owner.borrow().drains.identities().contains(&a_identity), "held provider owner lost");
        assert_eq!(fixture.control.owner.borrow().charges()?, before, "child reap released provider-owned charges");
        ensure!(!fixture.control.owner.borrow().workers.contains_key(&a_identity.project), "native A child not reaped");
        let refused = fixture.control.owner.borrow_mut().execute(catalog::Operation::Connect {
            selection: catalog::ConnectionSelection::Reviewed(fixture.selection(0)?), lifetime: Duration::from_secs(120),
        });
        ensure!(refused.is_err(), "draining worker accepted fresh brokerage");
        search_scope(&mut b, "lexical", "cobalt harbor").await?;
        fixture.control.owner.borrow_mut().workers.get_mut(&b_identity.project).context("B owner missing")?
            .work_status(None, false, None)?;
        fixture.gate.resume()?;
        // A was actually reaped; its unread result is not replayed or presented
        // as a successful read. Accepted helper work still needed the barrier.
        ensure!(accepted.await.context("native A client join failed")?.is_err(),
            "reaped native worker delivered a held successful read");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !fixture.control.owner.borrow().drains.identities().is_empty() {
            fixture.control.owner.borrow_mut().poll_exits()?;
            ensure!(Instant::now() < deadline, "actual engine Release never drained");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert_eq!(fixture.control.owner.borrow().charges()?, (1, 2, 0, 0, 2, 2 * MAX_FRAME_BYTES));
        search_scope(&mut b, "vector", "cobalt harbor harbor").await?;
        let mut owner = fixture.control.owner.borrow_mut();
        assert_eq!(owner.workers[&b_identity.project].identity, Some(b_identity));
        let pool = owner.pool.as_mut().context("native sibling helper missing")?;
        assert_eq!(pool.identity(), helper);
        ensure!(matches!(pool.status()?, crate::embedding_pool_protocol::Observation::Status { loads: now, .. } if now == loads),
            "ordinary A exit replaced/reloaded the healthy shared model");
        fixture.source_bytes_unchanged()?;
        Ok(())
    })
}

#[test]
#[ignore = "requires separately pinned AIRK_TEST_GUARDED_RUNTIME; actual guarded native pair only"]
fn retire_requires_actual_engine_release_and_complete_native_worker_obligation() -> Result<()> {
    use airk_core::{
        catalog_management::{CatalogEntity, CatalogItem},
        catalog_source_draft::{SourceMutationDraftSelection, SourceMutationSelection},
    };
    let runtime = executor()?;
    let mut fixture = NativeFixture::build()?;
    runtime.block_on(async {
        let mut a = connect_scope(&fixture.control.owner, fixture.selection(0)?).await?;
        let mut b = connect_scope(&fixture.control.owner, fixture.selection(1)?).await?;
        require_semantic_ready(&mut a).await?;
        require_semantic_ready(&mut b).await?;
        search_scope(&mut a, "vector", "cobalt harbor").await?;
        search_scope(&mut b, "vector", "cobalt harbor").await?;
        let (a_identity, b_identity, helper) = {
            let owner = fixture.control.owner.borrow();
            (
                owner.workers[&fixture.indices[0].registration.binding]
                    .identity
                    .context("A identity absent")?,
                owner.workers[&fixture.indices[1].registration.binding]
                    .identity
                    .context("B identity absent")?,
                owner.pool.as_ref().context("helper absent")?.identity(),
            )
        };
        let key = fixture.key()?;
        let state = fixture
            .registry
            .catalog_state()?
            .context("native catalog absent")?;
        let root = fixture.control.owner.borrow().root.clone();
        let draft = crate::catalog_source_change_control::draft(
            &mut fixture.registry,
            &root,
            &SourceMutationDraftSelection {
                key,
                expected_catalog_revision: state.revision,
                index_id: fixture.indices[0].definition,
                change: SourceMutationSelection::RetireIndex,
            },
        )?;
        let mut coordinator = crate::catalog_coordinator::CatalogCoordinator::new(
            fixture.control.gate.clone(),
            fixture.control.clone(),
        )?;
        let nonce = fixture.gate.arm(a_identity, helper)?;
        let accepted =
            tokio::spawn(
                async move { search_scope(&mut a, "vector", "harbor cobalt cobalt").await },
            );
        let entered = fixture.gate.entered(nonce)?;
        ensure!(
            entered.worker == a_identity && entered.helper == helper,
            "retirement held foreign engine work"
        );
        let error = coordinator
            .apply_fenced(&mut fixture.registry, &draft.request, draft.preview_digest)
            .err()
            .context("Retire committed while actual engine work was held")?;
        ensure!(
            error.is::<crate::catalog_coordinator::CatalogApplyOutcomeUnknown>(),
            "accepted native retirement lost uncertain outcome classification"
        );
        let intent = fixture
            .registry
            .catalog_pending_fence()?
            .context("durable native WDO absent")?;
        intent.validate()?;
        let obligation = intent
            .plan
            .worker_obligation
            .as_ref()
            .context("native worker census absent")?;
        assert_eq!(obligation.workers, vec![a_identity]);
        assert_eq!(obligation.index_id, fixture.indices[0].definition);
        assert_eq!(obligation.backing, a_identity.project);
        assert_eq!(
            obligation.providers,
            vec![crate::catalog_admission::native_provider_obligation(
                a_identity,
                vec![helper]
            )]
        );
        ensure!(
            !intent.plan.targets.is_empty(),
            "native installed read scope omitted"
        );
        ensure!(
            intent.worker_confirmations.is_empty(),
            "Scope ACK falsely completed actual provider drain"
        );
        assert_eq!(
            fixture.control.owner.borrow().charges()?,
            (2, 2, 0, 0, 4, 4 * MAX_FRAME_BYTES)
        );
        let detail = fixture.registry.catalog_detail(
            &CatalogEntity::Index {
                id: fixture.indices[0].definition,
            },
            None,
        )?;
        ensure!(
            matches!(detail.item, Some(CatalogItem::Index(index)) if !index.tombstone),
            "metadata retired before actual provider drain"
        );
        ensure!(
            fixture
                .control
                .gate
                .begin_issue(state.namespace, state.revision)
                .is_err(),
            "prepared retirement reopened admission"
        );
        search_scope(&mut b, "lexical", "cobalt harbor").await?;
        fixture
            .control
            .owner
            .borrow_mut()
            .workers
            .get_mut(&b_identity.project)
            .context("native B disappeared")?
            .work_status(None, false, None)?;
        fixture.gate.resume()?;
        ensure!(
            accepted
                .await
                .context("retired A client join failed")?
                .is_err(),
            "revoked/reaped A delivered successful queued evidence"
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        let receipt = loop {
            match coordinator.apply_fenced(
                &mut fixture.registry,
                &draft.request,
                draft.preview_digest,
            ) {
                Ok(receipt) => break receipt,
                Err(error) => {
                    ensure!(
                        error.is::<crate::catalog_coordinator::CatalogApplyOutcomeUnknown>(),
                        "native retirement retry changed its original outcome class"
                    );
                    ensure!(
                        Instant::now() < deadline,
                        "native retirement never observed engine Release"
                    );
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            }
        };
        receipt.validate_for(
            &draft.request,
            draft.preview_digest,
            receipt.prepared_digest,
            &intent.plan,
        )?;
        assert_eq!(
            receipt.proof.worker_obligation,
            intent.plan.worker_obligation
        );
        assert_eq!(receipt.proof.worker_confirmations.len(), 1);
        assert_eq!(receipt.proof.worker_confirmations[0].worker, a_identity);
        assert_eq!(
            coordinator.apply_fenced(
                &mut fixture.registry,
                &draft.request,
                draft.preview_digest
            )?,
            receipt,
            "terminal replay repeated or changed native effects"
        );
        fixture.control.owner.borrow_mut().poll_exits()?;
        assert_eq!(
            fixture.control.owner.borrow().charges()?,
            (1, 2, 0, 0, 2, 2 * MAX_FRAME_BYTES)
        );
        search_scope(&mut b, "vector", "cobalt harbor harbor").await?;
        let owner = fixture.control.owner.borrow();
        assert_eq!(
            owner.workers[&b_identity.project].identity,
            Some(b_identity)
        );
        assert_eq!(
            owner
                .pool
                .as_ref()
                .context("healthy native sibling helper lost")?
                .identity(),
            helper
        );
        fixture.source_bytes_unchanged()?;
        Ok(())
    })
}

async fn require_semantic_ready(stream: &mut local_ipc::Stream) -> Result<()> {
    local_ipc::write_message(
        stream,
        &protocol::Query {
            read_attempt: None,
            frontend: None,
            tool: "airk_status".into(),
            arguments: serde_json::json!({"response_mode":"full"})
                .as_object()
                .context("native status arguments missing")?
                .clone(),
            native: None,
            status_receipt: Default::default(),
        },
        Duration::from_secs(5),
    )
    .await?;
    let result: protocol::QueryResult = local_ipc::read_message(stream, Duration::from_secs(10))
        .await?
        .context("native semantic readiness EOF")?;
    ensure!(
        result.result["isError"] != true,
        "native readiness refused: {}",
        result.result
    );
    let status: serde_json::Value = serde_json::from_str(
        result.result["content"][0]["text"]
            .as_str()
            .context("complete native status body missing")?,
    )?;
    ensure!(
        status["semantic_readiness"]["state"] == "ready"
            && status["semantic_readiness"]["selected_artifacts"] == 1
            && status["semantic_readiness"]["completed_artifacts"] == 1,
        "actual vector turn did not establish full native semantic readiness: {status}"
    );
    Ok(())
}

mod split_tests;

#[cfg(feature = "local-inference")]
mod modern_checkpoint_tests;
