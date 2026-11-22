// SPDX-License-Identifier: Apache-2.0
//! Private CUDA Answer fixture that can reopen the CPU fixture's own installation.
//!
//! The CPU roster stays the installation's identity: a same-install restart pins
//! the original Ready, Closed and final status exactly as the CPU selector does.
//! Up to three extra CUDA profiles (at most one Compact, one Large and one
//! strict-GPU Focus) join the evaluation catalog only; their assets are held as
//! read descriptors and never seeded into the store. Each stage switches between
//! its entries through ordinary Configure, and only the selected one per stage is
//! ever resident.

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
/// Context Compact, Context Large or Focus profile with a CUDA device binding,
/// a manifest that no CPU roster entry uses, and a CPU twin of the same model.
fn require_cuda_profile(cpu: &[Facts<'_>], cuda: Facts<'_>) -> Result<()> {
    ensure!(
        matches!(
            cuda.model,
            ModelChoice::ContextQwenCompact
                | ModelChoice::ContextQwenLarge
                | ModelChoice::FocusQwenReranker06B
        ) && !cuda.qualified
            && cuda.profile_id.starts_with("private-")
            && cuda.cuda
            && cpu
                .iter()
                .all(|profile| profile.manifest != cuda.manifest && !profile.device)
            && cpu.iter().any(|profile| profile.model == cuda.model),
        "same-install CUDA requires unqualified CUDA Compact, Large or Focus profiles beside their CPU roster"
    );
    Ok(())
}

/// The pinned CUDA roster (one to three entries), each prepared with the same
/// checks as each CPU roster entry: pinned profile bytes, authenticated asset
/// closure under artifacts and held read descriptors of the original bytes.
fn prepare_cuda(path: &Path, expected: Sha256Digest) -> Result<(Vec<u8>, Vec<Prepared>)> {
    let artifacts = artifact_root()?;
    let (_parent, mut file) = source_file(path, &artifacts)?;
    let raw_inputs = bounded(&mut file)?;
    ensure!(
        sha(&raw_inputs) == expected,
        "private CUDA roster hash differs"
    );
    let inputs: Vec<super::super::Input> = serde_json::from_slice(&raw_inputs)?;
    ensure!(
        (1..=3).contains(&inputs.len()),
        "private CUDA roster names one to three profiles"
    );
    let mut prepared = Vec::new();
    for input in &inputs {
        prepared.push(prepare_cuda_entry(input, &artifacts)?);
    }
    Ok((raw_inputs, prepared))
}

fn prepare_cuda_entry(input: &super::super::Input, artifacts: &Path) -> Result<Prepared> {
    let (_parent, mut file) = source_file(&input.profile, artifacts)?;
    let raw = bounded(&mut file)?;
    ensure!(
        sha(&raw) == input.profile_sha256,
        "private CUDA profile bytes differ from pinned input"
    );
    let profile: Profile = serde_json::from_slice(&raw)?;
    profile.validate()?;
    let root = input.assets.canonicalize()?;
    ensure!(
        root.starts_with(artifacts),
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
    Ok(Prepared {
        profile,
        raw,
        sha256: input.profile_sha256,
        sources,
        _profile_file: file,
        _assets: assets,
    })
}

/// One pinned review receipt per CUDA Context roster entry, in roster order, from
/// `AIRK_ARBITER_EVALUATION_CUDA_REVIEWS`: a JSON list of
/// `{path, sha256, profile_sha256}`, the last naming the reviewed pass-1 profile.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewPin {
    path: PathBuf,
    sha256: Sha256Digest,
    /// SHA-256 of the pass-1 profile the receipt reviewed.
    profile_sha256: Sha256Digest,
}

/// One CUDA entry as the seam attaches it.
struct PlannedEntry {
    manifest: Sha256Digest,
    adapter: [u8; 16],
    route: crate::service_runtime::arbiter::evaluation::PrivateGpuRoute,
    /// The review pin: a Context entry's graph or bounded policy, or
    /// `strict_gpu` for a Focus entry, whose placement is verified at open.
    pin: String,
}

/// The inputs of every CUDA entry. Context entries take the protected review
/// receipts when they are named, in Context roster order, else the original
/// single reviewed graph result. A Focus entry takes no review: its strict
/// placement is checked against the live session when it opens.
fn cuda_graphs(
    artifacts: &Path,
    cpu: &[Prepared],
    cuda: &[Prepared],
    expires: Instant,
) -> Result<Vec<PlannedEntry>> {
    use crate::service_runtime::arbiter::evaluation::PrivateGpuRoute;
    let models: Vec<ModelChoice> = cuda.iter().map(|entry| entry.profile.model).collect();
    let (focus, context): (Vec<&Prepared>, Vec<&Prepared>) = cuda
        .iter()
        .partition(|entry| entry.profile.model == ModelChoice::FocusQwenReranker06B);
    ensure!(
        focus.len() <= 1,
        "the CUDA roster holds at most one Focus entry"
    );
    let mut planned = Vec::new();
    match std::env::var_os("AIRK_ARBITER_EVALUATION_CUDA_REVIEWS") {
        None => {
            require_plan_shape(&models, None)?;
            ensure!(
                context.len() <= 1 && focus.is_empty(),
                "two CUDA Context entries or a CUDA Focus entry require AIRK_ARBITER_EVALUATION_CUDA_REVIEWS"
            );
            if let Some(entry) = context.first() {
                let manifest = entry.profile.manifest;
                let input = Input::read_matching(artifacts, expires, |named| {
                    ensure!(
                        named == manifest,
                        "reviewed CUDA graph result names another profile"
                    );
                    Ok(())
                })?;
                planned.push(PlannedEntry {
                    manifest: input.manifest,
                    adapter: input.adapter,
                    pin: input.pin.to_string(),
                    route: PrivateGpuRoute::Answer(input.graph),
                });
            }
        }
        Some(reviews) => {
            let reviews: Vec<ReviewPin> = serde_json::from_str(
                reviews
                    .to_str()
                    .context("CUDA review list is not Unicode")?,
            )?;
            require_plan_shape(&models, Some(reviews.len()))?;
            let adapter: [u8; 16] =
                serde_json::from_str(&std::env::var("AIRK_ARBITER_EVALUATION_CUDA_ADAPTER_UUID")?)?;
            for (review, entry) in reviews.iter().zip(&context) {
                let input = Input::read_review(
                    artifacts,
                    &review.path,
                    review.sha256,
                    review.profile_sha256,
                    &entry.profile,
                    adapter,
                    expires,
                )?;
                planned.push(PlannedEntry {
                    manifest: input.manifest,
                    adapter: input.adapter,
                    pin: input.pin.to_string(),
                    route: PrivateGpuRoute::Answer(input.graph),
                });
            }
            for entry in focus {
                ensure!(adapter != [0; 16], "private CUDA adapter identity missing");
                let parity: ParityPin = serde_json::from_str(
                    &std::env::var("AIRK_ARBITER_EVALUATION_CUDA_FOCUS_PARITY")
                        .context("a CUDA Focus entry requires its pinned parity receipt")?,
                )?;
                let (_directory, mut file) = source_file(&parity.path, artifacts)?;
                let raw = bounded(&mut file)?;
                ensure!(
                    sha(&raw) == parity.sha256,
                    "CUDA Focus parity receipt SHA differs"
                );
                let cpu_focus: Vec<Sha256Digest> = cpu
                    .iter()
                    .filter(|twin| twin.profile.model == ModelChoice::FocusQwenReranker06B)
                    .map(|twin| twin.profile.manifest)
                    .collect();
                require_focus_parity(
                    &serde_json::from_slice(&raw)?,
                    entry.profile.manifest,
                    &cpu_focus,
                )?;
                planned.push(PlannedEntry {
                    manifest: entry.profile.manifest,
                    adapter,
                    pin: format!("strict_gpu+parity:{}", parity.sha256),
                    route: PrivateGpuRoute::Focus { expires },
                });
            }
        }
    }
    ensure!(!planned.is_empty(), "the CUDA roster has no entry");
    Ok(planned)
}

/// The shape of a CUDA roster: at most one Focus entry; without review pins at
/// most one Context entry and no Focus entry; with them, one pin per Context
/// entry.
fn require_plan_shape(models: &[ModelChoice], reviews: Option<usize>) -> Result<()> {
    let focus = models
        .iter()
        .filter(|model| **model == ModelChoice::FocusQwenReranker06B)
        .count();
    let context = models.len() - focus;
    ensure!(focus <= 1, "the CUDA roster holds at most one Focus entry");
    match reviews {
        None => ensure!(
            context <= 1 && focus == 0,
            "two CUDA Context entries or a CUDA Focus entry require AIRK_ARBITER_EVALUATION_CUDA_REVIEWS"
        ),
        Some(count) => ensure!(
            count == context,
            "one CUDA review receipt per CUDA Context roster entry"
        ),
    }
    Ok(())
}

/// The parity a CUDA Focus entry must show against its CPU twin, from the
/// pinned `l5c_focus_parity` receipt (`AIRK_ARBITER_EVALUATION_CUDA_FOCUS_PARITY`,
/// `{path, sha256}`): complete and passed, at least 8 compiled pairs, every
/// winner agreeing and every raw score within a tolerance of at most 1e-3, for
/// exactly this CUDA manifest against a CPU Focus profile of the roster.
const FOCUS_PARITY_TOLERANCE: f64 = 1e-3;
const FOCUS_PARITY_ROWS: u64 = 8;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParityPin {
    path: PathBuf,
    sha256: Sha256Digest,
}

fn require_focus_parity(
    receipt: &serde_json::Value,
    cuda_manifest: Sha256Digest,
    cpu_manifests: &[Sha256Digest],
) -> Result<()> {
    let tolerance = receipt["tolerance"].as_f64().unwrap_or(f64::INFINITY);
    let rows = receipt["row_count"].as_u64().unwrap_or(0);
    let worst = receipt["max_abs"].as_f64().unwrap_or(f64::INFINITY);
    let cuda: Option<Sha256Digest> =
        serde_json::from_value(receipt["cuda"]["manifest"].clone()).ok();
    let cpu: Option<Sha256Digest> = serde_json::from_value(receipt["cpu"]["manifest"].clone()).ok();
    ensure!(
        receipt["schema"] == "airk.private.l5c-focus-parity/1"
            && receipt["complete"] == true
            && receipt["passed"] == true
            && receipt.get("error").is_none()
            && tolerance <= FOCUS_PARITY_TOLERANCE
            && rows >= FOCUS_PARITY_ROWS
            && receipt["winners_agree"].as_u64() == Some(rows)
            && worst <= tolerance,
        "the CUDA Focus parity receipt does not show a passing parity within 1e-3"
    );
    ensure!(
        cuda == Some(cuda_manifest) && cpu.is_some_and(|cpu| cpu_manifests.contains(&cpu)),
        "the CUDA Focus parity receipt is of another profile or CPU twin"
    );
    Ok(())
}

/// The roles a CUDA Focus profile must share byte for byte with its CPU twin.
const FOCUS_TWIN_ROLES: [&str; 3] = ["compiler", "tokenizer", "fixture"];
/// The profile fields a CUDA Focus profile must share with its CPU twin.
const FOCUS_TWIN_FIELDS: [&str; 8] = [
    "model",
    "tasks",
    "task_policies",
    "max_tokens",
    "max_candidates",
    "compiler",
    "abi",
    "policy",
];

/// Binds an unreviewed strict-GPU CUDA Focus profile to a CPU twin in the
/// roster: the same compiler, tokenizer and fixture assets, the same task,
/// policies and limits, and, for an FP32-activation route, the same graph and
/// weights. An FP16 Focus route has no verifiable weight lineage here and is
/// refused. Numeric parity with the twin is not checked here; it is an LG item.
fn require_focus_twin(cpu: &[serde_json::Value], cuda: &serde_json::Value) -> Result<()> {
    let assets = |profile: &serde_json::Value, roles: &[&str]| -> BTreeSet<String> {
        profile["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|asset| roles.iter().any(|role| asset["role"] == *role))
            .map(|asset| asset.to_string())
            .collect()
    };
    let precision = cuda["device_execution"]["precision"].as_str();
    ensure!(
        precision == Some("float32"),
        "a CUDA Focus entry needs an FP32-activation route; FP16 Focus weight lineage is not verifiable here"
    );
    let weights = ["model", "external_weights"];
    let bound = cpu.iter().any(|twin| {
        twin["device_execution"].is_null()
            && FOCUS_TWIN_FIELDS
                .iter()
                .all(|field| twin[field] == cuda[field])
            && assets(twin, &FOCUS_TWIN_ROLES) == assets(cuda, &FOCUS_TWIN_ROLES)
            && assets(twin, &weights) == assets(cuda, &weights)
            && !assets(cuda, &weights).is_empty()
    });
    ensure!(
        bound,
        "the CUDA Focus profile differs from every CPU Focus profile in its assets, task or limits"
    );
    Ok(())
}

/// The CUDA entries are distinct models: at most one Compact, one Large and
/// one Focus.
fn require_distinct_models(cuda: &[Facts<'_>]) -> Result<()> {
    let models: BTreeSet<_> = cuda.iter().map(|facts| facts.model).collect();
    ensure!(
        models.len() == cuda.len(),
        "the CUDA roster holds at most one profile per model"
    );
    Ok(())
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
    let cpu_facts = prepared
        .iter()
        .map(|entry| Facts::of(&entry.profile))
        .collect::<Vec<_>>();
    let cpu_values = prepared
        .iter()
        .map(|entry| serde_json::to_value(&entry.profile))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for entry in &cuda {
        require_cuda_profile(&cpu_facts, Facts::of(&entry.profile))?;
        if entry.profile.model == ModelChoice::FocusQwenReranker06B {
            require_focus_twin(&cpu_values, &serde_json::to_value(&entry.profile)?)?;
        }
    }
    require_distinct_models(
        &cuda
            .iter()
            .map(|entry| Facts::of(&entry.profile))
            .collect::<Vec<_>>(),
    )?;
    let cuda_manifests: Vec<Sha256Digest> =
        cuda.iter().map(|entry| entry.profile.manifest).collect();
    let cuda_models: Vec<ModelChoice> = cuda.iter().map(|entry| entry.profile.model).collect();
    let graphs = cuda_graphs(
        &artifacts,
        &prepared,
        &cuda,
        started + Duration::from_secs(seconds),
    )?;
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
    // CPU roster first in its original order, then the CUDA profiles, so the
    // configuration emitter finds every manifest in this lifetime's Ready.
    prepared.extend(cuda);
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
            "cuda_manifests": cuda_manifests,
            "cuda_models": cuda_models,
            "graph_pins": graphs
                .iter()
                .map(|graph| graph.pin.clone())
                .collect::<Vec<_>>(),
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
        let mut readers = Vec::new();
        for graph in &graphs {
            let entry = source
                .iter()
                .find(|entry| entry.profile.manifest == graph.manifest)
                .context("private CUDA Answer asset profile missing")?;
            readers.push(move |asset: &Asset| -> Result<std::fs::File> {
                entry
                    .sources
                    .get(&asset.name)
                    .context("original private CUDA asset descriptor missing")?
                    .try_clone()
                    .map_err(Into::into)
            });
        }
        let cuda_manifests: Vec<Sha256Digest> = graphs.iter().map(|graph| graph.manifest).collect();
        owner = owner.with_private_gpu(
            graphs
                .into_iter()
                .zip(readers.iter_mut())
                .map(|(graph, reader)| {
                    crate::service_runtime::arbiter::evaluation::PrivateGpuEntry {
                        manifest: graph.manifest,
                        adapter: graph.adapter,
                        route: graph.route,
                        source: reader,
                    }
                })
                .collect(),
        )?;
        if let Some(seed) = seed {
            for entry in source.iter().filter(|entry| {
                !cuda_manifests.contains(&entry.profile.manifest)
                    && seed.includes(entry.profile.model)
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
    // A CUDA Focus profile with a CPU Focus twin joins too.
    require_cuda_profile(
        &cpu,
        Facts {
            model: ModelChoice::FocusQwenReranker06B,
            manifest: digest(8),
            ..large
        },
    )?;
    for refused in [
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
    // Two CUDA entries are one Compact and one Large; two of one model are refused.
    require_distinct_models(&[compact, large])?;
    require_distinct_models(&[large])?;
    assert!(
        require_distinct_models(&[
            large,
            Facts {
                manifest: digest(5),
                ..large
            }
        ])
        .is_err()
    );
    // Refused: a roster without a CPU twin, or with a device profile of its own.
    assert!(require_cuda_profile(&cpu[..2], large).is_err());
    let mut with_device = cpu;
    with_device[1].device = true;
    assert!(require_cuda_profile(&with_device, compact).is_err());
    Ok(())
}

#[test]
fn the_cuda_roster_shape_follows_its_review_pins() -> Result<()> {
    use ModelChoice::{ContextQwenCompact as C, ContextQwenLarge as L, FocusQwenReranker06B as F};
    require_plan_shape(&[C], None)?;
    require_plan_shape(&[C, L, F], Some(2))?;
    require_plan_shape(&[F], Some(0))?;
    // A Focus entry or two Context entries need review pins.
    assert!(require_plan_shape(&[F], None).is_err());
    assert!(require_plan_shape(&[C, L], None).is_err());
    // One pin per Context entry, never per Focus entry.
    assert!(require_plan_shape(&[C, F], Some(2)).is_err());
    assert!(require_plan_shape(&[C, L], Some(1)).is_err());
    // At most one Focus entry.
    assert!(require_plan_shape(&[F, F], Some(0)).is_err());
    Ok(())
}

#[test]
fn a_cuda_focus_profile_must_match_its_cpu_twin() -> Result<()> {
    use serde_json::json;
    let asset = |name: &str, role: &str, byte: u8| json!({"name": name, "role": role, "bytes": 10, "sha256": format!("{byte:02x}").repeat(32)});
    let cpu = json!({
        "model": "arbiter-focus-qwen3-reranker-06b", "tasks": ["airk.focus_rank.v1"],
        "task_policies": {"airk.focus_rank.v1": "aa"}, "max_tokens": 1024, "max_candidates": 4,
        "compiler": "cc", "abi": {"readout": "focus_yes_no"}, "policy": "pp", "device_execution": null,
        "assets": [asset("compiler.json", "compiler", 1), asset("tokenizer.json", "tokenizer", 2),
                   asset("fixture.json", "fixture", 3), asset("model.onnx", "model", 4),
                   asset("model.onnx.data", "external_weights", 5),
                   asset("focus-relevance-metadata.json", "calibration", 6)],
    });
    let mut cuda = cpu.clone();
    cuda["device_execution"] = json!({"precision": "float32", "placement": {"mode": "strict_gpu"}});
    // A device-specific calibration asset may differ.
    cuda["assets"][5] = asset("focus-relevance-metadata.json", "calibration", 7);
    require_focus_twin(std::slice::from_ref(&cpu), &cuda)?;
    for (field, value) in [
        ("max_tokens", json!(512)),
        ("max_candidates", json!(8)),
        ("tasks", json!(["airk.context_action.v1"])),
        ("compiler", json!("other")),
    ] {
        let mut changed = cuda.clone();
        changed[field] = value;
        assert!(
            require_focus_twin(std::slice::from_ref(&cpu), &changed).is_err(),
            "{field}"
        );
    }
    for (index, role) in [
        (0, "compiler"),
        (1, "tokenizer"),
        (2, "fixture"),
        (4, "external_weights"),
    ] {
        let mut changed = cuda.clone();
        changed["assets"][index] = asset("x", role, 9);
        assert!(
            require_focus_twin(std::slice::from_ref(&cpu), &changed).is_err(),
            "{role}"
        );
    }
    let mut fp16 = cuda.clone();
    fp16["device_execution"]["precision"] = json!("float16");
    assert!(require_focus_twin(std::slice::from_ref(&cpu), &fp16).is_err());
    // The twin must be a CPU profile.
    assert!(require_focus_twin(&[cuda.clone()], &cuda).is_err());
    Ok(())
}

#[test]
fn a_cuda_focus_entry_needs_a_passing_parity_receipt_for_its_twin() {
    use serde_json::json;
    let digest = |byte| Sha256Digest::from_bytes([byte; 32]);
    let receipt = json!({"schema": "airk.private.l5c-focus-parity/1", "complete": true, "passed": true,
        "tolerance": 0.001, "row_count": 15, "winners_agree": 15, "max_abs": 1.7e-5,
        "cuda": {"manifest": digest(1)}, "cpu": {"manifest": digest(2)}});
    assert!(require_focus_parity(&receipt, digest(1), &[digest(2)]).is_ok());
    for (field, value) in [
        ("passed", json!(false)),
        ("complete", json!(false)),
        ("tolerance", json!(0.01)),
        ("row_count", json!(4)),
        ("winners_agree", json!(14)),
        ("max_abs", json!(0.002)),
    ] {
        let mut changed = receipt.clone();
        changed[field] = value;
        assert!(
            require_focus_parity(&changed, digest(1), &[digest(2)]).is_err(),
            "{field}"
        );
    }
    // The receipt must be of this CUDA profile and of a CPU twin in the roster.
    assert!(require_focus_parity(&receipt, digest(3), &[digest(2)]).is_err());
    assert!(require_focus_parity(&receipt, digest(1), &[digest(4)]).is_err());
}
