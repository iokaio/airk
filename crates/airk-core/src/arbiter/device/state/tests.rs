// SPDX-License-Identifier: Apache-2.0
use super::*;
fn binding() -> DeviceCatalogBinding {
    DeviceCatalogBinding {
        catalog: Sha256Digest::from_bytes([10; 32]),
        source: Sha256Digest::from_bytes([11; 32]),
    }
}
fn configuration() -> DeviceConfiguration {
    DeviceConfiguration::disabled(
        Sha256Digest::from_bytes([1; 32]),
        Incarnation::from_bytes([2; 16]),
        binding(),
    )
}
fn stage(stage: Stage, configuration: &DeviceStageConfiguration) -> DeviceStageStatus {
    let mut value = DeviceStageStatus {
        selected_model: configuration.selected_model,
        device_policy: configuration.device_policy,
        gpu_precision: configuration.gpu_precision,
        active_profile: None,
        availability: Availability::Disabled,
        reason: "Literal native-contract structural fixture; no execution authority".into(),
        queued: 0,
        running: 0,
        selection: None,
        models: match stage {
            Stage::Focus => vec![DeviceModelStatus {
                model: SelectedModel::Focus,
                availability: Availability::Unqualified,
                reason: "Synthetic no compiled variant; no qualification".into(),
                tasks: BTreeSet::new(),
                variants: vec![],
            }],
            Stage::Context => vec![
                DeviceModelStatus {
                    model: SelectedModel::ContextCompact,
                    availability: Availability::Unqualified,
                    reason: "Synthetic no compiled variant; no qualification".into(),
                    tasks: BTreeSet::new(),
                    variants: vec![],
                },
                DeviceModelStatus {
                    model: SelectedModel::ContextLarge,
                    availability: Availability::Unqualified,
                    reason: "Synthetic no compiled variant; no qualification".into(),
                    tasks: BTreeSet::new(),
                    variants: vec![],
                },
            ],
        },
    };
    for model in &mut value.models {
        model.tasks = crate::arbiter::stage_tests::profile_status(
            model.model.model(),
            Availability::Unqualified,
        )
        .tasks;
    }
    value
}
fn status() -> DeviceStatus {
    let configuration = configuration();
    DeviceStatus {
        schema: "airk.arbiter.status/3".into(),
        initialization: None,
        focus: stage(Stage::Focus, &configuration.focus),
        context: stage(Stage::Context, &configuration.context),
        configuration,
    }
}
#[test]
fn config4_canonical_identity_and_catalog_binding_are_exact() -> Result<()> {
    let value = configuration();
    let bytes = value.canonical_bytes()?;
    assert_eq!(
        DeviceConfiguration::decode(&bytes, value.installation, value.catalog, &binding())?,
        value
    );
    let mut wrong = binding();
    wrong.source = Sha256Digest::from_bytes([12; 32]);
    assert!(
        DeviceConfiguration::decode(&bytes, value.installation, value.catalog, &wrong).is_err()
    );
    assert!(
        DeviceConfiguration::decode(
            &serde_json::to_vec_pretty(&value)?,
            value.installation,
            value.catalog,
            &binding()
        )
        .is_err()
    );
    let mut json = serde_json::to_value(&value)?;
    json["configuration_schema"] = serde_json::json!("claimed-extra");
    assert!(serde_json::from_value::<DeviceConfiguration>(json).is_err());
    assert!(Configuration::decode(&bytes, value.installation, value.catalog).is_err());
    Ok(())
}
#[test]
fn status3_preserves_model_slots_and_never_projects_gpu_as_legacy_cpu() -> Result<()> {
    let value = status();
    value.validate()?;
    let bytes = serde_json::to_vec(&value)?;
    assert!(serde_json::from_slice::<Status>(&bytes).is_err());
    let mut wrong = value.clone();
    wrong.context.models.pop();
    assert!(wrong.validate().is_err());
    let mut wrong = value.clone();
    wrong.context.models.push(DeviceModelStatus {
        model: SelectedModel::Focus,
        availability: Availability::Unqualified,
        reason: "Synthetic no compiled variant; no qualification".into(),
        tasks: BTreeSet::new(),
        variants: vec![],
    });
    assert!(wrong.validate().is_err());
    let mut wrong = value.clone();
    wrong.focus.availability = Availability::Ready;
    assert!(wrong.validate().is_err());
    let mut wrong = value;
    wrong.configuration.focus.device_policy = DevicePolicy::Cpu;
    assert!(wrong.validate().is_err());
    Ok(())
}
#[test]
fn management_current_and_legacy_fields_do_not_cross_decode() -> Result<()> {
    let current = r#"{"operation":"configure_device","stage":"context","expected_revision":1,"configuration":{"enabled":false,"selected_model":"context_large","device_policy":"auto","gpu_precision":"fp16","indexes":[]}}"#;
    let request: ManagementRequest = serde_json::from_str(current)?;
    assert!(matches!(request, ManagementRequest::ConfigureDevice { .. }));
    let wrong = current.replace("configure_device", "configure");
    assert!(serde_json::from_str::<ManagementRequest>(&wrong).is_err());
    let legacy = r#"{"operation":"configure","stage":"context","expected_revision":1,"enabled":false,"selected_profile":null,"indexes":[]}"#;
    assert!(matches!(
        serde_json::from_str::<ManagementRequest>(legacy)?,
        ManagementRequest::Configure { .. }
    ));
    assert!(
        serde_json::from_str::<ManagementRequest>(&legacy.replace("configure", "configure_device"))
            .is_err()
    );
    Ok(())
}
#[test]
fn current_private_import_retains_native_role_and_full_frame_limits() -> Result<()> {
    use crate::{
        native_offline_import::NativeOfflineImportRequest,
        service::{ControlMethod, ControlResponse},
    };
    let request = NativeOfflineImportRequest {
        expected_revision: 1,
        profile: Sha256Digest::from_bytes([3; 32]),
        directory: if cfg!(windows) {
            "D:\\owned-fixture".into()
        } else {
            "/owned-fixture".into()
        },
    };
    request.validate()?;
    let method = ControlMethod::ArbiterImportDevice {
        request: Box::new(request),
    };
    assert_eq!(
        method.metadata_relay_refusal(),
        Some("console_private_setup_forbidden")
    );
    assert_eq!(
        method.request_frame_budget(),
        crate::service::MAX_TOOLS_CALL_REQUEST_BYTES
    );
    assert_eq!(
        method.response_frame_budget(),
        crate::service::MAX_TOOLS_CALL_RESPONSE_BYTES
    );
    let _: ControlMethod = serde_json::from_value(serde_json::to_value(&method)?)?;
    let response = ControlResponse::ArbiterDevice {
        observation: Box::new(status()),
    };
    let mut value = serde_json::to_value(response)?;
    let _: ControlResponse = serde_json::from_value(value.clone())?;
    value["observation"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<ControlResponse>(value).is_err());
    let schema = crate::arbiter::schema::document();
    assert!(schema["$defs"].get("NativeOfflineImportRequest").is_none());
    Ok(())
}

