// SPDX-License-Identifier: Apache-2.0
//! Diagnostic with FP32 dense products and the original stock FP16 attention boundaries.
use super::*;

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
#[ignore = "owned one-operator Compact CUDA precision investigation"]
fn compact_cutlass_original_t1014_operand() -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(120);
    let (root, manifest) = airk_host::gpu_runtime::installed_paths()?;
    let prep = root
        .parent()
        .context("scientific stage has no campaign parent")?
        .join("compact-cutlass-fp32-t1014-operands-r1");
    let (_parent, control) = private_control()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(lease.clone())?;
    let result = owner.run(|| -> Result<_> {
        let runtime = airk_host::gpu_runtime::admit(lease.clone(), &root, &manifest, deadline)?;
        ensure!(
            runtime.manifest_identity().to_string()
                == "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5",
            "exact scientific CU15 manifest required"
        );
        runtime.observe_compact_cutlass_operand(&prep, deadline)
    })??;
    drop(owner);
    drop(lease);
    let released = airk_store::service::ServiceLease::acquire(&control)?;
    drop(released);
    println!(
        "AIRK_COMPACT_CUTLASS_OPERAND={}",
        serde_json::to_string(&result)?
    );
    Ok(())
}

#[cfg(all(windows, target_arch = "x86_64"))]
fn protected_cutlass_model(
    score: bool,
    timing: Option<timing::Population>,
    pressure: bool,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(120);
    let (root, manifest) = airk_host::gpu_runtime::installed_paths()?;
    let campaign = root
        .parent()
        .context("original scientific campaign missing")?;
    let expected = if score {
        let value = std::env::var("AIRK_COMPACT_CUTLASS_TYPED_GRAPH")?;
        ensure!(
            value.len() == 64 && value.bytes().all(|x| x.is_ascii_hexdigit()),
            "validated typed-graph digest required"
        );
        Some(value)
    } else {
        ensure!(
            std::env::var_os("AIRK_COMPACT_CUTLASS_TYPED_GRAPH").is_none(),
            "graph-only mode cannot accept scoring gate"
        );
        None
    };
    let (_parent, control) = private_control()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(lease.clone())?;
    let result = owner.run(|| -> Result<_> {
        let runtime = airk_host::gpu_runtime::admit(lease.clone(), &root, &manifest, deadline)?;
        ensure!(
            runtime.manifest_identity().to_string()
                == "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5",
            "exact reviewed scientific runtime required"
        );
        if let Some(population) = timing {
            protected_timing(
                &runtime,
                campaign,
                expected
                    .as_deref()
                    .context("missing protected graph gate")?,
                deadline,
                population,
                TimingModel::Compact { pressure },
            )
        } else {
            runtime.observe_protected_cutlass_model(campaign, expected.as_deref(), deadline)
        }
    })??;
    drop(owner);
    drop(lease);
    let released = airk_store::service::ServiceLease::acquire(&control)?;
    drop(released);
    println!(
        "AIRK_COMPACT_CUTLASS_MODEL={}",
        serde_json::to_string(&result)?
    );
    Ok(())
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
#[ignore = "owned protected Compact CUTLASS graph-only placement"]
fn compact_cutlass_protected_graph() -> Result<()> {
    protected_cutlass_model(false, None, false)
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
#[ignore = "owned protected Compact CUTLASS original engineering score"]
fn compact_cutlass_protected_fixed_score() -> Result<()> {
    protected_cutlass_model(true, None, false)
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
#[ignore = "owned protected Compact unprofiled twenty-sample timing"]
fn compact_cutlass_protected_fixed_timing() -> Result<()> {
    protected_cutlass_model(true, Some(timing::Population::Fixed), false)
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
#[ignore = "owned protected Compact unprofiled mixed-width timing"]
fn compact_cutlass_protected_mixed_timing() -> Result<()> {
    protected_cutlass_model(true, Some(timing::Population::Mixed), false)
}

#[cfg(all(windows, target_arch = "x86_64"))]
pub(in super::super) enum TimingModel {
    Compact {
        pressure: bool,
    },
    Large {
        digests: [&'static str; 4],
        pressure: bool,
    },
}

#[cfg(all(windows, target_arch = "x86_64"))]
pub(in super::super) fn protected_timing(
    runtime: &airk_retrieval::arbiter::runtime::cuda::CudaRuntime,
    campaign: &Path,
    gate: &str,
    deadline: Instant,
    population: timing::Population,
    model: TimingModel,
) -> Result<Value> {
    let prefix = match model {
        TimingModel::Large { .. } => "AIRK_LARGE_CUTLASS",
        TimingModel::Compact { .. } => "AIRK_COMPACT_CUTLASS",
    };
    let variable = |key: &str| std::env::var(format!("{prefix}_{key}"));
    let expires_ns = variable("EXPIRES_NS")?.parse::<u64>()?;
    let now_ns = airk_process::monotonic::now_ns()?;
    ensure!(
        now_ns < expires_ns && expires_ns - now_ns <= 120_000_000_000,
        "invalid original timing deadline"
    );
    let output = std::path::PathBuf::from(std::env::var("AIRK_PRIVATE_CENSUS_BASE")?);
    let binary = variable("TEST_IMAGE_SHA")?.parse()?;
    let bundle = variable("EXPECTED_WORDS")?;
    let expected_words: [[u32; 8]; 4] = serde_json::from_str(&bundle)?;
    let identity = operation::Identity {
        input_sha256: sha(bundle.as_bytes()),
        profile: gate.parse()?,
        process_id: std::process::id(),
        native_binary_sha256: binary,
    };
    let mut journal =
        operation::Journal::create_until(&output.join("protected.json"), identity, expires_ns)?;
    let digests = match model {
        TimingModel::Compact { .. } => [
            T32_INPUT,
            DEVELOPMENT_CASES[0].2,
            DEVELOPMENT_CASES[1].2,
            DEVELOPMENT_CASES[2].2,
        ],
        TimingModel::Large { digests, .. } => digests,
    };
    let measure = |call: &mut dyn FnMut(usize) -> Result<Vec<f32>>,
                   device_snapshot: &mut dyn FnMut() -> Result<Value>| {
        let mut rows = Vec::new();
        for (index, (width_index, cycle, label)) in population.schedule().into_iter().enumerate() {
            let (kind, ordinal) = population.journal_operation(index);
            let width = [32, 128, 1024, 8192][width_index];
            let digest = digests[width_index];
            let utc = || -> Result<u128> {
                Ok(std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos())
            };
            let adapter_before = device_snapshot()?;
            let host_rss_before =
                airk_kernel::service_resource_observations::rss(std::process::id());
            let start_ns = airk_process::monotonic::now_ns()?;
            let start_utc = utc()?;
            let observed =
                journal.observe(kind, ordinal, Some(digest.parse()?), || call(width_index))?;
            let finish_utc = utc()?;
            let finish_ns = airk_process::monotonic::now_ns()?;
            let adapter_after = device_snapshot()?;
            let host_rss_after =
                airk_kernel::service_resource_observations::rss(std::process::id());
            let scores = observed.value?;
            ensure!(
                observed.within_deadline
                    && scores
                        .iter()
                        .map(|v| v.to_bits())
                        .eq(expected_words[width_index]),
                "warm call expired or differed from the sealed candidate score words"
            );
            rows.push(json!({"tokens":width,"cycle":cycle,"kind":label,"input_digest":digest,
                        "adapter_before":adapter_before,"adapter_after":adapter_after,"host_rss_before_bytes":host_rss_before,"host_rss_after_bytes":host_rss_after,
                        "native_call_us":observed.native_call_elapsed_us,"start_us":start_ns/1000,"finish_us":finish_ns/1000,
                        "start_utc_ns":start_utc,"finish_utc_ns":finish_utc,"forward_synchronized":true,"raw_scores":scores}));
        }
        Ok(json!({"rows":rows,"expires_monotonic_ns":expires_ns,"profiled":false}))
    };
    match model {
        TimingModel::Large { pressure, .. } => {
            let tier = if pressure {
                Some(std::env::var("AIRK_LARGE_PRESSURE_TIER_GIB")?.parse::<u64>()?)
            } else {
                None
            };
            runtime.time_large_direct(campaign, gate, deadline, tier, measure)
        }
        TimingModel::Compact { pressure } => {
            let tier = if pressure {
                Some(std::env::var("AIRK_COMPACT_PRESSURE_TIER_GIB")?.parse::<u64>()?)
            } else {
                None
            };
            runtime.time_protected_cutlass_model(campaign, gate, deadline, tier, measure)
        }
    }
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
#[ignore = "owned protected Compact single-call measured-availability fit"]
fn compact_cutlass_pressure_fit() -> Result<()> {
    protected_cutlass_model(true, Some(timing::Population::Fit), true)
}

#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
#[ignore = "owned protected Compact retained-pressure full-width twenty-sample timing"]
fn compact_cutlass_pressure_fixed() -> Result<()> {
    protected_cutlass_model(true, Some(timing::Population::Fixed), true)
}

pub(super) const SPEC: ProjectionSpec = ProjectionSpec {
    artifact: Some((
        "6486768c16820bc09f11c2dfbbbdd1d0ddf896e3a10b2315ec9f5d798ae9add1",
        "fe94664a4addf797feaa54235e07f484f4e72051a894a154e5aff7f2c1c11ea6",
    )),
    stage_schema: "airk.private.compact-full-projection-candidate/1",
    source_schema: "airk.private.compact-fp32-full-source/1",
    metadata_schema: "airk.private.compact-fp32-full-probe-metadata/1",
    graph_schema: "airk.private.compact-fp32-full-graph-diagnostic/1",
    native_graph_sha: Some("492da42c6b55364d79daa2b20b1d17373039acc5c43995c4df11031fa82e4c54"),
    t32_schema: "airk.private.compact-fp32-full-t32/1",
    reference_schema: "airk.private.compact-full-projection-reference/1",
    reference_sha: Some("0995832e12ef5a84af11562187281c86b96304693053272b3b1fbd091026afc7"),
    recipe_routes: "routes",
    matching_scores: "matching_fp16",
    projections: 196,
};

pub(super) fn stage_proof(fact: &Value) -> bool {
    let proof = &fact["graph_proof"];
    proof["fp32_weight_upcasts"] == 196
        && proof["removed_activation_and_output_half_boundaries"] == 392
        && proof["attention_nodes"] == 28
        && proof["source_graph_recovered_byte_for_byte"] == true
        && proof["retained_sidecar"] == true
}

#[test]
#[ignore = "owned Compact full-projection metadata only"]
fn emit_full_metadata() -> Result<()> {
    emit_compact_fp16_probe_metadata(Some(&SPEC))
}

#[test]
#[ignore = "owned Compact full-projection final typed SessionState graph"]
fn observe_full_graph() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::FullGraph)
}

#[test]
#[ignore = "owned Compact full-projection precision score on the original admission"]
fn observe_full_scores() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::FullScores)
}

#[test]
#[ignore = "owned Compact T1014 sparse attribution with enforced unchanged original score words"]
fn observe_full_cuts() -> Result<()> {
    observe_compact_fp16_t32(CompactFp16Probe::FullCuts)
}

pub(super) fn selected_taps(group: &str) -> Result<&'static [(&'static str, usize)]> {
    match group {
        "head" => Ok(Scorer::COMPACT_FULL_HEAD_TAPS),
        "attention" => Ok(Scorer::COMPACT_FULL_ATTENTION_TAPS),
        "dense" => Ok(Scorer::COMPACT_FULL_DENSE_TAPS),
        _ => anyhow::bail!("unknown full-projection diagnostic group"),
    }
}

