// SPDX-License-Identifier: Apache-2.0
//! Private pinned Compact/Large graph identities only. No scoring or qualification authority.
use super::*;

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    pub graph: ResourceFilePin,
    pub shape_proof: ResourceFilePin,
    pub owned_creation: ResourceFilePin,
    pub baseline_input: ResourceFilePin,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DenseInput {
    pub candidate: Input,
    pub prior: Input,
}

struct Family {
    layers: usize,
    width: usize,
    source: &'static str,
    initializers: usize,
    cores: usize,
    reservation: u64,
    profile_id: &'static str,
    resolved_field: &'static str,
    owner_schema: &'static str,
}
fn family(model: ModelChoice) -> Result<Family> {
    match model {
        ModelChoice::ContextQwenCompact => Ok(Family {
            layers: 28,
            width: 2048,
            source: "c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4",
            initializers: 310,
            cores: 4,
            reservation: 16u64 << 30,
            profile_id: "private-compact-fp32-gqa-diagnostic-v1",
            resolved_field: "all28_resolved",
            owner_schema: "airk.private.compact-graph-owner/1",
        }),
        ModelChoice::ContextQwenLarge => Ok(Family {
            layers: 36,
            width: 4096,
            source: "f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94",
            initializers: 398,
            cores: 16,
            reservation: 40u64 << 30,
            profile_id: "private-large-fp32-gqa-diagnostic-v1",
            resolved_field: "all36_resolved",
            owner_schema: "airk.private.large-graph-owner/1",
        }),
        _ => anyhow::bail!("only pinned Compact or Large GQA identity is admitted"),
    }
}
pub(super) fn profile_id(model: ModelChoice) -> Result<&'static str> {
    Ok(family(model)?.profile_id)
}