#[test]
fn catalog_rebind_validates_both_snapshots_and_preserves_all_user_settings() -> Result<()> {
    use crate::arbiter::device::tests::literal_profile;
    let profiles = [
        literal_profile(SelectedModel::Focus, 1, false)?,
        literal_profile(SelectedModel::ContextCompact, 2, false)?,
        literal_profile(SelectedModel::ContextLarge, 3, false)?,
        literal_profile(SelectedModel::ContextCompact, 4, true)?,
    ];
    let row = |i: usize| -> Result<VariantReference> {
        Ok(VariantReference {
            manifest: profiles[i].manifest,
            execution_identity: profiles[i].device_execution_identity()?,
        })
    };
    let old = DeviceCatalog {
        schema: DEVICE_CATALOG_SCHEMA.into(),
        focus: vec![row(0)?],
        context_compact: vec![row(1)?],
        context_large: vec![row(2)?],
    };
    let mut new = old.clone();
    new.context_compact.push(row(3)?);
    let resolve = |pin| profiles.iter().find(|profile| profile.manifest == pin);
    let old_binding = old.binding(Sha256Digest::from_bytes([40; 32]), resolve)?;
    let new_binding = new.binding(Sha256Digest::from_bytes([41; 32]), resolve)?;
    let mut current = DeviceConfiguration::disabled(
        Sha256Digest::from_bytes([1; 32]),
        Incarnation::from_bytes([2; 16]),
        old_binding.clone(),
    );
    current.context.enabled = true;
    current.context.selected_model = Some(SelectedModel::ContextCompact);
    current.context.device_policy = DevicePolicy::Cpu;
    current.context.gpu_precision = GpuPrecisionPreference::Fp16;
    current.context.indexes = crate::arbiter::stage_tests::configuration(false, true)
        .context
        .indexes;
    let rebound = current.rebind_catalog(
        &old_binding,
        new_binding.clone(),
        &old,
        &new,
        resolve,
        resolve,
    )?;
    assert_eq!(rebound.revision, current.revision + 1);
    assert_eq!(rebound.device_catalog, new_binding);
    assert_eq!(rebound.focus, current.focus);
    assert_eq!(rebound.context, current.context);
    assert_eq!(rebound.installation, current.installation);
    assert_eq!(rebound.catalog, current.catalog);
    let mut wrong_old = old_binding.clone();
    wrong_old.source = Sha256Digest::from_bytes([99; 32]);
    assert!(
        current
            .rebind_catalog(
                &wrong_old,
                new_binding.clone(),
                &old,
                &new,
                resolve,
                resolve
            )
            .is_err()
    );
    let mut wrong_new = new_binding.clone();
    wrong_new.catalog = Sha256Digest::from_bytes([99; 32]);
    assert!(
        current
            .rebind_catalog(&old_binding, wrong_new, &old, &new, resolve, resolve)
            .is_err()
    );
    assert!(
        current
            .rebind_catalog(
                &old_binding,
                new_binding.clone(),
                &new,
                &new,
                resolve,
                resolve
            )
            .is_err()
    );
    assert!(
        current
            .rebind_catalog(&old_binding, new_binding, &old, &new, resolve, |_| None)
            .is_err()
    );
    assert!(
        current
            .rebind_catalog(
                &old_binding,
                old_binding.clone(),
                &old,
                &old,
                resolve,
                resolve
            )
            .is_err()
    );
    Ok(())
}

