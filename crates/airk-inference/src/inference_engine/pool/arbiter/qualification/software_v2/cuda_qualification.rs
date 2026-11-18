// SPDX-License-Identifier: Apache-2.0
//! Explicit component probes. No product admission or qualified GPU fit follows.
use super::*;
use airk_core::{
    arbiter::device::GpuProbeReservation,
    identity::Incarnation,
    service_resources::{ResourceConfiguration, ResourcePreset},
};
use airk_retrieval::arbiter::runtime::cuda::{self, DeviceCapacityPlan, RuntimeAssemblyInput};
use std::time::{Duration, Instant};

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Assembly {
        schema: String,
        assembly: RuntimeAssemblyInput,
        deadline_ms: u64,
    },
    PackageProjection {
        schema: String,
        directory: PathBuf,
        descriptor: ResourceFilePin,
        deadline_ms: u64,
    },
    Prepare {
        schema: String,
        cpu_input: ResourceFilePin,
        runtime_manifest: ResourceFilePin,
        bounds: GpuProbeReservation,
        cold_bytes: u64,
        warm_bytes: u64,
        headroom_bytes: u64,
        job_limit_bytes: u64,
    },
    Probe {
        schema: String,
        gpu_input: ResourceFilePin,
        runtime_manifest: ResourceFilePin,
        bounds: GpuProbeReservation,
        cold_bytes: u64,
        warm_bytes: u64,
        headroom_bytes: u64,
        job_limit_bytes: u64,
    },
}
const SCHEMA: &str = "airk.private.cuda-component-probe-request/1";
fn text(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("missing {key}"))
}
fn number(key: &str) -> Result<u64> {
    Ok(text(key)?.parse()?)
}
fn env_pin(path: &str, hash: &str) -> Result<ResourceFilePin> {
    let pin = ResourceFilePin {
        path: text(path)?,
        sha256: text(hash)?.parse()?,
    };
    pin.validate()?;
    Ok(pin)
}
fn list(key: &str) -> Result<Vec<String>> {
    let value = text(key)?;
    if value.is_empty() {
        return Ok(vec![]);
    }
    let rows = value.split(';').map(str::to_owned).collect::<Vec<_>>();
    ensure!(
        rows.len() <= 256 && rows.iter().all(|r| !r.is_empty()),
        "native request name-list bound"
    );
    Ok(rows)
}
fn output() -> Result<PathBuf> {
    Ok(PathBuf::from(text("AIRK_CUDA_QUALIFICATION_OUTPUT")?))
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<ResourceFilePin> {
    ensure!(bytes.len() <= 16 << 20, "CUDA component output cap");
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(ResourceFilePin {
        path: path.to_str().context("native output UTF8 path")?.into(),
        sha256: sha(bytes),
    })
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<ResourceFilePin> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_new(path, &bytes)
}
fn request() -> Result<(Request, ResourceFilePin)> {
    let pin = env_pin(
        "AIRK_CUDA_QUALIFICATION_INPUT",
        "AIRK_CUDA_QUALIFICATION_INPUT_SHA256",
    )?;
    let raw = pinned(&pin, 1 << 20)?;
    Ok((serde_json::from_slice(&raw)?, pin))
}
fn bounds_validate(
    bounds: &GpuProbeReservation,
    cold: u64,
    warm: u64,
    headroom: u64,
    job: u64,
    profile_peak: u64,
) -> Result<DeviceCapacityPlan> {
    bounds.validate()?;
    ensure!(
        bounds.host_limit_bytes >= profile_peak
            && job >= bounds.host_limit_bytes
            && job
                <= bounds
                    .host_limit_bytes
                    .checked_add(8 << 30)
                    .context("Job limit overflow")?,
        "original explicit component host/Job bounds differ"
    );
    ensure!(
        cold.checked_add(headroom)
            .is_some_and(|v| v <= bounds.device_limit_bytes),
        "device planning bound exceeds explicit reservation"
    );
    DeviceCapacityPlan::probe(cold, warm, headroom)
}
fn compact_probe_limits(
    bounds: &GpuProbeReservation,
    cold: u64,
    warm: u64,
    headroom: u64,
    job: u64,
) -> bool {
    let host_and_job = (bounds.host_limit_bytes, job);
    host_and_job == (16 << 30, 24 << 30)
        || (cfg!(all(windows, target_arch = "x86_64"))
            && host_and_job == (32 << 30, 32 << 30)
            && bounds.device_limit_bytes == 24 << 30
            && bounds.deadline_ms == 120_000
            && (cold, warm, headroom) == (16 << 30, 8 << 30, 2 << 30))
}
fn profile_probe_bounds(
    bounds: &GpuProbeReservation,
    cold: u64,
    warm: u64,
    headroom: u64,
    job: u64,
    profile: &Profile,
) -> Result<DeviceCapacityPlan> {
    if profile.model == ModelChoice::ContextQwenCompact {
        ensure!(
            compact_probe_limits(bounds, cold, warm, headroom, job),
            "Compact probe host/Job and device bounds differ"
        );
    }
    bounds_validate(
        bounds,
        cold,
        warm,
        headroom,
        job,
        profile.memory_reservation_bytes,
    )
}
#[test]
fn compact_probe_host_limits_preserve_legacy_and_bind_windows_expansion() {
    let mut bounds = GpuProbeReservation {
        schema: "airk.arbiter.gpu-probe-reservation/1".into(),
        host_limit_bytes: 16 << 30,
        device_limit_bytes: 24 << 30,
        deadline_ms: 120_000,
    };
    assert!(compact_probe_limits(
        &bounds,
        16 << 30,
        8 << 30,
        2 << 30,
        24 << 30
    ));
    assert!(!compact_probe_limits(
        &bounds,
        16 << 30,
        8 << 30,
        2 << 30,
        32 << 30
    ));
    bounds.host_limit_bytes = 32 << 30;
    assert_eq!(
        compact_probe_limits(&bounds, 16 << 30, 8 << 30, 2 << 30, 32 << 30),
        cfg!(all(windows, target_arch = "x86_64"))
    );
    for (cold, warm, headroom, job) in [
        (16 << 30, 8 << 30, 2 << 30, 24 << 30),
        (15 << 30, 8 << 30, 2 << 30, 32 << 30),
        (16 << 30, 7 << 30, 2 << 30, 32 << 30),
        (16 << 30, 8 << 30, 1 << 30, 32 << 30),
    ] {
        assert!(!compact_probe_limits(&bounds, cold, warm, headroom, job));
    }
    bounds.device_limit_bytes = 32 << 30;
    assert!(!compact_probe_limits(
        &bounds,
        16 << 30,
        8 << 30,
        2 << 30,
        32 << 30
    ));
    bounds.device_limit_bytes = 24 << 30;
    bounds.deadline_ms = 119_999;
    assert!(!compact_probe_limits(
        &bounds,
        16 << 30,
        8 << 30,
        2 << 30,
        32 << 30
    ));
}
fn final_host_peak(peak: Option<u64>, limit: u64) -> Result<u64> {
    let value = peak.context("actual final persistent host RSS peak unavailable")?;
    ensure!(
        value > 0 && value <= limit,
        "final persistent host RSS peak exceeds original bound or is invalid"
    );
    Ok(value)
}
#[derive(Clone, Copy)]
enum CaseKind {
    Context { index: usize, tokens: usize },
    Focus512,
}
fn selected_rows(
    kind: CaseKind,
    profile: &Profile,
    fixture: &NumericFixture,
) -> Result<Vec<usize>> {
    match kind {
        CaseKind::Context { index, tokens } => {
            ensure!(
                matches!(
                    profile.model,
                    ModelChoice::ContextQwenCompact | ModelChoice::ContextQwenLarge
                ),
                "Context selector cannot consume Focus"
            );
            ensure!(
                fixture.rows.len() == 3,
                "literal native Context roster differs"
            );
            let row = fixture
                .rows
                .get(index)
                .context("literal native Context row absent")?;
            ensure!(
                row.input.input_ids.len() == tokens,
                "literal native Context row differs"
            );
            if tokens == 8192 {
                ensure!(
                    row.input.attention_mask.len() == tokens
                        && row.input.attention_mask.iter().all(|&token| token == 1)
                        && row.input.candidate_positions.len() == 8,
                    "unchanged all-live Context 8192/N8 row required"
                );
            }
            Ok(vec![index])
        }
        CaseKind::Focus512 => {
            ensure!(
                profile.model == ModelChoice::FocusQwenReranker06B
                    && fixture.rows.len() == 12
                    && fixture.rows.iter().all(|r| r.input.input_ids.len() == 512),
                "full twelve-case static512 Focus roster required"
            );
            Ok((0..12).collect())
        }
    }
}
#[cfg(windows)]
fn job_limits(host: u64, job: u64) -> Result<serde_json::Value> {
    use windows_sys::Win32::System::JobObjects::*;
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        QueryInformationJobObject(
            std::ptr::null_mut(),
            JobObjectExtendedLimitInformation,
            (&mut info as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&info) as u32,
            std::ptr::null_mut(),
        )
    };
    ensure!(ok != 0, "actual outer Job information unavailable");
    let flags = info.BasicLimitInformation.LimitFlags;
    ensure!(
        flags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE != 0
            && flags & JOB_OBJECT_LIMIT_PROCESS_MEMORY != 0
            && flags & JOB_OBJECT_LIMIT_JOB_MEMORY != 0
            && info.ProcessMemoryLimit as u64 == host
            && info.JobMemoryLimit as u64 == job,
        "actual owned-process memory/Job bounds differ from reviewed request"
    );
    Ok(
        json!({"process_memory_limit_bytes":info.ProcessMemoryLimit,"job_memory_limit_bytes":info.JobMemoryLimit,"flags":flags}),
    )
}
#[cfg(not(windows))]
fn job_limits(_: u64, _: u64) -> Result<Value> {
    anyhow::bail!("CUDA component probe requires real Windows Job limits")
}
struct Custody<'a> {
    scorer: Option<Scorer>,
    device: Option<Arc<cuda::CudaDevice>>,
    owner: Arc<airk_kernel::inference_owner::NativeInferenceOwner>,
    lease: Arc<airk_store::service::ServiceLease>,
    charge: Option<Arc<airk_kernel::service_resources::OptionalReservation>>,
    control: &'a mut Option<tempfile::TempDir>,
    files: Option<Files>,
}
impl Drop for Custody<'_> {
    fn drop(&mut self) {
        drop(self.scorer.take());
        if cuda::process_poisoned() || self.device.as_ref().is_some_and(|d| d.poisoned()) {
            std::mem::forget(self.owner.clone());
            std::mem::forget(self.lease.clone());
            if let Some(value) = self.charge.take() {
                std::mem::forget(value);
            }
            if let Some(value) = self.control.take() {
                std::mem::forget(value);
            }
            if let Some(value) = self.files.take() {
                std::mem::forget(value);
            }
        }
    }
}

