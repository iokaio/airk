// SPDX-License-Identifier: Apache-2.0
//! Portable Context graph (FP16-stored weights, FP32 arithmetic) on the static
//! CPU runtime at the 8-core class. Private development observations only: no
//! selector here sets a qualification, a qualified target or a download, and
//! none is release admission. See `scripts/arbiter-portable-cpu/README.md`.
use super::private_control;
use airk_core::{
    arbiter::{
        Asset, CpuFit, ModelChoice, Profile, Stage, Task,
        resource_policy::{ResourcePolicy, ResourceSample, ResourceSeries},
    },
    identity::Sha256Digest,
};
use airk_retrieval::arbiter::{
    compiler::{Candidate, CompiledInput},
    runtime::{
        AssetSource, ExecutionProfile, Scorer, current_target,
        qualification::{AssetInput, CalibrationInput, ProfileInput, prepare, prepare_with_limits},
    },
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

const EMIT_REQUEST: &str = "airk.private.portable-cpu-emit-request/1";
const TIER_INPUT: &str = "airk.private.portable-cpu-tier-input/1";
const CORES: usize = 8;
/// Development screen only: the campaign's frozen FP16 raw-score limit, applied
/// here against the retained original FP32 reference. Not a qualification gate.
const RAW_SCREEN: f64 = 0.007;
const SCOPE: &str = "Private unqualified development observation of the portable graph on the static CPU runtime under an 8-core affinity cap. No task quality, calibration, composed service admission, platform transfer, catalog or release claim.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Tier {
    Full,
    Fast,
}
impl Tier {
    fn max_tokens(self) -> usize {
        match self {
            Tier::Full => 8192,
            Tier::Fast => 2048,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Tier::Full => "full",
            Tier::Fast => "fast",
        }
    }
    fn admits(self, tokens: usize) -> bool {
        tokens <= self.max_tokens()
    }
}
fn cpu8() -> CpuFit {
    CpuFit {
        minimum_physical_cores: CORES,
        intra_threads: CORES,
        inter_threads: 1,
        sequential: true,
        spinning: false,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestedAsset {
    asset: Asset,
    path: PathBuf,
}
/// One retained reference receipt: `result.compiled` is the input and
/// `result.original_binding.original_fp32` the reference scores.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CasePin {
    name: String,
    path: PathBuf,
    sha256: Sha256Digest,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmitRequest {
    schema: String,
    model: ModelChoice,
    profile_id: String,
    memory_reservation_bytes: u64,
    peak_disk_bytes: u64,
    assets: Vec<RequestedAsset>,
    cases: Vec<CasePin>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TierInput {
    schema: String,
    tier: Tier,
    profile: PathBuf,
    profile_sha256: Sha256Digest,
    files: BTreeMap<String, PathBuf>,
    cases: Vec<CasePin>,
}

fn sha(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn file_sha256(path: &Path) -> Result<Sha256Digest> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 4 << 20];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(Sha256Digest::from_bytes(hash.finalize().into()))
}
fn variable(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("missing {name}"))
}
fn bounded(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)
        .with_context(|| format!("private input {}", path.display()))?;
    ensure!(
        metadata.is_file() && metadata.len() <= maximum,
        "private input is not a bounded regular file"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= maximum,
        "private input grew beyond its bound"
    );
    Ok(bytes)
}
/// Reads the file named by `name` and requires the SHA-256 in `name_SHA256`.
fn pinned(name: &str, maximum: u64) -> Result<(Vec<u8>, Sha256Digest)> {
    let path = PathBuf::from(variable(name)?);
    let expected: Sha256Digest = variable(&format!("{name}_SHA256"))?.parse()?;
    let bytes = bounded(&path, maximum)?;
    ensure!(
        sha(&bytes) == expected,
        "{name} bytes differ from their SHA-256 pin"
    );
    Ok((bytes, expected))
}
/// Receipts and emitted profiles hold live paths, so they stay under `artifacts/`.
fn private_output() -> Result<PathBuf> {
    let output = PathBuf::from(variable("AIRK_PORTABLE_CPU_OUTPUT")?);
    let artifacts = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("repository root missing")?
        .join("artifacts")
        .canonicalize()?;
    ensure!(
        output.is_absolute()
            && !output.try_exists()?
            && output
                .parent()
                .context("output parent missing")?
                .canonicalize()?
                .starts_with(&artifacts),
        "fresh private artifact output required"
    );
    Ok(output)
}

fn unqualified_candidate(profile: &Profile, tier: Tier) -> Result<()> {
    ensure!(
        matches!(
            profile.model,
            ModelChoice::ContextQwenCompact | ModelChoice::ContextQwenLarge
        ) && profile.stage == Some(Stage::Context)
            && profile.tasks == BTreeSet::from([Task::ContextAction])
            && profile.cpu_fit.as_ref() == Some(&cpu8())
            && profile.device_execution.is_none()
            && profile.max_tokens == tier.max_tokens()
            && profile.max_candidates == 8
            && profile.profile_id.starts_with("private-")
            && profile.qualification.is_none()
            && profile.qualified_targets.is_empty()
            && profile.downloads.is_empty(),
        "exact private unqualified 8-core portable candidate required"
    );
    Ok(())
}