fn semantic_status(
    model: SelectedModel,
    policy: DevicePolicy,
    preference: GpuPrecisionPreference,
    actual: Option<(Option<GpuBackend>, Option<DevicePrecision>)>,
    admission: ModelAdmissionStatus,
    availability: Availability,
    rejection: Option<SelectionReason>,
) -> Result<DeviceStatus> {
    use crate::arbiter::device::tests::literal_profile;
    let mut value = status();
    let which = model.stage();
    let base = match model {
        SelectedModel::Focus => 21,
        SelectedModel::ContextCompact => 31,
        SelectedModel::ContextLarge => 41,
    };
    let indexes = crate::arbiter::stage_tests::configuration(true, true)
        .stage(which)
        .indexes
        .clone();
    *value.configuration.stage_mut(which) = DeviceStageConfiguration {
        enabled: availability != Availability::Disabled,
        selected_model: Some(model),
        device_policy: policy,
        gpu_precision: preference,
        indexes,
    };
    let mut profiles = vec![
        literal_profile(model, base, false)?,
        literal_profile(model, base + 1, true)?,
        literal_profile(model, base + 2, true)?,
    ];
    profiles[2].device_execution.as_mut().unwrap().precision = DevicePrecision::Float16;
    let mut variants = Vec::new();
    let mut selected = None;
    for profile in profiles {
        let backend = profile
            .device_execution
            .as_ref()
            .map(|device| device.backend);
        let precision = profile
            .device_execution
            .as_ref()
            .map(|device| device.precision);
        let chosen = actual == Some((backend, precision));
        let kind = if backend.is_some() && admission == ModelAdmissionStatus::LocalCandidate {
            ModelAdmissionStatus::Unavailable
        } else {
            admission
        };
        let available = if chosen {
            Availability::Ready
        } else if kind == ModelAdmissionStatus::Unavailable {
            Availability::Unqualified
        } else if precision == Some(DevicePrecision::Float32)
            && rejection == Some(SelectionReason::DeviceMemoryInsufficient)
        {
            Availability::Constrained
        } else {
            Availability::Requested
        };
        let evidence = if backend.is_some() && kind == ModelAdmissionStatus::Qualified {
            Some(DeviceEvidence {
                target: "x86_64-pc-windows-msvc".into(),
                execution_identity: profile.device_execution_identity()?,
                numerical: Sha256Digest::from_bytes([51; 32]),
                resources: Sha256Digest::from_bytes([52; 32]),
                placement: Sha256Digest::from_bytes([53; 32]),
            })
        } else {
            None
        };
        let variant = DeviceVariantStatus::from_description(
            &profile,
            available,
            "Synthetic native view fixture; NOT hardware/model qualification".into(),
            kind,
            evidence,
        )?;
        if chosen {
            selected = Some(SelectedVariant {
                manifest: variant.manifest,
                execution_identity: variant.execution_identity.clone(),
                backend,
                precision,
                adapter_key: if backend.is_some() {
                    "synthetic-adapter".into()
                } else {
                    "native-cpu-profile".into()
                },
                capability_epoch: 1,
            });
        }
        variants.push(variant);
    }
    let mut observed = stage(which, value.configuration.stage(which));
    observed.availability = availability;
    observed.reason = "Synthetic native typed state; NOT an execution/qualification claim".into();
    observed.active_profile = selected.as_ref().map(|row| row.manifest);
    observed.selection = Some(DeviceSelection {
        schema: DEVICE_STATUS_SCHEMA.into(),
        model,
        policy,
        gpu_precision: preference,
        selected,
        reasons: rejection
            .map(|reason| {
                vec![CandidateRejection {
                    manifest: None,
                    adapter_key: None,
                    reason,
                }]
            })
            .unwrap_or_default(),
        operation_failure: None,
    });
    let summary = observed
        .models
        .iter_mut()
        .find(|row| row.model == model)
        .unwrap();
    summary.tasks = variants
        .iter()
        .flat_map(|variant| variant.tasks.iter().copied())
        .collect();
    summary.variants = variants;
    summary.availability = if availability == Availability::Ready {
        Availability::Ready
    } else if admission == ModelAdmissionStatus::Unavailable {
        Availability::Unqualified
    } else {
        Availability::Requested
    };
    summary.reason =
        "Synthetic native model-admission observation; no inferred client permission".into();
    match which {
        Stage::Focus => value.focus = observed,
        Stage::Context => value.context = observed,
    };
    value.validate()?;
    Ok(value)
}
#[test]
fn native_variant_metadata_preserves_all_legacy_status_fields_without_promoting_model() -> Result<()>
{
    let mut profile = crate::arbiter::stage_tests::current_profile(Stage::Focus)?;
    profile.cpu_fit = Some(CpuFit::default());
    profile.downloads.clear();
    for asset in &mut profile.assets {
        asset.bytes = 1;
    }
    profile.assets.push(Asset {
        name: "literal-external.bin".into(),
        role: AssetRole::ExternalWeights,
        bytes: 7,
        sha256: Sha256Digest::from_bytes([19; 32]),
    });
    profile.memory_reservation_bytes = 1234;
    profile.qualification = Some(Sha256Digest::from_bytes([23; 32]));
    profile.qualified_targets = vec![QualifiedTarget {
        target: "x86_64-pc-windows-msvc".into(),
        execution_identity: "onnxruntime-cpu/1.30.0/physical1/intra1/inter1/sequential/spinning0"
            .into(),
        receipt: Sha256Digest::from_bytes([24; 32]),
    }];
    let value = DeviceVariantStatus::from_description(
        &profile,
        Availability::Requested,
        "Literal metadata copy".into(),
        ModelAdmissionStatus::LocalCandidate,
        None,
    )?;
    assert_eq!(value.cpu_fit, Some(CpuFit::default()));
    assert_eq!(value.qualified_targets.len(), 1);
    assert_eq!(value.qualified_targets[0].target, "x86_64-pc-windows-msvc");
    assert_eq!(value.tasks, BTreeSet::from([Task::FocusRank]));
    assert_eq!(value.download_bytes, Some(14));
    assert_eq!(value.memory_reservation_bytes, Some(1234));
    assert_eq!(value.model_admission, ModelAdmissionStatus::LocalCandidate); // descriptor qualification never sets admission
    let json = serde_json::to_value(value)?;
    for key in [
        "cpu_fit",
        "qualified_targets",
        "tasks",
        "download_bytes",
        "memory_reservation_bytes",
    ] {
        assert!(json.get(key).is_some(), "lost native field {key}");
    }
    Ok(())
}
#[test]
fn requested_and_actual_device_precision_views_are_independent_and_truthful() -> Result<()> {
    let cpu = semantic_status(
        SelectedModel::ContextCompact,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Fp16,
        Some((None, None)),
        ModelAdmissionStatus::LocalCandidate,
        Availability::Ready,
        Some(SelectionReason::RuntimeUnavailable),
    )?;
    assert_eq!(
        cpu.context
            .selection
            .as_ref()
            .unwrap()
            .selected
            .as_ref()
            .unwrap()
            .precision,
        None
    );
    assert_eq!(
        cpu.configuration.context.gpu_precision,
        GpuPrecisionPreference::Fp16
    );
    let fp16 = semantic_status(
        SelectedModel::ContextLarge,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        Some((Some(GpuBackend::Cuda), Some(DevicePrecision::Float16))),
        ModelAdmissionStatus::Qualified,
        Availability::Ready,
        Some(SelectionReason::DeviceMemoryInsufficient),
    )?;
    let mut wrong = fp16.clone();
    let active = wrong.context.active_profile;
    wrong
        .context
        .models
        .iter_mut()
        .find(|row| row.model == SelectedModel::ContextLarge)
        .unwrap()
        .variants
        .iter_mut()
        .find(|row| Some(row.manifest) == active)
        .unwrap()
        .device_evidence = None;
    assert!(wrong.validate().is_err());
    let mut wrong = fp16;
    wrong
        .context
        .models
        .iter_mut()
        .find(|row| row.model == SelectedModel::ContextLarge)
        .unwrap()
        .reason
        .clear();
    assert!(wrong.validate().is_err());
    Ok(())
}
#[test]
fn current_device_semantic_case_emitter() -> Result<()> {
    use sha2::{Digest, Sha256};
    let mut cases = vec![serde_json::json!({"id":"both_disabled_auto","status":status()})];
    let specifications = [
        (
            "focus_cpu_local_candidate",
            SelectedModel::Focus,
            DevicePolicy::Cpu,
            GpuPrecisionPreference::Auto,
            Some((None, None)),
            ModelAdmissionStatus::LocalCandidate,
            Availability::Ready,
            None,
        ),
        (
            "context_compact_auto_fp32_gpu",
            SelectedModel::ContextCompact,
            DevicePolicy::Auto,
            GpuPrecisionPreference::Auto,
            Some((Some(GpuBackend::Cuda), Some(DevicePrecision::Float32))),
            ModelAdmissionStatus::Qualified,
            Availability::Ready,
            None,
        ),
        (
            "context_compact_auto_fp16_when_fp32_unavailable",
            SelectedModel::ContextCompact,
            DevicePolicy::Auto,
            GpuPrecisionPreference::Auto,
            Some((Some(GpuBackend::Cuda), Some(DevicePrecision::Float16))),
            ModelAdmissionStatus::Qualified,
            Availability::Ready,
            Some(SelectionReason::DeviceMemoryInsufficient),
        ),
        (
            "context_compact_requested_fp16_actual_cpu_fallback",
            SelectedModel::ContextCompact,
            DevicePolicy::Auto,
            GpuPrecisionPreference::Fp16,
            Some((None, None)),
            ModelAdmissionStatus::LocalCandidate,
            Availability::Ready,
            Some(SelectionReason::RuntimeUnavailable),
        ),
        (
            "context_large_requested_fp32_runtime_unavailable",
            SelectedModel::ContextLarge,
            DevicePolicy::Auto,
            GpuPrecisionPreference::Fp32,
            None,
            ModelAdmissionStatus::Qualified,
            Availability::Failed,
            Some(SelectionReason::RuntimeUnavailable),
        ),
        (
            "context_large_fp16_qualified_gpu",
            SelectedModel::ContextLarge,
            DevicePolicy::Auto,
            GpuPrecisionPreference::Fp16,
            Some((Some(GpuBackend::Cuda), Some(DevicePrecision::Float16))),
            ModelAdmissionStatus::Qualified,
            Availability::Ready,
            None,
        ),
        (
            "no_admitted_variant",
            SelectedModel::ContextLarge,
            DevicePolicy::Auto,
            GpuPrecisionPreference::Auto,
            None,
            ModelAdmissionStatus::Unavailable,
            Availability::Unqualified,
            Some(SelectionReason::Unqualified),
        ),
    ];
    for (id, model, policy, precision, actual, admission, availability, rejection) in specifications
    {
        cases.push(serde_json::json!({"id":id,"status":semantic_status(model,policy,precision,actual,admission,availability,rejection)?}));
    }
    let mut both = semantic_status(
        SelectedModel::ContextCompact,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Fp16,
        Some((Some(GpuBackend::Cuda), Some(DevicePrecision::Float16))),
        ModelAdmissionStatus::Qualified,
        Availability::Ready,
        None,
    )?;
    let focus = semantic_status(
        SelectedModel::Focus,
        DevicePolicy::Cpu,
        GpuPrecisionPreference::Auto,
        Some((None, None)),
        ModelAdmissionStatus::LocalCandidate,
        Availability::Ready,
        None,
    )?;
    both.configuration.focus = focus.configuration.focus;
    both.focus = focus.focus;
    both.validate()?;
    cases.push(serde_json::json!({"id":"both_stages_independent","status":both}));
    for (name, availability) in [
        ("disabled", Availability::Disabled),
        ("index_disabled", Availability::IndexDisabled),
        ("unqualified", Availability::Unqualified),
        ("unsupported_system", Availability::UnsupportedSystem),
        ("unsupported_task", Availability::UnsupportedTask),
        ("missing_assets", Availability::MissingAssets),
        ("requested", Availability::Requested),
        ("downloading", Availability::Downloading),
        ("verifying", Availability::Verifying),
        ("warming", Availability::Warming),
        ("ready", Availability::Ready),
        ("constrained", Availability::Constrained),
        ("draining", Availability::Draining),
        ("failed", Availability::Failed),
    ] {
        cases.push(serde_json::json!({"id":format!("lifecycle_{name}"),"status":semantic_status(SelectedModel::ContextCompact,DevicePolicy::Auto,GpuPrecisionPreference::Auto,if availability==Availability::Ready {Some((None,None))}else{None},ModelAdmissionStatus::LocalCandidate,availability,None)?}));
    }
    let legacy = crate::arbiter::stage_tests::configuration(true, true);
    let raw = legacy.canonical_bytes()?;
    let old_sha = Sha256Digest::from_bytes(Sha256::digest(&raw).into());
    let commands = [
        ("status_device", ManagementRequest::StatusDevice),
        (
            "initialize_device",
            ManagementRequest::InitializeDevice {
                expected_legacy_configuration: None,
            },
        ),
        (
            "migrate_device",
            ManagementRequest::MigrateDevice {
                expected_revision: legacy.revision,
                expected_configuration: old_sha,
            },
        ),
        (
            "configure_device",
            ManagementRequest::ConfigureDevice {
                expected_revision: 1,
                stage: Stage::Context,
                configuration: DeviceStageConfiguration {
                    enabled: false,
                    selected_model: Some(SelectedModel::ContextCompact),
                    device_policy: DevicePolicy::Auto,
                    gpu_precision: GpuPrecisionPreference::Auto,
                    indexes: vec![],
                },
            },
        ),
        (
            "provision_device",
            ManagementRequest::ProvisionDevice {
                expected_revision: 1,
                profile: Sha256Digest::from_bytes([31; 32]),
            },
        ),
        (
            "cancel_device",
            ManagementRequest::CancelDevice {
                expected_revision: 1,
                stage: Stage::Context,
            },
        ),
        (
            "remove_device",
            ManagementRequest::RemoveDevice {
                expected_revision: 1,
                profile: Sha256Digest::from_bytes([31; 32]),
            },
        ),
        (
            "recover_device",
            ManagementRequest::RecoverDevice {
                expected_revision: 1,
                stage: Stage::Context,
            },
        ),
    ];
    for (id, request) in commands {
        cases.push(serde_json::json!({"id":format!("request_{id}"),"request":request}));
    }
    for (stage_name, which, model) in [
        ("focus", Stage::Focus, SelectedModel::Focus),
        ("context", Stage::Context, SelectedModel::ContextCompact),
    ] {
        for (policy_name, policy) in [("auto", DevicePolicy::Auto), ("cpu", DevicePolicy::Cpu)] {
            for (precision_name, precision) in [
                ("auto", GpuPrecisionPreference::Auto),
                ("fp32", GpuPrecisionPreference::Fp32),
                ("fp16", GpuPrecisionPreference::Fp16),
            ] {
                cases.push(serde_json::json!({"id":format!("configure_{stage_name}_{policy_name}_{precision_name}"),"request":ManagementRequest::ConfigureDevice {expected_revision:1,stage:which,configuration:DeviceStageConfiguration {enabled:false,selected_model:Some(model),device_policy:policy,gpu_precision:precision,indexes:legacy.stage(which).indexes.clone()}}}));
            }
        }
    }
    cases.push(serde_json::json!({"id":"unknown_precision_refuses","invalid":{"operation":"configure_device","stage":"context","expected_revision":1,"configuration":{"enabled":false,"selected_model":"context_compact","device_policy":"auto","gpu_precision":"bf16","indexes":[]}}}));

    // Absence is ordinary writable disabled state; legacy initialization is a
    // separate native observation. Unknown/corrupt current state is an ERROR.
    cases.push(serde_json::json!({"id":"initialization_missing","status":status(),"meaning":"Actual absence: disabled current intent, no initialization marker"}));
    let mut retained = status();
    retained.initialization = Some(InitializationStatus {
        actual_legacy_digest: Some(Sha256Digest::from_bytes([71; 32])),
        reason: "Synthetic retained v1/v2 state; explicit disabled initialization required".into(),
    });
    retained.focus.availability = Availability::Failed;
    retained.context.availability = Availability::Failed;
    retained.validate()?;
    cases.push(serde_json::json!({"id":"initialization_legacy","status":retained}));
    cases.push(serde_json::json!({"id":"initialization_ambiguous","failure":crate::service::ControlResponse::Failure {code:"arbiter_legacy_configuration_ambiguous".into()}}));
    cases.push(serde_json::json!({"id":"initialization_corrupt","failure":crate::service::ControlResponse::Failure {code:"arbiter_current_configuration_invalid".into()}}));
    cases.push(serde_json::json!({"id":"initialization_unresolved_current3","failure":crate::service::ControlResponse::Failure {code:"arbiter_configuration_migration_refused".into()}}));
    // Route-order cases are appended after every earlier case, so the
    // positions that other suites read stay fixed.
    for (id, reason) in [
        (
            "context_compact_gpu_not_opted_in_cpu",
            SelectionReason::GpuNotOptedIn,
        ),
        (
            "context_compact_no_qualified_gpu_route_cpu",
            SelectionReason::NoQualifiedGpuRoute,
        ),
    ] {
        cases.push(serde_json::json!({"id":id,"status":semantic_status(SelectedModel::ContextCompact,DevicePolicy::Auto,GpuPrecisionPreference::Auto,Some((None,None)),ModelAdmissionStatus::LocalCandidate,Availability::Ready,Some(reason))?}));
    }
    let mut no_fit = semantic_status(
        SelectedModel::ContextCompact,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        None,
        ModelAdmissionStatus::LocalCandidate,
        Availability::UnsupportedSystem,
        Some(SelectionReason::GpuNotOptedIn),
    )?;
    let cpu_tier = no_fit
        .context
        .models
        .iter()
        .flat_map(|model| &model.variants)
        .find(|variant| variant.backend.is_none())
        .map(|variant| variant.manifest)
        .context("synthetic CPU tier missing")?;
    let selection = no_fit
        .context
        .selection
        .as_mut()
        .context("synthetic selection missing")?;
    selection.reasons.push(CandidateRejection {
        manifest: Some(cpu_tier),
        adapter_key: Some("cpu".into()),
        reason: SelectionReason::CpuFitInsufficient,
    });
    assert!(selection.selected.is_none() && selection.operation_failure.is_none());
    no_fit.validate()?;
    cases.push(
        serde_json::json!({"id":"context_compact_no_fit_unsupported_system","status":no_fit}),
    );
    let mut stopped = semantic_status(
        SelectedModel::ContextCompact,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        None,
        ModelAdmissionStatus::LocalCandidate,
        Availability::Failed,
        Some(SelectionReason::DeadlineExpired),
    )?;
    stopped
        .context
        .selection
        .as_mut()
        .context("synthetic selection missing")?
        .operation_failure = Some(SelectionReason::DeadlineExpired);
    stopped.validate()?;
    cases.push(
        serde_json::json!({"id":"context_compact_deadline_operation_failure","status":stopped}),
    );
    let document = serde_json::json!({"schema":"airk.arbiter.device-semantic-cases/1","qualification":false,"scope":"Synthetic Rust-native transport/UI conformance only; NO actual GPU/model qualification", "cases":cases,"legacy_migration":{"configuration":legacy,"canonical_sha256":old_sha}});
    if let Some(path) = std::env::var_os("AIRK_WRITE_ARBITER_DEVICE_CASES") {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        let mut bytes = serde_json::to_vec_pretty(&document)?;
        bytes.push(b'\n');
        file.write_all(&bytes)?;
        file.sync_all()?;
    } else {
        // Without the explicit write, the reviewed capture must be this emission.
        let reviewed: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../../../fixtures/split/arbiter/device_cases.json"
        ))?;
        assert!(
            reviewed == document,
            "regenerate fixtures/split/arbiter/device_cases.json with AIRK_WRITE_ARBITER_DEVICE_CASES"
        );
    }
    Ok(())
}