#[test]
#[ignore = "metadata only: actual reviewed paths/pins/scalars required; emits owning Rust request DTO"]
fn emit_cuda_component_probe_request() -> Result<()> {
    let mode = text("AIRK_CUDA_REQUEST_KIND")?;
    let value = if mode == "assembly" {
        Request::Assembly {
            schema: SCHEMA.into(),
            assembly: RuntimeAssemblyInput {
                directory: PathBuf::from(text("AIRK_CUDA_RUNTIME_ROOT")?),
                producer_name: text("AIRK_CUDA_PRODUCER_NAME")?,
                producer_sha256: text("AIRK_CUDA_PRODUCER_SHA256")?.parse()?,
                checkpointed_proof: match std::env::var_os("AIRK_CUDA_CHECKPOINTED_PROOF") {
                    None => {
                        ensure!(
                            std::env::var_os("AIRK_CUDA_CHECKPOINTED_PROOF_SHA256").is_none(),
                            "checkpoint proof hash requires explicit proof path"
                        );
                        None
                    }
                    Some(_) => Some(env_pin(
                        "AIRK_CUDA_CHECKPOINTED_PROOF",
                        "AIRK_CUDA_CHECKPOINTED_PROOF_SHA256",
                    )?),
                },
                incremental_proof: match std::env::var_os("AIRK_CUDA_INCREMENTAL_PROOF") {
                    None => {
                        ensure!(
                            std::env::var_os("AIRK_CUDA_INCREMENTAL_PROOF_SHA256").is_none(),
                            "incremental proof hash requires explicit proof path"
                        );
                        None
                    }
                    Some(_) => {
                        ensure!(
                            std::env::var_os("AIRK_CUDA_CHECKPOINTED_PROOF").is_none(),
                            "conflicting producer proof modes"
                        );
                        Some(env_pin(
                            "AIRK_CUDA_INCREMENTAL_PROOF",
                            "AIRK_CUDA_INCREMENTAL_PROOF_SHA256",
                        )?)
                    }
                },
                custody_copies: match std::env::var_os("AIRK_CUDA_METADATA_CUSTODY") {
                    None => {
                        ensure!(
                            std::env::var_os("AIRK_CUDA_METADATA_CUSTODY_SHA256").is_none(),
                            "custody hash requires explicit native custody input"
                        );
                        Vec::new()
                    }
                    Some(_) => cuda::decode_metadata_custody(&pinned(
                        &env_pin(
                            "AIRK_CUDA_METADATA_CUSTODY",
                            "AIRK_CUDA_METADATA_CUSTODY_SHA256",
                        )?,
                        1 << 20,
                    )?)?,
                },
                historical_tool_copies: match std::env::var_os("AIRK_CUDA_HISTORICAL_TOOL_CUSTODY")
                {
                    None => {
                        ensure!(
                            std::env::var_os("AIRK_CUDA_HISTORICAL_TOOL_CUSTODY_SHA256").is_none(),
                            "historical custody hash requires native input"
                        );
                        Vec::new()
                    }
                    Some(_) => cuda::decode_historical_tool_custody(&pinned(
                        &env_pin(
                            "AIRK_CUDA_HISTORICAL_TOOL_CUSTODY",
                            "AIRK_CUDA_HISTORICAL_TOOL_CUSTODY_SHA256",
                        )?,
                        1 << 20,
                    )?)?,
                },
                module_names: list("AIRK_CUDA_MODULE_NAMES")?,
                load_order: list("AIRK_CUDA_LOAD_ORDER")?,
                system_imports: list("AIRK_CUDA_SYSTEM_IMPORTS")?.into_iter().collect(),
            },
            deadline_ms: number("AIRK_CUDA_DEADLINE_MS")?,
        }
    } else if mode == "package_projection" {
        Request::PackageProjection {
            schema: SCHEMA.into(),
            directory: PathBuf::from(text("AIRK_CUDA_RUNTIME_ROOT")?),
            descriptor: env_pin(
                "AIRK_CUDA_RUNTIME_MANIFEST",
                "AIRK_CUDA_RUNTIME_MANIFEST_SHA256",
            )?,
            deadline_ms: number("AIRK_CUDA_DEADLINE_MS")?,
        }
    } else {
        ensure!(mode == "prepare", "unknown native request kind");
        let cpu_input = env_pin(
            "AIRK_SOFTWARE_V2_NATIVE_INPUT",
            "AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256",
        )?;
        let (_, profile, _, _) = inputs_from(&cpu_input)?;
        let bounds = GpuProbeReservation {
            schema: "airk.arbiter.gpu-probe-reservation/1".into(),
            host_limit_bytes: number("AIRK_CUDA_HOST_LIMIT_BYTES")?,
            device_limit_bytes: number("AIRK_CUDA_DEVICE_LIMIT_BYTES")?,
            deadline_ms: number("AIRK_CUDA_DEADLINE_MS")?,
        };
        let (cold_bytes, warm_bytes, headroom_bytes, job_limit_bytes) = (
            number("AIRK_CUDA_COLD_BYTES")?,
            number("AIRK_CUDA_WARM_BYTES")?,
            number("AIRK_CUDA_HEADROOM_BYTES")?,
            number("AIRK_CUDA_JOB_LIMIT_BYTES")?,
        );
        profile_probe_bounds(
            &bounds,
            cold_bytes,
            warm_bytes,
            headroom_bytes,
            job_limit_bytes,
            &profile,
        )?;
        Request::Prepare {
            schema: SCHEMA.into(),
            cpu_input,
            runtime_manifest: env_pin(
                "AIRK_CUDA_RUNTIME_MANIFEST",
                "AIRK_CUDA_RUNTIME_MANIFEST_SHA256",
            )?,
            bounds,
            cold_bytes,
            warm_bytes,
            headroom_bytes,
            job_limit_bytes,
        }
    };
    let path = PathBuf::from(text("AIRK_CUDA_REQUEST_OUTPUT")?);
    write_json(&path, &value)?;
    Ok(())
}

#[test]
#[ignore = "NN0 requires exact reviewed producer+staged DLLs; emits UNAPPROVED fits-empty native manifest"]
fn emit_cuda_runtime_manifest_from_held_native_closure() -> Result<()> {
    let started = Instant::now();
    let (request, pin) = request()?;
    let Request::Assembly {
        schema,
        assembly,
        deadline_ms,
    } = request
    else {
        anyhow::bail!("assembly request required")
    };
    ensure!(
        schema == SCHEMA && (1..=120000).contains(&deadline_ms),
        "assembly schema/deadline differs"
    );
    let deadline = started + Duration::from_millis(deadline_ms);
    let (_parent, control) = private_control()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(lease.clone())?;
    let bytes = owner.run(|| -> Result<Vec<u8>> {
        // SAFETY: the erased Arc is this exact actual exclusive lease, and run owns
        // the existing real ServiceComputeScope throughout metadata observation.
        unsafe { cuda::observe_runtime_assembly(lease.clone(), assembly, deadline) }
    })??;
    ensure!(
        Instant::now() < deadline,
        "original metadata deadline exceeded"
    );
    let path = output()?;
    let manifest = write_new(&path, &bytes)?;
    write_json(
        &path.with_extension("receipt.json"),
        &json!({"schema":"airk.private.cuda-runtime-assembly-emission/1","input":pin,"manifest":manifest,"pid":std::process::id(),"execution_user":airk_ipc::endpoint::current_user()?,"elapsed_us":started.elapsed().as_micros(),"model_requests_dispatched":false,"runtime_admission_dispatched":false,"forward_count_observed":null,"approved":false,"fits_empty":true,"scope":"Actual held native assembly metadata only; no compiled runtime approval or GPU execution capability"}),
    )?;
    Ok(())
}

#[test]
#[ignore = "NN0 uses current native CPU source metadata + actual raw RuntimeManifest; creates fresh GPU Profile/CAL/request"]
fn emit_cuda_fp32_probe_metadata() -> Result<()> {
    let started = Instant::now();
    let (request, request_pin) = request()?;
    let Request::Prepare {
        schema,
        cpu_input,
        runtime_manifest,
        bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
    } = request
    else {
        anyhow::bail!("prepare request required")
    };
    ensure!(schema == SCHEMA, "unknown metadata schema");
    let (mut input, profile, files, _) = inputs_from(&cpu_input)?;
    let plan = profile_probe_bounds(
        &bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
        &profile,
    )?;
    let deadline = started + Duration::from_millis(bounds.deadline_ms);
    let raw = pinned(&runtime_manifest, 128 << 10)?;
    let prepared =
        cuda::prepare_fp32_probe_profile(&profile, &files, &raw, runtime_manifest.sha256, plan)?;
    ensure!(
        Instant::now() < deadline,
        "original metadata expiry reached"
    );
    let directory = PathBuf::from(text("AIRK_CUDA_METADATA_DIRECTORY")?);
    std::fs::create_dir(&directory)?;
    let calibration = write_new(
        &directory.join(&prepared.calibration_name),
        &prepared.calibration_bytes,
    )?;
    let profile_pin = write_json(&directory.join("profile.json"), &prepared.profile)?;
    input.profile = profile_pin;
    input
        .files
        .insert(prepared.calibration_name, calibration.path);
    input.validate()?;
    let gpu_input = write_json(&directory.join("native-input.json"), &input)?;
    let probe = Request::Probe {
        schema: SCHEMA.into(),
        gpu_input,
        runtime_manifest,
        bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
    };
    let probe_pin = write_json(&directory.join("probe-request.json"), &probe)?;
    let selectors = if profile.model == ModelChoice::FocusQwenReranker06B {
        vec!["cuda_selected_focus_fp32_512_component"]
    } else {
        vec![
            "cuda_original_context_fp32_t17_component",
            "cuda_original_context_fp32_t128_component",
            "cuda_original_context_fp32_t8192_component",
        ]
    };
    let prefix =
        "inference_engine::pool::arbiter::qualification::software_v2::cuda_qualification::";
    let commands=selectors.into_iter().map(|selector|json!({"program":std::env::current_exe().ok(),"args":["--exact",format!("{prefix}{selector}"),"--ignored","--nocapture"],"env":{"AIRK_CUDA_QUALIFICATION_INPUT":probe_pin.path,"AIRK_CUDA_QUALIFICATION_INPUT_SHA256":probe_pin.sha256},"requires":"fresh explicit AIRK_CUDA_QUALIFICATION_OUTPUT and reviewed same-human bounded owner; no native run authorized by this data"})).collect::<Vec<_>>();
    write_json(&directory.join("commands.json"), &commands)?;
    write_json(
        &directory.join("metadata-receipt.json"),
        &json!({"schema":"airk.private.cuda-probe-metadata/1","request":request_pin,"probe_request":probe_pin,"cpu_input":cpu_input,"model":profile.model,"profile":prepared.profile.manifest,"model_requests_dispatched":false,"forward_count_observed":null,"qualified":false,"elapsed_us":started.elapsed().as_micros()}),
    )?;
    ensure!(
        Instant::now() < deadline,
        "original metadata deadline exceeded during retention"
    );
    Ok(())
}

