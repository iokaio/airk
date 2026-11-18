// SPDX-License-Identifier: Apache-2.0
//! Private ignored native evaluation host; never mounted in a product role.
use super::*;
use airk_core::{
    arbiter::{ModelChoice, Profile},
    identity::Sha256Digest,
};
use airk_process::directory::OpenedDirectory;
use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs::File,
    io::Read,
    path::PathBuf,
};

mod acquisition;
#[cfg(all(windows, target_arch = "x86_64", feature = "gpu-cuda"))]
mod cuda_answer;
mod execution_observer;
mod readable_selection;
mod restart;
mod warm_journal;

const LIMIT: usize = 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    profile: PathBuf,
    assets: PathBuf,
    fixtures: PathBuf,
    profile_sha256: Sha256Digest,
}
struct Prepared {
    profile: Profile,
    raw: Vec<u8>,
    sha256: Sha256Digest,
    sources: BTreeMap<String, File>,
    _profile_file: File,
    _assets: OpenedDirectory,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileBinding {
    manifest: Sha256Digest,
    model: ModelChoice,
    profile_sha256: Sha256Digest,
    snapshot: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ready {
    schema: String,
    evaluation: bool,
    release_qualified: bool,
    control_root: PathBuf,
    inputs_sha256: Sha256Digest,
    profiles: Vec<ProfileBinding>,
    health: airk_core::service::ServiceHealth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    warm_observer_id: Option<airk_core::identity::Incarnation>,
}
fn sha(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn bounded(file: &mut File) -> Result<Vec<u8>> {
    ensure!(
        file.metadata()?.is_file() && file.metadata()?.len() <= LIMIT as u64,
        "private fixture input exceeds bound"
    );
    let mut bytes = Vec::new();
    file.take(LIMIT as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= LIMIT,
        "private fixture input grew beyond bound"
    );
    Ok(bytes)
}
fn artifact_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("repository root unavailable")?
        .join("artifacts")
        .canonicalize()
        .map_err(Into::into)
}
fn source_file(path: &Path, artifacts: &Path) -> Result<(OpenedDirectory, File)> {
    ensure!(
        path.is_absolute(),
        "private fixture source must be absolute"
    );
    let parent = path
        .parent()
        .context("private fixture source parent missing")?
        .canonicalize()?;
    ensure!(
        parent.starts_with(artifacts),
        "private evaluation inputs must remain under repository artifacts"
    );
    let directory = OpenedDirectory::open(&parent)?;
    let file = directory.open_file(
        path.file_name()
            .context("private fixture source name missing")?,
    )?;
    Ok((directory, file))
}
fn prepare(path: &Path, expected: Sha256Digest) -> Result<(Vec<u8>, Vec<Prepared>)> {
    let artifacts = artifact_root()?;
    let (_parent, mut file) = source_file(path, &artifacts)?;
    let raw = bounded(&mut file)?;
    ensure!(
        sha(&raw) == expected,
        "private fixture input manifest hash differs"
    );
    let inputs: Vec<Input> = serde_json::from_slice(&raw)?;
    ensure!(
        inputs.len() == 3,
        "private fixture requires exactly the three selected stage profiles"
    );
    let mut models = BTreeSet::new();
    let mut manifests = BTreeSet::new();
    let mut prepared = Vec::new();
    for input in inputs {
        let (_parent, mut file) = source_file(&input.profile, &artifacts)?;
        let raw = bounded(&mut file)?;
        ensure!(
            sha(&raw) == input.profile_sha256,
            "private profile bytes differ from pinned input"
        );
        let profile: Profile = serde_json::from_slice(&raw)?;
        profile.validate()?;
        ensure!(
            profile.qualification.is_none(),
            "private evaluation must never relabel a product-qualified profile"
        );
        ensure!(
            models.insert(profile.model) && manifests.insert(profile.manifest),
            "private profiles duplicate model choice or manifest"
        );
        let root = input.assets.canonicalize()?;
        ensure!(
            root.starts_with(&artifacts),
            "private model assets leave artifacts"
        );
        let assets = OpenedDirectory::open(&root)?;
        let fixture_name = input
            .fixtures
            .file_name()
            .context("private compiler fixture name missing")?;
        ensure!(
            input.fixtures.canonicalize()? == root.join(fixture_name).canonicalize()?
                && profile
                    .assets
                    .iter()
                    .any(|asset| OsStr::new(&asset.name) == fixture_name),
            "compiler fixture is outside the profile's authenticated asset closure"
        );
        let mut sources = BTreeMap::new();
        for asset in &profile.assets {
            let source = assets.open_file(OsStr::new(&asset.name))?;
            ensure!(
                source.metadata()?.len() == asset.bytes,
                "private asset length differs before seeding"
            );
            sources.insert(asset.name.clone(), source);
        }
        prepared.push(Prepared {
            profile,
            raw,
            sha256: input.profile_sha256,
            sources,
            _profile_file: file,
            _assets: assets,
        });
    }
    ensure!(
        models
            == BTreeSet::from([
                ModelChoice::FocusQwenReranker06B,
                ModelChoice::ContextQwenCompact,
                ModelChoice::ContextQwenLarge
            ]),
        "private model roster differs"
    );
    Ok((raw, prepared))
}
fn write(work: &Path, name: &str, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    crate::ipc::private_record::write_private(work, name, &bytes, 8 * LIMIT, false)
}

fn context_template() -> Result<airk_core::arbiter::context_action::ContextRequestV2> {
    use airk_core::arbiter::{
        Budget,
        context_action::{ContextRequestV2 as ContextRequest, REQUEST_SCHEMA_V2 as REQUEST_SCHEMA},
    };
    // This named historical control must not inherit a later shipping default.
    let budget = Budget {
        calls: 8,
        rounds: 3,
        fanout: 8,
        input_tokens: 2048,
        candidates: 32,
        max_bytes: 262144,
        deadline_ms: 30000,
    };
    ensure!(
        (
            budget.calls,
            budget.rounds,
            budget.fanout,
            budget.input_tokens,
            budget.candidates,
            budget.max_bytes,
            budget.deadline_ms
        ) == (8, 3, 8, 2048, 32, 262144, 30000),
        "explicit historical goal budget differs from its frozen control"
    );
    let request = ContextRequest {
        schema: REQUEST_SCHEMA.into(),
        question: "__PRIVATE_FROZEN_QUESTION__".into(),
        budget,
    };
    request.validate()?;
    Ok(request)
}

fn answer_template() -> Result<serde_json::Value> {
    answer_template_for(context_template()?)
}
fn software_v2_context_template() -> Result<airk_core::arbiter::context_action::ContextRequestV2> {
    use airk_core::arbiter::{
        Budget,
        context_action::{ContextRequestV2, REQUEST_SCHEMA_V2},
        resource_policy::ResourcePolicy,
    };
    let policy = ResourcePolicy::software_v2();
    policy.validate()?;
    let budget = Budget::default();
    ensure!(
        (
            budget.calls,
            budget.rounds,
            budget.fanout,
            budget.input_tokens,
            budget.candidates,
            budget.max_bytes,
            budget.deadline_ms
        ) == (8, 3, 8, 8192, 32, 262144, 120000),
        "software-v2 defaults differ from the separately frozen current profile"
    );
    ensure!(
        budget.input_tokens == policy.shared_input_tokens
            && budget.deadline_ms == policy.deadline_ms,
        "current fixture budget differs from native software-v2 resource policy"
    );
    let request = ContextRequestV2 {
        schema: REQUEST_SCHEMA_V2.into(),
        question: "__PRIVATE_FROZEN_QUESTION__".into(),
        budget,
    };
    request.validate()?;
    Ok(request)
}
fn software_v2_answer_template() -> Result<serde_json::Value> {
    answer_template_for(software_v2_context_template()?)
}
fn answer_template_for(
    request: airk_core::arbiter::context_action::ContextRequestV2,
) -> Result<serde_json::Value> {
    let mut request = serde_json::to_value(request)?;
    let response = airk_mcp::response::ResponseArgs {
        response_mode: Some(airk_mcp::response::ResponseMode::Full),
        include_status: true,
        ..Default::default()
    };
    request
        .as_object_mut()
        .context("answer template is not an object")?
        .extend(
            serde_json::to_value(response)?
                .as_object()
                .context("response template is not an object")?
                .clone(),
        );
    let decoded: airk_mcp::AnswerArgs = serde_json::from_value(request.clone())?;
    decoded.request.validate()?;
    ensure!(
        decoded.response.response_mode == Some(airk_mcp::response::ResponseMode::Full)
            && decoded.response.include_status,
        "answer template lost full native response policy"
    );
    Ok(request)
}

#[test]
fn native_goal_context_template_has_the_independent_frozen_instance() -> Result<()> {
    assert_eq!(
        serde_json::to_value(context_template()?)?,
        serde_json::json!({"schema":"airk.arbiter.context.request/2","question":"__PRIVATE_FROZEN_QUESTION__","budget":{"calls":8,"rounds":3,"fanout":8,"input_tokens":2048,"candidates":32,"max_bytes":262144,"deadline_ms":30000}})
    );
    let answer = answer_template()?;
    assert_eq!(answer["response_mode"], "full");
    assert_eq!(answer["include_status"], true);
    let mut core = answer.as_object().unwrap().clone();
    core.remove("response_mode");
    core.remove("include_status");
    assert_eq!(
        serde_json::Value::Object(core),
        serde_json::to_value(context_template()?)?
    );
    if let Some(path) = std::env::var_os("AIRK_ARBITER_CONTEXT_REQUEST_OUTPUT") {
        use std::io::Write;
        let path = PathBuf::from(path);
        ensure!(
            path.is_absolute()
                && path
                    .parent()
                    .context("context template output parent missing")?
                    .canonicalize()?
                    .starts_with(artifact_root()?),
            "context template output leaves artifacts"
        );
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        let mut bytes = serde_json::to_vec_pretty(&answer_template()?)?;
        bytes.push(b'\n');
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    Ok(())
}

#[test]
fn native_software_v2_context_template_has_literal_budget_and_complete_response_options()
-> Result<()> {
    let request = software_v2_answer_template()?;
    assert_eq!(
        request,
        serde_json::json!({"schema":"airk.arbiter.context.request/2",
        "question":"__PRIVATE_FROZEN_QUESTION__","budget":{"calls":8,"rounds":3,"fanout":8,
            "input_tokens":8192,"candidates":32,"max_bytes":262144,"deadline_ms":120000},
        "response_mode":"full","include_status":true})
    );
    let decoded: airk_mcp::AnswerArgs = serde_json::from_value(request.clone())?;
    decoded.request.validate()?;
    assert_eq!(
        serde_json::to_value(decoded.response)?,
        serde_json::to_value(airk_mcp::response::ResponseArgs {
            response_mode: Some(airk_mcp::response::ResponseMode::Full),
            include_status: true,
            ..Default::default()
        })?
    );
    assert_eq!(context_template()?.budget.input_tokens, 2048);
    assert_eq!(context_template()?.budget.deadline_ms, 30000);
    if let Some(path) = std::env::var_os("AIRK_ARBITER_SOFTWARE_V2_CONTEXT_REQUEST_OUTPUT") {
        use std::io::Write;
        let path = PathBuf::from(path);
        ensure!(
            path.is_absolute()
                && path
                    .parent()
                    .context("current template output parent missing")?
                    .canonicalize()?
                    .starts_with(artifact_root()?),
            "current template output leaves artifacts"
        );
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        let mut bytes = serde_json::to_vec_pretty(&request)?;
        bytes.push(b'\n');
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    Ok(())
}

/// Invoke the compiled test binary directly with --ignored --exact and the
/// documented environment. No public command/configuration can attach it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "private actual native evaluation service; explicit frozen inputs and lifetime required"]
async fn actual_private_arbiter_fixture() -> Result<()> {
    run_actual_private_arbiter_fixture(answer_template()?, None, false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "private current8192/120 native evaluation service; explicit reviewed inputs and lifetime required"]
async fn actual_private_arbiter_fixture_software_v2() -> Result<()> {
    let consent = std::env::var("AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT")
        .context("current fixture requires explicit resource configuration consent")?;
    let resources = software_v2_resource_configuration(&consent)?;
    run_actual_private_arbiter_fixture(software_v2_answer_template()?, Some(resources), false).await
}

fn software_v2_resource_configuration(
    consent: &str,
) -> Result<airk_core::service_resources::ResourceConfiguration> {
    use airk_core::service_resources::{ResourceConfiguration, ResourcePreset};
    ensure!(
        consent == "throughput64gib4threads",
        "current fixture requires explicit throughput64gib4threads resource consent"
    );
    let configuration = ResourceConfiguration {
        preset: ResourcePreset::Throughput,
        memory_bytes: Some(64 << 30),
        inference_threads: Some(4),
        ..ResourceConfiguration::default()
    };
    configuration.validate()?;
    Ok(configuration)
}

fn require_software_v2_resource_observation(
    response: &ControlResponse,
    expected: &airk_core::service_resources::ResourceConfiguration,
) -> Result<()> {
    let ControlResponse::Resources {
        observation,
        configuration_error: false,
        ..
    } = response
    else {
        bail!("current fixture native resource configuration refused or unknown")
    };
    ensure!(
        serde_json::to_value(&observation.configuration)? == serde_json::to_value(expected)?,
        "current fixture actual resource configuration differs"
    );
    // Pressure/available memory/reservations are retained facts, not proof of
    // native load or fit. Ordinary native admission remains the only owner.
    Ok(())
}

#[test]
fn current_private_fixture_resources_require_explicit_consent_and_exact_native_configuration()
-> Result<()> {
    use airk_core::service_resources::ResourcePreset;
    for invalid in ["", "true", "balanced", "throughput", "throughput64gib"] {
        assert!(software_v2_resource_configuration(invalid).is_err());
    }
    let configuration = software_v2_resource_configuration("throughput64gib4threads")?;
    assert_eq!(configuration.preset, ResourcePreset::Throughput);
    assert_eq!(configuration.memory_bytes, Some(68719476736));
    assert_eq!(configuration.inference_threads, Some(4));
    assert_eq!(configuration.extraction_threads, None);
    assert_eq!(configuration.background_io_bytes_per_second, None);
    assert_eq!(configuration.query_concurrency, 16);
    assert!(
        require_software_v2_resource_observation(
            &ControlResponse::Failure {
                code: "resource_configuration_outcome_unknown".into()
            },
            &configuration
        )
        .is_err()
    );
    let balanced = airk_kernel::service_resources::Resources::new(
        airk_core::service_resources::ResourceConfiguration::default(),
        32,
        Some(128 << 30),
    )?;
    let wrong_configuration = ControlResponse::Resources {
        observation: Box::new(balanced.snapshot()),
        age_ms: 0,
        configuration_error: false,
    };
    assert!(
        require_software_v2_resource_observation(&wrong_configuration, &configuration).is_err()
    );
    let requested =
        airk_kernel::service_resources::Resources::new(configuration.clone(), 32, Some(128 << 30))?;
    let mut observed = requested.snapshot();
    // A configured ceiling is deliberately not a claim that pressure is absent.
    observed.pressure = true;
    let actual = ControlResponse::Resources {
        observation: Box::new(observed),
        age_ms: 0,
        configuration_error: false,
    };
    require_software_v2_resource_observation(&actual, &configuration)?;
    Ok(())
}

async fn run_actual_private_arbiter_fixture(
    context_request: serde_json::Value,
    resource_configuration: Option<airk_core::service_resources::ResourceConfiguration>,
    cuda_answer: bool,
) -> Result<()> {
    let started = Instant::now();
    // The selector's Rust owner supplies the whole frozen request, including
    // current MCP response options. Environment/input metadata cannot rewrite it.
    let decoded: airk_mcp::AnswerArgs = serde_json::from_value(context_request.clone())?;
    decoded.request.validate()?;
    let input = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_EVALUATION_INPUTS")
            .context("AIRK_ARBITER_EVALUATION_INPUTS missing")?,
    );
    let expected: Sha256Digest = std::env::var("AIRK_ARBITER_EVALUATION_INPUTS_SHA256")?.parse()?;
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
    let (inputs, prepared) = prepare(&input, expected)?;
    #[cfg(all(windows, target_arch = "x86_64", feature = "gpu-cuda"))]
    let cuda_answer = cuda_answer
        .then(|| {
            cuda_answer::Input::read(
                &artifacts,
                &prepared,
                started + Duration::from_secs(seconds),
            )
        })
        .transpose()?;
    #[cfg(not(all(windows, target_arch = "x86_64", feature = "gpu-cuda")))]
    let _ = (cuda_answer, started);
    let parent_pin = OpenedDirectory::open(&parent)?;
    let work_pin = crate::ipc::endpoint::create_root_at(
        &parent_pin,
        work.file_name().context("private work name missing")?,
    )?;
    let restart = restart::from_env(expected, &prepared)?;
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
    // Use the actual first-start transaction, actual engine identity and real
    // activation/readiness producer; do not seed catalog rows or fake I4.
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    let lease = if restart.is_none() {
        first_start::ensure_catalog(lease)?
    } else {
        lease
    };
    drop(lease);
    let prepared = Arc::new(prepared);
    let context_observer = match std::env::var("AIRK_SELECTED_CONTEXT_CAL_OBSERVER") {
        Ok(value) if value == "true" => Some(selected_cal_observer(prepared.clone(), &work)?),
        Ok(value) if value == "false" => None,
        Err(std::env::VarError::NotPresent) => None,
        _ => bail!("private CAL observer mode must be explicit true/false"),
    };
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
    let factory: EvaluationFactory = Box::new(move |lease, gate| {
        let namespace = gate
            .catalog_clock()
            .context("actual fixture catalog clock missing")?
            .namespace;
        let mut owner = if let Some(saved) = &restart {
            ensure!(
                namespace == saved.configuration.catalog,
                "Restart catalog namespace changed"
            );
            EvaluationOwner::reopen_same_configuration(
                lease,
                gate,
                source.iter().map(|entry| entry.profile.clone()).collect(),
                &saved.configuration,
            )?
        } else {
            EvaluationOwner::new(
                lease,
                gate,
                source.iter().map(|entry| entry.profile.clone()).collect(),
            )?
        };
        for entry in source.iter().filter(|_| restart.is_none()) {
            owner.seed(entry.profile.manifest, &mut |asset| {
                entry
                    .sources
                    .get(&asset.name)
                    .context("original private asset descriptor missing")?
                    .try_clone()
                    .map_err(Into::into)
            })?;
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
            "private fixture must start disabled"
        );
        #[cfg(all(windows, target_arch = "x86_64", feature = "gpu-cuda"))]
        if let Some(input) = cuda_answer {
            ensure!(
                restart.is_none() && context_observer.is_none(),
                "paired CUDA Answer requires actual scoring in a fresh isolated service"
            );
            owner = owner.with_cuda_answer(input.manifest, input.adapter, input.graph)?;
        }
        if let Some(observer) = context_observer {
            owner = owner.observe_context(observer);
        }
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
    if let Some(configuration) = &resource_configuration {
        let request = ControlMethod::ConfigureResources { configuration: configuration.clone() };
        write(&work, "resource-configuration-request.json", &request)?;
        let response = crate::ipc::control::request_once(
            &control, request, env!("CARGO_PKG_VERSION"), Some(instance),
        ).await?;
        write(&work, "resource-configuration-response.json", &response)?;
        require_software_v2_resource_observation(&response, configuration)?;
        let actual = crate::ipc::control::request_once(
            &control, ControlMethod::Resources, env!("CARGO_PKG_VERSION"), Some(instance),
        ).await?;
        write(&work, "resources-before-ready.json", &actual)?;
        require_software_v2_resource_observation(&actual, configuration)?;
    }
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
        Ok::<_,anyhow::Error>(instance)
    }.await;
    let instance = match startup {
        Ok(instance) => instance,
        Err(error) => {
            let _ = write(
                &work,
                "startup-unknown.json",
                &serde_json::json!({"schema":"airk.private-arbiter-fixture-startup/1","evaluation":true,"release_qualified":false,"startup_completed":false,"reason":format!("{error:#}"),"shutdown_completed":false}),
            );
            if !service.is_finished() {
                // The root is fresh, private and owned by this actual task.
                // No process kill/abort substitutes for ordinary shutdown.
                let _ = crate::ipc::control::request_once(
                    &control,
                    ControlMethod::Shutdown,
                    env!("CARGO_PKG_VERSION"),
                    None,
                )
                .await;
            }
            // Seed timeout drops its receiver, making the factory fail as soon
            // as its current actual descriptor transaction returns. Retain the
            // task until native ownership truly completes, even after refusal.
            if !observed_service_exit {
                let _ = service.await;
            }
            if let Some(recorder) = execution_recorder.take() {
                let _ = recorder.finish().await;
            }
            if let Some(recorder) = warm_recorder.take() {
                let _ = recorder.finish().await;
            }
            return Err(error.context(
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
    }.await;
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
    // Actual lease reacquisition proves no model/session still retains its
    // service ownership capability. Work, assets and receipts remain private.
    let released = airk_store::service::ServiceLease::acquire(&control)?;
    drop(released);
    write(
        &work,
        "closed.json",
        &serde_json::json!({"schema":"airk.private-arbiter-fixture-closed/1","evaluation":true,"release_qualified":false,"control_root":control,"instance":instance,"shutdown_completed":true,"service_lease_reacquired":true}),
    )?;
    Ok(())
}

#[derive(Serialize)]
struct IndexBinding {
    source_root: PathBuf,
    index_id: airk_core::index_catalog::IndexId,
    definition: airk_core::index_catalog::IndexDefinition,
    source_roots: Vec<airk_core::source_policy::SourceRoot>,
    policy: Option<airk_core::source_policy::SourcePolicy>,
    publication: Option<airk_core::evidence::RevisionEnvelope>,
    window_scope_selection: airk_core::service::WindowScopeSelection,
    status_result: serde_json::Value,
}

fn readable_fixture_project(
    index: airk_core::index_catalog::IndexId,
    sources: &[airk_core::index_catalog::SourceRootId],
    projects: &[airk_core::index_catalog::QueryProject],
    memberships: &[airk_core::index_catalog::Membership],
) -> Result<airk_core::index_catalog::QueryProjectId> {
    use airk_core::index_catalog::PathScope;
    let expected: BTreeSet<_> = sources.iter().copied().collect();
    ensure!(
        !expected.is_empty() && expected.len() == sources.len(),
        "private readable source roster is empty or duplicated"
    );
    let mut project_ids = BTreeSet::new();
    for project in projects {
        project.validate()?;
        ensure!(
            project_ids.insert(project.id),
            "private readable project is ambiguous"
        );
    }
    let candidates: Vec<_> = memberships
        .iter()
        .filter(|member| member.index_id == index)
        .collect();
    ensure!(
        candidates.len() == 1,
        "private index requires exactly one actual readable membership"
    );
    let member = candidates[0];
    member.validate()?;
    let project = projects
        .iter()
        .find(|project| project.id == member.project_id)
        .context("private readable membership project is missing")?;
    ensure!(
        memberships
            .iter()
            .filter(|other| other.project_id == project.id)
            .count()
            == 1
            && project.default_indexes.iter().all(|other| *other == index),
        "private readable project merges other indexes"
    );
    ensure!(
        member.scope.len() == expected.len()
            && member
                .scope
                .iter()
                .all(|atom| atom.kind == PathScope::Subtree && atom.path.is_empty())
            && member
                .scope
                .iter()
                .map(|atom| atom.source_root_id)
                .collect::<BTreeSet<_>>()
                == expected,
        "private readable membership does not cover exactly the whole original source roots"
    );
    Ok(project.id)
}

fn fixture_catalog_rows(
    registry: &mut airk_store::service::registry::ServiceRegistry,
    namespace: airk_core::identity::Incarnation,
    revision: airk_core::index_catalog::CatalogRevision,
    kind: airk_core::catalog_management::CatalogKind,
) -> Result<Vec<airk_core::catalog_management::CatalogItem>> {
    use airk_core::catalog_management::{CatalogPageRequest, MAX_PAGE_BYTES};
    let mut request = CatalogPageRequest {
        kind,
        index_id: None,
        project_id: None,
        after: None,
        expected_catalog_revision: Some(revision),
        limit: airk_core::index_catalog::MAX_SELECTION,
        max_bytes: MAX_PAGE_BYTES,
    };
    let mut items = Vec::new();
    for _ in 0..64 {
        let page = registry.catalog_page(&request)?;
        ensure!(
            page.namespace == namespace && page.revision == revision,
            "private readable catalog revision changed"
        );
        items.extend(page.items);
        ensure!(
            items.len() <= 4096,
            "private readable catalog exceeds fixture bound"
        );
        let Some(next) = page.continuation else {
            return Ok(items);
        };
        ensure!(
            request.after.as_ref() != Some(&next),
            "private readable catalog continuation did not progress"
        );
        request.after = Some(next);
    }
    bail!("private readable catalog page bound exceeded")
}

#[test]
fn actual_readable_catalog_binds_one_whole_index_and_refuses_missing_foreign_or_ambiguous_members()
-> Result<()> {
    use airk_core::index_catalog::{IndexId, Membership, QueryProject, SourceRootId};
    let index: IndexId = "idx_97a0908b7435bc91b050849487544562".parse()?;
    let source: SourceRootId = "src_7b198478f66dc0daeb0c4ead235a95f1".parse()?;
    let projects: Vec<QueryProject> = serde_json::from_value(
        serde_json::json!([{"id":"prj_fb876a271dde1dffe98dc55fec6ad1b0","name":"goal-source-01","definition_revision":1,"scope_revision":2,"defaults_revision":1,"default_indexes":[]}]),
    )?;
    let members: Vec<Membership> = serde_json::from_value(
        serde_json::json!([{"project_id":"prj_fb876a271dde1dffe98dc55fec6ad1b0","index_id":"idx_97a0908b7435bc91b050849487544562","revision":1,"scope":[{"source_root_id":"src_7b198478f66dc0daeb0c4ead235a95f1","kind":"subtree","path":""}]}]),
    )?;
    assert_eq!(
        readable_fixture_project(index, &[source], &projects, &members)?,
        projects[0].id
    );
    assert!(readable_fixture_project(index, &[source], &[], &members).is_err());
    assert!(readable_fixture_project(index, &[source], &projects, &[]).is_err());
    let mut foreign = members.clone();
    foreign[0].project_id = "prj_9339def46ca2f665b9bc07115bdb54f2".parse()?;
    assert!(readable_fixture_project(index, &[source], &projects, &foreign).is_err());
    let mut foreign = members.clone();
    foreign[0].index_id = "idx_408453a0f41b66667eb5a0ba92f5a4db".parse()?;
    assert!(readable_fixture_project(index, &[source], &projects, &foreign).is_err());
    let mut foreign = members.clone();
    foreign[0].scope[0].source_root_id = "src_31432f7bc1107061b36db5d653c6855c".parse()?;
    assert!(readable_fixture_project(index, &[source], &projects, &foreign).is_err());
    let mut partial = members.clone();
    partial[0].scope[0].path = "subset".into();
    assert!(readable_fixture_project(index, &[source], &projects, &partial).is_err());
    let mut duplicate_projects = projects.clone();
    duplicate_projects.push(projects[0].clone());
    assert!(readable_fixture_project(index, &[source], &duplicate_projects, &members).is_err());
    let mut two_projects = projects.clone();
    let mut another_project = projects[0].clone();
    another_project.id = "prj_9339def46ca2f665b9bc07115bdb54f2".parse()?;
    another_project.name = "goal-source-03".into();
    two_projects.push(another_project.clone());
    let mut two_members = members.clone();
    let mut another_member = members[0].clone();
    another_member.project_id = another_project.id;
    two_members.push(another_member);
    assert!(
        readable_fixture_project(index, &[source], &two_projects, &two_members).is_err(),
        "two actual project identities cannot be chosen ambiguously"
    );
    let mut ambiguous = members.clone();
    ambiguous.push(members[0].clone());
    assert!(readable_fixture_project(index, &[source], &projects, &ambiguous).is_err());
    let mut merged = members.clone();
    let mut other = members[0].clone();
    other.index_id = "idx_408453a0f41b66667eb5a0ba92f5a4db".parse()?;
    merged.push(other);
    assert!(readable_fixture_project(index, &[source], &projects, &merged).is_err());
    Ok(())
}

fn bound_publication(
    body: &serde_json::Value,
    index: airk_core::index_catalog::IndexId,
    source_revision: airk_core::index_catalog::CatalogRevision,
    source_roots: &[airk_core::index_catalog::SourceRootId],
    collection: &str,
) -> Result<airk_core::evidence::RevisionEnvelope> {
    let scope: airk_core::scoped_reads::ScopeObservation = serde_json::from_value(
        body.get("scope_observation")
            .context("canonical status scope observation missing")?
            .clone(),
    )?;
    scope.validate()?;
    ensure!(
        scope.members.len() == 1,
        "canonical status must observe exactly one immutable index"
    );
    let member = &scope.members[0];
    ensure!(
        member.index_id == index
            && member.source_revision == source_revision
            && member
                .source_root_ids
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                == source_roots.iter().copied().collect::<BTreeSet<_>>(),
        "canonical status immutable index/source identity differs"
    );
    if let Some(actual_collection) = body.get("collection") {
        ensure!(
            actual_collection.as_str() == Some(collection),
            "canonical status top-level collection differs"
        );
    }
    let publication = if let Some(value) = body.get("publication").filter(|value| !value.is_null())
    {
        let fields = value
            .as_object()
            .context("canonical status publication must be an object")?;
        if fields.contains_key("collection") {
            // Full envelopes retain strict Core field and identity validation.
            serde_json::from_value::<airk_core::evidence::RevisionEnvelope>(value.clone())?
        } else {
            ensure!(
                fields.len() == 1 && fields.contains_key("token"),
                "unknown or partial canonical status publication shape"
            );
            let actual_collection = body
                .get("collection")
                .and_then(serde_json::Value::as_str)
                .context("canonical status collection missing")?;
            ensure!(
                actual_collection == collection,
                "canonical status collection differs"
            );
            let token: airk_core::identity::RevisionToken =
                serde_json::from_value(fields["token"].clone())?;
            airk_core::evidence::RevisionEnvelope::new(actual_collection.to_owned(), token)
        }
    } else {
        let vector: airk_core::combined_reads::PublicationVector = serde_json::from_value(
            body.get("publication_vector")
                .context("canonical status publication vector missing")?
                .clone(),
        )?;
        vector.validate()?;
        ensure!(
            vector.members.len() == 1 && vector.members[0].index_id == index,
            "canonical status names another immutable index"
        );
        vector.members[0]
            .publication
            .clone()
            .context("canonical status has no published index")?
    };
    ensure!(
        publication.collection == collection
            && publication.incarnation == publication.token.incarnation
            && publication.generation == publication.token.generation
            && publication.revision == publication.token.revision,
        "canonical status publication identity differs"
    );
    Ok(publication)
}
#[test]
fn original_publication_refuses_same_collection_foreign_index_or_missing_scope() -> Result<()> {
    use airk_core::{
        identity::{Incarnation, RevisionToken},
        index_catalog::{CatalogRevision, IndexId, SourceRootId},
    };
    let index = IndexId::from_identity(Incarnation::from_bytes([1; 16]));
    let foreign = IndexId::from_identity(Incarnation::from_bytes([2; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([3; 16]));
    let publication = airk_core::evidence::RevisionEnvelope::new(
        "same-label".into(),
        RevisionToken {
            incarnation: Incarnation::from_bytes([4; 16]),
            generation: 7,
            revision: Sha256Digest::from_bytes([5; 32]),
        },
    );
    let body = serde_json::json!({"publication":publication,"scope_observation":{"schema":1,"qualifier":"selected_subset","fingerprint":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","selection":{"owner":"agent","scope_identity":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","catalog_revision":1},"members":[{"index_id":index,"memberships":[],"source_root_ids":[source],"source_revision":1,"policy_epoch":0,"policy_digest":null}]}});
    assert_eq!(
        bound_publication(
            &body,
            index,
            CatalogRevision::INITIAL,
            &[source],
            "same-label"
        )?,
        publication
    );
    let mut missing = body.clone();
    missing.as_object_mut().unwrap().remove("scope_observation");
    assert!(
        bound_publication(
            &missing,
            index,
            CatalogRevision::INITIAL,
            &[source],
            "same-label"
        )
        .is_err()
    );
    let mut wrong = body.clone();
    wrong["scope_observation"]["members"][0]["index_id"] = serde_json::to_value(foreign)?;
    assert!(
        bound_publication(
            &wrong,
            index,
            CatalogRevision::INITIAL,
            &[source],
            "same-label"
        )
        .is_err(),
        "same collection label does not identify the original index"
    );
    let mut wrong = body.clone();
    wrong["scope_observation"]["members"][0]["source_root_ids"] =
        serde_json::json!([SourceRootId::from_identity(Incarnation::from_bytes(
            [6; 16]
        ))]);
    assert!(
        bound_publication(
            &wrong,
            index,
            CatalogRevision::INITIAL,
            &[source],
            "same-label"
        )
        .is_err()
    );
    let mut wrong = body.clone();
    wrong["scope_observation"]["members"][0]["source_revision"] = serde_json::json!(2);
    assert!(
        bound_publication(
            &wrong,
            index,
            CatalogRevision::INITIAL,
            &[source],
            "same-label"
        )
        .is_err()
    );
    let mut vector = body.clone();
    vector.as_object_mut().unwrap().remove("publication");
    vector["publication_vector"] = serde_json::json!({"authority":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","observed_from_unix_ms":1,"observed_to_unix_ms":2,"members":[{"index_id":index,"publication":publication}]});
    assert_eq!(
        bound_publication(
            &vector,
            index,
            CatalogRevision::INITIAL,
            &[source],
            "same-label"
        )?,
        publication
    );
    vector["publication_vector"]["members"][0]["index_id"] = serde_json::to_value(foreign)?;
    assert!(
        bound_publication(
            &vector,
            index,
            CatalogRevision::INITIAL,
            &[source],
            "same-label"
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn actual_status_token_projection_binds_top_level_collection_and_refuses_unknown_fields()
-> Result<()> {
    use airk_core::{
        identity::{Incarnation, RevisionToken},
        index_catalog::{CatalogRevision, IndexId, SourceRootId},
    };
    let index = IndexId::from_identity(Incarnation::from_bytes([1; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([3; 16]));
    // Actual canonical Status projects publication as token-only while the
    // collection remains top-level. This is not a full RevisionEnvelope.
    let token:RevisionToken="airk-r1:04040404040404040404040404040404:7:0505050505050505050505050505050505050505050505050505050505050505".parse()?;
    let fixture = serde_json::json!({"collection":"actual-collection","publication":{"token":token},"scope_observation":{"schema":1,"qualifier":"selected_subset","fingerprint":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","selection":{"owner":"agent","scope_identity":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","catalog_revision":1},"members":[{"index_id":index,"memberships":[],"source_root_ids":[source],"source_revision":1,"policy_epoch":0,"policy_digest":null}]}});
    let expected = airk_core::evidence::RevisionEnvelope::new("actual-collection".into(), token);
    assert_eq!(
        bound_publication(
            &fixture,
            index,
            CatalogRevision::INITIAL,
            &[source],
            "actual-collection"
        )?,
        expected
    );
    let mut foreign = fixture.clone();
    foreign["collection"] = "foreign-collection".into();
    assert!(
        bound_publication(
            &foreign,
            index,
            CatalogRevision::INITIAL,
            &[source],
            "actual-collection"
        )
        .is_err()
    );
    let mut missing = fixture.clone();
    missing.as_object_mut().unwrap().remove("collection");
    assert!(
        bound_publication(
            &missing,
            index,
            CatalogRevision::INITIAL,
            &[source],
            "actual-collection"
        )
        .is_err()
    );
    let mut unknown = fixture.clone();
    unknown["publication"]["unexpected_authority"] = true.into();
    assert!(
        bound_publication(
            &unknown,
            index,
            CatalogRevision::INITIAL,
            &[source],
            "actual-collection"
        )
        .is_err()
    );
    let mut invalid = fixture.clone();
    invalid["publication"]["token"] = "generation-only:7".into();
    assert!(
        bound_publication(
            &invalid,
            index,
            CatalogRevision::INITIAL,
            &[source],
            "actual-collection"
        )
        .is_err()
    );
    let mut partial = fixture;
    partial["publication"]["collection"] = "actual-collection".into();
    assert!(
        bound_publication(
            &partial,
            index,
            CatalogRevision::INITIAL,
            &[source],
            "actual-collection"
        )
        .is_err()
    );
    Ok(())
}

/// Observe canonical setup results and emit a Core-owned request instance.
/// This never creates indexes, mutates configuration or claims native warm.
#[tokio::test]
#[ignore = "private actual evaluation control instance emitter; explicit running fixture and scope pins required"]
async fn emit_private_arbiter_configuration() -> Result<()> {
    use airk_core::arbiter::{Enrollment, ManagementRequest, Task};
    let work = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_EVALUATION_WORK")
            .context("AIRK_ARBITER_EVALUATION_WORK missing")?,
    );
    let work = work.canonicalize()?;
    ensure!(
        work.starts_with(artifact_root()?),
        "private controls leave artifacts"
    );
    let ready_bytes = crate::ipc::private_record::read_private(&work, "ready.json", 8 * LIMIT)?
        .context("actual fixture Ready missing")?;
    let ready: Ready = serde_json::from_slice(&ready_bytes)?;
    ensure!(
        ready.schema == "airk.private-arbiter-fixture/1"
            && ready.evaluation
            && !ready.release_qualified
            && restart::matches_control(&work, &ready)?,
        "actual private fixture binding differs"
    );
    let current = crate::ipc::control::request_once(
        &ready.control_root,
        ControlMethod::Health,
        env!("CARGO_PKG_VERSION"),
        Some(ready.health.instance),
    )
    .await?;
    let ControlResponse::Health { observation } = current else {
        bail!("actual fixture Health refused")
    };
    ensure!(
        observation.instance == ready.health.instance
            && observation.process_id == ready.health.process_id
            && observation.executable_sha256 == ready.health.executable_sha256
            && observation.executable_sha256
                == airk_host::runtime_pair::current_executable_sha256()?,
        "actual private fixture service identity changed"
    );
    let scope_input = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_EVALUATION_SCOPES")
            .context("private frozen scope input missing")?,
    );
    let expected: Sha256Digest = std::env::var("AIRK_ARBITER_EVALUATION_SCOPES_SHA256")?.parse()?;
    let (_parent, mut input) = source_file(&scope_input, &artifact_root()?)?;
    let raw = bounded(&mut input)?;
    ensure!(
        sha(&raw) == expected,
        "private frozen scope input hash differs"
    );
    let value: serde_json::Value = serde_json::from_slice(&raw)?;
    let roots: Vec<PathBuf> = serde_json::from_value(
        value
            .get("source_roots")
            .context("frozen source root roster missing")?
            .clone(),
    )?;
    ensure!(
        !roots.is_empty() && roots.len() <= 64,
        "private frozen source roster bound"
    );
    let mut expected_roots = BTreeMap::new();
    for root in roots {
        let canonical = root.canonicalize()?;
        ensure!(
            canonical.starts_with(artifact_root()?)
                && expected_roots.insert(canonical, root).is_none(),
            "duplicate or foreign private source root"
        );
    }
    let mut registry =
        airk_store::service::registry::ServiceRegistry::open_saved(&ready.control_root)?;
    let catalog = registry
        .catalog_state()?
        .context("actual private catalog missing")?;
    ensure!(
        catalog.activated
            && observation
                .catalog_clock
                .is_some_and(|clock| clock.namespace == catalog.namespace),
        "actual private catalog changed"
    );
    let projects = fixture_catalog_rows(
        &mut registry,
        catalog.namespace,
        catalog.revision,
        airk_core::catalog_management::CatalogKind::Projects,
    )?
    .into_iter()
    .map(|item| match item {
        airk_core::catalog_management::CatalogItem::Project(project) => Ok(project),
        _ => bail!("private project page contains a foreign entity"),
    })
    .collect::<Result<Vec<_>>>()?;
    let memberships = fixture_catalog_rows(
        &mut registry,
        catalog.namespace,
        catalog.revision,
        airk_core::catalog_management::CatalogKind::Memberships,
    )?
    .into_iter()
    .map(|item| match item {
        airk_core::catalog_management::CatalogItem::Membership(member) => Ok(member),
        _ => bail!("private membership page contains a foreign entity"),
    })
    .collect::<Result<Vec<_>>>()?;
    let reviewed_selections = if let Some(raw) = value.get("reviewed_selections") {
        let selections: Vec<airk_core::service::ScopedConnectRequest> =
            serde_json::from_value(raw.clone())?;
        ensure!(
            selections.len() == expected_roots.len(),
            "explicit private selection/source roster differs"
        );
        let mut selected = BTreeMap::new();
        for request in selections {
            request.validate()?;
            ensure!(
                selected.insert(request.index_id, request).is_none(),
                "duplicate explicit private index selection"
            );
        }
        Some(selected)
    } else {
        None
    };
    let mut reviewed_authorities = Vec::new();
    let census = registry.catalog_clean_break_sources(catalog.revision)?;
    let mut bindings = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for (definition, roots, policy) in census {
        if roots.len() != 1 {
            continue;
        }
        let canonical = Path::new(&roots[0].canonical_path).canonicalize()?;
        if let Some(original) = expected_roots.get(&canonical) {
            ensure!(
                Path::new(&definition.backing.root).canonicalize()? == canonical,
                "private index has a foreign backing root"
            );
            let readable_project = if let Some(selections) = &reviewed_selections {
                let request = selections
                    .get(&definition.id)
                    .context("explicit private selection is missing this immutable index")?;
                let (project, authority) = readable_selection::qualify(
                    &mut registry,
                    request,
                    &catalog,
                    &definition,
                    &roots,
                    &projects,
                    &memberships,
                )?;
                reviewed_authorities
                    .push(serde_json::json!({"selection":request,"authority":authority}));
                project
            } else {
                readable_fixture_project(
                    definition.id,
                    &roots.iter().map(|root| root.id).collect::<Vec<_>>(),
                    &projects,
                    &memberships,
                )?
            };
            ensure!(
                bindings
                    .insert(
                        canonical,
                        IndexBinding {
                            source_root: original.clone(),
                            index_id: definition.id,
                            window_scope_selection: airk_core::service::WindowScopeSelection {
                                namespace: catalog.namespace,
                                expected_catalog_revision: catalog.revision,
                                index_ids: vec![definition.id],
                                project_ids: vec![readable_project],
                            },
                            definition: definition.clone(),
                            source_roots: roots,
                            policy,
                            publication: None,
                            status_result: serde_json::Value::Null,
                        }
                    )
                    .is_none()
                    && ids.insert(definition.id),
                "private source is ambiguous or shares an immutable index"
            );
        }
    }
    ensure!(
        bindings.len() == expected_roots.len(),
        "canonical setup has not completed exactly one index for every frozen source root"
    );
    let prefix = std::env::var("AIRK_ARBITER_EVALUATION_CONTROL_PREFIX")?;
    ensure!(
        !prefix.is_empty()
            && prefix.len() <= 64
            && prefix
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_')),
        "private control output prefix invalid"
    );
    if reviewed_selections.is_some() {
        write(
            &work,
            &format!("{prefix}.reviewed-authorities.private.json"),
            &reviewed_authorities,
        )?;
    }
    for (ordinal, binding) in bindings.values_mut().enumerate() {
        let result = crate::ipc::control::request_once(
            &ready.control_root,
            ControlMethod::ToolsCall {
                read_attempt: None,
                tool: "status".into(),
                arguments: serde_json::Map::new(),
                selection: airk_core::service::ToolSelection::Open {
                    index_ids: vec![binding.index_id.to_string()],
                    index_set_ids: vec![],
                    connection_id: None,
                    workspace: None,
                },
                response_mode: airk_core::service::ToolResponseMode::Full,
                usage_client: airk_core::usage::ClientFamily::Cli,
            },
            env!("CARGO_PKG_VERSION"),
            Some(ready.health.instance),
        )
        .await?;
        write(&work, &format!("{prefix}.status-{ordinal}.json"), &result)?;
        let ControlResponse::ToolResult { result } = result else {
            bail!("canonical status did not return a complete tool envelope")
        };
        ensure!(
            result.get("isError").and_then(serde_json::Value::as_bool) != Some(true),
            "canonical status refused source publication"
        );
        let body = if let Some(value) = result.get("structuredContent") {
            value.clone()
        } else {
            let content = result
                .get("content")
                .and_then(serde_json::Value::as_array)
                .context("canonical status content missing")?;
            ensure!(
                content.len() == 1
                    && content[0].get("type").and_then(serde_json::Value::as_str) == Some("text"),
                "canonical status content differs"
            );
            serde_json::from_str(
                content[0]
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .context("canonical status text missing")?,
            )?
        };
        let publication = bound_publication(
            &body,
            binding.index_id,
            binding.definition.source_revision,
            &binding
                .source_roots
                .iter()
                .map(|root| root.id)
                .collect::<Vec<_>>(),
            &binding.definition.backing.collection,
        )?;
        binding.publication = Some(publication);
        binding.status_result = result;
    }
    ensure!(
        registry
            .catalog_state()?
            .is_some_and(|current| current.namespace == catalog.namespace
                && current.revision == catalog.revision),
        "private catalog changed during publication observation"
    );
    let selected: Sha256Digest = std::env::var("AIRK_ARBITER_EVALUATION_PROFILE")?.parse()?;
    let stage = match std::env::var("AIRK_ARBITER_EVALUATION_STAGE")?.as_str() {
        "focus" => airk_core::arbiter::Stage::Focus,
        "context" => airk_core::arbiter::Stage::Context,
        _ => bail!("private stage must be explicit focus/context"),
    };
    let (profile_index, binding) = ready
        .profiles
        .iter()
        .enumerate()
        .find(|(_, profile)| profile.manifest == selected)
        .context("selected profile is outside actual private fixture")?;
    ensure!(
        binding.snapshot == format!("profile-{profile_index}.json"),
        "private profile snapshot name changed"
    );
    let profile_bytes = crate::ipc::private_record::read_private(&work, &binding.snapshot, LIMIT)?
        .context("private profile snapshot missing")?;
    ensure!(
        sha(&profile_bytes) == binding.profile_sha256,
        "private profile snapshot changed"
    );
    let profile: Profile = serde_json::from_slice(&profile_bytes)?;
    ensure!(
        profile.qualification.is_none()
            && profile.manifest == selected
            && profile.model.stage() == Some(stage),
        "private profile identity changed"
    );
    let enabled = match std::env::var("AIRK_ARBITER_EVALUATION_ENABLED")?.as_str() {
        "true" => true,
        "false" => false,
        _ => bail!("private enabled state must be explicit true/false"),
    };
    let state = crate::ipc::control::request_once(
        &ready.control_root,
        ControlMethod::Arbiter {
            request: ManagementRequest::Status,
        },
        env!("CARGO_PKG_VERSION"),
        Some(ready.health.instance),
    )
    .await?;
    let ControlResponse::Arbiter {
        observation: status,
    } = state
    else {
        bail!("actual private Arbiter status refused")
    };
    ensure!(
        status.configuration.catalog == catalog.namespace,
        "actual private configuration namespace changed"
    );
    let request = ManagementRequest::Configure {
        expected_revision: status.configuration.revision,
        stage,
        enabled,
        selected_profile: Some(selected),
        indexes: bindings
            .values()
            .map(|binding| Enrollment {
                index_id: binding.index_id,
                tasks: profile.tasks.clone(),
            })
            .collect(),
    };

    if std::env::var("AIRK_SELECTED_MEMBERSHIP_REVOKE")
        .ok()
        .as_deref()
        == Some("1")
    {
        ensure!(
            bindings.len() == 1,
            "membership revocation fixture requires one actual bound index"
        );
        let binding = bindings
            .values()
            .next()
            .context("single fault binding absent")?;
        ensure!(
            binding.window_scope_selection.project_ids.len() == 1
                && binding.window_scope_selection.index_ids == vec![binding.index_id],
            "fault W4 selection must be exactly one project/index"
        );
        let project_id = binding.window_scope_selection.project_ids[0];
        let project = projects
            .iter()
            .find(|project| project.id == project_id)
            .context("actual fault project absent")?;
        let members = memberships
            .iter()
            .filter(|member| member.project_id == project_id)
            .collect::<Vec<_>>();
        ensure!(
            members.len() == 1 && members[0].index_id == binding.index_id,
            "fault project must have exactly its actual single membership"
        );
        let member = members[0];
        let operation = airk_core::catalog_management::CatalogOperation::RemoveMembership {
            project_id,
            index_id: binding.index_id,
            expected_scope_revision: project.scope_revision,
            expected_membership_revision: member.revision,
        };
        ensure!(
            operation.requires_authority_fence() && !operation.requires_worker_drain(),
            "membership fixture cannot wait for worker NN drain"
        );
        write(&work, "fault-membership-remove.operation.json", &operation)?;
        write(
            &work,
            "fault-membership-before.private.json",
            &serde_json::json!({
                "schema":"airk.private-composed-membership-before/1","evaluation":true,"release_qualified":false,
                "instance":ready.health.instance,"namespace":catalog.namespace,"catalog_revision":catalog.revision,
                "project":project,"membership":member,"index":binding.definition,
                "window_scope_selection":binding.window_scope_selection,"operation":operation,
            }),
        )?;
    }
    write(&work, &format!("{prefix}.configure.json"), &request)?;
    write(
        &work,
        &format!("{prefix}.bindings.json"),
        &serde_json::json!({"schema":"airk.private-arbiter-index-bindings/1","evaluation":true,"release_qualified":false,"control_root":ready.control_root,"instance":ready.health.instance,"namespace":catalog.namespace,"catalog_revision":catalog.revision,"scopes_sha256":expected,"bindings":bindings.into_values().collect::<Vec<_>>() }),
    )?;
    write(
        &work,
        &format!("{prefix}.receipt.json"),
        &serde_json::json!({"schema":"airk.private-arbiter-configure-instance/1","evaluation":true,"release_qualified":false,"ready_sha256":sha(&ready_bytes),"profile":selected,"profile_sha256":binding.profile_sha256,"expected_revision":status.configuration.revision,"stage":stage,"task_policy_current":match stage {
            airk_core::arbiter::Stage::Context => profile.tasks.contains(&Task::ContextAction)
                && profile.task_policies.get(&Task::ContextAction)==Some(&airk_retrieval::arbiter::context_action::policy_digest()?),
            airk_core::arbiter::Stage::Focus => profile.tasks.contains(&Task::FocusRank)
                && profile.task_policies.get(&Task::FocusRank)==Some(&airk_retrieval::arbiter::compiler::focus::policy_digest()?),
        },"request":request,"actual_status":status,"actual_health":observation}),
    )?;
    Ok(())
}

/// Only the ignored actual private fixture can attach this observer capability.
fn selected_cal_observer(
    prepared: Arc<Vec<Prepared>>,
    work: &Path,
) -> Result<arbiter::evaluation::ContextObservationSink> {
    use airk_core::arbiter::{
        AssetRole, NativeRead,
        focus::{FocusObservation, search_record_observation},
    };
    use airk_retrieval::arbiter::{compiler::Compiler, context_action};
    use std::io::{Seek, SeekFrom};
    let expected_pipeline: Sha256Digest =
        std::env::var("AIRK_SELECTED_CONTEXT_MINILM_PIPELINE")?.parse()?;
    let focus_profile = prepared
        .iter()
        .find(|entry| entry.profile.model == ModelChoice::FocusQwenReranker06B)
        .context("selected Focus profile missing")?
        .profile
        .manifest;
    let mut compilers = Vec::new();
    for entry in prepared
        .iter()
        .filter(|entry| entry.profile.model.stage() == Some(airk_core::arbiter::Stage::Context))
    {
        let read = |role| -> Result<Vec<u8>> {
            let asset = entry
                .profile
                .assets
                .iter()
                .find(|asset| asset.role == role)
                .context("compiler census asset missing")?;
            ensure!(asset.bytes <= 64 << 20, "compiler census byte limit");
            let mut file = entry
                .sources
                .get(&asset.name)
                .context("retained compiler census descriptor missing")?
                .try_clone()?;
            file.seek(SeekFrom::Start(0))?;
            let mut raw = Vec::new();
            file.take(asset.bytes + 1).read_to_end(&mut raw)?;
            ensure!(
                raw.len() as u64 == asset.bytes && sha(&raw) == asset.sha256,
                "compiler census asset drift"
            );
            Ok(raw)
        };
        compilers.push((
            entry.profile.model,
            entry.profile.manifest,
            Compiler::from_verified(&read(AssetRole::Compiler)?, &read(AssetRole::Tokenizer)?)?,
        ));
    }
    ensure!(
        compilers.len() == 2,
        "two Context compiler censuses required"
    );
    let directory = work.join("context-cal-admissions");
    std::fs::create_dir(&directory)?;
    let work = directory;
    let state = Arc::new(std::sync::Mutex::new((BTreeSet::new(), compilers, 0usize)));
    Ok(Arc::new(move |request, profile| {
        let mut state = state
            .lock()
            .map_err(|_| anyhow::anyhow!("private admission recorder poisoned"))?;
        let key = (
            request.read_attempt.request_id,
            request.scope.index_id,
            request.ordinal,
        );
        ensure!(
            state.0.insert(key) && state.2 < 100,
            "duplicate/excess selected CAL observation"
        );
        ensure!(
            profile.model.stage() == Some(airk_core::arbiter::Stage::Context),
            "foreign captured stage"
        );
        let prompt = context_action::prompt(request)?;
        let mut focus = Vec::new();
        let mut known_focus_tokens = 0usize;
        let mut actual_focus_forward = false;
        for record in request
            .records
            .iter()
            .filter(|record| record.operation == NativeRead::Search)
        {
            let search: airk_core::SearchResponse =
                serde_json::from_value(record.canonical_result.clone())?;
            ensure!(
                search
                    .execution
                    .as_ref()
                    .is_some_and(|execution| execution.served_mode
                        == airk_core::search_execution::ServedMode::Hybrid
                        && !execution.incomplete)
                    && search
                        .semantic_readiness
                        .as_ref()
                        .is_some_and(|semantic| semantic.state
                            == airk_core::readiness::SemanticState::Ready
                            && semantic.pipeline_digest == Some(expected_pipeline)),
                "selected MiniLM actual hybrid/pipeline identity differs"
            );
            let observation = search_record_observation(record)?
                .context("FocusON actual Search observation omitted")?;
            if let FocusObservation::Receipt { receipt } = &observation {
                if let Some(identity) = &receipt.identity {
                    ensure!(
                        identity.profile == focus_profile,
                        "selected Focus actual profile differs"
                    );
                }
                ensure!(
                    receipt.observed_input_tokens.is_some(),
                    "unknown Focus work cannot enter selected Context CAL"
                );
                known_focus_tokens = known_focus_tokens
                    .checked_add(receipt.work.input_tokens)
                    .context("Focus census overflow")?;
                actual_focus_forward |= receipt.applied && receipt.work.input_tokens > 0;
            }
            focus.push(observation);
        }
        ensure!(
            !focus.is_empty()
                && known_focus_tokens + request.remaining_input_tokens
                    <= request.parent_budget.input_tokens,
            "combined original Focus/Context input allowance differs"
        );
        let mut census = Vec::new();
        for (model, manifest, compiler) in &state.1 {
            let started = std::time::Instant::now();
            let compiled = compiler.compile(&prompt.state, &prompt.question, &prompt.candidates);
            census.push(match compiled {
                Ok(compiled)=>serde_json::json!({"model":model,"profile":manifest,"compile_us":started.elapsed().as_micros(),
                    "fits_remaining_allowance":compiled.input_ids.len()<=request.remaining_input_tokens,
                    "input_tokens":compiled.input_ids.len(),"compiled":compiled,"NN_invocations":0}),
                Err(error)=>serde_json::json!({"model":model,"profile":manifest,"compile_us":started.elapsed().as_micros(),
                    "fits_remaining_allowance":false,"compiler_refusal":format!("{error:#}"),"NN_invocations":0}),
            });
        }
        state.2 += 1;
        let remaining_ns = request
            .read_attempt
            .remaining_at(
                request.read_attempt.clock,
                airk_process::monotonic::now_ns()?,
            )?
            .as_nanos();
        write(
            &work,
            &format!("record-{:04}.json", state.2),
            &serde_json::json!({
            "schema":"airk.private-selected-Context-native-admission-observation/1","ordinal_in_private_run":state.2,
            "request":request,"planned_input_digest":airk_core::identity::framed_digest(b"airk.arbiter.planned-task-input/1", &[&serde_json::to_vec(&prompt)?]),"actual_Context_profile":profile,"Context_NN_dispatch":false,
            "actual_Focus_profile":focus_profile,"MiniLM_pipeline":expected_pipeline,"Focus_configuration":"ON",
            "actual_Focus_forward_observed":actual_focus_forward,"Focus_observations":focus,"known_Focus_input_tokens":known_focus_tokens,
            "remaining_ns_at_observation":remaining_ns,"compiler_census":census,"verified_probe_requests":selected_verified_probe_requests(request)?,
            "custody":"Prechecked private snapshot; canonical caller must also complete original fixed-issue, source/configuration/profile/deadline postchecks. Recorder I/O is outside native gate; retained bytes alone are not a qualified or delivered result.",
            "qualification":"Actual ready service admission plus separate pure compiler census only. No Context score/action or positive CAL qualification. Known Focus fallbacks retained."}),
        )?;
        Ok(())
    }))
}

fn selected_verified_probe_requests(
    request: &airk_core::native_context_action::NativeContextActionRequest,
) -> Result<Vec<serde_json::Value>> {
    use airk_core::arbiter::{NativeRead, literal::SourceLiteral};
    let max_bytes = (request.parent_budget.max_bytes / 8).clamp(1024, 4096);
    let mut probes = Vec::new();
    for choice in request.choices.iter().filter(|choice| {
        request
            .roster
            .basis
            .read_choices_scored
            .contains(&choice.id)
    }) {
        let (tool, args) = match choice.operation {
            NativeRead::Search => {
                let literal = SourceLiteral::parse(&choice.target)?
                    .context("offered search lacks verified source literal")?;
                let record = request
                    .records
                    .iter()
                    .find(|record| record.id == literal.record)
                    .context("offered literal source missing")?;
                let valid = record.canonical_result["hits"]
                    .as_array()
                    .context("literal source hits missing")?
                    .iter()
                    .map(|value| serde_json::from_value::<airk_core::Hit>(value.clone()))
                    .collect::<std::result::Result<Vec<_>, _>>()?
                    .iter()
                    .any(|hit| literal.matches(record.id, hit));
                ensure!(
                    valid,
                    "offered literal no longer matches actual returned source bytes"
                );
                let args = serde_json::json!({"query":literal.query,"limit":4,"max_bytes":max_bytes,"full_chunks":true,"response_mode":"full","include_status":true});
                let _: airk_mcp::SearchArgs = serde_json::from_value(args.clone())?;
                ("search", args)
            }
            NativeRead::Context => {
                let (path, range) = choice
                    .target
                    .rsplit_once(':')
                    .context("offered context target missing")?;
                let (line, _) = range
                    .split_once('-')
                    .context("offered context line missing")?;
                let line = line.parse::<usize>()?;
                let mut grounded = false;
                for record in &request.records {
                    if let Some(hits) = record.canonical_result["hits"].as_array() {
                        for value in hits {
                            let hit: airk_core::Hit = serde_json::from_value(value.clone())?;
                            grounded |= hit.citation.path == path
                                && line >= hit.citation.start_line
                                && line <= hit.citation.end_line;
                        }
                    }
                }
                ensure!(
                    grounded,
                    "offered context target not grounded in actual citation"
                );
                let args = serde_json::json!({"path":path,"line":line,"before":0,"after":32,"max_bytes":max_bytes,"revision":request.scope.publication.token.to_string(),"response_mode":"full","include_status":true});
                let _: airk_mcp::ContextArgs = serde_json::from_value(args.clone())?;
                ("context", args)
            }
            _ => bail!("unsupported offered probe operation"),
        };
        probes.push(serde_json::json!({"choice":choice,"tool":tool,"arguments":args,
            "publication":request.scope.publication,"scope":"Independent canonical source-reference counterfactual only; not an actuated learned trajectory or a fresh allowance in the captured user request."}));
    }
    ensure!(probes.len() <= 5, "offered whole-card probe bound");
    Ok(probes)
}

/// Serialize actual Core semantic setup drafts and inspect the CPU-selected
/// MiniLM object plan without opening an inference session or any task data.
#[test]
#[ignore = "private selected semantic setup draft emitter; explicit immutable MiniLM assets and fresh output required"]
fn emit_private_selected_vector_configuration() -> Result<()> {
    use airk_core::service_configuration::ConfigurationDraft;
    let directory = PathBuf::from(
        std::env::var_os("AIRK_SELECTED_MINILM_DIRECTORY")
            .context("selected MiniLM directory missing")?,
    )
    .canonicalize()?;
    let (_parent, mut manifest_file) =
        source_file(&directory.join("model.json"), &artifact_root()?)?;
    let manifest_raw = bounded(&mut manifest_file)?;
    ensure!(
        serde_json::from_slice::<serde_json::Value>(&manifest_raw)?["schema"] == 2,
        "selected MiniLM must use the actual current ONNX model definition"
    );
    let plan = airk_retrieval::model::inspect_object_plan(&directory)?;
    let catalog = airk_retrieval::model_catalog::inspect_catalog()?;
    let output = PathBuf::from(
        std::env::var_os("AIRK_SELECTED_VECTOR_DRAFT_OUTPUT")
            .context("selected semantic draft output missing")?,
    );
    ensure!(
        output.is_absolute() && !output.try_exists()?,
        "semantic draft output must be fresh/absolute"
    );
    let parent = output
        .parent()
        .context("semantic draft output parent missing")?
        .canonicalize()?;
    ensure!(
        parent.starts_with(artifact_root()?),
        "semantic draft output leaves private artifacts"
    );
    let pin = OpenedDirectory::open(&parent)?;
    let _output_pin = crate::ipc::endpoint::create_root_at(
        &pin,
        output
            .file_name()
            .context("semantic draft output name missing")?,
    )?;
    let selection = airk_core::vector_selection::VectorSelectionPolicy::default();
    selection.validate()?;
    write(
        &output,
        "vectors-enabled.json",
        &ConfigurationDraft::Vectors { enabled: true },
    )?;
    write(
        &output,
        "minilm-install.json",
        &ConfigurationDraft::InstallModel {
            directory: directory.clone(),
        },
    )?;
    write(
        &output,
        "vector-selection-all.json",
        &ConfigurationDraft::VectorSelection {
            policy: selection.clone(),
        },
    )?;
    write(
        &output,
        "minilm-plan.json",
        &serde_json::json!({
            "schema":"airk.private-selected-native-semantic-setup/1","evaluation":true,"release_qualified":false,
            "source_directory":directory,"model_definition_sha256":sha(&manifest_raw),
            "object_plan":plan,"actual_native_catalog":catalog,"vector_selection_digest":selection.digest()?,
            "NN_invocations":0,"scope":"Only actual Core configuration draft serialization and CPU-selected immutable MiniLM object inspection. No service/index/source/task/model execution or semantic readiness claim."
        }),
    )?;
    println!(
        "{}",
        serde_json::json!({"output":output,"NN_invocations":0,"task_data_opened":false})
    );
    Ok(())
}

/// Fresh synthetic-only source and owning request cases for composed native
/// acceptance. Never reads or mutates the frozen CAL/confirmation artifacts.
#[test]
#[ignore = "private synthetic composed source/request emitter; fresh protected artifacts output required"]
fn emit_private_selected_synthetic_fixture() -> Result<()> {
    emit_selected_synthetic_fixture(answer_template()?)
}

#[test]
#[ignore = "private current8192/120 synthetic composed source/request emitter; fresh protected output required"]
fn emit_private_selected_synthetic_fixture_software_v2() -> Result<()> {
    emit_selected_synthetic_fixture(software_v2_answer_template()?)
}

fn emit_selected_synthetic_fixture(mut answer: serde_json::Value) -> Result<()> {
    use std::io::Write;
    let output = PathBuf::from(
        std::env::var_os("AIRK_SELECTED_SYNTHETIC_OUTPUT")
            .context("synthetic fixture output missing")?,
    );
    ensure!(
        output.is_absolute() && !output.try_exists()?,
        "synthetic output must be fresh/absolute"
    );
    let parent = output
        .parent()
        .context("synthetic output parent missing")?
        .canonicalize()?;
    ensure!(
        parent.starts_with(artifact_root()?),
        "synthetic fixture leaves private artifacts"
    );
    let pin = OpenedDirectory::open(&parent)?;
    let output_pin = crate::ipc::endpoint::create_root_at(
        &pin,
        output
            .file_name()
            .context("synthetic output name missing")?,
    )?;
    let source = output.join("sources");
    let _source_pin = crate::ipc::endpoint::create_root_at(&output_pin, OsStr::new("sources"))?;
    let question = "Where is the SyntheticRelay lantern maintenance schedule documented?";
    let mut files = Vec::new();
    let mut first_hash = None;
    for ordinal in 0..8 {
        let name = format!("doc-{ordinal:02}.txt");
        let body = format!(
            "Synthetic document {ordinal}\nSyntheticRelay maintenance follows the lantern schedule every Friday.\nThis synthetic source is for native isolation and lifecycle checks.\n"
        );
        let raw = body.as_bytes();
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(source.join(&name))?;
        file.write_all(raw)?;
        file.sync_all()?;
        let digest = sha(raw);
        if ordinal == 0 {
            first_hash = Some(digest);
        }
        files.push(serde_json::json!({"path":name,"bytes":raw.len(),"sha256":digest,"lines":3}));
    }
    for mode in ["hybrid", "vector", "lexical"] {
        let value = serde_json::json!({"query":question,"mode":mode,"limit":8,"max_bytes":65536,
            "full_chunks":true,"response_mode":"full","include_status":true});
        let _: airk_mcp::SearchArgs = serde_json::from_value(value.clone())?;
        write(&output, &format!("search-{mode}.json"), &value)?;
    }
    let references = serde_json::json!({"name":"SyntheticRelay","limit":32,"max_bytes":65536,
        "include_files":true,"include_count":true,"order":"path","response_mode":"full","include_status":true});
    let _: airk_mcp::ReferenceArgs = serde_json::from_value(references.clone())?;
    write(&output, "references-exact.json", &references)?;
    let read = serde_json::json!({"path":"doc-00.txt","start_line":1,"end_line":3,"max_bytes":8192,
        "response_mode":"full","include_status":true});
    let _: airk_mcp::ReadArgs = serde_json::from_value(read.clone())?;
    write(&output, "read-file.json", &read)?;
    let mut pinned_read = read;
    pinned_read
        .as_object_mut()
        .context("read case not object")?
        .insert(
            "known_hash".into(),
            serde_json::to_value(first_hash.context("synthetic first source missing")?)?,
        );
    let _: airk_mcp::ReadArgs = serde_json::from_value(pinned_read.clone())?;
    write(&output, "read-file-known-source.json", &pinned_read)?;
    answer
        .as_object_mut()
        .context("answer case not object")?
        .insert("question".into(), question.into());
    let decoded: airk_mcp::AnswerArgs = serde_json::from_value(answer.clone())?;
    decoded.request.validate()?;
    write(&output, "answer.json", &answer)?;
    let resources = ControlMethod::Resources;
    ensure!(
        resources.metadata_relay_refusal().is_none(),
        "Resources unavailable to metadata relay"
    );
    write(&output, "resources.json", &resources)?;
    write(
        &output,
        "source-scopes.private.json",
        &serde_json::json!({
            "schema":"airk.private-selected-synthetic-source-scopes/1","source_roots":[source],
            "questions":[{"id":"synthetic:maintenance","question":question,"source_root":source,"split":"synthetic"}],
            "scope":"Fresh synthetic only; no CAL/confirmation, answer/support/task gold or human actionlabels. Native live configuration emitter still owns actual index/source/publication bindings."
        }),
    )?;
    write(
        &output,
        "fixture.private.json",
        &serde_json::json!({
            "schema":"airk.private-selected-synthetic-native-fixture/1","evaluation":true,"release_qualified":false,
            "source_root":source,"files":files,"NN_invocations":0,"human_semantic_gold_count":0,
            "requests":[{"file":"search-hybrid.json","tool":"search"},{"file":"search-vector.json","tool":"search"},
                {"file":"search-lexical.json","tool":"search"},{"file":"references-exact.json","tool":"references"},
                {"file":"read-file.json","tool":"read_file"},{"file":"read-file-known-source.json","tool":"read_file"},
                {"file":"answer.json","tool":"answer"},{"file":"resources.json","transport":"service rpc"}],
            "semantics":"SearchMode has only hybrid/vector/lexical. Exact is original case-sensitive whole-identifier references, not a fabricated exact search mode. JSON cases are validated by the actual MCP owner types; Resources is serialized directly from Core. These fixtures prove exercised lifecycle/isolation/work boundaries only, never quality gold."
        }),
    )?;
    println!(
        "{}",
        serde_json::json!({"output":output,"source_files":8,"NN_invocations":0,"CAL_confirmation_accessed":false})
    );
    Ok(())
}
