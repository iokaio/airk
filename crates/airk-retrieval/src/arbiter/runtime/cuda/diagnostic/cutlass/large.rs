// SPDX-License-Identifier: Apache-2.0
//! Fixed Windows Large scientific controls for all 252 projections and grouped causal attention.
use super::{CudaRuntime, Result, Sha256, Value, ensure, json, pinned};
use crate::arbiter::compiler::CompiledInput;
use anyhow::Context;
use ort::{
    execution_providers::{CUDAExecutionProvider, cuda::CUDAAttentionBackend},
    session::Session,
    value::Tensor,
};
use sha2::Digest;
use std::{path::Path, time::Instant};

const GRAPH: &str = "a9badf775c6b4e2be5dd7ffa015c37d7aeace0c56ec943f55bc1be23e0854157";
const DIRECT_GRAPH: &str = "c1b5776144c6d9af07d08552cae40e491d8f866e89c53bab567adeeb32ca75e4";
const WEIGHT: &str = "5b053f02795e72ff88123b28f89d3f2c89da055e9ed2228bc60809dd0a2b4270";
const WIDTH: usize = 4096;
const TOKENS: usize = 32;

fn expanded_heads(input: &[f32]) -> Result<Vec<f32>> {
    ensure!(
        input.len() == TOKENS * 8 * 128,
        "expected eight Large KV heads per token"
    );
    Ok(input
        .as_chunks::<128>()
        .0
        .iter()
        .flat_map(|head| (0..4).flat_map(move |_| head.iter().copied()))
        .collect())
}

fn control(session: &mut Session, root: &Path) -> Result<Value> {
    let words = pinned(
        &root.join("operands.fp16"),
        393216,
        "aeadaeb542fc77bf4d3f872cd9d808693e9782bfa5fabf7a272dd70ca5ac4d26",
    )?;
    let vals: Vec<_> = words
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| half::f16::from_bits(u16::from_le_bytes(*p)).to_f32())
        .collect();
    ensure!(
        vals.iter().all(|x| x.is_finite()),
        "invalid original operand"
    );
    let (q, kv) = vals.split_at(TOKENS * WIDTH);
    let (k, v) = kv.split_at(TOKENS * 8 * 128);
    let shape = [1, TOKENS, WIDTH];
    let output = session.run(ort::inputs![
        "q" => Tensor::from_array((shape, q.to_vec()))?,
        "k" => Tensor::from_array((shape, expanded_heads(k)?))?,
        "v" => Tensor::from_array((shape, expanded_heads(v)?))?
    ])?;
    let (dims, data) = output["attention"].try_extract_tensor::<f32>()?;
    ensure!(
        dims.as_ref() == [1, 32, 4096]
            && data.len() == TOKENS * WIDTH
            && data.iter().all(|x| x.is_finite()),
        "incomplete Large native attention"
    );
    let expected = pinned(
        &root.join("reference.fp32"),
        524288,
        "b3940a3cd3fbe8e324f20d15049a808232ba7d1029017edf1a1c6020fd4bfa20",
    )?;
    let mut actual_digest = Sha256::new();
    let mut maximum = 0f64;
    let mut squared = 0f64;
    for (actual, reference) in data.iter().zip(expected.as_chunks::<4>().0) {
        let reference = f32::from_le_bytes(*reference);
        ensure!(reference.is_finite(), "nonfinite independent reference");
        let diff = f64::from(*actual) - f64::from(reference);
        maximum = maximum.max(diff.abs());
        squared += diff * diff;
        actual_digest.update(actual.to_le_bytes());
    }
    Ok(
        json!({"values":data.len(),"actual_sha256":format!("{:x}",actual_digest.finalize()),"maximum_absolute_error":maximum,
              "rms_error":(squared/(data.len() as f64)).sqrt()}),
    )
}

const TRAIN_WIDTH: usize = 1588;
const TRAIN_REMAINING_NS: u64 = 100_253_589_300;

