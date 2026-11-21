// SPDX-License-Identifier: Apache-2.0
//! Private exact-graph input and matched CPU metadata for CUDA Answer timing.

use super::*;
use airk_core::{
    arbiter::{
        Asset, AssetRole, Task,
        context_action::calibration::{CALIBRATION_SCHEMA_V2, JointCalibration},
        device::{DevicePrecision, GpuBackend, PlacementPolicy},
        resource_policy::{ResourceFilePin, ResourcePolicy},
    },
    identity::framed_digest,
};
use airk_inference::inference_engine::pool::arbiter::EvaluationAnswerGraph;

const MAX_GRAPH_BYTES: u64 = 16 << 20;
const COMPARATOR_CALIBRATION_NAME: &str = "context-action-calibration.json";

pub(super) struct Input {
    pub(super) manifest: Sha256Digest,
    pub(super) adapter: [u8; 16],
    pub(super) graph: EvaluationAnswerGraph,
}

impl Input {
    pub(super) fn read(artifacts: &Path, prepared: &[Prepared], expires: Instant) -> Result<Self> {
        let graph_path = PathBuf::from(
            std::env::var_os("AIRK_ARBITER_EVALUATION_CUDA_GRAPH")
                .context("private CUDA Answer requires a reviewed graph result")?,
        );
        let expected: Sha256Digest =
            std::env::var("AIRK_ARBITER_EVALUATION_CUDA_GRAPH_SHA256")?.parse()?;
        let adapter: [u8; 16] =
            serde_json::from_str(&std::env::var("AIRK_ARBITER_EVALUATION_CUDA_ADAPTER_UUID")?)?;
        ensure!(
            adapter != [0; 16],
            "private CUDA Answer adapter identity missing"
        );
        let (_directory, file) = source_file(&graph_path, artifacts)?;
        let mut graph = Vec::new();
        file.take(MAX_GRAPH_BYTES + 1).read_to_end(&mut graph)?;
        ensure!(
            !graph.is_empty() && graph.len() as u64 <= MAX_GRAPH_BYTES && sha(&graph) == expected,
            "private CUDA Answer graph size or SHA differs"
        );
        let result: serde_json::Value = serde_json::from_slice(&graph)?;
        let manifest = serde_json::from_value(result["profile"].clone())?;
        ensure!(
            prepared.iter().any(|item| {
                item.profile.manifest == manifest
                    && item.profile.model == ModelChoice::ContextQwenCompact
                    && item.profile.device_execution.is_some()
            }),
            "exact CUDA Compact graph profile missing from the isolated fixture"
        );
        Ok(Self {
            manifest,
            adapter,
            graph: EvaluationAnswerGraph::new(graph.into(), expected, expires),
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ComparatorInputs {
    cpu_profile: ResourceFilePin,
    gpu_profile: ResourceFilePin,
    gpu_calibration: ResourceFilePin,
}

fn pinned_comparator_bytes(pin: &ResourceFilePin, artifacts: &Path) -> Result<Vec<u8>> {
    let (_directory, mut file) = source_file(Path::new(&pin.path), artifacts)?;
    let bytes = bounded(&mut file)?;
    ensure!(
        sha(&bytes) == pin.sha256,
        "CPU comparator input SHA differs"
    );
    Ok(bytes)
}

fn require_calibration_bytes(
    raw: &[u8],
    declared: &Asset,
    profile_calibration: Sha256Digest,
) -> Result<()> {
    ensure!(
        declared.role == AssetRole::Calibration
            && declared.name == COMPARATOR_CALIBRATION_NAME
            && declared.bytes == raw.len() as u64
            && declared.sha256 == profile_calibration
            && sha(raw) == profile_calibration,
        "CPU comparator requires the exact held CUDA calibration bytes"
    );
    Ok(())
}

fn cpu_comparator_profile(
    mut cpu: Profile,
    gpu: &Profile,
    raw_calibration: &[u8],
    source: &ComparatorInputs,
) -> Result<Profile> {
    let policy = ResourcePolicy::software_v2();
    let task_policy = airk_retrieval::arbiter::context_action::policy_digest()?;
    for profile in [&cpu, gpu] {
        profile.validate()?;
        ensure!(
            profile.model == ModelChoice::ContextQwenCompact
                && profile.profile_id.starts_with("private-")
                && profile.qualification.is_none()
                && profile.qualified_targets.is_empty()
                && profile.downloads.is_empty()
                && profile.max_tokens == policy.context_input_tokens
                && profile.max_candidates == policy.maximum_context_candidates
                && profile.tasks == BTreeSet::from([Task::ContextAction])
                && profile.task_policies.get(&Task::ContextAction) == Some(&task_policy),
            "CPU comparator requires unqualified Compact profiles under the current Context policy"
        );
    }
    ensure!(
        cpu.device_execution.is_none()
            && gpu.device_execution.as_ref().is_some_and(|device| {
                device.backend == GpuBackend::Cuda
                    && device.precision == DevicePrecision::Float32
                    && !device.tf32
                    && device.placement == PlacementPolicy::StrictGpu
                    && cpu.cpu_fit.as_ref() == Some(&device.host_fit)
            })
            && cpu.compiler == gpu.compiler
            && cpu.policy == gpu.policy
            && cpu.abi == gpu.abi
            && cpu
                .assets
                .iter()
                .filter(|asset| asset.role != AssetRole::Calibration)
                .eq(gpu
                    .assets
                    .iter()
                    .filter(|asset| asset.role != AssetRole::Calibration)),
        "CPU and CUDA comparator profiles differ in a frozen execution input"
    );
    let gpu_calibration = gpu
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("CUDA comparator calibration asset missing")?;
    require_calibration_bytes(raw_calibration, gpu_calibration, gpu.calibration)?;
    let calibration = JointCalibration::parse_and_validate(raw_calibration, gpu, task_policy)?;
    ensure!(
        calibration.schema == CALIBRATION_SCHEMA_V2 && calibration.rules.is_empty(),
        "CPU comparator requires the CUDA all-unavailable calibration"
    );
    calibration.validate(&cpu, task_policy)?;

    let replaced = cpu
        .assets
        .iter_mut()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("CPU comparator calibration asset missing")?;
    ensure!(
        replaced.name == gpu_calibration.name,
        "CPU and CUDA comparator calibration names differ"
    );
    *replaced = gpu_calibration.clone();
    cpu.calibration = gpu.calibration;
    cpu.profile_id = "private-compact-answer-cpu-comparator-v1".into();
    cpu.manifest = Sha256Digest::from_bytes([0; 32]);
    cpu.manifest = framed_digest(
        b"airk.private.compact-answer-cpu-comparator/1",
        &[
            source.cpu_profile.sha256.as_bytes(),
            source.gpu_profile.sha256.as_bytes(),
            source.gpu_calibration.sha256.as_bytes(),
            policy.digest()?.as_bytes(),
            &serde_json::to_vec(&cpu)?,
        ],
    );
    cpu.validate()?;
    Ok(cpu)
}

#[test]
fn paired_compact_cpu_comparator_refuses_tampered_calibration_bytes() -> Result<()> {
    let raw = b"abc";
    let original: Sha256Digest =
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".parse()?;
    let asset = Asset {
        name: "context-action-calibration.json".into(),
        role: AssetRole::Calibration,
        bytes: 3,
        sha256: original,
    };
    require_calibration_bytes(raw, &asset, original)?;
    assert!(require_calibration_bytes(b"abd", &asset, original).is_err());
    assert!(require_calibration_bytes(b"abc\n", &asset, original).is_err());
    assert!(require_calibration_bytes(raw, &asset, Sha256Digest::from_bytes([1; 32])).is_err());
    Ok(())
}

#[test]
#[ignore = "private data-only paired CPU comparator metadata from exact held Compact inputs"]
fn emit_paired_compact_cpu_comparator() -> Result<()> {
    let artifacts = artifact_root()?;
    let inputs = ResourceFilePin {
        path: std::env::var("AIRK_ARBITER_EVALUATION_CPU_COMPARATOR_INPUTS")?,
        sha256: std::env::var("AIRK_ARBITER_EVALUATION_CPU_COMPARATOR_INPUTS_SHA256")?.parse()?,
    };
    let source: ComparatorInputs =
        serde_json::from_slice(&pinned_comparator_bytes(&inputs, &artifacts)?)?;
    let cpu: Profile =
        serde_json::from_slice(&pinned_comparator_bytes(&source.cpu_profile, &artifacts)?)?;
    let gpu: Profile =
        serde_json::from_slice(&pinned_comparator_bytes(&source.gpu_profile, &artifacts)?)?;
    let raw_calibration = pinned_comparator_bytes(&source.gpu_calibration, &artifacts)?;
    let source_cpu = cpu.manifest;
    let comparator = cpu_comparator_profile(cpu, &gpu, &raw_calibration, &source)?;
    let mut profile = serde_json::to_vec_pretty(&comparator)?;
    profile.push(b'\n');

    let work = PathBuf::from(
        std::env::var_os("AIRK_ARBITER_EVALUATION_CPU_COMPARATOR_OUTPUT")
            .context("private CPU comparator output path missing")?,
    );
    ensure!(
        work.is_absolute() && !work.try_exists()?,
        "fresh absolute CPU comparator output required"
    );
    let parent = work
        .parent()
        .context("CPU comparator output parent")?
        .canonicalize()?;
    ensure!(
        parent.starts_with(&artifacts),
        "private CPU comparator output must remain under artifacts"
    );
    let work = parent.join(
        work.file_name()
            .context("CPU comparator output directory name")?,
    );
    let cal = comparator
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("built CPU comparator calibration missing")?;
    let parent = OpenedDirectory::open(&parent)?;
    let _output = crate::ipc::endpoint::create_root_at(
        &parent,
        work.file_name()
            .context("CPU comparator output directory name")?,
    )?;
    crate::ipc::private_record::write_private(&work, &cal.name, &raw_calibration, LIMIT, false)?;
    crate::ipc::private_record::write_private(
        &work,
        "experimental-profile.json",
        &profile,
        LIMIT,
        false,
    )?;
    write(
        &work,
        "comparator-metadata.json",
        &serde_json::json!({
            "schema": "airk.private.compact-answer-cpu-comparator/1",
            "complete": true,
            "evaluation": true,
            "release_qualified": false,
            "inputs": inputs,
            "sources": source,
            "source_cpu_manifest": source_cpu,
            "source_gpu_manifest": gpu.manifest,
            "manifest": comparator.manifest,
            "profile": {
                "name": "experimental-profile.json",
                "bytes": profile.len(),
                "sha256": sha(&profile),
            },
            "calibration": cal,
        }),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "private Windows CUDA Answer timing with exact reviewed fixture inputs"]
async fn actual_private_arbiter_fixture_cuda_answer() -> Result<()> {
    let consent = std::env::var("AIRK_ARBITER_EVALUATION_RESOURCE_CONSENT")
        .context("current CUDA fixture requires explicit resource configuration consent")?;
    run_actual_private_arbiter_fixture(
        software_v2_answer_template()?,
        Some(cuda_answer_resource_configuration(&consent)?),
        true,
    )
    .await
}
