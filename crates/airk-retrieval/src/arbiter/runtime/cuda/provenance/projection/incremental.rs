// SPDX-License-Identifier: Apache-2.0
//! Exact incremental metadata derivation. Never an execution or qualification permit.
use super::*;
use checkpoint::CheckpointedProducer;

const PARENT: &str = "f9fac8eb2fe1a1cb5191118124503782792ca6d3aff14d02326167cd7276b178";
const AGGREGATE: &str = "042c970193bf3d2fc7c80f53b055f74130c653d561f7735142fc01b01cebac00";
// Replaced only after the producer owner's complete recipe/source freeze.
const REVIEWED_RECIPE: &str = "__REVIEWED_RECIPE_SHA256__";
const SID: &str = "S-1-5-21-3678370870-3858642417-826968105-1002";
const EXPORTS: [&str; 2] = [
    "airk_ort_collect_final_graph_metadata_v1",
    "airk_ort_collect_final_placement_v1",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pin {
    path: String,
    bytes: u64,
    sha256: Sha256Digest,
}
impl Pin {
    fn resource(&self) -> Result<ResourceFilePin> {
        let pin = ResourceFilePin {
            path: self.path.clone(),
            sha256: self.sha256,
        };
        pin.validate()?;
        ensure!(
            self.bytes <= 2u64 << 30 && nonzero(self.sha256),
            "incremental pin capacity"
        );
        Ok(pin)
    }
    fn authenticate(&self, source: &dyn Source) -> Result<()> {
        source.authenticate(&self.resource()?, self.bytes, 2u64 << 30)
    }
    fn metadata(&self, source: &dyn Source) -> Result<Value> {
        ensure!(self.bytes <= 16 << 20, "incremental metadata capacity");
        let bytes = read(source, &self.resource()?, 16 << 20)?;
        ensure!(
            bytes.len() as u64 == self.bytes,
            "incremental metadata length differs"
        );
        strict::parse(&bytes)
    }
}
fn pin(v: &Value) -> Result<Pin> {
    let p: Pin = serde_json::from_value(v.clone())?;
    p.resource()?;
    ensure!(p.bytes > 0, "incremental non-log pin is empty");
    Ok(p)
}
fn exact_keys(v: &Value, expected: &[&str]) -> Result<()> {
    let actual: BTreeSet<_> = v
        .as_object()
        .context("incremental object missing")?
        .keys()
        .map(String::as_str)
        .collect();
    ensure!(
        actual == expected.iter().copied().collect(),
        "incremental field roster differs"
    );
    Ok(())
}
fn finite(v: &Value) -> Result<f64> {
    let n = v.as_f64().context("incremental clock missing")?;
    ensure!(n.is_finite() && n >= 0.0, "invalid incremental clock");
    Ok(n)
}
fn utc(v: &Value) -> Result<i128> {
    checkpoint::utc(v.as_str().context("incremental UTC missing")?)
}
fn limits(v: &Value) -> Result<()> {
    ensure!(
        *v == serde_json::json!({
            "session_wall_seconds":60,"whole_wall_seconds":600,"log_bytes":67108864u64,
            "aggregate_wall_seconds":300,"compile_wall_seconds":120,
            "process_bytes":17179869184u64,"parallelism":1,"job_bytes":25769803776u64,
            "cleanup_seconds":5,"proof_wall_seconds":120
        }),
        "incremental original limits differ"
    );
    Ok(())
}
fn clocks(raw: &Value) -> Result<(f64, f64)> {
    let start = finite(&raw["started_monotonic"])?;
    let finish = finite(&raw["finished_monotonic"])?;
    let deadline = finite(&raw["original_deadline_monotonic"])?;
    let elapsed = finite(&raw["elapsed_seconds"])?;
    let utc_start = utc(&raw["started_utc"])?;
    let utc_finish = utc(&raw["finished_utc"])?;
    let utc_deadline = utc(&raw["original_deadline"])?;
    ensure!(
        deadline == start + 600.0
            && start <= finish
            && finish < deadline
            && elapsed == finish - start
            && elapsed < 600.0
            && utc_start <= utc_finish
            && utc_finish <= utc_deadline
            && utc_deadline == utc_start + 600_000_000_000,
        "incremental original deadline differs"
    );
    Ok((start, deadline))
}
fn exits(v: &Value, leader: u64) -> Result<()> {
    let rows = rows(v)?;
    ensure!(
        !rows.is_empty() && rows.len() <= 2,
        "incremental retained exits absent"
    );
    let mut ids = BTreeSet::new();
    for row in rows {
        let pid = row["pid"]
            .as_u64()
            .context("incremental retained PID missing")?;
        ensure!(
            pid > 0 && ids.insert(pid) && row["exit_observed"] == true && row["exit_code"] == 0,
            "incremental retained exit differs"
        );
    }
    ensure!(ids.contains(&leader), "incremental leader exit absent");
    Ok(())
}
fn owned(command: &Value, tools: &[Value]) -> Result<()> {
    let v = &command["owned"];
    for key in [
        "timed_out",
        "log_oversized",
        "owned_job_forced_termination",
        "parent_assigned_to_job",
        "unexpected_third_process",
    ] {
        ensure!(v[key] == false, "incremental process refusal: {key}");
    }
    for key in [
        "owned_descendants_settled",
        "child_assigned_before_resume",
        "whole_process_deadline_not_refreshed",
    ] {
        ensure!(v[key] == true, "incremental process proof absent: {key}");
    }
    ensure!(
        v["exit_code"] == 0
            && v["owned_job_final_active_processes"] == 0
            && v["owned_job_final_member_pids"] == serde_json::json!([])
            && v.get("cleanup_refusal").is_none(),
        "incremental natural settlement differs"
    );
    let leader = v["owned_leader_pid"]
        .as_u64()
        .context("incremental leader missing")?;
    ensure!(
        leader > 0
            && v["owned_total_processes"]
                .as_u64()
                .is_some_and(|n| (1..=2).contains(&n))
            && v["peak_host_committed_bytes"]
                .as_u64()
                .is_some_and(|n| (1..=25769803776).contains(&n)),
        "incremental process census/capacity differs"
    );
    exits(&v["retained_member_exits"], leader)?;
    let close = &v["handle_close_proof"];
    ensure!(
        close["forced"] == false
            && close["settled"] == true
            && close["final_active"] == 0
            && close["members_before"] == serde_json::json!([])
            && close["final_member_pids"] == serde_json::json!([]),
        "incremental HANDLE/Job closure differs"
    );
    exits(&close["retained_exits"], leader)?;
    let auth = &v["suspended_assignment"];
    let executable = command["argv"][0]
        .as_str()
        .context("incremental executable missing")?;
    let matches: Vec<_> = tools
        .iter()
        .filter(|p| p["path"].as_str() == Some(executable))
        .collect();
    ensure!(
        matches.len() == 1,
        "incremental executable outside sealed tool roster"
    );
    let executable_pin = pin(matches[0])?;
    ensure!(
        auth["pid"].as_u64() == Some(leader)
            && auth["execution_user_sid"] == SID
            && auth["default_owner_sid"] == SID
            && auth["image"]
                == serde_json::json!({"path":executable_pin.path,"sha256":executable_pin.sha256})
            && auth["component_job"]
                == serde_json::json!({"process_bytes":17179869184u64,
            "job_bytes":25769803776u64,"flags":8968,"active_limit":2}),
        "incremental suspended image/token/Job differs"
    );
    Ok(())
}
fn replaced(before: &[Value], after: &[Value], old: &Pin, fresh: &Pin, count: usize) -> Result<()> {
    ensure!(
        before.len() == count && after.len() == count && old.sha256 != fresh.sha256,
        "incremental replacement count differs"
    );
    let mut replacements = 0;
    let mut paths = BTreeSet::new();
    for (a, b) in before.iter().zip(after) {
        let a = pin(a)?;
        let b = pin(b)?;
        ensure!(
            paths.insert(b.path.to_ascii_lowercase()),
            "incremental duplicate input"
        );
        if a == *old {
            replacements += 1;
            ensure!(b == *fresh, "incremental replacement artifact differs");
        } else {
            ensure!(a == b, "incremental retained input changed");
        }
    }
    ensure!(
        replacements == 1,
        "incremental must replace exactly one member"
    );
    Ok(())
}
fn bodies(v: &Value) -> Result<BTreeMap<(Sha256Digest, u64), u64>> {
    ensure!(
        v["kind"] == "static"
            && v["import_members"] == 0
            && v["imported_dlls"] == serde_json::json!([]),
        "incremental import archive refused"
    );
    let mut map = BTreeMap::new();
    for row in rows(&v["object_bodies"])? {
        exact_keys(row, &["sha256", "bytes", "count"])?;
        let key = (sha(&row["sha256"])?, size(&row["bytes"])?);
        let count = size(&row["count"])?;
        ensure!(
            count <= 200000 && map.insert(key, count).is_none(),
            "incremental duplicate object body"
        );
    }
    ensure!(
        !map.is_empty() && map.len() <= 200000,
        "incremental object census capacity"
    );
    let count: u64 = map.values().sum();
    ensure!(
        v["members"].as_u64() == Some(count) && v["static_members"].as_u64() == Some(count),
        "incremental member count differs"
    );
    Ok(map)
}
fn body_closure(actual: &Value, expected: &BTreeMap<(Sha256Digest, u64), u64>) -> Result<()> {
    let actual = bodies(actual)?;
    ensure!(
        actual.len() == expected.len()
            && actual
                .iter()
                .all(|(key, n)| { expected.get(key).is_some_and(|max| *n >= 1 && n <= max) }),
        "incremental COFF object closure differs"
    );
    Ok(())
}
pub(in crate::arbiter::runtime::cuda) struct IncrementalProducer {
    pub static_core_build: Sha256Digest,
    pub provider_dlls: Vec<PublicFile>,
    pub(super) parent: CheckpointedProducer,
    pub(super) history: IncrementalHistory,
    pub(super) producer: Value,
    pub(super) build: Value,
    pub(super) recipe: Value,
}
pub(in crate::arbiter::runtime::cuda) fn validate_incremental_producer(
    source: &dyn Source,
    raw_pin: &ResourceFilePin,
    proof_pin: &ResourceFilePin,
) -> Result<IncrementalProducer> {
    validate_with_recipe(source, raw_pin, proof_pin, REVIEWED_RECIPE.parse()?)
}
fn validate_with_recipe(
    source: &dyn Source,
    raw_pin: &ResourceFilePin,
    proof_pin: &ResourceFilePin,
    approved_recipe: Sha256Digest,
) -> Result<IncrementalProducer> {
    let raw = json(source, raw_pin)?;
    let proof = json(source, proof_pin)?;
    exact_keys(&raw, RAW_FIELDS)?;
    exact_keys(&proof, PROOF_FIELDS)?;
    canonical_sha(&proof)?;
    ensure!(
        raw["schema"] == "airk.private.cuda-static-incremental-receipt/1"
            && raw["status"] == "PASS_INCREMENTAL_PRODUCER_ONLY_UNQUALIFIED"
            && raw["mode"] == "incremental-finalgraph"
            && proof["schema"] == "airk.cuda-static-build-proof/3"
            && proof["target"] == "x86_64-pc-windows-msvc"
            && proof["ort_version"] == "1.30.0",
        "unknown incremental producer/proof"
    );
    for value in [&raw, &proof] {
        for key in [
            "qualification",
            "activation_pins_available",
            "cold_build_qualified",
        ] {
            ensure!(value[key] == false, "incremental authority claim refused");
        }
    }
    ensure!(
        raw["failure"].is_null() && raw["input_recheck_errors"] == serde_json::json!([]),
        "incremental producer retained failure"
    );
    let build_receipt = pin(&proof["build_receipt"])?;
    ensure!(
        build_receipt.resource()? == *raw_pin,
        "incremental raw receipt identity differs"
    );
    build_receipt.metadata(source)?;
    for key in [
        "producer_source",
        "parent_producer",
        "parent_build_proof",
        "parent_aggregate",
        "recipe",
        "source_patch",
        "new_source",
        "new_object",
        "new_session",
        "new_aggregate",
        "session_input_pins",
        "aggregate_input_pins",
        "actual_include_pins",
    ] {
        ensure!(
            proof[key] == raw[key],
            "incremental canonical/raw field differs: {key}"
        );
    }
    let recipe_pin = pin(&proof["recipe"])?;
    ensure!(
        recipe_pin.sha256 == approved_recipe,
        "incremental unreviewed recipe"
    );
    let recipe = recipe_pin.metadata(source)?;
    ensure!(
        recipe["producer_source"] == proof["producer_source"]
            && recipe["parent_build_proof"] == proof["parent_build_proof"],
        "incremental producer/helper authority differs"
    );
    pin(&proof["producer_source"])?.authenticate(source)?;
    for key in ["checked_guard", "powershell"] {
        pin(&recipe[key])?.authenticate(source)?;
    }
    let helpers = rows(&recipe["helper_pins"])?;
    ensure!(
        !helpers.is_empty() && helpers.len() <= 64,
        "incremental helper closure capacity"
    );
    let mut helper_paths = BTreeSet::new();
    for helper in helpers {
        let p = pin(helper)?;
        ensure!(
            helper_paths.insert(p.path.to_ascii_lowercase()),
            "incremental duplicate helper"
        );
        p.authenticate(source)?;
    }
    let parent_raw = pin(&proof["parent_producer"])?;
    let parent_proof = pin(&proof["parent_build_proof"])?;
    let parent_aggregate = pin(&proof["parent_aggregate"])?;
    ensure!(
        parent_raw.sha256.to_string() == PARENT
            && parent_aggregate.sha256.to_string() == AGGREGATE
            && raw_pin.sha256 != parent_raw.sha256
            && recipe["old_producer"] == proof["parent_producer"]
            && recipe["old_aggregate"] == proof["parent_aggregate"],
        "incremental parent identity differs"
    );
    let parent = checkpoint::validate_checkpointed_producer(
        source,
        &parent_raw.resource()?,
        &parent_proof.resource()?,
    )?;
    parent_aggregate.authenticate(source)?;
    let source_before = pin(&proof["source_patch"]["base"])?;
    let source_after = pin(&proof["source_patch"]["after"])?;
    let compiled_source = pin(&proof["new_source"])?;
    let patch = pin(&proof["source_patch"]["patch"])?;
    exact_keys(&proof["source_patch"], &["base", "after", "patch"])?;
    ensure!(
        source_before.sha256 != source_after.sha256
            && recipe["base_source"] == proof["source_patch"]["base"]
            && recipe["source_patch"] == proof["source_patch"]
            && compiled_source.bytes == source_after.bytes
            && compiled_source.sha256 == source_after.sha256
            && recipe["new_source_sha256"] == proof["new_source"]["sha256"]
            && recipe["new_source_bytes"] == proof["new_source"]["bytes"],
        "incremental source derivation differs"
    );
    for p in [&source_before, &source_after, &compiled_source, &patch] {
        p.authenticate(source)?;
    }
    let object = pin(&proof["new_object"])?;
    let session = pin(&proof["new_session"])?;
    let aggregate = pin(&proof["new_aggregate"])?;
    let output_root = std::path::Path::new(
        recipe["fresh_output"]
            .as_str()
            .context("incremental output root missing")?,
    );
    for (p, relative) in [
        (&compiled_source, "source/onnxruntime_c_api.cc"),
        (&object, "objects/onnxruntime_c_api.cc.obj"),
        (&session, "session/onnxruntime_session.lib"),
        (&aggregate, "aggregate/onnxruntime.lib"),
    ] {
        ensure!(
            std::path::Path::new(&p.path) == output_root.join(relative),
            "incremental artifact outside exact produced path"
        );
    }
    ensure!(
        aggregate.sha256 != parent_aggregate.sha256,
        "incremental reused parent archive"
    );
    for p in [&object, &session, &aggregate] {
        p.authenticate(source)?;
    }
    let old_objects = rows(&recipe["session_objects"])?;
    let old_object_rows: Vec<_> = old_objects
        .iter()
        .filter(|v| {
            v["path"]
                .as_str()
                .is_some_and(|p| p.ends_with("onnxruntime_c_api.cc.obj"))
        })
        .collect();
    ensure!(
        old_object_rows.len() == 1,
        "incremental old translation unit object ambiguous"
    );
    let old_object = pin(old_object_rows[0])?;
    replaced(
        old_objects,
        rows(&proof["session_input_pins"])?,
        &old_object,
        &object,
        40,
    )?;
    let old_libraries = rows(&recipe["aggregate_libraries"])?;
    let old_sessions: Vec<_> = old_libraries
        .iter()
        .filter(|v| {
            v["path"]
                .as_str()
                .is_some_and(|p| p.ends_with("onnxruntime_session.lib"))
        })
        .collect();
    ensure!(old_sessions.len() == 1, "incremental old session ambiguous");
    replaced(
        old_libraries,
        rows(&proof["aggregate_input_pins"])?,
        &pin(old_sessions[0])?,
        &session,
        96,
    )?;
    for key in [
        "session_input_pins",
        "aggregate_input_pins",
        "actual_include_pins",
    ] {
        for value in rows(&proof[key])? {
            pin(value)?.authenticate(source)?;
        }
    }
    ensure!(
        proof["compile_dependencies"] == recipe["compiler_dependencies"]
            && rows(&proof["compile_dependencies"])?.len() == 482,
        "incremental dependency roster differs"
    );
    let mut expected_includes = BTreeMap::new();
    for v in rows(&proof["compile_dependencies"])?
        .iter()
        .chain(rows(&recipe["approved_new_dependencies"])?)
    {
        let p = pin(v)?;
        p.authenticate(source)?;
        if p != source_before {
            ensure!(
                expected_includes
                    .insert(p.path.to_ascii_lowercase(), p)
                    .is_none(),
                "incremental duplicate dependency"
            );
        }
    }
    let actual_includes: BTreeMap<_, _> = rows(&proof["actual_include_pins"])?
        .iter()
        .map(|v| {
            let p = pin(v)?;
            Ok((p.path.to_ascii_lowercase(), p))
        })
        .collect::<Result<_>>()?;
    ensure!(
        actual_includes.len() == rows(&proof["actual_include_pins"])?.len()
            && actual_includes == expected_includes,
        "incremental actual include closure differs"
    );
    for key in [
        "source_inventory_sha256",
        "frontend_inventory_sha256",
        "cuda_inventory_sha256",
        "frozen_input_closure_sha256",
        "ninja",
        "toolchain_pins",
        "provider_dlls",
    ] {
        ensure!(
            proof[key] == parent.build[key],
            "incremental inherited ancestry differs: {key}"
        );
    }
    let mut expected_libraries = parent.build["libraries"].clone();
    let mut replaced_library = 0;
    for row in expected_libraries
        .as_array_mut()
        .context("parent libraries absent")?
    {
        if row["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("/onnxruntime_session.lib"))
        {
            row["bytes"] = session.bytes.into();
            row["sha256"] = serde_json::to_value(session.sha256)?;
            replaced_library += 1;
        }
    }
    ensure!(
        replaced_library == 1
            && proof["libraries"] == expected_libraries
            && rows(&proof["libraries"])?.len() == 96,
        "incremental library ancestry differs"
    );
    let identity = &raw["identity"];
    ensure!(
        raw["execution_user_sid"] == SID
            && raw["default_owner_sid"] == SID
            && identity["UserSid"] == SID
            && identity["OwnerSid"] == SID
            && identity["SessionId"].as_u64().is_some_and(|n| n > 0)
            && rows(&identity["InteractiveOwnerSids"])?
                .iter()
                .any(|v| v == SID),
        "incremental execution identity differs"
    );
    ensure!(
        raw["timing_scope"]
            == "Original 600s covers preflight, commands, input rechecks, proof hashing and output audit; final receipt retention guard, encoding/write and postwrite owner audit are excluded.",
        "incremental timing scope differs"
    );
    limits(&raw["limits"])?;
    ensure!(
        raw["limits"] == recipe["proposed_limits"],
        "incremental recipe limits differ"
    );
    let (start, deadline) = clocks(&raw)?;
    let commands = rows(&raw["commands"])?;
    ensure!(commands.len() == 5, "incremental command roster differs");
    let tools = rows(&recipe["compiler_tool_files"])?;
    for tool in tools {
        pin(tool)?.authenticate(source)?;
    }
    let mut previous = start;
    for (index, (stage, cap)) in [
        ("compile", 120),
        ("session", 60),
        ("aggregate", 300),
        ("object-symbols", 120),
        ("aggregate-directives", 120),
    ]
    .into_iter()
    .enumerate()
    {
        let c = &commands[index];
        exact_keys(c, COMMAND_FIELDS)?;
        let begin = finite(&c["started_monotonic"])?;
        let end = finite(&c["finished_monotonic"])?;
        let cutoff = finite(&c["absolute_deadline_monotonic"])?;
        ensure!(
            c["stage"] == stage
                && c["phase_limit_seconds"] == cap
                && previous <= begin
                && begin <= end
                && end < cutoff
                && cutoff == deadline.min(begin + cap as f64)
                && utc(&raw["started_utc"])? <= utc(&c["started_utc"])?
                && utc(&c["started_utc"])? <= utc(&c["finished_utc"])?
                && utc(&c["finished_utc"])? <= utc(&raw["finished_utc"])?
                && c["argv"] == recipe["commands"][stage.replace('-', "_")]
                && c["cwd"] == recipe["fresh_output"],
            "incremental command/deadline differs"
        );
        owned(c, tools)?;
        let log: Pin = serde_json::from_value(c["stdout"].clone())?;
        ensure!(
            log.bytes <= 64 << 20
                && std::path::Path::new(&log.path)
                    == output_root.join("logs").join(format!("{stage}.log")),
            "incremental log capacity/path"
        );
        log.authenticate(source)?;
        previous = end;
    }
    ensure!(
        previous <= finite(&raw["finished_monotonic"])?,
        "incremental final clock precedes child"
    );
    let owners = raw["output_owner_audit"]
        .as_object()
        .context("incremental output audit absent")?;
    ensure!(
        !owners.is_empty()
            && owners.contains_key(
                recipe["fresh_output"]
                    .as_str()
                    .context("incremental output root missing")?
            )
            && owners.values().all(Value::is_null)
            && [&compiled_source, &object, &session, &aggregate]
                .iter()
                .all(|p| owners.contains_key(&p.path)),
        "incremental output custody incomplete"
    );
    exact_keys(
        &raw["archive_proof"],
        &["session", "aggregate", "required_exports"],
    )?;
    let declared: BTreeSet<_> = rows(&raw["archive_proof"]["required_exports"])?
        .iter()
        .map(|v| v.as_str().context("export not text"))
        .collect::<Result<_>>()?;
    ensure!(
        rows(&raw["archive_proof"]["required_exports"])?.len() == 2
            && declared == EXPORTS.into_iter().collect(),
        "incremental collector export roster differs"
    );
    let mut expected_session = BTreeMap::new();
    for v in rows(&proof["session_input_pins"])? {
        let p = pin(v)?;
        *expected_session.entry((p.sha256, p.bytes)).or_insert(0) += 1;
    }
    body_closure(&raw["archive_proof"]["session"], &expected_session)?;
    let parent_archive_receipt = pin(&recipe["old_aggregate_receipt"])?.metadata(source)?;
    ensure!(
        parent_archive_receipt["output"]["sha256"] == AGGREGATE,
        "incremental parent aggregate receipt differs"
    );
    let mut aggregate_bodies = bodies(&parent_archive_receipt["coff"])?;
    let old_count = aggregate_bodies
        .remove(&(old_object.sha256, old_object.bytes))
        .context("original aggregate lacks replaced object")?;
    ensure!(
        old_count == 1 && !aggregate_bodies.contains_key(&(object.sha256, object.bytes)),
        "incremental replaced object must be unique"
    );
    aggregate_bodies.insert((object.sha256, object.bytes), old_count);
    body_closure(&raw["archive_proof"]["aggregate"], &aggregate_bodies)?;
    for name in ["session", "aggregate"] {
        let exports: BTreeSet<_> = rows(&raw["archive_proof"][name]["export_directives"])?
            .iter()
            .filter_map(Value::as_str)
            .collect();
        ensure!(
            EXPORTS.iter().all(|e| exports.contains(e)),
            "incremental collector export absent"
        );
    }
    let history = IncrementalHistory {
        mode: "incremental-finalgraph".into(),
        parent_producer_sha256: parent_raw.sha256,
        parent_build_proof_sha256: parent_proof.sha256,
        parent_aggregate_sha256: parent_aggregate.sha256,
        recipe_sha256: recipe_pin.sha256,
        source_before_sha256: source_before.sha256,
        source_after_sha256: source_after.sha256,
        patch_sha256: patch.sha256,
        new_object_sha256: object.sha256,
        new_session_sha256: session.sha256,
        aggregate_sha256: aggregate.sha256,
        qualification: false,
        cold_build_qualified: false,
    };
    source.recheck()?;
    Ok(IncrementalProducer {
        static_core_build: raw_pin.sha256,
        provider_dlls: parent.provider_dlls.clone(),
        parent,
        history,
        producer: raw,
        build: proof,
        recipe,
    })
}
#[cfg(test)]
mod tests;

const RAW_FIELDS: &[&str] = &[
    "schema",
    "status",
    "mode",
    "recipe",
    "producer_source",
    "parent_producer",
    "parent_build_proof",
    "parent_aggregate",
    "source_patch",
    "new_source",
    "new_object",
    "new_session",
    "new_aggregate",
    "session_input_pins",
    "aggregate_input_pins",
    "actual_include_pins",
    "commands",
    "started_utc",
    "finished_utc",
    "started_monotonic",
    "finished_monotonic",
    "original_deadline",
    "original_deadline_monotonic",
    "elapsed_seconds",
    "execution_user_sid",
    "default_owner_sid",
    "identity",
    "limits",
    "input_recheck_errors",
    "output_owner_audit",
    "archive_proof",
    "qualification",
    "activation_pins_available",
    "cold_build_qualified",
    "failure",
    "timing_scope",
];

const PROOF_FIELDS: &[&str] = &[
    "schema",
    "target",
    "ort_version",
    "build_receipt",
    "producer_source",
    "parent_producer",
    "parent_build_proof",
    "parent_aggregate",
    "recipe",
    "source_patch",
    "new_source",
    "new_object",
    "new_session",
    "new_aggregate",
    "session_input_pins",
    "aggregate_input_pins",
    "compile_dependencies",
    "actual_include_pins",
    "provider_dlls",
    "libraries",
    "toolchain_pins",
    "source_inventory_sha256",
    "frontend_inventory_sha256",
    "cuda_inventory_sha256",
    "frozen_input_closure_sha256",
    "ninja",
    "qualification",
    "activation_pins_available",
    "cold_build_qualified",
];

const COMMAND_FIELDS: &[&str] = &[
    "stage",
    "argv",
    "cwd",
    "started_utc",
    "finished_utc",
    "started_monotonic",
    "finished_monotonic",
    "absolute_deadline_monotonic",
    "phase_limit_seconds",
    "owned",
    "stdout",
];