fn graph_evidence(proof: &Value, owner: &Value, input: &Input, model: ModelChoice) -> Result<()> {
    graph_evidence_mode(proof, owner, input, model, false)
}
fn selected_family(model: ModelChoice, dense: bool) -> Result<Family> {
    let mut selected = family(model)?;
    if dense {
        ensure!(
            model == ModelChoice::ContextQwenLarge,
            "dense candidate is Large only"
        );
        selected.profile_id = crate::arbiter::runtime::LARGE_DENSE_DIAGNOSTIC_PROFILE_ID;
        selected.owner_schema = "airk.private.large-dense-graph-owner/1";
    }
    Ok(selected)
}
pub(super) fn dense_profile_id() -> &'static str {
    crate::arbiter::runtime::LARGE_DENSE_DIAGNOSTIC_PROFILE_ID
}
fn graph_evidence_mode(
    proof: &Value,
    owner: &Value,
    input: &Input,
    model: ModelChoice,
    dense: bool,
) -> Result<()> {
    let selected = selected_family(model, dense)?;
    if dense {
        ensure!(
            proof["attention_mode"] == "large_explicit_mask_dense/1"
                && proof["causal_attribute"] == 0
                && proof["global_flash_setting"] == "0"
                && proof["rewrite_input_sha256"]
                    == "e4cd63d26c1e5b8bbfd28c65cb04f5ab3946f57e5511d2cd09e125cdf46c4aca"
                && proof["prior_shape_proof_sha256"]
                    == "6a9cc23d3d9172fab09fe370207361830f33003ec61bded61f7d3adddc41e42b"
                && proof["only_prior_graph_causal_attributes_changed"] == true
                && proof["original_bias_name"] == "/scorer/Where_output_0"
                && proof["original_bias_closure_sha256"]
                    == "8bf893388fce7d661bc12b012aa8fb35d25cf23c8c4d226209304f5622387f22"
                && proof["original_bias_closure_entries"] == 29
                && proof["original_bias_preserved_verbatim"] == true,
            "exact original explicit-mask dense source/mode proof required"
        );
    }
    ensure!(
        proof["new_graph_sha256"] == json!(input.graph.sha256)
            && proof["source_sha256"] == selected.source
            && proof["gqa_nodes"] == selected.layers
            && proof["original_scale_float32_bits"] == "f304b53d"
            && proof["original_initializers"] == selected.initializers
            && proof["original_nodes_preserved_verbatim"] == true
            && proof["original_initializers_and_external_ranges_preserved_verbatim"] == true
            && proof["all6inputs_scores_only"] == true
            && proof["cache_outputs_internal_only"] == true
            && proof["model_forward"] == false
            && proof["custom_shape_gate_pending"] == false
            && proof["native_kernel_gate_pending"] == true
            && proof["onnx_checker"]
                == "original and candidate; real original external-data context; custom kernel ownership separate",
        "actual graph/checker/source proof differs"
    );
    if model == ModelChoice::ContextQwenLarge {
        ensure!(
            proof["original_external_initializers"] == 326
                && proof["original_scores_output_preserved_verbatim"] == true,
            "exact Large external-weight/readout preservation required"
        );
    }
    let cases = proof["custom_shape_witnesses"]
        .as_array()
        .context("strict shape cases absent")?;
    ensure!(
        cases.len() == 4,
        "all four original strict shape cases required"
    );
    for (case, (tokens, options)) in cases.iter().zip([(1, 1), (17, 3), (128, 8), (8192, 8)]) {
        ensure!(
            case["tokens"] == tokens
                && case["options"] == options
                && case[selected.resolved_field] == true
                && case["external_weight_value_policy"]["external_required_values_refused"] == 0,
            "strict shape case failed or changed"
        );
        let rows = case["witnesses"]
            .as_array()
            .context("shape witnesses absent")?;
        ensure!(
            rows.len() == selected.layers,
            "complete selected-layer shape witnesses required"
        );
        for row in rows {
            ensure!(
                row["context"] == json!([1, tokens, selected.width])
                    && row["cache_shapes_resolved_by_pinned_cpu_compute"]
                        == json!([[1, 8, tokens, 128], [1, 8, tokens, 128]]),
                "actual strict context/cache dimensions differ"
            );
        }
    }
    ensure!(
        owner["schema"] == selected.owner_schema
            && owner["complete"] == true
            && owner["failure"].is_null()
            && owner["graph_only"] == true
            && owner["model_forward"] == false
            && owner["original_phase_expired"] == false
            && owner["post_input_drift"] == json!([])
            && owner["graph"]["path"] == input.graph.path
            && owner["graph"]["sha256"] == json!(input.graph.sha256)
            && owner["shape_proof"]["path"] == input.shape_proof.path
            && owner["shape_proof"]["sha256"] == json!(input.shape_proof.sha256),
        "authentic complete graph creation binding absent"
    );
    let owned = &owner["owned"];
    ensure!(
        owned["exit_code"] == 0
            && owned["timed_out"] == false
            && owned["log_oversized"] == false
            && owned["owned_descendants_settled"] == true
            && owned["owned_job_forced_termination"] == false
            && owned["owned_job_final_active_processes"] == 0
            && owned["owned_job_final_member_pids"] == json!([])
            && owned["parent_assigned_to_job"] == false
            && owned["child_assigned_before_resume"] == true,
        "actual graph creator did not naturally settle"
    );
    Ok(())
}
fn baseline(input: &Input, model: ModelChoice) -> Result<(ResourceRunInput, Profile)> {
    let selected = family(model)?;
    let request: ResourceRunInput =
        serde_json::from_slice(&pinned_metadata(&input.baseline_input, 1 << 20)?)?;
    request.validate()?;
    ensure!(
        request.policy == ResourcePolicy::software_v2(),
        "unchanged original software-v2 policy required"
    );
    let profile: Profile = serde_json::from_slice(&pinned_metadata(&request.profile, 1 << 20)?)?;
    profile.validate()?;
    let fit = profile.cpu_fit.as_ref().context("original CpuFit absent")?;
    ensure!(
        profile.model == model
            && profile.max_tokens == 8192
            && profile.max_candidates == 8
            && profile.memory_reservation_bytes == selected.reservation
            && fit.minimum_physical_cores == selected.cores
            && fit.intra_threads == selected.cores
            && fit.inter_threads == 1
            && fit.sequential
            && !fit.spinning,
        "unchanged selected-model8192/CPU/memory baseline required"
    );
    Ok((request, profile))
}
pub(super) fn apply(input: &Input, assets: &mut [AssetInput], model: ModelChoice) -> Result<()> {
    apply_mode(input, assets, model, false)
}
fn apply_mode(
    input: &Input,
    assets: &mut [AssetInput],
    model: ModelChoice,
    dense: bool,
) -> Result<()> {
    let selected = selected_family(model, dense)?;
    let (request, old) = baseline(input, model)?;
    let graph = pinned_metadata(&input.graph, 64 << 20)?;
    let proof: Value = serde_json::from_slice(&pinned_metadata(&input.shape_proof, 1 << 20)?)?;
    let owner: Value = serde_json::from_slice(&pinned_metadata(&input.owned_creation, 1 << 20)?)?;
    graph_evidence_mode(&proof, &owner, input, model, dense)?;
    ensure!(
        proof["bytes"] == graph.len() && owner["graph"]["bytes"] == graph.len(),
        "actual graph length differs"
    );
    for item in assets {
        let previous = old
            .assets
            .iter()
            .find(|a| a.name == item.asset.name)
            .context("baseline logical asset absent")?;
        ensure!(
            &item.asset == previous
                && request.files.get(&item.asset.name).map(Path::new) == Some(item.path.as_path()),
            "original Asset identity or physical weight/tokenizer/fixture path changed"
        );
        if item.asset.role == AssetRole::Model {
            ensure!(
                item.asset.sha256.to_string() == selected.source
                    && input.graph.sha256 != item.asset.sha256,
                "exact selected original/new graph identity required"
            );
            item.asset.sha256 = input.graph.sha256;
            item.asset.bytes = graph.len() as u64;
            item.path = PathBuf::from(&input.graph.path);
        }
    }
    Ok(())
}
pub(super) fn validate_prepared(input: &Input, prepared: &PreparedProfile) -> Result<()> {
    validate_prepared_mode(input, prepared, false)
}
fn validate_prepared_mode(input: &Input, prepared: &PreparedProfile, dense: bool) -> Result<()> {
    let selected = selected_family(prepared.profile.model, dense)?;
    let (request, old) = baseline(input, prepared.profile.model)?;
    let new = &prepared.profile;
    ensure!(
        new.profile_id == selected.profile_id
            && new.model == old.model
            && new.manifest != old.manifest
            && new.compiler == old.compiler
            && new.policy == old.policy
            && new.task_policies == old.task_policies
            && new.max_tokens == old.max_tokens
            && new.max_candidates == old.max_candidates
            && new.abi == old.abi
            && new.cpu_fit == old.cpu_fit
            && new.memory_reservation_bytes == old.memory_reservation_bytes
            && new.peak_disk_bytes == old.peak_disk_bytes
            && new.calibration != old.calibration,
        "new graph identity must preserve original compiler/ABI/resources and rebind calibration"
    );
    for asset in &old.assets {
        let current = new
            .assets
            .iter()
            .find(|a| a.name == asset.name)
            .context("original asset absent")?;
        if asset.role == AssetRole::Model {
            ensure!(
                current.sha256 == input.graph.sha256
                    && prepared.files[&asset.name].as_path() == Path::new(&input.graph.path),
                "new model pin absent"
            );
        } else if asset.role == AssetRole::Compiler {
            ensure!(current == asset, "native8192 compiler bytes changed");
        } else if asset.role != AssetRole::Calibration {
            ensure!(
                current == asset
                    && prepared.files[&asset.name].as_path()
                        == Path::new(&request.files[&asset.name]),
                "unchanged asset/path differs"
            );
        }
    }
    Ok(())
}

