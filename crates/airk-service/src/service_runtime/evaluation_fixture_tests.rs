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
        inputs.len() == 4,
        "private fixture requires exactly the four reviewed model choices"
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
                ModelChoice::SmallIntern,
                ModelChoice::SmallStrands,
                ModelChoice::LargeIntern,
                ModelChoice::LargeKev
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

fn context_template() -> Result<airk_core::arbiter::ContextRequest> {
    use airk_core::arbiter::{Budget, ContextRequest, REQUEST_SCHEMA};
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
        ) == (8, 3, 8, 2048, 32, 262144, 30000),
        "native defaults differ from the frozen goal budget"
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
    let mut request = serde_json::to_value(context_template()?)?;
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
        serde_json::json!({"schema":"airk.arbiter.context.request/1","question":"__PRIVATE_FROZEN_QUESTION__","budget":{"calls":8,"rounds":3,"fanout":8,"input_tokens":2048,"candidates":32,"max_bytes":262144,"deadline_ms":30000}})
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

/// Invoke the compiled test binary directly with --ignored --exact and the
/// documented environment. No public command/configuration can attach it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "private actual native evaluation service; explicit frozen inputs and lifetime required"]
async fn actual_private_arbiter_fixture() -> Result<()> {
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
    let parent_pin = OpenedDirectory::open(&parent)?;
    let work_pin = crate::ipc::endpoint::create_root_at(
        &parent_pin,
        work.file_name().context("private work name missing")?,
    )?;
    let control = work.join("control");
    let _control_pin = crate::ipc::endpoint::create_root_at(&work_pin, OsStr::new("control"))?;
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
    write(&work, "context-request.json", &answer_template()?)?;
    // Use the actual first-start transaction, actual engine identity and real
    // activation/readiness producer; do not seed catalog rows or fake I4.
    let lease = airk_store::service::ServiceLease::acquire(&control)?;
    let lease = first_start::ensure_catalog(lease)?;
    drop(lease);
    let prepared = Arc::new(prepared);
    let source = prepared.clone();
    let (seeded, seed_ready) = tokio::sync::oneshot::channel();
    let factory: EvaluationFactory = Box::new(move |lease, gate| {
        let namespace = gate
            .catalog_clock()
            .context("actual fixture catalog clock missing")?
            .namespace;
        let owner = EvaluationOwner::new(
            lease,
            gate,
            source.iter().map(|entry| entry.profile.clone()).collect(),
        )?;
        for entry in source.iter() {
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
            !owner.configuration()?.result().enabled,
            "private fixture must start disabled"
        );
        seeded
            .send(namespace)
            .map_err(|_| anyhow::anyhow!("private fixture observer disappeared"))?;
        Ok(owner)
    });
    let root = control.clone();
    let mut service = tokio::spawn(async move { serve_evaluation_fixture(&root, factory).await });
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
        return Err(error.context("private fixture failed; no successful closure was claimed"));
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
    let publication = if let Some(value) = body.get("publication").filter(|value| !value.is_null())
    {
        serde_json::from_value::<airk_core::evidence::RevisionEnvelope>(value.clone())?
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
            && ready.control_root == work.join("control"),
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
    let registry = airk_store::service::registry::ServiceRegistry::open_saved(&ready.control_root)?;
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
                                project_ids: vec![],
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
    for (ordinal, binding) in bindings.values_mut().enumerate() {
        let result = crate::ipc::control::request_once(
            &ready.control_root,
            ControlMethod::ToolsCall {
                tool: "status".into(),
                arguments: serde_json::Map::new(),
                selection: airk_core::service::ToolSelection::Open {
                    index_ids: vec![binding.index_id.to_string()],
                    index_set_ids: vec![],
                    connection_id: None,
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
        profile.qualification.is_none() && profile.manifest == selected,
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
    write(&work, &format!("{prefix}.configure.json"), &request)?;
    write(
        &work,
        &format!("{prefix}.bindings.json"),
        &serde_json::json!({"schema":"airk.private-arbiter-index-bindings/1","evaluation":true,"release_qualified":false,"control_root":ready.control_root,"instance":ready.health.instance,"namespace":catalog.namespace,"catalog_revision":catalog.revision,"scopes_sha256":expected,"bindings":bindings.into_values().collect::<Vec<_>>() }),
    )?;
    write(
        &work,
        &format!("{prefix}.receipt.json"),
        &serde_json::json!({"schema":"airk.private-arbiter-configure-instance/1","evaluation":true,"release_qualified":false,"ready_sha256":sha(&ready_bytes),"profile":selected,"profile_sha256":binding.profile_sha256,"expected_revision":status.configuration.revision,"context_next_read_eligible":profile.tasks.contains(&Task::ContextNextRead)&&profile.task_policies.get(&Task::ContextNextRead)==Some(&airk_retrieval::arbiter::tasks::policy_digest()?),"request":request,"actual_status":status,"actual_health":observation}),
    )?;
    Ok(())
}
