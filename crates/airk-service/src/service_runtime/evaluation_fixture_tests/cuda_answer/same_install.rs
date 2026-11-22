// SPDX-License-Identifier: Apache-2.0
//! Private CUDA Answer fixture that can reopen the CPU fixture's own installation.
//!
//! The CPU roster stays the installation's identity: a same-install restart pins
//! the original Ready, Closed and final status exactly as the CPU selector does.
//! One extra CUDA Context profile (Compact or Large) joins the evaluation catalog
//! only; its assets are held as read descriptors and never seeded into the store.

use super::*;

const SEED_VARIABLE: &str = "AIRK_ARBITER_EVALUATION_CUDA_SEED_CPU";

/// How a fresh installation treats the CPU roster. A restart never seeds.
/// `Focus` installs only the CPU Focus weights, for a small synthetic proving
/// installation that pairs CPU Focus with CUDA Context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CpuSeed {
    All,
    Focus,
    None,
}

impl CpuSeed {
    fn includes(self, model: ModelChoice) -> bool {
        match self {
            Self::All => true,
            Self::Focus => model == ModelChoice::FocusQwenReranker06B,
            Self::None => false,
        }
    }
}

fn cpu_seed(value: Option<&str>, restart: bool) -> Result<Option<CpuSeed>> {
    match (restart, value) {
        (true, None) => Ok(None),
        (true, Some(_)) => bail!("a same-install CUDA restart never seeds the CPU roster"),
        (false, Some("all")) => Ok(Some(CpuSeed::All)),
        (false, Some("focus")) => Ok(Some(CpuSeed::Focus)),
        (false, Some("none")) => Ok(Some(CpuSeed::None)),
        (false, _) => bail!("a fresh CUDA fixture requires {SEED_VARIABLE}=all|focus|none"),
    }
}

/// The roster facts the same-install CUDA check reads from each profile.
#[derive(Clone, Copy)]
struct Facts<'a> {
    manifest: Sha256Digest,
    model: ModelChoice,
    qualified: bool,
    profile_id: &'a str,
    cuda: bool,
    device: bool,
}

impl<'a> Facts<'a> {
    fn of(profile: &'a Profile) -> Self {
        Self {
            manifest: profile.manifest,
            model: profile.model,
            qualified: profile.qualification.is_some(),
            profile_id: &profile.profile_id,
            cuda: profile.device_execution.as_ref().is_some_and(|device| {
                device.backend == airk_core::arbiter::device::GpuBackend::Cuda
            }),
            device: profile.device_execution.is_some(),
        }
    }
}

