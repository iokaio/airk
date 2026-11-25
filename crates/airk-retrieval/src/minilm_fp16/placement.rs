// SPDX-License-Identifier: Apache-2.0
//! Exact native MiniLM placement, limited to reviewed integer shape controls.
use crate::arbiter::runtime::cuda::{PlacedNode, PlacementObservation};
use airk_core::identity::{Sha256Digest, framed_digest};
use anyhow::{Result, bail, ensure};
use std::collections::BTreeSet;

pub(super) const REVIEWED_GRAPH_SHA256: &str =
    "8d6ee9ccbeeb7a2c9a5dca313cb7b63ba7eb860fdfeab00bed7cc2fb3056bcb4";

// Exact A6000 native final placement under the authenticated static core.
// Source ONNX type inference matched each of these root-standard-ONNX nodes.
// Their 158 input/output slots contain 156 INT64 and two BOOL tensors, all
// scalars or shape vectors of at most four elements. See CONTRACT.md.
const CPU_CONTROLS: &[(&str, &str)] = &[
    ("/inner/embeddings/Gather", "Gather"),
    ("/inner/embeddings/Unsqueeze", "Unsqueeze"),
    ("/inner/Gather", "Gather"),
    ("/inner/Gather_1", "Gather"),
    ("/inner/Gather_2", "Gather"),
    ("/inner/Gather_3", "Gather"),
    ("/inner/Concat", "Concat"),
    ("/inner/Unsqueeze_11", "Unsqueeze"),
    ("/inner/Concat_1", "Concat"),
    ("/inner/Reshape_2", "Reshape"),
    ("/inner/Equal", "Equal"),
    ("/inner/Where", "Where"),
    ("/inner/encoder/layer.0/attention/self/Concat_1", "Concat"),
    (
        "/inner/encoder/layer.0/attention/self/Unsqueeze_6",
        "Unsqueeze",
    ),
    (
        "/inner/encoder/layer.0/attention/self/Unsqueeze_7",
        "Unsqueeze",
    ),
    ("/inner/encoder/layer.0/attention/self/Concat_3", "Concat"),
    ("/inner/encoder/layer.1/attention/self/Gather", "Gather"),
    ("/inner/encoder/layer.1/attention/self/Gather_1", "Gather"),
    ("/inner/encoder/layer.1/attention/self/Concat_1", "Concat"),
    (
        "/inner/encoder/layer.1/attention/self/Unsqueeze_6",
        "Unsqueeze",
    ),
    (
        "/inner/encoder/layer.1/attention/self/Unsqueeze_7",
        "Unsqueeze",
    ),
    ("/inner/encoder/layer.1/attention/self/Concat_3", "Concat"),
    ("/inner/encoder/layer.2/attention/self/Gather", "Gather"),
    ("/inner/encoder/layer.2/attention/self/Gather_1", "Gather"),
    ("/inner/encoder/layer.2/attention/self/Concat_1", "Concat"),
    (
        "/inner/encoder/layer.2/attention/self/Unsqueeze_6",
        "Unsqueeze",
    ),
    (
        "/inner/encoder/layer.2/attention/self/Unsqueeze_7",
        "Unsqueeze",
    ),
    ("/inner/encoder/layer.2/attention/self/Concat_3", "Concat"),
    ("/inner/encoder/layer.3/attention/self/Gather", "Gather"),
    ("/inner/encoder/layer.3/attention/self/Gather_1", "Gather"),
    ("/inner/encoder/layer.3/attention/self/Concat_1", "Concat"),
    (
        "/inner/encoder/layer.3/attention/self/Unsqueeze_6",
        "Unsqueeze",
    ),
    (
        "/inner/encoder/layer.3/attention/self/Unsqueeze_7",
        "Unsqueeze",
    ),
    ("/inner/encoder/layer.3/attention/self/Concat_3", "Concat"),
    ("/inner/encoder/layer.4/attention/self/Gather", "Gather"),
    ("/inner/encoder/layer.4/attention/self/Gather_1", "Gather"),
    ("/inner/encoder/layer.4/attention/self/Concat_1", "Concat"),
    (
        "/inner/encoder/layer.4/attention/self/Unsqueeze_6",
        "Unsqueeze",
    ),
    (
        "/inner/encoder/layer.4/attention/self/Unsqueeze_7",
        "Unsqueeze",
    ),
    ("/inner/encoder/layer.4/attention/self/Concat_3", "Concat"),
    ("/inner/encoder/layer.5/attention/self/Gather", "Gather"),
    ("/inner/encoder/layer.5/attention/self/Gather_1", "Gather"),
    ("/inner/encoder/layer.5/attention/self/Concat_1", "Concat"),
    (
        "/inner/encoder/layer.5/attention/self/Unsqueeze_6",
        "Unsqueeze",
    ),
    (
        "/inner/encoder/layer.5/attention/self/Unsqueeze_7",
        "Unsqueeze",
    ),
    ("/inner/encoder/layer.5/attention/self/Concat_3", "Concat"),
];

