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

fn routes(p: &super::super::super::PlacementObservation) -> Result<()> {
    let nodes: std::collections::BTreeMap<_, _> =
        p.nodes.iter().map(|n| (n.name.as_str(), n)).collect();
    ensure!(
        p.complete
            && nodes.len() == p.nodes.len()
            && p.nodes
                .iter()
                .filter(|n| n.op == "MultiHeadAttention")
                .count()
                == 36
            && !p.nodes.iter().any(|n| n.op == "GroupQueryAttention"),
        "incomplete optimized Large attention"
    );
    for i in 0..36 {
        let name = format!("/airk_large_cutlass/{i}/efficient");
        let n = nodes.get(name.as_str()).context("Large MHA absent")?;
        ensure!(
            n.domain == "com.microsoft"
                && n.op == "MultiHeadAttention"
                && n.ep == "CUDAExecutionProvider",
            "Large MHA left CUDA"
        );
        for site in [
            "self_attn/q_proj",
            "self_attn/k_proj",
            "self_attn/v_proj",
            "self_attn/o_proj",
            "mlp/gate_proj",
            "mlp/up_proj",
            "mlp/down_proj",
        ] {
            let name = format!("/scorer/layers.{i}/{site}/MatMul");
            let n = nodes
                .get(name.as_str())
                .context("Large projection absent")?;
            ensure!(
                n.op == "MatMul" && n.ep == "CUDAExecutionProvider",
                "Large learned projection left CUDA"
            );
        }
    }
    Ok(())
}

impl CudaRuntime {
    /// Measure either the fixed layer-17 control or the distinct authenticated Large scientific graph.
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
    pub fn observe_large_direct(
        &self,
        campaign: &Path,
        case: Option<(usize, &str)>,
        deadline: Instant,
    ) -> Result<Value> {
        self.observe_large_arithmetic(campaign, false, case, true, deadline)
    }

    fn observe_large_arithmetic(
        &self,
        campaign: &Path,
        operand: bool,
        case: Option<(usize, &str)>,
        direct: bool,
        deadline: Instant,
    ) -> Result<Value> {
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
            _retained = (
                super::model::held(&graph, graph_bytes, graph_digest, deadline)?,
                super::model::held(
                    &weights.join("weights.bin"),
                    8_045_328_384,
                    WEIGHT,
                    deadline,
                )?,
            );
            for (name, value) in super::model::OPTIMIZERS {
                builder = builder.with_config_entry(name, value)?;
            }
            builder
                .with_config_entry(
                    "session.model_external_initializers_file_folder_path",
                    weights.to_str().context("invalid weight path")?,
                )?
                .commit_from_file(&graph)?
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
            let routes = routes(&placement).map_err(|e| e.to_string());
            let cpu = meta
                .validate_bounded_fp16_cpu_values(12_067)
                .map_err(|e| e.to_string());
            let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&meta.rows)?));
            let score = if let Some((width, expected)) = case {
                ensure!(
                    digest == expected && routes.is_ok() && cpu.is_ok(),
                    "Large typed graph, real projection or bounded shape controls differ"
                );
                let x = input(campaign, width)?;
                let input_digest = x.input_digest.to_string();
                let values = super::model::score(&mut session, x)?;
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
