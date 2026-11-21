// SPDX-License-Identifier: Apache-2.0
use super::*;
#[test]
fn gpu_precision_only_restricts_gpu_and_auto_orders_fp32_then_fp16_then_cpu() {
    for (preference, expected) in [
        (GpuPrecisionPreference::Auto, [true, true, true]),
        (GpuPrecisionPreference::Fp32, [true, false, true]),
        (GpuPrecisionPreference::Fp16, [false, true, true]),
    ] {
        assert_eq!(
            [
                preference.admits(Some(DevicePrecision::Float32)),
                preference.admits(Some(DevicePrecision::Float16)),
                preference.admits(None)
            ],
            expected
        );
    }
    // The service no longer ranks candidates itself. The Core planner it calls
    // keeps the same order: FP32 GPU, then FP16 GPU, then CPU.
    let candidate = |id: u8, precision: Option<DevicePrecision>| AdvisoryCandidate {
        manifest: airk_core::identity::Sha256Digest::from_bytes([id; 32]),
        model: SelectedModel::ContextCompact,
        execution_identity: "literal-execution".into(),
        backend: precision.map(|_| GpuBackend::Cuda),
        precision,
        adapter_key: Some(
            if precision.is_some() {
                "adapter"
            } else {
                "cpu"
            }
            .into(),
        ),
        vendor: precision.map(|_| AdapterVendor::Nvidia),
        capability_epoch: 1,
        max_tokens: 2048,
        cpu_cores: 4,
        rejection: None,
    };
    let plan = attempt_order(
        SelectedModel::ContextCompact,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        GpuUse::OptedIn,
        1,
        &[
            candidate(1, None),
            candidate(2, Some(DevicePrecision::Float16)),
            candidate(3, Some(DevicePrecision::Float32)),
        ],
    )
    .unwrap();
    assert_eq!(plan.order, [2, 1, 0]);
    assert_eq!(
        [
            gpu_route_rank(GpuBackend::Cuda, DevicePrecision::Float32),
            gpu_route_rank(GpuBackend::Cuda, DevicePrecision::Float16),
        ],
        [(0, 0), (1, 0)]
    );
}
#[test]
fn known_failed_large_cpu_candidate_is_never_auto_fit_but_model_switch_is_not_invented() {
    assert!(!cpu_model_allowed(ModelChoice::ContextQwenLarge, true));
    assert!(cpu_model_allowed(ModelChoice::ContextQwenLarge, false));
    assert!(cpu_model_allowed(ModelChoice::ContextQwenCompact, true));
    assert!(cpu_model_allowed(ModelChoice::FocusQwenReranker06B, true));
}
#[test]
fn current_and_legacy_management_routes_are_distinct_and_import_never_changes_consent() {
    assert!(current_request(&ManagementRequest::StatusDevice));
    assert!(!current_request(&ManagementRequest::Status));
    assert!(current_request(&ManagementRequest::ConfigureDevice {
        expected_revision: 1,
        stage: Stage::Context,
        configuration: DeviceStageConfiguration::disabled()
    }));
    assert!(!current_request(&ManagementRequest::Cancel {
        expected_revision: 1,
        stage: Stage::Focus
    }));
}
#[cfg(feature = "local-inference")]
fn document() -> DeviceConfiguration {
    DeviceConfiguration::disabled(
        airk_core::identity::Sha256Digest::from_bytes([1; 32]),
        airk_core::identity::Incarnation::from_bytes([2; 16]),
        DeviceCatalogBinding {
            catalog: airk_core::identity::Sha256Digest::from_bytes([3; 32]),
            source: airk_core::identity::Sha256Digest::from_bytes([4; 32]),
        },
    )
}
#[cfg(feature = "local-inference")]
#[test]
fn final_device_delivery_refuses_generation_revision_and_compiled_source_binding_changes()
-> Result<()> {
    let document = document();
    let shared = Arc::new(Shared::default());
    shared.install(
        document.clone(),
        Stage::Context,
        None,
        None,
        None,
        None,
        None,
    )?;
    let generation = shared.generation.load(std::sync::atomic::Ordering::Acquire);
    let delivery = Delivery {
        binding: document.device_catalog.clone(),
        policy: NativeConfiguration::device(document.clone(), vec![])?,
        shared: shared.clone(),
        generation,
        devices: vec![],
        eligibilities: BTreeMap::new(),
    };
    delivery.current(&document)?;
    let mut changed = document.clone();
    changed.revision += 1;
    assert!(delivery.current(&changed).is_err());
    let mut changed = document.clone();
    changed.device_catalog.source = airk_core::identity::Sha256Digest::from_bytes([5; 32]);
    assert!(delivery.current(&changed).is_err());
    shared
        .generation
        .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    assert!(delivery.current(&document).is_err());
    Ok(())
}
#[cfg(feature = "local-inference")]
#[test]
fn changed_device_document_does_not_reuse_cached_native_selection_as_authority() -> Result<()> {
    let shared = Shared::default();
    let document = document();
    let first = shared.install(document.clone(), Stage::Focus, None, None, None, None, None)?;
    assert!(first.same_document(&shared.policy(document.clone())?));
    let generation = shared.generation.load(std::sync::atomic::Ordering::Acquire);
    let mut current = document;
    current.revision += 1;
    current.context.device_policy = DevicePolicy::Cpu;
    let next = shared.policy(current.clone())?;
    assert!(!first.same_document(&next));
    assert!(next.selected_profile(Stage::Focus).is_none());
    shared.install(current, Stage::Context, None, None, None, None, None)?;
    assert_eq!(
        shared.generation.load(std::sync::atomic::Ordering::Acquire),
        generation + 1
    );
    Ok(())
}

