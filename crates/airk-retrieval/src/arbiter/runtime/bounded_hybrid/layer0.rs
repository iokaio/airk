// SPDX-License-Identifier: Apache-2.0
//! Private fixed-T32 outputs for an authenticated Compact FP16 session.

use super::*;

pub const COMPACT_FP16_LAYER0_TAPS: &[(&str, usize)] = &[
    ("/airk_g5/attention/0/q16", 2048),
    ("/airk_g5/attention/0/k16", 1024),
    (V_PROJECTION, 1024),
    ("/airk_g5/attention/0/raw16", 2048),
];
const GQA: &str = "/airk_g5/attention/0/efficient";
const V_PROJECTION: &str = "/airk_g5/weight//scorer/layers.0/self_attn/v_proj/MatMul/output16";
const PROJECTION_INPUT: &str = "/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/input16";
pub const COMPACT_FP16_LAYER0_PROJECTION_TAPS: &[(&str, usize)] = &[
    (PROJECTION_INPUT, 2048),
    (
        "/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/output16",
        1024,
    ),
    (V_PROJECTION, 1024),
    (
        "/airk_g5/weight//scorer/layers.0/self_attn/q_proj/MatMul/output16",
        2048,
    ),
];
const PROJECTION_NODES: [&str; 3] = [
    "/scorer/layers.0/self_attn/k_proj/MatMul",
    "/scorer/layers.0/self_attn/v_proj/MatMul",
    "/scorer/layers.0/self_attn/q_proj/MatMul",
];

#[derive(Clone, Copy)]
pub(super) enum Layer0Cut {
    Attention,
    Projection,
}

impl Layer0Cut {
    pub(super) fn taps(self) -> &'static [(&'static str, usize)] {
        match self {
            Self::Attention => COMPACT_FP16_LAYER0_TAPS,
            Self::Projection => COMPACT_FP16_LAYER0_PROJECTION_TAPS,
        }
    }

    pub(super) fn scope(self) -> &'static [u8] {
        match self {
            Self::Attention => b"airk.private.compact-fp16-cuda-layer0-taps/1",
            Self::Projection => b"airk.private.compact-fp16-cuda-layer0-projection-taps/1",
        }
    }
}
const T32_INPUT: &str = "ec7b4845c37fb7cbe2a64a866f8b6cdc03479d65593cdb0a1d01102fe56fc335";

#[derive(Debug)]
pub struct CompactFp16Layer0GraphRefusal {
    pub observation: Box<GraphMetadataObservation>,
    pub(super) cause: anyhow::Error,
}

impl std::fmt::Display for CompactFp16Layer0GraphRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Compact FP16 layer-0 observation refused: {:#}",
            self.cause
        )
    }
}

impl std::error::Error for CompactFp16Layer0GraphRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.cause.as_ref())
    }
}

#[derive(Clone, Copy)]
struct ReviewedProjection {
    graph_sha: &'static str,
    load_sha: &'static str,
    load_schema: &'static str,
}

const REVIEWED_QKV: ReviewedProjection = ReviewedProjection {
    graph_sha: "e621cf6bffad1f0a27e3cc1ecba5b4379949295498f6585963620d71cfe4ff14",
    load_sha: "42a273598b5c8678535691426ed725041a5d9565ff1fbaf535a43938894f07a4",
    load_schema: "airk.private.compact-fp32-qkv-graph-diagnostic/1",
};
const REVIEWED_QKVO: Option<ReviewedProjection> = Some(ReviewedProjection {
    graph_sha: "0f2407345000cb244674238eead6178bc711238f2a4ddc5b61eef603d87be0b1",
    load_sha: "ef46663525337868d8a47fe440b9199bc927cf1625e4b4caa3abeb764e93a1f1",
    load_schema: "airk.private.compact-fp32-qkvo-graph-diagnostic/1",
});

fn qkv_projection_options(
    builder: ort::session::builder::SessionBuilder,
) -> Result<ort::session::builder::SessionBuilder> {
    Ok(builder
        .with_config_entry(
            "optimization.constant_folding_max_output_size_in_bytes",
            "1048576",
        )?
        .with_config_entry("optimization.enable_cast_chain_elimination", "0")?
        .with_config_entry(
            "optimization.disable_specified_optimizers",
            "FuseFp16InitializerToFp32NodeTransformer",
        )?)
}