#[test]
fn selection_holds_twenty_rows_and_a_no_fit_selection_is_a_valid_unsupported_system() -> Result<()>
{
    let mut value = semantic_status(
        SelectedModel::ContextCompact,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        None,
        ModelAdmissionStatus::LocalCandidate,
        Availability::UnsupportedSystem,
        None,
    )?;
    let rows = |count: usize| {
        vec![
            CandidateRejection {
                manifest: None,
                adapter_key: None,
                reason: SelectionReason::HostMemoryInsufficient,
            };
            count
        ]
    };
    for (count, valid) in [(0, true), (18, true), (19, true), (20, true), (21, false)] {
        value.context.selection.as_mut().unwrap().reasons = rows(count);
        assert_eq!(value.validate().is_ok(), valid, "{count} rows");
    }
    value.context.selection.as_mut().unwrap().reasons = rows(1);
    let selection = value.context.selection.as_ref().unwrap();
    assert!(selection.selected.is_none());
    assert_eq!(value.context.availability, Availability::UnsupportedSystem);
    assert_eq!(value.context.active_profile, None);
    // A stopped operation records its typed failure and can never select.
    let mut stopped = value.clone();
    stopped.context.availability = Availability::Failed;
    stopped
        .context
        .selection
        .as_mut()
        .unwrap()
        .operation_failure = Some(SelectionReason::DeadlineExpired);
    stopped.validate()?;
    let ready = semantic_status(
        SelectedModel::ContextCompact,
        DevicePolicy::Auto,
        GpuPrecisionPreference::Auto,
        Some((None, None)),
        ModelAdmissionStatus::LocalCandidate,
        Availability::Ready,
        Some(SelectionReason::GpuNotOptedIn),
    )?;
    let mut wrong = ready;
    wrong.context.selection.as_mut().unwrap().operation_failure =
        Some(SelectionReason::DeadlineExpired);
    assert!(wrong.validate().is_err());
    Ok(())
}

