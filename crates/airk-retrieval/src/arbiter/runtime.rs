// SPDX-License-Identifier: Apache-2.0
//! Full trained readouts under native service compute ownership.
use super::{
    compiler::{CompiledInput, Compiler},
    external_data,
};
use airk_core::{arbiter::*, identity::Sha256Digest};
use anyhow::{Context, Result, ensure};
use ort::{
    execution_providers::CPUExecutionProvider,
    session::{
        Session,
        builder::GraphOptimizationLevel,
        run_options::{OutputSelector, RunOptions},
    },
    value::{Tensor, ValueType},
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};

mod cpu_source_transition;
#[cfg(feature = "gpu-cuda")]
pub mod cuda;
#[cfg(test)]
mod embedded_catalog_tests;
mod input;
#[cfg(all(test, feature = "test-support"))]
mod local_test_catalog_emitter;
#[cfg(test)]
mod local_test_tests;
#[cfg(feature = "test-support")]
pub mod qualification;
mod session_owner;

/// Source-policy checks precede asset reads/session construction. New Context
/// compiler policy is distinct from the whole-card semantic task policy.
pub(super) fn validate_selected_policies(profile: &Profile) -> Result<()> {
    let Some(stage) = profile.model.stage() else {
        return Ok(());
    };
    let (task, task_policy, compiler_policy) = match stage {
        Stage::Focus => {
            let policy = super::compiler::focus::policy_digest()?;
            (Task::FocusRank, policy, policy)
        }
        Stage::Context => (
            Task::ContextAction,
            super::context_action::policy_digest()?,
            super::compiler::qwen::policy_digest()?,
        ),
    };
    ensure!(
        profile.stage == Some(stage)
            && profile.tasks == BTreeSet::from([task])
            && profile.task_policies == BTreeMap::from([(task, task_policy)])
            && profile.policy == compiler_policy,
        "selected model compiler/task source policy is stale or inconsistent"
    );
    Ok(())
}

pub trait AssetSource {
    /// Read exactly this bounded asset. The execution facade authenticates the
    /// actual returned bytes; a path/stat/hash claim never substitutes for them.
    fn read_asset(&self, asset: &Asset) -> Result<Vec<u8>>;
}

pub fn current_target() -> &'static str {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "x86_64-pc-windows-msvc"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu"
    } else {
        "unsupported"
    }
}

/// Prefix that marks a compiled profile as an unqualified local-test candidate.
pub const LOCAL_TEST_PROFILE_PREFIX: &str = "unqualified-local-test";

