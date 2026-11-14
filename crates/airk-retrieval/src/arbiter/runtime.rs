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
        ensure!(
            description
                .qualified_targets
                .iter()
                .any(|row| row.target == current_target()),
            "Arbiter profile has no qualification on this native target"
        );
        if description.tasks.contains(&Task::ContextNextRead) {
            ensure!(
                description.task_policies.get(&Task::ContextNextRead)
                    == Some(&super::tasks::policy_digest()?),
                "Arbiter next-read task policy differs from qualified profile"
            );
        }
        ensure!(
            description.qualification.is_some() && description.cpu_fit.is_some(),
            "Arbiter release qualification missing"
        );
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
        crate::native_inference::require_service_compute()?;
        profile.description.validate()?;
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
        let expected_recipe = match profile.description.model {
            ModelChoice::SmallIntern | ModelChoice::LargeIntern => {
                CompilerRecipe::InternSingleChoiceV1
            }
            ModelChoice::SmallStrands => CompilerRecipe::StrandsSingleChoiceV1,
            ModelChoice::LargeKev => CompilerRecipe::KevSingleChoiceV1,
        };
        ensure!(
            compiler.recipe() == expected_recipe,
            "Arbiter model/compiler recipe mismatch"
        );
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
        let expected = BTreeSet::from([
            "input_ids",
            "attention_mask",
            "position_ids",
            "decision_positions",
            "candidate_positions",
            "option_mask",
        ]);
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
                && profile.description.abi.output.name == "scores",
            "Arbiter declared ABI differs from supported prefill recipe"
        );
        ensure!(
            session.outputs.iter().any(|output| output.name == "scores"),
            "Arbiter native scores missing"
        );
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
        }
        let output = session
            .outputs
            .iter()
            .find(|output| output.name == "scores")
            .unwrap();
        ensure!(
            matches!(&output.output_type,ValueType::Tensor {ty,shape,..}
            if *ty==ort::tensor::TensorElementType::Float32 && shape.len()==2),
            "Arbiter native output ABI differs"
        );
        Ok(Self {
            profile,
            compiler,
            session: Mutex::new(session),
            _environment: environment,
            physical_cores,
        })
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
        ensure!(
            tokens > 0
                && tokens <= self.profile.description.max_tokens
                && candidates > 0
                && candidates <= self.profile.description.max_candidates.min(8)
                && input.input_ids.iter().all(|id| *id >= 0)
                && input.attention_mask == vec![1; tokens]
                && input.position_ids.iter().copied().eq(0..tokens as i64)
                && input.option_mask == vec![true; candidates]
                && (0..tokens as i64).contains(&input.decision_position),
            "Arbiter compiled input differs from qualified B1 unpadded profile"
        );
        if self.profile.description.abi.readout == Readout::PointerHead {
            ensure!(
                input
                    .candidate_positions
                    .iter()
                    .all(|position| (0..tokens as i64).contains(position)),
                "Arbiter pointer position is outside verified input"
            );
        }
        let mut session = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("Arbiter native session poisoned"))?;
        let options = RunOptions::new()?.with_outputs(OutputSelector::no_default().with("scores"));
        let outputs = session.run_with_options(ort::inputs![
            "input_ids" => Tensor::from_array(([1,tokens],input.input_ids.clone()))?,
            "attention_mask" => Tensor::from_array(([1,tokens],input.attention_mask.clone()))?,
            "position_ids" => Tensor::from_array(([1,tokens],input.position_ids.clone()))?,
            "decision_positions" => Tensor::from_array(([1],vec![input.decision_position]))?,
            "candidate_positions" => Tensor::from_array(([1,candidates],input.candidate_positions.clone()))?,
            "option_mask" => Tensor::from_array(([1,candidates],input.option_mask.clone()))?,
        ],&options).context("Arbiter full trained readout execution failed")?;
        let (shape, scores) = outputs["scores"].try_extract_tensor::<f32>()?;
        ensure!(
            shape.as_ref() == [1, candidates as i64]
                && scores.iter().all(|score| score.is_finite()),
            "Arbiter score shape or finite-value contract changed"
        );
        Ok(scores.to_vec())
    }
}
