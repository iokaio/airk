// SPDX-License-Identifier: Apache-2.0
//! Private Answer evaluation, change set 1: the CPU fixture with its layers on.
//!
//! The CPU roster stays the installation's restart identity, exactly as for the
//! CPU selector. The change set's extra pinned profiles (a width-16 Focus
//! profile per instruction and Context profiles with the provisional Read rule)
//! join the evaluation catalog only: their original assets are authenticated
//! once and held as read descriptors, never seeded or copied into the store.
//! The kernel layers are bound once for this process (default `all`).

use super::super::*;
use super::from_env;
use airk_core::arbiter::{Asset, AssetRole};

const INPUTS: &str = "AIRK_ARBITER_EVALUATION_CHANGE_SET_INPUTS";
const LAYERS: &str = "AIRK_ARBITER_EVALUATION_ANSWER_LAYERS";
const SEED: &str = "AIRK_ARBITER_EVALUATION_CHANGE_SET_SEED_CPU";
const OUTPUT: &str = "AIRK_ARBITER_EVALUATION_CHANGE_SET_OUTPUT";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    path: PathBuf,
    sha256: Sha256Digest,
}

/// The fixture agent's input format plus pinned per-asset replacements.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtraInput {
    profile: PathBuf,
    assets: PathBuf,
    fixtures: PathBuf,
    profile_sha256: Sha256Digest,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    overrides: BTreeMap<String, Pin>,
    /// A trained Focus candidate: the SHA-pinned candidate input (fixture
    /// format) that lists this exact profile. Only a listed candidate may
    /// carry new external weights; its graph must still be the twin's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    candidate: Option<Pin>,
}

/// One extra profile and whether a pinned candidate input lists it.
struct Extra<'a> {
    profile: &'a Profile,
    candidate: bool,
}

fn layers(value: Option<&str>) -> Result<airk_kernel::arbiter::software::Layers> {
    airk_kernel::arbiter::software::Layers::parse(value.unwrap_or("all"))
}

/// Which CPU roster weights a fresh installation seeds; a restart never seeds.
fn seed(value: Option<&str>, restart: bool) -> Result<Option<Vec<ModelChoice>>> {
    Ok(match (restart, value) {
        (true, None) => None,
        (true, Some(_)) => bail!("a change-set restart never seeds the CPU roster"),
        (false, Some("all")) => Some(vec![
            ModelChoice::FocusQwenReranker06B,
            ModelChoice::ContextQwenCompact,
            ModelChoice::ContextQwenLarge,
        ]),
        (false, Some("focus")) => Some(vec![ModelChoice::FocusQwenReranker06B]),
        (false, Some("none")) => Some(vec![]),
        (false, _) => bail!("a fresh change-set fixture requires {SEED}=all|focus|none"),
    })
}