/// The CUDA profile joins the CPU roster's catalog: an unqualified private
/// Context Compact or Large profile with a CUDA device binding, a manifest
/// that no CPU roster entry uses, and a CPU twin of the same model.
fn require_cuda_profile(cpu: &[Facts<'_>], cuda: Facts<'_>) -> Result<()> {
    ensure!(
        matches!(
            cuda.model,
            ModelChoice::ContextQwenCompact | ModelChoice::ContextQwenLarge
        ) && !cuda.qualified
            && cuda.profile_id.starts_with("private-")
            && cuda.cuda
            && cpu
                .iter()
                .all(|profile| profile.manifest != cuda.manifest && !profile.device)
            && cpu.iter().any(|profile| profile.model == cuda.model),
        "same-install CUDA Answer requires one unqualified CUDA Compact or Large profile beside its CPU roster"
    );
    Ok(())
}

/// One pinned CUDA roster entry, prepared with the same checks as each CPU
/// roster entry: pinned profile bytes, authenticated asset closure under
/// artifacts and held read descriptors of the original bytes.
fn prepare_cuda(path: &Path, expected: Sha256Digest) -> Result<(Vec<u8>, Prepared)> {
    let artifacts = artifact_root()?;
    let (_parent, mut file) = source_file(path, &artifacts)?;
    let raw_inputs = bounded(&mut file)?;
    ensure!(
        sha(&raw_inputs) == expected,
        "private CUDA roster hash differs"
    );
    let mut inputs: Vec<super::super::Input> = serde_json::from_slice(&raw_inputs)?;
    ensure!(
        inputs.len() == 1,
        "private CUDA roster names exactly one profile"
    );
    let input = inputs.remove(0);
    let (_parent, mut file) = source_file(&input.profile, &artifacts)?;
    let raw = bounded(&mut file)?;
    ensure!(
        sha(&raw) == input.profile_sha256,
        "private CUDA profile bytes differ from pinned input"
    );
    let profile: Profile = serde_json::from_slice(&raw)?;
    profile.validate()?;
    let root = input.assets.canonicalize()?;
    ensure!(
        root.starts_with(&artifacts),
        "private CUDA model assets leave artifacts"
    );
    let assets = OpenedDirectory::open(&root)?;
    let fixture_name = input
        .fixtures
        .file_name()
        .context("private CUDA compiler fixture name missing")?;
    ensure!(
        input.fixtures.canonicalize()? == root.join(fixture_name).canonicalize()?
            && profile
                .assets
                .iter()
                .any(|asset| OsStr::new(&asset.name) == fixture_name),
        "CUDA compiler fixture is outside the profile's authenticated asset closure"
    );
    let mut sources = BTreeMap::new();
    for asset in &profile.assets {
        let source = assets.open_file(OsStr::new(&asset.name))?;
        ensure!(
            source.metadata()?.len() == asset.bytes,
            "private CUDA asset length differs"
        );
        sources.insert(asset.name.clone(), source);
    }
    Ok((
        raw_inputs,
        Prepared {
            profile,
            raw,
            sha256: input.profile_sha256,
            sources,
            _profile_file: file,
            _assets: assets,
        },
    ))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "private Windows CUDA Answer over the CPU fixture's installation; exact reviewed inputs required"]
async fn actual_private_arbiter_fixture_cuda_same_install() -> Result<()> {
    let consent = std::env::var("AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT")
        .context("current CUDA fixture requires explicit resource configuration consent")?;
    run(
        software_v2_answer_template()?,
        cuda_answer_resource_configuration(&consent)?,
    )
    .await
}

/// One ordered lifetime, following `run_actual_private_arbiter_fixture`:
/// inputs, startup, Ready, the service lifetime and settlement.
async fn run(
    context_request: serde_json::Value,
    resource_configuration: airk_core::service_resources::ResourceConfiguration,
) -> Result<()> {
    let started = Instant::now();
    let decoded: airk_mcp::AnswerArgs = serde_json::from_value(context_request.clone())?;
    decoded.request.validate()?;
    let input = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_EVALUATION_INPUTS")
            .context("AIRK_ARBITER_EVALUATION_INPUTS missing")?,
    );
    let expected: Sha256Digest = std::env::var("AIRK_ARBITER_EVALUATION_INPUTS_SHA256")?.parse()?;
    let cuda_input = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_EVALUATION_CUDA_INPUTS")
            .context("AIRK_ARBITER_EVALUATION_CUDA_INPUTS missing")?,
    );
    let cuda_expected: Sha256Digest =
        std::env::var("AIRK_ARBITER_EVALUATION_CUDA_INPUTS_SHA256")?.parse()?;
    let work = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_EVALUATION_WORK")
            .context("AIRK_ARBITER_EVALUATION_WORK missing")?,
    );
    let seconds: u64 = std::env::var("AIRK_ARBITER_EVALUATION_SECONDS")?.parse()?;
    ensure!(
        (1..=21600).contains(&seconds),
        "private fixture lifetime must be explicit and at most six hours"
    );
    ensure!(
        work.is_absolute() && !work.try_exists()?,
        "private fixture work must be a fresh absolute directory"
    );
    match std::env::var("AIRK_SELECTED_CONTEXT_CAL_OBSERVER") {
        Ok(value) if value == "false" => {}
        Err(std::env::VarError::NotPresent) => {}
        _ => bail!("CUDA Answer requires actual Context scoring; the CAL observer stays off"),
    }
    let artifacts = artifact_root()?;
    let parent = work
        .parent()
        .context("private fixture work parent missing")?
        .canonicalize()?;
    ensure!(
        parent.starts_with(&artifacts),
        "private fixture work must remain under artifacts"
    );
    let work = parent.join(
        work.file_name()
            .context("private fixture work name missing")?,
    );
    let (inputs, mut prepared) = prepare(&input, expected)?;
    let (cuda_inputs, cuda) = prepare_cuda(&cuda_input, cuda_expected)?;
    require_cuda_profile(
        &prepared
            .iter()
            .map(|entry| Facts::of(&entry.profile))
            .collect::<Vec<_>>(),
        Facts::of(&cuda.profile),
    )?;
    let cuda_manifest = cuda.profile.manifest;
    let cuda_model = cuda.profile.model;
    let graph = Input::read_matching(&artifacts, started + Duration::from_secs(seconds), |manifest| {
        ensure!(
            manifest == cuda_manifest,
            "reviewed CUDA graph result names another profile"
        );
        Ok(())
    })?;
    let parent_pin = OpenedDirectory::open(&parent)?;
    let work_pin = crate::ipc::endpoint::create_root_at(
        &parent_pin,
        work.file_name().context("private work name missing")?,
    )?;
    // The CPU roster alone is the restart identity, exactly as for the CPU selector.
    let restart = restart::from_env(expected, &prepared)?;
    let seed = cpu_seed(
        std::env::var(SEED_VARIABLE).ok().as_deref(),
        restart.is_some(),
    )?;
    let control = restart
        .as_ref()
        .map_or_else(|| work.join("control"), |saved| saved.control.clone());
    let _control_pin = if restart.is_none() {
        Some(crate::ipc::endpoint::create_root_at(
            &work_pin,
            OsStr::new("control"),
        )?)
    } else {
        None
    };
    if let Some(saved) = &restart {
        write(&work, "restart-owner.json", saved)?;
    }
    crate::ipc::private_record::write_private(&work, "inputs.json", &inputs, LIMIT, false)?;
    crate::ipc::private_record::write_private(
        &work,
        "cuda-inputs.json",
        &cuda_inputs,
        LIMIT,
        false,
    )?;
    // CPU roster first in its original order, then the CUDA profile, so the
    // configuration emitter finds every manifest in this lifetime's Ready.
    prepared.push(cuda);
    for (index, entry) in prepared.iter().enumerate() {
        crate::ipc::private_record::write_private(
            &work,
            &format!("profile-{index}.json"),
            &entry.raw,
            LIMIT,
            false,
        )?;
    }
    write(&work, "context-request.json", &context_request)?;
    write(
        &work,
        "cuda-same-install.json",
        &serde_json::json!({
            "schema": "airk.private.cuda-same-install/1",
            "evaluation": true,
            "release_qualified": false,
            "restart": restart.is_some(),
            "cpu_seed": format!("{seed:?}"),
            "cpu_inputs_sha256": expected,
            "cuda_inputs_sha256": cuda_expected,
            "cuda_manifest": cuda_manifest,
            "cuda_model": cuda_model,
            "graph_sha256": std::env::var("AIRK_ARBITER_EVALUATION_CUDA_GRAPH_SHA256")?,
        }),
    )?;
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    let lease = if restart.is_none() {
        first_start::ensure_catalog(lease)?
    } else {
        lease
    };
    drop(lease);
    let prepared = Arc::new(prepared);
    let mut execution_recorder = execution_observer::Recorder::from_env(&work)?;
    let execution_observer = execution_recorder
        .as_ref()
        .map(|recorder| recorder.observer());
    let mut warm_recorder = warm_journal::Recorder::from_env(&work)?;
    let warm_observer = warm_recorder.as_ref().map(|recorder| recorder.observer());
    let warm_observer_id = warm_recorder.as_ref().map(|recorder| recorder.id());
    let restart_guard: Option<EvaluationStartupGuard> = restart.as_ref().map(|saved| {
        let expected = saved.configuration.clone();
        Box::new(move |lease: &Arc<airk_store::service::ServiceLease>| {
            EvaluationOwner::verify_restart_settled(lease)?;
            let registry =
                airk_store::service::registry::ServiceRegistry::open_saved(lease.root())?;
            let catalog = registry
                .catalog_state()?
                .context("Restart catalog missing before startup")?;
            ensure!(
                catalog.activated && catalog.namespace == expected.catalog,
                "Restart catalog identity changed before supervisor startup"
            );
            let store = airk_store::service::arbiter::ConfigurationStore::open(
                lease.clone(),
                expected.installation,
                expected.catalog,
            )?;
            ensure!(
                store.load()? == expected,
                "Restart configuration changed before supervisor startup"
            );
            Ok(())
        }) as EvaluationStartupGuard
    });
    let source = prepared.clone();
    let (seeded, seed_ready) = tokio::sync::oneshot::channel();
    let lease_witness = Arc::new(std::sync::OnceLock::new());
    let capture_lease = lease_witness.clone();
    let factory: EvaluationFactory = Box::new(move |lease, gate| {
        capture_lease
            .set(Arc::downgrade(&lease))
            .map_err(|_| anyhow::anyhow!("private original lease observed twice"))?;
        let namespace = gate
            .catalog_clock()
            .context("actual fixture catalog clock missing")?
            .namespace;
        let profiles = source.iter().map(|entry| entry.profile.clone()).collect();
        let mut owner = if let Some(saved) = &restart {
            ensure!(
                namespace == saved.configuration.catalog,
                "Restart catalog namespace changed"
            );
            EvaluationOwner::reopen_same_configuration(lease, gate, profiles, &saved.configuration)?
        } else {
            EvaluationOwner::new(lease, gate, profiles)?
        };
        let entry = source
            .iter()
            .find(|entry| entry.profile.manifest == graph.manifest)
            .context("private CUDA Answer asset profile missing")?;
        owner = owner.with_cuda_answer(graph.manifest, graph.adapter, graph.graph, |asset| {
            entry
                .sources
                .get(&asset.name)
                .context("original private CUDA asset descriptor missing")?
                .try_clone()
                .map_err(Into::into)
        })?;
        if let Some(seed) = seed {
            for entry in source.iter().filter(|entry| {
                entry.profile.manifest != graph.manifest && seed.includes(entry.profile.model)
            }) {
                owner.seed(entry.profile.manifest, &mut |asset| {
                    entry
                        .sources
                        .get(&asset.name)
                        .context("original private asset descriptor missing")?
                        .try_clone()
                        .map_err(Into::into)
                })?;
            }
        }
        ensure!(
            {
                let configuration = owner.configuration()?;
                restart
                    .as_ref()
                    .is_some_and(|saved| configuration.result() == &saved.configuration)
                    || (!configuration.result().focus.enabled
                        && !configuration.result().context.enabled)
            },
            "private fixture must start disabled or with its exact saved configuration"
        );
        if let Some(observer) = execution_observer {
            owner = owner.observe_execution(observer);
        }
        if let Some(observer) = warm_observer {
            owner = owner.observe_warm(observer)?;
        }
        seeded
            .send(namespace)
            .map_err(|_| anyhow::anyhow!("private fixture observer disappeared"))?;
        Ok(owner)
    });
    let root = control.clone();
    let mut service = tokio::spawn(async move {
        if let Some(guard) = restart_guard {
            serve_restarted_evaluation_fixture(&root, factory, guard).await
        } else {
            serve_evaluation_fixture(&root, factory).await
        }
    });
    let mut observed_service_exit = false;
    let startup = async {
        let namespace = tokio::select! {
            value=seed_ready=>value.context("private fixture failed before actual seeding completed")?,
            value=&mut service=>{observed_service_exit=true;value??;bail!("private service exited before startup")},
            _=tokio::time::sleep(Duration::from_secs(900))=>{bail!("private fixture seed deadline expired; retain original work and observe the process before recovery")}
        };
        let response = tokio::select! {
            value=crate::ipc::control::request_once(&control,ControlMethod::Health,env!("CARGO_PKG_VERSION"),None)=>value?,
            value=&mut service=>{observed_service_exit=true;value??;bail!("private service exited before health observation")},
        };
        let ControlResponse::Health { observation } = response else {
            bail!("private service did not return real health")
        };
        ensure!(
            observation.process_id == std::process::id()
                && observation
                    .catalog_clock
                    .is_some_and(|clock| clock.namespace == namespace),
            "private health differs from actual owner/catalog"
        );
        ensure!(
            observation.executable_sha256 == airk_host::runtime_pair::current_executable_sha256()?,
            "private actual executable differs"
        );
        let instance = observation.instance;
        let request = ControlMethod::ConfigureResources {
            configuration: resource_configuration.clone(),
        };
        write(&work, "resource-configuration-request.json", &request)?;
        let response = crate::ipc::control::request_once(
            &control,
            request,
            env!("CARGO_PKG_VERSION"),
            Some(instance),
        )
        .await?;
        write(&work, "resource-configuration-response.json", &response)?;
        require_software_v2_resource_observation(&response, &resource_configuration)?;
        let actual = crate::ipc::control::request_once(
            &control,
            ControlMethod::Resources,
            env!("CARGO_PKG_VERSION"),
            Some(instance),
        )
        .await?;
        write(&work, "resources-before-ready.json", &actual)?;
        require_software_v2_resource_observation(&actual, &resource_configuration)?;
        write(
            &work,
            "ready.json",
            &Ready {
                schema: "airk.private-arbiter-fixture/1".into(),
                evaluation: true,
                release_qualified: false,
                control_root: control.clone(),
                inputs_sha256: expected,
                profiles: prepared
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| ProfileBinding {
                        manifest: entry.profile.manifest,
                        model: entry.profile.model,
                        profile_sha256: entry.sha256,
                        snapshot: format!("profile-{index}.json"),
                    })
                    .collect(),
                health: *observation,
                warm_observer_id,
            },
        )?;
        Ok::<_, anyhow::Error>(instance)
    }
    .await;
    let instance = match startup {
        Ok(instance) => instance,
        Err(error) => {
            let _ = write(
                &work,
                "startup-unknown.json",
                &serde_json::json!({"schema":"airk.private-arbiter-fixture-startup/1","evaluation":true,"release_qualified":false,"startup_completed":false,"reason":format!("{error:#}"),"shutdown_completed":false}),
            );
            if !service.is_finished() {
                let _ = crate::ipc::control::request_once(
                    &control,
                    ControlMethod::Shutdown,
                    env!("CARGO_PKG_VERSION"),
                    None,
                )
                .await;
            }
            let service_failure = if observed_service_exit {
                None
            } else {
                match service.await {
                    Ok(Ok(())) => None,
                    Ok(Err(failure)) => Some(failure),
                    Err(failure) => Some(failure.into()),
                }
            };
            if let Some(recorder) = execution_recorder.take() {
                let _ = recorder.finish().await;
            }
            if let Some(recorder) = warm_recorder.take() {
                let _ = recorder.finish().await;
            }
            let failure = match service_failure {
                Some(failure) => failure.context(format!("{error:#}")),
                None => error,
            };
            return Err(failure.context(
                "private fixture startup refused; no Ready or successful closure was claimed",
            ));
        }
    };
    let lifetime = tokio::time::sleep(Duration::from_secs(seconds));
    tokio::pin!(lifetime);
    let outcome = async {
        tokio::select! {
            value=&mut service=>{observed_service_exit=true;value?},
            _=&mut lifetime=>{
                let stopped=crate::ipc::control::request_once(&control,ControlMethod::Shutdown,env!("CARGO_PKG_VERSION"),Some(instance)).await?;
                ensure!(matches!(stopped,ControlResponse::Stopping),"private lifetime shutdown was refused");
                match tokio::time::timeout(Duration::from_secs(180),&mut service).await {
                    Ok(value)=>{observed_service_exit=true;value?},
                    Err(_)=>{
                        write(&work,"shutdown-unknown.json",&serde_json::json!({"evaluation":true,"release_qualified":false,"shutdown_completed":false,"instance":instance,"reason":"Actual native shutdown has not completed; no kill, abort or cleanup proof was invented."}))?;
                        let _=(&mut service).await;
                        observed_service_exit=true;
                        bail!("private service missed shutdown deadline; actual completion was observed later")
                    }
                }
            }
        }
    }
    .await;
    if let Err(error) = outcome {
        if !observed_service_exit {
            let _ = crate::ipc::control::request_once(
                &control,
                ControlMethod::Shutdown,
                env!("CARGO_PKG_VERSION"),
                Some(instance),
            )
            .await;
            let _ = (&mut service).await;
        }
        if let Some(recorder) = execution_recorder.take() {
            let _ = recorder.finish().await;
        }
        if let Some(recorder) = warm_recorder.take() {
            let _ = recorder.finish().await;
        }
        return Err(error.context("private fixture failed; no successful closure was claimed"));
    }
    if let Some(recorder) = execution_recorder.take() {
        recorder.finish().await?;
    }
    if let Some(recorder) = warm_recorder.take() {
        recorder.finish().await?;
    }
    let original = lease_witness
        .get()
        .context("original private CUDA service lease unobserved")?;
    write(
        &work,
        "cuda-custody-before-reacquisition.json",
        &serde_json::json!({
            "schema": "airk.private.cuda-answer-shutdown-custody/1",
            "original_lease_strong_count": original.strong_count(),
            "native_cuda_poisoned": airk_retrieval::arbiter::runtime::cuda::process_poisoned(),
        }),
    )?;
    let released = airk_store::service::ServiceLease::acquire(&control)?;
    drop(released);
    write(
        &work,
        "closed.json",
        &serde_json::json!({"schema":"airk.private-arbiter-fixture-closed/1","evaluation":true,"release_qualified":false,"control_root":control,"instance":instance,"shutdown_completed":true,"service_lease_reacquired":true}),
    )?;
    Ok(())
}