struct Files(BTreeMap<String, PathBuf>);
impl AssetSource for Files {
    fn read_asset(&self, asset: &Asset) -> Result<Vec<u8>> {
        let path = self.0.get(&asset.name).context("unmapped portable asset")?;
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.len() == asset.bytes,
            "portable asset length differs"
        );
        // Reserve the authenticated length exactly, as production does;
        // geometric growth would inflate the measured peak.
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(usize::try_from(asset.bytes)?)
            .context("portable asset allocation unavailable")?;
        file.take(asset.bytes + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == asset.bytes,
            "portable asset changed during its bounded read"
        );
        Ok(bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
struct Memory {
    resident_bytes: Option<u64>,
    peak_resident_bytes: u64,
    private_commit_bytes: Option<u64>,
    peak_private_commit_bytes: Option<u64>,
}
#[cfg(windows)]
fn process_memory() -> Result<Memory> {
    use windows_sys::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };
    // SAFETY: the counters are plain integers, for which zero is a valid value.
    let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&counters) as u32;
    counters.cb = size;
    // SAFETY: current-process pseudo handle and correctly sized output storage.
    let observed = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) };
    ensure!(observed != 0, "process memory observation unavailable");
    Ok(Memory {
        resident_bytes: Some(counters.WorkingSetSize as u64),
        peak_resident_bytes: counters.PeakWorkingSetSize as u64,
        private_commit_bytes: Some(counters.PagefileUsage as u64),
        peak_private_commit_bytes: Some(counters.PeakPagefileUsage as u64),
    })
}
/// Linux reports the current resident set in `/proc`; other hosts report the
/// lifetime peak only.
#[cfg(not(windows))]
fn process_memory() -> Result<Memory> {
    let resident_bytes = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("VmRSS:"))
                .and_then(|value| value.trim().strip_suffix("kB"))
                .and_then(|value| value.trim().parse::<u64>().ok())
        })
        .and_then(|kibibytes| kibibytes.checked_mul(1024));
    Ok(Memory {
        resident_bytes,
        peak_resident_bytes: super::peak_rss().context("process peak RSS unavailable")?,
        private_commit_bytes: None,
        peak_private_commit_bytes: None,
    })
}
#[cfg(windows)]
fn affinity_mask() -> Option<u64> {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessAffinityMask};
    let (mut process, mut system) = (0usize, 0usize);
    // SAFETY: current-process pseudo handle and two correctly sized outputs.
    (unsafe { GetProcessAffinityMask(GetCurrentProcess(), &mut process, &mut system) } != 0)
        .then_some(process as u64)
}
#[cfg(not(windows))]
fn affinity_mask() -> Option<u64> {
    None
}
fn isa() -> Value {
    #[cfg(target_arch = "x86_64")]
    {
        json!({"architecture":"x86_64",
            "avx2":std::arch::is_x86_feature_detected!("avx2"),
            "fma":std::arch::is_x86_feature_detected!("fma"),
            "f16c":std::arch::is_x86_feature_detected!("f16c"),
            "avx512f":std::arch::is_x86_feature_detected!("avx512f")})
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        json!({"architecture":std::env::consts::ARCH})
    }
}