/// Extra profiles are unqualified private CPU profiles with new manifests, each
/// a variant of a CPU roster model that shares its weights. A listed Focus
/// candidate shares the twin's graph (`Model` assets) and may differ only in
/// its single `ExternalWeights` asset.
fn require_extras(cpu: &[Profile], extras: &[Extra<'_>]) -> Result<()> {
    let mut manifests: BTreeSet<_> = cpu.iter().map(|profile| profile.manifest).collect();
    ensure!(
        !extras.is_empty() && extras.len() <= 8,
        "change set names one to eight extra profiles"
    );
    for Extra {
        profile: extra,
        candidate,
    } in extras
    {
        let twin = cpu.iter().find(|profile| profile.model == extra.model);
        let weights = |profile: &Profile| {
            profile
                .assets
                .iter()
                .filter(|asset| asset.role == AssetRole::ExternalWeights)
                .count()
        };
        let reuses = |twin: &Profile| {
            twin.assets
                .iter()
                .filter(|asset| match asset.role {
                    AssetRole::Model => true,
                    AssetRole::ExternalWeights => !candidate,
                    _ => false,
                })
                .all(|asset| extra.assets.contains(asset))
                && (!candidate
                    || extra.model == ModelChoice::FocusQwenReranker06B
                        && weights(twin) == 1
                        && weights(extra) == 1
                        && extra
                            .assets
                            .iter()
                            .filter(|asset| asset.role == AssetRole::Model)
                            .count()
                            == twin
                                .assets
                                .iter()
                                .filter(|asset| asset.role == AssetRole::Model)
                                .count())
        };
        ensure!(
            extra.qualification.is_none()
                && extra.profile_id.starts_with("private-")
                && extra.device_execution.is_none()
                && manifests.insert(extra.manifest)
                && twin.is_some_and(reuses),
            "change-set extras must be distinct private CPU variants that reuse a roster model's weights (a pinned Focus candidate may replace only its external weights)"
        );
    }
    Ok(())
}

/// Like the roster's preparation: pinned profile bytes, the asset closure under
/// artifacts, held read descriptors; overrides are pinned small replacements.
pub(in crate::service_runtime::evaluation_fixture_tests) fn prepare_extras(
    path: &Path,
    expected: Sha256Digest,
) -> Result<(Vec<u8>, Vec<Prepared>, Vec<bool>)> {
    let artifacts = artifact_root()?;
    let (_parent, mut file) = source_file(path, &artifacts)?;
    let raw_inputs = bounded(&mut file)?;
    ensure!(
        sha(&raw_inputs) == expected,
        "change-set inputs hash differs"
    );
    let inputs: Vec<ExtraInput> = serde_json::from_slice(&raw_inputs)?;
    let mut prepared = Vec::new();
    let mut candidates = Vec::new();
    for input in inputs {
        if let Some(pin) = &input.candidate {
            listed_candidate(&input, pin, &artifacts)?;
        }
        candidates.push(input.candidate.is_some());
        let (_parent, mut file) = source_file(&input.profile, &artifacts)?;
        let raw = bounded(&mut file)?;
        ensure!(
            sha(&raw) == input.profile_sha256,
            "change-set profile bytes differ from pinned input"
        );
        let profile: Profile = serde_json::from_slice(&raw)?;
        profile.validate()?;
        let root = input.assets.canonicalize()?;
        ensure!(
            root.starts_with(&artifacts),
            "change-set model assets leave artifacts"
        );
        let assets = OpenedDirectory::open(&root)?;
        let fixture_name = input
            .fixtures
            .file_name()
            .context("change-set fixture name missing")?;
        ensure!(
            input.fixtures.canonicalize()? == root.join(fixture_name).canonicalize()?
                && profile
                    .assets
                    .iter()
                    .any(|asset| OsStr::new(&asset.name) == fixture_name),
            "change-set fixture is outside the profile's asset closure"
        );
        let mut sources = BTreeMap::new();
        for asset in &profile.assets {
            let source = match input.overrides.get(&asset.name) {
                Some(pin) => {
                    let (_directory, mut replacement) = source_file(&pin.path, &artifacts)?;
                    let bytes = bounded(&mut replacement)?;
                    ensure!(
                        sha(&bytes) == pin.sha256 && pin.sha256 == asset.sha256,
                        "change-set replacement asset differs from its pin or profile"
                    );
                    replacement
                }
                None => assets.open_file(OsStr::new(&asset.name))?,
            };
            ensure!(
                source.metadata()?.len() == asset.bytes,
                "change-set asset length differs"
            );
            sources.insert(asset.name.clone(), source);
        }
        ensure!(
            input
                .overrides
                .keys()
                .all(|name| profile.assets.iter().any(|asset| &asset.name == name)),
            "change-set replacement names no profile asset"
        );
        prepared.push(Prepared {
            profile,
            raw,
            sha256: input.profile_sha256,
            sources,
            _profile_file: file,
            _assets: assets,
        });
    }
    Ok((raw_inputs, prepared, candidates))
}

/// The pinned candidate input must list exactly this profile, assets and fixture.
fn listed_candidate(input: &ExtraInput, pin: &Pin, artifacts: &Path) -> Result<()> {
    let (_directory, mut file) = source_file(&pin.path, artifacts)?;
    let bytes = bounded(&mut file)?;
    ensure!(
        sha(&bytes) == pin.sha256,
        "candidate input differs from its pin"
    );
    let listed: Vec<Input> = serde_json::from_slice(&bytes)?;
    let same = |left: &Path, right: &Path| -> Result<bool> {
        Ok(left.canonicalize()? == right.canonicalize()?)
    };
    let mut found = false;
    for entry in &listed {
        found |= entry.profile_sha256 == input.profile_sha256
            && same(&entry.profile, &input.profile)?
            && same(&entry.assets, &input.assets)?
            && same(&entry.fixtures, &input.fixtures)?;
    }
    ensure!(found, "pinned candidate input does not list this profile");
    Ok(())
}

/// Which prefix of this lifetime's roster a prior Ready of `count` profiles may
/// restart: the CPU roster, the roster with the same extras (a change-set
/// lifetime), or that roster with this selector's extension profiles (the
/// CS1-CUDA lifetime). Any other length is refused.
fn roster_length(count: usize, cpu: usize, all: usize, extended: usize) -> Result<usize> {
    ensure!(
        count == cpu || count == all || count == extended,
        "restart roster is neither the CPU roster nor this change set's roster"
    );
    Ok(count)
}

/// The prior lifetime's roster length (see [`roster_length`]).
fn restart_roster(cpu: usize, all: usize, extended: usize) -> Result<Option<usize>> {
    let Some(path) = std::env::var_os("AIRK_ARBITER_EVALUATION_RESTART") else {
        return Ok(None);
    };
    let digest: Sha256Digest = std::env::var("AIRK_ARBITER_EVALUATION_RESTART_SHA256")?.parse()?;
    let request: super::Request = serde_json::from_slice(&super::read(&super::Pinned {
        path: PathBuf::from(path),
        sha256: digest,
    })?)?;
    let ready: Ready = serde_json::from_slice(&super::read(&request.ready)?)?;
    roster_length(ready.profiles.len(), cpu, all, extended).map(Some)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "private CPU Answer evaluation with change set 1; exact reviewed inputs required"]
async fn actual_private_arbiter_fixture_change_set_1() -> Result<()> {
    let consent = std::env::var("AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT")
        .context("change-set fixture requires explicit resource configuration consent")?;
    run(
        software_v2_answer_template()?,
        software_v2_resource_configuration(&consent)?,
    )
    .await
}

/// Extra catalog entries beyond the change set's own extras, with the hook
/// that attaches them to the evaluation owner (the CUDA Context selector).
/// They are never seeded and never registered as catalog-only CPU profiles.
pub(in crate::service_runtime::evaluation_fixture_tests) struct Extension {
    pub(in crate::service_runtime::evaluation_fixture_tests) prepared: Vec<Prepared>,
    pub(in crate::service_runtime::evaluation_fixture_tests) record: serde_json::Value,
    pub(in crate::service_runtime::evaluation_fixture_tests) attach: Attach,
}

/// Attaches the extension's entries to the evaluation owner during startup.
pub(in crate::service_runtime::evaluation_fixture_tests) type Attach =
    Box<dyn FnOnce(EvaluationOwner, &[Prepared]) -> Result<EvaluationOwner> + Send>;

async fn run(
    context_request: serde_json::Value,
    resource_configuration: airk_core::service_resources::ResourceConfiguration,
) -> Result<()> {
    run_with(context_request, resource_configuration, None).await
}

/// The change-set lifetime, optionally with an [`Extension`].
pub(in crate::service_runtime::evaluation_fixture_tests) async fn run_with(
    context_request: serde_json::Value,
    resource_configuration: airk_core::service_resources::ResourceConfiguration,
    extension: Option<Extension>,
) -> Result<()> {
    let decoded: airk_mcp::AnswerArgs = serde_json::from_value(context_request.clone())?;
    decoded.request.validate()?;
    let layers = layers(std::env::var(LAYERS).ok().as_deref())?;
    let input = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_EVALUATION_INPUTS")
            .context("AIRK_ARBITER_EVALUATION_INPUTS missing")?,
    );
    let expected: Sha256Digest = std::env::var("AIRK_ARBITER_EVALUATION_INPUTS_SHA256")?.parse()?;
    let extra_input = PathBuf::from(std::env::var_os(INPUTS).context("change-set inputs missing")?);
    let extra_expected: Sha256Digest = std::env::var(format!("{INPUTS}_SHA256"))?.parse()?;
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
        _ => bail!(
            "the change-set fixture requires actual Context scoring; the CAL observer stays off"
        ),
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
    let (extra_inputs, extras, candidates) = prepare_extras(&extra_input, extra_expected)?;
    require_extras(
        &prepared
            .iter()
            .map(|entry| entry.profile.clone())
            .collect::<Vec<_>>(),
        &extras
            .iter()
            .zip(&candidates)
            .map(|(entry, candidate)| Extra {
                profile: &entry.profile,
                candidate: *candidate,
            })
            .collect::<Vec<_>>(),
    )?;
    let cpu = prepared.len();
    prepared.extend(extras);
    let all = prepared.len();
    let (attach, extension_record) = match extension {
        Some(extension) => {
            prepared.extend(extension.prepared);
            (Some(extension.attach), extension.record)
        }
        None => (None, serde_json::Value::Null),
    };
    let restart = match restart_roster(cpu, all, prepared.len())? {
        Some(count) => from_env(expected, &prepared[..count])?,
        None => None,
    };
    let seed = seed(std::env::var(SEED).ok().as_deref(), restart.is_some())?;
    airk_kernel::arbiter::software::bind_evaluation_layers(layers)?;
    let parent_pin = OpenedDirectory::open(&parent)?;
    let work_pin = crate::ipc::endpoint::create_root_at(
        &parent_pin,
        work.file_name().context("private work name missing")?,
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
        "change-set-inputs.json",
        &extra_inputs,
        LIMIT,
        false,
    )?;
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
        "change-set.json",
        &serde_json::json!({
            "schema": "airk.private.answer-change-set/1",
            "change_set": 1,
            "evaluation": true,
            "release_qualified": false,
            "layers": layers,
            "layer_names": layers.names(),
            "restart": restart.is_some(),
            "cpu_seed": seed,
            "cpu_inputs_sha256": expected,
            "change_set_inputs_sha256": extra_expected,
            "extension": extension_record,
            "catalog_only": prepared[cpu..all].iter().zip(&candidates).map(|(entry, candidate)| serde_json::json!({
                "candidate": candidate,
                "manifest": entry.profile.manifest,
                "model": entry.profile.model,
                "profile_id": entry.profile.profile_id,
                "profile_sha256": entry.sha256,
            })).collect::<Vec<_>>(),
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
    let factory: EvaluationFactory = Box::new(move |lease, gate| {
        let namespace = gate
            .catalog_clock()
            .context("actual fixture catalog clock missing")?
            .namespace;
        for entry in &source[cpu..all] {
            arbiter::catalog_only::register(&lease, &entry.profile, |asset| {
                entry
                    .sources
                    .get(&asset.name)
                    .context("original change-set asset descriptor missing")?
                    .try_clone()
                    .map_err(Into::into)
            })?;
        }
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
        if let Some(attach) = attach {
            owner = attach(owner, &source)?;
        }
        if let Some(models) = &seed {
            for entry in source[..cpu]
                .iter()
                .filter(|entry| models.contains(&entry.profile.model))
            {
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
    let released = airk_store::service::ServiceLease::acquire(&control)?;
    drop(released);
    write(
        &work,
        "closed.json",
        &serde_json::json!({"schema":"airk.private-arbiter-fixture-closed/1","evaluation":true,"release_qualified":false,"control_root":control,"instance":instance,"shutdown_completed":true,"service_lease_reacquired":true}),
    )?;
    Ok(())
}

/// The change set's profile variants, derived from one pinned roster profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Variant {
    /// Change 4 and 5: width 16 with the SoftwareEvidenceV1 instruction.
    FocusSoftware,
    /// Change 4 alone: width 16 with the publisher instruction.
    FocusPublisher,
    /// Change 6c: the Context profile with the provisional Read rule.
    ContextProvisionalRead,
}

impl Variant {
    fn tag(self) -> &'static str {
        match self {
            Self::FocusSoftware => "cs1-software-w16",
            Self::FocusPublisher => "cs1-publisher-w16",
            Self::ContextProvisionalRead => "cs1-provisional-read",
        }
    }
}

fn change_set_manifest(
    source_sha256: Sha256Digest,
    variant: Variant,
    profile: &Profile,
) -> Result<Sha256Digest> {
    let mut preimage = profile.clone();
    preimage.manifest = Sha256Digest::from_bytes([0; 32]);
    Ok(airk_core::identity::framed_digest(
        b"airk.private.answer-change-set-1-profile/1",
        &[
            source_sha256.as_bytes(),
            variant.tag().as_bytes(),
            &serde_json::to_vec(&preimage)?,
        ],
    ))
}

/// A width-16 Focus variant. The weights, compiler and calibration assets are
/// the source profile's; only width, policy, identity and name change.
fn focus_variant(
    source: &Profile,
    source_sha256: Sha256Digest,
    variant: Variant,
) -> Result<Profile> {
    use airk_core::arbiter::Task;
    ensure!(
        source.model == ModelChoice::FocusQwenReranker06B
            && source.qualification.is_none()
            && source.device_execution.is_none()
            && matches!(variant, Variant::FocusSoftware | Variant::FocusPublisher),
        "Focus variant requires the private CPU Focus roster profile"
    );
    let policy = match variant {
        Variant::FocusSoftware => {
            airk_retrieval::arbiter::compiler::focus::experiment::software_policy_digest()?
        }
        _ => airk_retrieval::arbiter::compiler::focus::policy_digest()?,
    };
    let mut profile = source.clone();
    profile.max_candidates = 16;
    profile.policy = policy;
    profile.task_policies = BTreeMap::from([(Task::FocusRank, policy)]);
    profile.profile_id = format!("{}-{}", source.profile_id, variant.tag());
    profile.downloads.clear();
    profile.qualified_targets.clear();
    profile.manifest = change_set_manifest(source_sha256, variant, &profile)?;
    profile.validate()?;
    Ok(profile)
}

/// The option-A provisional rule (design 6c): a Read card is accepted when it
/// is the model's top card; every terminal class stays unsupported.
pub(in crate::service_runtime::evaluation_fixture_tests) fn provisional_calibration(
    frozen: &airk_core::arbiter::context_action::calibration::JointCalibration,
) -> Result<airk_core::arbiter::context_action::calibration::JointCalibration> {
    let mut calibration = frozen.clone();
    let reason = "provisional evaluation: terminal not qualified";
    calibration.schema =
        airk_core::arbiter::context_action::calibration::CALIBRATION_SCHEMA_V2.into();
    calibration.rules = serde_json::from_value(serde_json::json!([{
        "action":"read","allowed_gaps":["unsupported"],"allowed_reasons":["additional_evidence_may_help"],
        "allowed_roles":["unknown"],"allowed_relevance":["uncertain"],"minimum_raw_margin":0.0}]))?;
    calibration.class_support = Some(serde_json::from_value(serde_json::json!([
        {"action":"read","support":{"status":"supported","qualification":null}},
        {"action":"sufficient","support":{"status":"unsupported","reason":reason}},
        {"action":"clarify","support":{"status":"unsupported","reason":reason}},
        {"action":"no_justified_read","support":{"status":"unsupported","reason":reason}}
    ]))?);
    calibration.validate_class_support()?;
    Ok(calibration)
}

/// A Context variant whose Calibration asset is `calibration`; all other assets
/// (weights included) stay the source profile's.
fn context_variant(
    source: &Profile,
    source_sha256: Sha256Digest,
    calibration: &[u8],
) -> Result<Profile> {
    use airk_core::arbiter::Task;
    ensure!(
        matches!(
            source.model,
            ModelChoice::ContextQwenCompact | ModelChoice::ContextQwenLarge
        ) && source.qualification.is_none()
            && source.device_execution.is_none(),
        "Context variant requires a private CPU Context roster profile"
    );
    let mut profile = source.clone();
    let asset = profile
        .assets
        .iter_mut()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("Context calibration asset missing")?;
    asset.bytes = calibration.len() as u64;
    asset.sha256 = sha(calibration);
    profile.calibration = asset.sha256;
    profile.profile_id = format!(
        "{}-{}",
        source.profile_id,
        Variant::ContextProvisionalRead.tag()
    );
    profile.downloads.clear();
    profile.qualified_targets.clear();
    profile.manifest =
        change_set_manifest(source_sha256, Variant::ContextProvisionalRead, &profile)?;
    profile.validate()?;
    let policy = *profile
        .task_policies
        .get(&Task::ContextAction)
        .context("Context task policy missing")?;
    airk_core::arbiter::context_action::calibration::JointCalibration::parse_and_validate(
        calibration,
        &profile,
        policy,
    )?;
    Ok(profile)
}

fn write_new(directory: &Path, name: &str, bytes: &[u8]) -> Result<(PathBuf, Sha256Digest)> {
    use std::io::Write;
    let path = directory.join(name);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok((path, sha(bytes)))
}

fn pretty(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Writes the change set's private profiles, provisional calibrations and the
/// extra inputs roster (with SHA-256 pins) into a fresh folder under artifacts.
/// Weights are never read or copied: every variant names the roster's assets.
#[test]
#[ignore = "private change-set profile emitter; pinned CPU roster and a fresh output folder required"]
fn emit_private_change_set_1_profiles() -> Result<()> {
    let input = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_EVALUATION_INPUTS")
            .context("AIRK_ARBITER_EVALUATION_INPUTS missing")?,
    );
    let expected: Sha256Digest = std::env::var("AIRK_ARBITER_EVALUATION_INPUTS_SHA256")?.parse()?;
    let output = PathBuf::from(std::env::var_os(OUTPUT).context("change-set output missing")?);
    let artifacts = artifact_root()?;
    ensure!(
        output.is_absolute()
            && !output.try_exists()?
            && output
                .parent()
                .context("output parent missing")?
                .canonicalize()?
                .starts_with(&artifacts),
        "change-set output must be a fresh folder under artifacts"
    );
    let (_, mut raw) = source_file(&input, &artifacts)?;
    let raw = bounded(&mut raw)?;
    ensure!(sha(&raw) == expected, "CPU roster inputs hash differs");
    let roster: Vec<Input> = serde_json::from_slice(&raw)?;
    std::fs::create_dir(&output)?;
    let mut extras = Vec::new();
    let mut receipt = Vec::new();
    for entry in &roster {
        let (_, mut file) = source_file(&entry.profile, &artifacts)?;
        let bytes = bounded(&mut file)?;
        ensure!(
            sha(&bytes) == entry.profile_sha256,
            "roster profile bytes differ"
        );
        let source: Profile = serde_json::from_slice(&bytes)?;
        source.validate()?;
        let mut variants = Vec::new();
        match source.model {
            ModelChoice::FocusQwenReranker06B => {
                for variant in [Variant::FocusSoftware, Variant::FocusPublisher] {
                    variants.push((
                        variant,
                        focus_variant(&source, entry.profile_sha256, variant)?,
                        BTreeMap::new(),
                    ));
                }
            }
            ModelChoice::ContextQwenCompact | ModelChoice::ContextQwenLarge => {
                let asset = source
                    .assets
                    .iter()
                    .find(|asset| asset.role == AssetRole::Calibration)
                    .context("roster calibration asset missing")?;
                let (_, mut file) = source_file(&entry.assets.join(&asset.name), &artifacts)?;
                let frozen_bytes = bounded(&mut file)?;
                ensure!(
                    sha(&frozen_bytes) == asset.sha256,
                    "roster calibration bytes differ"
                );
                let frozen = airk_core::arbiter::context_action::calibration::JointCalibration::parse_and_validate(
                    &frozen_bytes,
                    &source,
                    *source
                        .task_policies
                        .get(&airk_core::arbiter::Task::ContextAction)
                        .context("roster Context policy missing")?,
                )?;
                let calibration = pretty(&provisional_calibration(&frozen)?)?;
                let name =
                    format!("{:?}-provisional-read-calibration.json", source.model).to_lowercase();
                let (path, digest) = write_new(&output, &name, &calibration)?;
                let profile = context_variant(&source, entry.profile_sha256, &calibration)?;
                variants.push((
                    Variant::ContextProvisionalRead,
                    profile,
                    BTreeMap::from([(
                        asset.name.clone(),
                        Pin {
                            path,
                            sha256: digest,
                        },
                    )]),
                ));
            }
            _ => bail!("unexpected roster model"),
        }
        for (variant, profile, overrides) in variants {
            let name = format!("{}.json", profile.profile_id);
            let (path, digest) = write_new(&output, &name, &pretty(&profile)?)?;
            receipt.push(serde_json::json!({
                "variant": variant.tag(), "model": profile.model, "profile": path, "profile_sha256": digest,
                "manifest": profile.manifest, "source_profile": entry.profile, "source_profile_sha256": entry.profile_sha256,
                "source_manifest": source.manifest, "max_candidates": profile.max_candidates,
                "task_policies": profile.task_policies, "calibration": profile.calibration,
                "overrides": overrides, "weights_copied": false,
            }));
            extras.push(ExtraInput {
                profile: path,
                assets: entry.assets.clone(),
                fixtures: entry.fixtures.clone(),
                profile_sha256: digest,
                overrides,
                candidate: None,
            });
        }
    }
    let (inputs_path, inputs_sha256) =
        write_new(&output, "change-set-1-inputs.json", &pretty(&extras)?)?;
    let report = serde_json::json!({
        "schema": "airk.private.answer-change-set-1-profiles/1",
        "evaluation": true, "release_qualified": false,
        "cpu_inputs": input, "cpu_inputs_sha256": expected,
        "change_set_inputs": inputs_path, "change_set_inputs_sha256": inputs_sha256,
        "focus_software_policy": airk_retrieval::arbiter::compiler::focus::experiment::software_policy_digest()?,
        "focus_publisher_policy": airk_retrieval::arbiter::compiler::focus::policy_digest()?,
        "profiles": receipt,
    });
    write_new(&output, "change-set-1-profiles.json", &pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[test]
fn restart_accepts_the_cpu_change_set_and_extended_rosters_only() -> Result<()> {
    // CPU roster 3, change-set extras 5 more, one CUDA extension profile.
    for count in [3, 8, 9] {
        assert_eq!(roster_length(count, 3, 8, 9)?, count);
    }
    for refused in [0, 2, 4, 7, 10] {
        assert!(roster_length(refused, 3, 8, 9).is_err(), "{refused}");
    }
    // Without an extension the extended length is the change-set roster.
    assert_eq!(roster_length(8, 3, 8, 8)?, 8);
    assert!(roster_length(9, 3, 8, 8).is_err());
    Ok(())
}

#[test]
fn change_set_layers_default_to_all_and_seeding_is_explicit_for_fresh_only() -> Result<()> {
    use airk_kernel::arbiter::software::Layers;
    assert_eq!(layers(None)?, Layers::all());
    assert!(!layers(Some("all,-focus"))?.focus);
    assert!(layers(Some("bogus")).is_err());
    assert_eq!(seed(None, true)?, None);
    assert!(seed(Some("all"), true).is_err());
    assert_eq!(seed(Some("none"), false)?, Some(vec![]));
    assert_eq!(
        seed(Some("focus"), false)?,
        Some(vec![ModelChoice::FocusQwenReranker06B])
    );
    assert_eq!(
        seed(Some("all"), false)?.map(|models| models.len()),
        Some(3)
    );
    assert!(seed(None, false).is_err());
    Ok(())
}

fn fixture_profile(model: ModelChoice) -> Result<Profile> {
    let mut profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    profile.model = model;
    profile.profile_id = "private-roster".into();
    profile.qualification = None;
    profile.qualified_targets.clear();
    profile.downloads.clear();
    Ok(profile)
}

fn plain(profile: &Profile) -> Extra<'_> {
    Extra {
        profile,
        candidate: false,
    }
}

fn listed(profile: &Profile) -> Extra<'_> {
    Extra {
        profile,
        candidate: true,
    }
}

fn with_external_weights(mut profile: Profile) -> Profile {
    if !profile
        .assets
        .iter()
        .any(|asset| asset.role == AssetRole::ExternalWeights)
    {
        profile.assets.push(Asset {
            name: "model.onnx.data".into(),
            role: AssetRole::ExternalWeights,
            bytes: 1024,
            sha256: Sha256Digest::from_bytes([6; 32]),
        });
    }
    profile
}

fn retrained(roster: &Profile, byte: u8) -> Profile {
    let mut candidate = roster.clone();
    candidate.manifest = Sha256Digest::from_bytes([byte; 32]);
    candidate.profile_id = "private-roster-cand1".into();
    for asset in &mut candidate.assets {
        if asset.role == AssetRole::ExternalWeights {
            asset.sha256 = Sha256Digest::from_bytes([byte; 32]);
        }
    }
    candidate
}

#[test]
fn extras_must_be_distinct_private_variants_that_reuse_roster_weights() -> Result<()> {
    let roster = with_external_weights(fixture_profile(ModelChoice::FocusQwenReranker06B)?);
    let mut extra = roster.clone();
    extra.manifest = Sha256Digest::from_bytes([9; 32]);
    extra.profile_id = "private-roster-cs1".into();
    require_extras(std::slice::from_ref(&roster), &[plain(&extra)])?;
    assert!(require_extras(std::slice::from_ref(&roster), &[plain(&roster)]).is_err());
    let mut public = extra.clone();
    public.profile_id = "arbiter-focus".into();
    assert!(require_extras(std::slice::from_ref(&roster), &[plain(&public)]).is_err());
    let mut foreign = extra.clone();
    for asset in &mut foreign.assets {
        if matches!(asset.role, AssetRole::Model | AssetRole::ExternalWeights) {
            asset.sha256 = Sha256Digest::from_bytes([8; 32]);
        }
    }
    assert!(require_extras(std::slice::from_ref(&roster), &[plain(&foreign)]).is_err());
    let mut other_model = extra;
    other_model.model = ModelChoice::ContextQwenLarge;
    assert!(require_extras(std::slice::from_ref(&roster), &[plain(&other_model)]).is_err());
    assert!(require_extras(&[], &[]).is_err());
    Ok(())
}

#[test]
fn a_listed_focus_candidate_may_replace_only_its_external_weights() -> Result<()> {
    let roster = with_external_weights(fixture_profile(ModelChoice::FocusQwenReranker06B)?);
    let candidate = retrained(&roster, 7);
    // Accepted: listed, same graph, new weights, new manifest, private CPU.
    require_extras(std::slice::from_ref(&roster), &[listed(&candidate)])?;
    // Refused: the same new weights without a pinned candidate listing.
    assert!(require_extras(std::slice::from_ref(&roster), &[plain(&candidate)]).is_err());
    // Refused: a different graph (Model asset), even when listed.
    let mut graph = candidate.clone();
    for asset in &mut graph.assets {
        if asset.role == AssetRole::Model {
            asset.sha256 = Sha256Digest::from_bytes([5; 32]);
        }
    }
    assert!(require_extras(std::slice::from_ref(&roster), &[listed(&graph)]).is_err());
    // Refused: an added graph asset.
    let mut extra_graph = candidate.clone();
    let mut second = extra_graph
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Model)
        .unwrap()
        .clone();
    second.name = "second.onnx".into();
    extra_graph.assets.push(second);
    assert!(require_extras(std::slice::from_ref(&roster), &[listed(&extra_graph)]).is_err());
    // Refused: two external weight files.
    let mut two = candidate.clone();
    two.assets.push(Asset {
        name: "more.onnx.data".into(),
        role: AssetRole::ExternalWeights,
        bytes: 1024,
        sha256: Sha256Digest::from_bytes([4; 32]),
    });
    assert!(require_extras(std::slice::from_ref(&roster), &[listed(&two)]).is_err());
    // Refused: qualified, public, a reused manifest, or not Focus.
    let mut qualified = candidate.clone();
    qualified.qualification = Some(Sha256Digest::from_bytes([3; 32]));
    let mut public = candidate.clone();
    public.profile_id = "arbiter-focus-cand1".into();
    let mut reused = candidate.clone();
    reused.manifest = roster.manifest;
    for refused in [&qualified, &public, &reused] {
        assert!(require_extras(std::slice::from_ref(&roster), &[listed(refused)]).is_err());
    }
    let context = with_external_weights(fixture_profile(ModelChoice::ContextQwenCompact)?);
    let context_candidate = retrained(&context, 2);
    assert!(require_extras(&[roster.clone(), context], &[listed(&context_candidate)]).is_err());
    // Two candidates cannot share one manifest.
    assert!(
        require_extras(
            std::slice::from_ref(&roster),
            &[listed(&candidate), listed(&candidate)]
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn provisional_calibration_admits_only_read_and_keeps_terminals_unsupported() -> Result<()> {
    use airk_core::arbiter::context_action::calibration::{
        ClassSupport, ContextActionClass, JointCalibration,
    };
    let frozen: JointCalibration = serde_json::from_value(serde_json::json!({
        "schema":"airk.arbiter.context-action.calibration/2","model":"arbiter-context-qwen3-17b",
        "compiler":Sha256Digest::from_bytes([1;32]),"task_policy":Sha256Digest::from_bytes([2;32]),
        "score_abi":{"readout":"decision_symbol","inputs":[{"name":"input_ids","element":"int64","axes":["batch","tokens"]}],
            "output":{"name":"scores","element":"float32","axes":["batch","options"]}},
        "assets":{},"rules":[],
        "class_support":[
            {"action":"read","support":{"status":"unsupported","reason":"probe"}},
            {"action":"sufficient","support":{"status":"unsupported","reason":"probe"}},
            {"action":"clarify","support":{"status":"unsupported","reason":"probe"}},
            {"action":"no_justified_read","support":{"status":"unsupported","reason":"probe"}}]}))?;
    let provisional = provisional_calibration(&frozen)?;
    assert_eq!(provisional.rules.len(), 1);
    assert_eq!(provisional.rules[0].action, ContextActionClass::Read);
    assert_eq!(provisional.rules[0].minimum_raw_margin, 0.0);
    let classes = provisional.class_support.as_ref().unwrap();
    assert!(classes.iter().all(|class| match class.action {
        ContextActionClass::Read => matches!(
            class.support,
            ClassSupport::Supported {
                qualification: None
            }
        ),
        _ => matches!(class.support, ClassSupport::Unsupported { .. }),
    }));
    assert_eq!(provisional.compiler, frozen.compiler);
    assert_eq!(provisional.task_policy, frozen.task_policy);
    Ok(())
}

#[test]
fn change_set_manifests_are_new_identities_per_variant() -> Result<()> {
    let profile = fixture_profile(ModelChoice::FocusQwenReranker06B)?;
    let source = Sha256Digest::from_bytes([3; 32]);
    let software = change_set_manifest(source, Variant::FocusSoftware, &profile)?;
    assert_eq!(
        software,
        change_set_manifest(source, Variant::FocusSoftware, &profile)?
    );
    assert_ne!(
        software,
        change_set_manifest(source, Variant::FocusPublisher, &profile)?
    );
    assert_ne!(
        software,
        change_set_manifest(
            Sha256Digest::from_bytes([4; 32]),
            Variant::FocusSoftware,
            &profile
        )?
    );
    let mut self_hashed = profile.clone();
    self_hashed.manifest = Sha256Digest::from_bytes([5; 32]);
    assert_eq!(
        software,
        change_set_manifest(source, Variant::FocusSoftware, &self_hashed)?
    );
    assert_ne!(software, profile.manifest);
    Ok(())
}