#[test]
fn same_install_seeding_is_explicit_for_fresh_and_absent_for_restart() -> Result<()> {
    assert_eq!(cpu_seed(None, true)?, None);
    assert!(cpu_seed(Some("all"), true).is_err());
    assert!(cpu_seed(Some("none"), true).is_err());
    assert!(cpu_seed(Some("focus"), true).is_err());
    assert_eq!(cpu_seed(Some("all"), false)?, Some(CpuSeed::All));
    assert_eq!(cpu_seed(Some("focus"), false)?, Some(CpuSeed::Focus));
    assert_eq!(cpu_seed(Some("none"), false)?, Some(CpuSeed::None));
    assert!(cpu_seed(None, false).is_err());
    assert!(cpu_seed(Some("true"), false).is_err());
    let models = [
        ModelChoice::FocusQwenReranker06B,
        ModelChoice::ContextQwenCompact,
        ModelChoice::ContextQwenLarge,
    ];
    assert!(models.iter().all(|model| CpuSeed::All.includes(*model)));
    assert!(!models.iter().any(|model| CpuSeed::None.includes(*model)));
    assert_eq!(
        models
            .iter()
            .filter(|model| CpuSeed::Focus.includes(**model))
            .collect::<Vec<_>>(),
        vec![&ModelChoice::FocusQwenReranker06B]
    );
    Ok(())
}

