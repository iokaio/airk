// SPDX-License-Identifier: Apache-2.0
//! Owned cold load and unprofiled Context T8192 warm measurements.
use super::*;

const TOKEN_LIMIT: usize = 8192;
const WHOLE_PHASE_NS: u64 = 3_600_000_000_000;

fn verify_numeric(row: &NumericCase, scores: &[f32]) -> Result<()> {
    for expected in reference_arrays(row)? {
        let (raw, mass, argmax) = comparison(scores, expected)?;
        ensure!(
            raw <= 0.001 && mass <= 0.05 && argmax,
            "Context resource score differs from its independent FP32 reference"
        );
    }
    Ok(())
}

pub(super) fn run(experiment: Experiment) -> Result<()> {
    let large = experiment.equivalent_predecessor;
    let started = Instant::now();
    let now_ns = airk_process::monotonic::now_ns()?;
    let expires_ns = number("AIRK_CUDA_HYBRID_WHOLE_EXPIRES_NS")?;
    ensure!(
        expires_ns > now_ns && expires_ns - now_ns <= WHOLE_PHASE_NS,
        "invalid original Context resource whole-process expiry"
    );
    let whole_deadline = started
        .checked_add(Duration::from_nanos(expires_ns - now_ns))
        .context("native resource whole-process expiry overflow")?;
    let execution_user = airk_ipc::endpoint::current_user()?;
    let default_owner = diagnostic_default_owner()?;
    ensure!(
        execution_user == default_owner,
        "native resource default owner differs"
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
        anyhow::bail!("reviewed Context graph Probe request required");
    };
    ensure!(
        schema == SCHEMA
            && bounds.host_limit_bytes == experiment.host_gib << 30
            && job_limit_bytes == experiment.host_gib << 30
            && bounds.deadline_ms == 120_000
            && bounds.device_limit_bytes == experiment.device_gib << 30,
        "approved original Context host, device or operation limit differs"
    );
    let (input, profile, files, _) = inputs_from(&gpu_input)?;
    ensure!(
        profile.model == experiment.model
            && profile.cpu_fit.is_none()
            && profile.device_execution.is_some()
            && input.policy.deadline_ms == bounds.deadline_ms
            && input.policy.required_warm_samples == 20,
        "frozen Context GPU and resource profiles required"
    );
    let request_capacity = profile_probe_bounds(
        &bounds,
        cold_bytes,
        warm_bytes,
        headroom_bytes,
        job_limit_bytes,
        &profile,
    )?;
    let (capacity_pin, capacity_plan) = if large {
        ensure!(
            (cold_bytes, warm_bytes, headroom_bytes) == (40 << 30, 8 << 30, 4 << 30),
            "Large resource measurement requires the reviewed device reservation"
        );
        (None, request_capacity)
    } else {
        let pin = exact_pin("AIRK_CUDA_HYBRID_CAPACITY", CAPACITY_SHA)?;
        let (_, plan) = capacity(&pinned(&pin, 4096)?, bounds.device_limit_bytes)?;
        (Some(pin), plan)
    };
    let graph_pin = exact_pin("AIRK_CUDA_HYBRID_GRAPH", experiment.graph)?;
    let graph_bytes = pinned(&graph_pin, PROOF_BYTES)?;
    let graph: Value = serde_json::from_slice(&graph_bytes)?;
    ensure!(
        graph["request"] == json!(request_pin)
            && graph["native_input"] == json!(gpu_input)
            && graph["runtime_manifest"] == json!(runtime_manifest),
        "reviewed graph request, input or runtime differs"
    );
    let fixture_pin = exact_pin("AIRK_CUDA_HYBRID_RETAINED", experiment.retained)?;
    let fixture: NumericFixture = serde_json::from_slice(&pinned(&fixture_pin, PROOF_BYTES)?)?;
    numeric_shape(&fixture, &profile, &input.policy)?;
    let lineage = if large {
        Some(equivalent_predecessor(&profile, &input)?)
    } else {
        None
    };
    let reference_profile = lineage.as_ref().map_or(&profile, |proof| &proof.profile);
    let producer = validate_reference_producer(&fixture, reference_profile)?;
    let row = &fixture.rows[2];
    ensure!(
        row.input.input_ids.len() == TOKEN_LIMIT,
        "exact full T8192 resource input required"
    );
    reference_arrays(row)?;
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

    let out = output()?;
    ensure!(
        out.is_absolute(),
        "absolute checked native resource output required"
    );
    let binary = file_hash(&std::env::current_exe()?)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(&out)?;
    let mut receipt = json!({
        "schema":if large { "airk.private.cuda-large-batch-view-bounded-hybrid-resource/1" }
            else { "airk.private.cuda-compact-bounded-hybrid-resource/1" },
        "complete":false, "qualified":false, "product_admission":false,
        "pid":std::process::id(), "execution_user":execution_user, "default_owner":default_owner,
        "native_binary_sha256":binary, "request":request_pin, "native_input":gpu_input,
        "profile":profile.manifest, "source_manifest":input.source_manifest, "runtime_manifest":runtime_manifest,
        "reviewed_graph":graph_pin, "retained_fixture":fixture_pin, "retained_producer":producer,
        "capacity_policy":capacity_pin, "applied_capacity":{
            "arena_limit_bytes":capacity_plan.cold_peak_bytes(), "warm_extra_bytes":capacity_plan.warm_extra_bytes(),
            "headroom_bytes":capacity_plan.headroom_bytes(),
        },
        "bounds":bounds, "actual_job":actual_job, "native_host_reservation_bytes":charge.bytes(),
        "resource_policy":input.policy, "resource_policy_digest":input.policy_digest,
        "resource":ResourceSeries::new(bounds.host_limit_bytes)?, "native_call_samples_us":[],
        "input_digest":row.input.input_digest, "tokens":TOKEN_LIMIT, "profiling_enabled":false,
        "operation_journal":out.with_extension("operations.jsonl"),
        "original_whole_expires_monotonic_ns":expires_ns, "operation_deadline_ms":120000,
        "session_teardown_observed":false, "measured_peak_device_bytes":null, "measured_transfer_bytes":null,
    });
    if let Some(proof) = &lineage {
        receipt["reference_basis"] = json!("equivalent_predecessor");
        receipt["reference_request"] = json!(proof.request);
        receipt["reference_equivalence"] = json!(proof.equivalence);
    }
    flush_graph_diagnostic(&mut file, &receipt)?;
    let mut journal = operation::Journal::create(
        &out,
        operation::Identity {
            input_sha256: request_pin.sha256,
            profile: profile.manifest,
            process_id: std::process::id(),
            native_binary_sha256: binary,
        },
        input.policy.deadline_ms,
    )?;
    let run = owner.run(|| -> Result<()> {
        let mut custody = Custody {
            scorer:None, device:None, owner:owner.clone(), lease:lease.clone(), charge:Some(charge),
            control:&mut control_parent, files:Some(files),
        };
        ensure!(Instant::now() < whole_deadline, "resource phase expired before admission");
        let files = custody.files.as_ref().unwrap();
        verify_files(files, &profile)?;
        let runtime = airk_host::gpu_runtime::admit(lease.clone(), &root, &manifest, whole_deadline)?;
        ensure!(runtime.manifest_identity() == runtime_manifest.sha256, "admitted runtime differs");
        let adapters = runtime.adapters(whole_deadline)?;
        ensure!(adapters.len() == 1, "Context resource probe requires exactly one observed CUDA adapter");
        let device = runtime.device(adapters[0].uuid, whole_deadline)?;
        custody.device = Some(device.clone());
        receipt["adapter_before"] = json!(device.observation(whole_deadline)?);
        let catalog = airk_kernel::inference_owner::evaluation_catalog(lease.clone(), vec![profile.clone()])?;
        let loaded = journal.observe_resource(operation::Kind::Load, 0, None, expires_ns, |deadline| {
            let execution = catalog.execution_profile(profile.manifest)?
                .bind_cuda_probe(device.clone(), capacity_plan, deadline)?;
            let open = if large {
                Scorer::open_large_bounded_hybrid_resource
            } else {
                Scorer::open_compact_bounded_hybrid_resource
            };
            open(execution, files, &graph_bytes, graph_pin.sha256, whole_deadline)
        })?;
        receipt["load_us"] = json!(loaded.elapsed_us);
        receipt["load_native_call_us"] = json!(loaded.native_call_elapsed_us);
        let mut scorer = loaded.value?;
        ensure!(loaded.within_deadline, "original unprofiled CUDA load expiry exceeded");
        let policy_identity = match &capacity_pin {
            Some(pin) => airk_core::identity::framed_digest(
                b"airk.private.compact-cuda-bounded-hybrid-resource-capacity/1",
                &[scorer.policy_identity().as_bytes(), pin.sha256.as_bytes()],
            ),
            None => airk_core::identity::framed_digest(
                b"airk.private.large-cuda-batch-view-bounded-hybrid-resource-capacity/1",
                &[scorer.policy_identity().as_bytes(), request_pin.sha256.as_bytes()],
            ),
        };
        receipt["policy_identity"] = json!(policy_identity);
        let warmed = journal.observe_resource(
            operation::Kind::Warmup, 0, Some(row.input.input_digest), expires_ns,
            |deadline| scorer.score_with_deadline(&row.input, deadline),
        )?;
        let peak = peak_rss().context("actual native resource RSS unavailable")?;
        receipt["warmup"] = json!({
            "elapsed_us":warmed.elapsed_us, "native_call_elapsed_us":warmed.native_call_elapsed_us,
            "peak_process_rss_bytes":peak, "returned":true, "success":warmed.value.is_ok(),
        });
        flush_graph_diagnostic(&mut file, &receipt)?;
        ensure!(warmed.within_deadline && warmed.elapsed_us <= input.policy.p95_limit_us
            && peak <= bounds.host_limit_bytes, "native warmup exceeded its resource or original expiry");
        verify_numeric(row, &warmed.value?)?;
        let mut series = ResourceSeries::new(bounds.host_limit_bytes)?;
        let mut ordinal = 0;
        let mut native_times = Vec::new();
        let series_result = observe_series(
            &mut series, &input.policy,
            || {
                ordinal += 1;
                let scored = journal.observe_resource(
                    operation::Kind::Sample, ordinal, Some(row.input.input_digest), expires_ns,
                    |deadline| scorer.score_with_deadline(&row.input, deadline),
                )?;
                let peak = peak_rss().context("actual native resource RSS unavailable")?;
                let elapsed_us = scored.elapsed_us;
                let scores = scored.value.map_err(|error| error.context(ObservedNativeFailure {
                    elapsed_us, peak_process_rss_bytes:peak,
                }))?;
                ensure!(scored.within_deadline, "original CUDA sample expiry exceeded");
                verify_numeric(row, &scores)?;
                native_times.push(scored.native_call_elapsed_us);
                Ok(ResourceSample {elapsed_us, peak_process_rss_bytes:peak})
            },
            |series| {
                receipt["resource"] = serde_json::to_value(series)?;
                flush_graph_diagnostic(&mut file, &receipt)
            },
        );
        receipt["native_call_samples_us"] = json!(native_times);
        series_result?;
        scorer.synchronize(whole_deadline)?;
        ensure!(scorer.device_settled(), "Context CUDA resource work remains unsettled");
        receipt["adapter_after"] = json!(device.observation(whole_deadline)?);
        drop(scorer);
        device.synchronize(whole_deadline)?;
        ensure!(!device.poisoned() && !cuda::process_poisoned(), "native resource teardown settlement unknown");
        receipt["session_teardown_observed"] = json!(true);
        verify_files(files, &profile)?;
        for (pin,max) in [
            (&request_pin,1<<20), (&gpu_input,1<<20), (&input.profile,1<<20),
            (&runtime_manifest,128<<10), (&input.source_manifest,PROOF_BYTES),
            (&graph_pin,PROOF_BYTES), (&fixture_pin,PROOF_BYTES), (&producer,PROOF_BYTES),
        ] {
            pinned(pin,max)?;
        }
        if let Some(pin) = &capacity_pin {
            pinned(pin, 4096)?;
        }
        if let Some(proof) = &lineage {
            for pin in [&proof.request, &proof.equivalence, &proof.native_input, &proof.profile_pin] {
                pinned(pin, 1 << 20)?;
            }
        }
        ensure!(Instant::now() < whole_deadline, "original Context resource expiry exceeded during postpins");
        Ok(())
    }).and_then(std::convert::identity);
    drop(owner);
    drop(lease);
    let control_retained = control_parent.is_none();
    let cleanup = control_parent
        .take()
        .map(tempfile::TempDir::close)
        .transpose();
    let mut wait = std::pin::pin!(release.wait());
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let released = matches!(
        std::future::Future::poll(wait.as_mut(), &mut cx),
        std::task::Poll::Ready(Ok(()))
    );
    let poisoned = cuda::process_poisoned();
    let peak = peak_rss();
    receipt["native_control_retained_for_unknown_cuda"] = json!(control_retained);
    receipt["native_control_cleanup_observed"] = json!(matches!(&cleanup, Ok(Some(()))));
    receipt["native_control_cleanup_error"] =
        json!(cleanup.as_ref().err().map(ToString::to_string));
    receipt["native_host_charge_release_observed"] = json!(released);
    receipt["process_cuda_poisoned"] = json!(poisoned);
    receipt["peak_process_rss_bytes"] = json!(peak);
    let result = run.and_then(|()| {
        ensure!(
            receipt["session_teardown_observed"] == true
                && !control_retained
                && matches!(&cleanup, Ok(Some(())))
                && released
                && !poisoned,
            "Context resource teardown or host custody unproven"
        );
        final_host_peak(peak, bounds.host_limit_bytes)?;
        Ok(())
    });
    if let Some(observed) = result
        .as_ref()
        .err()
        .and_then(|e| e.downcast_ref::<ObservedNativeFailure>())
    {
        receipt["native_failure_observation"] = json!({
            "elapsed_us": observed.elapsed_us,
            "peak_process_rss_bytes": observed.peak_process_rss_bytes,
            "native_return_observed": true,
        });
    }
    receipt["complete"] = json!(result.is_ok());
    receipt["error"] = json!(result.as_ref().err().map(|e| format!("{e:#}")));
    receipt["elapsed_us_to_final_report_start"] = json!(started.elapsed().as_micros());
    flush_graph_diagnostic(&mut file, &receipt)?;
    if Instant::now() >= whole_deadline {
        receipt["complete"] = json!(false);
        receipt["final_report_retention_exceeded_original_deadline"] = json!(true);
        flush_graph_diagnostic(&mut file, &receipt)?;
        anyhow::bail!("original Context resource expiry during result retention");
    }
    result
}