#[test]
fn graph_proof_refuses_missing_cases_changed_source_or_unsettled_creator() -> Result<()> {
    let input = Input {
        graph: ResourceFilePin {
            path: "C:/private/model.onnx".into(),
            sha256: Sha256Digest::from_bytes([1; 32]),
        },
        shape_proof: ResourceFilePin {
            path: "C:/private/proof.json".into(),
            sha256: Sha256Digest::from_bytes([2; 32]),
        },
        owned_creation: ResourceFilePin {
            path: "C:/private/owner.json".into(),
            sha256: Sha256Digest::from_bytes([3; 32]),
        },
        baseline_input: ResourceFilePin {
            path: "C:/private/input.json".into(),
            sha256: Sha256Digest::from_bytes([4; 32]),
        },
    };
    let cases = [(1,1),(17,3),(128,8),(8192,8)].into_iter().map(|(t,n)| json!({
        "tokens":t,"options":n,"all28_resolved":true,
        "external_weight_value_policy":{"external_required_values_refused":0},
        "witnesses":(0..28).map(|_|json!({"context":[1,t,2048],
            "cache_shapes_resolved_by_pinned_cpu_compute":[[1,8,t,128],[1,8,t,128]]})).collect::<Vec<_>>()
    })).collect::<Vec<_>>();
    let proof = json!({"new_graph_sha256":input.graph.sha256,
        "source_sha256":"c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4",
        "gqa_nodes":28,"original_scale_float32_bits":"f304b53d","original_initializers":310,
        "original_nodes_preserved_verbatim":true,"original_initializers_and_external_ranges_preserved_verbatim":true,
        "all6inputs_scores_only":true,"cache_outputs_internal_only":true,"model_forward":false,
        "custom_shape_gate_pending":false,"native_kernel_gate_pending":true,
        "onnx_checker":"original and candidate; real original external-data context; custom kernel ownership separate",
        "custom_shape_witnesses":cases});
    let owner = json!({"schema":"airk.private.compact-graph-owner/1","complete":true,"failure":null,
        "graph_only":true,"model_forward":false,"original_phase_expired":false,"post_input_drift":[],
        "graph":{"path":input.graph.path,"sha256":input.graph.sha256},
        "shape_proof":{"path":input.shape_proof.path,"sha256":input.shape_proof.sha256},
        "owned":{"exit_code":0,"timed_out":false,"log_oversized":false,"owned_descendants_settled":true,
            "owned_job_forced_termination":false,"owned_job_final_active_processes":0,
            "owned_job_final_member_pids":[],"parent_assigned_to_job":false,"child_assigned_before_resume":true}});
    graph_evidence(&proof, &owner, &input, ModelChoice::ContextQwenCompact)?;
    for (path, value) in [
        ("/source_sha256", json!("changed")),
        ("/custom_shape_witnesses", json!([])),
        ("/custom_shape_witnesses/3/tokens", json!(2048)),
        (
            "/custom_shape_witnesses/3/witnesses/0/context",
            json!([1, 2048, 2048]),
        ),
        (
            "/custom_shape_witnesses/3/external_weight_value_policy/external_required_values_refused",
            json!(1),
        ),
        ("/all6inputs_scores_only", json!(false)),
        ("/model_forward", json!(true)),
    ] {
        let mut changed = proof.clone();
        *changed
            .pointer_mut(path)
            .context("literal proof mutation")? = value;
        assert!(graph_evidence(&changed, &owner, &input, ModelChoice::ContextQwenCompact).is_err());
    }
    for (path, value) in [
        ("/complete", json!(false)),
        ("/owned/exit_code", json!(1)),
        ("/owned/owned_job_forced_termination", json!(true)),
        ("/owned/owned_job_final_member_pids", json!([1])),
        ("/graph/sha256", json!(Sha256Digest::from_bytes([9; 32]))),
        ("/post_input_drift", json!(["changed"])),
    ] {
        let mut changed = owner.clone();
        *changed
            .pointer_mut(path)
            .context("literal owner mutation")? = value;
        assert!(graph_evidence(&proof, &changed, &input, ModelChoice::ContextQwenCompact).is_err());
    }
    Ok(())
}

