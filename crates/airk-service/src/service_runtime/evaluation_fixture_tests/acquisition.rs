// SPDX-License-Identifier: Apache-2.0
//! Private real-data Store finalization/cache verification and actual I1 controls.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

async fn control(
    control: &Path,
    instance: Option<airk_core::identity::Incarnation>,
    method: ControlMethod,
    work: &Path,
    label: &str,
) -> Result<ControlResponse> {
    write(work, &format!("{label}-request.json"), &method)?;
    // The actual owning IPC adapter supplies its original operation/transport
    // deadlines. No fixture timeout replaces or refreshes those deadlines.
    let result =
        crate::ipc::control::request_once(control, method, env!("CARGO_PKG_VERSION"), instance)
            .await;
    match &result {
        Ok(response) => write(work, &format!("{label}-response.json"), response)?,
        Err(error) => write(
            work,
            &format!("{label}-error.json"),
            &serde_json::json!({"error":format!("{error:#}")}),
        )?,
    }
    result
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "actual current private Large assets, fresh protected work root and exclusive Root-owned disk/CPU window; no model request"]
async fn actual_large_finalization_and_cached_verification_keep_native_control_and_lease_custody()
-> Result<()> {
    let input = PathBuf::from(
        std::env::var_os("AIRK_ACTUAL_ACQUISITION_INPUTS")
            .context("AIRK_ACTUAL_ACQUISITION_INPUTS missing")?,
    );
    let expected: Sha256Digest = std::env::var("AIRK_ACTUAL_ACQUISITION_INPUTS_SHA256")?.parse()?;
    let original = PathBuf::from(
        std::env::var_os("AIRK_ACTUAL_ACQUISITION_WORK")
            .context("AIRK_ACTUAL_ACQUISITION_WORK missing")?,
    );
    ensure!(
        original.is_absolute() && !original.try_exists()?,
        "actual acquisition work must be fresh and absolute"
    );
    let parent = original
        .parent()
        .context("work parent missing")?
        .canonicalize()?;
    let parent_pin = OpenedDirectory::open(&parent)?;
    let work = parent.join(original.file_name().context("work name missing")?);
    let work_pin = crate::ipc::endpoint::create_root_at(
        &parent_pin,
        original.file_name().context("work name missing")?,
    )?;
    let _control_pin = crate::ipc::endpoint::create_root_at(&work_pin, OsStr::new("control"))?;
    let control_root = work.join("control");
    let (inputs, prepared) = prepare(&input, expected)?;
    write(
        &work,
        "inputs.json",
        &serde_json::from_slice::<serde_json::Value>(&inputs)?,
    )?;
    let prepared = Arc::new(prepared);
    let large = prepared
        .iter()
        .find(|item| item.profile.model == ModelChoice::ContextQwenLarge)
        .context("current Large absent")?;
    ensure!(
        large
            .profile
            .assets
            .iter()
            .any(|asset| asset.name == "model.onnx.data" && asset.bytes >= 16_000_000_000),
        "actual Large closure was replaced by a small fixture"
    );
    write(&work, "actual-profile.json", &large.profile)?;
    let manifest = large.profile.manifest;
    let lease = airk_store::service::ServiceLease::acquire(&control_root)?;
    drop(first_start::ensure_catalog(lease)?);
    let mut saved: Option<airk_core::arbiter::Configuration> = None;
    for (ordinal, finalize) in [true, false].into_iter().enumerate() {
        let phase_name = if finalize { "finalize" } else { "cached-pin" };
        let phase_root = work.join(phase_name);
        let _phase_pin = crate::ipc::endpoint::create_root_at(&work_pin, OsStr::new(phase_name))?;
        let source = prepared.clone();
        let prior = saved.clone();
        let restart_configuration = prior.clone();
        let (captured, ready) = tokio::sync::oneshot::channel();
        let factory: EvaluationFactory = Box::new(move |lease, gate, settlement| {
            let owner = if prior.is_some() {
                EvaluationOwner::reopen_same_configuration(
                    lease,
                    gate,
                    source.iter().map(|entry| entry.profile.clone()).collect(),
                    settlement.context("actual acquisition stationary restart proof missing")?,
                )?
            } else {
                EvaluationOwner::new(
                    lease,
                    gate,
                    source.iter().map(|entry| entry.profile.clone()).collect(),
                )?
            };
            let configuration = owner.configuration()?.into_result();
            let large = source
                .iter()
                .find(|entry| entry.profile.manifest == manifest)
                .context("Large manifest changed")?;
            let mut descriptors = |asset: &airk_core::arbiter::Asset| -> Result<File> {
                large
                    .sources
                    .get(&asset.name)
                    .context("retained original source descriptor missing")?
                    .try_clone()
                    .map_err(Into::into)
            };
            let operation = owner.actual_acquisition_fixture(
                manifest,
                if finalize {
                    Some(&mut descriptors)
                } else {
                    None
                },
            )?;
            captured
                .send((operation, configuration))
                .map_err(|_| anyhow::anyhow!("actual acquisition fixture observer closed"))?;
            Ok(owner)
        });
        let root = control_root.clone();
        let mut service = tokio::spawn(async move {
            if let Some(configuration) = restart_configuration {
                let guard: EvaluationStartupGuard = Box::new(move |lease| {
                    EvaluationOwner::stationary_restart(lease, &configuration)
                });
                serve_restarted_evaluation_fixture(&root, factory, guard).await
            } else {
                serve_evaluation_fixture(&root, factory).await
            }
        });
        let mut observed_exit = false;
        let mut shutdown_dispatched = false;
        let mut instance = None;
        let mut worker = None;
        let mut release = None;
        let mut completion = None;
        let mut store_return = None;
        let mut store_return_persistence_attempted = false;
        let mut retained = serde_json::json!({"evaluation":true,"release_qualified":false,"phase":phase_name,"ordinal":ordinal,"passed":false,"native_process_id":std::process::id(),"managed_stop_complete_claimed":false,"service_component_exited":false});
        let mut outcome: Result<()> = async {
            let (operation, configuration) = tokio::select! {
                value=ready=>value.context("actual acquisition factory closed")?,
                value=&mut service=>{observed_exit=true;value??;bail!("service exited before acquisition factory")},
            };
            saved = Some(configuration.clone());
            let health = control(&control_root, None, ControlMethod::Health, &phase_root, "health-before").await?;
            let ControlResponse::Health { observation } = health else { bail!("actual Health refused") };
            ensure!(observation.version == env!("CARGO_PKG_VERSION") && observation.user == local_ipc::current_user()? && observation.process_id == std::process::id() && observation.executable_sha256 == airk_host::runtime_pair::current_executable_sha256()?, "actual Health differs from native test process");
            ensure!(observation.catalog_clock.is_some_and(|clock|clock.namespace==configuration.catalog), "actual catalog differs");
            instance = Some(observation.instance);
            let active = Arc::new(AtomicBool::new(false));
            let (prepared_tx, prepared_rx) = tokio::sync::oneshot::channel();
            let (release_tx, release_rx) = tokio::sync::oneshot::channel();
            let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
            let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
            release = Some(release_tx);
            completion = Some(finished_rx);
            worker = Some(tokio::spawn(operation.run(prepared_tx,release_rx,entered_tx,finished_tx,active.clone())));
            // Fixture preparation has one original 900s bound; no HTTP/model
            // allowance is changed. After expiry we still join actual work.
            let preparation_deadline = tokio::time::Instant::now() + Duration::from_secs(900);
            let cancellation = tokio::time::timeout_at(preparation_deadline, prepared_rx).await.context("actual preparation exceeded original fixture bound")??;
            release.take().context("entry gate missing")?.send(()).map_err(|_|anyhow::anyhow!("actual worker closed before entry"))?;
            let actual_entry = entered_rx.await.context("actual hash entry absent")?;
            retained["actual_hash_entry"] = serde_json::to_value(&actual_entry)?;
            write(&phase_root,"actual-hash-entry.json",&actual_entry)?;
            let began = Instant::now();
            let reply = control(&control_root, instance, ControlMethod::Health, &phase_root, "health-during").await?;
            ensure!(matches!(reply, ControlResponse::Health { .. }) && active.load(Ordering::Acquire), "actual Health did not complete during full Store hashing");
            retained["health_overlap_elapsed_us"] = u64::try_from(began.elapsed().as_micros())?.into();
            ensure!(airk_store::service::ServiceLease::acquire(&control_root).is_err(), "actual lease escaped during hashing");
            let canceled = control(&control_root, instance, ControlMethod::Arbiter { request:airk_core::arbiter::ManagementRequest::Cancel { expected_revision:configuration.revision,stage:airk_core::arbiter::Stage::Context } }, &phase_root, "cancel-during").await?;
            ensure!(matches!(canceled, ControlResponse::Arbiter { .. }) && cancellation.is_cancelled() && active.load(Ordering::Acquire), "actual cancellation did not overlap real native hashing");
            ensure!(!worker.as_ref().context("worker missing")?.is_finished(), "Cancel fabricated worker settlement");
            ensure!(airk_store::service::ServiceLease::acquire(&control_root).is_err(), "Cancel released actual lease");
            // Native service Draining captures its existing absolute30s cutoff.
            // Stopping is request acceptance, never ManagedStopComplete.
            shutdown_dispatched = true;
            let shutdown_began = Instant::now();
            let stopped = control(&control_root, instance, ControlMethod::Shutdown, &phase_root, "shutdown-during").await?;
            ensure!(matches!(stopped, ControlResponse::Stopping), "actual Shutdown refused");
            store_return = Some(completion.take().context("actual Store receiver missing")?.await.context("actual full Store return absent")?);
            let finished = store_return.as_ref().context("actual Store result missing")?;
            ensure!(finished.io_succeeded, "original actual Store I/O failed: {}",finished.io_error.as_deref().unwrap_or("unknown native I/O error"));
            ensure!(finished.phase_error.is_none(), "actual Store phase observation failed after successful I/O: {:?}",finished.phase_error);
            ensure!(finished.original_cancellation_observed && finished.actual_phase == Some(airk_store::service::arbiter_assets::Phase::Ready), "actual Store cancellation/custody observation differs");
            store_return_persistence_attempted = true;
            write(&phase_root,"actual-store-return.json",finished)?;
            let joined = worker.take().context("worker missing")?.await?;
            let canceled = joined.err().context("canceled acquisition could activate after full hash")?;
            ensure!(canceled.to_string() == "Arbiter operation canceled", "worker failed for another reason instead of original cancellation: {canceled:#}");
            retained["actual_worker_joined"] = true.into();
            let exit = (&mut service).await;
            observed_exit = true;
            exit??;
            retained["service_component_exited"] = true.into();
            retained["shutdown_observation_elapsed_us"] = u64::try_from(shutdown_began.elapsed().as_micros())?.into();
            let lease = airk_store::service::ServiceLease::acquire(&control_root)?;
            drop(lease);
            retained["service_lease_reacquired"] = true.into();
            retained["passed"] = true.into();
            Ok(())
        }.await;
        // Preserve actual custody on every failure. No abort, kill, detached
        // blocking hash or manufactured durable lifecycle completion is allowed.
        if let Some(gate) = release.take() {
            let _ = gate.send(());
        }
        if outcome.is_err() && !shutdown_dispatched && !observed_exit {
            let _ = control(
                &control_root,
                instance,
                ControlMethod::Shutdown,
                &phase_root,
                "failure-shutdown",
            )
            .await;
        }
        if let Some(work) = worker.take() {
            let result = work.await;
            retained["worker_join_after_failure"] = match result {
                Ok(Ok(())) => serde_json::json!({"joined":true,"operation_succeeded":true}),
                Ok(Err(error)) => {
                    serde_json::json!({"joined":true,"operation_succeeded":false,"error":format!("{error:#}")})
                }
                Err(error) => serde_json::json!({"joined":true,"task_error":error.to_string()}),
            };
        }
        // The receiver lives outside the fallible control outcome. Join the
        // actual worker first, then recover its real I/O/cancel/phase result
        // even if Health/Cancel/Shutdown failed before the normal observation.
        if store_return.is_none()
            && let Some(receiver) = completion.take()
        {
            match receiver.await {
                Ok(observation) => store_return = Some(observation),
                Err(error) => retained["store_return_unavailable"] = error.to_string().into(),
            }
        }
        if let Some(observation) = &store_return {
            match serde_json::to_value(observation) {
                Ok(value) => retained["actual_store_return"] = value,
                Err(error) => {
                    retained["store_return_encoding_error"] = error.to_string().into();
                    if outcome.is_ok() {
                        outcome = Err(error.into());
                        retained["passed"] = false.into();
                    }
                }
            }
            if !store_return_persistence_attempted
                && let Err(error) = write(&phase_root, "actual-store-return.json", observation)
            {
                retained["store_return_persistence_error"] = format!("{error:#}").into();
                if outcome.is_ok() {
                    outcome = Err(error);
                    retained["passed"] = false.into();
                }
            }
        }
        if !observed_exit {
            let exit = (&mut service).await;
            retained["service_component_exit_after_failure"] =
                serde_json::json!({"joined":true,"success":matches!(exit,Ok(Ok(())))});
        }
        if let Err(error) = &outcome {
            retained["error"] = format!("{error:#}").into();
            retained["lease_reacquisition_after_failed_outcome"] =
                airk_store::service::ServiceLease::acquire(&control_root)
                    .is_ok()
                    .into();
        }
        if let Err(error) = write(&phase_root, "result.json", &retained) {
            if outcome.is_ok() {
                return Err(error);
            }
            eprintln!("Additional acquisition evidence persistence failure: {error:#}");
        }
        outcome?;
    }
    write(
        &work,
        "complete.json",
        &serde_json::json!({"evaluation":true,"release_qualified":false,"phases":2,"fresh_large_copies":1,"no_model_operation_request_dispatched":true,"model_forward_invocations_observed":null,"service_component_exit_and_lease_reacquisition":true,"managed_stop_complete_claimed":false,"native_process_exit_requires_external_runner_observation":true}),
    )?;
    Ok(())
}