/// Clock and process-memory boundary. The selectors read the monotonic clock
/// and the current process; the unit tests script both.
trait Observer {
    fn now_us(&mut self) -> u64;
    fn memory(&mut self) -> Result<Memory>;
}
struct Native(Instant);
impl Observer for Native {
    fn now_us(&mut self) -> u64 {
        u64::try_from(self.0.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
    fn memory(&mut self) -> Result<Memory> {
        process_memory()
    }
}

#[derive(Debug, PartialEq, Serialize)]
struct Load {
    load_us: u64,
    before: Memory,
    after: Memory,
    execution_identity: (String, usize),
}
/// `S` is opaque here, so this function can open and identify a scorer but
/// cannot dispatch a forward.
fn observed_load<S>(
    observer: &mut impl Observer,
    open: impl FnOnce() -> Result<S>,
    identity: impl FnOnce(&S) -> Result<(String, usize)>,
) -> Result<(S, Load)> {
    let before = observer.memory()?;
    let started = observer.now_us();
    let scorer = open()?;
    let load_us = observer.now_us().saturating_sub(started);
    let after = observer.memory()?;
    let execution_identity = identity(&scorer)?;
    Ok((
        scorer,
        Load {
            load_us,
            before,
            after,
            execution_identity,
        },
    ))
}
/// Load, identify, drop and observe again. Zero forwards by construction.
fn load_only<S>(
    observer: &mut impl Observer,
    open: impl FnOnce() -> Result<S>,
    identity: impl FnOnce(&S) -> Result<(String, usize)>,
    reservation: u64,
) -> Result<Value> {
    let (scorer, load) = observed_load(observer, open, identity)?;
    drop(scorer);
    let after_drop = observer.memory()?;
    Ok(json!({
        "load": load,
        "after_drop": after_drop,
        "forwards": 0,
        "declared_reservation_bytes": reservation,
        "peak_within_declared_reservation": after_drop.peak_resident_bytes <= reservation,
    }))
}

#[derive(Debug, PartialEq, Serialize)]
struct Comparison {
    maximum_absolute_difference: f64,
    maximum_mass_difference: f64,
    winner: usize,
    reference_winner: usize,
    winner_agrees: bool,
}
fn winner(scores: &[f32]) -> usize {
    scores.iter().enumerate().fold(
        0,
        |best, (index, value)| if *value > scores[best] { index } else { best },
    )
}
fn softmax(scores: &[f32]) -> Vec<f64> {
    let peak = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let weights = scores
        .iter()
        .map(|score| (f64::from(*score) - f64::from(peak)).exp())
        .collect::<Vec<_>>();
    let total: f64 = weights.iter().sum();
    weights.into_iter().map(|weight| weight / total).collect()
}
fn compare(actual: &[f32], reference: &[f32]) -> Result<Comparison> {
    ensure!(
        !actual.is_empty()
            && actual.len() == reference.len()
            && actual
                .iter()
                .chain(reference)
                .all(|score| score.is_finite()),
        "finite score vectors of equal length required"
    );
    let maximum = |left: Vec<f64>, right: Vec<f64>| {
        left.iter()
            .zip(&right)
            .map(|(a, b)| (a - b).abs())
            .fold(0f64, f64::max)
    };
    let wide = |scores: &[f32]| scores.iter().map(|score| f64::from(*score)).collect();
    let (first, second) = (winner(actual), winner(reference));
    Ok(Comparison {
        maximum_absolute_difference: maximum(wide(actual), wide(reference)),
        maximum_mass_difference: maximum(softmax(actual), softmax(reference)),
        winner: first,
        reference_winner: second,
        winner_agrees: first == second,
    })
}

struct ReferenceCase {
    name: String,
    sha256: Sha256Digest,
    input: CompiledInput,
    reference: Vec<f32>,
}
fn reference_case(pin: &CasePin, bytes: &[u8]) -> Result<ReferenceCase> {
    ensure!(
        sha(bytes) == pin.sha256,
        "retained reference case bytes differ from their SHA-256 pin"
    );
    let document: Value = serde_json::from_slice(bytes)?;
    let result = &document["result"];
    let input: CompiledInput =
        serde_json::from_value(result["compiled"].clone()).context("retained compiled input")?;
    let reference: Vec<f32> =
        serde_json::from_value(result["original_binding"]["original_fp32"].clone())
            .context("retained original FP32 scores")?;
    ensure!(
        !pin.name.is_empty()
            && pin.name.len() <= 128
            && reference.len() == input.candidate_positions.len(),
        "retained reference name or cardinality differs from its input"
    );
    Ok(ReferenceCase {
        name: pin.name.clone(),
        sha256: pin.sha256,
        input,
        reference,
    })
}
fn reference_cases(pins: &[CasePin]) -> Result<Vec<ReferenceCase>> {
    ensure!(pins.len() <= 16, "reference roster bound");
    let mut names = BTreeSet::new();
    let mut cases = Vec::new();
    for pin in pins {
        ensure!(names.insert(&pin.name), "duplicate reference case name");
        cases.push(reference_case(pin, &bounded(&pin.path, 4 << 20)?)?);
    }
    Ok(cases)
}

fn summary(series: &ResourceSeries) -> Result<Value> {
    ensure!(
        series.complete && series.samples.len() == 20,
        "a timing summary requires the complete twenty-sample series"
    );
    let mut ordered = series
        .samples
        .iter()
        .map(|sample| sample.elapsed_us)
        .collect::<Vec<_>>();
    ordered.sort_unstable();
    Ok(json!({
        "samples": 20,
        "minimum_us": ordered[0],
        "median_us": (ordered[9] + ordered[10]) / 2,
        "median_rule": "mean of the two central samples, rounded down",
        "nearest_rank_p95_us": series.nearest_rank_p95_us,
        "maximum_us": series.maximum_us,
    }))
}
/// One warmup, then the policy's warm samples through Core `ResourceSeries`.
/// Each observation is retained before it can be refused, and the first
/// overrun or native failure ends the run without quantiles.
fn timed_forwards(
    observer: &mut impl Observer,
    policy: &ResourcePolicy,
    reservation: u64,
    mut forward: impl FnMut() -> Result<()>,
    mut retain: impl FnMut(&Value) -> Result<()>,
) -> (Value, Result<()>) {
    let mut timing = json!({"complete": false,
        "required_samples": policy.required_warm_samples,
        "deadline_us": policy.p95_limit_us,
        "declared_reservation_bytes": reservation});
    let outcome = (|| -> Result<()> {
        let started = observer.now_us();
        forward().context("warmup forward failed")?;
        let elapsed_us = observer.now_us().saturating_sub(started);
        let memory = observer.memory()?;
        timing["warmup"] = json!({"elapsed_us": elapsed_us, "memory": memory});
        retain(&timing)?;
        ensure!(
            elapsed_us <= policy.p95_limit_us && memory.peak_resident_bytes <= reservation,
            "warmup exceeded the deadline or the declared reservation; no measured forward admitted"
        );
        let mut series = ResourceSeries::new(reservation)?;
        for _ in 0..policy.required_warm_samples {
            series.begin_sample(policy)?;
            let started = observer.now_us();
            let returned = forward();
            let elapsed_us = observer.now_us().saturating_sub(started);
            let admitted = match returned {
                Ok(()) => {
                    let peak_process_rss_bytes = observer.memory()?.peak_resident_bytes;
                    series.finish_sample(
                        ResourceSample {
                            elapsed_us,
                            peak_process_rss_bytes,
                        },
                        policy,
                    )
                }
                Err(error) => {
                    series.fail_native(policy)?;
                    Err(error.context("measured forward failed"))
                }
            };
            timing["series"] = serde_json::to_value(&series)?;
            retain(&timing)?;
            admitted?;
        }
        series.require_complete(policy)?;
        timing["summary"] = summary(&series)?;
        timing["complete"] = json!(true);
        Ok(())
    })();
    (timing, outcome)
}

/// Classifies one profiled forward by where FP16 tensors appear. The portable
/// graph stores its learned weights as FP16 and computes in FP32.
/// - `folded_at_load`: no kernel touched an FP16 tensor, so every weight Cast
///   ran at load.
/// - `folded_at_load_except_gathered_rows`: the only FP16 weights left are
///   tables read by Gather, and the forward casts just the gathered rows.
/// - `run_per_forward`: a Cast converted a stored FP16 weight in the forward.
fn cast_census(profile: &[u8]) -> Result<Value> {
    let events: Vec<Value> =
        serde_json::from_slice(profile).context("native profile is not a JSON event array")?;
    let mut session: BTreeMap<String, u64> = BTreeMap::new();
    let mut operators: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut casts: BTreeMap<String, u64> = BTreeMap::new();
    let mut half_kernels: BTreeMap<String, u64> = BTreeMap::new();
    // Events and total microseconds of Casts from FP16 weights and from FP16 activations.
    let (mut stored, mut gathered) = ((0u64, 0u64), (0u64, 0u64));
    let (mut kernels, mut untyped) = (0u64, 0u64);
    for event in &events {
        let name = event["name"].as_str().unwrap_or_default();
        let duration = event["dur"].as_u64().unwrap_or(0);
        if event["cat"] == "Session" {
            *session.entry(name.to_owned()).or_default() += duration;
        }
        if event["cat"] != "Node" || !name.ends_with("_kernel_time") {
            continue;
        }
        kernels += 1;
        let arguments = &event["args"];
        let types = |field: &str| -> Option<Vec<String>> {
            arguments[field]
                .as_array()?
                .iter()
                .map(|item| {
                    let item = item.as_object()?;
                    (item.len() == 1).then(|| item.keys().next().cloned())?
                })
                .collect()
        };
        let (Some(operator), Some(inputs), Some(outputs), Some(parameter_bytes)) = (
            arguments["op_name"].as_str(),
            types("input_type_shape"),
            types("output_type_shape"),
            arguments["parameter_size"]
                .as_str()
                .and_then(|size| size.parse::<u64>().ok()),
        ) else {
            untyped += 1;
            continue;
        };
        let entry = operators.entry(operator.to_owned()).or_default();
        entry.0 += 1;
        entry.1 += duration;
        let half = |types: &[String]| types.iter().any(|name| name == "float16");
        if operator == "Cast" {
            *casts
                .entry(format!("{}->{}", inputs.join(","), outputs.join(",")))
                .or_default() += 1;
        }
        if operator == "Cast" && half(&inputs) {
            let counter = if parameter_bytes > 0 {
                &mut stored
            } else {
                &mut gathered
            };
            counter.0 += 1;
            counter.1 += duration;
        } else if half(&inputs) || half(&outputs) {
            *half_kernels.entry(operator.to_owned()).or_default() += 1;
        }
    }
    let weight_casts = if kernels == 0 || untyped > 0 {
        "indeterminate"
    } else if stored.0 > 0 {
        "run_per_forward"
    } else if gathered.0 == 0 && half_kernels.is_empty() {
        "folded_at_load"
    } else if !half_kernels.is_empty() && half_kernels.keys().all(|operator| operator == "Gather") {
        "folded_at_load_except_gathered_rows"
    } else {
        "indeterminate"
    };
    let operators = operators
        .into_iter()
        .map(|(name, (events, total_us))| (name, json!({"events": events, "total_us": total_us})))
        .collect::<BTreeMap<_, _>>();
    Ok(json!({
        "session_us": session,
        "kernel_events": kernels,
        "operators": operators,
        "casts": casts,
        "float16_weight_casts": {"events": stored.0, "total_us": stored.1},
        "float16_activation_casts": {"events": gathered.0, "total_us": gathered.1},
        "other_float16_kernels": half_kernels,
        "untyped_events": untyped,
        "weight_casts": weight_casts,
    }))
}

struct Receipt {
    file: File,
    value: Value,
}
impl Receipt {
    fn create(path: &Path, value: Value) -> Result<Self> {
        let file = OpenOptions::new().write(true).create_new(true).open(path)?;
        let mut receipt = Self { file, value };
        receipt.save()?;
        Ok(receipt)
    }
    fn save(&mut self) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(&self.value)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&bytes)?;
        self.file.set_len(bytes.len() as u64)?;
        self.file.sync_data()?;
        Ok(())
    }
    fn finish(mut self, result: Result<()>) -> Result<()> {
        self.value["complete"] = json!(result.is_ok());
        if let Err(error) = &result {
            self.value["error"] = format!("{error:#}").into();
        }
        self.save()?;
        result
    }
}