fn reviewed_projection_gate(
    raw: &[u8],
    evidence_sha: Sha256Digest,
    review: ReviewedProjection,
) -> Result<GraphGate> {
    ensure!(
        evidence_sha == review.load_sha.parse()?,
        "exact reviewed Compact QKV graph receipt required"
    );
    let result = GraphGate::read_result(raw, evidence_sha)?;
    let snapshot = &result["graph_observation"];
    let teardown = &snapshot["teardown"];
    ensure!(
        result["schema"] == review.load_schema
            && result["complete"] == true
            && result["qualified"] == false
            && result["product_admission"] == false
            && result["source_mask"]["verified_candidate_graph"] == review.graph_sha
            && result["numeric_forward_attempts"] == 0
            && result["native_forward_returned"] == false
            && result["forward_synchronized"] == false
            && result.get("runtime_profile").is_some_and(Value::is_null)
            && result["rows"].as_array().is_some_and(Vec::is_empty)
            && result["input_refusals"]
                .as_array()
                .is_some_and(Vec::is_empty)
            && result["session_teardown_observed"] == true
            && result["device_teardown_observed"] == true
            && result["native_host_charge_release_observed"] == true
            && result["native_control_cleanup_observed"] == true
            && result["native_control_retained_for_unknown_cuda"] == false
            && result["process_cuda_poisoned"] == false
            && snapshot["session_created"] == true
            && snapshot["placement"]["complete"] == true
            && teardown["pre_sync_observed"] == true
            && teardown["session_drop_attempted"] == true
            && teardown["session_drop_returned"] == true
            && teardown["post_sync_observed"] == true
            && teardown["pending_claims"] == 0
            && teardown["poisoned"] == false
            && teardown["errors"].as_array().is_some_and(Vec::is_empty),
        "complete reviewed Compact QKV graph-only evidence required"
    );
    GraphGate::from_result(result, evidence_sha, GraphPolicy::CompactFp16)
}

fn require_qkv_projection_profile(
    profile: &ExecutionProfile,
    graph_sha: Sha256Digest,
) -> Result<()> {
    use airk_core::arbiter::device::DevicePrecision;

    cuda::diagnostic::require_graph_diagnostic_profile(profile)?;
    let description = profile.description();
    let device = description
        .device_execution
        .as_ref()
        .context("Compact QKV projection diagnostic CUDA profile missing")?;
    ensure!(
        description.model == ModelChoice::ContextQwenCompact
            && device.precision == DevicePrecision::Float16
            && device.graph == graph_sha,
        "Compact QKV projection diagnostic profile or graph differs"
    );
    Ok(())
}

