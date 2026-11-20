// SPDX-License-Identifier: Apache-2.0
//! Independently staged Compact MLP down projections with FP32 residual inputs.
use super::*;

pub(super) const SPEC: ProjectionSpec = ProjectionSpec {
    artifact: Some((
        "95e81ed4222c4cfdfc57e7e5f4600f135fe9bee9cbc7f7439b6111543476a737",
        "a583cbd0906a9801461ac0678d1607ecfe73398b637c19c97ea072c5750a33a1",
    )),
    stage_schema: "airk.private.compact-fp32-qkv-down-projection-candidate/1",
    source_schema: "airk.private.compact-fp32-qkv-down-source/1",
    metadata_schema: "airk.private.compact-fp32-qkv-down-probe-metadata/1",
    graph_schema: "airk.private.compact-fp32-qkv-down-graph-diagnostic/1",
    native_graph_sha: Some("295f0c79716cba2ec3da993e4a2659a7a67b6daec8bb473185c5818413d6daa4"),
    t32_schema: "airk.private.compact-fp32-qkv-down-t32/1",
    reference_schema: "airk.private.compact-mlp-down-projection-reference/1",
    reference_sha: Some("ffda904c92a2354ed9a4c17cf27ea8ce05452b56172c9d36c3a1c045a13ecf52"),
    recipe_routes: "routes",
    matching_scores: "all_down_fp32",
    projections: 112,
};

pub(super) fn stage_proof(fact: &Value) -> bool {
    let proof = &fact["graph_proof"];
    proof["prior_qkv_routes"] == 84
        && proof["new_input_and_weight_casts"] == 56
        && proof["removed_half_output_casts"] == 28
        && proof["source_graph_recovered_byte_for_byte"] == true
        && proof["retained_sidecar"] == true
        && fact["qkv_graph"]["sha256"] == QKV.artifact.expect("sealed Q/K/V").1
        && proof["mlp_down_routes"].as_array().is_some_and(|rows| {
            rows.len() == 28
                && rows.iter().enumerate().all(|(n, row)| {
                    row.as_str()
                        == Some(format!("/scorer/layers.{n}/mlp/down_proj/MatMul").as_str())
                })
        })
}

#[test]
#[ignore = "owned Compact FP32 down-projection metadata only"]
fn emit_down_metadata() -> Result<()> {
    emit_compact_fp16_probe_metadata(Some(&SPEC))
}

#[test]
#[ignore = "owned Compact FP32 down-projection final typed SessionState graph"]
fn observe_down_graph() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::DownGraph)
}

#[test]
#[ignore = "owned Compact protected MLP down scoring on an independent original admission"]
fn observe_down_scores() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::DownScores)
}

#[test]
fn down_stage_rejects_output_requantization_and_reordered_routes() {
    let mut receipt = json!({"qkv_graph":{"sha256":"e621cf6bffad1f0a27e3cc1ecba5b4379949295498f6585963620d71cfe4ff14"},
        "graph_proof":{"prior_qkv_routes":84, "new_input_and_weight_casts":56, "removed_half_output_casts":28,
        "source_graph_recovered_byte_for_byte":true, "retained_sidecar":true,
        "mlp_down_routes":(0..28).map(|n|format!("/scorer/layers.{n}/mlp/down_proj/MatMul")).collect::<Vec<_>>()}});
    assert!(stage_proof(&receipt));
    receipt["graph_proof"]["removed_half_output_casts"] = json!(0);
    assert!(!stage_proof(&receipt));
    receipt["graph_proof"]["removed_half_output_casts"] = json!(28);
    receipt["graph_proof"]["mlp_down_routes"][0] = json!("/scorer/layers.1/mlp/down_proj/MatMul");
    assert!(!stage_proof(&receipt));
}
