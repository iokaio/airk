// SPDX-License-Identifier: Apache-2.0
//! Physical CUDA placement required by the Large compensated attention graph.
use anyhow::{Context, Result, ensure};

pub(super) fn routes(p: &super::super::super::PlacementObservation) -> Result<()> {
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
