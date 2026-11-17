// SPDX-License-Identifier: Apache-2.0
//! Actual transfer buffers survive native run errors until observed GPU settlement.
use super::*;
use ort::{
    session::{
        Session,
        run_options::{HasSelectedOutputs, OutputSelector, RunOptions},
    },
    value::{DynValue, Tensor},
};
use std::{
    cell::{Cell, RefCell},
    mem::ManuallyDrop,
};
struct Transfers {
    inputs: ManuallyDrop<Vec<(&'static str, DynValue)>>,
    options: ManuallyDrop<RunOptions<HasSelectedOutputs>>,
    claim: RefCell<Option<DeviceReservation>>,
    device: Arc<CudaDevice>,
    settled: Cell<bool>,
}
impl Transfers {
    fn finish(&self) -> Result<()> {
        let Some(claim) = self.claim.borrow_mut().take() else {
            ensure!(
                self.settled.get(),
                "GPU transfer settlement remains unknown"
            );
            return Ok(());
        };
        let result = claim.settle();
        self.settled.set(!self.device.poisoned());
        result
    }
}
impl Drop for Transfers {
    fn drop(&mut self) {
        let _ = self.finish();
        if self.settled.get() {
            // SAFETY: inputs and host preallocated output are released only
            // after the exact native context has really synchronized.
            unsafe {
                ManuallyDrop::drop(&mut self.inputs);
                ManuallyDrop::drop(&mut self.options);
            }
        } else {
            std::mem::forget(self.device.clone());
        }
    }
}
pub(in crate::arbiter::runtime) fn score(
    session: &mut Session,
    binding: &Binding,
    input: &super::super::CompiledInput,
    deadline: Instant,
    focus: bool,
) -> Result<Vec<f32>> {
    in_time(deadline)?;
    binding.authorize_score(input.input_digest)?;
    let tokens = input.input_ids.len();
    let candidates = input.candidate_positions.len();
    let output_name = if focus { "logits" } else { "scores" };
    let mut inputs: Vec<(&'static str, DynValue)> = vec![
        (
            "input_ids",
            Tensor::from_array(([1, tokens], input.input_ids.clone()))?.into_dyn(),
        ),
        (
            "attention_mask",
            Tensor::from_array(([1, tokens], input.attention_mask.clone()))?.into_dyn(),
        ),
        (
            "position_ids",
            Tensor::from_array(([1, tokens], input.position_ids.clone()))?.into_dyn(),
        ),
    ];
    if !focus {
        inputs.extend([
            (
                "decision_positions",
                Tensor::from_array(([1], vec![input.decision_position]))?.into_dyn(),
            ),
            (
                "candidate_positions",
                Tensor::from_array(([1, candidates], input.candidate_positions.clone()))?
                    .into_dyn(),
            ),
            (
                "option_mask",
                Tensor::from_array(([1, candidates], input.option_mask.clone()))?.into_dyn(),
            ),
        ]);
    }
    // This actual Value stays owned in RunOptions through success, error and
    // panic cleanup. SessionOutputs only clones its backing in pinned ort rc10.
    let options = RunOptions::new()?.with_outputs(
        OutputSelector::no_default().with(output_name).preallocate(
            output_name,
            Tensor::from_array(([1, candidates], vec![0.0_f32; candidates]))?,
        ),
    );
    let claim = binding.device.reserve_warm_work(
        binding.plan.warm_extra_bytes(),
        binding.plan.headroom_bytes(),
        deadline,
    )?;
    let transfers = Transfers {
        inputs: ManuallyDrop::new(inputs),
        options: ManuallyDrop::new(options),
        claim: RefCell::new(Some(claim)),
        device: binding.device.clone(),
        settled: Cell::new(false),
    };
    let borrowed = transfers
        .inputs
        .iter()
        .map(|(name, value)| (*name, value))
        .collect::<Vec<_>>();
    let result = session
        .run_with_options(borrowed, &transfers.options)
        .map_err(run_failure)
        .context("Arbiter full trained GPU readout execution failed");
    let settlement = transfers.finish();
    if !transfers.settled.get() {
        let error = match result {
            Ok(outputs) => {
                std::mem::forget(outputs);
                settlement
                    .err()
                    .unwrap_or_else(|| native_fault("GPU settlement unknown".into()))
            }
            Err(error) => preserve_primary(
                error,
                format!(
                    "GPU settlement also failed: {}",
                    settlement.err().map(|e| e.to_string()).unwrap_or_default()
                ),
            ),
        };
        return Err(error);
    }
    let outputs = match (result, settlement) {
        (Ok(v), Ok(())) => v,
        (Err(e), Ok(())) | (Ok(_), Err(e)) => return Err(e),
        (Err(e), Err(wait)) => {
            return Err(preserve_primary(
                e,
                format!("GPU settlement also failed: {wait:#}"),
            ));
        }
    };
    let (shape, scores) = outputs[output_name].try_extract_tensor::<f32>()?;
    ensure!(
        shape.as_ref() == [1, candidates as i64] && scores.iter().all(|s| s.is_finite()),
        "Arbiter GPU score shape/finite contract changed"
    );
    in_time(deadline)?;
    binding.authorize_score(input.input_digest)?;
    Ok(scores.to_vec())
}
