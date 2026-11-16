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

#[cfg(test)]
mod embedded_catalog_tests;
mod input;
#[cfg(all(test, feature = "test-support"))]
mod local_test_catalog_emitter;
#[cfg(test)]
mod local_test_tests;
#[cfg(feature = "test-support")]
pub mod qualification;

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
impl ExecutionProfile {
    pub fn from_release(manifest: Sha256Digest) -> Result<Self> {
        let description = catalog_profiles()?
            .into_iter()
            .find(|profile| profile.manifest == manifest)
            .context("Arbiter profile is not release-qualified")?;
        description.validate()?;
        description.validate_release_downloads()?;
        validate_selected_policies(&description)?;
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
                description.task_policies.get(&Task::FocusRank)
                    == Some(&super::compiler::focus::policy_digest()?),
                "Arbiter Focus source policy differs from qualified profile"
            );
        }
        Ok(Self {
            description,
            release: true,
            evaluation: None,
        })
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_qualification(description: Profile) -> Result<Self> {
        description.validate()?;
        Ok(Self {
            description,
            release: false,
            evaluation: None,
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
            validate_selected_policies(profile)?;
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
                description.task_policies.get(&Task::FocusRank)
                    == Some(&super::compiler::focus::policy_digest()?),
                "evaluation Focus source policy differs from native constructor"
            );
        }
        Ok(ExecutionProfile {
            description,
            release: false,
            evaluation: Some(self.lease.clone()),
        })
    }
}

pub struct Scorer {
    profile: ExecutionProfile,
    compiler: Compiler,
    session: Mutex<Session>,
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
        crate::native_inference::require_service_compute()?;
        profile.description.validate()?;
        validate_selected_policies(profile.description())?;
        ensure!(
            profile.description.runtime == crate::model::ONNX_RUNTIME_ENGINE,
            "Arbiter runtime differs from native image"
        );
        let fit = profile.description.cpu_fit.clone().unwrap_or_default();
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
        let mut builder = Session::builder()?
            .with_intra_threads(fit.intra_threads)?
            .with_inter_threads(fit.inter_threads)?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_parallel_execution(false)?
            .with_memory_pattern(false)?
            .with_execution_providers([CPUExecutionProvider::default()
                .with_arena_allocator(false)
                .build()])?
            .with_config_entry("session.intra_op.allow_spinning", "0")?
            .with_config_entry("session.inter_op.allow_spinning", "0")?;
        builder = configure(builder)?;
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
        let session = builder
            .commit_from_memory(&graph)
            .context("Arbiter native model load failed")?;
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
        Ok(Self {
            profile,
            compiler,
            session: Mutex::new(session),
            _environment: environment,
            physical_cores,
            joint_calibration,
        })
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
        self.compiler.compile(state, question, candidates)
    }
    pub fn score(&self, input: &CompiledInput) -> Result<Vec<f32>> {
        crate::native_inference::require_service_compute()?;
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
        Ok(scores.to_vec())
    }
}

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
    let selected = selected_gqa_model(model)?;
    let kv_width = selected.kv_heads * selected.head_size;
    let query_group = selected.query_heads / selected.kv_heads;
    crate::native_inference::require_service_compute()?;
    ensure!(
        matches!(tokens, 17 | 8192) && graph.len() <= 65536,
        "bounded synthetic GQA control required"
    );
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
    let mut bias = vec![0f32; tokens * tokens];
    for t in 0..tokens {
        for future in t + 1..tokens {
            bias[t * tokens + future] = f32::MIN;
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
                    + t as f32 / 16384.;
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
    Ok(
        serde_json::json!({"tokens":tokens,"profiling_path":profiling,
        "optimized_graph_path":optimized_model,"maximum_error":maximum_error,
        "graph_outputs":["context"],"model_weights_loaded":false,
        "scope":"actual synthetic CPU GQA kernel only; no model/task qualification"}),
    )
}