/// Fail closed: only an exact `major.minor.patch` below 1.9.0 is a local-test
/// series. Anything else (1.9.0 and later, pre-release suffixes, garbage) is not.
fn local_test_series(version: &str) -> bool {
    let mut parts = version.split('.');
    let (Some(major), Some(minor), Some(patch), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let number = |part: &str| {
        (!part.is_empty()
            && part.bytes().all(|b| b.is_ascii_digit())
            && (part == "0" || !part.starts_with('0')))
        .then(|| part.parse::<u64>().ok())
        .flatten()
    };
    let (Some(major), Some(minor), Some(_)) = (number(major), number(minor), number(patch)) else {
        return false;
    };
    (major, minor) < (1, 9)
}

fn local_test_profile_for(profile: &Profile, version: &str) -> bool {
    profile.profile_id.starts_with(LOCAL_TEST_PROFILE_PREFIX) && local_test_series(version)
}

/// Release admission: a qualified native target row, a qualification digest and
/// a CPU fit. An unqualified local-test profile in a pre-1.9.0 series skips only
/// the target row and the qualification digest, never the CPU fit.
fn release_qualification_gate(profile: &Profile, version: &str) -> Result<()> {
    let local_test = local_test_profile_for(profile, version);
    if !local_test {
        ensure!(
            profile
                .qualified_targets
                .iter()
                .any(|row| row.target == current_target()),
            "Arbiter profile has no qualification on this native target"
        );
    }
    ensure!(
        (local_test || profile.qualification.is_some()) && profile.cpu_fit.is_some(),
        "Arbiter release qualification missing"
    );
    Ok(())
}

/// True for a compiled unqualified local-test profile in a pre-1.9.0 build.
/// Never true for release series, so a final build refuses these profiles.
pub fn is_local_test_profile(profile: &Profile) -> bool {
    local_test_profile_for(profile, env!("CARGO_PKG_VERSION"))
}

/// No Deserialize implementation: downloaded metadata cannot mint this proof.
pub struct ExecutionProfile {
    description: Profile,
    release: bool,
    evaluation: Option<Arc<dyn Send + Sync>>,
    source_transition: Option<cpu_source_transition::CpuSourceTransition>,
    #[cfg(feature = "gpu-cuda")]
    gpu: Option<cuda::Binding>,
}
/// Descriptions come from the same immutable native owner as execution. A
/// listed candidate is not an execution capability or qualification claim.
pub fn catalog_profiles() -> Result<Vec<Profile>> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Catalog {
        schema: String,
        profiles: Vec<Profile>,
    }
    let catalog: Catalog = serde_json::from_slice(include_bytes!("release-catalog.json"))?;
    ensure!(
        catalog.schema == CATALOG_SCHEMA && catalog.profiles.len() <= 16,
        "invalid embedded Arbiter catalog"
    );
    let mut seen = BTreeSet::new();
    for profile in &catalog.profiles {
        profile.validate()?;
        ensure!(
            seen.insert(profile.manifest),
            "duplicate Arbiter catalog manifest"
        );
    }
    Ok(catalog.profiles)
}
fn admit_source_transition(
    profile: &Profile,
) -> Result<Option<cpu_source_transition::CpuSourceTransition>> {
    match validate_selected_policies(profile) {
        Ok(()) => Ok(None),
        Err(current_error) => match cpu_source_transition::CpuSourceTransition::admit(profile) {
            Ok(witness) => Ok(Some(witness)),
            Err(_) => Err(current_error),
        },
    }
}
impl ExecutionProfile {
    fn validate_native_source(&self) -> Result<()> {
        match &self.source_transition {
            Some(witness) => witness.validate(&self.description),
            None => validate_selected_policies(&self.description),
        }
    }
    /// Return this exact admitted profile's declared task identity, only after
    /// current source or the closed native CPU witness has been revalidated.
    pub fn validated_task_policy(&self, task: Task) -> Result<Sha256Digest> {
        self.validate_native_source()?;
        self.description
            .task_policies
            .get(&task)
            .copied()
            .context("task not in admitted native profile")
    }
    pub fn source_transition_record(&self) -> Result<Option<serde_json::Value>> {
        self.validate_native_source()?;
        if let Some(witness) = &self.source_transition {
            return Ok(Some(serde_json::to_value(
                witness.record(&self.description)?,
            )?));
        }
        Ok(None)
    }
    pub fn from_release(manifest: Sha256Digest) -> Result<Self> {
        let description = catalog_profiles()?
            .into_iter()
            .find(|profile| profile.manifest == manifest)
            .context("Arbiter profile is not release-qualified")?;
        description.validate()?;
        ensure!(
            description.device_execution.is_none(),
            "GPU release requires actual native device witness"
        );
        description.validate_release_downloads()?;
        let source_transition = admit_source_transition(&description)?;
        release_qualification_gate(&description, env!("CARGO_PKG_VERSION"))?;
        if description.tasks.contains(&Task::ContextNextRead) {
            ensure!(
                description.task_policies.get(&Task::ContextNextRead)
                    == Some(&super::tasks::policy_digest()?),
                "Arbiter next-read task policy differs from qualified profile"
            );
        }
        if description.tasks.contains(&Task::ContextAction) {
            ensure!(
                description.task_policies.get(&Task::ContextAction)
                    == Some(&super::context_action::policy_digest()?),
                "Arbiter joint action policy differs from qualified profile"
            );
        }
        if description.tasks.contains(&Task::FocusRank) {
            ensure!(
                source_transition.is_some()
                    || description.task_policies.get(&Task::FocusRank)
                        == Some(&super::compiler::focus::policy_digest()?),
                "Arbiter Focus source policy differs from qualified profile"
            );
        }
        Ok(Self {
            description,
            release: true,
            evaluation: None,
            source_transition,
            #[cfg(feature = "gpu-cuda")]
            gpu: None,
        })
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_qualification(description: Profile) -> Result<Self> {
        description.validate()?;
        Ok(Self {
            description,
            release: false,
            evaluation: None,
            source_transition: None,
            #[cfg(feature = "gpu-cuda")]
            gpu: None,
        })
    }
    pub fn execution_authorized(&self) -> bool {
        self.release || self.evaluation.is_some()
    }
    pub fn evaluation(&self) -> bool {
        self.evaluation.is_some()
    }
    pub fn description(&self) -> &Profile {
        &self.description
    }
    pub fn release_qualified(&self) -> bool {
        self.release
    }
}

/// Private native evaluation capability. No serialized value constructs it.
#[cfg(feature = "test-support")]
#[derive(Clone)]
pub struct EvaluationCatalog {
    lease: Arc<dyn Send + Sync>,
    profiles: Arc<Vec<Profile>>,
}
#[cfg(feature = "test-support")]
impl EvaluationCatalog {
    /// # Safety
    /// `lease` must be the actual exclusive native ServiceLease retained by the
    /// private evaluation fixture, never a client supplied descriptor or flag.
    pub unsafe fn from_native_lease(
        lease: Arc<dyn Send + Sync>,
        profiles: Vec<Profile>,
    ) -> Result<Self> {
        ensure!(
            !profiles.is_empty() && profiles.len() <= 16,
            "invalid evaluation catalog size"
        );
        let mut seen = BTreeSet::new();
        for profile in &profiles {
            profile.validate()?;
            admit_source_transition(profile)?;
            ensure!(
                profile.qualification.is_none(),
                "evaluation cannot mint final qualification"
            );
            ensure!(
                seen.insert(profile.manifest),
                "duplicate evaluation manifest"
            );
        }
        Ok(Self {
            lease,
            profiles: Arc::new(profiles),
        })
    }
    pub fn profiles(&self) -> Vec<Profile> {
        self.profiles.as_ref().clone()
    }
    pub fn execution_profile(&self, manifest: Sha256Digest) -> Result<ExecutionProfile> {
        let description = self
            .profiles
            .iter()
            .find(|p| p.manifest == manifest)
            .context("profile outside private evaluation catalog")?
            .clone();
        let source_transition = admit_source_transition(&description)?;
        if description.tasks.contains(&Task::ContextNextRead) {
            ensure!(
                description.task_policies.get(&Task::ContextNextRead)
                    == Some(&super::tasks::policy_digest()?),
                "evaluation task policy differs from native constructor"
            );
        }
        if description.tasks.contains(&Task::ContextAction) {
            ensure!(
                description.task_policies.get(&Task::ContextAction)
                    == Some(&super::context_action::policy_digest()?),
                "evaluation joint action policy differs from native constructor"
            );
        }
        if description.tasks.contains(&Task::FocusRank) {
            ensure!(
                source_transition.is_some()
                    || description.task_policies.get(&Task::FocusRank)
                        == Some(&super::compiler::focus::policy_digest()?),
                "evaluation Focus source policy differs from native constructor"
            );
        }
        Ok(ExecutionProfile {
            description,
            release: false,
            evaluation: Some(self.lease.clone()),
            source_transition,
            #[cfg(feature = "gpu-cuda")]
            gpu: None,
        })
    }
}

pub struct Scorer {
    profile: ExecutionProfile,
    compiler: Compiler,
    session: Mutex<session_owner::SessionOwner>,
    _environment: crate::onnx::SessionEnvironmentLease,
    physical_cores: usize,
    joint_calibration:
        Option<Arc<airk_core::arbiter::context_action::calibration::JointCalibration>>,
}

fn bytes(source: &dyn AssetSource, asset: &Asset, maximum: u64) -> Result<Vec<u8>> {
    ensure!(
        asset.bytes <= maximum,
        "Arbiter asset exceeds execution admission"
    );
    let bytes = source.read_asset(asset)?;
    ensure!(
        bytes.len() as u64 == asset.bytes
            && Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == asset.sha256,
        "Arbiter asset bytes differ from authenticated catalog"
    );
    Ok(bytes)
}

enum OpenMode {
    Execution,
    #[cfg(all(feature = "gpu-cuda", feature = "test-support"))]
    PlacementDiagnostic,
}
impl Scorer {
    pub fn open(profile: ExecutionProfile, source: &dyn AssetSource) -> Result<Self> {
        Self::open_configured(profile, source, Ok)
    }
    /// Private profiling uses ordinary production initializer retention.
    /// This setter adds kernel events only; no optimized model is requested.
    #[cfg(any(test, feature = "test-support"))]
    pub fn open_profiled(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: &std::path::Path,
    ) -> Result<Self> {
        Self::open_configured(profile, source, |builder| {
            Ok(builder.with_profiling(profiling_prefix)?)
        })
    }
    fn open_configured(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        configure: impl FnOnce(
            ort::session::builder::SessionBuilder,
        ) -> Result<ort::session::builder::SessionBuilder>,
    ) -> Result<Self> {
        match Self::open_internal(profile, source, configure, OpenMode::Execution)? {
            Some(scorer) => Ok(scorer),
            None => {
                anyhow::bail!("diagnostic cannot produce an execution scorer")
            }
        }
    }
    #[cfg(all(feature = "gpu-cuda", feature = "test-support"))]
    pub fn diagnose_cuda_placement(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
    ) -> Result<()> {
        cuda::diagnostic::require_diagnostic_profile(&profile)?;
        match Self::open_internal(profile, source, Ok, OpenMode::PlacementDiagnostic)? {
            None => Ok(()),
            Some(_) => anyhow::bail!("diagnostic unexpectedly constructed a scorer"),
        }
    }
    fn open_internal(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        configure: impl FnOnce(
            ort::session::builder::SessionBuilder,
        ) -> Result<ort::session::builder::SessionBuilder>,
        _mode: OpenMode,
    ) -> Result<Option<Self>> {
        crate::native_inference::require_service_compute()?;
        profile.description.validate()?;
        profile.validate_native_source()?;
        #[cfg(feature = "gpu-cuda")]
        let gpu = profile.gpu.clone();
        #[cfg(feature = "gpu-cuda")]
        if let Some(binding) = &gpu {
            binding.device.validate_profile(profile.description())?;
            cuda::in_time(binding.load_deadline)?;
        }
        ensure!(
            profile.description.runtime == crate::model::ONNX_RUNTIME_ENGINE || {
                #[cfg(feature = "gpu-cuda")]
                {
                    gpu.is_some()
                }
                #[cfg(not(feature = "gpu-cuda"))]
                {
                    false
                }
            },
            "Arbiter runtime differs from native image"
        );
        let fit = profile
            .description
            .device_execution
            .as_ref()
            .map(|v| v.host_fit.clone())
            .or_else(|| profile.description.cpu_fit.clone())
            .unwrap_or_default();
        fit.validate()?;
        let physical_cores = airk_process::cpu_fit::usable_physical_cores()
            .context("usable physical-core observation unavailable")?;
        ensure!(
            physical_cores >= fit.minimum_physical_cores,
            "native physical-core fit refused"
        );
        let asset = |role: AssetRole| -> Result<&Asset> {
            let selected: Vec<_> = profile
                .description
                .assets
                .iter()
                .filter(|asset| asset.role == role)
                .collect();
            ensure!(
                selected.len() == 1,
                "Arbiter singleton asset missing or duplicated"
            );
            Ok(selected[0])
        };
        let compiler = Compiler::from_verified(
            &bytes(source, asset(AssetRole::Compiler)?, 64 << 10)?,
            &bytes(source, asset(AssetRole::Tokenizer)?, 64 << 20)?,
        )?;
        let joint_calibration = if profile.description.tasks.contains(&Task::ContextAction) {
            Some(Arc::new(airk_core::arbiter::context_action::calibration::JointCalibration::parse_and_validate(
                &bytes(source, asset(AssetRole::Calibration)?, 64 << 10)?,
                profile.description(), super::context_action::policy_digest()?)?))
        } else {
            None
        };
        let expected_recipe = match profile.description.model {
            ModelChoice::SmallIntern | ModelChoice::LargeIntern => {
                CompilerRecipe::InternSingleChoiceV1
            }
            ModelChoice::SmallStrands => CompilerRecipe::StrandsSingleChoiceV1,
            ModelChoice::LargeKev => CompilerRecipe::KevSingleChoiceV1,
            ModelChoice::ContextQwenCompact | ModelChoice::ContextQwenLarge => {
                CompilerRecipe::QwenSingleChoiceV1
            }
            ModelChoice::FocusQwenReranker06B => CompilerRecipe::QwenRerankYesNoV1,
        };
        ensure!(
            compiler.recipe() == expected_recipe,
            "Arbiter model/compiler recipe mismatch"
        );
        if matches!(
            expected_recipe,
            CompilerRecipe::QwenSingleChoiceV1 | CompilerRecipe::QwenRerankYesNoV1
        ) {
            ensure!(
                compiler.limits().0 == profile.description.max_tokens,
                "selected Qwen compiler/profile token ceilings differ"
            );
            if expected_recipe == CompilerRecipe::QwenSingleChoiceV1 {
                ensure!(
                    compiler.limits().1 == profile.description.max_candidates,
                    "selected Qwen compiler/profile closed-card ceilings differ"
                );
            }
        }
        let graph = bytes(source, asset(AssetRole::Model)?, 64 << 20)?;
        let external: BTreeMap<_, _> = profile
            .description
            .assets
            .iter()
            .filter(|asset| asset.role == AssetRole::ExternalWeights)
            .map(|asset| (asset.name.clone(), asset.bytes))
            .collect();
        external_data::validate(&graph, &external)?;
        let large_initializers = external_data::large_initializers(&graph, &external)?;
        crate::onnx::initialize_runtime()?;
        let environment = crate::onnx::acquire_session_environment()?;
        #[cfg(feature = "gpu-cuda")]
        let cold = match &gpu {
            Some(b) => Some(b.device.reserve_cold_load(
                b.plan.cold_peak_bytes(),
                b.plan.headroom_bytes(),
                b.load_deadline,
            )?),
            None => None,
        };
        let mut builder = Session::builder()?
            .with_intra_threads(fit.intra_threads)?
            .with_inter_threads(fit.inter_threads)?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_parallel_execution(false)?
            .with_memory_pattern(false)?
            .with_config_entry("session.intra_op.allow_spinning", "0")?
            .with_config_entry("session.inter_op.allow_spinning", "0")?;
        #[cfg(feature = "gpu-cuda")]
        {
            builder = match &gpu {
                Some(binding) => binding.configure(builder)?,
                None => builder.with_execution_providers([CPUExecutionProvider::default()
                    .with_arena_allocator(false)
                    .build()])?,
            };
        }
        #[cfg(not(feature = "gpu-cuda"))]
        {
            builder = builder.with_execution_providers([CPUExecutionProvider::default()
                .with_arena_allocator(false)
                .build()])?;
        }
        builder = configure(builder)?;
        #[cfg(all(feature = "gpu-cuda", feature = "test-support"))]
        if matches!(_mode, OpenMode::PlacementDiagnostic) {
            cuda::diagnostic::require_diagnostic_profile(&profile)?;
            builder = builder.with_config_entry("session.disable_cpu_ep_fallback", "0")?;
        }
        for asset in profile
            .description
            .assets
            .iter()
            .filter(|asset| asset.role == AssetRole::ExternalWeights)
        {
            let verified = bytes(source, asset, 16u64 << 30)?;
            for tensor in large_initializers
                .iter()
                .filter(|tensor| tensor.file == asset.name)
            {
                let start = usize::try_from(tensor.offset)?;
                let end = usize::try_from(
                    tensor
                        .offset
                        .checked_add(tensor.length)
                        .context("large initializer range overflow")?,
                )?;
                let data = verified
                    .get(start..end)
                    .context("large initializer outside verified bytes")?;
                let value = match tensor.element {
                    1 => {
                        let values: Vec<f32> = data
                            .as_chunks::<4>()
                            .0
                            .iter()
                            .map(|chunk| f32::from_bits(u32::from_le_bytes(*chunk)))
                            .collect();
                        Tensor::from_array((tensor.shape.clone(), values))?.into_dyn()
                    }
                    2 => Tensor::from_array((tensor.shape.clone(), data.to_vec()))?.into_dyn(),
                    _ => anyhow::bail!("unsupported large initializer element"),
                };
                // AddExternalInitializers runs BEFORE file-in-memory injection
                // in the pinned ORT 1.30 Initialize path. The injected large
                // OrtValue becomes in-memory external data, so the later file
                // pass skips it rather than embedding a >2GiB TensorProto.
                builder = builder.with_external_initializer(&tensor.name, Arc::new(value))?;
            }
            builder = builder
                .with_external_initializer_file_in_memory(&asset.name, Cow::Owned(verified))?;
        }
        // Every external reference has been audited and supplied from verified
        // memory. Native loading never reopens an asset path or source file.
        #[cfg(feature = "gpu-cuda")]
        if let Some(binding) = &gpu {
            cuda::in_time(binding.load_deadline)?;
        }
        let session = builder
            .commit_from_memory(&graph)
            .map_err(|error| {
                #[cfg(feature = "gpu-cuda")]
                if gpu.is_some() {
                    return cuda::run_failure(error);
                }
                anyhow::Error::from(error)
            })
            .context("Arbiter native model load failed")?;
        let session = session_owner::SessionOwner::new(
            session,
            #[cfg(feature = "gpu-cuda")]
            gpu.clone(),
        );
        #[cfg(all(feature = "gpu-cuda", feature = "test-support"))]
        if matches!(_mode, OpenMode::PlacementDiagnostic) {
            cuda::diagnostic::session_created(gpu.as_ref().context("diagnostic binding missing")?)?;
        }
        #[cfg(feature = "gpu-cuda")]
        if let Some(claim) = cold {
            claim.settle()?;
        }
        #[cfg(feature = "gpu-cuda")]
        if let Some(binding) = &gpu {
            let diagnostic = {
                #[cfg(feature = "test-support")]
                {
                    matches!(_mode, OpenMode::PlacementDiagnostic)
                }
                #[cfg(not(feature = "test-support"))]
                {
                    false
                }
            };
            if !diagnostic {
                binding.device.placement(&session)?.require_strict()?;
            }
        }

        let focus = profile.description.abi.readout == Readout::FocusYesNo;
        let output_name = if focus { "logits" } else { "scores" };
        let expected = if focus {
            BTreeSet::from(["input_ids", "attention_mask", "position_ids"])
        } else {
            BTreeSet::from([
                "input_ids",
                "attention_mask",
                "position_ids",
                "decision_positions",
                "candidate_positions",
                "option_mask",
            ])
        };
        ensure!(
            session
                .inputs
                .iter()
                .map(|input| input.name.as_str())
                .collect::<BTreeSet<_>>()
                == expected,
            "Arbiter native input catalog differs"
        );
        ensure!(
            profile
                .description
                .abi
                .inputs
                .iter()
                .map(|input| input.name.as_str())
                .collect::<BTreeSet<_>>()
                == expected
                && profile.description.abi.output.name == output_name
                && profile.description.abi.output.axes.len() == 2,
            "Arbiter declared ABI differs from supported prefill recipe"
        );
        ensure!(
            session
                .outputs
                .iter()
                .any(|output| output.name == output_name),
            "Arbiter native scores missing"
        );
        if matches!(
            expected_recipe,
            CompilerRecipe::QwenSingleChoiceV1 | CompilerRecipe::QwenRerankYesNoV1
        ) {
            ensure!(
                session.outputs.len() == 1,
                "selected Qwen production graph must expose only its declared scores"
            );
        }
        for input in &session.inputs {
            let ValueType::Tensor { ty, shape, .. } = &input.input_type else {
                anyhow::bail!("Arbiter input is not a tensor")
            };
            let expected = if input.name == "option_mask" {
                ort::tensor::TensorElementType::Bool
            } else {
                ort::tensor::TensorElementType::Int64
            };
            ensure!(*ty == expected, "Arbiter native input element type differs");
            let declared = profile
                .description
                .abi
                .inputs
                .iter()
                .find(|item| item.name == input.name)
                .unwrap();
            let element = if input.name == "option_mask" {
                TensorType::Bool
            } else {
                TensorType::Int64
            };
            let rank = if input.name == "decision_positions" {
                1
            } else {
                2
            };
            ensure!(
                declared.element == element
                    && declared.axes.len() == rank
                    && shape.len() == rank
                    && shape[0] == 1,
                "Arbiter native input rank or batch differs"
            );
            input::selected_graph_shape(
                &input.name,
                expected_recipe,
                profile.description.max_tokens,
                shape.as_ref(),
            )?;
        }
        let output = session
            .outputs
            .iter()
            .find(|output| output.name == output_name)
            .unwrap();
        ensure!(
            matches!(&output.output_type,ValueType::Tensor {ty,shape,..}
            if *ty==ort::tensor::TensorElementType::Float32 && shape.len()==2),
            "Arbiter native output ABI differs"
        );
        if focus {
            ensure!(
                matches!(&output.output_type, ValueType::Tensor {shape,..} if shape.as_ref() == [1,2]),
                "Focus output must be the exact raw no/yes pair"
            );
        }
        #[cfg(all(feature = "gpu-cuda", feature = "test-support"))]
        if matches!(_mode, OpenMode::PlacementDiagnostic) {
            cuda::diagnostic::finish(session, gpu.as_ref().context("diagnostic binding missing")?)?;
            return Ok(None);
        }
        Ok(Some(Self {
            profile,
            compiler,
            session: Mutex::new(session),
            _environment: environment,
            physical_cores,
            joint_calibration,
        }))
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn finish_profiling(&self) -> Result<std::path::PathBuf> {
        crate::native_inference::require_service_compute()?;
        Ok(self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter native session poisoned"))?
            .end_profiling()?
            .into())
    }
    pub fn execution_identity(&self) -> Result<(String, usize)> {
        #[cfg(feature = "gpu-cuda")]
        if self.profile.gpu.is_some() {
            return Ok((
                self.profile.description.device_execution_identity()?,
                self.physical_cores,
            ));
        }
        Ok((
            self.profile
                .description
                .cpu_fit
                .clone()
                .unwrap_or_default()
                .runtime_identity()?,
            self.physical_cores,
        ))
    }
    pub fn profile(&self) -> &Profile {
        self.profile.description()
    }
    /// Metadata authenticated once during this actual resident's warm. No
    /// asset-path reopen or whole-weight rehash on an assessment's deadline.
    pub fn joint_calibration(
        &self,
    ) -> Option<Arc<airk_core::arbiter::context_action::calibration::JointCalibration>> {
        self.joint_calibration.clone()
    }
    pub fn release_qualified(&self) -> bool {
        self.profile.release_qualified()
    }
    pub fn compile(
        &self,
        state: &str,
        question: &str,
        candidates: &[super::compiler::Candidate],
    ) -> Result<CompiledInput> {
        self.profile.validate_native_source()?;
        self.compiler.compile(state, question, candidates)
    }
    pub fn source_transition_record(&self) -> Result<Option<serde_json::Value>> {
        self.profile.source_transition_record()
    }
    pub fn source_transition_input(
        &self,
        input: &CompiledInput,
    ) -> Result<Option<serde_json::Value>> {
        self.profile.validate_native_source()?;
        if let Some(witness) = &self.profile.source_transition {
            return Ok(Some(serde_json::to_value(
                witness.bind_input(self.profile.description(), input.input_digest)?,
            )?));
        }
        Ok(None)
    }
    pub fn score(&self, input: &CompiledInput) -> Result<Vec<f32>> {
        #[cfg(feature = "gpu-cuda")]
        ensure!(
            self.profile.gpu.is_none(),
            "GPU scoring requires caller original deadline"
        );
        self.score_owned(input, None)
    }
    pub fn score_with_deadline(
        &self,
        input: &CompiledInput,
        deadline: std::time::Instant,
    ) -> Result<Vec<f32>> {
        ensure!(
            std::time::Instant::now() < deadline,
            "original scorer deadline expired"
        );
        self.score_owned(input, Some(deadline))
    }
    fn score_owned(
        &self,
        input: &CompiledInput,
        deadline: Option<std::time::Instant>,
    ) -> Result<Vec<f32>> {
        crate::native_inference::require_service_compute()?;
        self.profile.validate_native_source()?;
        #[cfg(not(feature = "gpu-cuda"))]
        let _ = deadline;
        let tokens = input.input_ids.len();
        let candidates = input.candidate_positions.len();
        let focus = self.profile.description.abi.readout == Readout::FocusYesNo;
        input::validate(
            input,
            self.profile.description.abi.readout,
            self.profile.description.max_tokens,
            self.profile.description.max_candidates,
        )?;
        self.compiler.validate_qwen_readout(input)?;
        let mut session = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter native session poisoned"))?;
        #[cfg(feature = "gpu-cuda")]
        if let Some(binding) = &self.profile.gpu {
            return cuda::score_gpu(
                &mut session,
                binding,
                input,
                deadline.context("original GPU score deadline required")?,
                focus,
            );
        }
        let output_name = if focus { "logits" } else { "scores" };
        let options =
            RunOptions::new()?.with_outputs(OutputSelector::no_default().with(output_name));
        let outputs = if focus { session.run_with_options(ort::inputs![
            "input_ids" => Tensor::from_array(([1,tokens],input.input_ids.clone()))?,
            "attention_mask" => Tensor::from_array(([1,tokens],input.attention_mask.clone()))?,
            "position_ids" => Tensor::from_array(([1,tokens],input.position_ids.clone()))?,
        ], &options) } else { session.run_with_options(ort::inputs![
            "input_ids" => Tensor::from_array(([1,tokens],input.input_ids.clone()))?,
            "attention_mask" => Tensor::from_array(([1,tokens],input.attention_mask.clone()))?,
            "position_ids" => Tensor::from_array(([1,tokens],input.position_ids.clone()))?,
            "decision_positions" => Tensor::from_array(([1],vec![input.decision_position]))?,
            "candidate_positions" => Tensor::from_array(([1,candidates],input.candidate_positions.clone()))?,
            "option_mask" => Tensor::from_array(([1,candidates],input.option_mask.clone()))?,
        ],&options) }.context("Arbiter full trained readout execution failed")?;
        let (shape, scores) = outputs[output_name].try_extract_tensor::<f32>()?;
        ensure!(
            shape.as_ref() == [1, candidates as i64]
                && scores.iter().all(|score| score.is_finite()),
            "Arbiter score shape or finite-value contract changed"
        );
        if let Some(deadline) = deadline {
            ensure!(
                std::time::Instant::now() < deadline,
                "original scorer deadline expired before delivery"
            );
        }
        Ok(scores.to_vec())
    }
}

/// Owning private experiment identity, shared only through test-support.
#[cfg(any(test, feature = "test-support"))]
pub const LARGE_DENSE_DIAGNOSTIC_PROFILE_ID: &str =
    "private-large-fp32-explicit-mask-dense-diagnostic-v1";

/// Literal selected attention dimensions for test-support diagnostics only.
/// Large query_width4096 is distinct from its backbone hidden width2560.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectedGqaModel {
    pub model: ModelChoice,
    pub query_width: usize,
    pub query_heads: usize,
    pub kv_heads: usize,
    pub head_size: usize,
    pub layers: usize,
    pub cpu_threads: usize,
    pub memory_reservation_bytes: u64,
}
#[cfg(any(test, feature = "test-support"))]
pub fn selected_gqa_model(model: ModelChoice) -> Result<SelectedGqaModel> {
    let (query_width, query_heads, layers, cpu_threads, memory_reservation_bytes) = match model {
        ModelChoice::ContextQwenCompact => (2048, 16, 28, 4, 16u64 << 30),
        ModelChoice::ContextQwenLarge => (4096, 32, 36, 16, 40u64 << 30),
        _ => anyhow::bail!("selected Context GQA diagnostic model required"),
    };
    Ok(SelectedGqaModel {
        model,
        query_width,
        query_heads,
        kv_heads: 8,
        head_size: 128,
        layers,
        cpu_threads,
        memory_reservation_bytes,
    })
}

/// Historical Compact synthetic control; its caller and receipt identity remain unchanged.
#[cfg(any(test, feature = "test-support"))]
pub fn synthetic_gqa_profile(
    graph: &[u8],
    tokens: usize,
    profiling_prefix: &std::path::Path,
    optimized_model: &std::path::Path,
) -> Result<serde_json::Value> {
    synthetic_gqa_profile_for_model(
        ModelChoice::ContextQwenCompact,
        graph,
        tokens,
        profiling_prefix,
        optimized_model,
    )
}

/// Synthetic GQA operator control only; no model weights or execution profile.
#[cfg(any(test, feature = "test-support"))]
pub fn synthetic_gqa_profile_for_model(
    model: ModelChoice,
    graph: &[u8],
    tokens: usize,
    profiling_prefix: &std::path::Path,
    optimized_model: &std::path::Path,
) -> Result<serde_json::Value> {
    synthetic_gqa_profile_with_bias(
        model,
        graph,
        tokens,
        profiling_prefix,
        optimized_model,
        None,
    )
}

#[cfg(any(test, feature = "test-support"))]
const SYNTHETIC_PADDING_VALUE_SPIKE: f32 = 0.25;

/// Test-support values only; this enum has no serialization or public protocol.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntheticDenseMask {
    CausalPrefix,
    Unmasked,
    CausalWithMaskedKeyOne,
}
#[cfg(any(test, feature = "test-support"))]
impl SyntheticDenseMask {
    fn masks(self, query: usize, key: usize) -> bool {
        match self {
            Self::CausalPrefix => key > query,
            Self::Unmasked => false,
            Self::CausalWithMaskedKeyOne => key > query || key == 1,
        }
    }
    fn time_component(self, tokens: usize, query: usize) -> f32 {
        match self {
            Self::CausalPrefix => query as f32 / 16384.,
            Self::Unmasked => (tokens - 1) as f32 / 16384.,
            Self::CausalWithMaskedKeyOne if query < 2 => 0.,
            Self::CausalWithMaskedKeyOne => {
                ((query * (query + 1) / 2 - 1) as f32 / query as f32) / 8192.
            }
        }
    }
}
#[cfg(any(test, feature = "test-support"))]
fn synthetic_control_bounds(
    model: ModelChoice,
    tokens: usize,
    graph_bytes: usize,
    dense: bool,
) -> Result<()> {
    ensure!(graph_bytes <= 65536, "bounded synthetic graph required");
    if dense {
        ensure!(
            model == ModelChoice::ContextQwenLarge && matches!(tokens, 17 | 128),
            "Large explicit-mask dense tiny controls require17/128 only"
        );
    } else {
        ensure!(
            matches!(tokens, 17 | 8192),
            "original synthetic GQA17/8192 control required"
        );
    }
    Ok(())
}
/// Separate bounded dense-kernel controls; historical flash controls remain distinct.
#[cfg(any(test, feature = "test-support"))]
pub fn synthetic_large_dense_gqa_profile(
    graph: &[u8],
    tokens: usize,
    profiling_prefix: &std::path::Path,
    optimized_model: &std::path::Path,
    mask: SyntheticDenseMask,
) -> Result<serde_json::Value> {
    synthetic_gqa_profile_with_bias(
        ModelChoice::ContextQwenLarge,
        graph,
        tokens,
        profiling_prefix,
        optimized_model,
        Some(mask),
    )
}
#[cfg(any(test, feature = "test-support"))]
fn synthetic_gqa_profile_with_bias(
    model: ModelChoice,
    graph: &[u8],
    tokens: usize,
    profiling_prefix: &std::path::Path,
    optimized_model: &std::path::Path,
    dense_mask: Option<SyntheticDenseMask>,
) -> Result<serde_json::Value> {
    let selected = selected_gqa_model(model)?;
    let kv_width = selected.kv_heads * selected.head_size;
    let query_group = selected.query_heads / selected.kv_heads;
    crate::native_inference::require_service_compute()?;
    synthetic_control_bounds(model, tokens, graph.len(), dense_mask.is_some())?;
    ensure!(
        airk_process::cpu_fit::usable_physical_cores()
            .context("usable physical-core observation unavailable")?
            >= selected.cpu_threads,
        "selected native CPU fit refused"
    );
    crate::onnx::initialize_runtime()?;
    let _environment = crate::onnx::acquire_session_environment()?;
    let mut session = Session::builder()?
        .with_intra_threads(selected.cpu_threads)?
        .with_inter_threads(1)?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_parallel_execution(false)?
        .with_memory_pattern(false)?
        .with_execution_providers([CPUExecutionProvider::default()
            .with_arena_allocator(false)
            .build()])?
        .with_config_entry("session.intra_op.allow_spinning", "0")?
        .with_config_entry("session.inter_op.allow_spinning", "0")?
        .with_profiling(profiling_prefix)?
        .with_optimized_model_path(optimized_model)?
        .commit_from_memory(graph)?;
    ensure!(
        session
            .inputs
            .iter()
            .map(|input| input.name.as_str())
            .collect::<Vec<_>>()
            == [
                "query",
                "key",
                "value",
                "bias",
                "seqlens_k",
                "total_sequence_length"
            ]
            && session
                .outputs
                .iter()
                .map(|output| output.name.as_str())
                .collect::<Vec<_>>()
                == ["context"],
        "synthetic control ABI differs; cache must remain private"
    );
    let q = vec![0f32; tokens * selected.query_width];
    let k = vec![0f32; tokens * kv_width];
    let mut v = vec![0f32; tokens * kv_width];
    for t in 0..tokens {
        for h in 0..selected.kv_heads {
            for d in 0..selected.head_size {
                v[t * kv_width + h * selected.head_size + d] =
                    (h * selected.head_size + d) as f32 / kv_width as f32 + t as f32 / 8192.;
            }
        }
    }
    if dense_mask == Some(SyntheticDenseMask::CausalWithMaskedKeyOne) {
        // If padding bias is ignored, this isolated masked value causes error well
        // above the unchanged0.0001 tolerance even at the largest toy width128.
        // Correct masking excludes it, leaving the independent expected mean unchanged.
        for value in &mut v[kv_width..2 * kv_width] {
            *value += SYNTHETIC_PADDING_VALUE_SPIKE;
        }
    }
    let mut bias = vec![0f32; tokens * tokens];
    if let Some(mask) = dense_mask {
        for query in 0..tokens {
            for key in 0..tokens {
                if mask.masks(query, key) {
                    bias[query * tokens + key] = f32::MIN;
                }
            }
        }
    } else {
        for t in 0..tokens {
            for future in t + 1..tokens {
                bias[t * tokens + future] = f32::MIN;
            }
        }
    }
    let options = RunOptions::new()?.with_outputs(OutputSelector::no_default().with("context"));
    let outputs = session.run_with_options(
        ort::inputs![
            "query"=>Tensor::from_array(([1,tokens,selected.query_width],q))?,
            "key"=>Tensor::from_array(([1,tokens,kv_width],k))?,
            "value"=>Tensor::from_array(([1,tokens,kv_width],v))?,
            "bias"=>Tensor::from_array(([1,1,tokens,tokens],bias))?,
            "seqlens_k"=>Tensor::from_array(([1],vec![tokens as i32-1]))?,
            "total_sequence_length"=>Tensor::from_array(([1],vec![tokens as i32]))?,
        ],
        &options,
    )?;
    let (shape, actual) = outputs["context"].try_extract_tensor::<f32>()?;
    ensure!(
        shape.as_ref() == [1, tokens as i64, selected.query_width as i64],
        "actual synthetic context shape differs"
    );
    let mut maximum_error = 0f32;
    for t in 0..tokens {
        for h in 0..selected.query_heads {
            for d in 0..selected.head_size {
                let expected = ((h / query_group) * selected.head_size + d) as f32
                    / kv_width as f32
                    + dense_mask.map_or(t as f32 / 16384., |mask| mask.time_component(tokens, t));
                let value = actual[t * selected.query_width + h * selected.head_size + d];
                ensure!(value.is_finite(), "nonfinite synthetic GQA value");
                maximum_error = maximum_error.max((value - expected).abs());
            }
        }
    }
    drop(outputs);
    let profiling = session.end_profiling()?;
    ensure!(
        maximum_error <= 0.0001,
        "synthetic causal/GQA value check refused"
    );
    let mut result = serde_json::json!({"tokens":tokens,"profiling_path":profiling,
        "optimized_graph_path":optimized_model,"maximum_error":maximum_error,
        "graph_outputs":["context"],"model_weights_loaded":false,
        "scope":"actual synthetic CPU GQA kernel only; no model/task qualification"});
    if let Some(mask) = dense_mask {
        result["attention_mode"] = serde_json::json!("explicit_mask_dense");
        if mask == SyntheticDenseMask::CausalWithMaskedKeyOne {
            result["masked_key_one_value_spike"] = serde_json::json!(SYNTHETIC_PADDING_VALUE_SPIKE);
        }
        result["mask_control"] = serde_json::json!(match mask {
            SyntheticDenseMask::CausalPrefix => "explicit_causal_prefix_mean",
            SyntheticDenseMask::Unmasked => "zero_bias_full_mean_refuses_implicit_causal",
            SyntheticDenseMask::CausalWithMaskedKeyOne =>
                "explicit_causal_and_key_one_padding_mean",
        });
    }
    Ok(result)
}