struct Admitted {
    input: TierInput,
    input_sha256: Sha256Digest,
    profile: Profile,
    files: Files,
}
fn admitted() -> Result<Admitted> {
    let (raw, input_sha256) = pinned("AIRK_PORTABLE_CPU_INPUT", 1 << 20)?;
    let input: TierInput = serde_json::from_slice(&raw)?;
    ensure!(input.schema == TIER_INPUT, "portable tier input required");
    let profile_bytes = bounded(&input.profile, 1 << 20)?;
    ensure!(
        sha(&profile_bytes) == input.profile_sha256,
        "Rust-emitted profile bytes changed"
    );
    let profile: Profile = serde_json::from_slice(&profile_bytes)?;
    profile.validate()?;
    unqualified_candidate(&profile, input.tier)?;
    ensure!(
        input.files.keys().collect::<BTreeSet<_>>()
            == profile
                .assets
                .iter()
                .map(|asset| &asset.name)
                .collect::<BTreeSet<_>>(),
        "exact portable asset map required"
    );
    let files = Files(input.files.clone());
    Ok(Admitted {
        input,
        input_sha256,
        profile,
        files,
    })
}
fn receipt_base(schema: &str, admitted: &Admitted) -> Result<Value> {
    let cores = airk_process::cpu_fit::usable_physical_cores()
        .context("usable physical-core observation unavailable")?;
    ensure!(
        cores == CORES,
        "the 8-core class needs exactly eight usable physical cores; cap the process affinity (observed {cores})"
    );
    let profile = &admitted.profile;
    Ok(json!({
        "schema": schema, "complete": false, "evaluation": true,
        "release_qualified": false, "qualification": null,
        "tier": admitted.input.tier, "model": profile.model,
        "profile": profile.manifest, "profile_id": profile.profile_id,
        "profile_source_sha256": admitted.input.profile_sha256,
        "input_sha256": admitted.input_sha256,
        "max_tokens": profile.max_tokens, "cpu_fit": profile.cpu_fit,
        "declared_reservation_bytes": profile.memory_reservation_bytes,
        "runtime": profile.runtime, "target": current_target(), "isa": isa(),
        "process": {"id": std::process::id(),
            "execution_user": airk_ipc::endpoint::current_user()?,
            "affinity_mask": affinity_mask().map(|mask| format!("{mask:#018x}")),
            "usable_physical_cores": cores},
        "native_binary_sha256": file_sha256(&std::env::current_exe()?)?,
        "scope": SCOPE,
    }))
}
fn native_owner() -> Result<(
    tempfile::TempDir,
    airk_kernel::inference_owner::NativeInferenceOwner,
)> {
    let (parent, control) = private_control()?;
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(Arc::new(
        airk_store::service::ServiceLease::acquire(&control)?,
    ))?;
    Ok((parent, owner))
}
/// The widest input the tier's own compiler admits: all live, eight options.
fn maximum_input(scorer: &Scorer, tokens: usize) -> Result<CompiledInput> {
    let options = (0..8)
        .map(|index| Candidate {
            id: format!("synthetic-option-{index}"),
            text: format!("Synthetic whole option {index}; no source or task-truth authority."),
        })
        .collect::<Vec<_>>();
    let (mut low, mut high) = (0usize, 16384usize);
    let mut widest = None;
    while low <= high {
        let middle = low + (high - low) / 2;
        match scorer.compile(
            &" source".repeat(middle),
            "Select one complete synthetic option.",
            &options,
        ) {
            Ok(input) => {
                widest = Some(input);
                low = middle + 1;
            }
            Err(_) => {
                if middle == 0 {
                    break;
                }
                high = middle - 1;
            }
        }
    }
    let input = widest.context("the tier compiler admitted no synthetic input")?;
    ensure!(
        input.input_ids.len() == tokens
            && input.candidate_positions.len() == 8
            && input.option_mask == [true; 8],
        "exact tier-ceiling all-live input required; no shorter substitution"
    );
    Ok(input)
}

