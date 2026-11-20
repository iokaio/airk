// SPDX-License-Identifier: Apache-2.0
//! Test-support-only original-operand Windows CUTLASS attention measurement.
mod model;
use super::super::CudaRuntime;
use anyhow::{Context, Result, ensure};
use ort::execution_providers::{CUDAExecutionProvider, cuda::CUDAAttentionBackend};
use ort::session::Session;
use ort::value::Tensor;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Instant};

const TOKENS: usize = 1014;
const HEADS: usize = 16;
const DIM: usize = 128;
const WIDTH: usize = HEADS * DIM;
const NODE: &str = "airk_cutlass_gqa_decision_control";
const GRAPH: &str = "9f10f2f9d44840d77680644fd3d96d9a5fc8934726964463f62f0984bc68521a";
const INPUTS: &str = "f5376e4178eb2e8522225d77d87297f7110ca7c88767c06ebb2a56b09620f3b6";

fn pinned(path: &Path, length: usize, sha: &str) -> Result<Vec<u8>> {
    ensure!(
        fs::metadata(path)?.len() == u64::try_from(length)?,
        "unexpected CUTLASS input size"
    );
    let bytes = fs::read(path)?;
    ensure!(
        bytes.len() == length && format!("{:x}", Sha256::digest(&bytes)) == sha,
        "CUTLASS source changed"
    );
    Ok(bytes)
}

fn expanded_kv(input: &[f32]) -> Result<Vec<f32>> {
    ensure!(
        input.len() == TOKENS * WIDTH / 2,
        "expected eight distinct key/value heads per token"
    );
    Ok(input
        .as_chunks::<DIM>()
        .0
        .iter()
        .flat_map(|head| head.iter().chain(head.iter()).copied())
        .collect())
}

fn inputs(path: &Path) -> Result<(Vec<f32>, Vec<f32>, Vec<f32>)> {
    let bytes = pinned(path, (WIDTH + TOKENS * WIDTH) * 2, INPUTS)?;
    let floats: Vec<_> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| half::f16::from_bits(u16::from_le_bytes([b[0], b[1]])).to_f32())
        .collect();
    ensure!(
        floats.iter().all(|x| x.is_finite()),
        "nonfinite original attention operand"
    );
    let (q, kv) = floats.split_at(WIDTH);
    let (k, v) = kv.split_at(TOKENS * WIDTH / 2);
    Ok((q.to_vec(), expanded_kv(k)?, expanded_kv(v)?))
}

