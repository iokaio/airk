// SPDX-License-Identifier: Apache-2.0
//! Private profiling diagnostic only; no all-row/model/task qualification.
use super::*;

fn allocated_gqa_profile(raw: &[u8], tokens: usize, required: usize) -> Result<()> {
    allocated_gqa_profile_for_model(raw, tokens, required, ModelChoice::ContextQwenCompact)
}

fn allocated_gqa_profile_for_model(
    raw: &[u8],
    tokens: usize,
    required: usize,
    model: ModelChoice,
) -> Result<()> {
    let selected = airk_retrieval::arbiter::runtime::selected_gqa_model(model)?;
    let events: Vec<Value> = serde_json::from_slice(raw)?;
    let mut nodes = BTreeSet::new();
    for event in events
        .iter()
        .filter(|event| event["args"]["op_name"] == "GroupQueryAttention")
    {
        let args = &event["args"];
        ensure!(
            args["provider"] == "CPUExecutionProvider",
            "GQA did not execute on the pinned CPU provider"
        );
        ensure!(
            event["dur"].as_u64().is_some_and(|value| value > 0),
            "actual GQA kernel duration absent"
        );
        let key = args["node_index"]
            .as_str()
            .context("native profiler node identity absent")?;
        ensure!(
            nodes.insert(key.to_owned()),
            "repeated GQA execution in single-forward diagnostic"
        );
        let expected = json!([{"float":[1,tokens,selected.query_width]},{"float":[1,selected.kv_heads,tokens,selected.head_size]},{"float":[1,selected.kv_heads,tokens,selected.head_size]}]);
        ensure!(
            args["output_type_shape"] == expected,
            "actual context/internal KV allocations absent or different"
        );
        let bytes = args["output_size"]
            .as_str()
            .context("native allocated output size absent")?
            .parse::<u64>()?;
        ensure!(
            bytes
                == (tokens as u64)
                    * (selected.query_width + 2 * selected.kv_heads * selected.head_size) as u64
                    * 4,
            "actual allocated GQA output bytes differ"
        );
    }
    ensure!(
        nodes.len() == required,
        "actual GQA kernel population incomplete"
    );
    Ok(())
}

#[test]
fn actual_allocated_gqa_profile_refuses_missing_cache_provider_repeat_or_shape() -> Result<()> {
    let event = json!({"dur":1,"args":{"op_name":"GroupQueryAttention","provider":"CPUExecutionProvider",
        "node_index":"17","output_size":"278528",
        "output_type_shape":[{"float":[1,17,2048]},{"float":[1,8,17,128]},{"float":[1,8,17,128]}]}});
    allocated_gqa_profile(&serde_json::to_vec(&vec![event.clone()])?, 17, 1)?;
    for (path, value) in [
        ("/args/provider", json!("OtherProvider")),
        ("/args/output_type_shape", json!([{"float":[1,17,2048]}])),
        ("/args/output_size", json!("1")),
        ("/dur", json!(0)),
    ] {
        let mut changed = event.clone();
        *changed.pointer_mut(path).context("literal mutation path")? = value;
        assert!(allocated_gqa_profile(&serde_json::to_vec(&vec![changed])?, 17, 1).is_err());
    }
    assert!(
        allocated_gqa_profile(&serde_json::to_vec(&vec![event.clone(), event])?, 17, 2).is_err()
    );
    assert!(allocated_gqa_profile(b"[]", 8192, 28).is_err());
    Ok(())
}

#[test]
#[ignore = "explicit new Compact graph/current input/source pins and exclusive native window; ONE diagnostic forward"]
fn software_v2_compact_attention_profiled_8192_diagnostic() -> Result<()> {
    attention_profiled_diagnostic(ModelChoice::ContextQwenCompact)
}

#[test]
#[ignore = "explicit new Large graph/current input/source pins and exclusive native window; ONE diagnostic forward"]
fn software_v2_large_attention_profiled_8192_diagnostic() -> Result<()> {
    attention_profiled_diagnostic(ModelChoice::ContextQwenLarge)
}

