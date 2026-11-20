// SPDX-License-Identifier: Apache-2.0
//! Scientific graph load/score for the fixed protected-attention Compact candidate.
mod inputs;
mod pressure;

use super::{CudaRuntime, Result, Sha256, Value, ensure, json};
use crate::arbiter::{compiler::CompiledInput, runtime::cuda::GraphMetadataObservation};
use anyhow::Context;
use ort::{
    execution_providers::{CUDAExecutionProvider, cuda::CUDAAttentionBackend},
    session::Session,
    value::Tensor,
};
use sha2::Digest;
use std::{
    fs::{File, OpenOptions},
    io::Read,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    time::Instant,
};

const GRAPH: &str = "147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9";
const WEIGHT: &str = "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3";
const ORIGINAL: &str = "735c77839099adf75426cf3ef9884ad69a99b60bb73e5fdf6247b7f321b12ded";
const INPUT: &str = "14290e4a90a69dee4d60f29b31c3be731f1b0c9c857bd04432f3758a3944fad2";
const ROOT: &str = "compact-cutlass-protected-staged-r1";
pub(super) const OPTIMIZERS: [(&str, &str); 3] = [
    (
        "optimization.constant_folding_max_output_size_in_bytes",
        "1048576",
    ),
    ("optimization.enable_cast_chain_elimination", "0"),
    (
        "optimization.disable_specified_optimizers",
        "FuseFp16InitializerToFp32NodeTransformer",
    ),
];

pub(super) fn held(path: &Path, length: u64, expected: &str, deadline: Instant) -> Result<File> {
    let mut file = OpenOptions::new().read(true).share_mode(1).open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.len() == length && meta.file_attributes() & 0x400 == 0,
        "incorrect or aliased protected asset"
    );
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        ensure!(
            Instant::now() < deadline,
            "deadline while authenticating protected asset"
        );
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    ensure!(
        format!("{:x}", digest.finalize()) == expected,
        "protected asset digest differs"
    );
    Ok(file)
}

fn fixed_input(campaign: &Path) -> Result<CompiledInput> {
    let path = campaign.join("compact-fp16-full-engineering-score-r1/native-result.json");
    let raw = super::pinned(&path, 325581, ORIGINAL)?;
    let input: CompiledInput = serde_json::from_value(
        serde_json::from_slice::<Value>(&raw)?["rows"][0]["compiled"].take(),
    )?;
    ensure!(
        input.input_digest.to_string() == INPUT
            && input.input_ids.len() == 1014
            && input.decision_position == 1013
            && input.attention_mask == vec![1; 1014]
            && input.position_ids.iter().copied().eq(0..1014)
            && input.option_mask == vec![true; 7]
            && input.candidate_positions.iter().copied().eq(32..39),
        "original sealed input differs"
    );
    Ok(input)
}

pub(super) fn score(session: &mut Session, x: CompiledInput) -> Result<Vec<f32>> {
    let width = x.input_ids.len();
    let cards = x.candidate_positions.len();
    let result = session.run(ort::inputs![
        "input_ids"=>Tensor::from_array(([1,width],x.input_ids))?,
        "attention_mask"=>Tensor::from_array(([1,width],x.attention_mask))?,
        "position_ids"=>Tensor::from_array(([1,width],x.position_ids))?,
        "decision_positions"=>Tensor::from_array(([1],vec![x.decision_position]))?,
        "candidate_positions"=>Tensor::from_array(([1,cards],x.candidate_positions))?,
        "option_mask"=>Tensor::from_array(([1,cards],x.option_mask))?
    ])?;
    let (shape, values) = result["scores"].try_extract_tensor::<f32>()?;
    ensure!(
        shape.as_ref() == [1, i64::try_from(cards)?]
            && values.len() == cards
            && values.iter().all(|x| x.is_finite()),
        "wrong protected score output"
    );
    Ok(values.to_vec())
}