fn probe(kind: CaseKind) -> Result<()> {
    let started_ns = airk_process::monotonic::now_ns()?;
    let started = Instant::now();
    let (request, request_pin) = request()?;
    let Request::Probe {
        schema,
        gpu_input,
        runtime_manifest,
        bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
    } = request
    else {
        anyhow::bail!("native-emitted probe request required")
    };
    ensure!(schema == SCHEMA, "unknown probe schema");
    bounds.validate()?;
    let deadline = started + Duration::from_millis(bounds.deadline_ms);
    let expires_ns = started_ns
        .checked_add(bounds.deadline_ms * 1_000_000)
        .context("absolute probe expiry overflow")?;
    let (input, profile, files, _) = inputs_from(&gpu_input)?;
    let plan = profile_probe_bounds(
        &bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
        &profile,
    )?;
    let manifest_bytes = pinned(&runtime_manifest, 128 << 10)?;
    let _ = manifest_bytes;
    ensure!(
        profile.device_execution.is_some() && profile.cpu_fit.is_none(),
        "new GPU metadata required"
    );
    let fixture_pin = input
        .numeric_fixture
        .as_ref()
        .context("pinned independent numeric fixture required")?;
    let fixture: NumericFixture = serde_json::from_slice(&pinned(fixture_pin, 16 << 20)?)?;
    numeric_shape(&fixture, &profile, &input.policy)?;
    let producer = fixture
        .reference_producer
        .as_ref()
        .context("independent producer closure pin required")?;
    pinned(producer, 16 << 20)?;
    ensure!(
        !fixture.original_reference_assets.is_empty(),
        "independent original source asset roster missing"
    );
    let selected = selected_rows(kind, &profile, &fixture)?;
    for &index in &selected {
        for reference in [
            &fixture.rows[index].reference_same_artifact,
            &fixture.rows[index].reference_original_fp32,
        ] {
            let scores = reference
                .as_ref()
                .context("both exact independent reference arrays required before model load")?;
            ensure!(
                scores.len() == fixture.rows[index].input.candidate_positions.len()
                    && scores.iter().all(|v| v.is_finite()),
                "numeric reference shape/finite mismatch"
            );
        }
    }
    let actual_job = job_limits(bounds.host_limit_bytes, job_limit_bytes)?;
    let mut resources = airk_kernel::service_resources::Resources::detect(ResourceConfiguration {
        preset: ResourcePreset::Throughput,
        ..ResourceConfiguration::default()
    })?;
    let available = airk_kernel::service_resource_observations::available_memory();
    resources.observe(
        airk_kernel::service_resource_observations::rss(std::process::id()),
        available,
        true,
    );
    let incarnation = Incarnation::from_bytes(request_pin.sha256.as_bytes()[..16].try_into()?);
    let charge =
        Arc::new(resources.reserve_optional(incarnation, bounds.host_limit_bytes, available)?);
    let charge_release = charge.release_observer();
    let resource_snapshot = resources.snapshot();
    let (parent, control) = private_control()?;
    let mut control_parent = Some(parent);
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = Arc::new(airk_kernel::inference_owner::NativeInferenceOwner::admit(
        lease.clone(),
    )?);
    let output = output()?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    let binary = file_hash(&std::env::current_exe()?)?;
    let mut receipt = json!({"schema":"airk.private.cuda-component-result/1","complete":false,"qualified":false,"product_admission":false,"measured_peak_device_bytes":null,"request":request_pin,"native_input":gpu_input,"profile":profile.manifest,"model":profile.model,"fixture":fixture_pin,"reference_producer":producer,"source_manifest":input.source_manifest,"runtime_manifest":runtime_manifest,"pid":std::process::id(),"native_binary_sha256":binary,"execution_user":airk_ipc::endpoint::current_user()?,"bounds":bounds,"actual_job":actual_job,"native_host_reservation_bytes":charge.bytes(),"resource_snapshot":resource_snapshot,"original_expires_monotonic_ns":expires_ns,"rows":[],"scope":"Actual component CUDA score, final placement and exact independent references at stated rows only; Resource20, semantic/CAL, product lane and public qualification remain separate"});
    flush(&mut file, &receipt)?;
    let mut operations = operation::Journal::create_until(
        &output,
        operation::Identity {
            input_sha256: request_pin.sha256,
            profile: profile.manifest,
            process_id: std::process::id(),
            native_binary_sha256: binary,
        },
        expires_ns,
    )?;
    let run=owner.run(||->Result<()>{
  let mut custody=Custody{scorer:None,device:None,owner:owner.clone(),lease:lease.clone(),charge:Some(charge),control:&mut control_parent,files:Some(files)};
  ensure!(Instant::now()<deadline,"original probe deadline expired before preflight");verify_files(custody.files.as_ref().unwrap(),&profile)?;
  let(root,manifest)=airk_host::gpu_runtime::installed_paths()?;ensure!(manifest.as_path()==Path::new(&runtime_manifest.path),"probe manifest differs from authenticated installed runtime path");
  let runtime=airk_host::gpu_runtime::admit(lease.clone(),&root,&manifest,deadline)?;ensure!(runtime.manifest_identity()==runtime_manifest.sha256,"actual runtime manifest differs from frozen request");
  let adapters=runtime.adapters(deadline)?;ensure!(adapters.len()==1,"first scientific component probe requires exactly one observed adapter");let device=runtime.device(adapters[0].uuid,deadline)?;custody.device=Some(device.clone());receipt["adapter_before"]=serde_json::to_value(device.observation(deadline)?)?;flush(&mut file,&receipt)?;
  let catalog=airk_kernel::inference_owner::evaluation_catalog(lease.clone(),vec![profile.clone()])?;let execution=catalog.execution_profile(profile.manifest)?.bind_cuda_probe(device.clone(),plan,deadline)?;
  let loaded=operations.observe(operation::Kind::Load,0,None,||Scorer::open(execution,custody.files.as_ref().unwrap()))?;receipt["load_us"]=json!(loaded.elapsed_us);flush(&mut file,&receipt)?;custody.scorer=Some(loaded.value?);ensure!(loaded.within_deadline,"original CUDA load expiry exceeded");
  let scorer=custody.scorer.as_ref().unwrap();receipt["execution_identity"]=json!(scorer.execution_identity()?);receipt["final_placement"]=serde_json::to_value(scorer.device_placement()?.context("actual final SessionState map missing")?)?;flush(&mut file,&receipt)?;
  for index in selected{
   let row=&fixture.rows[index];let scored=operations.observe(operation::Kind::Numeric,index+1,Some(row.input.input_digest),||scorer.score_with_deadline(&row.input,deadline))?;let actual=scored.value?;
   let(same_raw,same_mass,same_argmax)=comparison(&actual,row.reference_same_artifact.as_ref().unwrap())?;let(original_raw,original_mass,original_argmax)=comparison(&actual,row.reference_original_fp32.as_ref().unwrap())?;
   receipt["rows"].as_array_mut().unwrap().push(json!({"name":row.name,"tokens":row.input.input_ids.len(),"input_digest":row.input.input_digest,"raw_scores":actual,"elapsed_us":scored.elapsed_us,"native_call_elapsed_us":scored.native_call_elapsed_us,"same_artifact_raw_error":same_raw,"same_artifact_mass_error":same_mass,"original_fp32_raw_error":original_raw,"original_fp32_mass_error":original_mass,"argmax_matches":same_argmax&&original_argmax}));flush(&mut file,&receipt)?;
   let focus=profile.model==ModelChoice::FocusQwenReranker06B;ensure!(scored.within_deadline&&same_raw<=0.001&&same_mass<=if focus{0.00001}else{0.05}&&original_raw<=if focus{0.25}else{0.001}&&original_mass<=0.05&&same_argmax&&original_argmax,"original absolute timing/numeric tolerance refused");
   ensure!(peak_rss().is_some_and(|v|v<=bounds.host_limit_bytes),"actual component host peak unavailable/exceeds original bound");
  }
  scorer.synchronize(deadline)?;ensure!(scorer.device_settled(),"CUDA work remains unsettled");receipt["adapter_after"]=serde_json::to_value(device.observation(deadline)?)?;
  drop(custody.scorer.take());ensure!(!device.poisoned(),"native session teardown settlement unknown");verify_files(custody.files.as_ref().unwrap(),&profile)?;pinned(&runtime_manifest,128<<10)?;pinned(&request_pin,1<<20)?;pinned(&gpu_input,1<<20)?;pinned(&input.profile,1<<20)?;pinned(&input.source_manifest,16<<20)?;pinned(fixture_pin,16<<20)?;pinned(producer,16<<20)?;ensure!(Instant::now()<deadline,"original component expiry exceeded during teardown/posthash");Ok(())
 });
    let result = run.and_then(std::convert::identity);
    drop(owner);
    drop(lease);
    let control_retained = control_parent.is_none();
    let cleanup = control_parent
        .take()
        .map(tempfile::TempDir::close)
        .transpose();
    receipt["native_control_retained_for_unknown_cuda"] = json!(control_retained);
    receipt["native_control_cleanup_observed"] = json!(matches!(&cleanup, Ok(Some(()))));
    receipt["native_control_cleanup_error"] =
        json!(cleanup.as_ref().err().map(ToString::to_string));
    let mut released = std::pin::pin!(charge_release.wait());
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let host_released = matches!(
        std::future::Future::poll(released.as_mut(), &mut cx),
        std::task::Poll::Ready(Ok(()))
    );
    receipt["native_host_charge_release_observed"] = json!(host_released);
    let result = match (result, cleanup) {
        (Ok(()), Ok(_)) if host_released => Ok(()),
        (Ok(()), Ok(_)) => Err(anyhow::anyhow!(
            "native host charge release remains unknown"
        )),
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error.into()),
    };
    let final_peak = peak_rss();
    let peak_decision = final_host_peak(final_peak, bounds.host_limit_bytes);
    receipt["peak_process_rss_bytes"] = json!(final_peak);
    receipt["final_host_peak_error"] = json!(peak_decision.as_ref().err().map(ToString::to_string));
    let result = match (result, peak_decision) {
        (Err(primary), _) => Err(primary),
        (Ok(()), Ok(_)) => Ok(()),
        (Ok(()), Err(peak_error)) => Err(peak_error),
    };
    receipt["complete"] = json!(result.is_ok());
    receipt["elapsed_us_to_final_report_start"] = json!(started.elapsed().as_micros());
    receipt["process_cuda_poisoned"] = json!(cuda::process_poisoned());
    if let Err(error) = &result {
        receipt["error"] = json!(format!("{error:#}"));
        if let Some(refusal) = error.downcast_ref::<cuda::PlacementRefusal>() {
            receipt["placement_refusal"] = json!(&refusal.observation);
        }
    }
    flush(&mut file, &receipt)?;
    if Instant::now() >= deadline {
        receipt["complete"] = json!(false);
        receipt["final_report_retention_exceeded_original_deadline"] = json!(true);
        flush(&mut file, &receipt)?;
        return result.and_then(|_| {
            Err(anyhow::anyhow!(
                "original CUDA component deadline exceeded during final report retention"
            ))
        });
    }
    result
}
#[test]
#[ignore = "actual approved CUDA runtime + current native metadata + exact independent references; original120s/host/device/Job caps"]
fn cuda_original_context_fp32_t17_component() -> Result<()> {
    probe(CaseKind::Context {
        index: 0,
        tokens: 17,
    })
}
#[test]
#[ignore = "actual approved CUDA runtime + current native metadata + exact independent references; original120s/host/device/Job caps"]
fn cuda_original_context_fp32_t128_component() -> Result<()> {
    probe(CaseKind::Context {
        index: 1,
        tokens: 128,
    })
}
#[test]
#[ignore = "actual approved CUDA runtime + current native metadata + exact independent all-live Context8192/N8 references; original120s/host/device/Job caps"]
fn cuda_original_context_fp32_t8192_component() -> Result<()> {
    probe(CaseKind::Context {
        index: 2,
        tokens: 8192,
    })
}
#[test]
#[ignore = "actual approved CUDA runtime + current native Focus512 metadata + all twelve independent cases; original120s/caps"]
fn cuda_selected_focus_fp32_512_component() -> Result<()> {
    probe(CaseKind::Focus512)
}

