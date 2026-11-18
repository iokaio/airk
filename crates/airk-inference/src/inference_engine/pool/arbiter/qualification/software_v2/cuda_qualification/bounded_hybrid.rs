// SPDX-License-Identifier: Apache-2.0
//! Private four-width Compact FP32 CUDA profiling with exact reference inputs.
use super::*;
use serde_json::json;

const PROOF_BYTES: u64 = 16 << 20;
const PROFILE_BYTES: u64 = 64 << 20;
const WIDTHS: [usize; 4] = [32, 128, 1024, 8192];
const GRAPH_SHA: &str = "2f4d3211e0bbf097c702acf90dcce046552a3e2d75dc49b2f5945d7988c76fee";
const RETAINED_SHA: &str = "3962186640b42648b345fa200516ed44a3de4ccab0d783e671c9c22c3b8b1bbd";
const SUPPLEMENTAL_SHA: &str = "c4360d8a759c68c305fba0df7bd04001a6b043d8a2a57b36f0d8ef0f898519b9";

fn exact_pin(key: &str, expected: &str) -> Result<ResourceFilePin> {
    let pin = env_pin(key, &format!("{key}_SHA256"))?;
    ensure!(
        pin.sha256 == expected.parse()?,
        "reviewed input differs: {key}"
    );
    Ok(pin)
}

fn reference_arrays(row: &NumericCase) -> Result<[&[f32]; 2]> {
    let finite = |values: &[f32]| {
        values.len() == row.input.candidate_positions.len()
            && values.iter().all(|value| value.is_finite())
    };
    match (
        row.reference_same_artifact.as_deref(),
        row.reference_original_fp32.as_deref(),
    ) {
        (Some(same), Some(original)) if finite(same) && finite(original) => Ok([same, original]),
        _ => anyhow::bail!("both complete, finite independent Compact references required"),
    }
}

fn validate_reference_producer(
    fixture: &NumericFixture,
    profile: &Profile,
) -> Result<ResourceFilePin> {
    let pin = fixture
        .reference_producer
        .clone()
        .context("independent producer missing")?;
    let producer: Value = serde_json::from_slice(&pinned(&pin, PROOF_BYTES)?)?;
    let rows: Vec<NumericCase> = serde_json::from_value(producer["rows"].clone())?;
    let assets = profile
        .assets
        .iter()
        .filter(|asset| {
            matches!(
                asset.role,
                AssetRole::Model | AssetRole::ExternalWeights | AssetRole::Tokenizer
            )
        })
        .collect::<Vec<_>>();
    ensure!(
        producer["schema"] == "airk.arbiter.independent-native-numeric-reference/2"
            && producer["qualification"] == false
            && producer["model"] == json!(fixture.model)
            && producer["compiler"] == json!(fixture.compiler)
            && producer["resource_policy_digest"] == json!(fixture.resource_policy)
            && producer["candidate_assets"] == json!(assets)
            && producer["original_reference_assets"] == json!(fixture.original_reference_assets)
            && !fixture.original_reference_assets.is_empty()
            && json!(rows) == json!(fixture.rows),
        "independent reference producer differs from exact current neural assets or tensors"
    );
    Ok(pin)
}

fn score_rows(
    scorer: &mut airk_retrieval::arbiter::runtime::BoundedHybridScorer,
    rows: [&NumericCase; 4],
    operations: &mut operation::Journal,
    receipt: &mut Value,
    file: &mut File,
    deadline: Instant,
) -> Result<()> {
    for (index, row) in rows.into_iter().enumerate() {
        let [same, original] = reference_arrays(row)?;
        let timed = operations.observe(
            operation::Kind::Numeric,
            index + 1,
            Some(row.input.input_digest),
            || scorer.score_with_deadline(&row.input, deadline),
        )?;
        let actual = timed.value?;
        let (same_raw, same_mass, same_argmax) = comparison(&actual, same)?;
        let (original_raw, original_mass, original_argmax) = comparison(&actual, original)?;
        receipt["rows"].as_array_mut().unwrap().push(json!({
            "name": row.name, "tokens": row.input.input_ids.len(), "input_digest": row.input.input_digest,
            "raw_scores": actual, "elapsed_us": timed.elapsed_us, "native_call_elapsed_us": timed.native_call_elapsed_us,
            "same_artifact_raw_error": same_raw, "same_artifact_mass_error": same_mass,
            "original_fp32_raw_error": original_raw, "original_fp32_mass_error": original_mass,
            "argmax_matches": same_argmax && original_argmax,
        }));
        flush_graph_diagnostic(file, receipt)?;
        ensure!(
            timed.within_deadline
                && same_raw <= 0.001
                && original_raw <= 0.001
                && same_mass <= 0.05
                && original_mass <= 0.05
                && same_argmax
                && original_argmax,
            "Compact FP32 reference or original deadline refused at T{}",
            WIDTHS[index]
        );
    }
    Ok(())
}