fn train_input(campaign: &Path) -> Result<CompiledInput> {
    let base = campaign.join("compact-development-TRAIN_Q543-focus1024-run-r1");
    let capture: Value = serde_json::from_slice(&pinned(
        &base.join("capture.private.json"),
        1624,
        "0eb6a5b4f433e9022179a1c950095f1466e9d8ca0b111ecd63ea2a0d7eeaaa47",
    )?)?;
    ensure!(
        capture["case_id"] == "TRAIN_Q543" && capture["judged"] == false,
        "unexpected development capture"
    );
    let record: Value = serde_json::from_slice(&pinned(
        &base.join("fixture/context-cal-admissions/record-0001.json"),
        204733,
        "50ed40cec62a39b0067515b9aab2cd97c2849ef547dad149c49d86a32a863912",
    )?)?;
    train_compiled(&record)
}

fn train_compiled(record: &Value) -> Result<CompiledInput> {
    ensure!(
        record["ordinal_in_private_run"] == 1
            && record["Context_NN_dispatch"] == false
            && record["remaining_ns_at_observation"].as_u64() == Some(TRAIN_REMAINING_NS)
            && record["request"]["remaining_input_tokens"] == 4096
            && record["request"]["roster"]["cards"]
                .as_array()
                .is_some_and(|c| c.len() == 7),
        "original TRAIN capture or remaining allowance differs"
    );
    let mut rows = record["compiler_census"]
        .as_array()
        .context("missing Large compiler census")?
        .iter()
        .filter(|r| {
            r["model"] == "arbiter-context-qwen3-4b"
                && r["profile"]
                    == "d0e3c74813347b9bb4d3e17ffa4e337bbd16574ca80a81e4b1504c7e1546b307"
        });
    let r = rows.next().context("missing Large TRAIN compiler row")?;
    ensure!(
        rows.next().is_none()
            && r["NN_invocations"] == 0
            && r["fits_remaining_allowance"] == true
            && r["input_tokens"] == 1588,
        "wrong Large compiler row"
    );
    let x: CompiledInput = serde_json::from_value(r["compiled"].clone())?;
    ensure!(
        x.input_digest.to_string()
            == "af125f4c7ef1cd3776ea0cf7308743724a958744a921559746d907d0278868a4"
            && x.input_ids.len() == TRAIN_WIDTH
            && x.attention_mask == vec![1; TRAIN_WIDTH]
            && x.position_ids.iter().copied().eq(0..1588)
            && x.decision_position == 1587
            && x.candidate_positions.iter().copied().eq(32..39)
            && x.option_mask == vec![true; 7],
        "wrong original Large TRAIN input"
    );
    Ok(x)
}

fn input(campaign: &Path, width: usize) -> Result<CompiledInput> {
    ensure!(
        matches!(width, 32 | 128 | 1024 | 8192),
        "unreviewed original width"
    );
    let bytes = pinned(
        &campaign.join("large-fp16-independent-reference-inputs-r1.json"),
        206957,
        "c01cd56af84f90a6b842c5318f0c3e8eb757f14a9ce3387752c928c2f835c8c7",
    )?;
    let root: Value = serde_json::from_slice(&bytes)?;
    let rows = root["rows"]
        .as_array()
        .context("missing original Large rows")?;
    let originals: Vec<_> = rows
        .iter()
        .filter(|row| {
            row["input"]["input_ids"]
                .as_array()
                .is_some_and(|v| v.len() == width)
        })
        .collect();
    let [row] = originals.as_slice() else {
        anyhow::bail!("ambiguous original width");
    };
    let x: CompiledInput = serde_json::from_value(row["input"].clone())?;
    ensure!(
        x.attention_mask == vec![1; width]
            && x.position_ids.iter().copied().eq(0..i64::try_from(width)?)
            && x.decision_position == i64::try_from(width)? - 1
            && x.option_mask == vec![true; 8]
            && x.candidate_positions.iter().copied().eq(32..40),
        "original Large tensors differ"
    );
    Ok(x)
}

impl CudaRuntime {
    /// Measure either the fixed layer-17 control or the distinct authenticated Large scientific graph.
    #[cfg(windows)]
    pub fn observe_large_compensated(
        &self,
        campaign: &Path,
        operand: bool,
        case: Option<(usize, &str)>,
        deadline: Instant,
    ) -> Result<Value> {
        self.observe_large_arithmetic(campaign, operand, case, false, deadline)
    }

    /// Observe the direct FP32-activation model backed by the original immutable half weights.
    #[cfg(windows)]
    pub fn observe_large_direct(
        &self,
        campaign: &Path,
        case: Option<(usize, &str)>,
        deadline: Instant,
    ) -> Result<Value> {
        self.observe_large_arithmetic(campaign, false, case, true, deadline)
    }