fn selected_diagnostic_profile(
    model: ModelChoice,
    max_tokens: usize,
    memory_bytes: u64,
    fit: Option<&airk_core::arbiter::CpuFit>,
    expected_model: ModelChoice,
) -> Result<airk_retrieval::arbiter::runtime::SelectedGqaModel> {
    let selected = airk_retrieval::arbiter::runtime::selected_gqa_model(expected_model)?;
    let fit = fit.context("native CPU fit absent")?;
    ensure!(
        model == expected_model
            && max_tokens == 8192
            && memory_bytes == selected.memory_reservation_bytes
            && fit.minimum_physical_cores == selected.cpu_threads
            && fit.intra_threads == selected.cpu_threads
            && fit.inter_threads == 1
            && fit.sequential
            && !fit.spinning,
        "exact selected model8192/native CPU/reservation diagnostic required"
    );
    Ok(selected)
}

use airk_retrieval::arbiter::runtime::LARGE_DENSE_DIAGNOSTIC_PROFILE_ID as DENSE_PROFILE_ID;

fn diagnostic_attention_identity(
    model: ModelChoice,
    profile_id: &str,
    dense: bool,
) -> Result<(&'static str, &'static str, &'static str)> {
    if dense {
        ensure!(
            model == ModelChoice::ContextQwenLarge && profile_id == DENSE_PROFILE_ID,
            "source-bound Large explicit-mask dense profile required; flash authority cannot transfer"
        );
        return Ok((
            "large-dense",
            "large_dense_attention_diagnostic",
            "large-fp32-explicit-mask-dense-profiled/1",
        ));
    }
    ensure!(
        profile_id != DENSE_PROFILE_ID,
        "dense profile cannot be relabeled as a flash diagnostic"
    );
    match model {
        ModelChoice::ContextQwenCompact => Ok((
            "compact",
            "compact_attention_diagnostic",
            "compact-fp32-gqa-profiled/2",
        )),
        ModelChoice::ContextQwenLarge => Ok((
            "large",
            "large_attention_diagnostic",
            "large-fp32-gqa-profiled/2",
        )),
        _ => anyhow::bail!("selected Context diagnostic required"),
    }
}
#[test]
#[ignore = "explicit new Large DENSE graph/current source/proof pins and exclusive40GiB/120s native window; ONE full8192/N8 diagnostic"]
fn software_v2_large_dense_attention_profiled_8192_diagnostic() -> Result<()> {
    attention_profiled_diagnostic_with_mode(ModelChoice::ContextQwenLarge, true)
}