#[test]
fn large_graph_proof_refuses_compact_shape_source_or_unsettled_creator() -> Result<()> {
    let input = Input {
        graph: ResourceFilePin {
            path: "C:/private/model.onnx".into(),
            sha256: Sha256Digest::from_bytes([1; 32]),
        },
        shape_proof: ResourceFilePin {
            path: "C:/private/proof.json".into(),
            sha256: Sha256Digest::from_bytes([2; 32]),
        },
        owned_creation: ResourceFilePin {
            path: "C:/private/owner.json".into(),
            sha256: Sha256Digest::from_bytes([3; 32]),
        },
        baseline_input: ResourceFilePin {
            path: "C:/private/input.json".into(),
            sha256: Sha256Digest::from_bytes([4; 32]),
        },
    };
    let cases = [(1,1),(17,3),(128,8),(8192,8)].into_iter().map(|(t,n)| json!({
        "tokens":t,"options":n,"all36_resolved":true,
        "external_weight_value_policy":{"external_required_values_refused":0},
        "witnesses":(0..36).map(|_|json!({"context":[1,t,4096],
            "cache_shapes_resolved_by_pinned_cpu_compute":[[1,8,t,128],[1,8,t,128]]})).collect::<Vec<_>>()
    })).collect::<Vec<_>>();
    let proof = json!({"new_graph_sha256":input.graph.sha256,
        "source_sha256":"f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94",
        "gqa_nodes":36,"original_scale_float32_bits":"f304b53d","original_initializers":398,
        "original_external_initializers":326,"original_scores_output_preserved_verbatim":true,
        "original_nodes_preserved_verbatim":true,"original_initializers_and_external_ranges_preserved_verbatim":true,
        "all6inputs_scores_only":true,"cache_outputs_internal_only":true,"model_forward":false,
        "custom_shape_gate_pending":false,"native_kernel_gate_pending":true,
        "onnx_checker":"original and candidate; real original external-data context; custom kernel ownership separate",
        "custom_shape_witnesses":cases});
    let owner = json!({"schema":"airk.private.large-graph-owner/1","complete":true,"failure":null,
        "graph_only":true,"model_forward":false,"original_phase_expired":false,"post_input_drift":[],
        "graph":{"path":input.graph.path,"sha256":input.graph.sha256},
        "shape_proof":{"path":input.shape_proof.path,"sha256":input.shape_proof.sha256},
        "owned":{"exit_code":0,"timed_out":false,"log_oversized":false,"owned_descendants_settled":true,
            "owned_job_forced_termination":false,"owned_job_final_active_processes":0,
            "owned_job_final_member_pids":[],"parent_assigned_to_job":false,"child_assigned_before_resume":true}});
    graph_evidence(&proof, &owner, &input, ModelChoice::ContextQwenLarge)?;
    for (path, value) in [
        (
            "/source_sha256",
            json!("c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4"),
        ),
        ("/gqa_nodes", json!(28)),
        ("/original_external_initializers", json!(254)),
        ("/original_scores_output_preserved_verbatim", json!(false)),
        ("/original_initializers", json!(310)),
        ("/custom_shape_witnesses", json!([])),
        ("/custom_shape_witnesses/3/tokens", json!(2048)),
        (
            "/custom_shape_witnesses/3/witnesses/0/context",
            json!([1, 2048, 2048]),
        ),
        (
            "/custom_shape_witnesses/3/external_weight_value_policy/external_required_values_refused",
            json!(1),
        ),
        ("/all6inputs_scores_only", json!(false)),
        ("/model_forward", json!(true)),
    ] {
        let mut changed = proof.clone();
        *changed
            .pointer_mut(path)
            .context("literal proof mutation")? = value;
        assert!(graph_evidence(&changed, &owner, &input, ModelChoice::ContextQwenLarge).is_err());
    }
    for (path, value) in [
        ("/complete", json!(false)),
        ("/owned/exit_code", json!(1)),
        ("/owned/owned_job_forced_termination", json!(true)),
        ("/owned/owned_job_final_member_pids", json!([1])),
        ("/graph/sha256", json!(Sha256Digest::from_bytes([9; 32]))),
        ("/post_input_drift", json!(["changed"])),
    ] {
        let mut changed = owner.clone();
        *changed
            .pointer_mut(path)
            .context("literal owner mutation")? = value;
        assert!(graph_evidence(&proof, &changed, &input, ModelChoice::ContextQwenLarge).is_err());
    }
    Ok(())
}