impl Scorer {
    /// Captures one disposable observation of a separately reviewed Compact FP16
    /// graph whose Q, K and V projections are under diagnostic review.
    pub fn load_private_compact_fp32_qkv_projection_diagnostic(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        baseline_graph_sha: Sha256Digest,
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<()> {
        ensure!(
            reviewed_graph_sha != baseline_graph_sha,
            "Compact QKV projection diagnostic requires a distinct reviewed graph identity"
        );
        require_qkv_projection_profile(&profile, reviewed_graph_sha)?;
        let session = Self::open_internal(
            profile,
            source,
            qkv_projection_options,
            OpenMode::GraphMetadataDiagnostic,
        )?;
        ensure!(
            session.is_none(),
            "QKV projection diagnostic unexpectedly returned a scorer"
        );
        Ok(())
    }

    /// Opens only the reviewed Compact projection experiment in a bounded CUDA session.
    pub fn open_private_compact_fp32_qkv_projection_profiled(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: &Path,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<Self> {
        Self::open_private_compact_fp32_projection_profiled(
            profile,
            source,
            profiling_prefix,
            reviewed_graph_result,
            reviewed_graph_sha,
            REVIEWED_QKV,
        )
    }

    pub fn open_private_compact_fp32_qkvo_projection_profiled(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: &Path,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
    ) -> Result<Self> {
        Self::open_private_compact_fp32_projection_profiled(
            profile,
            source,
            profiling_prefix,
            reviewed_graph_result,
            reviewed_graph_sha,
            REVIEWED_QKVO.context("reviewed Compact Q/K/V/O load receipt unavailable")?,
        )
    }

    fn open_private_compact_fp32_projection_profiled(
        profile: ExecutionProfile,
        source: &dyn AssetSource,
        profiling_prefix: &Path,
        reviewed_graph_result: &[u8],
        reviewed_graph_sha: Sha256Digest,
        review: ReviewedProjection,
    ) -> Result<Self> {
        require_qkv_projection_profile(&profile, review.graph_sha.parse()?)?;
        let binding = profile
            .gpu
            .as_ref()
            .context("Compact QKV projection CUDA binding missing")?;
        ensure!(
            (
                binding.plan.cold_peak_bytes(),
                binding.plan.warm_extra_bytes(),
                binding.plan.headroom_bytes(),
            ) == (16 << 30, 8 << 30, 2 << 30),
            "Compact QKV projection CUDA reservation differs"
        );
        let deadline = binding.load_deadline;
        let gate = reviewed_projection_gate(reviewed_graph_result, reviewed_graph_sha, review)?;
        gate.verify_profile(profile.description(), binding.device.observation(deadline)?)?;
        let scorer = Self::open_internal(
            profile,
            source,
            |builder| Ok(qkv_projection_options(builder)?.with_profiling(profiling_prefix)?),
            OpenMode::BoundedHybrid(Box::new(gate)),
        )?
        .context("Compact QKV projection scorer missing")?;
        scorer.compact_fp16_bounded_hybrid_attention_plan()?;
        cuda::in_time(deadline)?;
        Ok(scorer)
    }

    /// Scores the original independent T32 input through the reviewed QKV projection experiment.
    pub fn score_private_compact_fp32_qkv_projection_t32(
        &self,
        input: &CompiledInput,
        deadline: Instant,
    ) -> Result<Vec<f32>> {
        self.score_private_compact_fp32_projection_t32(input, deadline, REVIEWED_QKV)
    }

    pub fn score_private_compact_fp32_qkvo_projection_t32(
        &self,
        input: &CompiledInput,
        deadline: Instant,
    ) -> Result<Vec<f32>> {
        self.score_private_compact_fp32_projection_t32(
            input,
            deadline,
            REVIEWED_QKVO.context("reviewed Compact Q/K/V/O load receipt unavailable")?,
        )
    }

    fn score_private_compact_fp32_projection_t32(
        &self,
        input: &CompiledInput,
        deadline: Instant,
        review: ReviewedProjection,
    ) -> Result<Vec<f32>> {
        require_qkv_projection_profile(&self.profile, review.graph_sha.parse()?)?;
        let binding = self
            .profile
            .gpu
            .as_ref()
            .context("Compact QKV projection CUDA binding missing")?;
        ensure!(
            self.bounded_hybrid
                .as_ref()
                .is_some_and(|evidence| matches!(evidence.policy, GraphPolicy::CompactFp16))
                && deadline <= binding.load_deadline,
            "reviewed Compact QKV projection scorer and original deadline required"
        );
        validate_input(input)?;
        self.score_with_deadline(input, deadline)
    }
}

pub(super) fn validate_input(input: &CompiledInput) -> Result<()> {
    ensure!(
        input.input_digest == T32_INPUT.parse()?
            && input.prompt.is_empty()
            && input.input_ids.iter().copied().eq(1..=32)
            && input.attention_mask == [1; 32]
            && input.position_ids.iter().copied().eq(0..32)
            && input.decision_position == 31
            && input.candidate_positions.iter().copied().eq(32..40)
            && input.option_mask == [true; 8],
        "exact Compact FP16 diagnostic T32 input required"
    );
    Ok(())
}

fn varint(input: &mut &[u8]) -> Result<u64> {
    let mut value = 0;
    for shift in (0..70).step_by(7) {
        let (&byte, rest) = input
            .split_first()
            .context("truncated tap protobuf integer")?;
        *input = rest;
        ensure!(shift != 63 || byte <= 1, "overflowing tap protobuf integer");
        value |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    anyhow::bail!("overflowing tap protobuf integer")
}

fn take<'a>(input: &mut &'a [u8], size: usize) -> Result<&'a [u8]> {
    let (prefix, rest) = input
        .split_at_checked(size)
        .context("truncated tap protobuf field")?;
    *input = rest;
    Ok(prefix)
}

fn field<'a>(input: &mut &'a [u8]) -> Result<(u64, Option<&'a [u8]>)> {
    let tag = varint(input)?;
    let payload = match tag & 7 {
        0 => {
            varint(input)?;
            None
        }
        1 => {
            take(input, 8)?;
            None
        }
        2 => {
            let size = usize::try_from(varint(input)?)?;
            Some(take(input, size)?)
        }
        5 => {
            take(input, 4)?;
            None
        }
        _ => anyhow::bail!("unsupported tap protobuf wire type"),
    };
    Ok((tag >> 3, payload))
}