#[test]
#[ignore = "pinned portable export, tokenizer, notice, fixture and reference cases plus a fresh private output; no model load"]
fn portable_cpu_emit_unqualified_profiles() -> Result<()> {
    let (raw, request_sha256) = pinned("AIRK_PORTABLE_CPU_REQUEST", 1 << 20)?;
    let request: EmitRequest = serde_json::from_slice(&raw)?;
    ensure!(
        request.schema == EMIT_REQUEST,
        "portable emit request required"
    );
    let output = private_output()?;
    let cases = reference_cases(&request.cases)?
        .iter()
        .map(|case| {
            json!({"name": case.name, "sha256": case.sha256,
                "tokens": case.input.input_ids.len(), "options": case.reference.len(),
                "fits_full": Tier::Full.admits(case.input.input_ids.len()),
                "fits_fast": Tier::Fast.admits(case.input.input_ids.len())})
        })
        .collect::<Vec<_>>();
    std::fs::create_dir(&output)?;
    let mut tiers = Vec::new();
    for tier in [Tier::Full, Tier::Fast] {
        let directory = output.join(tier.label());
        let input = ProfileInput {
            model: request.model,
            profile_id: format!("{}-{}", request.profile_id, tier.label()),
            assets: request
                .assets
                .iter()
                .map(|item| AssetInput {
                    asset: item.asset.clone(),
                    path: item.path.clone(),
                })
                .collect(),
            cpu_fit: cpu8(),
            memory_reservation_bytes: request.memory_reservation_bytes,
            peak_disk_bytes: request.peak_disk_bytes,
            maximum_candidates: 8,
            calibration: CalibrationInput::NumericRawOnly,
        };
        // The existing builder owns both widths: its software-v2 path emits the
        // 8,192-token compiler and its original path the 2,048-token one.
        let prepared = match tier {
            Tier::Full => prepare_with_limits(input, &ResourcePolicy::software_v2(), &directory)?,
            Tier::Fast => prepare(input, &directory)?,
        };
        unqualified_candidate(&prepared.profile, tier)?;
        let profile_path = directory.join("experimental-profile.json");
        let profile_sha256 = sha(&bounded(&profile_path, 1 << 20)?);
        let tier_bytes = serde_json::to_vec_pretty(&TierInput {
            schema: TIER_INPUT.into(),
            tier,
            profile: profile_path.clone(),
            profile_sha256,
            files: prepared.files.clone(),
            cases: request.cases.clone(),
        })?;
        let tier_path = directory.join("tier-input.private.json");
        std::fs::write(&tier_path, &tier_bytes)?;
        tiers.push(
            json!({"tier": tier, "max_tokens": prepared.profile.max_tokens,
            "profile_id": prepared.profile.profile_id, "manifest": prepared.profile.manifest,
            "profile": profile_path, "profile_sha256": profile_sha256,
            "compiler": prepared.profile.compiler, "calibration": prepared.profile.calibration,
            "execution_identity": cpu8().runtime_identity()?,
            "memory_reservation_bytes": prepared.profile.memory_reservation_bytes,
            "tier_input": tier_path, "tier_input_sha256": sha(&tier_bytes),
            "qualification": prepared.profile.qualification,
            "qualified_targets": prepared.profile.qualified_targets,
            "downloads": prepared.profile.downloads}),
        );
    }
    std::fs::write(
        output.join("emit.json"),
        serde_json::to_vec_pretty(&json!({"schema": "airk.private.portable-cpu-emit/1",
            "evaluation": true, "release_qualified": false, "qualification": null,
            "request_sha256": request_sha256, "model": request.model,
            "cases": cases, "tiers": tiers, "model_loads": 0, "forwards": 0,
            "scope": "Rust-owned private metadata and asset authentication only. No model load, fit, calibration, quality, platform, catalog or release claim."}))?,
    )?;
    Ok(())
}

#[test]
#[ignore = "emitted private tier input, an 8-core affinity cap and an exclusive native window; one load and zero forwards"]
fn portable_cpu_load_only_records_resources_with_zero_forwards() -> Result<()> {
    let admitted = admitted()?;
    let mut receipt = Receipt::create(
        &private_output()?,
        receipt_base("airk.private.portable-cpu-load/1", &admitted)?,
    )?;
    let (_parent, owner) = native_owner()?;
    let observed = owner
        .run(|| {
            load_only(
                &mut Native(Instant::now()),
                || {
                    Scorer::open(
                        ExecutionProfile::for_qualification(admitted.profile.clone())?,
                        &admitted.files,
                    )
                },
                Scorer::execution_identity,
                admitted.profile.memory_reservation_bytes,
            )
        })
        .and_then(std::convert::identity);
    let result = observed.map(|observed| {
        for (key, value) in observed.as_object().into_iter().flatten() {
            receipt.value[key.as_str()] = value.clone();
        }
    });
    receipt.finish(result)
}

#[test]
#[ignore = "emitted private tier input, retained references, an 8-core affinity cap and an exclusive native window"]
fn portable_cpu_score_references_timing_and_cast_profile() -> Result<()> {
    measured(true)
}

#[test]
#[ignore = "emitted private tier input, an 8-core affinity cap and a quiet exclusive native window; warmup plus twenty timed forwards only"]
fn portable_cpu_timing_only() -> Result<()> {
    measured(false)
}