fn check_gpu_nodes(placement: &super::super::super::PlacementObservation) -> Result<()> {
    let expected_sites = [
        "self_attn/q_proj",
        "self_attn/k_proj",
        "self_attn/v_proj",
        "self_attn/o_proj",
        "mlp/gate_proj",
        "mlp/up_proj",
        "mlp/down_proj",
    ];
    let nodes: std::collections::BTreeMap<_, _> = placement
        .nodes
        .iter()
        .map(|n| (n.name.as_str(), n))
        .collect();
    ensure!(
        placement.complete && nodes.len() == placement.nodes.len(),
        "incomplete or duplicate placed nodes"
    );
    for layer in 0..28 {
        let name = format!("/airk_compact_cutlass/{layer}/efficient");
        let node = nodes.get(name.as_str()).context("protected MHA missing")?;
        ensure!(
            node.ep == "CUDAExecutionProvider"
                && node.op == "MultiHeadAttention"
                && node.domain == "com.microsoft",
            "protected MHA placed incorrectly"
        );
        for site in expected_sites {
            let name = format!("/scorer/layers.{layer}/{site}/MatMul");
            let node = nodes
                .get(name.as_str())
                .context("learned projection missing")?;
            ensure!(
                node.ep == "CUDAExecutionProvider" && node.op == "MatMul",
                "learned projection left CUDA"
            );
        }
    }
    ensure!(
        nodes
            .values()
            .filter(|n| n.op == "MultiHeadAttention")
            .count()
            == 28
            && !nodes.values().any(|n| n.op == "GroupQueryAttention"),
        "wrong optimized attention population"
    );
    Ok(())
}

type Measure<'a> = dyn FnMut(
        &mut dyn FnMut(usize) -> Result<Vec<f32>>,
        &mut dyn FnMut() -> Result<Value>,
    ) -> Result<Value>
    + 'a;

type CalSession<'a> =
    dyn FnMut(&mut dyn FnMut(&CompiledInput, Instant) -> Result<Vec<f32>>) -> Result<Value> + 'a;

fn validate_cal_input(input: &CompiledInput) -> Result<()> {
    let t = input.input_ids.len();
    let n = input.candidate_positions.len();
    ensure!(
        (1..=8192).contains(&t)
            && (1..=8).contains(&n)
            && input.input_ids.iter().all(|id| (0..151936).contains(id))
            && input.attention_mask.len() == t
            && input.attention_mask.iter().all(|&bit| bit == 1)
            && input.decision_position == i64::try_from(t)? - 1
            && input.position_ids.iter().copied().eq(0..i64::try_from(t)?)
            && input.option_mask.len() == n
            && input.option_mask.iter().all(|&bit| bit)
            && input
                .candidate_positions
                .iter()
                .copied()
                .eq(32..32 + i64::try_from(n)?),
        "original all-live Compact compiler tensor contract differs"
    );
    Ok(())
}

impl CudaRuntime {
    /// Open an immutable science-only graph. Scoring requires an independently sealed observed typed graph digest.
    pub fn observe_protected_cutlass_model(
        &self,
        campaign: &Path,
        reviewed_graph: Option<&str>,
        deadline: Instant,
    ) -> Result<Value> {
        self.protected_cutlass_session(campaign, reviewed_graph, deadline, None, None, None)
    }

    /// Run the original qualification owner's warm schedule on one authenticated private session.
    pub fn time_protected_cutlass_model(
        &self,
        campaign: &Path,
        reviewed_graph: &str,
        deadline: Instant,
        pressure_tier: Option<u64>,
        mut measure: impl FnMut(
            &mut dyn FnMut(usize) -> Result<Vec<f32>>,
            &mut dyn FnMut() -> Result<Value>,
        ) -> Result<Value>,
    ) -> Result<Value> {
        self.protected_cutlass_session(
            campaign,
            Some(reviewed_graph),
            deadline,
            Some(&mut measure),
            None,
            pressure_tier,
        )
    }

    /// Supply the CAL owner a synchronized scorer only for the lifetime of the verified private model session.
    pub fn with_protected_cutlass_cal_session(
        &self,
        campaign: &Path,
        reviewed_typed_graph: &str,
        deadline: Instant,
        mut collect: impl FnMut(
            &mut dyn FnMut(&CompiledInput, Instant) -> Result<Vec<f32>>,
        ) -> Result<Value>,
    ) -> Result<Value> {
        self.protected_cutlass_session(
            campaign,
            Some(reviewed_typed_graph),
            deadline,
            None,
            Some(&mut collect),
            None,
        )
    }

