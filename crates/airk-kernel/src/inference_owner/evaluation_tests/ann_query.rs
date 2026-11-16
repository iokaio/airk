// SPDX-License-Identifier: Apache-2.0
//! Fresh ungraded Step 7 comparison. Private test-support only; old self-probes stay unchanged.
use super::*;
use airk_core::{
    identity::Incarnation,
    index_catalog::IndexId,
    model_objects::ModelObjectSet,
    service::{ProjectRegistration, ServiceHealth},
    service_lifecycle::{LifecycleAction, LifecyclePhase, LifecycleRecord},
};
use airk_retrieval::vector::mutable::{MutableDiskIndex, SearchLimits};
use airk_store::{
    current::CurrentHead,
    service::{ServiceLease, registry::ServiceRegistry},
};
use serde::Serialize;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path},
    sync::Arc,
    time::Duration,
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    path: PathBuf,
    bytes: u64,
    sha256: Sha256Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SourcePin {
    bytes: u64,
    sha256: Sha256Digest,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Mode {
    mode: String,
    arguments: Value,
    capture_pointer: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Question {
    id: String,
    query: String,
    disposition: Value,
    modes: Vec<Mode>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Roster {
    purpose: String,
    questions: Vec<Question>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Input {
    private_base: PathBuf,
    project: ProjectRegistration,
    control: PathBuf,
    expected_namespace: Incarnation,
    expected_runtime: Sha256Digest,
    expected_index: IndexId,
    expected_profile: String,
    expected_publication: airk_core::evidence::RevisionEnvelope,
    expected_head: Option<CurrentHead>,
    expected_model: Option<ModelObjectSet>,
    expected_indexed_sources: BTreeMap<String, (u64, String)>,
    expected_physical_sources: BTreeMap<String, SourcePin>,
    model_directory: PathBuf,
    query_roster: Pin,
    pre_stop_capture: Pin,
    stop_reply: Pin,
    stop_response_pointer: String,
    pre_stop_health: Pin,
    health_pointer: String,
    // Full source/dirty closure, paired runtime metadata/images, recipe and software-v2 policy.
    // These are opaque frozen evidence; their SHA values cannot supply native authority.
    identities: BTreeMap<String, Pin>,
}

fn unaliased(path: &Path, directory: bool) -> Result<()> {
    ensure!(
        path.is_absolute()
            && !path
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir)),
        "absolute unaliased path required"
    );
    for (offset, ancestor) in path.ancestors().enumerate() {
        let metadata = std::fs::symlink_metadata(ancestor)?;
        ensure!(
            !metadata.file_type().is_symlink() && (offset == 0 || metadata.is_dir()),
            "alias or non-directory ancestor refused"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "reparse ancestry refused"
            );
        }
        if offset == 0 {
            ensure!(
                if directory {
                    metadata.is_dir()
                } else {
                    metadata.is_file()
                },
                "unexpected path kind"
            );
        }
    }
    ensure!(
        path.canonicalize()? == path,
        "canonical path spelling required"
    );
    Ok(())
}
fn digest_file(path: &Path, bytes: u64, deadline: Instant) -> Result<Sha256Digest> {
    unaliased(path, false)?;
    let mut file = File::open(path)?;
    ensure!(
        file.metadata()?.len() == bytes,
        "frozen file length differs"
    );
    let mut hash = Sha256::new();
    let mut read = 0_u64;
    let mut buffer = [0_u8; 65536];
    loop {
        ensure!(
            Instant::now() < deadline,
            "original phase deadline exceeded while hashing"
        );
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        ensure!(read <= bytes, "frozen file grew during hashing");
        hash.update(&buffer[..count]);
    }
    ensure!(
        read == bytes && file.metadata()?.len() == bytes,
        "frozen file length changed"
    );
    Ok(Sha256Digest::from_bytes(hash.finalize().into()))
}
fn check_pin(pin: &Pin, deadline: Instant) -> Result<()> {
    ensure!(
        digest_file(&pin.path, pin.bytes, deadline)? == pin.sha256,
        "frozen input SHA differs: {}",
        pin.path.display()
    );
    Ok(())
}
fn json_pin(pin: &Pin, deadline: Instant) -> Result<Value> {
    ensure!(
        pin.bytes <= 64 << 20,
        "private JSON input exceeds fixed64MiB observation bound"
    );
    check_pin(pin, deadline)?;
    let bytes = std::fs::read(&pin.path)?;
    ensure!(
        bytes.len() as u64 == pin.bytes
            && Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == pin.sha256,
        "JSON changed after file check"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
fn source_census(
    root: &Path,
    expected: &BTreeMap<String, SourcePin>,
    deadline: Instant,
) -> Result<()> {
    unaliased(root, true)?;
    fn walk(
        root: &Path,
        directory: &Path,
        expected: &BTreeMap<String, SourcePin>,
        seen: &mut BTreeSet<String>,
        deadline: Instant,
    ) -> Result<()> {
        ensure!(
            Instant::now() < deadline,
            "original source census deadline exceeded"
        );
        for item in std::fs::read_dir(directory)? {
            let path = item?.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                unaliased(&path, true)?;
                walk(root, &path, expected, seen, deadline)?;
            } else {
                unaliased(&path, false)?;
                let relative = path
                    .strip_prefix(root)?
                    .to_str()
                    .context("non-UTF8 source path")?
                    .replace('\\', "/");
                let pin = expected
                    .get(&relative)
                    .context("source outside complete frozen roster")?;
                ensure!(
                    seen.insert(relative) && digest_file(&path, pin.bytes, deadline)? == pin.sha256,
                    "source identity changed"
                );
            }
        }
        Ok(())
    }
    let mut seen = BTreeSet::new();
    walk(root, root, expected, &mut seen, deadline)?;
    ensure!(seen.len() == expected.len(), "frozen source disappeared");
    Ok(())
}
fn validate_roster(roster: &Roster, index: IndexId) -> Result<()> {
    ensure!(
        roster.purpose == "fresh-ungraded-step7" && !roster.questions.is_empty(),
        "fresh whole pre-score query roster required"
    );
    let mut ids = BTreeSet::new();
    for row in &roster.questions {
        ensure!(
            !row.id.is_empty() && ids.insert(&row.id),
            "empty or duplicate original task ID"
        );
        airk_core::SearchQuery {
            text: row.query.clone(),
            path_prefix: None,
            limit: 10,
            max_bytes: 65536,
        }
        .validate()?;
        ensure!(
            row.modes.len() == 3
                && row
                    .modes
                    .iter()
                    .map(|m| m.mode.as_str())
                    .collect::<BTreeSet<_>>()
                    == BTreeSet::from(["lexical", "vector", "hybrid"]),
            "all three original mode slots required"
        );
        for mode in &row.modes {
            let args = &mode.arguments;
            ensure!(
                args.is_object()
                    && args["query"] == row.query
                    && args["mode"] == mode.mode
                    && args["limit"] == 10
                    && args["max_bytes"] == 65536
                    && args["response_mode"] == "full"
                    && args.get("path_prefix").is_none_or(Value::is_null)
                    && args
                        .get("project_ids")
                        .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty))
                    && args
                        .get("index_ids")
                        .is_none_or(|value| *value == json!([index])),
                "exact whole single-index canonical mode arguments required"
            );
            ensure!(
                mode.capture_pointer.starts_with('/'),
                "original capture row pointer required"
            );
        }
    }
    Ok(())
}
// Preserve opaque arguments/envelopes, using Core's existing materializer only for identity checks.
fn validate_mode_capture(input: &Input, roster: &Roster, capture: &Value) -> Result<bool> {
    ensure!(
        capture["binary_sha256"] == json!(input.identities["runtime_image"].sha256)
            && capture["binary_sha256_after"] == capture["binary_sha256"]
            && capture["airkctl_sha256"] == json!(input.identities["cli_image"].sha256),
        "pre-stop matched runtime images differ"
    );
    ensure!(
        capture["representative"]["source_oracle"]["selection"]["index_id"]
            == json!(input.expected_index)
            && capture["representative"]["source_oracle"]["selection"]["namespace"]
                == json!(input.expected_namespace)
            && capture["representative"]["source_oracle"]["request_scope"]
                == json!({"index_ids":[input.expected_index]}),
        "actual pre-query native whole-index selection differs"
    );
    let mut complete = true;
    let mut pointers = BTreeSet::new();
    for task in &roster.questions {
        for mode in &task.modes {
            ensure!(
                pointers.insert(&mode.capture_pointer),
                "capture row reused for another original slot"
            );
            let row = capture
                .pointer(&mode.capture_pointer)
                .context("original mode slot absent")?;
            ensure!(
                row["case"] == task.id
                    && row["mode"] == mode.mode
                    && row["input"]["query"] == task.query,
                "mode slot differs from original task"
            );
            if row["status"] == "not_run" {
                complete = false;
                continue;
            }
            ensure!(
                row["native_attempt"]["request"]
                    == json!({"name":"airk_search","arguments":mode.arguments}),
                "opaque actual canonical request differs"
            );
            ensure!(
                row["native_attempt"]["budget"]["operation_deadline_ms"]
                    == airk_core::arbiter::Budget::default().deadline_ms,
                "actual native operation deadline differs"
            );
            ensure!(
                row["native_attempt"]["budget"]["sha256"]
                    == json!(input.identities["read_budget_fixture"].sha256),
                "actual Rust-emitted budget fixture differs"
            );
            if let Some(body) = row
                .get("native_result")
                .or_else(|| row.pointer("/native_attempt/native_result"))
            {
                ensure!(
                    body["schema"] == airk_core::combined_reads::COMBINED_READ_SCHEMA,
                    "current U110 capture required"
                );
                let logical = airk_core::combined_reads::materialize_combined_read(body)?;
                let members = logical["publication_vector"]["members"]
                    .as_array()
                    .context("publication members absent")?;
                ensure!(
                    members.len() == 1 && members[0]["index_id"] == json!(input.expected_index),
                    "actual index identity differs"
                );
                if !members[0]["publication"].is_null() {
                    ensure!(
                        members[0]["publication"] == json!(input.expected_publication),
                        "pre-stop mode publication differs from frozen comparison"
                    );
                } else {
                    complete = false;
                }
            } else {
                complete = false;
            }
            if row["status"] != "ok" {
                complete = false;
            }
        }
    }
    Ok(complete)
}
fn process_has_exited(pid: u32) -> Result<()> {
    ensure!(
        pid > 0 && pid != std::process::id(),
        "invalid stopped process identity"
    );
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, ERROR_INVALID_PARAMETER, GetLastError},
            System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        };
        // No PID reuse is accepted as original process exit proof.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if !handle.is_null() {
            unsafe {
                CloseHandle(handle);
            }
            anyhow::bail!("accepted process PID still exists or was reused");
        }
        ensure!(
            unsafe { GetLastError() } == ERROR_INVALID_PARAMETER,
            "process exit cannot be established"
        );
    }
    #[cfg(unix)]
    {
        ensure!(
            unsafe { libc::kill(i32::try_from(pid)?, 0) } == -1
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH),
            "accepted process PID still exists, was reused or exit is unobservable"
        );
    }
    Ok(())
}
fn admit_stopped(input: &Input, stop: &Value, health: &Value) -> Result<Arc<ServiceLease>> {
    let complete_reply = stop
        .pointer(&input.stop_response_pointer)
        .context("native completed Stop response missing")?;
    ensure!(
        complete_reply["status"] == "lifecycle",
        "canonical native runtime lifecycle reply required"
    );
    let record: LifecycleRecord = serde_json::from_value(complete_reply["record"].clone())?;
    let observed: ServiceHealth = serde_json::from_value(
        health
            .pointer(&input.health_pointer)
            .context("actual pre-stop native Health missing")?
            .clone(),
    )?;
    ensure!(
        record.phase == LifecyclePhase::Complete
            && record.request.action == LifecycleAction::Stop
            && record.request.expected_instance == observed.instance
            && record.request.runtime == input.expected_runtime
            && observed.runtime_sha256.as_deref()
                == Some(input.expected_runtime.to_string().as_str())
            && observed.executable_sha256 == input.identities["runtime_image"].sha256.to_string()
            && record.request.key.namespace == input.expected_namespace,
        "native Stop/Health belongs to another owner"
    );
    process_has_exited(observed.process_id)?;
    // Exact original control root; never create a replacement namespace/registry/lease.
    let lease = Arc::new(ServiceLease::acquire(&input.control)?);
    process_has_exited(observed.process_id)?;
    let registry = ServiceRegistry::open_saved(&input.control)?;
    ensure!(
        airk_store::service::registry::retained_namespace(&input.control)?
            == input.expected_namespace
            && registry.service_explicitly_stopped()?
            && registry.pending_lifecycle()?.is_none(),
        "same owner is not completely stopped"
    );
    let complete = registry
        .lifecycle_by_id(record.id)?
        .context("native terminal Stop record absent")?;
    ensure!(
        serde_json::to_value(complete)? == serde_json::to_value(&record)?
            && registry.recorded_service_instance()? == Some(observed.instance),
        "actual native StopComplete differs"
    );
    ensure!(
        registry
            .retained_project(input.project.binding)?
            .registration
            == input.project,
        "retained native project registration differs"
    );
    let index = registry
        .catalog_index_for_binding(input.project.binding)?
        .context("actual native catalog backing absent")?;
    ensure!(
        index.id == input.expected_index && index.backing == input.project,
        "actual native index/backing differs"
    );
    Ok(lease)
}
fn verify_publication(kernel: &crate::Kernel, input: &Input) -> Result<()> {
    let read = kernel.current_reader()?;
    ensure!(
        crate::current::runtime::envelope(&read)? == input.expected_publication,
        "actual native publication differs"
    );
    if let Some(head) = &input.expected_head {
        ensure!(read.head() == head, "exact persisted head changed");
    }
    let model = read
        .collection_state()?
        .context("published collection state absent")?
        .configuration
        .model
        .context("published model absent")?;
    if let Some(expected) = &input.expected_model {
        ensure!(&model == expected, "published model changed");
    }
    let snapshot = kernel
        .load_optional()?
        .context("indexed source roster absent")?;
    let actual: BTreeMap<_, _> = snapshot
        .artifacts
        .iter()
        .map(|(path, a)| (path.clone(), (a.bytes as u64, a.content_hash.clone())))
        .collect();
    ensure!(
        actual == input.expected_indexed_sources,
        "independent frozen indexed source roster differs"
    );
    Ok(())
}
fn require_selected_profile(input: &Input) -> Result<()> {
    let expected = input
        .expected_model
        .as_ref()
        .context("typed model pin missing; run zero-NN preparation first")?;
    expected.validate()?;
    ensure!(
        airk_retrieval::model::inspect_object_plan(&input.model_directory)?.local_set()
            == expected.local_set(),
        "exact model asset references differ"
    );
    let manifest: airk_retrieval::model::OnnxManifest =
        serde_json::from_slice(&std::fs::read(input.model_directory.join("model.json"))?)?;
    let catalog = airk_retrieval::model_catalog::inspect_catalog()?;
    let profile = catalog
        .catalog
        .profiles
        .iter()
        .find(|p| p.id == input.expected_profile)
        .context("exact native catalog profile absent")?;
    ensure!(
        manifest == profile.manifest && input.expected_profile == catalog.selection.profile_id,
        "exact host-selected U8/S8 publication required; no profile relabeling"
    );
    ensure!(
        airk_retrieval::model::inspect_cache_descriptor(&input.model_directory)?.identity()?
            == expected.pipeline_digest,
        "native query recipe/pipeline differs"
    );
    Ok(())
}
fn dense(query: &[f32], live: &[(u32, Vec<f32>)]) -> Result<Vec<(u32, f64)>> {
    ensure!(
        !query.is_empty() && query.iter().all(|v| v.is_finite()),
        "finite nonempty query required"
    );
    let mut ids = BTreeSet::new();
    let mut values = Vec::with_capacity(live.len());
    for (id, vector) in live {
        ensure!(
            *id != 0
                && ids.insert(*id)
                && vector.len() == query.len()
                && vector.iter().all(|v| v.is_finite()),
            "invalid live distance census"
        );
        // Independent f64 scalar L2 over the actual f32 bits, no ANN/exact-provider helper.
        let distance = query
            .iter()
            .zip(vector)
            .map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2))
            .sum::<f64>();
        values.push((*id, distance));
    }
    values.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    Ok(values)
}
fn vector_bits_hash(live: &[(u32, Vec<f32>)]) -> Sha256Digest {
    let mut hash = Sha256::new();
    for (id, vector) in live {
        hash.update(id.to_be_bytes());
        hash.update((vector.len() as u64).to_be_bytes());
        for value in vector {
            hash.update(value.to_bits().to_be_bytes());
        }
    }
    Sha256Digest::from_bytes(hash.finalize().into())
}
fn event(file: &mut File, value: Value) -> Result<()> {
    serde_json::to_writer(&mut *file, &value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
fn geometry(input: &Input, output: &Path) -> Result<()> {
    let base = native_parent_bag::private_base(Some(input.private_base.as_os_str()))?;
    ensure!(
        base == input.private_base,
        "canonical protected base required"
    );
    for path in [
        &input.control,
        Path::new(&input.project.root),
        Path::new(&input.project.data_root),
        &input.model_directory,
    ] {
        unaliased(path, true)?;
    }
    ensure!(
        input.control.starts_with(&base) && Path::new(&input.project.data_root).starts_with(&base),
        "actual fixture state/control must remain below approved private base"
    );
    input.project.validate()?;
    let frozen = [
        Path::new(&input.project.root),
        Path::new(&input.project.data_root),
        &input.control,
        &input.model_directory,
    ];
    for (i, a) in frozen.iter().enumerate() {
        for b in frozen.iter().skip(i + 1) {
            ensure!(
                !a.starts_with(b) && !b.starts_with(a),
                "source/data/control/model overlap refused"
            );
        }
    }
    let parent = output
        .parent()
        .context("fresh private output parent absent")?;
    unaliased(parent, true)?;
    ensure!(
        output.is_absolute()
            && output.starts_with(&base)
            && !output.exists()
            && !output
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir)),
        "fresh private output required"
    );
    for path in frozen {
        ensure!(
            !output.starts_with(path) && !path.starts_with(parent),
            "output overlaps frozen owner/input"
        );
    }
    let keys: BTreeSet<_> = input.identities.keys().map(String::as_str).collect();
    ensure!(
        keys == BTreeSet::from([
            "source_dirty_manifest",
            "runtime_metadata",
            "runtime_image",
            "cli_image",
            "fresh_index_receipt",
            "query_recipe",
            "software_v2_policy",
            "read_budget_fixture"
        ]),
        "complete independently frozen identity input set required"
    );
    Ok(())
}
fn run(preparation: bool) -> Result<()> {
    let began = Instant::now();
    let deadline = began + Duration::from_millis(airk_core::arbiter::Budget::default().deadline_ms);
    let path = PathBuf::from(
        std::env::var_os("AIRK_ANN_QUERY_INPUT").context("missing reviewed typed input")?,
    );
    let pin: Sha256Digest = std::env::var("AIRK_ANN_QUERY_INPUT_SHA256")?.parse()?;
    ensure!(
        std::fs::metadata(&path)?.len() <= 64 << 20,
        "private typed input exceeds fixed64MiB observation bound"
    );
    let bytes = sealed(&path, pin)?;
    let parsed: Value = serde_json::from_slice(&bytes)?;
    let typed = if parsed["event"] == "typed_input" {
        &parsed["input"]
    } else {
        &parsed
    };
    let mut input: Input = serde_json::from_value(typed.clone())?;
    let output = PathBuf::from(
        std::env::var_os("AIRK_ANN_QUERY_OUTPUT").context("missing fresh private output")?,
    );
    geometry(&input, &output)?;
    for identity in input.identities.values() {
        check_pin(identity, deadline)?;
    }
    let roster: Roster = serde_json::from_value(json_pin(&input.query_roster, deadline)?)?;
    validate_roster(&roster, input.expected_index)?;
    let capture = json_pin(&input.pre_stop_capture, deadline)?;
    let dirty = json_pin(&input.identities["source_dirty_manifest"], deadline)?;
    ensure!(
        capture["source"] == dirty && capture["source_after"] == dirty,
        "pre-stop source/dirty closure differs"
    );
    let modes_complete = validate_mode_capture(&input, &roster, &capture)?;
    let stop = json_pin(&input.stop_reply, deadline)?;
    let health = json_pin(&input.pre_stop_health, deadline)?;
    source_census(
        Path::new(&input.project.root),
        &input.expected_physical_sources,
        deadline,
    )?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    // Every original slot exists before index admission/NN; later errors retain the denominator.
    event(
        &mut file,
        json!({"event":"start","schema":"airk.private-fresh-query-ann/1","preparation":preparation,"input_sha256":pin,"input":input,"query_roster":roster,"pre_stop_capture":capture,"pre_stop_native_stop":stop,"pre_stop_native_health":health,"native_default_budget":airk_core::arbiter::Budget::default(),"native_capabilities":airk_retrieval::model_catalog::detect_capabilities(),"mode_actual_forwards":null,"mode_actual_forwards_status":"unavailable from existing canonical read APIs; never inferred from call count","schedule":roster.questions.iter().map(|q|json!({"id":q.id,"status":"not_run","disposition":q.disposition})).collect::<Vec<_>>()}),
    )?;
    let mut compared = 0_usize;
    let mut no_vector = 0_usize;
    let result = (|| -> Result<()> {
        let lease = admit_stopped(&input, &stop, &health)?;
        let kernel = crate::Kernel::open_existing_service_observer(&input.project)?;
        verify_publication(&kernel, &input)?;
        if preparation {
            ensure!(
                input.expected_head.is_none() && input.expected_model.is_none(),
                "preparation never replaces existing pins"
            );
            let read = kernel.current_reader()?;
            input.expected_head = Some(read.head().clone());
            input.expected_model = Some(
                read.collection_state()?
                    .context("collection state absent")?
                    .configuration
                    .model
                    .context("model absent")?,
            );
            drop(read);
            require_selected_profile(&input)?;
            event(
                &mut file,
                json!({"event":"typed_input","input":input,"embedding_actual_forwards":0}),
            )?;
        } else {
            let head = input
                .expected_head
                .as_ref()
                .context("typed head missing; run zero-NN preparation")?;
            require_selected_profile(&input)?;
            ensure!(
                head.graph_head.live_nodes <= 4096 && head.graph_head.allocated_count <= 4097,
                "existing diagnostic all-live/allocated bound exceeded; no subsampling"
            );
            let read = kernel.current_reader()?;
            let embeddings = airk_retrieval::vector::mutable::self_probe::regeneration_cache::authenticated_embeddings(read.clone(), 384, head.graph_head.allocated_count, head.graph_head.live_nodes, deadline)?;
            let live: Vec<_> = embeddings
                .iter()
                .map(|(id, entry)| (*id, entry.vector.clone()))
                .collect();
            let census = vector_bits_hash(&live);
            event(
                &mut file,
                json!({"event":"authenticated_census","head":head,"entries":embeddings,"vector_bits_sha256":census,"distance":"squared_l2_scalar_f64_from_f32_bits"}),
            )?;
            drop(read);
            ensure!(
                input
                    .expected_model
                    .as_ref()
                    .context("published model missing")?
                    .assets
                    .get(&airk_core::model_objects::ModelAssetName::ModelJson)
                    .is_some_and(
                        |model| capture["representative"]["verification_manifest_sha256"]
                            == json!(model.sha256)
                    ),
                "pre-stop modes and comparison use different model.json"
            );
            let owner = NativeInferenceOwner::admit(lease.clone())?;
            owner.run(|| -> Result<()> {
                let load = Instant::now();
                let model = EmbeddingModel::open(&input.model_directory)?;
                let descriptor = model.cache_descriptor()?;
                ensure!(model.dimensions() == 384 && descriptor.identity()? == input.expected_model.as_ref().context("model absent")?.pipeline_digest, "actual query embedding recipe differs");
                event(&mut file, json!({"event":"model_loaded","elapsed_ms":load.elapsed().as_secs_f64()*1000.0,"descriptor":descriptor,"runtime":airk_retrieval::model::ONNX_RUNTIME}))?;
                for (ordinal, question) in roster.questions.iter().enumerate() {
                    let start = Instant::now();
                    let query_deadline = deadline; // One original phase cutoff; no per-query renewal.
                    event(&mut file, json!({"event":"query_start","ordinal":ordinal,"id":question.id,"query":question.query,"remaining_ms":query_deadline.saturating_duration_since(start).as_millis()}))?;
                    let mut stats = EmbeddingCacheStats::default();
                    let native_inputs = model.evaluation_inputs(&question.query)?;
                    let embedding = model.embed_cached(&question.query, &descriptor, None, true, &mut stats);
                    event(&mut file, json!({"event":"query_embedding","ordinal":ordinal,"id":question.id,"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"execution":stats,"native_query_inputs":native_inputs,"vector_bits":embedding.as_ref().ok().and_then(|v|v.as_ref()).map(|v|v.iter().map(|x|x.to_bits()).collect::<Vec<_>>()),"error":embedding.as_ref().err().map(|e|format!("{e:#}"))}))?;
                    let Some(vector) = embedding? else { event(&mut file, json!({"event":"query_no_vector","ordinal":ordinal,"id":question.id,"status":"no_vector","denominator_retained":true}))?; no_vector += 1; continue; };
                    ensure!(stats.actual_forwards == 1 && stats.inputs == 1 && stats.hits == 0, "exactly one actual comparison forward per original query required");
                    ensure!(Instant::now() < query_deadline, "original query deadline expired during embedding");
                    let exact_start = Instant::now();
                    let exact = dense(&vector, &live)?;
                    let exact_ms = exact_start.elapsed().as_secs_f64()*1000.0;
                    event(&mut file, json!({"event":"exact_observed","ordinal":ordinal,"id":question.id,"all_live_distances":exact,"elapsed_ms":exact_ms,"original_remaining_ms":query_deadline.saturating_duration_since(Instant::now()).as_millis()}))?;
                    ensure!(Instant::now() < query_deadline, "original query deadline exceeded before ANN");
                    let read = kernel.current_reader()?;
                    let original_token = read.token()?;
                    ensure!(read.head() == head, "query graph head changed");
                    let graph = MutableDiskIndex::open(read.clone(), model.dimensions(), descriptor.identity()?)?;
                    let ann_start = Instant::now();
                    let ann_result = graph.search(&vector, 10);
                    event(&mut file, match &ann_result {
                        Ok(result) => json!({"event":"native_ann_observed","ordinal":ordinal,"id":question.id,"hits":result.hits,"distance_bits":result.hits.iter().map(|(id,d)|(*id,d.to_bits())).collect::<Vec<_>>(),"partial":result.partial,"record_reads":result.record_reads,"passes":result.passes}),
                        Err(error) => json!({"event":"native_ann_refused","ordinal":ordinal,"id":question.id,"error":format!("{error:#}")}),
                    })?;
                    let ann = ann_result?;
                    let ann_ms = ann_start.elapsed().as_secs_f64()*1000.0;
                    let top: BTreeSet<_> = exact.iter().take(10).map(|(id,_)|*id).collect();
                    let actual: BTreeSet<_> = ann.hits.iter().map(|(id,_)|*id).collect();
                    ensure!(ann.hits.iter().all(|(_,distance)|distance.is_finite()) && actual.len() == ann.hits.len() && actual.iter().all(|id|live.iter().any(|(live_id,_)|live_id==id)), "ANN returned duplicate/non-live evidence");
                    let cutoff = exact.get(top.len().saturating_sub(1)).map(|(_,d)|*d);
                    let limits = SearchLimits::default();
                    event(&mut file, json!({"event":"query_result","ordinal":ordinal,"id":question.id,"same_query_vector_bits_sha256":vector_bits_hash(&[(0,vector)]),"same_live_census_sha256":census,"exact_all_live":exact,"ann":{"hits":ann.hits,"partial":ann.partial,"record_reads":ann.record_reads,"passes":ann.passes,"limits":{"record_reads":limits.record_reads,"candidates":limits.candidates}},"recall_at10":{"numerator":top.intersection(&actual).count(),"denominator":top.len()},"exact_cutoff_ties":exact.iter().filter(|(_,d)|Some(*d)==cutoff).map(|(id,_)|*id).collect::<Vec<_>>(),"strictly_closer_missing":exact.iter().filter(|(id,d)|cutoff.is_some_and(|c|*d<c)&&!actual.contains(id)).map(|(id,_)|*id).collect::<Vec<_>>(),"timings_ms":{"exact_distance":exact_ms,"ann_search":ann_ms,"component_total":start.elapsed().as_secs_f64()*1000.0},"on_original_deadline":Instant::now()<query_deadline,"status":"observed","quality_acceptance":null}))?;
                    ensure!(Instant::now() < query_deadline, "original query deadline exceeded; result retained without success claim");
                    read.require_token(&original_token)?;
                    drop(graph); drop(read);
                    verify_publication(&kernel, &input)?;
                    compared += 1;
                }
                Ok(())
            })??;
        }
        verify_publication(&kernel, &input)?;
        source_census(
            Path::new(&input.project.root),
            &input.expected_physical_sources,
            deadline,
        )?;
        for identity in input.identities.values() {
            check_pin(identity, deadline)?;
        }
        json_pin(&input.query_roster, deadline)?;
        json_pin(&input.pre_stop_capture, deadline)?;
        json_pin(&input.stop_reply, deadline)?;
        json_pin(&input.pre_stop_health, deadline)?;
        require_selected_profile(&input)?;
        sealed(&path, pin)?;
        // The one original native lease remains held through all synchronous work/fences.
        drop(kernel);
        drop(lease);
        Ok(())
    })();
    event(
        &mut file,
        json!({"event":"finish","complete":result.is_ok() && (preparation || (modes_complete && compared == roster.questions.len())),"native_phase_completed":result.is_ok(),"compared":compared,"no_vector":no_vector,"comparison_not_run":roster.questions.len().saturating_sub(compared+no_vector),"preparation":preparation,"all_modes_captured":modes_complete,"original_task_denominator":roster.questions.len(),"original_mode_denominator":roster.questions.len()*3,"elapsed_seconds":began.elapsed().as_secs_f64(),"error":result.as_ref().err().map(|e|format!("{e:#}")),"latency_scope":"comparison components are after-stop same-image synchronous native observations; canonical mode/e2e latency remains in full pre-stop raw captures; no cold-cache claim","release_qualified":false,"human_quality_claim":false,"cleanup":"no Stop/Start/purge or synthetic closure; original owner remains stopped; launcher must retain actual child natural settlement"}),
    )?;
    result
}

#[test]
#[ignore = "fresh stopped host-selected native U8/S8 fixture only; zero NN; root-owned private input preparation"]
fn emit_fresh_stopped_ann_query_typed_pins() -> Result<()> {
    run(true)
}
#[test]
#[ignore = "fresh full original query roster only; exact host-selected U8/S8; original native120s phase within root-owned1800s/8GiB controller capacity"]
fn fresh_queries_compare_exact_all_live_to_diskann_with_same_embedding() -> Result<()> {
    run(false)
}

#[test]
fn ann_query_scalar_distance_and_ties_are_independent() -> Result<()> {
    let rows = dense(
        &[1.0, 0.0],
        &[
            (9, vec![0.0, 1.0]),
            (4, vec![1.0, 0.0]),
            (2, vec![0.0, -1.0]),
        ],
    )?;
    assert_eq!(rows, vec![(4, 0.0), (2, 2.0), (9, 2.0)]);
    assert!(dense(&[f32::NAN], &[]).is_err());
    assert!(dense(&[0.0], &[(1, vec![0.0]), (1, vec![0.0])]).is_err());
    assert!(dense(&[0.0], &[(0, vec![0.0])]).is_err());
    assert!(dense(&[0.0], &[(1, vec![0.0, 0.0])]).is_err());
    assert!(dense(&[0.0], &[(1, vec![f32::INFINITY])]).is_err());
    assert!(dense(&[0.0], &[])?.is_empty());
    Ok(())
}
#[test]
fn ann_query_whole_roster_refuses_duplicates_omitted_modes_and_scope_changes() -> Result<()> {
    let index: IndexId = IndexId::from_identity(Incarnation::from_bytes([1; 16]));
    let request = |mode: &str| Mode {
        mode: mode.into(),
        arguments: json!({"query":"literal original query","mode":mode,"limit":10,"max_bytes":65536,"response_mode":"full","index_ids":[index]}),
        capture_pointer: format!("/attempts/{mode}"),
    };
    let mut roster = Roster {
        purpose: "fresh-ungraded-step7".into(),
        questions: vec![Question {
            id: "literal-q1".into(),
            query: "literal original query".into(),
            disposition: json!({"alignment":"ungraded"}),
            modes: vec![request("lexical"), request("vector"), request("hybrid")],
        }],
    };
    validate_roster(&roster, index)?;
    roster.questions[0].modes[0].arguments["path_prefix"] = json!("selected/");
    assert!(validate_roster(&roster, index).is_err());
    roster.questions[0].modes[0]
        .arguments
        .as_object_mut()
        .unwrap()
        .remove("path_prefix");
    roster.questions[0].modes.pop();
    assert!(validate_roster(&roster, index).is_err());
    roster.questions[0].modes.push(request("hybrid"));
    let copy: Question = serde_json::from_value(serde_json::to_value(&roster.questions[0])?)?;
    roster.questions.push(copy);
    assert!(validate_roster(&roster, index).is_err());
    roster.questions.pop();
    roster.purpose = "consumed-final".into();
    assert!(validate_roster(&roster, index).is_err());
    Ok(())
}
#[test]
fn ann_query_source_census_refuses_changed_missing_and_extra_files() -> Result<()> {
    let source = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let root = source.path().canonicalize()?;
    let path = root.join("literal.txt");
    std::fs::write(&path, b"literal source")?;
    let expected = BTreeMap::from([(
        "literal.txt".into(),
        SourcePin {
            bytes: 14,
            sha256: Sha256Digest::from_bytes(Sha256::digest(b"literal source").into()),
        },
    )]);
    let deadline = Instant::now() + Duration::from_secs(5);
    source_census(&root, &expected, deadline)?;
    assert!(
        source_census(&path, &expected, deadline).is_err(),
        "a source file cannot become a root"
    );
    assert!(source_census(Path::new("relative-source"), &expected, deadline).is_err());
    std::fs::write(&path, b"changed source")?;
    assert!(source_census(&root, &expected, deadline).is_err());
    std::fs::write(&path, b"literal source")?;
    std::fs::write(root.join("extra.txt"), b"extra")?;
    assert!(source_census(&root, &expected, deadline).is_err());
    std::fs::remove_file(root.join("extra.txt"))?;
    std::fs::remove_file(path)?;
    assert!(source_census(&root, &expected, deadline).is_err());
    Ok(())
}