#[cfg(test)]
mod tests;

#[test]
fn historical_assembly_request_omits_new_checkpoint_authority_and_requires_explicit_pin()
-> Result<()> {
    let assembly = RuntimeAssemblyInput {
        directory: PathBuf::from("literal-root"),
        producer_name: "producer.json".into(),
        producer_sha256: Sha256Digest::from_bytes([1; 32]),
        checkpointed_proof: None,
        incremental_proof: None,
        custody_copies: Vec::new(),
        historical_tool_copies: Vec::new(),
        module_names: vec![],
        load_order: vec![],
        system_imports: BTreeSet::new(),
    };
    let mut json = serde_json::to_value(&assembly)?;
    assert!(json.get("checkpointed_proof").is_none());
    assert!(json.get("incremental_proof").is_none());
    json["checkpointed_proof"] =
        serde_json::json!({"path":"literal-proof.json","sha256":Sha256Digest::from_bytes([2;32])});
    let decoded: RuntimeAssemblyInput = serde_json::from_value(json.clone())?;
    assert_eq!(
        decoded.checkpointed_proof.unwrap().sha256,
        Sha256Digest::from_bytes([2; 32])
    );
    json["checkpointed_proof"]["approved"] = serde_json::json!(true);
    assert!(serde_json::from_value::<RuntimeAssemblyInput>(json).is_err());
    Ok(())
}

#[test]
#[ignore = "NN0: supplied genuine manifest/2+held public closure; package metadata only, no activation"]
fn emit_cuda_runtime_package_projection_from_held_native_closure() -> Result<()> {
    let started = Instant::now();
    let (request, pin) = request()?;
    let Request::PackageProjection {
        schema,
        directory,
        descriptor,
        deadline_ms,
    } = request
    else {
        anyhow::bail!("package projection request required")
    };
    ensure!(
        schema == SCHEMA && (1..=120000).contains(&deadline_ms),
        "package projection schema/deadline differs"
    );
    let deadline = started + Duration::from_millis(deadline_ms);
    require_installed_descriptor_name(Path::new(&descriptor.path))?;
    let (_parent, control) = private_control()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(lease.clone())?;
    let bytes = owner.run(|| -> Result<Vec<u8>> {
        // SAFETY: this exact live lease and the actual owning compute scope are retained.
        unsafe {
            cuda::observe_runtime_package_projection(
                lease.clone(),
                &directory,
                Path::new(&descriptor.path),
                descriptor.sha256,
                deadline,
            )
        }
    })??;
    ensure!(
        Instant::now() < deadline,
        "original metadata deadline exceeded"
    );
    let path = output()?;
    let projection = write_new(&path, &bytes)?;
    write_json(
        &path.with_extension("receipt.json"),
        &json!({"schema":"airk.private.cuda-runtime-package-projection-emission/1","input":pin,"descriptor":descriptor,"projection":projection,"pid":std::process::id(),"execution_user":airk_ipc::endpoint::current_user()?,"elapsed_us":started.elapsed().as_micros(),"model_requests_dispatched":false,"runtime_admission_dispatched":false,"forward_count_observed":null,"activation":false,"qualification":false,"scope":"actual held metadata projection only; parent process/Job custody is separate"}),
    )?;
    Ok(())
}

fn require_installed_descriptor_name(descriptor: &Path) -> Result<()> {
    let (_, installed) = airk_host::gpu_runtime::installed_paths()?;
    ensure!(
        descriptor.file_name().is_some() && descriptor.file_name() == installed.file_name(),
        "package descriptor basename differs from the actual native installed loader owner"
    );
    Ok(())
}
#[test]
fn package_projection_selector_uses_installed_native_descriptor_filename() -> Result<()> {
    // Filename contract only. No descriptor file or positive runtime proof exists.
    let (_, installed) = airk_host::gpu_runtime::installed_paths()?;
    assert_eq!(
        installed.file_name(),
        Some(std::ffi::OsStr::new("airk-cuda-runtime.json"))
    );
    require_installed_descriptor_name(&installed)?;
    assert!(
        require_installed_descriptor_name(&installed.with_file_name("other-runtime.json")).is_err()
    );
    assert!(require_installed_descriptor_name(Path::new("")).is_err());
    Ok(())
}

fn custody_locator_rows(
    raw: &[u8],
) -> Result<(
    Vec<cuda::MetadataCustodyCopy>,
    Vec<cuda::HistoricalToolCustodyCopy>,
)> {
    ensure!(raw.len() <= 1 << 20, "custody locator input cap");
    let text = std::str::from_utf8(raw)?;
    let mut metadata = Vec::new();
    let mut historical = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        ensure!(fields.len() == 6, "six custody locator fields required");
        let original = ResourceFilePin {
            path: fields[2].into(),
            sha256: fields[4].parse()?,
        };
        let copy = ResourceFilePin {
            path: fields[3].into(),
            sha256: original.sha256,
        };
        let bytes = fields[5].parse()?;
        match fields[0] {
            "metadata" => {
                ensure!(fields[1] == "-", "metadata has no historical role");
                metadata.push(cuda::MetadataCustodyCopy {
                    original,
                    copy,
                    bytes,
                });
            }
            "historical" => historical.push(cuda::HistoricalToolCustodyCopy {
                tool: fields[1].into(),
                original,
                copy,
                bytes,
            }),
            _ => anyhow::bail!("unknown custody locator class"),
        }
    }
    cuda::validate_custody_compatibility(&metadata, &historical)?;
    Ok((metadata, historical))
}
fn check_custody_file(pin: &ResourceFilePin, expected_bytes: u64, deadline: Instant) -> Result<()> {
    ensure!(
        Instant::now() < deadline,
        "original custody emission deadline expired"
    );
    let bytes = pinned(pin, 16 << 20)?;
    ensure!(
        bytes.len() as u64 == expected_bytes,
        "custody locator byte count differs from actual file"
    );
    ensure!(
        Instant::now() < deadline,
        "original custody emission deadline expired"
    );
    Ok(())
}
#[test]
#[ignore = "NN0: actual reviewed locator rows and byte-identical existing copies; emits owning Rust custody inputs only"]
fn emit_cuda_metadata_custody_inputs() -> Result<()> {
    let started = Instant::now();
    let limit = number("AIRK_CUDA_DEADLINE_MS")?;
    ensure!(
        (1..=120000).contains(&limit),
        "custody original metadata budget differs"
    );
    let deadline = started + Duration::from_millis(limit);
    let locator = env_pin(
        "AIRK_CUDA_CUSTODY_LOCATORS",
        "AIRK_CUDA_CUSTODY_LOCATORS_SHA256",
    )?;
    let (metadata, historical) = custody_locator_rows(&pinned(&locator, 1 << 20)?)?;
    for row in &metadata {
        check_custody_file(&row.original, row.bytes, deadline)?;
        check_custody_file(&row.copy, row.bytes, deadline)?;
    }
    for row in &historical {
        check_custody_file(&row.original, row.bytes, deadline)?;
        check_custody_file(&row.copy, row.bytes, deadline)?;
    }
    let a = cuda::encode_metadata_custody(&metadata)?;
    let b = cuda::encode_historical_tool_custody(&historical)?;
    let directory = PathBuf::from(text("AIRK_CUDA_CUSTODY_OUTPUT_DIRECTORY")?);
    ensure!(
        std::fs::symlink_metadata(&directory)?.is_dir(),
        "existing approved custody output directory required"
    );
    ensure!(
        Instant::now() < deadline,
        "original custody emission deadline expired"
    );
    let metadata_pin = write_new(&directory.join("metadata-custody.json"), &a)?;
    let historical_pin = write_new(&directory.join("historical-tool-custody.json"), &b)?;
    // Reopen the concrete original/copy pins after serialization; no mutable
    // file becomes an execution/protected-custody permit through this data.
    for row in &metadata {
        check_custody_file(&row.original, row.bytes, deadline)?;
        check_custody_file(&row.copy, row.bytes, deadline)?;
    }
    for row in &historical {
        check_custody_file(&row.original, row.bytes, deadline)?;
        check_custody_file(&row.copy, row.bytes, deadline)?;
    }
    ensure!(
        Instant::now() < deadline,
        "original custody emission deadline expired"
    );
    pinned(&locator, 1 << 20)?;
    ensure!(
        Instant::now() < deadline,
        "original custody emission deadline expired"
    );
    write_json(
        &directory.join("custody-emission.json"),
        &json!({"schema":"airk.private.cuda-custody-input-emission/1","input":locator,"metadata":metadata_pin,"historical":historical_pin,"pid":std::process::id(),"execution_user":airk_ipc::endpoint::current_user()?,"elapsed_us":started.elapsed().as_micros(),"scope":"actual file hash/size metadata only; protected held Source revalidates before use; no copies or activation","runtime_admission_dispatched":false,"model_requests_dispatched":false,"forward_count_observed":null}),
    )?;
    ensure!(
        Instant::now() < deadline,
        "original custody emission deadline expired after receipt settlement"
    );
    Ok(())
}
#[test]
fn custody_locator_parser_emits_owned_envelopes_and_refuses_unknown_duplicate_or_mismatched_rows()
-> Result<()> {
    let base = std::env::temp_dir();
    let original = base.join("literal-original.json");
    let copy = base.join("literal-copy.json");
    let line = format!(
        "metadata\t-\t{}\t{}\t{}\t3\n",
        original.display(),
        copy.display(),
        "01".repeat(32)
    );
    let (metadata, historical) = custody_locator_rows(line.as_bytes())?;
    let raw = cuda::encode_metadata_custody(&metadata)?;
    let value: serde_json::Value = serde_json::from_slice(&raw)?;
    assert_eq!(value["schema"], "airk.native.cuda-metadata-custody/1");
    assert_eq!(value["copies"].as_array().unwrap().len(), 1);
    assert_eq!(cuda::decode_metadata_custody(&raw)?.len(), 1);
    assert!(
        cuda::decode_historical_tool_custody(&cuda::encode_historical_tool_custody(&historical)?)?
            .is_empty()
    );
    assert!(custody_locator_rows((line.clone() + &line).as_bytes()).is_err());
    assert!(custody_locator_rows(line.replace("metadata\t-", "executable\t-").as_bytes()).is_err());
    assert!(
        custody_locator_rows(line.replace("metadata\t-", "metadata\tninja").as_bytes()).is_err()
    );
    assert!(
        custody_locator_rows(line.replace("metadata\t-", "historical\tninja").as_bytes()).is_err()
    );
    assert!(custody_locator_rows(line.replace("\t3\n", "\t0\n").as_bytes()).is_err());
    assert!(custody_locator_rows(b"metadata\t-\ttoo-few").is_err());
    Ok(())
}