fn attention_profiled_diagnostic(expected_model: ModelChoice) -> Result<()> {
    attention_profiled_diagnostic_with_mode(expected_model, false)
}
fn attention_profiled_diagnostic_with_mode(expected_model: ModelChoice, dense: bool) -> Result<()> {
    let (input, profile, files, input_hash) = inputs()?;
    let selected = selected_diagnostic_profile(
        profile.model,
        profile.max_tokens,
        profile.memory_reservation_bytes,
        profile.cpu_fit.as_ref(),
        expected_model,
    )?;
    let (label, kind, measurement) =
        diagnostic_attention_identity(expected_model, &profile.profile_id, dense)?;
    ensure!(
        std::env::var("ORT_GQA_DISABLE_FLASH_ATTENTION").as_deref() == Ok("0"),
        "explicit frozen native GQA flash policy required"
    );
    let fixture_pin = input
        .numeric_fixture
        .as_ref()
        .context("original exact numeric fixture required")?;
    let fixture: NumericFixture = serde_json::from_slice(&pinned(fixture_pin, 16 << 20)?)?;
    numeric_shape(&fixture, &profile, &input.policy)?;
    let row = fixture
        .rows
        .get(2)
        .context("full8192 original row absent")?;
    ensure!(
        row.input.input_ids.len() == 8192
            && row.input.attention_mask == vec![1; 8192]
            && row.input.candidate_positions.len() == 8,
        "unchanged full8192/N8 row required"
    );
    let reference = row
        .reference_original_fp32
        .as_ref()
        .context("genuine original publisher array absent")?;
    let output = PathBuf::from(std::env::var("AIRK_SOFTWARE_V2_NATIVE_OUTPUT")?);
    let parent = output.parent().context("diagnostic output parent absent")?;
    let profile_prefix = parent.join(format!("{label}-gqa-profile"));
    let optimized = parent.join(format!("{label}-gqa-optimized.onnx"));
    ensure!(!optimized.exists(), "fresh optimized graph output required");
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    let binary_sha256 = file_hash(&std::env::current_exe()?)?;
    let mut receipt = json!({"kind":kind,"complete":false,"evaluation":true,
        "release_qualified":false,"qualification_granted":false,"input_sha256":input_hash,
        "profile":profile.manifest,"profile_source_sha256":input.profile.sha256,"source_manifest":input.source_manifest,
        "resource_policy":input.policy,"resource_policy_digest":input.policy_digest,"model":profile.model,
        "cpu_fit":profile.cpu_fit,"target":current_target(),"isa":isa(),"process_id":std::process::id(),
        "execution_user":airk_ipc::endpoint::current_user()?,"native_binary_sha256":binary_sha256,
        "measurement_identity":"native-original-operation-start-to-post-end-retention/1",
        "diagnostic_measurement_identity":measurement,"profiling_enabled":true,
        "optimized_save":{"enabled":false,"scope":"ordinary production initializer retention; profiling events only; no full-model optimized snapshot"},
        "operation_journal":output.with_extension("operations.jsonl"),"rows":[],
        "reference_scope":"original-publisher numeric diagnostic only; old same-artifact custody is NOT new-graph proof"});
    if dense {
        receipt["attention_mode"] = json!("explicit_mask_dense");
    }
    flush(&mut file, &receipt)?;
    let mut operations = operation::Journal::create(
        &output,
        operation::Identity {
            input_sha256: input_hash,
            profile: profile.manifest,
            process_id: std::process::id(),
            native_binary_sha256: binary_sha256,
        },
        input.policy.deadline_ms,
    )?;
    let (_parent, control) = private_control()?;
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(Arc::new(
        airk_store::service::ServiceLease::acquire(&control)?,
    ))?;
    let result=owner.run(||->Result<()> {
        verify_files(&files,&profile)?;
        let loaded=operations.observe(operation::Kind::Load,0,None,||Scorer::open_profiled(
            ExecutionProfile::for_qualification(profile.clone())?,&files,&profile_prefix))?;
        receipt["load_us"]=json!(loaded.elapsed_us);
        receipt["load_peak_process_rss_bytes"]=json!(peak_rss());flush(&mut file,&receipt)?;
        ensure!(loaded.within_deadline,"original diagnostic load deadline exceeded");
        ensure!(peak_rss().is_some_and(|rss| rss > 0 && rss <= profile.memory_reservation_bytes),"known diagnostic load RSS refused before forward");
        let scorer=loaded.value?;
        let scored=operations.observe(operation::Kind::Numeric,3,Some(row.input.input_digest),||scorer.score(&row.input))?;
        let within=scored.within_deadline;
        let actual=scored.value;
        if let Ok(values)=&actual {
            receipt["rows"]=json!([{"input_digest":row.input.input_digest,"tokens":8192,"options":8,
                "raw_scores":values,"elapsed_us":scored.elapsed_us,"native_call_elapsed_us":scored.native_call_elapsed_us}]);
            flush(&mut file,&receipt)?;
        }
        let profiling=scorer.finish_profiling();
        let (actual,profiling)=match(actual,profiling) {
            (Ok(values),Ok(path))=>(values,path),
            (Err(native),Err(retention))=>return Err(native.context(format!("profiling also failed: {retention:#}"))),
            (Err(native),Ok(path))=>{receipt["profiling_path"]=json!(path);return Err(native);},
            (Ok(_),Err(retention))=>return Err(retention),
        };
        receipt["profiling_path"]=json!(profiling);
        ensure!(!optimized.exists() && !optimized.with_extension("weights.bin").exists(),
            "profiling-only load must not emit optimized graph/weights artifacts");
        receipt["peak_process_rss_bytes"]=json!(peak_rss());flush(&mut file,&receipt)?;
        ensure!(within,"original120s diagnostic score operation exceeded");
        ensure!(peak_rss().is_some_and(|rss| rss > 0 && rss <= profile.memory_reservation_bytes),"actual diagnostic RSS refused");
        let(raw,mass,argmax)=comparison(&actual,reference)?;
        receipt["original_fp32_comparison"]=json!({"raw":raw,"mass":mass,"argmax":argmax});
        ensure!(raw<=0.001 && mass<=0.05 && argmax,"unchanged original FP32 numerical gate refused");
        allocated_gqa_profile_for_model(&std::fs::read(&profiling)?,8192,selected.layers,expected_model)?;
        drop(scorer);verify_files(&files,&profile)?;
        Ok(())
    }).and_then(std::convert::identity);
    receipt["complete"] = json!(result.is_ok());
    if let Err(error) = &result {
        receipt["error"] = json!(format!("{error:#}"));
    }
    flush(&mut file, &receipt)?;
    result
}