/// `complete` adds the retained reference comparisons before the timed series
/// and one profiled open after it. Timing alone needs neither.
fn measured(complete: bool) -> Result<()> {
    let admitted = admitted()?;
    let output = private_output()?;
    let tier = admitted.input.tier;
    let (scored, beyond): (Vec<_>, Vec<_>) = if complete {
        reference_cases(&admitted.input.cases)?
            .into_iter()
            .partition(|case| tier.admits(case.input.input_ids.len()))
    } else {
        (vec![], vec![])
    };
    ensure!(
        !complete || !scored.is_empty(),
        "at least one retained reference case must fit the tier"
    );
    let schema = if complete {
        "airk.private.portable-cpu-score/1"
    } else {
        "airk.private.portable-cpu-timing/1"
    };
    let mut receipt = Receipt::create(&output, receipt_base(schema, &admitted)?)?;
    receipt.value["cases_beyond_tier"] = beyond
        .iter()
        .map(|case| {
            json!({"name": case.name, "tokens": case.input.input_ids.len(),
                "scored": false, "reason": "exceeds the tier token ceiling"})
        })
        .collect();
    let policy = ResourcePolicy::software_v2();
    let reservation = admitted.profile.memory_reservation_bytes;
    let execution = || ExecutionProfile::for_qualification(admitted.profile.clone());
    let mut observer = Native(Instant::now());
    let (_parent, owner) = native_owner()?;
    let result = owner
        .run(|| -> Result<()> {
            let (scorer, load) = observed_load(
                &mut observer,
                || Scorer::open(execution()?, &admitted.files),
                Scorer::execution_identity,
            )?;
            receipt.value["load"] = serde_json::to_value(&load)?;
            receipt.save()?;
            let mut within_screen = true;
            let mut rows = Vec::new();
            for case in &scored {
                let started = observer.now_us();
                let actual = scorer.score(&case.input)?;
                let elapsed_us = observer.now_us().saturating_sub(started);
                let comparison = compare(&actual, &case.reference)?;
                within_screen &= comparison.winner_agrees
                    && comparison.maximum_absolute_difference <= RAW_SCREEN;
                rows.push(json!({"name": case.name, "source_sha256": case.sha256,
                    "input_digest": case.input.input_digest,
                    "tokens": case.input.input_ids.len(), "options": case.reference.len(),
                    "elapsed_us": elapsed_us, "raw_scores": actual,
                    "reference_original_fp32": case.reference, "comparison": comparison,
                    "memory": observer.memory()?}));
                receipt.value["cases"] = json!(rows);
                receipt.save()?;
            }
            if complete {
                receipt.value["reference_screen"] = json!({"raw_difference_limit": RAW_SCREEN,
                    "within_screen": within_screen,
                    "rule": "development screen against the retained original FP32 scores; winner must agree; not a qualification gate"});
            }
            let widest = maximum_input(&scorer, admitted.profile.max_tokens)?;
            receipt.value["timing_input"] = json!({"tokens": widest.input_ids.len(),
                "options": widest.candidate_positions.len(), "input_digest": widest.input_digest,
                "source": "native compiler, widest all-live synthetic input at the tier ceiling"});
            let (timing, outcome) = timed_forwards(
                &mut observer,
                &policy,
                reservation,
                || scorer.score(&widest).map(drop),
                |timing| {
                    receipt.value["timing"] = timing.clone();
                    receipt.save()
                },
            );
            receipt.value["timing"] = timing;
            receipt.save()?;
            outcome?;
            drop(scorer);
            receipt.value["after_drop"] = serde_json::to_value(observer.memory()?)?;
            if let Some(case) = scored.iter().min_by_key(|case| case.input.input_ids.len()) {
                let prefix = output.with_extension("ort-profile");
                let (profiled, load) = observed_load(
                    &mut observer,
                    || Scorer::open_profiled(execution()?, &admitted.files, &prefix),
                    Scorer::execution_identity,
                )?;
                profiled.score(&case.input)?;
                let path = profiled.finish_profiling()?;
                drop(profiled);
                let events = bounded(&path, 256 << 20)?;
                receipt.value["cast_profile"] = json!({"load": load, "case": case.name,
                    "tokens": case.input.input_ids.len(), "forwards": 1,
                    "native_profile": path, "native_profile_sha256": sha(&events),
                    "census": cast_census(&events)?,
                    "after_drop": observer.memory()?});
            }
            ensure!(
                within_screen,
                "a retained reference case left the development screen; every observation is retained"
            );
            Ok(())
        })
        .and_then(std::convert::identity);
    receipt.finish(result)
}

struct Scripted {
    clock: std::collections::VecDeque<u64>,
    memory: std::collections::VecDeque<Memory>,
    dropped: std::rc::Rc<std::cell::Cell<bool>>,
    dropped_at_memory: Vec<bool>,
}
impl Scripted {
    fn new(clock: &[u64], memory: &[Memory]) -> Self {
        Self {
            clock: clock.iter().copied().collect(),
            memory: memory.iter().copied().collect(),
            dropped: Default::default(),
            dropped_at_memory: vec![],
        }
    }
}
impl Observer for Scripted {
    fn now_us(&mut self) -> u64 {
        self.clock.pop_front().expect("scripted clock exhausted")
    }
    fn memory(&mut self) -> Result<Memory> {
        self.dropped_at_memory.push(self.dropped.get());
        self.memory.pop_front().context("scripted memory exhausted")
    }
}
fn observed(resident: u64, peak: u64) -> Memory {
    Memory {
        resident_bytes: Some(resident),
        peak_resident_bytes: peak,
        private_commit_bytes: None,
        peak_private_commit_bytes: None,
    }
}
struct Witness(std::rc::Rc<std::cell::Cell<bool>>);
impl Drop for Witness {
    fn drop(&mut self) {
        self.0.set(true);
    }
}
/// Clock readings for a warmup of `warmup` and then one sample per duration.
fn clock(warmup: u64, durations: &[u64]) -> Vec<u64> {
    let mut now = 0;
    let mut readings = vec![];
    for duration in std::iter::once(&warmup).chain(durations) {
        readings.push(now);
        now += duration;
        readings.push(now);
        now += 3;
    }
    readings
}

#[test]
fn portable_cpu_load_only_reports_scripted_load_drop_and_zero_forwards() -> Result<()> {
    let mut observer = Scripted::new(
        &[1_000, 4_500],
        &[
            observed(100, 150),
            observed(900, 1_200),
            observed(300, 1_200),
        ],
    );
    let dropped = observer.dropped.clone();
    let receipt = load_only(
        &mut observer,
        || Ok(Witness(dropped)),
        |_| Ok(("scripted-identity".into(), 8)),
        1_000,
    )?;
    let memory = |resident: u64, peak: u64| {
        json!({"resident_bytes": resident, "peak_resident_bytes": peak,
            "private_commit_bytes": null, "peak_private_commit_bytes": null})
    };
    assert_eq!(
        receipt,
        json!({"load": {"load_us": 3_500, "before": memory(100, 150),
                "after": memory(900, 1_200), "execution_identity": ["scripted-identity", 8]},
            "after_drop": memory(300, 1_200), "forwards": 0,
            "declared_reservation_bytes": 1_000, "peak_within_declared_reservation": false})
    );
    // The scorer is still open at the second observation and gone at the third.
    assert_eq!(observer.dropped_at_memory, [false, false, true]);
    let mut fits = Scripted::new(
        &[0, 1],
        &[observed(1, 1), observed(2, 1_000), observed(1, 1_000)],
    );
    let receipt = load_only(&mut fits, || Ok(()), |_| Ok((String::new(), 8)), 1_000)?;
    assert_eq!(receipt["peak_within_declared_reservation"], true);
    Ok(())
}

#[test]
fn portable_cpu_load_failure_is_reported_verbatim_without_later_observations() {
    let mut observer = Scripted::new(&[7, 9], &[observed(1, 1), observed(2, 2)]);
    let error = load_only(
        &mut observer,
        || -> Result<()> { anyhow::bail!("no kernel for GroupQueryAttention") },
        |_| Ok((String::new(), 8)),
        1,
    )
    .unwrap_err();
    assert_eq!(error.to_string(), "no kernel for GroupQueryAttention");
    assert_eq!(observer.memory.len(), 1);
    assert_eq!(observer.clock.len(), 1);
}