#[cfg(test)]
mod dense_synthetic_tests;

/// Exact embedded bytes, never a caller supplied catalog/descriptor digest.
pub fn catalog_source_digest() -> Sha256Digest {
    airk_core::identity::framed_digest(
        b"airk.native.arbiter-catalog-source/1",
        &[include_bytes!("release-catalog.json")],
    )
}
pub fn device_catalog() -> Result<airk_core::arbiter::device::DeviceCatalog> {
    use airk_core::arbiter::device::*;
    let profiles = catalog_profiles()?;
    let mut value = DeviceCatalog {
        schema: DEVICE_CATALOG_SCHEMA.into(),
        focus: Vec::new(),
        context_compact: Vec::new(),
        context_large: Vec::new(),
    };
    for p in &profiles {
        let row = VariantReference {
            manifest: p.manifest,
            execution_identity: p.device_execution_identity()?,
        };
        match SelectedModel::from_model(p.model)? {
            SelectedModel::Focus => value.focus.push(row),
            SelectedModel::ContextCompact => value.context_compact.push(row),
            SelectedModel::ContextLarge => value.context_large.push(row),
        }
    }
    value.validate(|sha| profiles.iter().find(|p| p.manifest == sha))?;
    Ok(value)
}
impl ExecutionProfile {
    pub fn device_evidence(&self) -> Option<airk_core::arbiter::device::DeviceEvidence> {
        #[cfg(feature = "gpu-cuda")]
        {
            self.gpu.as_ref().and_then(|b| b.evidence.clone())
        }
        #[cfg(not(feature = "gpu-cuda"))]
        {
            None
        }
    }
    pub fn host_warm_extra_bytes(&self) -> Option<u64> {
        #[cfg(feature = "gpu-cuda")]
        {
            self.gpu.as_ref().and_then(|b| b.host_warm_extra_bytes)
        }
        #[cfg(not(feature = "gpu-cuda"))]
        {
            None
        }
    }
    #[cfg(feature = "gpu-cuda")]
    pub fn cuda_device(&self) -> Option<Arc<cuda::CudaDevice>> {
        self.gpu.as_ref().map(|b| b.device.clone())
    }
    #[cfg(feature = "gpu-cuda")]
    pub fn bound_device_epoch(&self) -> Option<u64> {
        self.gpu.as_ref().map(|b| b.device.epoch())
    }
    #[cfg(feature = "gpu-cuda")]
    pub fn bound_device_uuid(&self) -> Option<[u8; 16]> {
        self.gpu.as_ref().map(|b| b.device.uuid())
    }
    #[cfg(feature = "gpu-cuda")]
    pub fn bind_cuda_device(
        mut self,
        device: Arc<cuda::CudaDevice>,
        deadline: std::time::Instant,
    ) -> Result<Self> {
        crate::native_inference::require_service_compute()?;
        ensure!(
            self.execution_authorized(),
            "GPU binding requires native release/evaluation capability"
        );
        self.gpu = Some(cuda::Binding::qualified(
            device,
            self.description(),
            deadline,
        )?);
        Ok(self)
    }
    #[cfg(all(feature = "gpu-cuda", any(test, feature = "test-support")))]
    pub fn bind_cuda_probe(
        mut self,
        device: Arc<cuda::CudaDevice>,
        plan: cuda::DeviceCapacityPlan,
        deadline: std::time::Instant,
    ) -> Result<Self> {
        crate::native_inference::require_service_compute()?;
        ensure!(
            self.execution_authorized() && self.evaluation() && !self.release,
            "unqualified GPU probe requires actual native evaluation lease"
        );
        self.gpu = Some(cuda::Binding::probe(
            device,
            self.description(),
            plan,
            deadline,
        )?);
        Ok(self)
    }
    #[cfg(feature = "gpu-cuda")]
    pub fn from_release_device(
        manifest: Sha256Digest,
        device: Arc<cuda::CudaDevice>,
        deadline: std::time::Instant,
    ) -> Result<Self> {
        crate::native_inference::require_service_compute()?;
        let description = catalog_profiles()?
            .into_iter()
            .find(|p| p.manifest == manifest)
            .context("GPU variant is outside compiled release catalog")?;
        description.validate()?;
        description.validate_release_downloads()?;
        validate_selected_policies(&description)?;
        ensure!(
            description.device_execution.is_some()
                && description.qualification.is_some()
                && description
                    .qualified_targets
                    .iter()
                    .any(|row| row.target == current_target()),
            "GPU variant has no native release qualification"
        );
        Self {
            description,
            release: true,
            evaluation: None,
            source_transition: None,
            gpu: None,
        }
        .bind_cuda_device(device, deadline)
    }
}
impl Scorer {
    pub fn device_settled(&self) -> bool {
        self.session
            .lock()
            .map(|s| s.device_settled())
            .unwrap_or(false)
    }
    pub fn synchronize(&self, deadline: std::time::Instant) -> Result<()> {
        self.session
            .lock()
            .map_err(|_| anyhow::anyhow!("native session owner poisoned"))?
            .synchronize(deadline)
    }
    #[cfg(feature = "gpu-cuda")]
    pub fn device_observation(
        &self,
        deadline: std::time::Instant,
    ) -> Result<Option<cuda::AdapterObservation>> {
        self.profile
            .gpu
            .as_ref()
            .map(|b| b.device.observation(deadline))
            .transpose()
    }
}