fn tiny_profile(tokens: usize) -> Result<()> {
    tiny_profile_for_model(ModelChoice::ContextQwenCompact, tokens)
}
fn tiny_profile_for_model(model: ModelChoice, tokens: usize) -> Result<()> {
    ensure!(
        std::env::var("ORT_GQA_DISABLE_FLASH_ATTENTION").as_deref() == Ok("0"),
        "explicit frozen GQA policy required"
    );
    let graph_pin = ResourceFilePin {
        path: std::env::var("AIRK_GQA_TINY_GRAPH")?,
        sha256: serde_json::from_value(json!(std::env::var("AIRK_GQA_TINY_GRAPH_SHA256")?))?,
    };
    let graph = pinned(&graph_pin, 65536)?;
    let output = PathBuf::from(std::env::var("AIRK_GQA_TINY_OUTPUT")?);
    let parent = output.parent().context("synthetic output parent")?;
    let label = match model {
        ModelChoice::ContextQwenCompact => "synthetic-gqa",
        ModelChoice::ContextQwenLarge => "synthetic-large-gqa",
        _ => anyhow::bail!("selected Context synthetic diagnostic required"),
    };
    let prefix = parent.join(format!("{label}-{tokens}"));
    let optimized = parent.join(format!("{label}-{tokens}-optimized.onnx"));
    ensure!(
        !optimized.exists(),
        "fresh synthetic optimized graph required"
    );
    let (_parent, control) = private_control()?;
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(Arc::new(
        airk_store::service::ServiceLease::acquire(&control)?,
    ))?;
    let result = owner
        .run(|| {
            airk_retrieval::arbiter::runtime::synthetic_gqa_profile_for_model(
                model, &graph, tokens, &prefix, &optimized,
            )
        })
        .and_then(std::convert::identity)?;
    let profile = PathBuf::from(
        result["profiling_path"]
            .as_str()
            .context("actual profiling output absent")?,
    );
    allocated_gqa_profile_for_model(&std::fs::read(profile)?, tokens, 1, model)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    flush(&mut file, &result)?;
    Ok(())
}
#[test]
#[ignore = "explicit synthetic GQA graph/pins/output and actual owned120s CPU4 process required; no model weights"]
fn software_v2_synthetic_gqa_allocation_t17() -> Result<()> {
    tiny_profile(17)
}
#[test]
#[ignore = "explicit synthetic GQA graph/pins/output and actual owned120s CPU4 process required; no model weights"]
fn software_v2_synthetic_gqa_allocation_t8192() -> Result<()> {
    tiny_profile(8192)
}

#[test]
#[ignore = "explicit Large synthetic graph/pins/output and actual owned120s CPU16 process required; no model weights"]
fn software_v2_synthetic_large_gqa_allocation_t17() -> Result<()> {
    tiny_profile_for_model(ModelChoice::ContextQwenLarge, 17)
}
#[test]
#[ignore = "explicit Large synthetic graph/pins/output and actual owned120s CPU16 process required; no model weights"]
fn software_v2_synthetic_large_gqa_allocation_t8192() -> Result<()> {
    tiny_profile_for_model(ModelChoice::ContextQwenLarge, 8192)
}

