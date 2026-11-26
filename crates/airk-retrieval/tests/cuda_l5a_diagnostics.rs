// SPDX-License-Identifier: Apache-2.0
//! Plan 12.5.2 L5 phase A: CUDA diagnostics and measurements only.
//!
//! Ignored selectors driven by explicit environment inputs. Every input file
//! is SHA-pinned, every output is a fresh file or directory, and every
//! receipt states what ran. Nothing here qualifies a model, emits a release
//! profile or creates execution authority beyond the existing unqualified
//! evaluation probe binding.
//!
//! ```text
//! l5a_prepare        CPU only. Emits FP32 (Focus 1024) or FP16 (G5) probe metadata.
//!   L5A_PRECISION        fp32 | fp16
//!   L5A_CPU_PROFILE(_SHA256), L5A_ASSET_MAP(_SHA256)   CPU source profile and name->path map
//!   L5A_RUNTIME_MANIFEST(_SHA256)                      scientific CUDA runtime descriptor
//!   L5A_FP16_EXPORT, L5A_ORIGINAL_GRAPH                fp16 only: export dir and FP32 lineage
//!   L5A_FP16_MODEL, L5A_FP16_WEIGHTS                   fp16 only, optional: per-file overrides
//!   L5A_PLAN             cold,warm,headroom bytes
//!   L5A_OUTPUT           fresh directory
//! l5a_gpu            GPU. Opens one probe profile in the scorer path.
//!   L5A_STEP             placement (FP32) | graph (FP16 G5) | strict
//!   L5A_PROFILE(_SHA256), L5A_ASSET_MAP(_SHA256), L5A_PLAN, L5A_OUTPUT (fresh file)
//!   protected prepare: L5A_MEASUREMENT (JSON), L5A_MEASUREMENT_RECEIPT, L5A_POLICY (pass 2),
//!     L5A_LOOP_TASKS=1 (both passes) declares the four Answer-loop card tasks with a
//!     zero-rule calibration set instead of the one-turn task
//! l5a_replay: L5A_GRAPH_RESULT(_SHA256) for an FP32 base, or L5A_REVIEW_RECEIPT(_SHA256)
//!   (the pass-1 graph diagnostic receipt) for a protected route; L5A_PANEL(_SHA256)
//!   L5A_CONTROL_PARENT   existing directory for the private lease root
//!   L5A_WARM             warm samples per width (default 10), L5A_DEADLINE_S (default 600)
//! l5a_cutlass        GPU. Raw scientific session timing for the protected graphs.
//!   L5A_MODEL compact, L5A_GATE typed-graph digest, L5A_CAMPAIGN, L5A_OUTPUT,
//!   L5A_CONTROL_PARENT, L5A_WARM (default 5), L5A_DEADLINE_S (default 600)
//! ```
#![cfg(all(
    feature = "gpu-cuda",
    feature = "test-support",
    windows,
    target_arch = "x86_64"
))]
use airk_core::{
    arbiter::{Asset, AssetRole, ModelChoice, Profile},
    identity::Sha256Digest,
};
use airk_retrieval::arbiter::{
    compiler::{Candidate, CompiledInput, Compiler},
    runtime::{
        AssetSource, EvaluationCatalog, Scorer, admit_loop_tasks,
        cuda::{self, CudaRuntime, DeviceCapacityPlan},
    },
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

fn env(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("{name} missing"))
}

fn sha(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

fn file_sha(path: &Path) -> Result<(u64, Sha256Digest)> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 4 << 20];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        hash.update(&buffer[..n]);
    }
    Ok((total, Sha256Digest::from_bytes(hash.finalize().into())))
}

/// Read a file only when its bytes equal the expected SHA-256.
fn pinned(path_var: &str) -> Result<(PathBuf, Vec<u8>, Sha256Digest)> {
    let path = PathBuf::from(env(path_var)?);
    let expected: Sha256Digest = env(&format!("{path_var}_SHA256"))?.parse()?;
    let bytes = std::fs::read(&path)?;
    ensure!(sha(&bytes) == expected, "{path_var} bytes differ from pin");
    Ok((path, bytes, expected))
}

fn plan() -> Result<DeviceCapacityPlan> {
    let values = env("L5A_PLAN")?
        .split(',')
        .map(|v| v.trim().parse::<u64>())
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(values.len() == 3, "L5A_PLAN is cold,warm,headroom");
    DeviceCapacityPlan::probe(values[0], values[1], values[2])
}

fn write_fresh(path: &Path, value: &Value) -> Result<Sha256Digest> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(sha(&bytes))
}

