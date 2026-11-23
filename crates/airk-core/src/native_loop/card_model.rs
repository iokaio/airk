// SPDX-License-Identifier: Apache-2.0
//! The card model observation of the Answer loop. It records which Context
//! model, device and precision a card call would use and decides nothing, so
//! it is kept outside `native_loop.rs`, which the loop semantics digest
//! (`arbiter::answer_loop::loop_semantic_policy_digest`) hashes.
use crate::arbiter::{answer_loop::LoopStage, answer_loop::card_task};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// The Context model that would score loop card calls now, as the native
/// owner observes its selected profile: model, device and (GPU only)
/// precision. Observation only; no authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCardModel {
    pub model: crate::arbiter::device::SelectedModel,
    pub device: crate::arbiter::answer_loop::ExecutionDevice,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<crate::arbiter::device::DevicePrecision>,
}
impl NativeCardModel {
    /// The card model of a profile that declares every loop card task, or
    /// `None` when it declares none of them or cannot score cards.
    pub fn from_profile(profile: &crate::arbiter::Profile) -> Result<Option<Self>> {
        use crate::arbiter::{
            answer_loop::ExecutionDevice,
            device::{GpuBackend, SelectedModel},
        };
        let tasks = [
            LoopStage::Split,
            LoopStage::Plan,
            LoopStage::Coverage,
            LoopStage::FollowUp,
        ]
        .into_iter()
        .filter_map(card_task);
        if !tasks.clone().all(|task| profile.tasks.contains(&task)) {
            return Ok(None);
        }
        let model = SelectedModel::from_model(profile.model)?;
        let (device, precision) = match &profile.device_execution {
            None => (ExecutionDevice::Cpu, None),
            Some(execution) => (
                match execution.backend {
                    GpuBackend::Cuda => ExecutionDevice::Cuda,
                    GpuBackend::DirectMl => ExecutionDevice::DirectMl,
                    GpuBackend::CoreMl => ExecutionDevice::CoreMl,
                },
                Some(execution.precision),
            ),
        };
        let value = Self {
            model,
            device,
            precision,
        };
        Ok(value.validate().is_ok().then_some(value))
    }
    pub fn validate(&self) -> Result<()> {
        use crate::arbiter::{answer_loop::ExecutionDevice, device::SelectedModel};
        let cpu = self.device == ExecutionDevice::Cpu;
        ensure!(
            self.model == SelectedModel::Arbiter && cpu == self.precision.is_none(),
            "invalid loop card model observation"
        );
        Ok(())
    }
}