#[cfg(feature = "local-inference")]
#[test]
fn retained_same_resident_reactivation_restores_ready_only_after_exact_validation() -> Result<()> {
    use airk_inference::inference_engine::pool::arbiter::Observation;
    let profile = runtime::catalog_profiles()?
        .into_iter()
        .find(|p| p.model == ModelChoice::ContextQwenCompact)
        .context("compiled Compact description absent")?;
    let mut doc = document();
    doc.context.enabled = true;
    doc.context.selected_model = Some(SelectedModel::ContextCompact);
    doc.context.indexes = vec![airk_core::arbiter::Enrollment {
        index_id: airk_core::index_catalog::IndexId::from_identity(
            airk_core::identity::Incarnation::from_bytes([31; 16]),
        ),
        tasks: profile.tasks.clone(),
    }];
    let shared = Shared::default();
    let actual = Observation {
        configuration_revision: Some(doc.revision),
        active_profile: Some(profile.manifest),
        warming_profile: None,
        queued: 0,
        running: 0,
    };
    for _ in 0..3 {
        shared.install(
            doc.clone(),
            Stage::Context,
            Some(profile.clone()),
            None,
            None,
            None,
            None,
        )?;
        assert!(!ready_from_observation(
            &shared.state.lock().unwrap(),
            &doc,
            Stage::Context,
            Some(&actual),
            true
        ));
        assert!(
            shared
                .finish_activation(
                    &doc,
                    Stage::Context,
                    &profile,
                    BTreeMap::new(),
                    || anyhow::bail!("retained native validation failed")
                )
                .is_err()
        );
        assert!(!shared.state.lock().unwrap().ready.contains(&Stage::Context));
        shared.finish_activation(&doc, Stage::Context, &profile, BTreeMap::new(), || Ok(()))?;
        assert!(ready_from_observation(
            &shared.state.lock().unwrap(),
            &doc,
            Stage::Context,
            Some(&actual),
            true
        ));
        assert!(!ready_from_observation(
            &shared.state.lock().unwrap(),
            &doc,
            Stage::Context,
            Some(&actual),
            false
        ));
        let changed = Observation {
            configuration_revision: Some(doc.revision + 1),
            active_profile: Some(profile.manifest),
            warming_profile: None,
            queued: 0,
            running: 0,
        };
        assert!(!ready_from_observation(
            &shared.state.lock().unwrap(),
            &doc,
            Stage::Context,
            Some(&changed),
            true
        ));
    }
    Ok(())
}
#[cfg(feature = "local-inference")]
#[test]
fn device_selection_install_cannot_cross_final_effect_and_later_commit_refuses() -> Result<()> {
    let doc = document();
    let shared = Arc::new(Shared::default());
    let policy = shared.install(doc.clone(), Stage::Context, None, None, None, None, None)?;
    let delivery = Delivery {
        binding: doc.device_catalog.clone(),
        policy,
        shared: shared.clone(),
        generation: shared.generation.load(std::sync::atomic::Ordering::Acquire),
        devices: vec![],
        eligibilities: BTreeMap::new(),
    };
    std::thread::scope(|threads| -> Result<()> {
        let (entered, observed) = std::sync::mpsc::channel();
        let install = delivery.with_selection(&doc, &[], || {
            assert!(
                shared.state.try_lock().is_err(),
                "selection owner lock must survive the last sample through the effect"
            );
            let shared = shared.clone();
            let doc = doc.clone();
            let thread = threads.spawn(move || -> Result<()> {
                entered.send(shared.state.try_lock().is_err())?;
                shared.install(doc, Stage::Context, None, None, None, None, None)?;
                Ok(())
            });
            assert!(observed.recv_timeout(Duration::from_secs(5))?);
            Ok(thread)
        })?;
        install
            .join()
            .map_err(|_| anyhow::anyhow!("selection installer panicked"))??;
        Ok(())
    })?;
    let effects = std::cell::Cell::new(0);
    assert!(
        delivery
            .with_selection(&doc, &[], || {
                effects.set(effects.get() + 1);
                Ok(())
            })
            .is_err()
    );
    assert_eq!(effects.get(), 0);
    Ok(())
}
#[cfg(feature = "local-inference")]
#[test]
fn initialization_mutation_owns_both_stage_admission_fences_and_refuses_pending_unknown_or_source_race()
-> Result<()> {
    let focus = Mutex::new(super::super::lifecycle::State::default());
    let context = Mutex::new(super::super::lifecycle::State::default());
    let devices = Shared::default();
    let effects = std::cell::Cell::new(0);
    context.lock().unwrap().device_tasks = 1;
    assert!(
        with_initialization_fence(
            &focus,
            &context,
            &devices,
            || Ok(()),
            |_| {
                effects.set(1);
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(effects.get(), 0);
    context.lock().unwrap().device_tasks = 0;
    context.lock().unwrap().phase = Availability::Draining;
    assert!(
        with_initialization_fence(
            &focus,
            &context,
            &devices,
            || Ok(()),
            |_| {
                effects.set(1);
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(effects.get(), 0);
    context.lock().unwrap().phase = Availability::Disabled;
    let resources = airk_kernel::service_resources::Resources::new(
        airk_core::service_resources::ResourceConfiguration::default(),
        8,
        Some(64 << 30),
    )?;
    let charge = resources.reserve_optional(
        airk_core::identity::Incarnation::from_bytes([67; 16]),
        1,
        Some(8 << 30),
    )?;
    context.lock().unwrap().charges.push((
        airk_core::identity::Sha256Digest::from_bytes([68; 32]),
        charge.release_observer(),
    ));
    assert!(
        with_initialization_fence(
            &focus,
            &context,
            &devices,
            || Ok(()),
            |_| {
                effects.set(1);
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(effects.get(), 0);
    drop(charge);
    let source_live = std::cell::Cell::new(true);
    assert!(
        with_initialization_fence(
            &focus,
            &context,
            &devices,
            || {
                ensure!(source_live.get(), "actual source revoked");
                Ok(())
            },
            |check| {
                assert!(focus.try_lock().is_err());
                assert!(context.try_lock().is_err());
                check()?;
                source_live.set(false);
                check()?;
                effects.set(1);
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(effects.get(), 0);
    with_initialization_fence(
        &focus,
        &context,
        &devices,
        || Ok(()),
        |check| {
            check()?;
            effects.set(1);
            Ok(())
        },
    )?;
    assert_eq!(effects.get(), 1);
    Ok(())
}
#[cfg(feature = "local-inference")]
#[test]
fn cancel_clears_queued_command_and_revokes_idle_fault_notifier_without_restarting_after_settlement()
-> Result<()> {
    // This same generic bounded map transition is used on the actual Launch slot.
    let mut pending = BTreeMap::from([(Stage::Context, "queued B"), (Stage::Focus, "other stage")]);
    let mut generations = BTreeMap::new();
    continuation_current(&generations, Stage::Context, 0)?;
    cancel_continuations(&mut pending, &mut generations, Stage::Context)?;
    assert_eq!(
        pending.remove(&Stage::Context),
        None,
        "A settling cannot discover B"
    );
    assert_eq!(pending.get(&Stage::Focus), Some(&"other stage"));
    assert!(
        continuation_current(&generations, Stage::Context, 0).is_err(),
        "popped old command/notifier cannot requeue or start"
    );
    let shared = Shared::default();
    let mut idle = super::super::lifecycle::State::default();
    let old_notifier = idle.cancel.clone();
    cancel_device_stage(&mut idle, &shared, Stage::Context)?;
    assert!(old_notifier.is_cancelled());
    assert!(!idle.busy);
    assert!(
        continuation_current(
            &shared.state.lock().unwrap().intent_generation,
            Stage::Context,
            0
        )
        .is_err()
    );
    let lifecycle = Arc::new(Mutex::new(idle));
    assert!(
        super::super::lifecycle::start_checked(
            &lifecycle,
            airk_core::identity::Sha256Digest::from_bytes([51; 32]),
            None,
            || continuation_current(
                &shared.state.lock().unwrap().intent_generation,
                Stage::Context,
                0
            )
        )
        .is_err()
    );
    assert!(
        !lifecycle.lock().unwrap().busy,
        "refused canceled command never becomes active"
    );
    Ok(())
}

#[cfg(feature = "local-inference")]
#[test]
fn read_only_device_stamp_fences_intent_selection_ready_and_generation_without_runtime_calls()
-> Result<()> {
    let shared = Shared::default();
    let original = shared.read_diagnostic_stamp()?;
    shared
        .state
        .lock()
        .unwrap()
        .intent_generation
        .insert(Stage::Context, 1);
    let intent = shared.read_diagnostic_stamp()?;
    assert_ne!(intent, original);
    shared.state.lock().unwrap().ready.insert(Stage::Context);
    let ready = shared.read_diagnostic_stamp()?;
    assert_ne!(ready, intent);
    shared.state.lock().unwrap().selection.insert(
        Stage::Context,
        DeviceSelection {
            schema: DEVICE_STATUS_SCHEMA.into(),
            model: SelectedModel::ContextCompact,
            policy: DevicePolicy::Auto,
            gpu_precision: GpuPrecisionPreference::Auto,
            selected: None,
            reasons: vec![],
            operation_failure: Some(SelectionReason::DeviceMemoryInsufficient),
        },
    );
    let selection = shared.read_diagnostic_stamp()?;
    assert_ne!(selection, ready);
    shared
        .generation
        .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    assert_ne!(shared.read_diagnostic_stamp()?, selection);
    assert!(!prework_device_failure(&anyhow::anyhow!(
        "source authority revoked"
    )));
    assert!(!prework_device_failure(&anyhow::anyhow!(
        "independent fixture malformed"
    )));
    assert!(!prework_device_failure(&anyhow::anyhow!(
        "original deadline expired"
    )));
    Ok(())
}

#[cfg(feature = "local-inference")]
#[test]
fn delivery_ignores_unrelated_stale_gpu_proof_but_rejects_the_same_expected_gpu_and_mixed_roster() {
    // Data-only stand-ins test the exact capture filter; no GPU proof is minted.
    let gpu = Sha256Digest::from_bytes([91; 32]);
    let cpu = Sha256Digest::from_bytes([92; 32]);
    let stale = BTreeMap::from([(Stage::Context, (gpu, false))]);
    let cpu_only = expected_eligibilities(&stale, [(Stage::Focus, Some(cpu))], |proof| proof.0);
    assert!(cpu_only.is_empty());
    assert!(cpu_only.values().all(|proof| proof.1));
    let same_gpu = expected_eligibilities(&stale, [(Stage::Context, Some(gpu))], |proof| proof.0);
    assert_eq!(same_gpu, BTreeMap::from([(Stage::Context, (gpu, false))]));
    assert!(!same_gpu.values().all(|proof| proof.1));
    let mixed = expected_eligibilities(
        &stale,
        [(Stage::Focus, Some(cpu)), (Stage::Context, Some(gpu))],
        |proof| proof.0,
    );
    assert!(!mixed.values().all(|proof| proof.1));
    assert!(
        expected_eligibilities(&stale, [(Stage::Context, None)], |proof| proof.0).is_empty(),
        "unenrolled index cannot inherit a global GPU requirement"
    );
    assert!(
        expected_eligibilities(&stale, [(Stage::Context, Some(cpu))], |proof| proof.0).is_empty(),
        "another profile is not the expected proof"
    );
}
#[cfg(feature = "local-inference")]
#[test]
fn retained_and_cold_fault_attribution_share_typed_settled_rules_and_preserve_actual_resident() {
    let own = Sha256Digest::from_bytes([93; 32]);
    let other = Sha256Digest::from_bytes([94; 32]);
    assert_eq!(recordable_fault_profile(true, false, None, own), Some(own));
    assert_eq!(
        recordable_fault_profile(true, false, Some(other), own),
        Some(other)
    );
    assert_eq!(recordable_fault_profile(true, true, Some(other), own), None);
    assert_eq!(
        recordable_fault_profile(false, false, Some(other), own),
        None
    );
    assert!(!prework_device_failure(&anyhow::anyhow!(
        "source authority revoked"
    )));
    assert!(!prework_device_failure(&anyhow::anyhow!(
        "original management deadline expired"
    )));
}

#[cfg(feature = "local-inference")]
#[test]
fn retained_population_precheck_includes_other_failed_gpu_and_excludes_drained_self_or_cpu() {
    let failed = Sha256Digest::from_bytes([95; 32]);
    let current = Sha256Digest::from_bytes([96; 32]);
    let targets = retained_gpu_targets(
        [(Stage::Focus, Some(failed)), (Stage::Context, None)],
        [
            (Stage::Focus, failed, true),
            (Stage::Context, current, true),
        ],
    );
    assert_eq!(
        targets,
        [(Stage::Focus, failed)],
        "recovering Context cannot omit the actual retained failed Focus"
    );
    let circuits = std::collections::BTreeSet::from([failed]);
    assert!(
        !targets
            .iter()
            .all(|(_, profile)| !circuits.contains(profile))
    );
    let after_self_drain = retained_gpu_targets(
        [(Stage::Focus, None), (Stage::Context, Some(current))],
        [
            (Stage::Focus, failed, true),
            (Stage::Context, current, true),
        ],
    );
    assert_eq!(after_self_drain, [(Stage::Context, current)]);
    assert!(
        after_self_drain
            .iter()
            .all(|(_, profile)| !circuits.contains(profile)),
        "old drained self selection cannot prevent its replacement"
    );
    assert!(
        retained_gpu_targets(
            [(Stage::Focus, Some(failed))],
            [(Stage::Focus, failed, false)]
        )
        .is_empty()
    );
}

#[cfg(feature = "local-inference")]
#[test]
fn secondary_fault_recorder_error_keeps_original_typed_cause() {
    #[derive(Debug)]
    struct NativeCause;
    impl std::fmt::Display for NativeCause {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("actual native cause")
        }
    }
    impl std::error::Error for NativeCause {}
    let error = secondary_fault_error(
        anyhow::Error::new(NativeCause),
        anyhow::anyhow!("recorder lock poisoned"),
    );
    assert!(error.downcast_ref::<NativeCause>().is_some());
    assert!(format!("{error:#}").contains("recorder lock poisoned"));
}

#[test]
fn idle_stage_reports_unsupported_system_and_constrained_and_every_other_phase_as_requested() {
    for (phase, reported) in [
        (Availability::Disabled, Availability::Requested),
        (Availability::IndexDisabled, Availability::Requested),
        (Availability::Unqualified, Availability::Requested),
        (
            Availability::UnsupportedSystem,
            Availability::UnsupportedSystem,
        ),
        (Availability::UnsupportedTask, Availability::Requested),
        (Availability::MissingAssets, Availability::Requested),
        (Availability::Requested, Availability::Requested),
        (Availability::Downloading, Availability::Requested),
        (Availability::Verifying, Availability::Requested),
        (Availability::Warming, Availability::Requested),
        (Availability::Ready, Availability::Requested),
        (Availability::Constrained, Availability::Constrained),
        (Availability::Draining, Availability::Requested),
        (Availability::Failed, Availability::Requested),
    ] {
        assert_eq!(settled_phase(phase), reported, "{phase:?}");
    }
}

#[test]
fn missing_assets_are_never_reported_as_an_unsupported_system() {
    assert_eq!(no_selection_phase(false), Availability::UnsupportedSystem);
    assert_eq!(no_selection_phase(true), Availability::MissingAssets);
    // An idle stage in that phase reports an unfulfilled request, never
    // `unsupported_system`, which clients treat as a model they cannot use.
    assert_eq!(
        settled_phase(no_selection_phase(true)),
        Availability::Requested
    );
    assert_eq!(
        no_selection_reason(false, "Typed sentence.".into()),
        "Typed sentence."
    );
    assert_eq!(
        no_selection_reason(true, "Typed sentence.".into()),
        "Verified assets for a candidate of this model are not installed, so it was not tried. Typed sentence."
    );
}

#[test]
fn remaining_time_is_zero_at_and_after_the_original_deadline() {
    let now = Instant::now();
    assert_eq!(remaining_us(now, now), 0);
    assert_eq!(remaining_us(now, now + Duration::from_secs(1)), 0);
    assert_eq!(remaining_us(now + Duration::from_micros(7), now), 7);
    assert_eq!(
        remaining_us(now + Duration::from_secs(120), now),
        120_000_000
    );
}

#[cfg(feature = "local-inference")]
#[test]
fn device_layer_starts_with_gpu_use_off_until_the_service_applies_its_configuration() -> Result<()>
{
    use std::sync::atomic::Ordering;
    let shared = Shared::default();
    assert!(!shared.gpu_allowed());
    assert_eq!(shared.gpu_use(), GpuUse::NotOptedIn);
    let generation = shared.generation.load(Ordering::Acquire);
    // A stored "off", or no applied configuration at all, changes nothing.
    assert!(!shared.apply_gpu_allowed(false));
    assert!(!shared.gpu_allowed());
    // Only an applied opt-in turns GPU use on, and turning it off closes it again.
    assert!(shared.apply_gpu_allowed(true));
    assert!(shared.gpu_allowed());
    assert_eq!(shared.gpu_use(), GpuUse::OptedIn);
    assert!(!shared.apply_gpu_allowed(true));
    assert!(shared.apply_gpu_allowed(false));
    assert!(!shared.gpu_allowed());
    assert_eq!(shared.generation.load(Ordering::Acquire), generation);
    // The owner the service builds starts closed for both stages.
    let root = tempfile::tempdir()?;
    let owner = super::super::Owner::new(
        Arc::new(airk_store::service::ServiceLease::acquire(root.path())?),
        airk_catalog_control::catalog_admission::NativeCatalogAdmission::new(
            airk_core::identity::Incarnation::from_bytes([81; 16]),
            airk_core::identity::Incarnation::from_bytes([82; 16]),
            airk_core::index_catalog::CatalogRevision::INITIAL,
            1,
        )?,
    )?;
    assert!(!owner.context.devices.gpu_allowed());
    assert!(!owner.focus.devices.gpu_allowed());
    assert!(Arc::ptr_eq(&owner.context.devices, &owner.focus.devices));
    Ok(())
}

#[cfg(feature = "local-inference")]
#[test]
fn gpu_off_is_reported_as_not_opted_in_and_an_explicit_cpu_policy_as_forced_cpu() -> Result<()> {
    let model = SelectedModel::ContextCompact;
    let catalog = runtime::catalog_profiles()?;
    // The service's own candidates for the compiled CPU tiers of this model.
    let tiers = catalog
        .iter()
        .filter(|profile| profile.model == model.model() && profile.device_execution.is_none())
        .map(|profile| advisory_candidate(model, profile, Some(("cpu".into(), 1)), None))
        .collect::<Result<Vec<_>>>()?;
    assert!(!tiers.is_empty(), "compiled Compact CPU tier absent");
    for tier in &tiers {
        assert_eq!(tier.adapter_key.as_deref(), Some("cpu"));
        assert_eq!(
            (tier.backend, tier.precision, tier.vendor),
            (None, None, None)
        );
    }
    let shared = Shared::default();
    let global_rows = |policy: DevicePolicy| -> Result<Vec<CandidateRejection>> {
        let plan = attempt_order(
            model,
            policy,
            GpuPrecisionPreference::Auto,
            shared.gpu_use(),
            1,
            &tiers,
        )?;
        assert_eq!(plan.order.len(), tiers.len());
        Ok(plan.reasons)
    };
    let global = |reason| CandidateRejection {
        manifest: None,
        adapter_key: None,
        reason,
    };
    // GPU use is off by default: Auto says so, and CPU policy keeps its own reason.
    assert_eq!(
        global_rows(DevicePolicy::Auto)?,
        [global(SelectionReason::GpuNotOptedIn)]
    );
    assert_eq!(
        global_rows(DevicePolicy::Cpu)?,
        [global(SelectionReason::ForcedCpu)]
    );
    // GPU use on with no GPU candidate: no qualified route, never "no GPU".
    assert!(shared.apply_gpu_allowed(true));
    assert_eq!(
        global_rows(DevicePolicy::Auto)?,
        [global(SelectionReason::NoQualifiedGpuRoute)]
    );
    assert_eq!(
        global_rows(DevicePolicy::Cpu)?,
        [global(SelectionReason::ForcedCpu)]
    );
    Ok(())
}

#[cfg(feature = "local-inference")]
#[test]
fn no_selection_records_its_rows_and_reports_unsupported_system_and_a_stopped_operation_reports_failed()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let owner = super::super::Owner::new(
        Arc::new(airk_store::service::ServiceLease::acquire(root.path())?),
        airk_catalog_control::catalog_admission::NativeCatalogAdmission::new(
            airk_core::identity::Incarnation::from_bytes([81; 16]),
            airk_core::identity::Incarnation::from_bytes([82; 16]),
            airk_core::index_catalog::CatalogRevision::INITIAL,
            1,
        )?,
    )?;
    let profile = runtime::catalog_profiles()?
        .into_iter()
        .find(|p| p.model == ModelChoice::ContextQwenCompact)
        .context("compiled Compact description absent")?;
    let mut doc = document();
    doc.context.enabled = true;
    doc.context.selected_model = Some(SelectedModel::ContextCompact);
    doc.context.indexes = vec![airk_core::arbiter::Enrollment {
        index_id: airk_core::index_catalog::IndexId::from_identity(
            airk_core::identity::Incarnation::from_bytes([31; 16]),
        ),
        tasks: profile.tasks.clone(),
    }];
    let stage = &owner.context;
    // Nothing has run: an enabled stage without a resident is a request.
    let status = stage.device_stage_status(&doc)?;
    assert_eq!(status.availability, Availability::Requested);
    assert!(status.selection.is_none());

    // The order ended with nothing selected.
    let rows = vec![
        CandidateRejection {
            manifest: None,
            adapter_key: None,
            reason: SelectionReason::GpuNotOptedIn,
        },
        CandidateRejection {
            manifest: Some(profile.manifest),
            adapter_key: Some("cpu".into()),
            reason: SelectionReason::HostMemoryInsufficient,
        },
    ];
    let no_fit = DeviceSelection {
        schema: DEVICE_STATUS_SCHEMA.into(),
        model: SelectedModel::ContextCompact,
        policy: DevicePolicy::Auto,
        gpu_precision: GpuPrecisionPreference::Auto,
        selected: None,
        reasons: rows.clone(),
        operation_failure: None,
    };
    let reason = disclosure(&no_fit);
    assert!(
        reason.starts_with(
            "Context Compact is not active. No admitted route fits this system. GPU use is not turned on in service resources. The "
        ),
        "{reason}"
    );
    assert!(
        reason.ends_with(" was not used: host_memory_insufficient."),
        "{reason}"
    );
    stage.devices.install(
        doc.clone(),
        Stage::Context,
        None,
        Some(no_fit),
        None,
        None,
        None,
    )?;
    {
        let mut life = stage.lifecycle.lock().unwrap();
        life.phase = Availability::UnsupportedSystem;
        life.reason = reason.clone();
    }
    let status = stage.device_stage_status(&doc)?;
    assert_eq!(status.availability, Availability::UnsupportedSystem);
    assert_eq!(status.reason, reason);
    assert_eq!(status.active_profile, None);
    let recorded = status.selection.context("no-selection record missing")?;
    assert!(recorded.selected.is_none());
    assert_eq!(recorded.operation_failure, None);
    assert_eq!(recorded.reasons, rows);
    // The refused tier shows its typed cause when the model is admitted here.
    let variant = status
        .models
        .iter()
        .flat_map(|model| &model.variants)
        .find(|variant| variant.manifest == profile.manifest)
        .context("compiled Compact variant missing from status")?;
    let expected = if variant.model_admission == ModelAdmissionStatus::Unavailable {
        variant_disclosure(&profile, VariantState::NotAdmitted, None)
    } else {
        variant_disclosure(
            &profile,
            VariantState::Admitted,
            Some(SelectionReason::HostMemoryInsufficient),
        )
    };
    assert_eq!(variant.reason, expected);
    // No native selection or readiness is held for the stage.
    {
        let state = stage.devices.state.lock().unwrap();
        assert!(!state.selected.contains_key(&Stage::Context));
        assert!(!state.ready.contains(&Stage::Context));
    }
    // The other stage keeps its own state.
    let focus = owner.focus.device_stage_status(&doc)?;
    assert_eq!(focus.availability, Availability::Disabled);
    assert!(focus.selection.is_none());

    // A stopped operation records its typed failure and reports failed.
    let generation = stage
        .devices
        .generation
        .load(std::sync::atomic::Ordering::Acquire);
    let mut stopped_rows = rows.clone();
    let stopped = DeviceSelection {
        schema: DEVICE_STATUS_SCHEMA.into(),
        model: SelectedModel::ContextCompact,
        policy: DevicePolicy::Auto,
        gpu_precision: GpuPrecisionPreference::Auto,
        selected: None,
        reasons: rows_with(&mut stopped_rows, SelectionReason::DeadlineExpired),
        operation_failure: Some(SelectionReason::DeadlineExpired),
    };
    let mut other = doc.clone();
    other.revision += 1;
    assert!(
        !stage
            .devices
            .record_operation_failure(&other, Stage::Context, stopped.clone()),
        "another document's failure cannot replace this record"
    );
    let mut selecting = stopped.clone();
    selecting.operation_failure = None;
    assert!(
        !stage
            .devices
            .record_operation_failure(&doc, Stage::Context, selecting)
    );
    assert!(
        stage
            .devices
            .record_operation_failure(&doc, Stage::Context, stopped.clone())
    );
    assert_eq!(
        stage
            .devices
            .generation
            .load(std::sync::atomic::Ordering::Acquire),
        generation,
        "a diagnostic record changes no selected variant"
    );
    {
        let mut life = stage.lifecycle.lock().unwrap();
        life.phase = Availability::Failed;
        life.reason = disclosure(&stopped);
    }
    let status = stage.device_stage_status(&doc)?;
    assert_eq!(status.availability, Availability::Failed);
    assert!(
        status.reason.starts_with(
            "Context Compact is not active. The operation stopped and nothing further was tried: deadline_expired."
        ),
        "{}",
        status.reason
    );
    let recorded = status.selection.context("stopped record missing")?;
    assert!(recorded.selected.is_none());
    assert_eq!(
        recorded.operation_failure,
        Some(SelectionReason::DeadlineExpired)
    );
    assert_eq!(recorded.reasons.len(), 3);
    assert_eq!(
        recorded.reasons[2],
        CandidateRejection {
            manifest: None,
            adapter_key: None,
            reason: SelectionReason::DeadlineExpired,
        }
    );
    // A resident stage keeps the selection it is serving.
    stage
        .devices
        .state
        .lock()
        .unwrap()
        .ready
        .insert(Stage::Context);
    let mut later = stopped.clone();
    later.operation_failure = Some(SelectionReason::AuthorityChanged);
    assert!(
        !stage
            .devices
            .record_operation_failure(&doc, Stage::Context, later)
    );
    assert_eq!(
        stage.devices.state.lock().unwrap().selection[&Stage::Context].operation_failure,
        Some(SelectionReason::DeadlineExpired)
    );
    Ok(())
}

#[cfg(feature = "local-inference")]
#[test]
fn stopped_rows_end_with_the_typed_cause_and_stay_within_the_row_cap() {
    let row = CandidateRejection {
        manifest: None,
        adapter_key: None,
        reason: SelectionReason::HostMemoryInsufficient,
    };
    for (before, kept) in [(0, 0), (1, 1), (19, 19), (20, 19), (40, 19)] {
        let mut rows = vec![row.clone(); before];
        let closed = rows_with(&mut rows, SelectionReason::SettlementUnknown);
        assert!(rows.is_empty());
        assert_eq!(closed.len(), kept + 1, "{before} rows");
        assert_eq!(
            closed.last().map(|row| row.reason),
            Some(SelectionReason::SettlementUnknown)
        );
        assert!(closed[..kept].iter().all(|kept| *kept == row));
    }
}