/// Replace a receipt in place; used for progressive flushes of one owned file.
fn flush(path: &Path, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Name -> path asset map. Every read is length- and SHA-checked by the scorer
/// and again here, so a path claim never substitutes for the bytes.
struct Files(BTreeMap<String, PathBuf>);
impl AssetSource for Files {
    fn read_asset(&self, asset: &Asset) -> Result<Vec<u8>> {
        let path = self
            .0
            .get(&asset.name)
            .with_context(|| format!("unmapped asset {}", asset.name))?;
        let bytes = std::fs::read(path)?;
        ensure!(
            bytes.len() as u64 == asset.bytes && sha(&bytes) == asset.sha256,
            "asset {} bytes differ",
            asset.name
        );
        Ok(bytes)
    }
}
fn files(raw: &[u8]) -> Result<Files> {
    let map: BTreeMap<String, String> = serde_json::from_slice(raw)?;
    Ok(Files(
        map.into_iter()
            .map(|(k, v)| (k, PathBuf::from(v.trim_start_matches(r"\\?\"))))
            .collect(),
    ))
}

#[derive(serde::Serialize, Default)]
struct HostMemory {
    peak_working_set_bytes: u64,
    working_set_bytes: u64,
    peak_pagefile_bytes: u64,
    pagefile_bytes: u64,
}
fn host_memory() -> HostMemory {
    use windows_sys::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };
    let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    // SAFETY: read-only query of this process into an owned, sized struct.
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) };
    if ok == 0 {
        return HostMemory::default();
    }
    HostMemory {
        peak_working_set_bytes: counters.PeakWorkingSetSize as u64,
        working_set_bytes: counters.WorkingSetSize as u64,
        peak_pagefile_bytes: counters.PeakPagefileUsage as u64,
        pagefile_bytes: counters.PagefileUsage as u64,
    }
}

fn math_environment() -> Value {
    json!({
        "ORT_CUDA_GEMM_OPTIONS": std::env::var("ORT_CUDA_GEMM_OPTIONS").ok(),
        "ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO": std::env::var("ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO").ok(),
    })
}

fn error_value(error: &anyhow::Error) -> Value {
    let mut value = json!({
        "error": format!("{error:#}"),
        "chain": error.chain().map(ToString::to_string).collect::<Vec<_>>(),
    });
    if let Some(refusal) = error.downcast_ref::<cuda::PlacementRefusal>() {
        value["placement_refusal"] = placement_summary(&refusal.observation);
    }
    value
}

fn placement_summary(p: &cuda::PlacementObservation) -> Value {
    let mut by_ep = BTreeMap::<String, u64>::new();
    let mut by_op_cpu = BTreeMap::<String, u64>::new();
    let mut cpu_nodes = Vec::new();
    let mut attention = Vec::new();
    for n in &p.nodes {
        *by_ep.entry(n.ep.clone()).or_default() += 1;
        if n.ep != "CUDAExecutionProvider" {
            *by_op_cpu.entry(format!("{}::{}", n.domain, n.op)).or_default() += 1;
            if cpu_nodes.len() < 256 {
                cpu_nodes.push(json!({"name": n.name, "op": n.op, "domain": n.domain, "ep": n.ep, "subgraph": n.subgraph_index}));
            }
        }
        if n.op.contains("Attention") {
            attention.push(json!({"name": n.name, "op": n.op, "ep": n.ep}));
        }
    }
    json!({"complete": p.complete, "scope": p.scope, "reported_nodes": p.reported_nodes,
           "nodes_by_ep": by_ep, "non_cuda_ops": by_op_cpu, "non_cuda_nodes": cpu_nodes,
           "attention_nodes": attention.len(), "attention": attention,
           "strict": p.nodes.iter().all(|n| n.ep == "CUDAExecutionProvider")})
}

#[test]
#[ignore = "L5A metadata only: explicit pinned CPU profile, asset map and runtime descriptor"]
fn l5a_prepare() -> Result<()> {
    let started = Instant::now();
    let precision = env("L5A_PRECISION")?;
    let (profile_path, raw_profile, profile_sha) = pinned("L5A_CPU_PROFILE")?;
    let (map_path, raw_map, map_sha) = pinned("L5A_ASSET_MAP")?;
    let (manifest_path, manifest, manifest_sha) = pinned("L5A_RUNTIME_MANIFEST")?;
    let cpu: Profile = serde_json::from_slice(&raw_profile)?;
    let mut source = files(&raw_map)?;
    let plan = plan()?;
    let output = PathBuf::from(env("L5A_OUTPUT")?);
    std::fs::create_dir(&output)?;
    let mut receipt = json!({
        "schema": "airk.private.l5a-prepare/1", "precision": precision,
        "cpu_profile": {"path": profile_path, "sha256": profile_sha, "manifest": cpu.manifest, "id": cpu.profile_id, "model": cpu.model},
        "asset_map": {"path": map_path, "sha256": map_sha},
        "runtime_manifest": {"path": manifest_path, "sha256": manifest_sha},
        "plan": {"cold_peak_bytes": plan.cold_peak_bytes(), "warm_extra_bytes": plan.warm_extra_bytes(), "headroom_bytes": plan.headroom_bytes()},
        "qualification": false, "model_inference": false,
    });
    // Name -> (role, override variable) of a replacement model/weight pair.
    let replace = |source: &mut Files, pairs: &[(&str, AssetRole, &str)], dir: Option<PathBuf>| -> Result<Vec<Asset>> {
        let mut assets = Vec::new();
        for (name, role, over) in pairs {
            // A staged graph may sit apart from its unchanged weight sidecar.
            let path = match std::env::var(over) {
                Ok(path) => PathBuf::from(path),
                Err(_) => dir.as_ref().context("replacement directory missing")?.join(name),
            };
            let (bytes, digest) = file_sha(&path)?;
            assets.push(Asset {
                name: (*name).into(),
                role: role.clone(),
                bytes,
                sha256: digest,
            });
            source.0.insert((*name).into(), path);
        }
        Ok(assets)
    };
    let fp16_pairs = [
        ("model.onnx", AssetRole::Model, "L5A_FP16_MODEL"),
        ("weights.bin", AssetRole::ExternalWeights, "L5A_FP16_WEIGHTS"),
    ];
    let result = (|| -> Result<(Profile, String, Vec<u8>, Option<Value>)> {
        match precision.as_str() {
            "fp32" => {
                // Optional full-FP32 widened graph (the 12.7.3 FP32 arm) in place of weight8.
                let replacement = match std::env::var("L5A_FP32_REPLACEMENT") {
                    Ok(dir) => Some(replace(
                        &mut source,
                        &[
                            ("model.onnx", AssetRole::Model, "L5A_FP32_MODEL"),
                            ("model.onnx.data", AssetRole::ExternalWeights, "L5A_FP32_WEIGHTS"),
                        ],
                        Some(PathBuf::from(dir)),
                    )?),
                    Err(_) => None,
                };
                let p = CudaRuntime::prepare_focus1024_fp32_probe_profile(
                    &cpu,
                    &source,
                    replacement,
                    &manifest,
                    manifest_sha,
                    plan,
                )?;
                Ok((p.profile, p.calibration_name, p.calibration_bytes, None))
            }
            "fp16" => {
                let export = std::env::var("L5A_FP16_EXPORT").ok().map(PathBuf::from);
                let original: Sha256Digest = env("L5A_ORIGINAL_GRAPH")?.parse()?;
                let assets = replace(&mut source, &fp16_pairs, export)?;
                let p = cuda::prepare_fp16_probe_profile(
                    &cpu,
                    &source,
                    assets,
                    original,
                    &manifest,
                    manifest_sha,
                    plan,
                )?;
                Ok((p.profile, p.calibration_name, p.calibration_bytes, Some(p.attention_plan)))
            }
            "protected" => {
                // Two-pass protected MHA emitter: L5A_POLICY absent = pass 1 (sentinel).
                let export = std::env::var("L5A_FP16_EXPORT").ok().map(PathBuf::from);
                let assets = replace(&mut source, &fp16_pairs, export)?;
                let measurement: cuda::ProtectedMeasurement =
                    serde_json::from_str(&env("L5A_MEASUREMENT")?)?;
                let policy = std::env::var("L5A_POLICY")
                    .ok()
                    .map(|v| v.parse::<Sha256Digest>())
                    .transpose()?;
                // The receipt the measured values were read from (verified by the emitter).
                let receipt = std::env::var("L5A_MEASUREMENT_RECEIPT")
                    .ok()
                    .map(std::fs::read)
                    .transpose()?;
                let p = cuda::prepare_protected_mha_profile(
                    &cpu,
                    &source,
                    assets,
                    &manifest,
                    manifest_sha,
                    measurement,
                    receipt.as_deref(),
                    policy,
                )?;
                let (mut profile, mut calibration_bytes) = (p.profile, p.calibration_bytes);
                let mut plan = json!({"attention_plan": p.attention_plan, "measurement": p.measurement, "pass": if policy.is_some() { 2 } else { 1 }});
                // Answer loop (plan 12.5.2 L4): declare the four card tasks in place of the
                // one-turn task. The zero-rule calibration set replaces the emitter's
                // JointCalibration under the same asset name, and the manifest is recomputed
                // over the emitter's manifest, which already binds the measurement.
                match std::env::var("L5A_LOOP_TASKS").ok().as_deref() {
                    None | Some("0") => {}
                    Some("1") => {
                        let base = profile.manifest;
                        calibration_bytes = admit_loop_tasks(&mut profile)?;
                        profile.manifest = Sha256Digest::from_bytes([0; 32]);
                        let canonical = serde_json::to_vec(&(base, manifest_sha, &profile))?;
                        profile.manifest = airk_core::identity::framed_digest(
                            b"airk.private.cuda-protected-mha-loop-profile/1",
                            &[&canonical],
                        );
                        profile.validate()?;
                        plan["loop_tasks"] = json!({"base_manifest": base, "tasks": profile.tasks, "task_policies": profile.task_policies,
                            "calibration": profile.calibration});
                    }
                    Some(other) => anyhow::bail!("L5A_LOOP_TASKS is 0 or 1, not {other}"),
                }
                Ok((profile, p.calibration_name, calibration_bytes, Some(plan)))
            }
            _ => anyhow::bail!("L5A_PRECISION is fp32, fp16 or protected"),
        }
    })();
    match result {
        Err(error) => {
            receipt["refusal"] = error_value(&error);
            receipt["elapsed_s"] = json!(started.elapsed().as_secs_f64());
            write_fresh(&output.join("prepare-receipt.json"), &receipt)?;
            Err(error)
        }
        Ok((profile, calibration_name, calibration_bytes, attention)) => {
            let calibration = output.join(&calibration_name);
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&calibration)?
                .write_all(&calibration_bytes)?;
            let mut map = BTreeMap::new();
            for asset in &profile.assets {
                let path = if asset.name == calibration_name {
                    calibration.clone()
                } else {
                    source
                        .0
                        .get(&asset.name)
                        .with_context(|| format!("asset {} has no source path", asset.name))?
                        .clone()
                };
                map.insert(asset.name.clone(), path);
            }
            let profile_sha = write_fresh(&output.join("profile.json"), &serde_json::to_value(&profile)?)?;
            let map_sha = write_fresh(&output.join("asset-map.json"), &serde_json::to_value(&map)?)?;
            receipt["profile"] = json!({"path": output.join("profile.json"), "sha256": profile_sha, "manifest": profile.manifest, "id": profile.profile_id,
                "device_execution": profile.device_execution});
            receipt["asset_map_out"] = json!({"path": output.join("asset-map.json"), "sha256": map_sha});
            receipt["calibration"] = json!({"name": calibration_name, "sha256": sha(&calibration_bytes)});
            receipt["attention_plan"] = json!(attention);
            receipt["elapsed_s"] = json!(started.elapsed().as_secs_f64());
            write_fresh(&output.join("prepare-receipt.json"), &receipt)?;
            Ok(())
        }
    }
}

/// Native owner, runtime and the single A6000 device for one GPU selector.
struct Native {
    _control: tempfile::TempDir,
    lease: Arc<airk_store::service::ServiceLease>,
    runtime: Arc<CudaRuntime>,
    _scope: airk_retrieval::native_inference::ServiceComputeScope,
}
fn native(deadline: Instant) -> Result<Native> {
    let parent = PathBuf::from(env("L5A_CONTROL_PARENT")?);
    let control = tempfile::Builder::new()
        .prefix("airk-l5a-control-")
        .tempdir_in(parent.canonicalize()?)?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(control.path())?);
    lease.admit_inference_owner()?;
    // SAFETY: this actual exclusive lease outlives the scope and every native call below.
    let scope = unsafe { airk_retrieval::native_inference::ServiceComputeScope::enter()? };
    let root = std::env::current_exe()?
        .parent()
        .context("image has no parent")?
        .to_path_buf();
    let manifest = root.join("airk-cuda-runtime.json");
    // SAFETY: the same retained real service lease, on this compute thread.
    let runtime = unsafe { CudaRuntime::from_native_lease(lease.clone(), &root, &manifest, deadline)? };
    Ok(Native {
        _control: control,
        lease,
        runtime,
        _scope: scope,
    })
}