fn reviewed_cpu_control(node: &PlacedNode) -> bool {
    node.subgraph_index == 0
        && node.domain.is_empty()
        && CPU_CONTROLS.contains(&(node.name.as_str(), node.op.as_str()))
}

pub(super) fn identity(placement: &PlacementObservation) -> Result<Sha256Digest> {
    ensure!(
        placement.complete
            && placement.scope == "onnxruntime-final-session-state-ep-placement"
            && placement.reported_nodes == placement.nodes.len() as u64
            && placement.shim_status == 0
            && placement.ort_error_code == 0,
        "incomplete MiniLM CUDA placement receipt"
    );
    let mut controls = BTreeSet::new();
    let mut cuda_nodes = 0usize;
    let mut reported_bytes = 0usize;
    for node in &placement.nodes {
        reported_bytes += node.name.len() + node.domain.len() + node.op.len() + node.ep.len();
        match node.ep.as_str() {
            "CUDAExecutionProvider" => cuda_nodes += 1,
            "CPUExecutionProvider" => ensure!(
                reviewed_cpu_control(node) && controls.insert(node.name.as_str()),
                "unexpected MiniLM CPU placement: {} ({}/{}) in subgraph {}",
                node.name,
                node.domain,
                node.op,
                node.subgraph_index,
            ),
            provider => bail!("unexpected MiniLM execution provider {provider}"),
        }
    }
    ensure!(
        cuda_nodes > 0
            && controls.len() == CPU_CONTROLS.len()
            && u64::try_from(reported_bytes)? == placement.reported_string_bytes,
        "MiniLM CUDA placement does not contain the complete reviewed shape-control set"
    );
    Ok(framed_digest(
        b"airk.minilm.cuda.placement/1",
        &[&serde_json::to_vec(placement)?],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn representative_cpu_node() -> PlacedNode {
        PlacedNode {
            subgraph_index: 0,
            name: "/inner/encoder/layer.2/attention/self/Concat_1".into(),
            domain: String::new(),
            op: "Concat".into(),
            ep: "CPUExecutionProvider".into(),
        }
    }

    #[test]
    fn reviewed_shape_control_requires_exact_node_operation_domain_and_subgraph() {
        let exact = representative_cpu_node();
        assert!(reviewed_cpu_control(&exact));
        for different in [
            PlacedNode {
                name: "/inner/encoder/layer.6/attention/self/Concat_1".into(),
                ..exact.clone()
            },
            PlacedNode {
                op: "MatMul".into(),
                ..exact.clone()
            },
            PlacedNode {
                domain: "com.microsoft".into(),
                ..exact.clone()
            },
            PlacedNode {
                subgraph_index: 1,
                ..exact.clone()
            },
        ] {
            assert!(!reviewed_cpu_control(&different));
        }
    }

    #[test]
    fn partial_cpu_assignment_is_refused_even_with_cuda_kernels() {
        let cpu = representative_cpu_node();
        let gpu = PlacedNode {
            name: "/inner/matmul".into(),
            domain: String::new(),
            op: "MatMul".into(),
            ep: "CUDAExecutionProvider".into(),
            subgraph_index: 0,
        };
        let reported_string_bytes = [&cpu, &gpu]
            .iter()
            .map(|node| {
                (node.name.len() + node.domain.len() + node.op.len() + node.ep.len()) as u64
            })
            .sum();
        let partial = PlacementObservation {
            scope: "onnxruntime-final-session-state-ep-placement",
            complete: true,
            shim_status: 0,
            ort_error_code: 0,
            reported_nodes: 2,
            reported_string_bytes,
            nodes: vec![cpu, gpu],
        };
        assert!(identity(&partial).is_err());
    }
}