#[test]
fn selected_graph_families_preserve_literal_cpu_memory_width_and_source() -> Result<()> {
    let compact = family(ModelChoice::ContextQwenCompact)?;
    assert_eq!(
        (
            compact.layers,
            compact.width,
            compact.cores,
            compact.reservation
        ),
        (28, 2048, 4, 17_179_869_184)
    );
    let large = family(ModelChoice::ContextQwenLarge)?;
    assert_eq!(
        (large.layers, large.width, large.cores, large.reservation),
        (36, 4096, 16, 42_949_672_960)
    );
    assert_eq!(large.initializers, 398);
    assert_eq!(
        large.source,
        "f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94"
    );
    assert!(family(ModelChoice::FocusQwenReranker06B).is_err());
    Ok(())
}

fn dense_source_identity(input: &DenseInput) -> Result<()> {
    ensure!(
        input.prior.graph.sha256.to_string()
            == "e4cd63d26c1e5b8bbfd28c65cb04f5ab3946f57e5511d2cd09e125cdf46c4aca"
            && input.prior.shape_proof.sha256.to_string()
                == "6a9cc23d3d9172fab09fe370207361830f33003ec61bded61f7d3adddc41e42b"
            && input.prior.owned_creation.sha256.to_string()
                == "2dcad61358578701be588d20b56787c0e169835d5329a5c760bdd662db2f857a"
            && input.candidate.graph.sha256 != input.prior.graph.sha256
            && input.candidate.graph.sha256.to_string()
                != "f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94"
            && input.candidate.baseline_input.path == input.prior.baseline_input.path
            && input.candidate.baseline_input.sha256 == input.prior.baseline_input.sha256,
        "dense prior actual graph/creator/baseline closure differs"
    );
    Ok(())
}
fn dense_prior(input: &DenseInput) -> Result<()> {
    dense_source_identity(input)?;
    let bytes = pinned_metadata(&input.prior.graph, 64 << 20)?;
    let proof: Value =
        serde_json::from_slice(&pinned_metadata(&input.prior.shape_proof, 1 << 20)?)?;
    let owner: Value =
        serde_json::from_slice(&pinned_metadata(&input.prior.owned_creation, 1 << 20)?)?;
    graph_evidence(&proof, &owner, &input.prior, ModelChoice::ContextQwenLarge)?;
    ensure!(proof["bytes"] == bytes.len(), "prior graph bytes changed");
    baseline(&input.prior, ModelChoice::ContextQwenLarge)?;
    Ok(())
}
pub(super) fn apply_dense(input: &DenseInput, assets: &mut [AssetInput]) -> Result<()> {
    dense_prior(input)?;
    apply_mode(
        &input.candidate,
        assets,
        ModelChoice::ContextQwenLarge,
        true,
    )
}
pub(super) fn validate_prepared_dense(
    input: &DenseInput,
    prepared: &PreparedProfile,
) -> Result<()> {
    dense_prior(input)?;
    validate_prepared_mode(&input.candidate, prepared, true)
}

