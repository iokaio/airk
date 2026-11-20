// SPDX-License-Identifier: Apache-2.0
//! Bounded observations only; never a session, scorer or execution capability.
use super::*;
#[derive(Clone, Debug, Serialize)]
pub struct LogRow {
    pub location: String,
    pub message: String,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct LogSnapshot {
    pub seen: u64,
    pub selected: u64,
    pub retained: u64,
    pub omitted: u64,
    pub bytes: usize,
    pub truncated: bool,
    pub rows: Vec<LogRow>,
}
#[cfg(feature = "test-support")]
impl LogSnapshot {
    fn validate_disclosure(&self) -> Result<()> {
        ensure!(
            self.retained == self.rows.len() as u64
                && self.selected.checked_sub(self.retained) == Some(self.omitted)
                && self.seen >= self.selected
                && (self.omitted == 0 || self.truncated)
                && self.bytes <= 65536
                && self
                    .rows
                    .iter()
                    .all(|r| r.location.len() + 2 + r.message.len() <= 4096)
                && self.bytes
                    == self
                        .rows
                        .iter()
                        .map(|r| r.location.len() + 2 + r.message.len())
                        .sum::<usize>(),
            "diagnostic selected-log disclosure is inconsistent"
        );
        Ok(())
    }
}
#[derive(Default)]
pub(crate) struct Capture {
    logs: LogSnapshot,
    #[cfg(feature = "test-support")]
    state: LoadSnapshot,
}
impl Capture {
    pub(super) fn push(&mut self, location: &str, message: &str) {
        self.logs.seen = self.logs.seen.saturating_add(1);
        if self.logs.seen == u64::MAX {
            self.logs.truncated = true;
        }
        if !selected(message) {
            return;
        }
        self.logs.selected = self.logs.selected.saturating_add(1);
        let bytes = location
            .len()
            .saturating_add(2)
            .saturating_add(message.len());
        if bytes > 4096
            || self.logs.bytes.saturating_add(bytes) > 65536
            || self.logs.rows.len() >= 65536
        {
            self.logs.omitted = self.logs.omitted.saturating_add(1);
            self.logs.truncated = true;
        } else {
            self.logs.bytes += bytes;
            self.logs.rows.push(LogRow {
                location: location.into(),
                message: message.into(),
            });
            self.logs.retained += 1;
        }
        if self.logs.seen == u64::MAX || self.logs.selected == u64::MAX {
            self.logs.truncated = true;
        }
    }
}
fn selected(message: &str) -> bool {
    [
        "ORT optimization- Force fallback to CPU execution for node:",
        "CUDA kernel not found in registries for Op type:",
        "CUDA kernel not supported. Fallback to CPU execution provider for Op type:",
        "This session contains graph nodes that are assigned to the default CPU EP",
        "Could not find an implementation for",
        "CUDA_EP Halting assignment due to capacity threshold",
    ]
    .iter()
    .any(|prefix| message.starts_with(prefix))
}
#[cfg(feature = "test-support")]
#[derive(Clone, Debug, Default, Serialize)]
pub struct Teardown {
    pub pre_sync_observed: bool,
    pub session_drop_attempted: bool,
    pub session_drop_returned: bool,
    pub post_sync_observed: bool,
    pub pending_claims: Option<usize>,
    pub poisoned: bool,
    pub errors: Vec<String>,
}
#[cfg(feature = "test-support")]
impl Teardown {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.pre_sync_observed
                && self.session_drop_attempted
                && self.session_drop_returned
                && self.post_sync_observed
                && self.pending_claims == Some(0)
                && !self.poisoned
                && self.errors.is_empty(),
            "disposable CUDA session teardown is incomplete or uncertain"
        );
        Ok(())
    }
}
#[cfg(feature = "test-support")]
pub(crate) fn settle_with<T>(
    session: T,
    pre: impl FnOnce(&T) -> Result<()>,
    unknown_release: impl FnOnce(),
    post: impl FnOnce() -> (Result<()>, Option<usize>, bool),
) -> Teardown {
    let before = pre(&session);
    let dropped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(session)));
    if dropped.is_err() {
        // Unknown native release must alter actual custody before later observations.
        unknown_release();
    }
    let (after, pending_claims, poisoned) = post();
    let mut errors = Vec::new();
    if let Err(error) = &before {
        errors.push(format!("pre-sync: {error:#}"));
    }
    if dropped.is_err() {
        errors.push("session Drop panicked; release is unknown".into());
    }
    if let Err(error) = &after {
        errors.push(format!("post-sync: {error:#}"));
    }
    Teardown {
        pre_sync_observed: before.is_ok(),
        session_drop_attempted: true,
        session_drop_returned: dropped.is_ok(),
        post_sync_observed: after.is_ok(),
        pending_claims,
        poisoned,
        errors,
    }
}
#[cfg(feature = "test-support")]
fn capture_then_settle<T>(
    session: T,
    capture: impl FnOnce() -> Result<()>,
    pre: impl FnOnce(&T) -> Result<()>,
    unknown_release: impl FnOnce(),
    post: impl FnOnce() -> (Result<()>, Option<usize>, bool),
) -> (Result<()>, Teardown) {
    let captured = std::panic::catch_unwind(std::panic::AssertUnwindSafe(capture))
        .unwrap_or_else(|_| Err(anyhow::anyhow!("diagnostic observation capture panicked")));
    let teardown = settle_with(session, pre, unknown_release, post);
    (captured, teardown)
}
#[cfg(feature = "test-support")]
#[derive(Clone, Debug, Default, Serialize)]
pub struct LoadSnapshot {
    pub session_created: bool,
    pub placement: Option<PlacementObservation>,
    pub graph_metadata: Option<super::GraphMetadataObservation>,
    pub logs: LogSnapshot,
    pub teardown: Option<Teardown>,
}
#[cfg(feature = "test-support")]
impl LoadSnapshot {
    pub fn validate_graph_metadata(&self) -> Result<()> {
        self.logs.validate_disclosure()?;
        ensure!(self.session_created, "diagnostic session was not created");
        let placement = self
            .placement
            .as_ref()
            .context("diagnostic placement missing")?;
        complete_placement(placement)?;
        self.graph_metadata
            .as_ref()
            .context("diagnostic graph metadata missing")?
            .validate_against(placement)?;
        self.teardown
            .as_ref()
            .context("diagnostic teardown missing")?
            .validate()?;
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(self.session_created, "diagnostic session was not created");
        complete_placement(
            self.placement
                .as_ref()
                .context("diagnostic placement missing")?,
        )?;
        self.teardown
            .as_ref()
            .context("diagnostic teardown missing")?
            .validate()?;
        let logs = &self.logs;
        ensure!(
            !logs.truncated
                && logs.omitted == 0
                && logs.retained == logs.rows.len() as u64
                && logs.selected == logs.retained
                && logs.seen >= logs.selected
                && logs.bytes <= 65536
                && logs
                    .rows
                    .iter()
                    .all(|r| r.location.len() + 2 + r.message.len() <= 4096)
                && logs.bytes
                    == logs
                        .rows
                        .iter()
                        .map(|r| r.location.len() + 2 + r.message.len())
                        .sum::<usize>(),
            "diagnostic selected logs are incomplete or inconsistent"
        );
        Ok(())
    }
}
#[cfg(feature = "test-support")]
#[derive(Clone)]
pub struct LoadObserver(Arc<Mutex<Capture>>);
#[cfg(feature = "test-support")]
impl LoadObserver {
    pub fn snapshot(&self) -> Result<LoadSnapshot> {
        let capture = self
            .0
            .lock()
            .map_err(|_| anyhow::anyhow!("CUDA diagnostic observation poisoned"))?;
        let mut state = capture.state.clone();
        state.logs = capture.logs.clone();
        Ok(state)
    }
}
#[cfg(feature = "test-support")]
impl super::super::ExecutionProfile {
    pub fn cuda_load_observer(&self) -> Result<LoadObserver> {
        require_diagnostic_profile(self)?;
        self.diagnostic_observer()
    }

