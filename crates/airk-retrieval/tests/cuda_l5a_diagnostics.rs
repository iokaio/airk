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
//!   protected prepare: L5A_MEASUREMENT (JSON), L5A_MEASUREMENT_RECEIPT, L5A_POLICY (pass 2)
//! l5a_replay: L5A_GRAPH_RESULT(_SHA256) for an FP32 base, or L5A_REVIEW_RECEIPT(_SHA256)
//!   (the pass-1 graph diagnostic receipt) for a protected route; L5A_PANEL(_SHA256)
//!   L5A_CONTROL_PARENT   existing directory for the private lease root
//!   L5A_WARM             warm samples per width (default 10), L5A_DEADLINE_S (default 600)
//! l5a_cutlass        GPU. Raw scientific session timing for the protected graphs.
//!   L5A_MODEL compact | large, L5A_GATE typed-graph digest, L5A_CAMPAIGN, L5A_OUTPUT,
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
        AssetSource, EvaluationCatalog, Scorer,
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
                Ok((
                    p.profile,
                    p.calibration_name,
                    p.calibration_bytes,
                    Some(json!({"attention_plan": p.attention_plan, "measurement": p.measurement, "pass": if policy.is_some() { 2 } else { 1 }})),
                ))
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
            "large" => n.runtime.time_large_direct(&campaign, &gate, deadline, None, measure),
            _ => anyhow::bail!("L5A_MODEL is compact or large"),
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