#[cfg(windows)]
fn diagnostic_default_owner() -> Result<String> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_OWNER, TOKEN_QUERY,
            TokenOwner,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    let mut raw = std::ptr::null_mut();
    // SAFETY: read-only query of this process, with writable handle output.
    ensure!(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } != 0,
        "diagnostic token query failed"
    );
    // SAFETY: successful query returned one owned token handle.
    let token = unsafe { OwnedHandle::from_raw_handle(raw) };
    let mut storage = [0usize; 64];
    let mut returned = 0;
    // SAFETY: aligned buffer holds TOKEN_OWNER and the maximum native SID.
    ensure!(
        unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenOwner,
                storage.as_mut_ptr().cast(),
                std::mem::size_of_val(&storage) as u32,
                &mut returned,
            )
        } != 0,
        "diagnostic default-owner query failed"
    );
    ensure!(
        returned as usize >= std::mem::size_of::<TOKEN_OWNER>()
            && returned as usize <= std::mem::size_of_val(&storage),
        "invalid token owner length"
    );
    // SAFETY: successful native query initialized aligned TOKEN_OWNER storage.
    let sid = unsafe { (*storage.as_ptr().cast::<TOKEN_OWNER>()).Owner };
    let mut text = std::ptr::null_mut();
    // SAFETY: SID remains in the live token-information buffer.
    ensure!(
        unsafe { ConvertSidToStringSidW(sid, &mut text) } != 0,
        "diagnostic default-owner SID conversion failed"
    );
    let result = (|| -> Result<String> {
        let mut length = 0;
        // SAFETY: conversion returned a NUL-terminated SID string; bound the scan.
        while length < 256 && unsafe { *text.add(length) } != 0 {
            length += 1;
        }
        ensure!(length < 256, "diagnostic default-owner SID too long");
        // SAFETY: the preceding bounded scan established the initialized length.
        Ok(String::from_utf16(unsafe {
            std::slice::from_raw_parts(text, length)
        })?)
    })();
    // SAFETY: successful SID conversion transfers this LocalAlloc allocation.
    unsafe {
        LocalFree(text.cast());
    }
    result
}
#[cfg(not(windows))]
fn diagnostic_default_owner() -> Result<String> {
    anyhow::bail!("runtime diagnostic requires Windows token ownership")
}
fn diagnostic_settlement(
    admitted: bool,
    retained: bool,
    cleaned: bool,
    released: bool,
    poisoned: bool,
) -> Result<()> {
    ensure!(
        (!retained && cleaned && released && !poisoned)
            || (!admitted && retained && !cleaned && !released && poisoned),
        "inconsistent runtime diagnostic native custody"
    );
    Ok(())
}

#[test]
#[ignore = "runtime admission only: actual frozen request/manifest, same-owner Job, original120s/16GiB/24GiB; no model"]
fn cuda_runtime_only_admission_diagnostic() -> Result<()> {
    let started_ns = airk_process::monotonic::now_ns()?;
    let started = Instant::now();
    let execution_user = airk_ipc::endpoint::current_user()?;
    let default_owner = diagnostic_default_owner()?;
    ensure!(
        execution_user == default_owner,
        "diagnostic default owner differs from execution user"
    );
    let (request, request_pin) = request()?;
    let Request::Probe {
        schema,
        gpu_input,
        runtime_manifest,
        bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
    } = request
    else {
        anyhow::bail!("runtime diagnostic requires unchanged native Probe request")
    };
    ensure!(schema == SCHEMA, "unknown diagnostic request schema");
    bounds.validate()?;
    ensure!(
        bounds.deadline_ms == 120000
            && bounds.host_limit_bytes == 16 << 30
            && bounds.device_limit_bytes == 24 << 30
            && job_limit_bytes == 24 << 30,
        "runtime diagnostic original bounds differ"
    );
    let deadline = started + Duration::from_millis(bounds.deadline_ms);
    let expires_ns = started_ns
        .checked_add(bounds.deadline_ms * 1_000_000)
        .context("diagnostic expiry overflow")?;
    // This parser authenticates metadata only; Files is a path map, not an asset open.
    let (input, profile, _, _) = inputs_from(&gpu_input)?;
    bounds_validate(
        &bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
        profile.memory_reservation_bytes,
    )?;
    ensure!(
        profile.model == ModelChoice::ContextQwenCompact
            && profile.device_execution.is_some()
            && profile.cpu_fit.is_none(),
        "runtime diagnostic requires frozen Compact GPU metadata"
    );
    pinned(&runtime_manifest, 128 << 10)?;
    let (root, manifest) = airk_host::gpu_runtime::installed_paths()?;
    ensure!(
        manifest.as_path() == Path::new(&runtime_manifest.path),
        "diagnostic manifest differs from installed runtime locator"
    );
    let actual_job = job_limits(bounds.host_limit_bytes, job_limit_bytes)?;
    let mut resources = airk_kernel::service_resources::Resources::detect(ResourceConfiguration {
        preset: ResourcePreset::Throughput,
        ..ResourceConfiguration::default()
    })?;
    let available = airk_kernel::service_resource_observations::available_memory();
    resources.observe(
        airk_kernel::service_resource_observations::rss(std::process::id()),
        available,
        true,
    );
    let incarnation = Incarnation::from_bytes(request_pin.sha256.as_bytes()[..16].try_into()?);
    let charge =
        Arc::new(resources.reserve_optional(incarnation, bounds.host_limit_bytes, available)?);
    let charge_release = charge.release_observer();
    let (parent, control) = private_control()?;
    let mut control_parent = Some(parent);
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = Arc::new(airk_kernel::inference_owner::NativeInferenceOwner::admit(
        lease.clone(),
    )?);
    let binary = file_hash(&std::env::current_exe()?)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output()?)?;
    let mut receipt = json!({
        "schema":"airk.private.cuda-runtime-admission-diagnostic/1", "diagnostic_complete":false,
        "operation_scope":"False operation flags describe explicit diagnostic caller operations only; production admission includes cuInit and makes no claim that driver internals allocate nothing or create no internal contexts",
        "runtime_admitted":false,"outcome":"not_dispatched","qualified":false,"product_admission":false,
        "model_requests_dispatched":false,"adapters_observed":false,"device_context_created":false,
        "session_created":false,"scorer_created":false,"operations_journal_created":false,"forward_count_observed":null,
        "request":request_pin,"native_input":gpu_input,"profile":profile.manifest,"source_manifest":input.source_manifest,
        "runtime_manifest":runtime_manifest,"pid":std::process::id(),"native_binary_sha256":binary,
        "execution_user":execution_user,"default_owner":default_owner,"bounds":bounds,"actual_job":actual_job,
        "native_host_reservation_bytes":charge.bytes(),"resource_snapshot":resources.snapshot(),
        "original_expires_monotonic_ns":expires_ns,"error":null,"error_chain":[],"diagnostic_error":null,
        "scope":"Production runtime admission only; diagnosed refusal is not runtime admission or model qualification"
    });
    flush(&mut file, &receipt)?;
    let mut dispatched = false;
    let mut admitted = false;
    let run = owner.run(|| -> Result<()> {
        let _custody = Custody {
            scorer: None,
            device: None,
            owner: owner.clone(),
            lease: lease.clone(),
            charge: Some(charge),
            control: &mut control_parent,
            files: None,
        };
        ensure!(
            Instant::now() < deadline,
            "original diagnostic expiry before admission"
        );
        dispatched = true;
        let runtime = airk_host::gpu_runtime::admit(lease.clone(), &root, &manifest, deadline)?;
        ensure!(
            runtime.manifest_identity() == runtime_manifest.sha256,
            "admitted diagnostic runtime differs from frozen manifest"
        );
        admitted = true;
        Ok(())
    });
    let admission = run.and_then(std::convert::identity);
    drop(owner);
    drop(lease);
    let retained = control_parent.is_none();
    let cleanup = control_parent
        .take()
        .map(tempfile::TempDir::close)
        .transpose();
    let cleaned = matches!(&cleanup, Ok(Some(())));
    let mut released_future = std::pin::pin!(charge_release.wait());
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let released = matches!(
        std::future::Future::poll(released_future.as_mut(), &mut cx),
        std::task::Poll::Ready(Ok(()))
    );
    let poisoned = cuda::process_poisoned();
    receipt["runtime_admitted"] = json!(admitted);
    receipt["outcome"] = json!(if admitted {
        "admitted"
    } else if dispatched {
        "refused"
    } else {
        "not_dispatched"
    });
    receipt["native_control_retained_for_unknown_cuda"] = json!(retained);
    receipt["native_control_cleanup_observed"] = json!(cleaned);
    receipt["native_control_cleanup_error"] =
        json!(cleanup.as_ref().err().map(ToString::to_string));
    receipt["native_host_charge_release_observed"] = json!(released);
    receipt["process_cuda_poisoned"] = json!(poisoned);
    if let Err(error) = &admission {
        receipt["error"] = json!(format!("{error:#}"));
        receipt["error_chain"] = json!(error.chain().map(ToString::to_string).collect::<Vec<_>>());
    }
    let post = (|| -> Result<()> {
        ensure!(dispatched, "runtime admission was not dispatched");
        ensure!(cleanup.is_ok(), "native diagnostic control cleanup failed");
        diagnostic_settlement(admitted, retained, cleaned, released, poisoned)?;
        pinned(&request_pin, 1 << 20)?;
        pinned(&gpu_input, 1 << 20)?;
        pinned(&input.profile, 1 << 20)?;
        pinned(&input.source_manifest, 16 << 20)?;
        pinned(&runtime_manifest, 128 << 10)?;
        ensure!(
            Instant::now() < deadline,
            "original runtime diagnostic expiry during postpins"
        );
        Ok(())
    })();
    let peak = peak_rss();
    let peak_decision = final_host_peak(peak, bounds.host_limit_bytes);
    receipt["peak_process_rss_bytes"] = json!(peak);
    receipt["final_host_peak_error"] = json!(peak_decision.as_ref().err().map(ToString::to_string));
    let final_result = post.and(peak_decision.map(|_| ()));
    receipt["diagnostic_complete"] = json!(final_result.is_ok());
    receipt["diagnostic_error"] = json!(final_result.as_ref().err().map(|e| format!("{e:#}")));
    receipt["elapsed_us_to_final_report_start"] = json!(started.elapsed().as_micros());
    flush(&mut file, &receipt)?;
    if Instant::now() >= deadline {
        receipt["diagnostic_complete"] = json!(false);
        receipt["final_report_retention_exceeded_original_deadline"] = json!(true);
        receipt["diagnostic_error"] =
            json!("original runtime diagnostic expiry during final report retention");
        flush(&mut file, &receipt)?;
        anyhow::bail!("original runtime diagnostic expiry during final report retention");
    }
    final_result
}