fn deadline_from_env(default_s: u64) -> Result<Instant> {
    let seconds = std::env::var("L5A_DEADLINE_S")
        .ok()
        .map(|v| v.parse::<u64>())
        .transpose()?
        .unwrap_or(default_s);
    Ok(Instant::now() + Duration::from_secs(seconds))
}

/// Context width controls: the frozen all-live 8-card rows of the independent Large
/// reference inputs (the same Qwen compiler serves Compact and Large), at the widths
/// in L5A_WIDTHS (default 32,1024,8192).
fn context_inputs() -> Result<Vec<(String, CompiledInput, Option<Value>)>> {
    let (_, raw, _) = pinned("L5A_CONTEXT_INPUTS")?;
    let widths: Vec<usize> = std::env::var("L5A_WIDTHS")
        .unwrap_or_else(|_| "32,1024,8192".into())
        .split(',')
        .map(|v| v.trim().parse())
        .collect::<std::result::Result<_, _>>()?;
    let value: Value = serde_json::from_slice(&raw)?;
    let rows = value["rows"].as_array().context("rows missing")?;
    let mut out = Vec::new();
    for width in widths {
        let row = rows
            .iter()
            .find(|r| r["input"]["input_ids"].as_array().is_some_and(|v| v.len() == width))
            .with_context(|| format!("no frozen Context row of width {width}"))?;
        let input: CompiledInput = serde_json::from_value(row["input"].clone())?;
        out.push((format!("context-t{width}"), input, None));
    }
    Ok(out)
}