    pub fn cuda_graph_observer(&self) -> Result<LoadObserver> {
        require_graph_diagnostic_profile(self)?;
        self.diagnostic_observer()
    }

    #[cfg(all(windows, target_arch = "x86_64"))]
    pub fn cuda_compact_fp16_load_observer(&self) -> Result<LoadObserver> {
        require_compact_fp16_profile(self)?;
        self.diagnostic_observer()
    }

    fn diagnostic_observer(&self) -> Result<LoadObserver> {
        Ok(LoadObserver(
            self.gpu
                .as_ref()
                .context("CUDA diagnostic binding missing")?
                .log
                .clone(),
        ))
    }
}
#[cfg(feature = "test-support")]
pub(crate) fn authority(
    evaluation: bool,
    release: bool,
    unqualified: bool,
    allowed_model: bool,
    matching_device: bool,
    binding: bool,
) -> Result<()> {
    ensure!(
        evaluation && !release && unqualified && allowed_model && matching_device && binding,
        "CUDA diagnostic requires an approved model and matching unqualified native evaluation binding"
    );
    Ok(())
}
#[cfg(feature = "test-support")]
pub(crate) fn require_diagnostic_profile(profile: &super::super::ExecutionProfile) -> Result<()> {
    require_profile(
        profile,
        profile.description.model == airk_core::arbiter::ModelChoice::ContextQwenCompact,
    )
}