/// Asset-management description only. This creates no execution capability.
pub fn release_asset_profile(manifest: Sha256Digest) -> Result<Profile> {
    let profile = catalog_profiles()?
        .into_iter()
        .find(|p| p.manifest == manifest)
        .context("profile is outside compiled release catalog")?;
    profile.validate()?;
    profile.validate_release_downloads()?;
    admit_source_transition(&profile)?;
    if profile.device_execution.is_some() {
        ensure!(
            profile.qualification.is_some()
                && profile
                    .qualified_targets
                    .iter()
                    .any(|v| v.target == current_target()),
            "GPU assets have no native release evidence"
        );
    } else {
        release_qualification_gate(&profile, env!("CARGO_PKG_VERSION"))?;
    }
    Ok(profile)
}

#[cfg(feature = "gpu-cuda")]
impl Scorer {
    pub fn device_placement(&self) -> Result<Option<cuda::PlacementObservation>> {
        crate::native_inference::require_service_compute()?;
        let session = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("native session owner poisoned"))?;
        self.profile
            .gpu
            .as_ref()
            .map(|b| b.device.placement(&session))
            .transpose()
    }
}

/// Only private typed native execution faults qualify. Input, source, deadline,
/// authority and cancellation strings are never a device-failure heuristic.
pub fn is_native_device_failure(error: &anyhow::Error) -> bool {
    #[cfg(feature = "gpu-cuda")]
    {
        error.chain().any(|v| v.is::<cuda::NativeDeviceFailure>())
    }
    #[cfg(not(feature = "gpu-cuda"))]
    {
        let _ = error;
        false
    }
}

#[cfg(feature = "test-support")]
impl ExecutionProfile {
    pub fn matches_evaluation_lease(&self, lease: &Arc<dyn Send + Sync>) -> bool {
        self.evaluation
            .as_ref()
            .is_some_and(|actual| Arc::ptr_eq(actual, lease))
    }
}
