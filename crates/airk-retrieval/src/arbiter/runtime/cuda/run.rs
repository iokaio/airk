// SPDX-License-Identifier: Apache-2.0
//! Actual transfer buffers survive native run errors until observed GPU settlement.
use super::*;
use ort::{
    session::{
        Session, SessionOutputs,
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
    let (scores, ()) =
        score_with_outputs(session, binding, input, deadline, focus, Ok, |_| Ok(()))?;
    Ok(scores)
}

#[cfg(all(feature = "test-support", windows, target_arch = "x86_64"))]
impl Binding {
    pub(in crate::arbiter::runtime) fn score_with_layer0_taps(
        &self,
        session: &mut Session,
        input: &super::super::CompiledInput,
        deadline: Instant,
        taps: &[(&'static str, usize)],
        tokens: usize,
    ) -> Result<(Vec<f32>, Vec<Vec<u16>>)> {
        let full_cut = super::super::Scorer::is_full_projection_tap_group(taps) && tokens == 1014;
        let decision_only =
            taps == super::super::Scorer::COMPACT_QKV_DECISION_TAPS && tokens == 1024;
        ensure!(
            self.evidence.is_none()
                && (full_cut || matches!(tokens, 32 | 1024))
                && input.input_ids.len() == tokens
                && input.candidate_positions.len() == if full_cut { 7 } else { 8 }
                && (full_cut
                    || decision_only
                    || taps
                        .iter()
                        .map(|&(_, width)| width)
                        .eq([2048, 1024, 1024, 2048])),
            "Compact FP16 layer-0 tap probe geometry changed"
        );
        score_with_outputs(
            session,
            self,
            input,
            deadline,
            false,
            |mut selected| {
                for &(name, width) in taps {
                    if full_cut && name.starts_with("/scorer/") {
                        let buffer = Tensor::from_array((
                            [1, tokens, width],
                            vec![0.0_f32; tokens * width],
                        ))?;
                        selected = selected.with(name).preallocate(name, buffer);
                        continue;
                    }
                    let buffer = Tensor::from_array((
                        [1, tokens, width],
                        vec![half::f16::ZERO; tokens * width],
                    ))?;
                    selected = selected.with(name).preallocate(name, buffer);
                }
                Ok(selected)
            },
            |outputs| {
                taps.iter()
                    .map(|&(name, width)| {
                        let value = outputs
                            .get(name)
                            .with_context(|| format!("Compact FP16 tap missing: {name}"))?;
                        if full_cut && name.starts_with("/scorer/") {
                            let (shape, values) = value.try_extract_tensor::<f32>()?;
                            ensure!(
                                shape.as_ref() == [1, tokens as i64, width as i64]
                                    && values.len() == tokens * width,
                                "full-projection FP32 tap shape differs: {name}"
                            );
                            let row = &values[(tokens - 1) * width..];
                            ensure!(
                                row.iter().all(|v| v.is_finite()),
                                "nonfinite full-projection tap: {name}"
                            );
                            return Ok(row
                                .iter()
                                .flat_map(|v| {
                                    let bits = v.to_bits();
                                    [bits as u16, (bits >> 16) as u16]
                                })
                                .collect());
                        }
                        let (shape, values) = value.try_extract_tensor::<half::f16>()?;
                        ensure!(
                            shape.as_ref() == [1, tokens as i64, width as i64]
                                && values.len() == tokens * width
                                && values.iter().all(|value| value.is_finite()),
                            "Compact FP16 tap shape or finite-value contract changed: {name}"
                        );
                        let keep_context = matches!(
                            name,
                            "/airk_g5/attention/13/k16" | "/airk_g5/attention/13/v16"
                        );
                        let retained = if decision_only || (full_cut && !keep_context) {
                            &values[(tokens - 1) * width..]
                        } else {
                            values
                        };
                        Ok(retained.iter().map(|value| value.to_bits()).collect())
                    })
                    .collect()
            },
        )
    }
}

fn score_with_outputs<T>(
    session: &mut Session,
    binding: &Binding,
    input: &super::super::CompiledInput,
    deadline: Instant,
    focus: bool,
    select_additional: impl FnOnce(OutputSelector) -> Result<OutputSelector>,
    read_additional: impl FnOnce(&SessionOutputs<'_>) -> Result<T>,
) -> Result<(Vec<f32>, T)> {
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
    // These Values stay owned in RunOptions through success, error and panic
    // cleanup. SessionOutputs only clones their backing in pinned ort rc10.
    let options = RunOptions::new()?;
    let selected = OutputSelector::no_default().with(output_name).preallocate(
        output_name,
        Tensor::from_array(([1, candidates], vec![0.0_f32; candidates]))?,
    );
    let options = options.with_outputs(select_additional(selected)?);
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
    let additional = read_additional(&outputs)?;
    in_time(deadline)?;
    binding.authorize_score(input.input_digest)?;
    Ok((scores.to_vec(), additional))
}

// MiniLM uses the same retained native device/claims and transfer guard as the
// Arbiter scorer. This hook has no profile or release-approval authority.
impl CudaDevice {
    pub(crate) fn configure_minilm_cuda(
        &self,
        builder: ort::session::builder::SessionBuilder,
        canonical_asset: &[u8],
        graph: &[u8],
        arena_limit: usize,
        deadline: Instant,
    ) -> Result<ort::session::builder::SessionBuilder> {
        crate::native_inference::require_service_compute()?;
        in_time(deadline)?;
        let external_name = "minilm-fp16.bin";
        let files = std::collections::BTreeMap::from([(
            external_name.to_owned(),
            canonical_asset.len().try_into()?,
        )]);
        let external = crate::arbiter::external_data::validate(graph, &files)?;
        ensure!(
            external.len() == 1 && external.contains(external_name),
            "MiniLM CUDA graph has an unexpected external weight reference"
        );
        let provider = ort::execution_providers::CUDAExecutionProvider::default()
            .with_device_id(self.ordinal())
            .with_tf32(false)
            .with_cuda_graph(false)
            .with_memory_limit(arena_limit);
        Ok(builder
            .with_execution_providers([provider.build().error_on_failure()])
            .map_err(run_failure)?
            .with_external_initializer_file_in_memory(
                external_name,
                std::borrow::Cow::Owned(canonical_asset.to_vec()),
            )?)
    }

    pub(crate) fn observe_minilm_placement(
        &self,
        session: &Session,
    ) -> Result<PlacementObservation> {
        self.placement(session)
    }

    pub(crate) fn run_minilm_cuda(
        self: &Arc<Self>,
        session: &mut Session,
        inputs: Vec<(&'static str, DynValue)>,
        shape: [usize; 3],
        extra_bytes: u64,
        headroom: u64,
        deadline: Instant,
    ) -> Result<DynValue> {
        crate::native_inference::require_service_compute()?;
        in_time(deadline)?;
        let output_name = "last_hidden_state";
        let options = RunOptions::new()?;
        let output = Tensor::from_array((shape, vec![0.0_f32; shape.iter().product()]))?;
        let options = options.with_outputs(
            OutputSelector::no_default()
                .with(output_name)
                .preallocate(output_name, output),
        );
        let claim = self.reserve_warm_work(extra_bytes, headroom, deadline)?;
        let transfers = Transfers {
            inputs: ManuallyDrop::new(inputs),
            options: ManuallyDrop::new(options),
            claim: RefCell::new(Some(claim)),
            device: self.clone(),
            settled: Cell::new(false),
        };
        let borrowed = transfers
            .inputs
            .iter()
            .map(|(name, value)| (*name, value))
            .collect::<Vec<_>>();
        let result = session
            .run_with_options(borrowed, &transfers.options)
            .map_err(run_failure);
        let wait = transfers.finish();
        match (result, wait) {
            (Ok(mut outputs), Ok(())) => outputs
                .remove(output_name)
                .context("MiniLM CUDA output was absent"),
            (Ok(outputs), Err(error)) => {
                if !transfers.settled.get() {
                    std::mem::forget(outputs);
                }
                Err(error)
            }
            (Err(error), Ok(())) => Err(error),
            (Err(error), Err(wait)) => Err(preserve_primary(
                error,
                format!("MiniLM GPU settlement also failed: {wait:#}"),
            )),
        }
    }
}

// After a failed MiniLM load, a poisoned resident remains in the runtime's
// weak-device map because unknown native work retains the strong device lease.
// Observe the exact adapter before the caller considers CPU fallback.
impl CudaRuntime {
    pub(crate) fn minilm_device_settled(&self, adapter_uuid: [u8; 16]) -> bool {
        !self.poisoned()
            && self.devices.lock().is_ok_and(|devices| {
                devices
                    .get(&adapter_uuid)
                    .and_then(Weak::upgrade)
                    .is_none_or(|device| !device.poisoned())
            })
    }
}