#[test]
fn portable_cpu_timing_reports_median_and_nearest_rank_p95_of_twenty_samples() -> Result<()> {
    let durations = [
        20, 1, 19, 2, 18, 3, 17, 4, 16, 5, 15, 6, 14, 7, 13, 8, 12, 9, 11, 10,
    ];
    let mut observer = Scripted::new(&clock(50, &durations), &[observed(400, 500); 21]);
    let (mut forwards, mut retained) = (0, 0);
    let (timing, outcome) = timed_forwards(
        &mut observer,
        &ResourcePolicy::software_v2(),
        1_000,
        || {
            forwards += 1;
            Ok(())
        },
        |_| {
            retained += 1;
            Ok(())
        },
    );
    outcome?;
    assert_eq!((forwards, retained), (21, 21));
    assert_eq!(timing["complete"], true);
    assert_eq!(timing["warmup"]["elapsed_us"], 50);
    assert_eq!(
        timing["summary"],
        json!({"samples": 20, "minimum_us": 1, "median_us": 10,
            "median_rule": "mean of the two central samples, rounded down",
            "nearest_rank_p95_us": 19, "maximum_us": 20})
    );
    assert_eq!(timing["series"]["samples"][0]["elapsed_us"], 20);
    assert_eq!(timing["series"]["samples"][19]["elapsed_us"], 10);
    Ok(())
}