#[cfg(feature = "test-support")]
pub(crate) fn require_graph_diagnostic_profile(
    profile: &super::super::ExecutionProfile,
) -> Result<()> {
    use airk_core::arbiter::ModelChoice;
    #[cfg(all(windows, target_arch = "x86_64"))]
    if profile
        .description
        .device_execution
        .as_ref()
        .is_some_and(|device| device.precision == DevicePrecision::Float16)
    {
        if profile.description.model == ModelChoice::ContextQwenLarge {
            let device = profile
                .description
                .device_execution
                .as_ref()
                .context("Large device descriptor missing")?;
            let baseline =
                "c00371ad9407c25bd52999d87327115025b7c6f55a4e585645ca112d858de5bd".parse()?;
            ensure!(
                device.graph == baseline
                    && profile.description.profile_id
                        == "private-cuda-fp16-large-efficient-probe-v1",
                "Large FP16 graph diagnostic requires the reviewed baseline"
            );
            return require_fp16_profile(profile, ModelChoice::ContextQwenLarge);
        }
        return require_compact_fp16_profile(profile);
    }
    let model = profile.description.model;
    require_profile(
        profile,
        model == ModelChoice::ContextQwenCompact
            || (cfg!(all(windows, target_arch = "x86_64"))
                && model == ModelChoice::ContextQwenLarge),
    )
}

#[cfg(all(feature = "test-support", windows, target_arch = "x86_64"))]
fn require_compact_fp16_profile(profile: &super::super::ExecutionProfile) -> Result<()> {
    require_fp16_profile(profile, airk_core::arbiter::ModelChoice::ContextQwenCompact)
}