fn write_varint(target: &mut Vec<u8>, mut value: u64) {
    while value >= 128 {
        target.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    target.push(value as u8);
}

fn write_message(target: &mut Vec<u8>, number: u64, body: &[u8]) {
    write_varint(target, number * 8 + 2);
    write_varint(target, body.len() as u64);
    target.extend_from_slice(body);
}

fn write_output(graph: &mut Vec<u8>, name: &str, width: usize) {
    let mut shape = Vec::new();
    // Keep the admitted graph's partial shape. A fixed token count in the
    // declaration allows ORT to fold unrelated downstream shape operations.
    if name.starts_with("/airk_g5/weight/") {
        write_message(&mut shape, 1, &[8, 1]); // batch dim_value
        let mut tokens = Vec::new();
        write_message(&mut tokens, 2, b"tokens"); // dim_param
        write_message(&mut shape, 1, &tokens);
    } else {
        write_message(&mut shape, 1, &[]);
        write_message(&mut shape, 1, &[]);
    }
    let mut value = vec![8]; // TensorShapeProto.Dimension.dim_value
    write_varint(&mut value, width as u64);
    write_message(&mut shape, 1, &value);
    let mut tensor_type = vec![8, 10]; // TypeProto.Tensor.elem_type = FLOAT16
    write_message(&mut tensor_type, 2, &shape);
    let mut value_type = Vec::new();
    write_message(&mut value_type, 1, &tensor_type);
    let mut info = Vec::new();
    write_message(&mut info, 1, name.as_bytes());
    write_message(&mut info, 2, &value_type);
    write_message(graph, 12, &info);
}

fn validate_base_graph(mut graph: &[u8], cut: Layer0Cut) -> Result<()> {
    let taps = cut.taps();
    let mut producers = vec![0; taps.len()];
    let mut outputs = 0;
    while !graph.is_empty() {
        match field(&mut graph)? {
            (1, Some(mut node)) => {
                while !node.is_empty() {
                    if let (2, Some(name)) = field(&mut node)?
                        && let Some(index) = taps.iter().position(|(tap, _)| name == tap.as_bytes())
                    {
                        producers[index] += 1;
                    }
                }
            }
            (12, Some(mut output)) => {
                outputs += 1;
                let mut names = 0;
                while !output.is_empty() {
                    if let (1, Some(name)) = field(&mut output)? {
                        ensure!(name == b"scores", "layer-0 base graph output differs");
                        names += 1;
                    }
                }
                ensure!(
                    names == 1,
                    "layer-0 base graph output name missing or repeated"
                );
            }
            _ => {}
        }
    }
    ensure!(
        outputs == 1 && producers.into_iter().all(|count| count == 1),
        "layer-0 requires one existing producer per tap and only the base scores output"
    );
    Ok(())
}

// Accept only after the original model digest and normal G5 audit pass.
pub(super) fn promote_model(model: &[u8], cut: Layer0Cut) -> Result<Vec<u8>> {
    let mut rest = model;
    let mut graph = None;
    while !rest.is_empty() {
        let start = model.len() - rest.len();
        if let (7, Some(payload)) = field(&mut rest)? {
            let end = model.len() - rest.len();
            ensure!(
                graph.replace((start, end, payload)).is_none(),
                "duplicate layer-0 base graph"
            );
        }
    }
    let (start, end, original) = graph.context("layer-0 base graph missing")?;
    validate_base_graph(original, cut)?;
    let mut graph = original.to_vec();
    for &(name, width) in cut.taps() {
        write_output(&mut graph, name, width);
    }
    let mut derived = Vec::with_capacity(model.len() + graph.len() - original.len() + 2);
    derived.extend_from_slice(&model[..start]);
    write_message(&mut derived, 7, &graph);
    derived.extend_from_slice(&model[end..]);
    ensure!(
        derived.len() <= 64 << 20,
        "layer-0 derived graph exceeds native cap"
    );
    Ok(derived)
}

pub(super) fn validate_output_abi(session: &Session, cut: Layer0Cut) -> Result<()> {
    ensure!(
        session.outputs.len() == 1 + cut.taps().len(),
        "layer-0 diagnostic output count differs"
    );
    for &(name, width) in cut.taps() {
        let batch = if name.starts_with("/airk_g5/weight/") {
            1
        } else {
            -1
        };
        ensure!(
            session.outputs.iter().any(|output| output.name == name
                && matches!(&output.output_type, ValueType::Tensor { ty, shape, .. }
                    if *ty == ort::tensor::TensorElementType::Float16
                        && shape.as_ref() == [batch, -1, width as i64])),
            "layer-0 diagnostic output ABI differs: {name}"
        );
    }
    Ok(())
}

type Operator<'a> = (u64, &'a str, &'a str, &'a str, &'a str, i64);

fn reviewed_operator(row: &Value) -> Result<Operator<'_>> {
    Ok((
        row["graph"]
            .as_u64()
            .context("reviewed operator graph missing")?,
        row["name"]
            .as_str()
            .context("reviewed operator name missing")?,
        row["domain"]
            .as_str()
            .context("reviewed operator domain missing")?,
        row["op"]
            .as_str()
            .context("reviewed operator type missing")?,
        row["ep"]
            .as_str()
            .context("reviewed operator provider missing")?,
        row["version"]
            .as_i64()
            .context("reviewed operator version missing")?,
    ))
}