    /// Retain the complete GNU final graph and placement before any numerical use.
    #[cfg(target_os = "linux")]
    pub fn observe_large_direct_gnu_graph(
        &self,
        campaign: &Path,
        deadline: Instant,
    ) -> Result<Value> {
        self.observe_large_arithmetic(campaign, false, None, true, deadline)
    }

    fn observe_large_arithmetic(
        &self,
        campaign: &Path,
        operand: bool,
        case: Option<(usize, &str)>,
        direct: bool,
        deadline: Instant,
    ) -> Result<Value> {
        #[cfg(target_os = "linux")]
        ensure!(
            direct && !operand && case.is_none(),
            "GNU Large requires its independently reviewed final graph and CPU bounds before scoring"
        );
        let train_start = Instant::now();
        crate::native_inference::require_service_compute()?;
        let (leaf, graph_bytes, graph_digest) = if direct {
            (
                "large-direct-fp32-stage-r2/model.onnx",
                1_100_471,
                DIRECT_GRAPH,
            )
        } else {
            ("large-cutlass-body-stage-r1/model.onnx", 1_326_071, GRAPH)
        };
        ensure!(
            !self.is_product()
                && (!operand || case.is_none())
                && Instant::now() < deadline
                && std::env::var("ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO").as_deref() == Ok("1")
                && std::env::var("ORT_CUDA_GEMM_OPTIONS").as_deref() == Ok("2"),
            "scientific Large environment required"
        );
        let adapters = self.adapters(deadline)?;
        let [adapter] = adapters.as_slice() else {
            anyhow::bail!("one dedicated adapter required")
        };
        ensure!(
            (adapter.compute_major, adapter.compute_minor) == (8, 6)
                && adapter.uuid
                    == [
                        8, 226, 179, 246, 106, 57, 52, 86, 119, 203, 165, 114, 135, 167, 2, 236
                    ]
                && adapter.physical_total_bytes == 51_526_500_352,
            "unexpected physical Large adapter"
        );
        let context = self
            .loaded
            .retain_primary(adapter.ordinal, self._native_lease.clone())?;
        let before = self.loaded.observe(adapter.ordinal, &context, deadline)?;
        ensure!(
            before.free_bytes >= if operand { 1 << 30 } else { 44 << 30 },
            "insufficient dedicated device headroom"
        );
        let provider = CUDAExecutionProvider::default()
            .with_device_id(adapter.ordinal)
            .with_tf32(false)
            .with_cuda_graph(false)
            .with_memory_limit(if operand { 256 << 20 } else { 40 << 30 })
            .with_attention_backend(CUDAAttentionBackend::EFFICIENT_ATTENTION)
            .build()
            .error_on_failure();
        let mut builder = Session::builder()?.with_execution_providers([provider])?;
        let root = campaign.join("large-cutlass-t32-operands-r1");
        let weights = campaign.join("large-fp16-conversion-owned-r5/candidate");
        #[cfg(windows)]
        let _retained;
        let mut session = if operand {
            let graph = pinned(
                &root.join("attention.onnx"),
                346,
                "c50be2de167e56d883ed62797985b27e0fe22083eb9b321bb2ff3984ec09d869",
            )?;
            builder
                .with_config_entry("session.disable_cpu_ep_fallback", "1")?
                .commit_from_memory(&graph)?
        } else {
            let graph = campaign.join(leaf);
            #[cfg(windows)]
            {
                _retained = (
                    super::model::held(&graph, graph_bytes, graph_digest, deadline)?,
                    super::model::held(
                        &weights.join("weights.bin"),
                        8_045_328_384,
                        WEIGHT,
                        deadline,
                    )?,
                );
            }
            for (name, value) in super::model::OPTIMIZERS {
                builder = builder.with_config_entry(name, value)?;
            }
            #[cfg(windows)]
            {
                builder
                    .with_config_entry(
                        "session.model_external_initializers_file_folder_path",
                        weights.to_str().context("invalid weight path")?,
                    )?
                    .commit_from_file(&graph)?
            }
            #[cfg(target_os = "linux")]
            {
                super::model::linux::Inputs::open_paths(
                    (&graph, graph_bytes, graph_digest),
                    (&weights.join("weights.bin"), 8_045_328_384, WEIGHT),
                    deadline,
                )?
                .session(builder)?
            }
        };
        let placement = self.loaded.placement(&session)?;
        let result = if operand {
            placement.validate_strict()?;
            let [n] = placement.nodes.as_slice() else {
                anyhow::bail!("single native MHA required")
            };
            ensure!(
                n.op == "MultiHeadAttention"
                    && n.name == "airk_large_cutlass_t32_operand"
                    && n.domain == "com.microsoft",
                "wrong Large operand dispatch"
            );
            control(&mut session, &root)?
        } else {
            let meta = self.loaded.graph_metadata(&session, deadline, &placement)?;
            meta.validate_against(&placement)?;
            let routes = super::large_routes::routes(&placement).map_err(|e| e.to_string());
            #[cfg(target_os = "linux")]
            let cpu = crate::arbiter::runtime::cuda::linux::check_model_cpu_bounds(
                airk_core::arbiter::ModelChoice::ContextQwenLarge,
                graph_digest.parse()?,
                &meta,
            )
            .map_err(|e| e.to_string());
            #[cfg(windows)]
            let cpu = meta
                .validate_bounded_fp16_cpu_values(12_067)
                .map_err(|e| e.to_string());
            let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&meta.rows)?));
            let score = if let Some((width, expected)) = case {
                ensure!(
                    digest == expected && routes.is_ok() && cpu.is_ok(),
                    "Large typed graph, real projection or bounded shape controls differ"
                );
                let x = if width == TRAIN_WIDTH {
                    ensure!(direct, "TRAIN Q543 requires the direct FP32 candidate");
                    train_input(campaign)?
                } else {
                    input(campaign, width)?
                };
                let input_digest = x.input_digest.to_string();
                let values = super::model::score(&mut session, x)?;
                if width == TRAIN_WIDTH {
                    ensure!(
                        train_start.elapsed().as_nanos() <= u128::from(TRAIN_REMAINING_NS),
                        "original TRAIN request allowance expired"
                    );
                }
                Some(
                    json!({"compiled_input_digest":input_digest,"raw_scores":values,"width":width}),
                )
            } else {
                None
            };
            json!({"typed_graph_sha256":digest,"graph":case.is_none().then_some(meta),"score":score,
                   "route_refusal":routes.err(),"cpu_refusal":cpu.err(),"model_sha256":graph_digest,"weight_sha256":WEIGHT})
        };
        self.loaded.synchronize(&context)?;
        let live = self.loaded.observe(adapter.ordinal, &context, deadline)?;
        drop(session);
        self.loaded.synchronize(&context)?;
        let released = self.loaded.observe(adapter.ordinal, &context, deadline)?;
        ensure!(
            Instant::now() < deadline,
            "Large diagnostic lifetime expired"
        );
        Ok(
            json!({"result":result,"adapter":adapter,"placement":placement,"before":before,"live":live,"released":released,"qualification":false}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_train_q543_requires_original_compiler_and_allowance() {
        let mut record = json!({
            "ordinal_in_private_run":1, "Context_NN_dispatch":false, "remaining_ns_at_observation":TRAIN_REMAINING_NS,
            "request":{"remaining_input_tokens":4096,"roster":{"cards":[1,2,3,4,5,6,7]}},
            "compiler_census":[{"model":"arbiter-context-qwen3-0.6b","profile":"compact"}]
        });
        assert!(
            train_compiled(&record)
                .is_err_and(|e| e.to_string().contains("missing Large TRAIN compiler row"))
        );
        record["remaining_ns_at_observation"] = json!(TRAIN_REMAINING_NS + 1);
        assert!(
            train_compiled(&record).is_err_and(|e| e.to_string().contains("remaining allowance"))
        );
    }

    #[test]
    fn large_kv_expansion_preserves_four_consecutive_heads() {
        let x: Vec<_> = (0..TOKENS * 8)
            .flat_map(|head| std::iter::repeat_n(head as f32, 128))
            .collect();
        let y = expanded_heads(&x).unwrap();
        for (head, chunk) in y.as_chunks::<128>().0.iter().enumerate() {
            assert!(chunk.iter().all(|&v| v == (head / 4) as f32));
        }
        assert!(expanded_heads(&x[..x.len() - 128]).is_err());
    }
}