#[test]
fn full_cut_selector_and_retained_word_shapes_are_exact() {
    assert!(selected_taps("all").is_err());
    for group in ["head", "attention", "dense"] {
        let taps = selected_taps(group).expect("reviewed group");
        assert_eq!(taps.len(), 4);
        assert_eq!(
            taps.iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            4
        );
    }
}

pub(super) fn record_cuts(
    receipt: &mut Value,
    scores: &[f32],
    tapped: Vec<Vec<u16>>,
) -> Result<()> {
    let group = text("AIRK_CUDA_COMPACT_FULL_CUT_GROUP")?;
    let roster = selected_taps(&group)?;
    receipt["full_cut_group"] = json!(group);
    let expected = [
        0x420f9708, 0x42071e6e, 0x42071e43, 0x42311294, 0x421bb727, 0x4243ae58, 0x422eb756,
    ];
    ensure!(
        scores.iter().map(|s| s.to_bits()).eq(expected) && tapped.len() == roster.len(),
        "full-projection cut perturbed the sealed score words or output roster"
    );
    let mut rows = Vec::with_capacity(roster.len());
    for ((name, width), words) in roster.iter().copied().zip(tapped) {
        let tokens = if matches!(
            name,
            "/airk_g5/attention/13/k16" | "/airk_g5/attention/13/v16"
        ) {
            1014
        } else {
            1
        };
        let word_size = if name.starts_with("/scorer/") { 2 } else { 1 };
        ensure!(
            words.len() == width * tokens * word_size,
            "native full-projection cut width differs"
        );
        rows.push(json!({"name":name, "width":width, "tokens_retained":tokens, "words":words}));
    }
    receipt["full_cut_rows"] = Value::Array(rows);
    Ok(())
}

#[test]
fn full_stage_requires_all_boundaries_and_original_attention() {
    let mut receipt = json!({"graph_proof": {"fp32_weight_upcasts":196,
        "removed_activation_and_output_half_boundaries":392, "attention_nodes":28,
        "source_graph_recovered_byte_for_byte":true, "retained_sidecar":true}});
    assert!(stage_proof(&receipt));
    receipt["graph_proof"]["attention_nodes"] = json!(27);
    assert!(!stage_proof(&receipt));
    receipt["graph_proof"]["attention_nodes"] = json!(28);
    receipt["graph_proof"]["removed_activation_and_output_half_boundaries"] = json!(391);
    assert!(!stage_proof(&receipt));
}