#[test]
fn same_install_cuda_profile_must_be_a_distinct_unqualified_cuda_context_profile() -> Result<()> {
    let digest = |byte| Sha256Digest::from_bytes([byte; 32]);
    let cpu_profile = |model, manifest| Facts {
        manifest,
        model,
        qualified: false,
        profile_id: "private-cpu",
        cuda: false,
        device: false,
    };
    let cpu = [
        cpu_profile(ModelChoice::FocusQwenReranker06B, digest(1)),
        cpu_profile(ModelChoice::ContextQwenCompact, digest(2)),
        cpu_profile(ModelChoice::ContextQwenLarge, digest(3)),
    ];
    let large = Facts {
        manifest: digest(4),
        model: ModelChoice::ContextQwenLarge,
        qualified: false,
        profile_id: "private-cuda-fp32-context-large-batch-view-v1",
        cuda: true,
        device: true,
    };
    require_cuda_profile(&cpu, large)?;
    let compact = Facts {
        model: ModelChoice::ContextQwenCompact,
        ..large
    };
    require_cuda_profile(&cpu, compact)?;
    for refused in [
        Facts {
            model: ModelChoice::FocusQwenReranker06B,
            ..large
        },
        Facts {
            cuda: false,
            device: false,
            ..large
        },
        Facts {
            cuda: false,
            ..large
        },
        Facts {
            manifest: digest(2),
            ..large
        },
        Facts {
            qualified: true,
            ..large
        },
        Facts {
            profile_id: "arbiter-context",
            ..large
        },
    ] {
        assert!(require_cuda_profile(&cpu, refused).is_err());
    }
    // Refused: a roster without a CPU twin, or with a device profile of its own.
    assert!(require_cuda_profile(&cpu[..2], large).is_err());
    let mut with_device = cpu;
    with_device[1].device = true;
    assert!(require_cuda_profile(&with_device, compact).is_err());
    Ok(())
}