impl CudaRuntime {
    /// Evaluate one fixed scientific control. The owner retains its genuine service lease and all native capabilities.
    pub fn observe_compact_cutlass_operand(&self, root: &Path, deadline: Instant) -> Result<Value> {
        crate::native_inference::require_service_compute()?;
        ensure!(
            Instant::now() < deadline,
            "original diagnostic lifetime expired"
        );
        ensure!(
            std::env::var("ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO").as_deref() == Ok("1")
                && std::env::var("ORT_CUDA_GEMM_OPTIONS").as_deref() == Ok("2"),
            "original process math/debug controls required"
        );
        let graph = pinned(&root.join("attention.onnx"), 352, GRAPH)?;
        let (last_query, keys, values) = inputs(&root.join("operands.fp16"))?;
        ensure!(!self.is_product(), "scientific CU15 runtime required");
        let adapters = self.adapters(deadline)?;
        let [adapter] = adapters.as_slice() else {
            anyhow::bail!("exactly one original adapter required")
        };
        ensure!(
            (adapter.compute_major, adapter.compute_minor) == (8, 6)
                && adapter.uuid
                    == [
                        8, 226, 179, 246, 106, 57, 52, 86, 119, 203, 165, 114, 135, 167, 2, 236
                    ]
                && adapter.physical_total_bytes == 51_526_500_352,
            "dedicated SM86 required"
        );
        let context = self
            .loaded
            .retain_primary(adapter.ordinal, self._native_lease.clone())?;
        let initial = self.loaded.observe(adapter.ordinal, &context, deadline)?;
        let provider = CUDAExecutionProvider::default()
            .with_device_id(adapter.ordinal)
            .with_tf32(false)
            .with_cuda_graph(false)
            .with_memory_limit(256 << 20)
            .with_attention_backend(CUDAAttentionBackend::EFFICIENT_ATTENTION)
            .build()
            .error_on_failure();
        let mut session = Session::builder()?
            .with_execution_providers([provider])?
            .with_config_entry("session.disable_cpu_ep_fallback", "1")?
            .commit_from_memory(&graph)?;
        let placement = self.loaded.placement(&session)?;
        placement.validate_strict()?;
        let [placed] = placement.nodes.as_slice() else {
            anyhow::bail!("one actual MHA kernel required")
        };
        ensure!(
            placed.name == NODE
                && placed.op == "MultiHeadAttention"
                && placed.domain == "com.microsoft",
            "unexpected final MHA placement"
        );
        let mut outputs = Vec::with_capacity(2);
        let mut timings = Vec::with_capacity(2);
        let mut observed = Vec::with_capacity(2);
        let shape = [1, TOKENS, WIDTH];
        for repeat in [false, true] {
            ensure!(
                Instant::now() < deadline,
                "original numeric deadline expired"
            );
            let mut queries = if repeat {
                last_query.repeat(TOKENS)
            } else {
                vec![0.; TOKENS * WIDTH]
            };
            queries[(TOKENS - 1) * WIDTH..].copy_from_slice(&last_query);
            let q = Tensor::from_array((shape, queries))?;
            let k = Tensor::from_array((shape, keys.clone()))?;
            let v = Tensor::from_array((shape, values.clone()))?;
            let start = Instant::now();
            let result = session.run(ort::inputs!["q"=>q,"k"=>k,"v"=>v])?;
            self.loaded.synchronize(&context)?;
            timings.push(start.elapsed().as_micros());
            let (actual, data) = result["attention"].try_extract_tensor::<f32>()?;
            ensure!(
                actual.as_ref() == [1, TOKENS as i64, WIDTH as i64]
                    && data.len() == TOKENS * WIDTH
                    && data.iter().all(|x| x.is_finite()),
                "nonfinite or incomplete CUTLASS output"
            );
            outputs.push(data[(TOKENS - 1) * WIDTH..].to_vec());
            observed.push(self.loaded.observe(adapter.ordinal, &context, deadline)?);
        }
        ensure!(
            outputs[0]
                .iter()
                .zip(&outputs[1])
                .all(|(x, y)| x.to_bits() == y.to_bits()),
            "causally unrelated earlier query rows changed the final query"
        );
        drop(session);
        self.loaded.synchronize(&context)?;
        let after = self.loaded.observe(adapter.ordinal, &context, deadline)?;
        ensure!(
            Instant::now() < deadline,
            "cutlass control exceeded original lifetime"
        );
        let executable = std::env::current_exe().context("current scientific image missing")?;
        Ok(
            json!({"schema":"airk.private.compact-cutlass-operand-native/1","image":executable,"model_sha256":GRAPH,
            "operands_sha256":INPUTS,"adapter":adapter,"placement":placement,"native_call_us":timings,
            "before_session":initial,"after_each_sync":observed,"after_session_release":after,
            "query_zero_prefix":outputs[0],"query_repeated_prefix":outputs[1],"qualification":false}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutlass_grouped_heads_duplicate_each_original_head_in_token_order() -> Result<()> {
        assert!(expanded_kv(&[0.; DIM]).is_err());
        let original: Vec<_> = (0..TOKENS * WIDTH / 2).map(|i| (i / DIM) as f32).collect();
        let expanded = expanded_kv(&original)?;
        for token in [0, 1, TOKENS - 1] {
            for qhead in 0..HEADS {
                let base = token * WIDTH + qhead * DIM;
                assert_eq!(
                    expanded[base..base + DIM],
                    [((token * HEADS + qhead) / 2) as f32; DIM]
                );
            }
        }
        Ok(())
    }
}