#[test]
fn large_dense_proof_refuses_wrong_mode_mask_source_and_flash_identity() -> Result<()> {
    let input = Input {
        graph: ResourceFilePin {
            path: "C:/private/model.onnx".into(),
            sha256: Sha256Digest::from_bytes([1; 32]),
        },
        shape_proof: ResourceFilePin {
            path: "C:/private/proof.json".into(),
            sha256: Sha256Digest::from_bytes([2; 32]),
        },
        owned_creation: ResourceFilePin {
            path: "C:/private/owner.json".into(),
            sha256: Sha256Digest::from_bytes([3; 32]),
        },
        baseline_input: ResourceFilePin {
            path: "C:/private/input.json".into(),
            sha256: Sha256Digest::from_bytes([4; 32]),
        },
    };
    let cases = [(1,1),(17,3),(128,8),(8192,8)].into_iter().map(|(t,n)| json!({
        "tokens":t,"options":n,"all36_resolved":true,
        "external_weight_value_policy":{"external_required_values_refused":0},
        "witnesses":(0..36).map(|_|json!({"context":[1,t,4096],
            "cache_shapes_resolved_by_pinned_cpu_compute":[[1,8,t,128],[1,8,t,128]]})).collect::<Vec<_>>()
    })).collect::<Vec<_>>();
    let proof = json!({"new_graph_sha256":input.graph.sha256,
        "source_sha256":"f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94",
        "gqa_nodes":36,"original_scale_float32_bits":"f304b53d","original_initializers":398,
        "original_external_initializers":326,"original_scores_output_preserved_verbatim":true,
        "attention_mode":"large_explicit_mask_dense/1","causal_attribute":0,"global_flash_setting":"0",
        "rewrite_input_sha256":"e4cd63d26c1e5b8bbfd28c65cb04f5ab3946f57e5511d2cd09e125cdf46c4aca",
        "prior_shape_proof_sha256":"6a9cc23d3d9172fab09fe370207361830f33003ec61bded61f7d3adddc41e42b",
        "only_prior_graph_causal_attributes_changed":true,"original_bias_name":"/scorer/Where_output_0",
        "original_bias_closure_sha256":"8bf893388fce7d661bc12b012aa8fb35d25cf23c8c4d226209304f5622387f22",
        "original_bias_closure_entries":29,"original_bias_preserved_verbatim":true,
        "original_nodes_preserved_verbatim":true,"original_initializers_and_external_ranges_preserved_verbatim":true,
        "all6inputs_scores_only":true,"cache_outputs_internal_only":true,"model_forward":false,
        "custom_shape_gate_pending":false,"native_kernel_gate_pending":true,
        "onnx_checker":"original and candidate; real original external-data context; custom kernel ownership separate",
        "custom_shape_witnesses":cases});
    let owner = json!({"schema":"airk.private.large-dense-graph-owner/1","complete":true,"failure":null,
        "graph_only":true,"model_forward":false,"original_phase_expired":false,"post_input_drift":[],
        "graph":{"path":input.graph.path,"sha256":input.graph.sha256},
        "shape_proof":{"path":input.shape_proof.path,"sha256":input.shape_proof.sha256},
        "owned":{"exit_code":0,"timed_out":false,"log_oversized":false,"owned_descendants_settled":true,
            "owned_job_forced_termination":false,"owned_job_final_active_processes":0,
            "owned_job_final_member_pids":[],"parent_assigned_to_job":false,"child_assigned_before_resume":true}});
    graph_evidence_mode(&proof, &owner, &input, ModelChoice::ContextQwenLarge, true)?;
    assert!(graph_evidence(&proof, &owner, &input, ModelChoice::ContextQwenLarge).is_err());
    assert!(selected_family(ModelChoice::ContextQwenCompact, true).is_err());
    for (path, value) in [
        (
            "/source_sha256",
            json!("c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4"),
        ),
        ("/gqa_nodes", json!(28)),
        ("/causal_attribute", json!(1)),
        ("/global_flash_setting", json!("1")),
        ("/attention_mode", json!("flash")),
        ("/original_bias_name", json!("other")),
        ("/original_bias_closure_sha256", json!("changed")),
        ("/original_bias_preserved_verbatim", json!(false)),
        ("/only_prior_graph_causal_attributes_changed", json!(false)),
        ("/original_external_initializers", json!(254)),
        ("/original_scores_output_preserved_verbatim", json!(false)),
        ("/original_initializers", json!(310)),
        ("/custom_shape_witnesses", json!([])),
        ("/custom_shape_witnesses/3/tokens", json!(2048)),
        (
            "/custom_shape_witnesses/3/witnesses/0/context",
            json!([1, 2048, 2048]),
        ),
        (
            "/custom_shape_witnesses/3/external_weight_value_policy/external_required_values_refused",
            json!(1),
        ),
        ("/all6inputs_scores_only", json!(false)),
        ("/model_forward", json!(true)),
    ] {
        let mut changed = proof.clone();
        *changed
            .pointer_mut(path)
            .context("literal proof mutation")? = value;
        assert!(
            graph_evidence_mode(
                &changed,
                &owner,
                &input,
                ModelChoice::ContextQwenLarge,
                true
            )
            .is_err()
        );
    }
    for (path, value) in [
        ("/complete", json!(false)),
        ("/owned/exit_code", json!(1)),
        ("/owned/owned_job_forced_termination", json!(true)),
        ("/owned/owned_job_final_member_pids", json!([1])),
        ("/graph/sha256", json!(Sha256Digest::from_bytes([9; 32]))),
        ("/post_input_drift", json!(["changed"])),
    ] {
        let mut changed = owner.clone();
        *changed
            .pointer_mut(path)
            .context("literal owner mutation")? = value;
        assert!(
            graph_evidence_mode(
                &proof,
                &changed,
                &input,
                ModelChoice::ContextQwenLarge,
                true
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn dense_source_identity_refuses_retagged_flash_or_changed_prior() -> Result<()> {
    fn source(graph: Sha256Digest) -> Input {
        Input {
            graph: ResourceFilePin {
                path: "C:/private/graph".into(),
                sha256: graph,
            },
            shape_proof: ResourceFilePin {
                path: "C:/private/proof".into(),
                sha256: "6a9cc23d3d9172fab09fe370207361830f33003ec61bded61f7d3adddc41e42b"
                    .parse()
                    .unwrap(),
            },
            owned_creation: ResourceFilePin {
                path: "C:/private/creator".into(),
                sha256: "2dcad61358578701be588d20b56787c0e169835d5329a5c760bdd662db2f857a"
                    .parse()
                    .unwrap(),
            },
            baseline_input: ResourceFilePin {
                path: "C:/private/baseline".into(),
                sha256: Sha256Digest::from_bytes([4; 32]),
            },
        }
    }
    let prior: Sha256Digest =
        "e4cd63d26c1e5b8bbfd28c65cb04f5ab3946f57e5511d2cd09e125cdf46c4aca".parse()?;
    let mut input = DenseInput {
        candidate: source(Sha256Digest::from_bytes([5; 32])),
        prior: source(prior),
    };
    dense_source_identity(&input)?;
    input.candidate.graph.sha256 = prior;
    assert!(dense_source_identity(&input).is_err());
    input.candidate.graph.sha256 = Sha256Digest::from_bytes([5; 32]);
    input.candidate.baseline_input.path = "C:/different".into();
    assert!(dense_source_identity(&input).is_err());
    input.candidate.baseline_input.path = "C:/private/baseline".into();
    input.prior.owned_creation.sha256 = Sha256Digest::from_bytes([9; 32]);
    assert!(dense_source_identity(&input).is_err());
    Ok(())
}