/// Focus pair inputs: one synthetic full-width pair plus the profile fixture's real pairs.
fn focus_inputs(compiler: &Compiler, fixture: Option<&[u8]>, width: usize) -> Result<Vec<(String, CompiledInput, Option<Value>)>> {
    let mut rows = Vec::new();
    let compile = |q: &str, d: &str| {
        compiler.compile(
            "",
            q,
            &[Candidate {
                id: "document".into(),
                text: d.into(),
            }],
        )
    };
    // Largest synthetic document that still compiles: the worst-case live suffix.
    let words = |n: usize| "alpha beta gamma delta epsilon ".repeat(n);
    let (mut low, mut high) = (1usize, 8192 / 31);
    while low < high {
        let mid = (low + high).div_ceil(2);
        if compile("What does the document describe?", &words(mid)).is_ok() {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let full = compile("What does the document describe?", &words(low))?;
    let live = full.attention_mask.iter().filter(|m| **m == 1).count();
    ensure!(full.input_ids.len() == width, "Focus width differs");
    rows.push((format!("synthetic-full-live{live}"), full, None));
    // For Focus, rows[0] is the worst-case full-width pair; fixture pairs follow.
    if let Some(raw) = fixture {
        let value: Value = serde_json::from_slice(raw)?;
        for row in value["rows"].as_array().into_iter().flatten() {
            let (Some(q), Some(d)) = (row["scoring_query"].as_str(), row["scoring_document"].as_str()) else {
                continue;
            };
            let id = row["id"].as_str().unwrap_or("row").to_string();
            match compile(q, d) {
                Ok(input) => rows.push((
                    id,
                    input,
                    Some(json!({"official_logits": row["official_logits"], "python_weight8_logits": row["python_weight8_logits"]})),
                )),
                // A pair longer than this width cannot compile (truncation is forbidden).
                Err(error) => eprintln!("L5A fixture row {id} skipped: {error}"),
            }
        }
    }
    Ok(rows)
}

fn max_abs(a: &[f32], b: &Value) -> Option<f64> {
    let b = b.as_array()?;
    (a.len() == b.len()).then(|| {
        a.iter()
            .zip(b)
            .map(|(x, y)| (f64::from(*x) - y.as_f64().unwrap_or(f64::NAN)).abs())
            .fold(0.0, f64::max)
    })
}

#[test]
#[ignore = "L5A GPU: one unqualified probe profile on the actual A6000 through the scorer path"]
fn l5a_gpu() -> Result<()> {
    let started = Instant::now();
    let deadline = deadline_from_env(600)?;
    let step = env("L5A_STEP")?;
    let (profile_path, raw_profile, profile_sha) = pinned("L5A_PROFILE")?;
    let (map_path, raw_map, map_sha) = pinned("L5A_ASSET_MAP")?;
    let profile: Profile = serde_json::from_slice(&raw_profile)?;
    let source = files(&raw_map)?;
    let plan = plan()?;
    let warm: usize = std::env::var("L5A_WARM").ok().map(|v| v.parse()).transpose()?.unwrap_or(10);
    let output = PathBuf::from(env("L5A_OUTPUT")?);
    let mut receipt = json!({
        "schema": "airk.private.l5a-gpu/1", "step": step, "complete": false,
        "profile": {"path": profile_path, "sha256": profile_sha, "manifest": profile.manifest, "id": profile.profile_id, "model": profile.model,
                    "max_tokens": profile.max_tokens, "device_execution": profile.device_execution},
        "asset_map": {"path": map_path, "sha256": map_sha},
        "plan": {"cold_peak_bytes": plan.cold_peak_bytes(), "warm_extra_bytes": plan.warm_extra_bytes(), "headroom_bytes": plan.headroom_bytes()},
        "math_environment": math_environment(), "pid": std::process::id(),
        "image": std::env::current_exe()?, "qualification": false, "product_admission": false,
    });
    write_fresh(&output, &receipt)?;
    let run = (|| -> Result<()> {
        let n = native(deadline)?;
        receipt["runtime_manifest"] = json!(n.runtime.manifest_identity());
        let adapters = n.runtime.adapters(deadline)?;
        ensure!(adapters.len() == 1, "exactly one adapter required");
        let device = n.runtime.device(adapters[0].uuid, deadline)?;
        receipt["adapter_before"] = serde_json::to_value(device.observation(deadline)?)?;
        // SAFETY: the actual exclusive lease is retained by `n` throughout.
        let catalog = unsafe { EvaluationCatalog::from_native_lease(n.lease.clone(), vec![profile.clone()])? };
        let execution = catalog
            .execution_profile(profile.manifest)?
            .bind_cuda_probe(device.clone(), plan, deadline)?;
        receipt["host_threads"] = json!(profile.device_execution.as_ref().map(|d| &d.host_fit));
        if step == "placement" || step == "graph" {
            // FP32 uses the placement diagnostic; FP16 (G5) admits only the graph-metadata one.
            let graph = step == "graph";
            let observer = if graph {
                execution.cuda_graph_observer()?
            } else {
                execution.cuda_load_observer()?
            };
            let t = Instant::now();
            let result = if graph {
                Scorer::diagnose_cuda_graph_metadata(execution, &source)
            } else {
                Scorer::diagnose_cuda_placement(execution, &source)
            };
            receipt["diagnostic_us"] = json!(t.elapsed().as_micros());
            let snapshot = observer.snapshot()?;
            receipt["session_created"] = json!(snapshot.session_created);
            receipt["placement"] = json!(snapshot.placement.as_ref().map(placement_summary));
            if let Some(metadata) = &snapshot.graph_metadata {
                let raw = serde_json::to_vec(metadata)?;
                receipt["graph_metadata"] = json!({"bytes": raw.len(), "sha256": sha(&raw)});
                // ProtectedMha pass 1: the bounded policy the pass-2 profile must carry.
                if let (Some(placement), true) = (
                    snapshot.placement.as_ref(),
                    profile.device_execution.as_ref().is_some_and(|d| matches!(
                        d.placement,
                        airk_core::arbiter::device::PlacementPolicy::BoundedHybrid { .. }
                    )),
                ) {
                    let policy = metadata.bounded_context_execution_policy(
                        &profile,
                        cuda::BoundedContextOptimizer::ProtectedEfficientMha,
                        placement,
                    );
                    let cap = if profile.model == ModelChoice::ContextQwenLarge { 12_067 } else { 9_443 };
                    receipt["bounded_policy"] = match policy {
                        Ok(digest) => json!(digest),
                        Err(error) => error_value(&error),
                    };
                    receipt["cpu_payload_check"] = json!(metadata
                        .validate_bounded_fp16_cpu_values(cap)
                        .map_err(|e| format!("{e:#}"))
                        .err());
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(output.with_extension("graph-metadata.json"))?
                        .write_all(&raw)?;
                }
            }
            receipt["logs"] = serde_json::to_value(&snapshot.logs)?;
            receipt["teardown"] = serde_json::to_value(&snapshot.teardown)?;
            receipt["adapter_after_release"] = serde_json::to_value(device.observation(deadline)?)?;
            if let Err(error) = &result {
                receipt["diagnostic_error"] = error_value(error);
            }
            result?;
            return Ok(());
        }
        ensure!(step == "strict", "L5A_STEP is placement, graph or strict");
        let compiler_asset = profile.assets.iter().find(|a| a.role == AssetRole::Compiler).context("compiler")?;
        let tokenizer_asset = profile.assets.iter().find(|a| a.role == AssetRole::Tokenizer).context("tokenizer")?;
        let compiler = Compiler::from_verified(&source.read_asset(compiler_asset)?, &source.read_asset(tokenizer_asset)?)?;
        let fixture = profile
            .assets
            .iter()
            .find(|a| a.role == AssetRole::Fixture)
            .map(|a| source.read_asset(a))
            .transpose()?;
        let focus = profile.model == ModelChoice::FocusQwenReranker06B;
        let inputs = if focus {
            focus_inputs(&compiler, fixture.as_deref(), profile.max_tokens)?
        } else {
            context_inputs()?
        };
        let t = Instant::now();
        let opened = Scorer::open(execution, &source);
        receipt["cold_open_us"] = json!(t.elapsed().as_micros());
        receipt["host_after_open"] = serde_json::to_value(host_memory())?;
        let scorer = match opened {
            Ok(s) => s,
            Err(error) => {
                receipt["open_error"] = error_value(&error);
                return Err(error);
            }
        };
        receipt["adapter_after_open"] = serde_json::to_value(device.observation(deadline)?)?;
        receipt["placement"] = json!(scorer.device_placement()?.as_ref().map(placement_summary));
        receipt["execution_identity"] = json!(scorer.execution_identity()?);
        flush(&output, &receipt)?;
        let mut rows = Vec::new();
        for (index, (id, input, reference)) in inputs.iter().enumerate() {
            // Real pairs are timed once; the full-width pair gets the warm population.
            let samples = if index == 0 || !focus { 1 + warm } else { 1 };
            let mut times = Vec::new();
            let mut scores = Vec::new();
            for _ in 0..samples {
                let t = Instant::now();
                scores = scorer.score_with_deadline(input, deadline)?;
                scorer.synchronize(deadline)?;
                times.push(t.elapsed().as_micros() as u64);
            }
            let mut sorted = times[1.min(times.len() - 1)..].to_vec();
            sorted.sort_unstable();
            let live = input.attention_mask.iter().filter(|m| **m == 1).count();
            let mut row = json!({"id": id, "tokens": input.input_ids.len(), "live_tokens": live, "input_digest": input.input_digest,
                "first_call_us": times[0], "warm_us": &times[1..], "warm_median_us": sorted.get(sorted.len() / 2),
                "warm_max_us": sorted.last(), "scores": scores,
                "adapter_after": serde_json::to_value(device.observation(deadline)?)?});
            if let Some(reference) = reference {
                row["reference"] = reference.clone();
                row["max_abs_vs_python_weight8"] = json!(max_abs(&scores, &reference["python_weight8_logits"]));
                row["max_abs_vs_official"] = json!(max_abs(&scores, &reference["official_logits"]));
                let argmax = |v: &[f64]| if v[1] > v[0] { 1 } else { 0 };
                if let Some(official) = reference["official_logits"].as_array() {
                    let o: Vec<f64> = official.iter().filter_map(Value::as_f64).collect();
                    if o.len() == 2 {
                        row["argmax_matches_official"] = json!(argmax(&o) == argmax(&[f64::from(scores[0]), f64::from(scores[1])]));
                    }
                }
            }
            rows.push(row);
            receipt["rows"] = json!(rows);
            flush(&output, &receipt)?;
        }
        scorer.synchronize(deadline)?;
        receipt["device_settled"] = json!(scorer.device_settled());
        receipt["host_after_scoring"] = serde_json::to_value(host_memory())?;
        drop(scorer);
        receipt["adapter_after_release"] = serde_json::to_value(device.observation(deadline)?)?;
        Ok(())
    })();
    receipt["host_final"] = serde_json::to_value(host_memory())?;
    receipt["elapsed_s"] = json!(started.elapsed().as_secs_f64());
    receipt["process_cuda_poisoned"] = json!(cuda::process_poisoned());
    match &run {
        Ok(()) => receipt["complete"] = json!(true),
        Err(error) => receipt["error"] = error_value(error),
    }
    flush(&output, &receipt)?;
    run
}

#[test]
#[ignore = "L5A GPU: raw scientific protected-graph session timing at T32/T1024/T8192 (not the scorer path)"]
fn l5a_cutlass() -> Result<()> {
    let started = Instant::now();
    let deadline = deadline_from_env(600)?;
    let model = env("L5A_MODEL")?;
    let gate = env("L5A_GATE")?;
    let campaign = PathBuf::from(env("L5A_CAMPAIGN")?);
    let output = PathBuf::from(env("L5A_OUTPUT")?);
    let warm: usize = std::env::var("L5A_WARM").ok().map(|v| v.parse()).transpose()?.unwrap_or(5);
    ensure!(3 * (1 + warm) <= 21, "scientific owner caps a session at 21 calls");
    let mut receipt = json!({"schema": "airk.private.l5a-cutlass/1", "model": model, "gate": gate, "campaign": campaign,
        "complete": false, "math_environment": math_environment(), "pid": std::process::id(),
        "image": std::env::current_exe()?, "scope": "raw scientific session, file-loaded weights; not the Scorer path (design F2)",
        "qualification": false, "product_admission": false});
    write_fresh(&output, &receipt)?;
    let run = (|| -> Result<Value> {
        let n = native(deadline)?;
        receipt["runtime_manifest"] = json!(n.runtime.manifest_identity());
        let measure = |call: &mut dyn FnMut(usize) -> Result<Vec<f32>>, snapshot: &mut dyn FnMut() -> Result<Value>| -> Result<Value> {
            let mut rows = Vec::new();
            for (index, width) in [(0usize, 32usize), (2, 1024), (3, 8192)] {
                let before = snapshot()?;
                let mut times = Vec::new();
                let mut scores = Vec::new();
                for _ in 0..(1 + warm) {
                    let t = Instant::now();
                    scores = call(index)?;
                    times.push(t.elapsed().as_micros() as u64);
                }
                let mut sorted = times[1..].to_vec();
                sorted.sort_unstable();
                rows.push(json!({"tokens": width, "first_call_us": times[0], "warm_us": &times[1..],
                    "warm_median_us": sorted.get(sorted.len() / 2), "warm_max_us": sorted.last(),
                    "scores": scores, "adapter_before": before, "adapter_after": snapshot()?,
                    "host": serde_json::to_value(host_memory())?}));
            }
            Ok(json!(rows))
        };
        match model.as_str() {
            "compact" => n.runtime.time_protected_cutlass_model(&campaign, &gate, deadline, None, measure),
            _ => anyhow::bail!("L5A_MODEL is compact"),
        }
    })();
    receipt["host_final"] = serde_json::to_value(host_memory())?;
    receipt["elapsed_s"] = json!(started.elapsed().as_secs_f64());
    receipt["process_cuda_poisoned"] = json!(cuda::process_poisoned());
    let result = match run {
        Ok(value) => {
            receipt["result"] = value;
            receipt["complete"] = json!(true);
            Ok(())
        }
        Err(error) => {
            receipt["error"] = error_value(&error);
            Err(error)
        }
    };
    flush(&output, &receipt)?;
    result
}

/// Panel rows: every `rows[].compiled` of a pinned CAL projection (R8 `compact.json`
/// or `large.json`; both carry the same 100 compiled inputs), in ordinal order.
fn panel_rows() -> Result<Vec<(u64, CompiledInput)>> {
    let (_, raw, _) = pinned("L5A_PANEL")?;
    let value: Value = serde_json::from_slice(&raw)?;
    let limit = std::env::var("L5A_PANEL_LIMIT").ok().map(|v| v.parse::<usize>()).transpose()?;
    let mut rows = Vec::new();
    for row in value["rows"].as_array().context("panel rows missing")? {
        let ordinal = row["ordinal"].as_u64().context("ordinal")?;
        rows.push((ordinal, serde_json::from_value(row["compiled"].clone())?));
    }
    if let Some(limit) = limit {
        rows.truncate(limit);
    }
    Ok(rows)
}

#[test]
#[ignore = "L5A GPU: numeric replay of the CAL R7/R8 panel through one Context CUDA Answer route"]
fn l5a_replay() -> Result<()> {
    let started = Instant::now();
    let (profile_path, raw_profile, profile_sha) = pinned("L5A_PROFILE")?;
    let (map_path, raw_map, map_sha) = pinned("L5A_ASSET_MAP")?;
    let profile: Profile = serde_json::from_slice(&raw_profile)?;
    let source = files(&raw_map)?;
    let output = PathBuf::from(env("L5A_OUTPUT")?);
    // protected: AnswerRecipe::ProtectedMha (no graph result; its policy is the pin).
    // base: the evaluation's reviewed FP32 route with its retained graph result.
    let (graph_result, graph_sha) = match std::env::var("L5A_GRAPH_RESULT") {
        Ok(_) => {
            let (_, raw, digest) = pinned("L5A_GRAPH_RESULT")?;
            (raw, digest)
        }
        // The protected review pin is the independently recorded pass-1 policy (the graph
        // diagnostic receipt's bounded_policy), never read back from the profile itself.
        Err(_) => {
            let (_, raw, _) = pinned("L5A_REVIEW_RECEIPT")?;
            let review: Value = serde_json::from_slice(&raw)?;
            ensure!(
                review["schema"] == "airk.private.l5a-gpu/1" && review["step"] == "graph" && review["complete"] == true,
                "review receipt is not a complete graph diagnostic"
            );
            let pin: Sha256Digest = review["bounded_policy"].as_str().context("review policy missing")?.parse()?;
            (Vec::new(), pin)
        }
    };
    let rows = panel_rows()?;
    let mut receipt = json!({
        "schema": "airk.private.l5a-replay/1", "complete": false,
        "profile": {"path": profile_path, "sha256": profile_sha, "manifest": profile.manifest, "id": profile.profile_id, "model": profile.model,
                    "device_execution": profile.device_execution},
        "asset_map": {"path": map_path, "sha256": map_sha}, "graph_result_sha256": graph_sha,
        "panel": env("L5A_PANEL")?, "panel_sha256": env("L5A_PANEL_SHA256")?, "rows_requested": rows.len(),
        "math_environment": math_environment(), "pid": std::process::id(),
        "image": std::env::current_exe()?, "qualification": false, "product_admission": false,
        "loop_card_inputs": null,
    });
    write_fresh(&output, &receipt)?;
    let run = (|| -> Result<()> {
        let load_deadline = Instant::now() + Duration::from_secs(110);
        let overall = Instant::now() + Duration::from_secs(
            std::env::var("L5A_DEADLINE_S").ok().map(|v| v.parse()).transpose()?.unwrap_or(1800),
        );
        let n = native(load_deadline)?;
        let adapters = n.runtime.adapters(load_deadline)?;
        ensure!(adapters.len() == 1, "exactly one adapter required");
        let device = n.runtime.device(adapters[0].uuid, load_deadline)?;
        receipt["adapter_before"] = serde_json::to_value(device.observation(load_deadline)?)?;
        // SAFETY: the actual exclusive lease is retained by `n` throughout.
        let catalog = unsafe { EvaluationCatalog::from_native_lease(n.lease.clone(), vec![profile.clone()])? };
        let unbound = catalog.execution_profile(profile.manifest)?;
        let plan = Scorer::private_compact_answer_capacity(&unbound, graph_sha)?;
        receipt["plan"] = json!({"cold_peak_bytes": plan.cold_peak_bytes(), "warm_extra_bytes": plan.warm_extra_bytes(), "headroom_bytes": plan.headroom_bytes()});
        let execution = unbound.bind_cuda_probe(device.clone(), plan, load_deadline)?;
        let t = Instant::now();
        let (scorer, provenance) = Scorer::open_private_compact_answer(
            execution,
            &source,
            &graph_result,
            graph_sha,
            overall,
            None,
        )?;
        receipt["cold_open_us"] = json!(t.elapsed().as_micros());
        receipt["policy_identity"] = json!(provenance.policy_identity());
        receipt["adapter_after_open"] = serde_json::to_value(device.observation(overall)?)?;
        receipt["host_after_open"] = serde_json::to_value(host_memory())?;
        flush(&output, &receipt)?;
        let mut out = Vec::new();
        for (ordinal, input) in &rows {
            let deadline = (Instant::now() + Duration::from_secs(110)).min(overall);
            let t = Instant::now();
            let scores = scorer.score_with_deadline(input, deadline)?;
            scorer.synchronize(deadline)?;
            out.push(json!({"ordinal": ordinal, "input_digest": input.input_digest, "tokens": input.input_ids.len(),
                            "cards": input.candidate_positions.len(), "scores": scores, "call_us": t.elapsed().as_micros() as u64}));
            if out.len() % 10 == 0 {
                receipt["rows"] = json!(out);
                flush(&output, &receipt)?;
            }
        }
        receipt["rows"] = json!(out);
        receipt["adapter_after_rows"] = serde_json::to_value(device.observation(overall)?)?;
        receipt["device_settled"] = json!(scorer.device_settled());
        drop(scorer);
        receipt["adapter_after_release"] = serde_json::to_value(device.observation(overall)?)?;
        Ok(())
    })();
    receipt["host_final"] = serde_json::to_value(host_memory())?;
    receipt["elapsed_s"] = json!(started.elapsed().as_secs_f64());
    receipt["process_cuda_poisoned"] = json!(cuda::process_poisoned());
    match &run {
        Ok(()) => receipt["complete"] = json!(true),
        Err(error) => receipt["error"] = error_value(error),
    }
    flush(&output, &receipt)?;
    run
}
/// One Context route of the L5C switch trial: its pinned profile, asset map and
/// independently recorded pass-1 review pin (the graph diagnostic receipt's policy).
struct SwitchRoute {
    name: String,
    profile: Profile,
    source: Files,
    pin: Sha256Digest,
    receipt: Value,
}
fn switch_route(name: &str) -> Result<SwitchRoute> {
    let upper = name.to_ascii_uppercase();
    let (profile_path, raw_profile, profile_sha) = pinned(&format!("L5C_{upper}_PROFILE"))?;
    let (map_path, raw_map, map_sha) = pinned(&format!("L5C_{upper}_ASSET_MAP"))?;
    let (review_path, raw_review, review_sha) = pinned(&format!("L5C_{upper}_REVIEW"))?;
    let profile: Profile = serde_json::from_slice(&raw_profile)?;
    let review: Value = serde_json::from_slice(&raw_review)?;
    // Same review rule as the service fixture's `review_pin`: a complete,
    // passing graph diagnostic of the operator-pinned pass-1 profile, whose
    // device execution the final profile keeps except placement and memory.
    let pass1: Sha256Digest = env(&format!("L5C_{upper}_PASS1_PROFILE_SHA256"))?.parse()?;
    ensure!(
        review["schema"] == "airk.private.l5a-gpu/1"
            && review["step"] == "graph"
            && review["complete"] == true
            && review["cpu_payload_check"].is_null()
            && review.get("diagnostic_error").is_none()
            && review.get("error").is_none(),
        "{name} review receipt is not a complete, passing graph diagnostic"
    );
    let reviewed = &review["profile"];
    let final_profile = serde_json::to_value(&profile)?;
    ensure!(
        reviewed["sha256"].as_str() == Some(pass1.to_string().as_str())
            && reviewed["id"] == final_profile["profile_id"]
            && reviewed["model"] == final_profile["model"],
        "{name} review receipt is of another profile"
    );
    let (Some(before), Some(after)) = (
        reviewed["device_execution"].as_object(),
        final_profile["device_execution"].as_object(),
    ) else {
        anyhow::bail!("{name} review or profile has no device execution");
    };
    for key in before.keys().chain(after.keys()) {
        ensure!(
            key == "placement" || key == "memory" || before.get(key) == after.get(key),
            "{name} profile device execution differs from its review in {key}"
        );
    }
    let pin: Sha256Digest = review["bounded_policy"]
        .as_str()
        .context("review policy missing")?
        .parse()?;
    ensure!(
        matches!(
            profile.device_execution.as_ref().map(|d| &d.placement),
            Some(airk_core::arbiter::device::PlacementPolicy::BoundedHybrid { policy }) if *policy == pin
        ),
        "{name} profile placement does not name its reviewed policy"
    );
    Ok(SwitchRoute {
        name: name.into(),
        receipt: json!({"profile": {"path": profile_path, "sha256": profile_sha, "manifest": profile.manifest,
            "id": profile.profile_id, "model": profile.model, "device_execution": profile.device_execution},
            "asset_map": {"path": map_path, "sha256": map_sha}, "review": {"path": review_path, "sha256": review_sha},
            "policy_pin": pin}),
        source: files(&raw_map)?,
        profile,
        pin,
    })
}

/// Waits, under the original deadline and at most 30 s, until device free
/// memory is back within `slack` bytes of `baseline` after a release. Returns
/// the elapsed time, the last observation and whether it settled; a release
/// that never settles is reported, not hidden.
fn wait_released(
    device: &cuda::CudaDevice,
    baseline: u64,
    slack: u64,
    deadline: Instant,
) -> Result<(u128, cuda::AdapterObservation, bool)> {
    let started = Instant::now();
    loop {
        let observed = device.observation(deadline)?;
        if observed.free_bytes.saturating_add(slack) >= baseline {
            return Ok((started.elapsed().as_micros(), observed, true));
        }
        if Instant::now() + Duration::from_millis(50) >= deadline
            || started.elapsed() >= Duration::from_secs(30)
        {
            return Ok((started.elapsed().as_micros(), observed, false));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Plan 12.5.2 item 6 and design L5 5.4, at the scorer level: Compact ->
/// Large -> Compact by default, in one process and one service lease. It opens
/// and drops the private Answer scorer directly; it does not go through
/// Configure, the service warm or its release-before-reserve path. Each phase
/// opens its route through the private Answer path, scores the same pinned CAL
/// panel rows, releases the session and observes the device memory return
/// before the next route opens, so the two models are never resident together.
/// It records per phase the cold open (reload) time, per-call time, the device
/// memory in use and the release time, and checks that repeated phases of one
/// route give bit-identical scores.
///
/// ```text
/// L5C_COMPACT_PROFILE(_SHA256), L5C_COMPACT_ASSET_MAP(_SHA256), L5C_COMPACT_REVIEW(_SHA256)
/// L5C_LARGE_PROFILE(_SHA256),   L5C_LARGE_ASSET_MAP(_SHA256),   L5C_LARGE_REVIEW(_SHA256)
/// L5C_COMPACT_PASS1_PROFILE_SHA256, L5C_LARGE_PASS1_PROFILE_SHA256 (the reviewed pass-1 profiles)
/// L5C_PHASES (default compact,large,compact), L5A_PANEL(_SHA256), L5A_PANEL_LIMIT
/// L5C_RELEASE_SLACK_BYTES (default 512 MiB), L5A_CONTROL_PARENT, L5A_DEADLINE_S, L5C_OUTPUT
/// ```
#[test]
#[ignore = "L5C GPU: Context switch Compact -> Large -> Compact in one lease on the actual A6000"]
fn l5c_switch() -> Result<()> {
    let started = Instant::now();
    let phases: Vec<String> = std::env::var("L5C_PHASES")
        .unwrap_or_else(|_| "compact,large,compact".into())
        .split(',')
        .map(|v| v.trim().to_ascii_lowercase())
        .collect();
    ensure!(
        !phases.is_empty() && phases.len() <= 6,
        "L5C_PHASES names one to six phases"
    );
    let mut routes = BTreeMap::new();
    for name in &phases {
        ensure!(
            name == "compact" || name == "large",
            "L5C phase {name} is not compact or large"
        );
        if !routes.contains_key(name) {
            routes.insert(name.clone(), switch_route(name)?);
        }
    }
    let slack: u64 = std::env::var("L5C_RELEASE_SLACK_BYTES")
        .ok()
        .map(|v| v.parse())
        .transpose()?
        .unwrap_or(512 << 20);
    let rows = panel_rows()?;
    ensure!(!rows.is_empty(), "L5C needs at least one panel row");
    let output = PathBuf::from(env("L5C_OUTPUT")?);
    let mut receipt = json!({
        "schema": "airk.private.l5c-switch/1", "complete": false, "phases_requested": phases,
        "routes": routes.values().map(|r| (r.name.clone(), r.receipt.clone())).collect::<BTreeMap<_, _>>(),
        "panel": env("L5A_PANEL")?, "panel_sha256": env("L5A_PANEL_SHA256")?, "rows_per_phase": rows.len(),
        "release_slack_bytes": slack, "math_environment": math_environment(), "pid": std::process::id(),
        "image": std::env::current_exe()?, "qualification": false, "product_admission": false,
    });
    write_fresh(&output, &receipt)?;
    let mut done = Vec::new();
    let run = (|| -> Result<()> {
        let overall = Instant::now()
            + Duration::from_secs(
                std::env::var("L5A_DEADLINE_S")
                    .ok()
                    .map(|v| v.parse())
                    .transpose()?
                    .unwrap_or(900),
            );
        let n = native((Instant::now() + Duration::from_secs(110)).min(overall))?;
        let adapters = n.runtime.adapters(overall)?;
        ensure!(adapters.len() == 1, "exactly one adapter required");
        let device = n.runtime.device(adapters[0].uuid, overall)?;
        let baseline = device.observation(overall)?;
        receipt["adapter_baseline"] = serde_json::to_value(&baseline)?;
        // SAFETY: the actual exclusive lease is retained by `n` throughout.
        let catalog = unsafe {
            EvaluationCatalog::from_native_lease(
                n.lease.clone(),
                routes.values().map(|r| r.profile.clone()).collect(),
            )?
        };
        let mut first_scores: BTreeMap<String, Vec<Vec<f32>>> = BTreeMap::new();
        for (index, name) in phases.iter().enumerate() {
            let route = &routes[name];
            let before = device.observation(overall)?;
            // Never resident together: the previous route's memory is back first.
            ensure!(
                before.free_bytes.saturating_add(slack) >= baseline.free_bytes,
                "phase {index} ({name}) would open while the previous route still holds device memory"
            );
            let unbound = catalog.execution_profile(route.profile.manifest)?;
            let plan = Scorer::private_compact_answer_capacity(&unbound, route.pin)?;
            let load_deadline = (Instant::now() + Duration::from_secs(110)).min(overall);
            let execution = unbound.bind_cuda_probe(device.clone(), plan, load_deadline)?;
            let t = Instant::now();
            let (scorer, provenance) = Scorer::open_private_compact_answer(
                execution,
                &route.source,
                &[],
                route.pin,
                overall,
                None,
            )?;
            let open_us = t.elapsed().as_micros();
            let after_open = device.observation(overall)?;
            let mut calls = Vec::new();
            let mut scores = Vec::new();
            let mut min_free = after_open.free_bytes;
            for (ordinal, input) in &rows {
                let deadline = (Instant::now() + Duration::from_secs(110)).min(overall);
                let t = Instant::now();
                let value = scorer.score_with_deadline(input, deadline)?;
                scorer.synchronize(deadline)?;
                let call_us = t.elapsed().as_micros() as u64;
                let observed = device.observation(overall)?;
                min_free = min_free.min(observed.free_bytes);
                calls.push(json!({"ordinal": ordinal, "tokens": input.input_ids.len(),
                    "cards": input.candidate_positions.len(), "call_us": call_us}));
                scores.push(value);
            }
            let settled = scorer.device_settled();
            let mut sorted: Vec<u64> = calls
                .iter()
                .skip(1)
                .filter_map(|c| c["call_us"].as_u64())
                .collect();
            sorted.sort_unstable();
            let drop_started = Instant::now();
            drop(scorer);
            let drop_us = drop_started.elapsed().as_micros();
            let (wait_us, after_release, released) =
                wait_released(&device, baseline.free_bytes, slack, overall)?;
            let identical = first_scores.get(name).map(|first| first == &scores);
            if identical.is_none() {
                first_scores.insert(name.clone(), scores.clone());
            }
            done.push(json!({
                "phase": index, "route": name, "manifest": route.profile.manifest,
                "policy_identity": provenance.policy_identity(),
                "free_before_bytes": before.free_bytes,
                "open_us": open_us, "free_after_open_bytes": after_open.free_bytes,
                "first_call_us": calls.first().and_then(|c| c["call_us"].as_u64()),
                "warm_median_us": sorted.get(sorted.len() / 2), "warm_max_us": sorted.last(),
                "min_free_during_calls_bytes": min_free,
                "peak_device_used_bytes": baseline.free_bytes.saturating_sub(min_free),
                "device_settled": settled,
                "release_drop_us": drop_us, "release_wait_us": wait_us, "released": released,
                "free_after_release_bytes": after_release.free_bytes,
                "scores_identical_to_first_phase_of_route": identical,
                "calls": calls, "scores": scores,
                "host_after_phase": serde_json::to_value(host_memory())?,
            }));
            receipt["phases"] = json!(done);
            flush(&output, &receipt)?;
            ensure!(
                released,
                "phase {index} ({name}) release did not return device memory within 30 s"
            );
            ensure!(
                identical != Some(false),
                "phase {index} ({name}) scores differ from the first phase of its route"
            );
        }
        Ok(())
    })();
    // Per-route peaks against their sum: a switch never holds both routes.
    let mut peaks = BTreeMap::<String, u64>::new();
    for phase in &done {
        let name = phase["route"].as_str().unwrap_or_default().to_string();
        let peak = phase["peak_device_used_bytes"].as_u64().unwrap_or_default();
        let entry = peaks.entry(name).or_insert(0);
        *entry = (*entry).max(peak);
    }
    receipt["peak_device_used_bytes_by_route"] = json!(peaks);
    receipt["peak_device_used_bytes_sum"] = json!(peaks.values().sum::<u64>());
    receipt["host_final"] = serde_json::to_value(host_memory())?;
    receipt["elapsed_s"] = json!(started.elapsed().as_secs_f64());
    receipt["process_cuda_poisoned"] = json!(cuda::process_poisoned());
    match &run {
        Ok(()) => receipt["complete"] = json!(true),
        Err(error) => receipt["error"] = error_value(error),
    }
    flush(&output, &receipt)?;
    run
}

/// Name -> path map of every file in one asset directory (a CPU roster entry).
fn directory_files(directory: &Path) -> Result<Files> {
    let mut map = BTreeMap::new();
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            map.insert(entry.file_name().to_string_lossy().into_owned(), entry.path());
        }
    }
    Ok(Files(map))
}

/// The parity rule a CUDA Focus entry must meet against its CPU twin: every
/// winner agrees and every raw score is within the recorded tolerance.
const L5C_FOCUS_TOLERANCE: f64 = 1e-3;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FocusShortlist {
    schema: String,
    rows: Vec<FocusShortlistRow>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FocusShortlistRow {
    id: String,
    query: String,
    candidates: Vec<Candidate>,
}

#[test]
#[ignore = "L5C GPU: frozen 16/40 Focus shortlist through the strict native scorer"]
fn l5c_focus_shortlist() -> Result<()> {
    use airk_core::arbiter::FocusScore;
    use std::collections::BTreeSet;

    let started = Instant::now();
    let deadline = deadline_from_env(900)?;
    let (_, raw_input, input_sha) = pinned("L5C_FOCUS_SHORTLIST_INPUT")?;
    ensure!(
        raw_input.len() <= 64 << 20,
        "Focus shortlist input exceeds 64 MiB"
    );
    let input: FocusShortlist = serde_json::from_slice(&raw_input)?;
    ensure!(
        input.schema == "airk.private.focus-shortlist-input/1"
            && (1..=20).contains(&input.rows.len()),
        "invalid Focus shortlist schema or row count"
    );
    let mut row_ids = BTreeSet::new();
    for row in &input.rows {
        let mut candidate_ids = BTreeSet::new();
        ensure!(
            !row.id.trim().is_empty()
                && row_ids.insert(&row.id)
                && !row.query.trim().is_empty()
                && row.candidates.len() == 40
                && row.candidates.iter().all(|candidate| {
                    !candidate.id.trim().is_empty()
                        && candidate_ids.insert(&candidate.id)
                        && !candidate.text.trim().is_empty()
                }),
            "invalid Focus shortlist row or candidate roster"
        );
    }

    let (_, raw_profile, profile_sha) = pinned("L5A_PROFILE")?;
    let (_, raw_map, map_sha) = pinned("L5A_ASSET_MAP")?;
    let profile: Profile = serde_json::from_slice(&raw_profile)?;
    let manifest = "a3157dd1a6de8f6b5c6ea5348a7d2e53d70c0853f8d66833a12a39ca91f2d183"
        .parse::<Sha256Digest>()?;
    let graph = "a16fcaa1bf2d67b019b30cda56d76ba23d589c346ef2ae76d75c229ebe866810"
        .parse::<Sha256Digest>()?;
    ensure!(
        profile.model == ModelChoice::FocusQwenReranker06B
            && profile.manifest == manifest
            && profile.max_tokens == 1024
            && profile.max_candidates == 4
            && profile.device_execution.is_some()
            && profile
                .assets
                .iter()
                .any(|asset| asset.role == AssetRole::Model && asset.sha256 == graph),
        "expected the retained strict CUDA Focus 1024 W8 profile"
    );
    let source = files(&raw_map)?;
    let compiler_asset = profile
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Compiler)
        .context("Focus compiler missing")?;
    let tokenizer_asset = profile
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Tokenizer)
        .context("Focus tokenizer missing")?;
    let compiler = Compiler::from_verified(
        &source.read_asset(compiler_asset)?,
        &source.read_asset(tokenizer_asset)?,
    )?;
    let capacity = plan()?;
    let output = PathBuf::from(env("L5A_OUTPUT")?);
    let mut receipt = json!({
        "schema": "airk.private.focus-shortlist-cuda/1", "complete": false,
        "input_sha256": input_sha, "profile_sha256": profile_sha,
        "asset_map_sha256": map_sha, "model_manifest": manifest, "graph_sha256": graph,
        "pid": std::process::id(), "image": std::env::current_exe()?,
        "math_environment": math_environment(), "rows": [],
        "qualification": false, "product_admission": false,
        "gpu_process_memory": {"available": false, "reason": "The native CUDA runner measures adapter-wide memory, not process-attributed GPU memory."},
    });
    write_fresh(&output, &receipt)?;

    let run = (|| -> Result<()> {
        let n = native(deadline)?;
        receipt["runtime_manifest"] = json!(n.runtime.manifest_identity());
        let adapters = n.runtime.adapters(deadline)?;
        ensure!(adapters.len() == 1, "exactly one CUDA adapter required");
        let device = n.runtime.device(adapters[0].uuid, deadline)?;
        receipt["adapter_before_open"] = json!(device.observation(deadline)?);
        // SAFETY: the actual exclusive lease is retained by n throughout.
        let catalog =
            unsafe { EvaluationCatalog::from_native_lease(n.lease.clone(), vec![profile])? };
        let execution = catalog.execution_profile(manifest)?.bind_cuda_probe(
            device.clone(),
            capacity,
            deadline,
        )?;
        let opening = Instant::now();
        let scorer = Scorer::open(execution, &source)?;
        receipt["cold_open_us"] = json!(opening.elapsed().as_micros());
        let placement = scorer
            .device_placement()?
            .context("CUDA placement missing")?;
        ensure!(
            placement.complete
                && !placement.nodes.is_empty()
                && placement
                    .nodes
                    .iter()
                    .all(|node| node.ep == "CUDAExecutionProvider"),
            "complete strict CUDA placement required"
        );
        receipt["placement"] = placement_summary(&placement);
        receipt["execution_identity"] = json!(scorer.execution_identity()?);
        receipt["adapter_after_open"] = json!(device.observation(deadline)?);
        flush(&output, &receipt)?;

        for row in &input.rows {
            let mut arms = Vec::new();
            for width in [16, 40] {
                let arm_start = Instant::now();
                let initial = device.observation(deadline)?;
                let mut peak_device_used = initial.total_bytes.saturating_sub(initial.free_bytes);
                let mut candidates = Vec::new();
                let mut relevance = Vec::new();
                let mut pair_wall_us = 0_u128;
                for (index, candidate) in row.candidates[..width].iter().enumerate() {
                    let ordinal = index + 1;
                    let pair = Instant::now();
                    // Use the same single-candidate compile and independent two-logit readout as Search Focus.
                    let compiled = compiler
                        .compile("", &row.query, std::slice::from_ref(candidate))
                        .with_context(|| {
                            format!("Focus width {width} ordinal {ordinal} could not compile")
                        })?;
                    let logits = scorer
                        .score_with_deadline(&compiled, deadline)
                        .with_context(|| format!("Focus width {width} ordinal {ordinal} failed"))?;
                    scorer.synchronize(deadline)?;
                    let pair_us = pair.elapsed().as_micros();
                    pair_wall_us += pair_us;
                    let [no_logit, yes_logit] = logits.as_slice() else {
                        anyhow::bail!(
                            "Focus width {width} ordinal {ordinal} returned invalid logit count"
                        );
                    };
                    let score = FocusScore::from_logits(
                        sha(candidate.id.as_bytes()),
                        *no_logit,
                        *yes_logit,
                    )
                    .with_context(|| {
                        format!("Focus width {width} ordinal {ordinal} returned invalid logits")
                    })?;
                    let observation = device.observation(deadline)?;
                    peak_device_used = peak_device_used.max(
                        observation
                            .total_bytes
                            .saturating_sub(observation.free_bytes),
                    );
                    relevance.push(score.relevance);
                    candidates.push(json!({
                        "id": candidate.id, "ordinal": ordinal, "input_digest": compiled.input_digest,
                        "no_logit": no_logit, "yes_logit": yes_logit, "relevance": score.relevance,
                        "wall_us": pair_us, "adapter_free_bytes": observation.free_bytes,
                    }));
                }
                let mut ranking: Vec<_> = (1..=width).collect();
                ranking.sort_by(|&left, &right| {
                    relevance[right - 1]
                        .total_cmp(&relevance[left - 1])
                        .then(left.cmp(&right))
                });
                arms.push(json!({
                    "width": width, "candidates": candidates, "ranked_ordinals": ranking,
                    "wall_us": arm_start.elapsed().as_micros(), "pair_wall_us": pair_wall_us,
                    "adapter_before": initial,
                    "sampled_adapter_peak_used_bytes": peak_device_used,
                }));
            }
            receipt["rows"]
                .as_array_mut()
                .context("Focus shortlist receipt row list missing")?
                .push(json!({"id": row.id, "arms": arms}));
            flush(&output, &receipt)?;
        }
        scorer.synchronize(deadline)?;
        ensure!(
            scorer.device_settled() && !cuda::process_poisoned(),
            "CUDA Focus scorer did not settle"
        );
        receipt["device_settled"] = json!(true);
        drop(scorer);
        receipt["adapter_after_release"] = json!(device.observation(deadline)?);
        Ok(())
    })();
    receipt["elapsed_s"] = json!(started.elapsed().as_secs_f64());
    receipt["process_cuda_poisoned"] = json!(cuda::process_poisoned());
    match &run {
        Ok(()) => receipt["complete"] = json!(true),
        Err(error) => receipt["error"] = error_value(error),
    }
    flush(&output, &receipt)?;
    run
}

/// Focus parity on the actual A6000 (review M2 of the CUDA Focus seam): the
/// CPU Focus profile and its CUDA twin score the same compiled pairs (the
/// worst-case full-width pair and the profile fixture's real pairs) in one
/// process and one lease, CPU first, then CUDA. The receipt records every
/// raw score of both, the largest difference, winner agreement and the
/// tolerance, and the seam pins it.
///
/// ```text
/// L5C_CPU_FOCUS_PROFILE(_SHA256), L5C_CPU_FOCUS_ASSETS (directory)
/// L5C_CUDA_FOCUS_PROFILE(_SHA256), L5C_CUDA_FOCUS_ASSET_MAP(_SHA256), L5A_PLAN
/// L5A_CONTROL_PARENT, L5A_DEADLINE_S, L5C_OUTPUT (fresh file)
/// ```
#[test]
#[ignore = "L5C GPU: CPU against CUDA Focus parity on fixed pairs on the actual A6000"]
fn l5c_focus_parity() -> Result<()> {
    let started = Instant::now();
    let (cpu_path, raw_cpu, cpu_sha) = pinned("L5C_CPU_FOCUS_PROFILE")?;
    let (cuda_path, raw_cuda, cuda_sha) = pinned("L5C_CUDA_FOCUS_PROFILE")?;
    let (map_path, raw_map, map_sha) = pinned("L5C_CUDA_FOCUS_ASSET_MAP")?;
    let cpu: Profile = serde_json::from_slice(&raw_cpu)?;
    let cuda: Profile = serde_json::from_slice(&raw_cuda)?;
    ensure!(
        cpu.model == ModelChoice::FocusQwenReranker06B
            && cuda.model == cpu.model
            && cpu.device_execution.is_none()
            && cuda.device_execution.is_some(),
        "parity needs a CPU Focus profile and its CUDA twin"
    );
    let cpu_assets = PathBuf::from(env("L5C_CPU_FOCUS_ASSETS")?);
    let cpu_source = directory_files(&cpu_assets)?;
    let cuda_source = files(&raw_map)?;
    let plan = plan()?;
    let output = PathBuf::from(env("L5C_OUTPUT")?);
    let mut receipt = json!({
        "schema": "airk.private.l5c-focus-parity/1", "complete": false, "passed": false,
        "cpu": {"path": cpu_path, "sha256": cpu_sha, "manifest": cpu.manifest, "id": cpu.profile_id,
                "assets": cpu_assets},
        "cuda": {"path": cuda_path, "sha256": cuda_sha, "manifest": cuda.manifest, "id": cuda.profile_id,
                 "asset_map": {"path": map_path, "sha256": map_sha}},
        "tolerance": L5C_FOCUS_TOLERANCE, "math_environment": math_environment(),
        "pid": std::process::id(), "image": std::env::current_exe()?,
        "qualification": false, "product_admission": false,
    });
    write_fresh(&output, &receipt)?;
    let run = (|| -> Result<()> {
        let deadline = deadline_from_env(900)?;
        let n = native((Instant::now() + Duration::from_secs(110)).min(deadline))?;
        // SAFETY: the actual exclusive lease is retained by `n` throughout.
        let catalog = unsafe {
            EvaluationCatalog::from_native_lease(n.lease.clone(), vec![cpu.clone(), cuda.clone()])?
        };
        let compiler_asset = cpu.assets.iter().find(|a| a.role == AssetRole::Compiler).context("compiler")?;
        let tokenizer_asset = cpu.assets.iter().find(|a| a.role == AssetRole::Tokenizer).context("tokenizer")?;
        let compiler = Compiler::from_verified(
            &cpu_source.read_asset(compiler_asset)?,
            &cpu_source.read_asset(tokenizer_asset)?,
        )?;
        let fixture = cpu
            .assets
            .iter()
            .find(|a| a.role == AssetRole::Fixture)
            .map(|a| cpu_source.read_asset(a))
            .transpose()?;
        let inputs = focus_inputs(&compiler, fixture.as_deref(), cpu.max_tokens)?;
        let score = |scorer: &Scorer| -> Result<(Vec<Vec<f32>>, Vec<u64>)> {
            let mut scores = Vec::new();
            let mut times = Vec::new();
            for (_, input, _) in &inputs {
                let t = Instant::now();
                scores.push(scorer.score_with_deadline(input, deadline)?);
                scorer.synchronize(deadline)?;
                times.push(t.elapsed().as_micros() as u64);
            }
            Ok((scores, times))
        };
        let t = Instant::now();
        let scorer = Scorer::open(catalog.execution_profile(cpu.manifest)?, &cpu_source)?;
        receipt["cpu"]["open_us"] = json!(t.elapsed().as_micros());
        let (cpu_scores, cpu_times) = score(&scorer)?;
        drop(scorer);
        let adapters = n.runtime.adapters(deadline)?;
        ensure!(adapters.len() == 1, "exactly one adapter required");
        let device = n.runtime.device(adapters[0].uuid, deadline)?;
        let execution = catalog
            .execution_profile(cuda.manifest)?
            .bind_cuda_probe(device.clone(), plan, deadline)?;
        let t = Instant::now();
        let scorer = Scorer::open(execution, &cuda_source)?;
        receipt["cuda"]["open_us"] = json!(t.elapsed().as_micros());
        receipt["cuda"]["placement"] = json!(scorer.device_placement()?.as_ref().map(placement_summary));
        let (cuda_scores, cuda_times) = score(&scorer)?;
        drop(scorer);
        let mut rows = Vec::new();
        let mut worst = 0.0f64;
        let mut winners = 0usize;
        for (index, (id, input, _)) in inputs.iter().enumerate() {
            let (a, b) = (&cpu_scores[index], &cuda_scores[index]);
            ensure!(a.len() == b.len() && !a.is_empty(), "score widths differ");
            let diff = a
                .iter()
                .zip(b)
                .map(|(x, y)| (f64::from(*x) - f64::from(*y)).abs())
                .fold(0.0, f64::max);
            let winner = |v: &[f32]| {
                v.iter()
                    .enumerate()
                    .max_by(|x, y| x.1.total_cmp(y.1))
                    .map(|(i, _)| i)
            };
            let agree = winner(a) == winner(b);
            winners += usize::from(agree);
            worst = worst.max(diff);
            rows.push(json!({"id": id, "tokens": input.input_ids.len(), "cpu": a, "cuda": b,
                "max_abs": diff, "winner_agrees": agree,
                "cpu_us": cpu_times[index], "cuda_us": cuda_times[index]}));
        }
        receipt["rows"] = json!(rows);
        receipt["row_count"] = json!(inputs.len());
        receipt["winners_agree"] = json!(winners);
        receipt["max_abs"] = json!(worst);
        receipt["passed"] = json!(winners == inputs.len() && worst <= L5C_FOCUS_TOLERANCE);
        Ok(())
    })();
    receipt["elapsed_s"] = json!(started.elapsed().as_secs_f64());
    receipt["process_cuda_poisoned"] = json!(cuda::process_poisoned());
    match &run {
        Ok(()) => receipt["complete"] = json!(true),
        Err(error) => receipt["error"] = error_value(error),
    }
    flush(&output, &receipt)?;
    run
}