#[test]
fn selected_large_diagnostic_refuses_compact_shape_and_resource_substitution() -> Result<()> {
    let large = json!({"dur":1,"args":{"op_name":"GroupQueryAttention","provider":"CPUExecutionProvider",
        "node_index":"35","output_size":"417792",
        "output_type_shape":[{"float":[1,17,4096]},{"float":[1,8,17,128]},{"float":[1,8,17,128]}]}});
    let compact = json!({"dur":1,"args":{"op_name":"GroupQueryAttention","provider":"CPUExecutionProvider",
        "node_index":"27","output_size":"278528",
        "output_type_shape":[{"float":[1,17,2048]},{"float":[1,8,17,128]},{"float":[1,8,17,128]}]}});
    allocated_gqa_profile_for_model(
        &serde_json::to_vec(&vec![large.clone()])?,
        17,
        1,
        ModelChoice::ContextQwenLarge,
    )?;
    assert!(
        allocated_gqa_profile_for_model(
            &serde_json::to_vec(&vec![compact])?,
            17,
            1,
            ModelChoice::ContextQwenLarge
        )
        .is_err()
    );
    assert!(allocated_gqa_profile(&serde_json::to_vec(&vec![large.clone()])?, 17, 1).is_err());
    let mut layers = Vec::new();
    for index in 0..36 {
        let mut event = large.clone();
        event["args"]["node_index"] = json!(index.to_string());
        layers.push(event);
    }
    allocated_gqa_profile_for_model(
        &serde_json::to_vec(&layers)?,
        17,
        36,
        ModelChoice::ContextQwenLarge,
    )?;
    layers.truncate(28);
    assert!(
        allocated_gqa_profile_for_model(
            &serde_json::to_vec(&layers)?,
            17,
            36,
            ModelChoice::ContextQwenLarge
        )
        .is_err()
    );
    let fit = airk_core::arbiter::CpuFit {
        minimum_physical_cores: 16,
        intra_threads: 16,
        inter_threads: 1,
        sequential: true,
        spinning: false,
    };
    let selected = selected_diagnostic_profile(
        ModelChoice::ContextQwenLarge,
        8192,
        40u64 << 30,
        Some(&fit),
        ModelChoice::ContextQwenLarge,
    )?;
    assert_eq!(
        (
            selected.query_width,
            selected.query_heads,
            selected.kv_heads,
            selected.head_size,
            selected.layers
        ),
        (4096, 32, 8, 128, 36)
    );
    assert!(
        selected_diagnostic_profile(
            ModelChoice::ContextQwenCompact,
            8192,
            40u64 << 30,
            Some(&fit),
            ModelChoice::ContextQwenLarge
        )
        .is_err()
    );
    for (tokens, memory) in [(2048, 40u64 << 30), (8192, 16u64 << 30)] {
        assert!(
            selected_diagnostic_profile(
                ModelChoice::ContextQwenLarge,
                tokens,
                memory,
                Some(&fit),
                ModelChoice::ContextQwenLarge
            )
            .is_err()
        );
    }
    for changed in [
        airk_core::arbiter::CpuFit {
            minimum_physical_cores: 4,
            intra_threads: 4,
            ..fit.clone()
        },
        airk_core::arbiter::CpuFit {
            inter_threads: 2,
            ..fit.clone()
        },
        airk_core::arbiter::CpuFit {
            sequential: false,
            ..fit.clone()
        },
        airk_core::arbiter::CpuFit {
            spinning: true,
            ..fit.clone()
        },
    ] {
        assert!(
            selected_diagnostic_profile(
                ModelChoice::ContextQwenLarge,
                8192,
                40u64 << 30,
                Some(&changed),
                ModelChoice::ContextQwenLarge
            )
            .is_err()
        );
    }
    assert!(
        selected_diagnostic_profile(
            ModelChoice::ContextQwenLarge,
            8192,
            40u64 << 30,
            None,
            ModelChoice::ContextQwenLarge
        )
        .is_err()
    );
    let compact_fit = airk_core::arbiter::CpuFit {
        minimum_physical_cores: 4,
        intra_threads: 4,
        ..fit
    };
    let compact = selected_diagnostic_profile(
        ModelChoice::ContextQwenCompact,
        8192,
        16u64 << 30,
        Some(&compact_fit),
        ModelChoice::ContextQwenCompact,
    )?;
    assert_eq!(
        (
            compact.query_width,
            compact.query_heads,
            compact.layers,
            compact.cpu_threads
        ),
        (2048, 16, 28, 4)
    );
    assert!(
        airk_retrieval::arbiter::runtime::selected_gqa_model(ModelChoice::FocusQwenReranker06B)
            .is_err()
    );
    Ok(())
}