#[test]
fn runtime_diagnostic_custody_distinguishes_release_from_retained_refusal() {
    assert!(diagnostic_settlement(true, false, true, true, false).is_ok());
    assert!(diagnostic_settlement(false, false, true, true, false).is_ok());
    assert!(diagnostic_settlement(false, true, false, false, true).is_ok());
    for (admitted, retained, cleaned, released, poisoned) in [
        (true, true, false, false, true),
        (false, true, true, false, true),
        (false, true, false, true, true),
        (false, false, true, false, false),
        (false, false, true, true, true),
        (false, true, false, false, false),
    ] {
        assert!(diagnostic_settlement(admitted, retained, cleaned, released, poisoned).is_err());
    }
}

fn placement_diagnostic_evidence(
    loaded: bool,
    snapshot: Option<&cuda::LoadSnapshot>,
    retained: bool,
    cleaned: bool,
    released: bool,
    poisoned: bool,
) -> Result<()> {
    ensure!(loaded, "disposable session diagnostic returned failure");
    snapshot
        .context("disposable session observer is absent")?
        .validate()?;
    diagnostic_settlement(true, retained, cleaned, released, poisoned)
}
#[test]
#[ignore = "disposable session placement only: original120s/16GiB/24GiB, actual CUDA lease/device; no scorer or forward"]
fn cuda_load_only_placement_diagnostic() -> Result<()> {
    let started_ns = airk_process::monotonic::now_ns()?;
    let started = Instant::now();
    let execution_user = airk_ipc::endpoint::current_user()?;
    let default_owner = diagnostic_default_owner()?;
    ensure!(
        execution_user == default_owner,
        "diagnostic default owner differs from execution user"
    );
    let (request, request_pin) = request()?;
    let Request::Probe {
        schema,
        gpu_input,
        runtime_manifest,
        bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
    } = request
    else {
        anyhow::bail!("placement diagnostic requires unchanged native Probe request")
    };
    ensure!(schema == SCHEMA, "unknown diagnostic request schema");
    bounds.validate()?;
    ensure!(
        bounds.deadline_ms == 120000
            && bounds.host_limit_bytes == 16 << 30
            && bounds.device_limit_bytes == 24 << 30
            && job_limit_bytes == 24 << 30,
        "placement diagnostic original bounds differ"
    );
    let deadline = started + Duration::from_millis(bounds.deadline_ms);
    let expires_ns = started_ns
        .checked_add(bounds.deadline_ms * 1_000_000)
        .context("diagnostic expiry overflow")?;
    // Actual source assets are authenticated below before disposable session creation.
    let (input, profile, files, _) = inputs_from(&gpu_input)?;
    let capacity_plan = bounds_validate(
        &bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
        profile.memory_reservation_bytes,
    )?;
    ensure!(
        profile.model == ModelChoice::ContextQwenCompact
            && profile.device_execution.is_some()
            && profile.cpu_fit.is_none(),
        "placement diagnostic requires frozen Compact GPU metadata"
    );
    pinned(&runtime_manifest, 128 << 10)?;
    let (root, manifest) = airk_host::gpu_runtime::installed_paths()?;
    ensure!(
        manifest.as_path() == Path::new(&runtime_manifest.path),
        "diagnostic manifest differs from installed runtime locator"
    );
    let actual_job = job_limits(bounds.host_limit_bytes, job_limit_bytes)?;
    let mut resources = airk_kernel::service_resources::Resources::detect(ResourceConfiguration {
        preset: ResourcePreset::Throughput,
        ..ResourceConfiguration::default()
    })?;
    let available = airk_kernel::service_resource_observations::available_memory();
    resources.observe(
        airk_kernel::service_resource_observations::rss(std::process::id()),
        available,
        true,
    );
    let incarnation = Incarnation::from_bytes(request_pin.sha256.as_bytes()[..16].try_into()?);
    let charge =
        Arc::new(resources.reserve_optional(incarnation, bounds.host_limit_bytes, available)?);
    let charge_release = charge.release_observer();
    let (parent, control) = private_control()?;
    let mut control_parent = Some(parent);
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = Arc::new(airk_kernel::inference_owner::NativeInferenceOwner::admit(
        lease.clone(),
    )?);
    let binary = file_hash(&std::env::current_exe()?)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output()?)?;
    let mut receipt = json!({
        "schema":"airk.private.cuda-placement-diagnostic/1", "diagnostic_complete":false,
        "operation_scope":"Disposable session creation and final placement observation only; no Scorer, forward, hybrid policy or qualification is returned",
        "outcome":"not_dispatched","qualification":false,"product_admission":false,
        "session_load_dispatched":false,"session_created":false,"strict_production_unchanged":true,
        "cpu_fallback_disabled":false,"scorer_returned":false,"forward_dispatched":false,
        "model_requests_dispatched":false,"operations_journal_created":false,"forward_count_observed":null,
        "placement":null,"logs":null,"teardown":null,
        "request":request_pin,"native_input":gpu_input,"profile":profile.manifest,"source_manifest":input.source_manifest,
        "runtime_manifest":runtime_manifest,"pid":std::process::id(),"native_binary_sha256":binary,
        "execution_user":execution_user,"default_owner":default_owner,"bounds":bounds,"actual_job":actual_job,
        "native_host_reservation_bytes":charge.bytes(),"resource_snapshot":resources.snapshot(),
        "original_expires_monotonic_ns":expires_ns,"error":null,"error_chain":[],"diagnostic_error":null
    });
    flush(&mut file, &receipt)?;
    let mut dispatched = false;
    let mut observer = None;
    let run = owner.run(|| -> Result<()> {
        let mut custody = Custody {
            scorer: None,
            device: None,
            owner: owner.clone(),
            lease: lease.clone(),
            charge: Some(charge),
            control: &mut control_parent,
            files: Some(files),
        };
        ensure!(
            Instant::now() < deadline,
            "original placement diagnostic expiry before admission"
        );
        verify_files(custody.files.as_ref().unwrap(), &profile)?;
        let runtime = airk_host::gpu_runtime::admit(lease.clone(), &root, &manifest, deadline)?;
        ensure!(
            runtime.manifest_identity() == runtime_manifest.sha256,
            "diagnostic runtime differs from frozen manifest"
        );
        let adapters = runtime.adapters(deadline)?;
        ensure!(
            adapters.len() == 1,
            "placement diagnostic requires one actual adapter"
        );
        let device = runtime.device(adapters[0].uuid, deadline)?;
        custody.device = Some(device.clone());
        receipt["adapter_before"] = serde_json::to_value(device.observation(deadline)?)?;
        let catalog =
            airk_kernel::inference_owner::evaluation_catalog(lease.clone(), vec![profile.clone()])?;
        let execution = catalog
            .execution_profile(profile.manifest)?
            .bind_cuda_probe(device, capacity_plan, deadline)?;
        observer = Some(execution.cuda_load_observer()?);
        ensure!(
            Instant::now() < deadline,
            "original placement diagnostic expiry before session load"
        );
        dispatched = true;
        receipt["session_load_dispatched"] = json!(true);
        let result = Scorer::diagnose_cuda_placement(execution, custody.files.as_ref().unwrap());
        let post_files = verify_files(custody.files.as_ref().unwrap(), &profile);
        result.and(post_files)
    });
    let admission = run.and_then(std::convert::identity);
    let snapshot = observer
        .as_ref()
        .map(cuda::LoadObserver::snapshot)
        .transpose();
    drop(owner);
    drop(lease);
    let retained = control_parent.is_none();
    let cleanup = control_parent
        .take()
        .map(tempfile::TempDir::close)
        .transpose();
    let cleaned = matches!(&cleanup, Ok(Some(())));
    let mut released_future = std::pin::pin!(charge_release.wait());
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let released = matches!(
        std::future::Future::poll(released_future.as_mut(), &mut cx),
        std::task::Poll::Ready(Ok(()))
    );
    let poisoned = cuda::process_poisoned();
    receipt["session_load_dispatched"] = json!(dispatched);
    if let Ok(Some(value)) = &snapshot {
        receipt["session_created"] = json!(value.session_created);
        receipt["placement"] = serde_json::to_value(&value.placement)?;
        receipt["logs"] = serde_json::to_value(&value.logs)?;
        receipt["teardown"] = serde_json::to_value(&value.teardown)?;
    }
    receipt["native_control_retained_for_unknown_cuda"] = json!(retained);
    receipt["native_control_cleanup_observed"] = json!(cleaned);
    receipt["native_control_cleanup_error"] =
        json!(cleanup.as_ref().err().map(ToString::to_string));
    receipt["native_host_charge_release_observed"] = json!(released);
    receipt["process_cuda_poisoned"] = json!(poisoned);
    if let Err(error) = &admission {
        receipt["error"] = json!(format!("{error:#}"));
        receipt["error_chain"] = json!(error.chain().map(ToString::to_string).collect::<Vec<_>>());
    }
    let post = (|| -> Result<()> {
        ensure!(dispatched, "disposable session load was not dispatched");
        ensure!(cleanup.is_ok(), "native diagnostic control cleanup failed");
        let observed = snapshot
            .as_ref()
            .map_err(|error| anyhow::anyhow!("{error:#}"))?;
        placement_diagnostic_evidence(
            admission.is_ok(),
            observed.as_ref(),
            retained,
            cleaned,
            released,
            poisoned,
        )?;
        pinned(&request_pin, 1 << 20)?;
        pinned(&gpu_input, 1 << 20)?;
        pinned(&input.profile, 1 << 20)?;
        pinned(&input.source_manifest, 16 << 20)?;
        pinned(&runtime_manifest, 128 << 10)?;
        ensure!(
            Instant::now() < deadline,
            "original placement diagnostic expiry during postpins"
        );
        Ok(())
    })();
    let peak = peak_rss();
    let peak_decision = final_host_peak(peak, bounds.host_limit_bytes);
    receipt["peak_process_rss_bytes"] = json!(peak);
    receipt["final_host_peak_error"] = json!(peak_decision.as_ref().err().map(ToString::to_string));
    let final_result = admission.and(post).and(peak_decision.map(|_| ()));
    receipt["outcome"] = json!(if final_result.is_ok() {
        "placement_observed"
    } else if dispatched {
        "failed"
    } else {
        "not_dispatched"
    });
    receipt["diagnostic_complete"] = json!(final_result.is_ok());
    receipt["diagnostic_error"] = json!(final_result.as_ref().err().map(|e| format!("{e:#}")));
    receipt["elapsed_us_to_final_report_start"] = json!(started.elapsed().as_micros());
    flush(&mut file, &receipt)?;
    if Instant::now() >= deadline {
        receipt["diagnostic_complete"] = json!(false);
        receipt["outcome"] = json!("failed");
        receipt["final_report_retention_exceeded_original_deadline"] = json!(true);
        receipt["diagnostic_error"] =
            json!("original placement diagnostic expiry during final report retention");
        flush(&mut file, &receipt)?;
        anyhow::bail!("original placement diagnostic expiry during final report retention");
    }
    final_result
}