#[cfg(all(feature = "test-support", windows, target_arch = "x86_64"))]
fn require_fp16_profile(
    profile: &super::super::ExecutionProfile,
    model: airk_core::arbiter::ModelChoice,
) -> Result<()> {
    use airk_core::arbiter::device::PlacementPolicy;

    crate::native_inference::require_service_compute()?;
    profile.validate_native_source()?;
    authority(
        profile.execution_authorized() && profile.evaluation(),
        profile.release,
        profile.description.qualification.is_none()
            && profile.description.qualified_targets.is_empty(),
        profile.description.model == model,
        profile
            .description
            .device_execution
            .as_ref()
            .is_some_and(|device| {
                device.precision == DevicePrecision::Float16
                    && device.placement == PlacementPolicy::StrictGpu
            }),
        profile
            .gpu
            .as_ref()
            .is_some_and(|binding| binding.evidence.is_none()),
    )
}
#[cfg(feature = "test-support")]
fn require_profile(profile: &super::super::ExecutionProfile, allowed_model: bool) -> Result<()> {
    crate::native_inference::require_service_compute()?;
    profile.validate_native_source()?;
    authority(
        profile.execution_authorized() && profile.evaluation(),
        profile.release,
        profile.description.qualification.is_none(),
        allowed_model,
        profile
            .description
            .device_execution
            .as_ref()
            .is_some_and(|p| p.precision == DevicePrecision::Float32),
        profile.gpu.as_ref().is_some_and(|b| b.evidence.is_none()),
    )
}
#[cfg(feature = "test-support")]
pub(crate) fn session_created(binding: &Binding) -> Result<()> {
    binding
        .log
        .lock()
        .map_err(|_| anyhow::anyhow!("CUDA diagnostic observation poisoned"))?
        .state
        .session_created = true;
    Ok(())
}
#[cfg(feature = "test-support")]
pub(crate) fn complete_placement(value: &PlacementObservation) -> Result<()> {
    ensure!(
        value.complete
            && value.scope == "onnxruntime-final-session-state-ep-placement"
            && value.reported_nodes <= 65536
            && value.reported_string_bytes <= 16 << 20
            && value
                .nodes
                .iter()
                .all(|n| [&n.name, &n.domain, &n.op, &n.ep]
                    .iter()
                    .all(|v| v.len() <= 4096))
            && value.shim_status == 0
            && value.ort_error_code == 0
            && !value.nodes.is_empty()
            && value.reported_nodes == value.nodes.len() as u64
            && value
                .nodes
                .iter()
                .all(|n| !n.op.is_empty() && !n.ep.is_empty())
            && value.reported_string_bytes
                == value
                    .nodes
                    .iter()
                    .map(|n| n.name.len() + n.domain.len() + n.op.len() + n.ep.len())
                    .sum::<usize>() as u64,
        "diagnostic final SessionState placement is incomplete"
    );
    Ok(())
}
#[cfg(feature = "test-support")]
pub(crate) fn finish(
    session: super::super::session_owner::SessionOwner,
    binding: &Binding,
) -> Result<()> {
    finish_mode(session, binding, false)
}
#[cfg(feature = "test-support")]
pub(crate) fn finish_graph_metadata(
    session: super::super::session_owner::SessionOwner,
    binding: &Binding,
) -> Result<()> {
    finish_mode(session, binding, true)
}
#[cfg(feature = "test-support")]
fn finish_mode(
    session: super::super::session_owner::SessionOwner,
    binding: &Binding,
    graph_mode: bool,
) -> Result<()> {
    let placement = binding.device.placement(&session);
    let metadata = if graph_mode {
        placement
            .as_ref()
            .map_err(|e| anyhow::anyhow!("{e:#}"))
            .and_then(|p| {
                binding
                    .device
                    .graph_metadata(&session, binding.load_deadline, p)
            })
            .map(Some)
    } else {
        Ok(None)
    };
    let (captured, teardown) = capture_then_settle(
        session,
        || {
            let mut capture = binding
                .log
                .lock()
                .map_err(|_| anyhow::anyhow!("CUDA diagnostic observation poisoned"))?;
            if let Ok(Some(value)) = &metadata {
                capture.state.graph_metadata = Some(value.clone());
            }
            if let Ok(value) = &placement {
                capture.state.placement = Some(value.clone());
            }
            Ok(())
        },
        |s| s.synchronize(binding.load_deadline),
        || {
            binding.device.poison();
            std::mem::forget(binding.clone());
        },
        || {
            let result = binding.device.synchronize(binding.load_deadline);
            (
                result,
                binding.device.pending_claims(),
                binding.device.poisoned(),
            )
        },
    );
    let recorded = binding
        .log
        .lock()
        .map_err(|_| anyhow::anyhow!("CUDA diagnostic observation poisoned"))
        .map(|mut capture| {
            capture.state.teardown = Some(teardown.clone());
        });
    let observed = placement?;
    complete_placement(&observed)?;
    teardown.validate()?;
    captured.and(recorded)?;
    if graph_mode {
        metadata?
            .context("graph metadata absent")?
            .validate_against(&observed)?;
    } else {
        ensure!(
            !binding
                .log
                .lock()
                .map_err(|_| anyhow::anyhow!("CUDA diagnostic observation poisoned"))?
                .logs
                .truncated,
            "selected CUDA diagnostic logs were truncated"
        );
    }
    in_time(binding.load_deadline)
}
#[cfg(test)]
mod tests;
