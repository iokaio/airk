// SPDX-License-Identifier: Apache-2.0
//! One original R8 phase on the protected scientific Large CUDA graph.
use super::super::fp16_compact::cal::{
    chronological_audit, json_file, original_compiler, pin, verify_chronological_row,
};
use super::*;
use airk_retrieval::arbiter::compiler::CompiledInput;

const GRAPH: &str = "13da8267e7490e334b5a860ced90af8318a01f5c2d03759173feac45ca598b46";
const RUNTIME: &str = "9433753580c8a370bbea42119aa67ac81306229bcf6876514213f3e2979083b5";
const PLAN: &str = "52e2fb170c4074da74c4f08a04b02364c5dfde7dd612d0f2c964e0fbd157242c";
const BATCH: &str = "fac3dd246c91ede7fef576baea1e65ff2971e26f229159878e1993ac068fe6bf";

fn phase_rows<'a>(projection: &'a Value, ids: &[Value], phase: usize) -> Result<&'a [Value]> {
    let rows = projection["rows"]
        .as_array()
        .context("missing original Large CAL rows")?;
    ensure!(
        phase < 13
            && projection["phases"]
                .as_array()
                .is_some_and(|phases| phases.len() == 13)
            && projection["original_requests"] == 100
            && ids.len() == 100
            && rows.len() == 100,
        "complete original Large R8 population required"
    );
    let start = phase * 8;
    let end = (start + 8).min(rows.len());
    let selected = &rows[start..end];
    for (ordinal, row) in (start..end).zip(selected) {
        let status = row["status"].as_str();
        ensure!(
            row["ordinal"] == ordinal && row["id"] == ids[ordinal],
            "nonchronological original Large CAL request"
        );
        ensure!(
            match status {
                Some("eligible") => row["compiled"].is_object(),
                Some("no_native_assessment_request" | "compiler_refused_remaining_allowance") => {
                    row.get("compiled").is_none()
                }
                _ => false,
            },
            "wrong original Large CAL eligibility or compiled input"
        );
    }
    Ok(selected)
}