#[test]
fn portable_cpu_timing_stops_at_the_first_overrun_or_failure_without_quantiles() {
    let policy = ResourcePolicy::software_v2();
    let mut forwards = 0;
    let mut observer = Scripted::new(&clock(5, &[7, 120_000_001]), &[observed(1, 500); 3]);
    let (timing, outcome) = timed_forwards(
        &mut observer,
        &policy,
        1_000,
        || {
            forwards += 1;
            Ok(())
        },
        |_| Ok(()),
    );
    assert!(outcome.is_err());
    assert_eq!(forwards, 3);
    assert_eq!(timing["complete"], false);
    assert_eq!(timing["series"]["attempted_samples"], 2);
    assert_eq!(timing["series"]["samples"][1]["elapsed_us"], 120_000_001);
    assert_eq!(timing["series"]["failure"], "deadline_overrun");
    assert_eq!(timing["series"]["nearest_rank_p95_us"], Value::Null);
    assert!(timing.get("summary").is_none());

    let mut forwards = 0;
    let mut observer = Scripted::new(&clock(5, &[]), &[observed(1, 1_001)]);
    let (timing, outcome) = timed_forwards(
        &mut observer,
        &policy,
        1_000,
        || {
            forwards += 1;
            Ok(())
        },
        |_| Ok(()),
    );
    assert!(outcome.is_err());
    assert_eq!(forwards, 1);
    assert_eq!(timing["warmup"]["memory"]["peak_resident_bytes"], 1_001);
    assert!(timing.get("series").is_none());

    let mut forwards = 0;
    let mut observer = Scripted::new(&clock(5, &[7, 8, 9]), &[observed(1, 500); 3]);
    let (timing, outcome) = timed_forwards(
        &mut observer,
        &policy,
        1_000,
        || {
            forwards += 1;
            ensure!(forwards < 4, "scripted native failure");
            Ok(())
        },
        |_| Ok(()),
    );
    assert_eq!(
        format!("{:#}", outcome.unwrap_err()),
        "measured forward failed: scripted native failure"
    );
    assert_eq!(timing["series"]["attempted_samples"], 3);
    assert_eq!(
        timing["series"]["samples"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(timing["series"]["failure"], "native_failure");
    assert!(timing.get("summary").is_none());
}

#[test]
fn portable_cpu_comparison_reports_literal_differences_and_winner_agreement() -> Result<()> {
    let near = compare(&[1.0, 3.0, 2.0], &[1.25, 2.5, 2.0])?;
    assert_eq!(near.maximum_absolute_difference, 0.5);
    assert_eq!((near.winner, near.reference_winner), (1, 1));
    assert!(near.winner_agrees);
    // Softmax of [0, 0] is [1/2, 1/2]; of [0, ln 3] it is [1/4, 3/4].
    let moved = compare(&[0.0, 0.0], &[0.0, 3f32.ln()])?;
    assert!((moved.maximum_mass_difference - 0.25).abs() < 1e-6);
    assert!((moved.maximum_absolute_difference - 1.098_612_3).abs() < 1e-6);
    assert_eq!((moved.winner, moved.reference_winner), (0, 1));
    assert!(!moved.winner_agrees);
    assert!(compare(&[1.0], &[1.0, 2.0]).is_err());
    assert!(compare(&[f32::NAN], &[1.0]).is_err());
    assert!(compare(&[], &[]).is_err());
    Ok(())
}

#[test]
fn portable_cpu_reference_case_requires_its_pin_and_matching_cardinality() -> Result<()> {
    let case = |scores: Value| {
        serde_json::to_vec(&json!({"result": {"compiled": {"prompt": "",
            "input_ids": [5, 6, 7], "attention_mask": [1, 1, 1], "position_ids": [0, 1, 2],
            "decision_position": 2, "candidate_positions": [32, 33], "option_mask": [true, true],
            "input_digest": "11".repeat(32)},
            "original_binding": {"original_fp32": scores}}}))
    };
    let bytes = case(json!([0.5, -1.5]))?;
    let pin = CasePin {
        name: "tiny".into(),
        path: PathBuf::new(),
        sha256: sha(&bytes),
    };
    let parsed = reference_case(&pin, &bytes)?;
    assert_eq!(parsed.name, "tiny");
    assert_eq!(parsed.input.input_ids, [5, 6, 7]);
    assert_eq!(parsed.reference, [0.5, -1.5]);
    let other = case(json!([0.5, -1.5, 2.0]))?;
    assert!(reference_case(&pin, &other).is_err());
    let relabelled = CasePin {
        sha256: sha(&other),
        ..pin.clone()
    };
    assert!(reference_case(&relabelled, &other).is_err());
    assert!(Tier::Fast.admits(2_048) && !Tier::Fast.admits(2_049));
    assert!(Tier::Full.admits(8_192) && !Tier::Full.admits(8_193));
    Ok(())
}

#[test]
fn portable_cpu_cast_census_separates_folded_gathered_per_forward_and_indeterminate() -> Result<()>
{
    // Arguments: node, operator, microseconds, parameter bytes, input types, output types.
    let kernel = |name: &str, operator: &str, us: u64, bytes: u64, input: Value, output: Value| {
        json!({"cat": "Node", "name": format!("{name}_kernel_time"), "dur": us,
            "args": {"op_name": operator, "provider": "CPUExecutionProvider",
                "parameter_size": bytes.to_string(),
                "input_type_shape": input, "output_type_shape": output}})
    };
    let single = |element: &str| json!([{element: [1, 3, 4]}]);
    let product = kernel(
        "/layer/MatMul",
        "MatMul",
        7,
        64,
        json!([{"float": [1, 3, 4]}, {"float": [4, 4]}]),
        single("float"),
    );
    let index_cast = kernel(
        "/positions/Cast",
        "Cast",
        2,
        0,
        single("int64"),
        single("float"),
    );
    let run = json!({"cat": "Session", "name": "model_run", "dur": 9, "args": {}});
    let fence = json!({"cat": "Node", "name": "/layer/MatMul_fence_before", "dur": 0,
        "args": {"op_name": "MatMul"}});
    let census = |events: Vec<Value>| cast_census(&serde_json::to_vec(&events)?);
    assert_eq!(
        census(vec![run, fence, product.clone(), index_cast])?,
        json!({"session_us": {"model_run": 9}, "kernel_events": 2,
            "operators": {"Cast": {"events": 1, "total_us": 2},
                "MatMul": {"events": 1, "total_us": 7}},
            "casts": {"int64->float": 1},
            "float16_weight_casts": {"events": 0, "total_us": 0},
            "float16_activation_casts": {"events": 0, "total_us": 0},
            "other_float16_kernels": {}, "untyped_events": 0,
            "weight_casts": "folded_at_load"})
    );
    // A table read by Gather stays FP16 and only its gathered rows are cast.
    let gather = kernel(
        "/embed/Gather",
        "Gather",
        4,
        80,
        json!([{"float16": [10, 4]}, {"int64": [1, 3]}]),
        single("float16"),
    );
    let row_cast = kernel(
        "/embed/Cast",
        "Cast",
        5,
        0,
        single("float16"),
        single("float"),
    );
    let rows = census(vec![product.clone(), gather.clone(), row_cast.clone()])?;
    assert_eq!(rows["weight_casts"], "folded_at_load_except_gathered_rows");
    assert_eq!(
        rows["float16_activation_casts"],
        json!({"events": 1, "total_us": 5})
    );
    assert_eq!(
        rows["float16_weight_casts"],
        json!({"events": 0, "total_us": 0})
    );
    assert_eq!(rows["other_float16_kernels"], json!({"Gather": 1}));
    let weight_cast = kernel(
        "/weight/Cast",
        "Cast",
        11,
        96,
        single("float16"),
        single("float"),
    );
    let per_forward = census(vec![product.clone(), gather, row_cast.clone(), weight_cast])?;
    assert_eq!(per_forward["weight_casts"], "run_per_forward");
    assert_eq!(
        per_forward["float16_weight_casts"],
        json!({"events": 1, "total_us": 11})
    );
    assert_eq!(per_forward["casts"], json!({"float16->float": 2}));
    let half_product = kernel(
        "/half/MatMul",
        "MatMul",
        1,
        96,
        single("float16"),
        single("float16"),
    );
    let old_format = kernel(
        "/old/MatMul",
        "MatMul",
        1,
        0,
        json!("float"),
        json!("float"),
    );
    let mut unmeasured = product.clone();
    unmeasured["args"]
        .as_object_mut()
        .context("kernel arguments")?
        .remove("parameter_size");
    for events in [
        vec![],
        vec![product.clone(), half_product],
        vec![product.clone(), old_format],
        vec![product.clone(), unmeasured],
        vec![product, row_cast],
    ] {
        assert_eq!(census(events)?["weight_casts"], "indeterminate");
    }
    assert!(cast_census(b"not a profile").is_err());
    Ok(())
}

#[test]
fn portable_cpu_admits_only_private_unqualified_cpu8_context_candidates() -> Result<()> {
    let zero = "00".repeat(32);
    let candidate = |max_tokens: usize| -> Result<Profile> {
        Ok(serde_json::from_value(json!({"stage": "context",
            "cpu_fit": {"minimum_physical_cores": 8, "intra_threads": 8, "inter_threads": 1,
                "sequential": true, "spinning": false},
            "model": "arbiter-context-qwen3-17b", "profile_id": "private-portable-test",
            "manifest": zero, "runtime": "onnxruntime-cpu/1.30.0", "compiler": zero,
            "calibration": zero, "policy": zero, "qualification": null,
            "tasks": ["airk.context_action.v1"],
            "abi": {"readout": "decision_symbol", "inputs": [],
                "output": {"name": "scores", "element": "float32", "axes": ["batch_1", "options"]}},
            "max_tokens": max_tokens, "max_candidates": 8,
            "memory_reservation_bytes": 1, "peak_disk_bytes": 1, "assets": []}))?)
    };
    unqualified_candidate(&candidate(8_192)?, Tier::Full)?;
    unqualified_candidate(&candidate(2_048)?, Tier::Fast)?;
    assert!(unqualified_candidate(&candidate(8_192)?, Tier::Fast).is_err());
    assert!(unqualified_candidate(&candidate(2_048)?, Tier::Full).is_err());
    let refused = |change: fn(&mut Profile)| -> Result<bool> {
        let mut profile = candidate(8_192)?;
        change(&mut profile);
        Ok(unqualified_candidate(&profile, Tier::Full).is_err())
    };
    assert!(refused(|profile| {
        profile.qualification = Some(Sha256Digest::from_bytes([1; 32]))
    })?);
    assert!(refused(|profile| {
        profile
            .qualified_targets
            .push(airk_core::arbiter::QualifiedTarget {
                target: "x86_64-pc-windows-msvc".into(),
                execution_identity: String::new(),
                receipt: Sha256Digest::from_bytes([2; 32]),
            })
    })?);
    assert!(refused(|profile| {
        profile.downloads.push(airk_core::arbiter::AssetDownload {
            asset_name: "model.onnx".into(),
            parts: vec![],
        })
    })?);
    assert!(refused(|profile| {
        profile.profile_id = "unqualified-local-test-portable".into()
    })?);
    assert!(refused(|profile| {
        profile.cpu_fit = Some(CpuFit::default())
    })?);
    assert!(refused(|profile| {
        profile.model = ModelChoice::FocusQwenReranker06B
    })?);
    Ok(())
}