#[test]
fn reviewed_device_cases_carry_the_route_reasons_and_never_the_reserved_one() -> Result<()> {
    let raw = include_str!("../../../../../../fixtures/split/arbiter/device_cases.json");
    let cases: serde_json::Value = serde_json::from_str(raw)?;
    assert_eq!(cases["qualification"], false);
    let rows = cases["cases"].as_array().context("device cases")?;
    let status = |id: &str| -> Result<DeviceStatus> {
        let row = rows
            .iter()
            .find(|row| row["id"] == id)
            .with_context(|| format!("reviewed case {id} missing"))?;
        let status: DeviceStatus = serde_json::from_value(row["status"].clone())?;
        status.validate()?;
        Ok(status)
    };
    let reasons = |status: &DeviceStatus| -> Vec<SelectionReason> {
        status
            .context
            .selection
            .iter()
            .flat_map(|selection| &selection.reasons)
            .map(|row| row.reason)
            .collect()
    };
    let opted_out = status("context_compact_gpu_not_opted_in_cpu")?;
    assert_eq!(reasons(&opted_out), [SelectionReason::GpuNotOptedIn]);
    assert_eq!(opted_out.context.availability, Availability::Ready);
    let no_route = status("context_compact_no_qualified_gpu_route_cpu")?;
    assert_eq!(reasons(&no_route), [SelectionReason::NoQualifiedGpuRoute]);
    let no_fit = status("context_compact_no_fit_unsupported_system")?;
    assert_eq!(
        reasons(&no_fit),
        [
            SelectionReason::GpuNotOptedIn,
            SelectionReason::CpuFitInsufficient
        ]
    );
    assert_eq!(no_fit.context.availability, Availability::UnsupportedSystem);
    let selection = no_fit.context.selection.as_ref().context("selection")?;
    assert!(selection.selected.is_none() && selection.operation_failure.is_none());
    let stopped = status("context_compact_deadline_operation_failure")?;
    assert_eq!(stopped.context.availability, Availability::Failed);
    assert_eq!(
        stopped
            .context
            .selection
            .as_ref()
            .and_then(|selection| selection.operation_failure),
        Some(SelectionReason::DeadlineExpired)
    );
    // The earlier positions that other suites read are unchanged.
    assert_eq!(rows[0]["id"], "both_disabled_auto");
    assert_eq!(
        rows[4]["id"],
        "context_compact_requested_fp16_actual_cpu_fallback"
    );
    assert_eq!(rows[6]["id"], "context_large_fp16_qualified_gpu");
    // The reserved value is in the schema vocabulary and in no emitted case.
    assert!(!raw.contains("smaller_model_selected"));
    Ok(())
}

#[test]
fn empty_variant_model_metadata_retains_native_supported_tasks_without_admission() -> Result<()> {
    let value = status();
    value.validate()?;
    for model in value.focus.models.iter().chain(&value.context.models) {
        assert!(model.variants.is_empty());
        assert_eq!(model.availability, Availability::Unqualified);
        assert_eq!(model.tasks, model.model.supported_tasks());
        assert!(!model.tasks.is_empty());
        assert!(
            model
                .tasks
                .iter()
                .all(|task| model.model.stage().permits(*task))
        );
    }
    Ok(())
}