fn tiny_dense_profile(tokens: usize) -> Result<()> {
    use airk_retrieval::arbiter::runtime::{SyntheticDenseMask, synthetic_large_dense_gqa_profile};
    ensure!(
        matches!(tokens, 17 | 128),
        "dense tiny control is bounded17/128, never full8192 qualification"
    );
    ensure!(
        std::env::var("ORT_GQA_DISABLE_FLASH_ATTENTION").as_deref() == Ok("0"),
        "global frozen flash0 policy remains unchanged; graph causal0 selects dense attention"
    );
    let graph_pin = ResourceFilePin {
        path: std::env::var("AIRK_GQA_TINY_GRAPH")?,
        sha256: serde_json::from_value(json!(std::env::var("AIRK_GQA_TINY_GRAPH_SHA256")?))?,
    };
    let graph = pinned(&graph_pin, 65536)?;
    let output = PathBuf::from(std::env::var("AIRK_GQA_TINY_OUTPUT")?);
    let parent = output.parent().context("dense tiny output parent")?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    let mut receipt = json!({"kind":"large_dense_synthetic_attention_diagnostic","complete":false,
        "attention_mode":"explicit_mask_dense","tokens":tokens,"model_weights_loaded":false,
        "qualification_granted":false,"controls":[],"graph":graph_pin,
        "scope":"three actual bounded CPU dense-kernel controls only; no full8192, model or task qualification"});
    flush(&mut file, &receipt)?;
    let (_parent, control) = private_control()?;
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(Arc::new(
        airk_store::service::ServiceLease::acquire(&control)?,
    ))?;
    let result = owner
        .run(|| -> Result<()> {
            for (label, mask) in [
                ("causal-prefix", SyntheticDenseMask::CausalPrefix),
                ("unmasked-full-mean", SyntheticDenseMask::Unmasked),
                (
                    "causal-padding-hole",
                    SyntheticDenseMask::CausalWithMaskedKeyOne,
                ),
            ] {
                let prefix = parent.join(format!("synthetic-large-dense-gqa-{tokens}-{label}"));
                let optimized = parent.join(format!(
                    "synthetic-large-dense-gqa-{tokens}-{label}-optimized.onnx"
                ));
                ensure!(
                    !optimized.exists(),
                    "fresh synthetic dense optimized graph required"
                );
                let actual =
                    synthetic_large_dense_gqa_profile(&graph, tokens, &prefix, &optimized, mask)?;
                // Each distinct profiling session contains exactly ONE actual CPU kernel.
                // A zero-bias control fails a causal1 graph even when the causal-prefix control passes.
                receipt["controls"]
                    .as_array_mut()
                    .context("dense control rows absent")?
                    .push(actual.clone());
                flush(&mut file, &receipt)?;
                let profiling = actual["profiling_path"]
                    .as_str()
                    .context("actual dense profile output absent")?;
                allocated_gqa_profile_for_model(
                    &std::fs::read(profiling)?,
                    tokens,
                    1,
                    ModelChoice::ContextQwenLarge,
                )?;
            }
            ensure!(
                pinned(&graph_pin, 65536)? == graph,
                "dense tiny input graph changed"
            );
            Ok(())
        })
        .and_then(std::convert::identity);
    receipt["complete"] = json!(result.is_ok());
    if let Err(error) = &result {
        receipt["error"] = json!(format!("{error:#}"));
    }
    flush(&mut file, &receipt)?;
    result
}
#[test]
#[ignore = "explicit dense causal0 tiny graph/pins/output and owned8GiB/120s CPU16 window; no model weights"]
fn software_v2_synthetic_large_dense_gqa_allocation_t17() -> Result<()> {
    tiny_dense_profile(17)
}
#[test]
#[ignore = "explicit dense causal0 tiny graph/pins/output and owned8GiB/120s CPU16 window; no model weights"]
fn software_v2_synthetic_large_dense_gqa_allocation_t128() -> Result<()> {
    tiny_dense_profile(128)
}

#[test]
fn dense_diagnostic_refuses_flash_profile_and_compact_or_historical_identity() -> Result<()> {
    assert_eq!(
        DENSE_PROFILE_ID,
        "private-large-fp32-explicit-mask-dense-diagnostic-v1"
    );
    let actual =
        diagnostic_attention_identity(ModelChoice::ContextQwenLarge, DENSE_PROFILE_ID, true)?;
    assert_eq!(
        actual,
        (
            "large-dense",
            "large_dense_attention_diagnostic",
            "large-fp32-explicit-mask-dense-profiled/1"
        )
    );
    assert!(
        diagnostic_attention_identity(ModelChoice::ContextQwenCompact, DENSE_PROFILE_ID, true)
            .is_err()
    );
    assert!(
        diagnostic_attention_identity(ModelChoice::ContextQwenLarge, "old-flash-profile", true)
            .is_err()
    );
    assert!(
        diagnostic_attention_identity(ModelChoice::ContextQwenLarge, DENSE_PROFILE_ID, false)
            .is_err()
    );
    assert_eq!(
        diagnostic_attention_identity(ModelChoice::ContextQwenCompact, "old-compact", false)?.2,
        "compact-fp32-gqa-profiled/2"
    );
    assert_eq!(
        diagnostic_attention_identity(ModelChoice::ContextQwenLarge, "old-large", false)?.2,
        "large-fp32-gqa-profiled/2"
    );
    Ok(())
}
