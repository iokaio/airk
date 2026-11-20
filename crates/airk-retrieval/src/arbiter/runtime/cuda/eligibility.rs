// SPDX-License-Identifier: Apache-2.0
//! Live per-resident admission. Never G4, p95, semantic or a persisted permit.
use super::super::{AssetSource, Scorer};
use super::*;
use crate::arbiter::compiler::CompiledInput;
use airk_core::{
    arbiter::{
        AssetRole, ModelChoice, Task, context_action::calibration::JointAssetBinding,
        resource_policy::ResourcePolicy,
    },
    identity::Incarnation,
};
use std::{marker::PhantomData, rc::Rc, thread::ThreadId};

/// Native actor identity data only, not a capability. No Deserialize.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EligibilityKey {
    configuration: Sha256Digest,
    source: Sha256Digest,
    joint_residency: Sha256Digest,
    nonce: Incarnation,
}
impl EligibilityKey {
    pub fn new(
        configuration: Sha256Digest,
        source: Sha256Digest,
        joint_residency: Sha256Digest,
        nonce: Incarnation,
    ) -> Result<Self> {
        ensure!(
            [configuration, source, joint_residency]
                .iter()
                .all(|v| *v != Sha256Digest::from_bytes([0; 32]))
                && nonce != Incarnation::from_bytes([0; 16]),
            "native eligibility key is unavailable"
        );
        Ok(Self {
            configuration,
            source,
            joint_residency,
            nonce,
        })
    }
}
/// Constructed only after actual native session validation; no public fields,
/// Deserialize, source-JSON approval flag or persisted reconstruction.
pub struct DeviceEligibility {
    key: EligibilityKey,
    device: Weak<CudaDevice>,
    epoch: u64,
    uuid: [u8; 16],
    driver_api: i32,
    driver_image: Sha256Digest,
    runtime_manifest: Sha256Digest,
    profile: Sha256Digest,
    fixture: Sha256Digest,
    valid: AtomicBool,
    session_alive: Weak<AtomicBool>,
    state: Weak<Mutex<AdmissionState>>,
}
impl DeviceEligibility {
    pub fn current(&self, key: &EligibilityKey) -> bool {
        if !self.valid.load(Ordering::Acquire)
            || key != &self.key
            || !self
                .session_alive
                .upgrade()
                .is_some_and(|v| v.load(Ordering::Acquire))
            || !self.state.upgrade().is_some_and(|v| !v.is_poisoned())
        {
            return false;
        }
        self.device.upgrade().is_some_and(|d| {
            !d.poisoned()
                && d.epoch() == self.epoch
                && d.uuid() == self.uuid
                && d.driver_api_version() == self.driver_api
                && d.driver_image_sha256() == self.driver_image
                && d.runtime.manifest_identity() == self.runtime_manifest
        })
    }
    /// Diagnostic only: the digest is not an execution or persisted permit.
    pub fn observation(&self) -> serde_json::Value {
        serde_json::json!({"schema":"airk.native.cuda.device-eligibility/1","scope":"actual bounded host admission only; not G4/p95/peak/semantic qualification","profile":self.profile,"fixture":self.fixture,"runtime_manifest":self.runtime_manifest,"adapter_uuid":self.uuid,"driver_api":self.driver_api,"driver_image_sha256":self.driver_image,"dispatch_epoch":self.epoch})
    }
}
pub(super) enum AdmissionState {
    Scientific,
    Pending,
    Validating {
        thread: ThreadId,
        input: Option<Sha256Digest>,
    },
    Eligible(Arc<DeviceEligibility>),
}
pub(super) fn initial_admission(product: bool) -> AdmissionState {
    if product {
        AdmissionState::Pending
    } else {
        AdmissionState::Scientific
    }
}
impl AdmissionState {
    fn revoke(&mut self) {
        if let Self::Eligible(w) = self {
            w.valid.store(false, Ordering::Release);
        }
        if !matches!(self, Self::Scientific) {
            *self = Self::Pending;
        }
    }
    pub(super) fn authorize(&self, digest: Sha256Digest) -> Result<()> {
        match self {
            Self::Scientific => Ok(()),
            Self::Validating { thread, input } => {
                ensure!(
                    *thread == std::thread::current().id() && *input == Some(digest),
                    "only the current canonical management row may score before eligibility"
                );
                Ok(())
            }
            Self::Eligible(w) => {
                ensure!(w.current(&w.key), "native device eligibility is stale");
                Ok(())
            }
            Self::Pending => {
                anyhow::bail!("native product device eligibility required before scoring")
            }
        }
    }
}
impl Binding {
    pub(super) fn authorize_score(&self, digest: Sha256Digest) -> Result<()> {
        self.admission
            .lock()
            .map_err(|_| anyhow::anyhow!("eligibility state poisoned"))?
            .authorize(digest)
    }
    pub(in crate::arbiter::runtime) fn revoke_eligibility(&self) {
        let mut state = self
            .admission
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.revoke();
    }
    pub(in crate::arbiter::runtime) fn end_session_eligibility(&self) {
        self.session_alive.store(false, Ordering::Release);
        self.revoke_eligibility();
    }
}
struct Validation<'a> {
    binding: &'a Binding,
    committed: bool,
    _local: PhantomData<Rc<()>>,
}
impl<'a> Validation<'a> {
    fn begin(binding: &'a Binding) -> Result<Self> {
        let mut state = binding
            .admission
            .lock()
            .map_err(|_| anyhow::anyhow!("eligibility state poisoned"))?;
        ensure!(
            !matches!(
                *state,
                AdmissionState::Scientific | AdmissionState::Validating { .. }
            ),
            "resident is scientific or already validating"
        );
        state.revoke();
        *state = AdmissionState::Validating {
            thread: std::thread::current().id(),
            input: None,
        };
        Ok(Self {
            binding,
            committed: false,
            _local: PhantomData,
        })
    }
    fn allow(&self, digest: Sha256Digest) -> Result<()> {
        let mut state = self
            .binding
            .admission
            .lock()
            .map_err(|_| anyhow::anyhow!("eligibility state poisoned"))?;
        let AdmissionState::Validating { thread, input } = &mut *state else {
            anyhow::bail!("native eligibility management was invalidated")
        };
        ensure!(
            *thread == std::thread::current().id(),
            "wrong eligibility owner thread"
        );
        *input = Some(digest);
        Ok(())
    }
    fn finish(mut self, witness: Arc<DeviceEligibility>) -> Result<()> {
        let mut state = self
            .binding
            .admission
            .lock()
            .map_err(|_| anyhow::anyhow!("eligibility state poisoned"))?;
        ensure!(
            matches!(&*state,AdmissionState::Validating{thread,..} if *thread==std::thread::current().id()),
            "eligibility invalidated before Ready"
        );
        *state = AdmissionState::Eligible(witness);
        self.committed = true;
        Ok(())
    }
}
impl Drop for Validation<'_> {
    fn drop(&mut self) {
        if !self.committed {
            self.binding.revoke_eligibility();
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    name: String,
    input: CompiledInput,
    reference_same_artifact: Vec<f32>,
    reference_original_fp32: Vec<f32>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    schema: String,
    model: ModelChoice,
    compiler: Sha256Digest,
    task_policy: Sha256Digest,
    trained_assets: BTreeMap<String, JointAssetBinding>,
    resource_policy: Sha256Digest,
    independent_reference_source: Sha256Digest,
    rows: Vec<Row>,
}
impl Fixture {
    fn validate(&self, profile: &Profile) -> Result<()> {
        let focus = profile.model == ModelChoice::FocusQwenReranker06B;
        ensure!(
            matches!(
                profile.model,
                ModelChoice::FocusQwenReranker06B
                    | ModelChoice::ContextQwenCompact
                    | ModelChoice::ContextQwenLarge
            ) && profile.max_tokens == if focus { 512 } else { 8192 },
            "eligibility profile maximum shape is not the selected canonical boundary"
        );
        let task = if focus {
            Task::FocusRank
        } else {
            Task::ContextAction
        };
        ensure!(
            self.schema == "airk.arbiter.device-acceptance-input/1"
                && self.model == profile.model
                && self.compiler == profile.compiler
                && profile.task_policies.get(&task) == Some(&self.task_policy)
                && self.resource_policy == ResourcePolicy::software_v2().digest()?
                && self.independent_reference_source != Sha256Digest::from_bytes([0; 32]),
            "eligibility fixture identity/source mismatch"
        );
        let assets = profile
            .assets
            .iter()
            .filter(|a| {
                matches!(
                    a.role,
                    AssetRole::Model | AssetRole::ExternalWeights | AssetRole::Tokenizer
                )
            })
            .map(|a| {
                (
                    a.name.clone(),
                    JointAssetBinding {
                        role: a.role.clone(),
                        sha256: a.sha256,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        ensure!(
            assets == self.trained_assets,
            "eligibility fixture trained asset closure mismatch"
        );
        ensure!(
            self.rows.len() == if focus { 12 } else { 3 },
            "complete canonical eligibility roster required"
        );
        let mut names = BTreeSet::new();
        for (index, row) in self.rows.iter().enumerate() {
            let (tokens, options) = if focus {
                (512, 2)
            } else {
                [(17, 3), (128, 8), (8192, 8)][index]
            };
            let input = &row.input;
            ensure!(
                !row.name.is_empty()
                    && row.name.len() <= 128
                    && names.insert(&row.name)
                    && input.input_ids.len() == tokens
                    && input.input_ids.iter().all(|v| *v >= 0)
                    && input.attention_mask.len() == tokens
                    && input.position_ids.iter().copied().eq(0..tokens as i64)
                    && input.decision_position == tokens as i64 - 1
                    && input.candidate_positions.len() == options
                    && input.candidate_positions.iter().all(|v| *v >= 0)
                    && input.option_mask == vec![true; options]
                    && input.prompt.len() <= 65536
                    && input.input_digest != Sha256Digest::from_bytes([0; 32]),
                "canonical eligibility tensor roster/shape changed"
            );
            ensure!(
                if focus {
                    input.attention_mask.iter().all(|v| matches!(v, 0 | 1))
                        && input.attention_mask.last() == Some(&1)
                        && input.attention_mask.windows(2).all(|p| p[0] <= p[1])
                } else {
                    input.attention_mask.iter().all(|v| *v == 1)
                },
                "canonical eligibility padding/visibility changed"
            );
            for scores in [&row.reference_same_artifact, &row.reference_original_fp32] {
                ensure!(
                    scores.len() == options && scores.iter().all(|v| v.is_finite()),
                    "complete finite independent reference required"
                );
            }
        }
        Ok(())
    }
}
#[derive(Debug)]
struct NumericMismatch {
    name: String,
    same_raw: f64,
    same_mass: f64,
    original_raw: f64,
    original_mass: f64,
    argmax: bool,
    actual: Vec<f32>,
}
impl std::fmt::Display for NumericMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "actual native synthetic numerical eligibility mismatch: case={} same_raw={} same_mass={} original_raw={} original_mass={} argmax={} actual={:?}",
            self.name,
            self.same_raw,
            self.same_mass,
            self.original_raw,
            self.original_mass,
            self.argmax,
            self.actual
        )
    }
}
impl std::error::Error for NumericMismatch {}
fn check_scores(actual: &[f32], row: &Row, focus: bool) -> Result<()> {
    let (a, m, w) = compare_reference_scores(actual, &row.reference_same_artifact)?;
    let (b, n, v) = compare_reference_scores(actual, &row.reference_original_fp32)?;
    if a > 0.001
        || m > if focus { 0.00001 } else { 0.05 }
        || b > if focus { 0.25 } else { 0.001 }
        || n > 0.05
        || !w
        || !v
    {
        return Err(NumericMismatch {
            name: row.name.clone(),
            same_raw: a,
            same_mass: m,
            original_raw: b,
            original_mass: n,
            argmax: w && v,
            actual: actual.to_vec(),
        }
        .into());
    }
    Ok(())
}
/// Only genuine observed compatibility failures; malformed fixture/source,
/// authority and deadline errors are never classified as provider faults.
pub fn is_device_eligibility_failure(error: &anyhow::Error) -> bool {
    super::super::is_native_device_failure(error)
        || error.chain().any(|v| {
            v.is::<NumericMismatch>()
                || v.downcast_ref::<PlacementRefusal>().is_some_and(|p| {
                    p.observation.complete
                        && p.observation.shim_status == 0
                        && p.observation.ort_error_code == 0
                        && !p.observation.nodes.is_empty()
                        && p.observation
                            .nodes
                            .iter()
                            .any(|n| n.ep != "CUDAExecutionProvider")
                })
        })
}
impl Scorer {
    /// Actual authenticated incremental host requirement; not resident peak again.
    pub fn host_warm_extra_bytes(&self) -> Option<u64> {
        self.profile.host_warm_extra_bytes()
    }
    pub fn requires_device_eligibility(&self) -> bool {
        self.profile.gpu.as_ref().is_some_and(|b| {
            b.admission
                .lock()
                .map(|state| !matches!(*state, AdmissionState::Scientific))
                .unwrap_or(true)
        })
    }
    pub fn invalidate_device_eligibility(&self) {
        if let Some(b) = &self.profile.gpu {
            b.revoke_eligibility();
        }
    }
    pub fn device_eligibility_current(&self, key: &EligibilityKey) -> bool {
        self.profile.gpu.as_ref().is_some_and(|b| {
            b.admission
                .lock()
                .map(|state| matches!(&*state,AdmissionState::Eligible(w) if w.current(key)))
                .unwrap_or(false)
        })
    }
    pub fn establish_device_eligibility(
        &self,
        source: &dyn AssetSource,
        key: EligibilityKey,
        deadline: Instant,
        liveness: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<Arc<DeviceEligibility>> {
        crate::native_inference::require_service_compute()?;
        let binding = self
            .profile
            .gpu
            .as_ref()
            .context("CPU resident has no device eligibility")?;
        binding.revoke_eligibility();
        in_time(deadline)?;
        ensure!(
            deadline.saturating_duration_since(Instant::now())
                <= std::time::Duration::from_millis(120000),
            "management eligibility budget cannot exceed original120s"
        );
        ensure!(liveness(), "native eligibility authority changed");
        ensure!(
            self.profile.release_qualified(),
            "product eligibility requires compiled qualified artifact"
        );
        let row = binding
            .device
            .runtime
            .manifest
            .requirement(self.profile.description())?;
        binding
            .device
            .validate_profile(self.profile.description())?;
        let validation = Validation::begin(binding)?;
        let raw = super::super::bytes(source, &row.acceptance_fixture, 16 << 20)?;
        let fixture: Fixture = serde_json::from_slice(&raw)?;
        fixture.validate(self.profile.description())?;
        for row in &fixture.rows {
            self.compiler.validate_qwen_readout(&row.input)?;
        }
        in_time(deadline)?;
        ensure!(liveness(), "native eligibility authority changed");
        {
            let session = self
                .session
                .lock()
                .map_err(|_| anyhow::anyhow!("native session owner poisoned"))?;
            binding.device.verify_session_placement(
                self.profile.description(),
                &session,
                deadline,
                self.fp16_attention.as_ref(),
            )?;
        }
        let capacity = binding.device.reserve_warm_work(
            binding.plan.warm_extra_bytes(),
            binding.plan.headroom_bytes(),
            deadline,
        )?;
        capacity.settle()?;
        let epoch = binding.device.epoch();
        let focus = self.profile.description().model == ModelChoice::FocusQwenReranker06B;
        for row in &fixture.rows {
            in_time(deadline)?;
            ensure!(liveness(), "native eligibility authority changed");
            validation.allow(row.input.input_digest)?;
            let actual = self.score_with_deadline(&row.input, deadline)?;
            check_scores(&actual, row, focus)?;
            in_time(deadline)?;
            ensure!(liveness(), "native eligibility authority changed");
        }
        self.synchronize(deadline)?;
        ensure!(self.device_settled(), "native eligibility work unsettled");
        let final_capacity = binding.device.reserve_warm_work(
            binding.plan.warm_extra_bytes(),
            binding.plan.headroom_bytes(),
            deadline,
        )?;
        final_capacity.settle()?;
        in_time(deadline)?;
        ensure!(
            liveness() && binding.device.epoch() == epoch && !binding.device.poisoned(),
            "native eligibility identity/authority changed before Ready"
        );
        let device = &binding.device;
        let witness = Arc::new(DeviceEligibility {
            key,
            device: Arc::downgrade(device),
            epoch,
            uuid: device.uuid(),
            driver_api: device.driver_api_version(),
            driver_image: device.driver_image_sha256(),
            runtime_manifest: device.runtime.manifest_identity(),
            profile: self.profile.description().manifest,
            fixture: row.acceptance_fixture.sha256,
            valid: AtomicBool::new(true),
            session_alive: Arc::downgrade(&binding.session_alive),
            state: Arc::downgrade(&binding.admission),
        });
        validation.finish(witness.clone())?;
        Ok(witness)
    }
}
#[cfg(test)]
mod tests;