pub(super) fn validate_graph(
    rows: &[GraphMetadataRow],
    base: &Value,
    cut: Layer0Cut,
) -> Result<()> {
    use GraphMetadataRow::*;
    let mut base_ops = base
        .as_array()
        .context("reviewed base graph rows missing")?
        .iter()
        .filter(|row| row["kind"] == "node")
        .map(reviewed_operator)
        .collect::<Result<Vec<_>>>()?;
    let mut live_ops = Vec::new();
    let mut cuda_nodes = BTreeSet::new();
    let mut gqa = None;
    let mut projections = [None; PROJECTION_NODES.len()];
    for row in rows {
        if let Node {
            graph,
            node,
            name,
            domain,
            op,
            ep,
            version,
        } = row
        {
            live_ops.push((
                *graph,
                name.as_str(),
                domain.as_str(),
                op.as_str(),
                ep.as_str(),
                *version,
            ));
            if *graph == 0 && ep == CUDA {
                cuda_nodes.insert(*node);
                if name == GQA && op == "GroupQueryAttention" && domain == "com.microsoft" {
                    gqa = Some(*node);
                }
                if op == "MatMul"
                    && domain.is_empty()
                    && let Some(index) = PROJECTION_NODES
                        .iter()
                        .position(|expected| name.as_str() == *expected)
                {
                    projections[index] = Some(*node);
                }
            }
        }
    }
    base_ops.sort_unstable();
    live_ops.sort_unstable();
    ensure!(
        live_ops == base_ops,
        "layer-0 optimized operator placement differs from reviewed base"
    );
    let gqa = gqa.context("layer-0 CUDA GroupQueryAttention missing")?;
    for (index, &(tap, _)) in cut.taps().iter().enumerate() {
        let value = rows
            .iter()
            .find_map(|row| match row {
                Value {
                    graph: 0,
                    value,
                    name,
                    element_type: Some(10),
                    rank: Some(3),
                    producer: Some(producer),
                    ..
                } if name == tap && cuda_nodes.contains(producer) => Some(*value),
                _ => None,
            })
            .context("layer-0 half tap producer missing or not CUDA")?;
        let has_edge = |owner, role, slot| {
            rows.iter().any(|row| matches!(row,
                Edge { graph: 0, node, value: Some(edge_value), role: edge_role, slot: edge_slot }
                    if *node == owner && *edge_value == value && *edge_role == role && *edge_slot == slot
            ))
        };
        let assigned = match cut {
            Layer0Cut::Attention if index == 3 => has_edge(Some(gqa), 2, 0),
            Layer0Cut::Attention => has_edge(Some(gqa), 0, index as u64),
            Layer0Cut::Projection if index == 0 => projections
                .iter()
                .all(|&node| node.is_some() && has_edge(node, 0, 0)),
            Layer0Cut::Projection => {
                let node = projections[index - 1];
                node.is_some() && has_edge(node, 2, 0)
            }
        };
        ensure!(
            assigned
                && rows.iter().any(|row| matches!(row,
                    Edge { graph: 0, node: None, value: Some(edge_value), role: 4, .. }
                        if *edge_value == value
                )),
            "layer-0 tap does not preserve its exact CUDA boundary and diagnostic output: {tap}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer0_projection_outputs_retain_the_symbolic_token_axis() -> Result<()> {
        let expected = [
            "/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/input16",
            "/airk_g5/weight//scorer/layers.0/self_attn/k_proj/MatMul/output16",
            "/airk_g5/weight//scorer/layers.0/self_attn/v_proj/MatMul/output16",
            "/airk_g5/weight//scorer/layers.0/self_attn/q_proj/MatMul/output16",
        ];
        let mut graph = Vec::new();
        for name in expected {
            let mut node = Vec::new();
            write_message(&mut node, 2, name.as_bytes());
            write_message(&mut graph, 1, &node);
        }
        write_output(&mut graph, "scores", 8);
        let mut model = Vec::new();
        write_message(&mut model, 7, &graph);
        let derived = promote_model(&model, Layer0Cut::Projection)?;
        let (_, body) = field(&mut derived.as_slice())?;
        let mut added = body.unwrap().strip_prefix(graph.as_slice()).unwrap();
        for (name, width_varint) in expected.into_iter().zip([16, 8, 8, 16]) {
            let (number, body) = field(&mut added)?;
            assert_eq!(number, 12);
            let mut info = body.unwrap();
            assert_eq!(field(&mut info)?, (1, Some(name.as_bytes())));
            let (_, ty) = field(&mut info)?;
            let (_, body) = field(&mut ty.unwrap())?;
            let mut tensor = body.unwrap().strip_prefix(&[8, 10]).unwrap();
            // Literal encoded dimensions: 1, symbolic tokens, and known width.
            let mut dims = b"\x0a\x02\x08\x01\x0a\x08\x12\x06tokens\x0a\x03\x08\x80".to_vec();
            dims.push(width_varint);
            assert_eq!(field(&mut tensor)?, (2, Some(dims.as_slice())));
        }
        assert!(added.is_empty());
        Ok(())
    }

    #[test]
    fn layer0_outputs_append_to_the_existing_graph_only() -> Result<()> {
        let mut graph = Vec::new();
        for (tap, _) in COMPACT_FP16_LAYER0_TAPS {
            let mut node = Vec::new();
            write_message(&mut node, 2, tap.as_bytes());
            write_message(&mut graph, 1, &node);
        }
        write_output(&mut graph, "scores", 8);
        let mut model = vec![8, 10];
        write_message(&mut model, 7, &graph);
        write_message(&mut model, 2, b"literal producer");
        let original = model.clone();
        let derived = promote_model(&model, Layer0Cut::Attention)?;
        let mut fields = derived.as_slice();
        assert_eq!(field(&mut fields)?, (1, None));
        let (number, body) = field(&mut fields)?;
        assert_eq!(number, 7);
        let added = body.unwrap().strip_prefix(graph.as_slice()).unwrap();
        let mut new_outputs = added;
        let head16 = [10, 0, 10, 0, 10, 3, 8, 128, 16];
        let head8 = [10, 0, 10, 0, 10, 3, 8, 128, 8];
        let projection = [
            10, 2, 8, 1, 10, 8, 18, 6, b't', b'o', b'k', b'e', b'n', b's', 10, 3, 8, 128, 8,
        ];
        for ((name, _), expected_shape) in COMPACT_FP16_LAYER0_TAPS.iter().zip([
            head16.as_slice(),
            head8.as_slice(),
            projection.as_slice(),
            head16.as_slice(),
        ]) {
            let (number, body) = field(&mut new_outputs)?;
            assert_eq!(number, 12);
            let mut info = body.unwrap();
            assert_eq!(field(&mut info)?, (1, Some(name.as_bytes())));
            let (number, body) = field(&mut info)?;
            assert_eq!(number, 2);
            let mut value_type = body.unwrap();
            let (number, body) = field(&mut value_type)?;
            assert_eq!(number, 1);
            let mut tensor = body.unwrap().strip_prefix(&[8, 10]).unwrap();
            assert_eq!(field(&mut tensor)?, (2, Some(expected_shape)));
            assert!(tensor.is_empty() && value_type.is_empty() && info.is_empty());
        }
        assert!(new_outputs.is_empty());
        assert_eq!(
            field(&mut fields)?,
            (2, Some(b"literal producer".as_slice()))
        );
        assert!(fields.is_empty());
        assert_eq!(model, original);
        assert!(promote_model(&derived, Layer0Cut::Attention).is_err());
        assert!(promote_model(&model, Layer0Cut::Projection).is_err());
        Ok(())
    }
}