    fn protected_cutlass_session(
        &self,
        campaign: &Path,
        reviewed_graph: Option<&str>,
        deadline: Instant,
        mut measure: Option<&mut Measure<'_>>,
        mut collect: Option<&mut CalSession<'_>>,
        pressure_tier: Option<u64>,
    ) -> Result<Value> {
        crate::native_inference::require_service_compute()?;
        ensure!(
            !self.is_product()
                && std::env::var("ORT_CUDA_GEMM_OPTIONS").as_deref() == Ok("2")
                && std::env::var("ORT_ENABLE_ATTENTION_KERNEL_DEBUG_INFO").as_deref()
                    == Ok(if measure.is_some() { "0" } else { "1" }),
            "fixed scientific provider environment required"
        );
        let stage = campaign.join(ROOT);
        let graph = stage.join("model.onnx");
        let _model = held(&graph, 854973, GRAPH, deadline)?;
        let _weight = held(&stage.join("weights.bin"), 3_441_397_760, WEIGHT, deadline)?;
        let adapters = self.adapters(deadline)?;
        let [adapter] = adapters.as_slice() else {
            anyhow::bail!("single original A6000 required")
        };
        ensure!(
            adapter.uuid
                == [
                    8, 226, 179, 246, 106, 57, 52, 86, 119, 203, 165, 114, 135, 167, 2, 236
                ]
                && (adapter.compute_major, adapter.compute_minor) == (8, 6),
            "wrong physical adapter"
        );
        let context = self
            .loaded
            .retain_primary(adapter.ordinal, self._native_lease.clone())?;
        let before = self.loaded.observe(adapter.ordinal, &context, deadline)?;
        let (arena, grant) = if let Some(tier) = pressure_tier {
            let gate = reviewed_graph.context("pressure requires sealed graph")?;
            ensure!(
                measure.is_some() && collect.is_none(),
                "only explicit scientific timing populations may use pressure"
            );
            let luid = self.loaded.pressure_adapter_luid(&context)?;
            let (arena, grant) = pressure::admit(
                tier,
                gate,
                &adapter.uuid,
                &luid,
                || {
                    Ok(self
                        .loaded
                        .observe(adapter.ordinal, &context, deadline)?
                        .free_bytes)
                },
                deadline,
            )?;
            (arena, Some(grant))
        } else {
            ensure!(
                std::env::var_os("AIRK_COMPACT_PRESSURE_TIER_GIB").is_none()
                    && std::env::var_os("AIRK_COMPACT_PRESSURE_NONCE").is_none(),
                "nonpressure run received an unrelated grant"
            );
            ensure!(
                before.free_bytes >= 24 << 30,
                "original 24 GiB CUDA reservation unavailable"
            );
            (24 << 30, None)
        };
        let provider = CUDAExecutionProvider::default()
            .with_device_id(adapter.ordinal)
            .with_tf32(false)
            .with_cuda_graph(false)
            .with_memory_limit(usize::try_from(arena)?)
            .with_attention_backend(CUDAAttentionBackend::EFFICIENT_ATTENTION)
            .build()
            .error_on_failure();
        let mut builder = Session::builder()?.with_execution_providers([provider])?;
        for (name, value) in OPTIMIZERS {
            builder = builder.with_config_entry(name, value)?;
        }
        let started = Instant::now();
        let mut session = builder.commit_from_file(&graph)?;
        let cold_us = started.elapsed().as_micros();
        let placement = self.loaded.placement(&session)?;
        let meta: GraphMetadataObservation =
            self.loaded.graph_metadata(&session, deadline, &placement)?;
        meta.validate_against(&placement)?;
        let routes = check_gpu_nodes(&placement).map_err(|e| e.to_string());
        // Keep graph-only refusals observable; numeric dispatch requires both independent checks.
        let cpu = meta
            .validate_bounded_fp16_cpu_values(9_443)
            .map_err(|e| e.to_string());
        let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&meta.rows)?));
        let mut population = None;
        let mut cal = None;
        let mut cal_native_forwards = 0usize;
        let score_result = if let Some(expected) = reviewed_graph {
            ensure!(
                routes.is_ok() && cpu.is_ok(),
                "uncertified optimized CUDA projection or CPU dataflow"
            );
            ensure!(
                fingerprint == expected,
                "typed optimized placement differs from original reviewed graph load"
            );
            if let Some(collector) = collect.as_mut() {
                let mut pending = false;
                cal = Some(collector(&mut |input, expires| {
                    ensure!(
                        !pending && expires <= deadline && Instant::now() < expires,
                        "original CAL deadline or prior native return refused"
                    );
                    validate_cal_input(input)?;
                    pending = true;
                    let scores = score(&mut session, input.clone())?;
                    self.loaded.synchronize(&context)?;
                    cal_native_forwards += 1;
                    ensure!(
                        Instant::now() < expires,
                        "CAL native result exceeded original deadline"
                    );
                    pending = false;
                    Ok(scores)
                })?);
                ensure!(
                    !pending,
                    "CAL scorer cannot close after unsettled or expired native work"
                );
                None
            } else if let Some(bench) = measure.as_mut() {
                let rows = ["32", "128", "1024", "8192"]
                    .map(|name| inputs::load(campaign, name))
                    .into_iter()
                    .collect::<Result<Vec<_>>>()?;
                let mut count = 0;
                let mut device_snapshot = || {
                    Ok(serde_json::to_value(self.loaded.observe(
                        adapter.ordinal,
                        &context,
                        deadline,
                    )?)?)
                };
                population = Some(bench(
                    &mut |index| {
                        ensure!(
                            Instant::now() < deadline && count < 21,
                            "original timing lifetime or sample cap exhausted"
                        );
                        let input = rows.get(index).context("unknown timing control")?;
                        count += 1;
                        let scores = score(&mut session, input.clone())?;
                        self.loaded.synchronize(&context)?;
                        ensure!(
                            Instant::now() < deadline,
                            "timing call exceeded original lifetime"
                        );
                        Ok(scores)
                    },
                    &mut device_snapshot,
                )?);
                None
            } else {
                let name =
                    std::env::var("AIRK_COMPACT_CUTLASS_INPUT").unwrap_or_else(|_| "fixed".into());
                let input = inputs::load(campaign, &name)?;
                let digest = input.input_digest.to_string();
                ensure!(
                    Instant::now() < deadline,
                    "numeric dispatch past original deadline"
                );
                let now = Instant::now();
                let values = score(&mut session, input)?;
                self.loaded.synchronize(&context)?;
                Some(
                    json!({"compiled_input_digest":digest,"raw_scores":values,"native_call_us":now.elapsed().as_micros(),"forward_synchronized":true}),
                )
            }
        } else {
            None
        };
        let after = self.loaded.observe(adapter.ordinal, &context, deadline)?;
        drop(session);
        self.loaded.synchronize(&context)?;
        let released = self.loaded.observe(adapter.ordinal, &context, deadline)?;
        ensure!(
            Instant::now() < deadline,
            "protected CUDA operation exceeded original lifetime"
        );
        let graph_evidence = if reviewed_graph.is_none() {
            Some(meta)
        } else {
            None
        };
        Ok(
            json!({"schema":"airk.private.compact-protected-cutlass-model/1","model_sha256":GRAPH,"weight_sha256":WEIGHT,
                 "adapter":adapter,"optimizers":OPTIMIZERS,"cold_us":cold_us,"typed_graph_sha256":fingerprint,
                 "graph":graph_evidence,"placement":placement,"route_refusal":routes.err(),"cpu_refusal":cpu.err(),"score":score_result,"before":before,"after":after,
                 "after_session_release":released,"pressure":grant,"cal":cal,"cal_native_forwards":cal_native_forwards,"timing":population,"product_admission":false,"qualification":false}),
        )
    }
}