#[test]
#[ignore = "owned Windows CUDA job, reviewed complete graph, and exact independent Compact references"]
fn cuda_compact_fp32_bounded_hybrid_profile() -> Result<()> {
    let started_ns = airk_process::monotonic::now_ns()?;
    let started = Instant::now();
    let execution_user = airk_ipc::endpoint::current_user()?;
    let default_owner = diagnostic_default_owner()?;
    ensure!(
        execution_user == default_owner,
        "native default owner differs"
    );
    let (request, request_pin) = request()?;
    let Request::Probe {
        schema,
        gpu_input,
        runtime_manifest,
        bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
    } = request
    else {
        anyhow::bail!("reviewed Compact graph Probe request required");
    };
    ensure!(
        schema == SCHEMA
            && bounds.host_limit_bytes == 32 << 30
            && job_limit_bytes == 32 << 30
            && bounds.deadline_ms == 120_000
            && bounds.device_limit_bytes == 24 << 30,
        "approved original Compact host, device or time bound differs"
    );
    let deadline = started + Duration::from_millis(bounds.deadline_ms);
    let expires_ns = started_ns
        .checked_add(bounds.deadline_ms * 1_000_000)
        .context("original expiry overflow")?;
    let (input, profile, files, _) = inputs_from(&gpu_input)?;
    ensure!(
        profile.model == ModelChoice::ContextQwenCompact
            && profile.cpu_fit.is_none()
            && profile.device_execution.is_some(),
        "frozen Compact GPU profile required"
    );
    let capacity_plan = profile_probe_bounds(
        &bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
        &profile,
    )?;
    let graph_pin = exact_pin("AIRK_CUDA_HYBRID_GRAPH", GRAPH_SHA)?;
    let graph_bytes = pinned(&graph_pin, PROOF_BYTES)?;
    let graph: Value = serde_json::from_slice(&graph_bytes)?;
    ensure!(
        graph["request"] == json!(request_pin)
            && graph["native_input"] == json!(gpu_input)
            && graph["runtime_manifest"] == json!(runtime_manifest),
        "reviewed graph did not consume the exact current request, input and runtime"
    );
    let retained_pin = exact_pin("AIRK_CUDA_HYBRID_RETAINED", RETAINED_SHA)?;
    let supplementary_pin = exact_pin("AIRK_CUDA_HYBRID_SUPPLEMENTAL", SUPPLEMENTAL_SHA)?;
    let retained: NumericFixture = serde_json::from_slice(&pinned(&retained_pin, PROOF_BYTES)?)?;
    let supplementary: NumericFixture =
        serde_json::from_slice(&pinned(&supplementary_pin, PROOF_BYTES)?)?;
    numeric_shape(&retained, &profile, &input.policy)?;
    ensure!(
        supplementary.schema == retained.schema
            && supplementary.model == retained.model
            && supplementary.compiler == retained.compiler
            && supplementary.resource_policy == retained.resource_policy,
        "supplemental Compact references use a different native scoring identity"
    );
    numeric_rows(&supplementary.rows, &[(32, 8), (1024, 8)], false)?;
    let retained_producer = validate_reference_producer(&retained, &profile)?;
    let supplementary_producer = validate_reference_producer(&supplementary, &profile)?;
    let rows = [
        &supplementary.rows[0],
        &retained.rows[1],
        &supplementary.rows[1],
        &retained.rows[2],
    ];
    for row in rows {
        reference_arrays(row)?;
    }
    let out = output()?;
    let prefix = PathBuf::from(text("AIRK_CUDA_HYBRID_ORT_PROFILE_PREFIX")?);
    ensure!(
        out.is_absolute()
            && prefix.is_absolute()
            && out.parent() == prefix.parent()
            && prefix.file_name().is_some()
            && out != prefix
            && !prefix.exists(),
        "fresh guarded ORT profiling prefix required beside native output"
    );
    pinned(&runtime_manifest, 128 << 10)?;
    let (root, manifest) = airk_host::gpu_runtime::installed_paths()?;
    ensure!(
        manifest.as_path() == Path::new(&runtime_manifest.path),
        "runtime locator pin differs"
    );
    let actual_job = job_limits(bounds.host_limit_bytes, job_limit_bytes)?;
    let mut resources = airk_kernel::service_resources::Resources::detect(ResourceConfiguration {
        preset: ResourcePreset::Throughput,
        ..ResourceConfiguration::default()
    })?;
    let available = airk_kernel::service_resource_observations::available_memory();
    resources.observe(
        airk_kernel::service_resource_observations::rss(std::process::id()),
        available,
        true,
    );
    let incarnation = Incarnation::from_bytes(request_pin.sha256.as_bytes()[..16].try_into()?);
    let charge =
        Arc::new(resources.reserve_optional(incarnation, bounds.host_limit_bytes, available)?);
    let release = charge.release_observer();
    let (parent, control) = private_control()?;
    let mut control_parent = Some(parent);
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = Arc::new(airk_kernel::inference_owner::NativeInferenceOwner::admit(
        lease.clone(),
    )?);
    let binary = file_hash(&std::env::current_exe()?)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(&out)?;
    let mut receipt = json!({
        "schema": "airk.private.cuda-compact-bounded-hybrid-profile/1", "complete": false,
        "qualified": false, "product_admission": false, "request": request_pin, "native_input": gpu_input,
        "profile": profile.manifest, "source_manifest": input.source_manifest, "runtime_manifest": runtime_manifest,
        "reviewed_graph": graph_pin, "retained_fixture": retained_pin, "supplemental_fixture": supplementary_pin,
        "retained_producer": retained_producer, "supplemental_producer": supplementary_producer,
        "pid": std::process::id(), "native_binary_sha256": binary, "execution_user": execution_user,
        "default_owner": default_owner, "bounds": bounds, "actual_job": actual_job,
        "native_host_reservation_bytes": charge.bytes(), "resource_snapshot": resources.snapshot(),
        "original_expires_monotonic_ns": expires_ns, "runtime_profile": null, "profiled_runs": [],
        "rows": [], "session_teardown_observed": false, "measured_peak_device_bytes": null,
        "measured_transfer_bytes": null,
    });
    flush_graph_diagnostic(&mut file, &receipt)?;
    let mut operations = operation::Journal::create_until(
        &out,
        operation::Identity {
            input_sha256: request_pin.sha256,
            profile: profile.manifest,
            process_id: std::process::id(),
            native_binary_sha256: binary,
        },
        expires_ns,
    )?;
    let run = owner
        .run(|| -> Result<()> {
            let mut custody = Custody {
                scorer: None,
                device: None,
                owner: owner.clone(),
                lease: lease.clone(),
                charge: Some(charge),
                control: &mut control_parent,
                files: Some(files),
            };
            ensure!(Instant::now() < deadline, "expired before CUDA admission");
            let files = custody.files.as_ref().unwrap();
            verify_files(files, &profile)?;
            let runtime = airk_host::gpu_runtime::admit(lease.clone(), &root, &manifest, deadline)?;
            ensure!(
                runtime.manifest_identity() == runtime_manifest.sha256,
                "admitted runtime differs"
            );
            let adapters = runtime.adapters(deadline)?;
            ensure!(
                adapters.len() == 1,
                "Compact profile requires one observed CUDA adapter"
            );
            let device = runtime.device(adapters[0].uuid, deadline)?;
            custody.device = Some(device.clone());
            receipt["adapter_before"] = json!(device.observation(deadline)?);
            let catalog = airk_kernel::inference_owner::evaluation_catalog(
                lease.clone(),
                vec![profile.clone()],
            )?;
            let execution = catalog
                .execution_profile(profile.manifest)?
                .bind_cuda_probe(device.clone(), capacity_plan, deadline)?;
            let loaded = operations.observe(operation::Kind::Load, 0, None, || {
                Scorer::open_compact_bounded_hybrid_profiled(
                    execution,
                    files,
                    &prefix,
                    &graph_bytes,
                    graph_pin.sha256,
                )
            })?;
            receipt["load_us"] = json!(loaded.elapsed_us);
            let mut scorer = loaded.value?;
            ensure!(
                loaded.within_deadline,
                "original Compact session load expiry exceeded"
            );
            receipt["policy_identity"] = json!(scorer.policy_identity());
            flush_graph_diagnostic(&mut file, &receipt)?;
            score_rows(
                &mut scorer,
                rows,
                &mut operations,
                &mut receipt,
                &mut file,
                deadline,
            )?;
            scorer.synchronize(deadline)?;
            ensure!(
                scorer.device_settled(),
                "Compact CUDA work remains unsettled"
            );
            let path = scorer.finish_profiling()?;
            ensure!(
                path.parent() == prefix.parent()
                    && path.extension().is_some_and(|ext| ext == "json")
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| prefix
                            .file_name()
                            .and_then(|s| s.to_str())
                            .is_some_and(|start| name.starts_with(start))),
                "ORT profile escaped its guarded prefix"
            );
            let bytes = small(&path, PROFILE_BYTES)?;
            receipt["runtime_profile"] = json!({
                "path": path, "sha256": sha(&bytes), "bytes": bytes.len(),
            });
            receipt["profiled_runs"] = json!(super::hybrid_profile_trace::profiled_runs(
                &bytes,
                &scorer.graph_metadata().rows,
                &WIDTHS,
            )?);
            receipt["adapter_after"] = json!(device.observation(deadline)?);
            drop(scorer);
            device.synchronize(deadline)?;
            ensure!(
                !device.poisoned() && !cuda::process_poisoned(),
                "native teardown settlement unknown"
            );
            receipt["session_teardown_observed"] = json!(true);
            verify_files(files, &profile)?;
            for (pin, max) in [
                (&request_pin, 1 << 20),
                (&gpu_input, 1 << 20),
                (&input.profile, 1 << 20),
                (&runtime_manifest, 128 << 10),
                (&input.source_manifest, PROOF_BYTES),
                (&graph_pin, PROOF_BYTES),
                (&retained_pin, PROOF_BYTES),
                (&supplementary_pin, PROOF_BYTES),
                (&retained_producer, PROOF_BYTES),
                (&supplementary_producer, PROOF_BYTES),
            ] {
                pinned(pin, max)?;
            }
            ensure!(
                Instant::now() < deadline,
                "original Compact expiry exceeded during postpins"
            );
            Ok(())
        })
        .and_then(std::convert::identity);
    drop(owner);
    drop(lease);
    let retained_control = control_parent.is_none();
    let cleanup = control_parent
        .take()
        .map(tempfile::TempDir::close)
        .transpose();
    let cleaned = matches!(&cleanup, Ok(Some(())));
    let mut wait = std::pin::pin!(release.wait());
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let released = matches!(
        std::future::Future::poll(wait.as_mut(), &mut cx),
        std::task::Poll::Ready(Ok(()))
    );
    let poisoned = cuda::process_poisoned();
    let peak = peak_rss();
    receipt["native_control_retained_for_unknown_cuda"] = json!(retained_control);
    receipt["native_control_cleanup_observed"] = json!(cleaned);
    receipt["native_control_cleanup_error"] =
        json!(cleanup.as_ref().err().map(ToString::to_string));
    receipt["native_host_charge_release_observed"] = json!(released);
    receipt["process_cuda_poisoned"] = json!(poisoned);
    receipt["peak_process_rss_bytes"] = json!(peak);
    let result = run.and_then(|()| {
        ensure!(
            receipt["session_teardown_observed"] == true
                && !retained_control
                && cleaned
                && released
                && !poisoned,
            "Compact CUDA session or host release unproven"
        );
        final_host_peak(peak, bounds.host_limit_bytes)?;
        Ok(())
    });
    receipt["complete"] = json!(result.is_ok());
    receipt["error"] = json!(result.as_ref().err().map(|e| format!("{e:#}")));
    receipt["elapsed_us_to_final_report_start"] = json!(started.elapsed().as_micros());
    flush_graph_diagnostic(&mut file, &receipt)?;
    if Instant::now() >= deadline {
        receipt["complete"] = json!(false);
        receipt["final_report_retention_exceeded_original_deadline"] = json!(true);
        flush_graph_diagnostic(&mut file, &receipt)?;
        anyhow::bail!("original Compact profiling expiry during result retention");
    }
    result
}