fn synthetic_placement_snapshot() -> cuda::LoadSnapshot {
    let mut value = cuda::LoadSnapshot {
        session_created: true,
        placement: Some(cuda::PlacementObservation {
            scope: "onnxruntime-final-session-state-ep-placement",
            complete: true,
            shim_status: 0,
            ort_error_code: 0,
            reported_nodes: 1,
            reported_string_bytes: 30,
            nodes: vec![cuda::PlacedNode {
                subgraph_index: 0,
                name: "shape".into(),
                domain: "".into(),
                op: "Shape".into(),
                ep: "CPUExecutionProvider".into(),
            }],
        }),
        teardown: Some(Default::default()),
        ..Default::default()
    };
    let teardown = value.teardown.as_mut().unwrap();
    teardown.pre_sync_observed = true;
    teardown.session_drop_attempted = true;
    teardown.session_drop_returned = true;
    teardown.post_sync_observed = true;
    teardown.pending_claims = Some(0);
    value
}
#[test]
fn placement_diagnostic_observation_never_substitutes_for_load_or_custody() -> Result<()> {
    let value = synthetic_placement_snapshot();
    placement_diagnostic_evidence(true, Some(&value), false, true, true, false)?;
    // CPU assignment is observable here; the unchanged production strict gate refuses it.
    assert!(value.placement.as_ref().unwrap().validate_strict().is_err());
    assert!(placement_diagnostic_evidence(false, Some(&value), false, true, true, false).is_err());
    assert!(placement_diagnostic_evidence(true, None, false, true, true, false).is_err());
    for (retained, cleaned, released, poisoned) in [
        (true, false, false, true),
        (false, false, true, false),
        (false, true, false, false),
        (false, true, true, true),
    ] {
        assert!(
            placement_diagnostic_evidence(
                true,
                Some(&value),
                retained,
                cleaned,
                released,
                poisoned
            )
            .is_err()
        );
    }
    Ok(())
}
#[test]
fn placement_diagnostic_partial_capture_or_unproven_teardown_refuses() {
    for mutation in 0..5 {
        let mut value = synthetic_placement_snapshot();
        match mutation {
            0 => value.session_created = false,
            1 => value.placement.as_mut().unwrap().complete = false,
            2 => value.logs.truncated = true,
            3 => value.teardown.as_mut().unwrap().session_drop_returned = false,
            4 => value.teardown.as_mut().unwrap().pending_claims = None,
            _ => unreachable!(),
        }
        assert!(
            placement_diagnostic_evidence(true, Some(&value), false, true, true, false).is_err(),
            "mutation {mutation}"
        );
    }
}

// This additive selector preserves the existing placement-mode wire and success rules.
const GRAPH_DIAGNOSTIC_BYTES: usize = 16 << 20;
fn bounded_graph_diagnostic_json(receipt: &Value, maximum: usize) -> Result<Vec<u8>> {
    ensure!(
        maximum > 0 && maximum <= GRAPH_DIAGNOSTIC_BYTES,
        "invalid graph diagnostic byte cap"
    );
    struct Bounded {
        bytes: Vec<u8>,
        maximum: usize,
    }
    impl std::io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if !self
                .bytes
                .len()
                .checked_add(bytes.len())
                .is_some_and(|n| n <= self.maximum)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "graph diagnostic output cap",
                ));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Bounded {
        bytes: Vec::new(),
        maximum,
    };
    serde_json::to_writer(&mut output, receipt)?;
    Ok(output.bytes)
}
fn flush_graph_diagnostic(file: &mut File, receipt: &Value) -> Result<()> {
    // Complete bounded serialization precedes seek/write: overflow preserves prior bytes.
    let bytes = bounded_graph_diagnostic_json(receipt, GRAPH_DIAGNOSTIC_BYTES)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.set_len(bytes.len() as u64)?;
    file.flush()?;
    file.sync_data()?;
    Ok(())
}
fn graph_diagnostic_logs_complete(snapshot: &cuda::LoadSnapshot) -> bool {
    !snapshot.logs.truncated && snapshot.logs.omitted == 0
}
fn graph_diagnostic_custody(
    loaded: bool,
    retained: bool,
    cleaned: bool,
    released: bool,
    poisoned: bool,
) -> Result<()> {
    ensure!(loaded, "disposable graph diagnostic returned failure");
    diagnostic_settlement(true, retained, cleaned, released, poisoned)
}
fn graph_diagnostic_evidence(
    loaded: bool,
    snapshot: Option<&cuda::LoadSnapshot>,
    retained: bool,
    cleaned: bool,
    released: bool,
    poisoned: bool,
) -> Result<()> {
    graph_diagnostic_custody(loaded, retained, cleaned, released, poisoned)?;
    snapshot
        .context("disposable graph observer is absent")?
        .validate_graph_metadata()
}

#[test]
#[ignore = "disposable final graph metadata only: original120s, explicit Compact host/Job, actual CUDA lease/device; no scorer or forward"]
fn cuda_load_only_graph_metadata_diagnostic() -> Result<()> {
    let started_ns = airk_process::monotonic::now_ns()?;
    let started = Instant::now();
    let execution_user = airk_ipc::endpoint::current_user()?;
    let default_owner = diagnostic_default_owner()?;
    ensure!(
        execution_user == default_owner,
        "diagnostic default owner differs from execution user"
    );
    let (request, request_pin) = request()?;
    let Request::Probe {
        schema,
        gpu_input,
        runtime_manifest,
        bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
    } = request
    else {
        anyhow::bail!("graph metadata diagnostic requires unchanged native Probe request")
    };
    ensure!(schema == SCHEMA, "unknown diagnostic request schema");
    bounds.validate()?;
    ensure!(
        bounds.deadline_ms == 120000 && bounds.device_limit_bytes == 24 << 30,
        "graph metadata diagnostic original deadline/device bounds differ"
    );
    let deadline = started + Duration::from_millis(bounds.deadline_ms);
    let expires_ns = started_ns
        .checked_add(bounds.deadline_ms * 1_000_000)
        .context("diagnostic expiry overflow")?;
    // Actual source assets are authenticated below before disposable session creation.
    let (input, profile, files, _) = inputs_from(&gpu_input)?;
    ensure!(
        profile.model == ModelChoice::ContextQwenCompact
            && profile.device_execution.is_some()
            && profile.cpu_fit.is_none(),
        "graph metadata diagnostic requires frozen Compact GPU metadata"
    );
    let capacity_plan = profile_probe_bounds(
        &bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
        &profile,
    )?;
    pinned(&runtime_manifest, 128 << 10)?;
    let (root, manifest) = airk_host::gpu_runtime::installed_paths()?;
    ensure!(
        manifest.as_path() == Path::new(&runtime_manifest.path),
        "diagnostic manifest differs from installed runtime locator"
    );
    let actual_job = job_limits(bounds.host_limit_bytes, job_limit_bytes)?;
    let mut resources = airk_kernel::service_resources::Resources::detect(ResourceConfiguration {
        preset: ResourcePreset::Throughput,
        ..ResourceConfiguration::default()
    })?;
    let available = airk_kernel::service_resource_observations::available_memory();
    resources.observe(
        airk_kernel::service_resource_observations::rss(std::process::id()),
        available,
        true,
    );
    let incarnation = Incarnation::from_bytes(request_pin.sha256.as_bytes()[..16].try_into()?);
    let charge =
        Arc::new(resources.reserve_optional(incarnation, bounds.host_limit_bytes, available)?);
    let charge_release = charge.release_observer();
    let (parent, control) = private_control()?;
    let mut control_parent = Some(parent);
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = Arc::new(airk_kernel::inference_owner::NativeInferenceOwner::admit(
        lease.clone(),
    )?);
    let binary = file_hash(&std::env::current_exe()?)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output()?)?;
    let mut receipt = json!({
        "schema":"airk.private.cuda-graph-metadata-diagnostic/1", "diagnostic_complete":false,
        "operation_scope":"Disposable session creation and final graph metadata observation only; no Scorer, forward, hybrid policy or qualification is returned",
        "outcome":"not_dispatched","qualification":false,"product_admission":false,
        "session_load_dispatched":false,"session_created":false,"strict_production_unchanged":true,
        "cpu_fallback_disabled":false,"scorer_returned":false,"forward_dispatched":false,
        "model_requests_dispatched":false,"operations_journal_created":false,"forward_count_observed":null,
        "placement":null,"graph_metadata":null,"logs":null,"selected_logs_complete":false,"teardown":null,
        "request":request_pin,"native_input":gpu_input,"profile":profile.manifest,"source_manifest":input.source_manifest,
        "runtime_manifest":runtime_manifest,"pid":std::process::id(),"native_binary_sha256":binary,
        "execution_user":execution_user,"default_owner":default_owner,"bounds":bounds,"actual_job":actual_job,
        "native_host_reservation_bytes":charge.bytes(),"resource_snapshot":resources.snapshot(),
        "original_expires_monotonic_ns":expires_ns,"error":null,"error_chain":[],"diagnostic_error":null
    });
    flush_graph_diagnostic(&mut file, &receipt)?;
    let mut dispatched = false;
    let mut observer = None;
    let run = owner.run(|| -> Result<()> {
        let mut custody = Custody {
            scorer: None,
            device: None,
            owner: owner.clone(),
            lease: lease.clone(),
            charge: Some(charge),
            control: &mut control_parent,
            files: Some(files),
        };
        ensure!(
            Instant::now() < deadline,
            "original graph metadata diagnostic expiry before admission"
        );
        verify_files(custody.files.as_ref().unwrap(), &profile)?;
        let runtime = airk_host::gpu_runtime::admit(lease.clone(), &root, &manifest, deadline)?;
        ensure!(
            runtime.manifest_identity() == runtime_manifest.sha256,
            "diagnostic runtime differs from frozen manifest"
        );
        let adapters = runtime.adapters(deadline)?;
        ensure!(
            adapters.len() == 1,
            "graph metadata diagnostic requires one actual adapter"
        );
        let device = runtime.device(adapters[0].uuid, deadline)?;
        custody.device = Some(device.clone());
        receipt["adapter_before"] = serde_json::to_value(device.observation(deadline)?)?;
        let catalog =
            airk_kernel::inference_owner::evaluation_catalog(lease.clone(), vec![profile.clone()])?;
        let execution = catalog
            .execution_profile(profile.manifest)?
            .bind_cuda_probe(device, capacity_plan, deadline)?;
        observer = Some(execution.cuda_load_observer()?);
        ensure!(
            Instant::now() < deadline,
            "original graph metadata diagnostic expiry before session load"
        );
        dispatched = true;
        receipt["session_load_dispatched"] = json!(true);
        let result =
            Scorer::diagnose_cuda_graph_metadata(execution, custody.files.as_ref().unwrap());
        let post_files = verify_files(custody.files.as_ref().unwrap(), &profile);
        result.and(post_files)
    });
    let admission = run.and_then(std::convert::identity);
    let snapshot = observer
        .as_ref()
        .map(cuda::LoadObserver::snapshot)
        .transpose();
    drop(owner);
    drop(lease);
    let retained = control_parent.is_none();
    let cleanup = control_parent
        .take()
        .map(tempfile::TempDir::close)
        .transpose();
    let cleaned = matches!(&cleanup, Ok(Some(())));
    let mut released_future = std::pin::pin!(charge_release.wait());
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let released = matches!(
        std::future::Future::poll(released_future.as_mut(), &mut cx),
        std::task::Poll::Ready(Ok(()))
    );
    let poisoned = cuda::process_poisoned();
    receipt["session_load_dispatched"] = json!(dispatched);
    if let Ok(Some(value)) = &snapshot {
        receipt["session_created"] = json!(value.session_created);
        receipt["placement"] = serde_json::to_value(&value.placement)?;
        receipt["logs"] = serde_json::to_value(&value.logs)?;
        receipt["graph_metadata"] = serde_json::to_value(&value.graph_metadata)?;
        receipt["selected_logs_complete"] = json!(graph_diagnostic_logs_complete(value));
        receipt["teardown"] = serde_json::to_value(&value.teardown)?;
    }
    receipt["native_control_retained_for_unknown_cuda"] = json!(retained);
    receipt["native_control_cleanup_observed"] = json!(cleaned);
    receipt["native_control_cleanup_error"] =
        json!(cleanup.as_ref().err().map(ToString::to_string));
    receipt["native_host_charge_release_observed"] = json!(released);
    receipt["process_cuda_poisoned"] = json!(poisoned);
    if let Err(error) = &admission {
        receipt["error"] = json!(format!("{error:#}"));
        receipt["error_chain"] = json!(error.chain().map(ToString::to_string).collect::<Vec<_>>());
    }
    let post = (|| -> Result<()> {
        ensure!(dispatched, "disposable session load was not dispatched");
        ensure!(cleanup.is_ok(), "native diagnostic control cleanup failed");
        let observed = snapshot
            .as_ref()
            .map_err(|error| anyhow::anyhow!("{error:#}"))?;
        graph_diagnostic_evidence(
            admission.is_ok(),
            observed.as_ref(),
            retained,
            cleaned,
            released,
            poisoned,
        )?;
        pinned(&request_pin, 1 << 20)?;
        pinned(&gpu_input, 1 << 20)?;
        pinned(&input.profile, 1 << 20)?;
        pinned(&input.source_manifest, 16 << 20)?;
        pinned(&runtime_manifest, 128 << 10)?;
        ensure!(
            Instant::now() < deadline,
            "original graph metadata diagnostic expiry during postpins"
        );
        Ok(())
    })();
    let peak = peak_rss();
    let peak_decision = final_host_peak(peak, bounds.host_limit_bytes);
    receipt["peak_process_rss_bytes"] = json!(peak);
    receipt["final_host_peak_error"] = json!(peak_decision.as_ref().err().map(ToString::to_string));
    let final_result = admission.and(post).and(peak_decision.map(|_| ()));
    receipt["outcome"] = json!(if final_result.is_ok() {
        "graph_metadata_observed"
    } else if dispatched {
        "failed"
    } else {
        "not_dispatched"
    });
    receipt["diagnostic_complete"] = json!(final_result.is_ok());
    receipt["diagnostic_error"] = json!(final_result.as_ref().err().map(|e| format!("{e:#}")));
    receipt["elapsed_us_to_final_report_start"] = json!(started.elapsed().as_micros());
    flush_graph_diagnostic(&mut file, &receipt)?;
    if Instant::now() >= deadline {
        receipt["diagnostic_complete"] = json!(false);
        receipt["outcome"] = json!("failed");
        receipt["final_report_retention_exceeded_original_deadline"] = json!(true);
        receipt["diagnostic_error"] =
            json!("original graph metadata diagnostic expiry during final report retention");
        flush_graph_diagnostic(&mut file, &receipt)?;
        anyhow::bail!("original graph metadata diagnostic expiry during final report retention");
    }
    final_result
}