#[test]
#[ignore = "verified R8 phase, exact Large direct CUDA graph and owned scientific Job"]
fn large_direct_original_cal_phase() -> Result<()> {
    let absolute_ns = number("AIRK_LARGE_CUTLASS_EXPIRES_NS")?;
    let start = Instant::now();
    let remaining = absolute_ns
        .checked_sub(airk_process::monotonic::now_ns()?)
        .context("original Large CAL Job deadline expired")?;
    ensure!(
        (1..=120_000_000_000).contains(&remaining),
        "invalid original Large CAL Job deadline"
    );
    let deadline = start + Duration::from_nanos(remaining);
    let job = job_limits(22 << 30, 22 << 30)?;
    let audit_pin = env_pin(
        "AIRK_CUDA_CAL_LAUNCH_AUDIT",
        "AIRK_CUDA_CAL_LAUNCH_AUDIT_SHA256",
    )?;
    let projection_pin = env_pin(
        "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE",
        "AIRK_CUDA_CAL_CHRONOLOGICAL_LARGE_SHA256",
    )?;
    ensure!(
        projection_pin.sha256.to_string() == BATCH,
        "original full R8 Large input pin required"
    );
    let audit = chronological_audit(&audit_pin)?;
    let projection = json_file(&projection_pin, 128 << 20)?;
    ensure!(
        projection["model"] == json!(ModelChoice::ContextQwenLarge)
            && projection["launch_audit_sha256"] == audit_pin.sha256.to_string()
            && projection["phase_plan_sha256"] == PLAN
            && projection["NN_invocations"] == 0,
        "foreign Large R8 compiler projection"
    );
    let census = json_file(&pin(&audit["census"])?, 2 << 20)?;
    let ids = census["question_ids"]
        .as_array()
        .context("missing original Large CAL question census")?;
    let phase = usize::try_from(number("AIRK_CUDA_CAL_PHASE")?)?;
    let rows = phase_rows(&projection, ids, phase)?;
    let (_, profile, compiler) = original_compiler(&audit, ModelChoice::ContextQwenLarge)?;
    ensure!(
        projection["original_manifest"] == profile.manifest.to_string()
            && rows.iter().any(|row| row["status"] == "eligible"),
        "original Large compiler or eligible phase missing"
    );
    let (root, manifest) = airk_host::gpu_runtime::installed_paths()?;
    let campaign = root.parent().context("scientific campaign missing")?;
    let output = std::path::PathBuf::from(std::env::var("AIRK_PRIVATE_CENSUS_BASE")?);
    let identity = operation::Identity {
        input_sha256: projection_pin.sha256,
        profile: GRAPH.parse()?,
        process_id: std::process::id(),
        native_binary_sha256: std::env::var("AIRK_LARGE_CUTLASS_TEST_IMAGE_SHA")?.parse()?,
    };
    let mut journal =
        operation::Journal::create_until(&output.join("protected.json"), identity, absolute_ns)?;
    let (_parent, control) = private_control()?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = airk_kernel::inference_owner::NativeInferenceOwner::admit(lease.clone())?;
    let result = owner.run(|| -> Result<_> {
        let runtime = airk_host::gpu_runtime::admit(lease.clone(), &root, &manifest, deadline)?;
        ensure!(runtime.manifest_identity().to_string() == RUNTIME, "wrong scientific runtime");
        runtime.with_large_direct_cal_session(campaign, GRAPH, deadline, |score| {
            let mut observations = Vec::with_capacity(rows.len());
            for (index, row) in rows.iter().enumerate() {
                let ordinal = phase * 8 + index;
                if row["status"] != "eligible" {
                    observations.push(json!({"ordinal":ordinal,"id":row["id"],"status":row["status"]}));
                    continue;
                }
                let begin = Instant::now();
                let budget = row["remaining_ms"].as_u64().context("missing original request time")?;
                ensure!((1..=120_000).contains(&budget), "invalid remaining request time");
                let expires = deadline.min(begin + Duration::from_millis(budget));
                verify_chronological_row(&projection, row, &compiler, ids)?;
                let input: CompiledInput = serde_json::from_value(row["compiled"].clone())?;
                let compile_us = begin.elapsed().as_micros();
                ensure!(Instant::now() < expires, "original request expired before native dispatch");
                let observed = journal.observe(
                    operation::Kind::Numeric,
                    index + 1,
                    Some(input.input_digest),
                    || score(&input, expires),
                )?;
                let scores = observed.value?;
                let cards = row["roster"]["cards"].as_array().context("missing original card roster")?;
                ensure!(
                    observed.within_deadline
                        && Instant::now() < expires
                        && scores.len() == cards.len()
                        && scores.iter().all(|score| score.is_finite()),
                    "incomplete or late original Large CAL scores"
                );
                observations.push(json!({"ordinal":ordinal,"id":row["id"],"status":"raw_scores_only",
                    "input_digest":input.input_digest,"captures":row["captures"],"compiled":row["compiled"],
                    "roster":row["roster"],"scores":scores,"remaining_ms":budget,
                    "remaining_input_tokens":row["remaining_input_tokens"],"compile_us":compile_us,
                    "inference_us":observed.elapsed_us,"native_call_elapsed_us":observed.native_call_elapsed_us}));
            }
            Ok(json!({"phase":phase,"original_projection":projection_pin,"launch_audit":audit_pin,
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
        "original Large CAL phase exceeded deadline"
    );
    println!(
        "AIRK_LARGE_COMPENSATED={}",
        json!({"job":job,"native":result})
    );
    Ok(())
}

#[test]
fn large_direct_cal_keeps_original_final_phase_and_refuses_foreign_rows() -> Result<()> {
    let ids: Vec<_> = (0..100).map(|i| json!(format!("original-{i}"))).collect();
    let rows: Vec<_> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| json!({"ordinal":i,"id":id,"status":"no_native_assessment_request"}))
        .collect();
    let mut projection = json!({"rows":rows,"original_requests":100,"phases":vec![Value::Null;13]});
    let final_rows = phase_rows(&projection, &ids, 12)?;
    assert_eq!(final_rows.len(), 4);
    assert_eq!(final_rows[0]["ordinal"], 96);
    assert_eq!(final_rows[3]["ordinal"], 99);
    assert!(phase_rows(&projection, &ids, 13).is_err());
    projection["rows"][98]["compiled"] = json!({});
    assert!(phase_rows(&projection, &ids, 12).is_err());
    projection["rows"][98]["status"] = json!("eligible");
    assert!(phase_rows(&projection, &ids, 12).is_ok());
    projection["rows"][97]["ordinal"] = json!(98);
    assert!(phase_rows(&projection, &ids, 12).is_err());
    Ok(())
}
