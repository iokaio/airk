// SPDX-License-Identifier: Apache-2.0
//! Read one original CAL phase through the observed Windows Compact scientific graph.
use super::*;
use airk_retrieval::arbiter::compiler::CompiledInput;

const GRAPH: &str = "6142500349b0c9e72ef97e6c1f06ab5a81eb519942fe35d936a9d29020542581";
const RUNTIME: &str = "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5";

#[test]
#[ignore = "original closed CAL input, reviewed native CUDA graph and bounded scientific job"]
fn compact_cutlass_original_cal_phase() -> Result<()> {
    let absolute_ns: u64 = std::env::var("AIRK_COMPACT_CUTLASS_EXPIRES_NS")?.parse()?;
    let start = Instant::now();
    let remaining = absolute_ns
        .checked_sub(airk_process::monotonic::now_ns()?)
        .context("original CAL Job deadline expired")?;
    ensure!(
        (1..=120_000_000_000).contains(&remaining),
        "invalid original CAL Job deadline"
    );
    let deadline = start + Duration::from_nanos(remaining);
    let audit_pin = env_pin(
        "AIRK_CUDA_CAL_LAUNCH_AUDIT",
        "AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256",
    )?;
    let audit = chronological_audit(&audit_pin)?;
    let projection_pin = env_pin(
        "AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT",
        "AIRK_CUDA_CAL_CHRONOLOGICAL_COMPACT_SHA256",
    )?;
    let projection = json_file(&projection_pin, 128 << 20)?;
    ensure!(projection["model"] == MODEL, "wrong original compiler");
    verify_chronological_projection(&audit, &audit_pin.sha256.to_string(), &projection, true)?;
    let census = json_file(&pin(&audit["census"])?, 2 << 20)?;
    let ids = array(&census, "question_ids")?;
    let (_, _, compiler) = original_compiler(&audit, ModelChoice::ContextQwenCompact)?;
    let phase = usize::try_from(number("AIRK_CUDA_CAL_PHASE")?)?;
    ensure!(phase < 13, "unknown original phase");
    let range = phase * 8..((phase + 1) * 8).min(100);
    let rows = array(&projection, "rows")?
        .get(range)
        .context("phase not in closed prefix")?;
    let (root, manifest) = airk_host::gpu_runtime::installed_paths()?;
    let campaign = root.parent().context("scientific campaign missing")?;
    let output = std::path::PathBuf::from(std::env::var("AIRK_PRIVATE_CENSUS_BASE")?);
    let identity = operation::Identity {
        input_sha256: projection_pin.sha256,
        profile: GRAPH.parse()?,
        process_id: std::process::id(),
        native_binary_sha256: std::env::var("AIRK_COMPACT_CUTLASS_TEST_IMAGE_SHA")?.parse()?,
    };
    let mut journal =
        operation::Journal::create_until(&output.join("protected.json"), identity, absolute_ns)?;
    let (_parent, control) = private_control()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(lease.clone())?;
    let result = owner.run(|| -> Result<_> {
        let runtime = airk_host::gpu_runtime::admit(lease.clone(), &root, &manifest, deadline)?;
        ensure!(runtime.manifest_identity().to_string() == RUNTIME, "wrong scientific runtime");
        runtime.with_protected_cutlass_cal_session(campaign, GRAPH, deadline, |score| {
            let mut observations = Vec::with_capacity(rows.len());
            for (phase_index, row) in rows.iter().enumerate() {
                let ordinal = usize::try_from(row["ordinal"].as_u64().context("missing CAL ordinal")?)?;
                if row["status"] != "eligible" {
                    observations.push(json!({"ordinal":ordinal,"id":row["id"],"status":row["status"]}));
                    continue;
                }
                let begin = Instant::now();
                let budget = row["remaining_ms"].as_u64().context("missing remaining request time")?;
                ensure!((1..=120_000).contains(&budget), "invalid remaining request time");
                let expires = deadline.min(begin + Duration::from_millis(budget));
                verify_chronological_row(&projection, row, &compiler, ids)?;
                let input: CompiledInput = serde_json::from_value(row["compiled"].clone())?;
                let compile_us = begin.elapsed().as_micros();
                ensure!(Instant::now() < expires, "original request expired before native dispatch");
                let observed = journal.observe(operation::Kind::Numeric, phase_index + 1, Some(input.input_digest), || score(&input, expires))?;
                let scores = observed.value?;
                ensure!(observed.within_deadline && Instant::now() < expires
                    && scores.len() == array(&row["roster"], "cards")?.len()
                    && scores.iter().all(|value| value.is_finite()), "incomplete or late CAL scores");
                observations.push(json!({"ordinal":ordinal,"id":row["id"],"status":"raw_scores_only",
                    "input_digest":input.input_digest, "captures":row["captures"],"compiled":row["compiled"],
                    "roster":row["roster"],"scores":scores,"remaining_ms":budget,"compile_us":compile_us,
                    "inference_us":observed.elapsed_us,"native_call_elapsed_us":observed.native_call_elapsed_us}));
            }
            Ok(json!({"phase":phase, "original_projection":projection_pin, "launch_audit":audit_pin,
                "rows":observations,"qualification":false,"calibration_fitted":false}))
        })
    })??;
    drop(owner);
    drop(lease);
    drop(airk_store::service::ServiceLease::acquire(&control)?);
    pinned(&projection_pin, 128 << 20)?;
    pinned(&audit_pin, 2 << 20)?;
    ensure!(
        Instant::now() < deadline,
        "original CAL phase exceeded deadline"
    );
    println!(
        "AIRK_COMPACT_CUTLASS_CAL={}",
        serde_json::to_string(&result)?
    );
    Ok(())
}