#[test]
fn graph_diagnostic_compact_encoding_preserves_literal_json_and_exact_cap() -> Result<()> {
    let value = json!({"x":"é\n"});
    let expected = "{\"x\":\"é\\n\"}".as_bytes();
    let encoded = bounded_graph_diagnostic_json(&value, expected.len())?;
    assert_eq!(encoded, expected);
    assert_eq!(serde_json::from_slice::<Value>(&encoded)?, value);
    assert!(bounded_graph_diagnostic_json(&value, expected.len() - 1).is_err());
    Ok(())
}
#[test]
fn graph_diagnostic_encoding_rejects_invalid_or_overflowed_budget() {
    assert!(bounded_graph_diagnostic_json(&json!(null), 0).is_err());
    assert!(bounded_graph_diagnostic_json(&json!(null), GRAPH_DIAGNOSTIC_BYTES + 1).is_err());
    assert!(bounded_graph_diagnostic_json(&json!({"x":"a".repeat(100)}), 16).is_err());
}
#[test]
fn graph_diagnostic_log_disclosure_does_not_relabel_placement_success() {
    let mut value = synthetic_placement_snapshot();
    assert!(graph_diagnostic_logs_complete(&value));
    value.logs.truncated = true;
    assert!(!graph_diagnostic_logs_complete(&value));
    assert!(value.validate().is_err());
    value.logs.truncated = false;
    value.logs.omitted = 1;
    assert!(!graph_diagnostic_logs_complete(&value));
    assert!(value.validate().is_err());
}
#[test]
fn graph_diagnostic_requires_actual_load_and_clean_outer_custody() -> Result<()> {
    graph_diagnostic_custody(true, false, true, true, false)?;
    for (loaded, retained, cleaned, released, poisoned) in [
        (false, false, true, true, false),
        (true, true, false, false, true),
        (true, false, false, true, false),
        (true, false, true, false, false),
        (true, false, true, true, true),
    ] {
        assert!(graph_diagnostic_custody(loaded, retained, cleaned, released, poisoned).is_err());
    }
    Ok(())
}
#[test]
fn graph_diagnostic_refuses_missing_snapshot_or_graph_even_with_complete_placement() {
    assert!(graph_diagnostic_evidence(true, None, false, true, true, false).is_err());
    let value = synthetic_placement_snapshot();
    assert!(value.validate().is_ok());
    assert!(graph_diagnostic_evidence(true, Some(&value), false, true, true, false).is_err());
}

fn synthetic_graph_snapshot() -> Result<cuda::LoadSnapshot> {
    use cuda::GraphMetadataRow as R;
    let mut snapshot = synthetic_placement_snapshot();
    let mut graph = cuda::GraphMetadataObservation {
        schema: "airk.private.ort-final-graph-metadata/1",
        complete: true,
        shim_status: 0,
        ort_error_code: 0,
        reported_counts: [1, 2, 5, 62, 0, 12],
        encoded_bytes: 0,
        failure: None,
        rows: vec![
            R::Graph {
                graph: 0,
                parent_graph: None,
                parent_node: None,
                attribute: "".into(),
                outer_bindings: "not_applicable".into(),
            },
            R::Value {
                graph: 0,
                value: 0,
                name: "input".into(),
                element_type: Some(1),
                rank: Some(1),
                type_state: "tensor".into(),
                provenance: "graph_input".into(),
                producer: None,
            },
            R::Dimension {
                graph: 0,
                value: 0,
                index: 0,
                number: Some(3),
                symbol: None,
            },
            R::Value {
                graph: 0,
                value: 1,
                name: "shape".into(),
                element_type: Some(7),
                rank: Some(1),
                type_state: "tensor".into(),
                provenance: "node_output".into(),
                producer: Some(7),
            },
            R::Dimension {
                graph: 0,
                value: 1,
                index: 0,
                number: Some(1),
                symbol: None,
            },
            R::Edge {
                graph: 0,
                node: None,
                value: Some(0),
                role: 3,
                slot: 0,
            },
            R::Edge {
                graph: 0,
                node: None,
                value: Some(1),
                role: 4,
                slot: 0,
            },
            R::Edge {
                graph: 0,
                node: None,
                value: Some(0),
                role: 5,
                slot: 0,
            },
            R::Node {
                graph: 0,
                node: 7,
                name: "shape".into(),
                domain: "".into(),
                op: "Shape".into(),
                ep: "CPUExecutionProvider".into(),
                version: 15,
            },
            R::Edge {
                graph: 0,
                node: Some(7),
                value: Some(0),
                role: 0,
                slot: 0,
            },
            R::Edge {
                graph: 0,
                node: Some(7),
                value: Some(1),
                role: 2,
                slot: 0,
            },
            R::End { graph: 0 },
        ],
    };
    // Only the self-describing encoded length is calculated; expected graph facts are literal.
    for _ in 0..8 {
        let size = serde_json::to_vec(&graph)?.len();
        if size == graph.encoded_bytes {
            break;
        }
        graph.encoded_bytes = size;
    }
    snapshot.graph_metadata = Some(graph);
    Ok(snapshot)
}
#[test]
fn graph_diagnostic_success_keeps_truncated_logs_explicit_without_old_mode_pass() -> Result<()> {
    let mut snapshot = synthetic_graph_snapshot()?;
    snapshot.logs.seen = 1;
    snapshot.logs.selected = 1;
    snapshot.logs.omitted = 1;
    snapshot.logs.truncated = true;
    graph_diagnostic_evidence(true, Some(&snapshot), false, true, true, false)?;
    assert!(!graph_diagnostic_logs_complete(&snapshot));
    assert!(
        placement_diagnostic_evidence(true, Some(&snapshot), false, true, true, false).is_err()
    );
    assert!(
        snapshot
            .placement
            .as_ref()
            .unwrap()
            .validate_strict()
            .is_err()
    );
    Ok(())
}
#[test]
fn graph_diagnostic_rejects_incomplete_graph_and_node_provider_mismatch() -> Result<()> {
    let snapshot = synthetic_graph_snapshot()?;
    graph_diagnostic_evidence(true, Some(&snapshot), false, true, true, false)?;
    let mut partial = snapshot.clone();
    partial.graph_metadata.as_mut().unwrap().complete = false;
    assert!(graph_diagnostic_evidence(true, Some(&partial), false, true, true, false).is_err());
    let mut changed = snapshot;
    changed.placement.as_mut().unwrap().nodes[0].ep = "CUDAExecutionProvider".into();
    assert!(graph_diagnostic_evidence(true, Some(&changed), false, true, true, false).is_err());
    Ok(())
}
