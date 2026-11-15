// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};

pub(crate) fn digest(byte: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([byte; 32])
}
pub(crate) fn index(byte: u8) -> IndexId {
    IndexId::from_identity(Incarnation::from_bytes([byte; 16]))
}
pub(crate) fn scope() -> ScopeBinding {
    let token = crate::identity::RevisionToken {
        incarnation: Incarnation::from_bytes([3; 16]),
        generation: 1,
        revision: digest(4),
    };
    ScopeBinding {
        installation: digest(1),
        catalog: Incarnation::from_bytes([2; 16]),
        index_id: index(5),
        authorization: digest(6),
        configuration_revision: 1,
        publication: crate::evidence::RevisionEnvelope::new("fixture".into(), token),
    }
}
pub(crate) fn configuration(focus: bool, context: bool) -> Configuration {
    let mut result = Configuration::disabled(digest(1), Incarnation::from_bytes([2; 16]));
    for (stage, enabled, profile, task) in [
        (Stage::Focus, focus, 7, Task::FocusRank),
        (Stage::Context, context, 8, Task::ContextAction),
    ] {
        if enabled {
            *result.stage_mut(stage) = StageConfiguration {
                enabled: true,
                selected_profile: Some(digest(profile)),
                indexes: vec![Enrollment {
                    index_id: index(5),
                    tasks: BTreeSet::from([task]),
                }],
            };
        }
    }
    result
}
pub(crate) fn profile_status(model: ModelChoice, availability: Availability) -> ProfileStatus {
    let stage = model.stage().unwrap();
    ProfileStatus {
        cpu_fit: None,
        qualified_targets: vec![],
        model,
        profile: None,
        availability,
        reason: "Synthetic structural fixture; no model or platform qualification".into(),
        tasks: BTreeSet::from([if stage == Stage::Focus {
            Task::FocusRank
        } else {
            Task::ContextAction
        }]),
        download_bytes: None,
        memory_reservation_bytes: None,
    }
}
pub(crate) fn status(configuration: Configuration, availability: Availability) -> Status {
    let make = |stage| StageStatus {
        selected_profile: configuration.stage(stage).selected_profile,
        active_profile: None,
        availability: if configuration.stage(stage).enabled {
            availability
        } else {
            Availability::Disabled
        },
        reason: "Synthetic structural fixture; no model execution".into(),
        queued: 0,
        running: 0,
        profiles: if stage == Stage::Focus {
            vec![profile_status(
                ModelChoice::FocusQwenReranker06B,
                availability,
            )]
        } else {
            vec![
                profile_status(ModelChoice::ContextQwenCompact, availability),
                profile_status(ModelChoice::ContextQwenLarge, availability),
            ]
        },
    };
    Status {
        schema: STATUS_SCHEMA.into(),
        initialization: None,
        focus: make(Stage::Focus),
        context: make(Stage::Context),
        configuration,
    }
}
pub(crate) fn focus_abi() -> ScoreAbi {
    ScoreAbi {
        readout: Readout::FocusYesNo,
        inputs: ["input_ids", "attention_mask", "position_ids"]
            .into_iter()
            .map(|name| TensorContract {
                name: name.into(),
                element: TensorType::Int64,
                axes: vec!["batch".into(), "tokens".into()],
            })
            .collect(),
        output: TensorContract {
            name: "logits".into(),
            element: TensorType::Float32,
            axes: vec!["batch".into(), "labels".into()],
        },
    }
}
pub(crate) fn context_qwen_abi() -> ScoreAbi {
    ScoreAbi {
        readout: Readout::DecisionSymbol,
        inputs: vec![
            TensorContract {
                name: "input_ids".into(),
                element: TensorType::Int64,
                axes: vec!["batch_1".into(), "tokens".into()],
            },
            TensorContract {
                name: "attention_mask".into(),
                element: TensorType::Int64,
                axes: vec!["batch_1".into(), "tokens".into()],
            },
            TensorContract {
                name: "position_ids".into(),
                element: TensorType::Int64,
                axes: vec!["batch_1".into(), "tokens".into()],
            },
            TensorContract {
                name: "decision_positions".into(),
                element: TensorType::Int64,
                axes: vec!["batch_1".into()],
            },
            TensorContract {
                name: "candidate_positions".into(),
                element: TensorType::Int64,
                axes: vec!["batch_1".into(), "options".into()],
            },
            TensorContract {
                name: "option_mask".into(),
                element: TensorType::Bool,
                axes: vec!["batch_1".into(), "options".into()],
            },
        ],
        output: TensorContract {
            name: "scores".into(),
            element: TensorType::Float32,
            axes: vec!["batch_1".into(), "options".into()],
        },
    }
}
pub(crate) fn current_profile(stage: Stage) -> Result<Profile> {
    let mut profile: Profile = serde_json::from_slice(include_bytes!(
        "../../../../fixtures/arbiter/profile-downloads.json"
    ))?;
    profile.stage = Some(stage);
    profile.qualification = None;
    profile.qualified_targets.clear();
    profile.model = if stage == Stage::Focus {
        ModelChoice::FocusQwenReranker06B
    } else {
        ModelChoice::ContextQwenCompact
    };
    let task = if stage == Stage::Focus {
        Task::FocusRank
    } else {
        Task::ContextAction
    };
    profile.tasks = BTreeSet::from([task]);
    profile.task_policies = BTreeMap::from([(task, digest(9))]);
    profile.abi = match stage {
        Stage::Focus => focus_abi(),
        Stage::Context => context_qwen_abi(),
    };
    Ok(profile)
}

#[test]
fn stages_have_independent_consent_with_neither_implicit_profile_nor_enrollment() -> Result<()> {
    for (focus, context) in [(false, false), (true, false), (false, true), (true, true)] {
        let configuration = configuration(focus, context);
        configuration.validate()?;
        assert_eq!(configuration.eligible(index(5), Task::FocusRank), focus);
        assert_eq!(
            configuration.eligible(index(5), Task::ContextAction),
            context
        );
        assert!(!configuration.eligible(index(6), Task::FocusRank));
        assert!(!configuration.eligible(index(6), Task::ContextAction));
        assert!(!configuration.eligible(index(5), Task::EvidenceRelevance));
    }
    Ok(())
}
#[test]
fn wrong_stage_task_duplicate_and_empty_enrollment_refuse() -> Result<()> {
    for stage in [Stage::Focus, Stage::Context] {
        let original = configuration(true, true);
        let mut value = original.clone();
        value.stage_mut(stage).indexes[0].tasks = BTreeSet::new();
        assert!(value.validate().is_err());
        let mut value = original.clone();
        value.stage_mut(stage).indexes[0].tasks = BTreeSet::from([if stage == Stage::Focus {
            Task::ContextAction
        } else {
            Task::FocusRank
        }]);
        assert!(value.validate().is_err());
        let mut value = original.clone();
        let entry = value.stage(stage).indexes[0].clone();
        value.stage_mut(stage).indexes.push(entry);
        assert!(value.validate().is_err());
        let mut value = original.clone();
        value.stage_mut(stage).selected_profile = None;
        assert!(value.validate().is_err());
    }
    Ok(())
}
#[test]
fn current_profiles_bind_stage_task_policy_and_distinct_pair_abi() -> Result<()> {
    for stage in [Stage::Focus, Stage::Context] {
        let original = current_profile(stage)?;
        original.validate()?;
        assert_eq!(original.declared_stage(), Some(stage));
        let mut value = original.clone();
        value.stage = None;
        assert!(value.validate().is_err());
        let mut value = original.clone();
        value.stage = Some(if stage == Stage::Focus {
            Stage::Context
        } else {
            Stage::Focus
        });
        assert!(value.validate().is_err());
        let mut value = original.clone();
        value.task_policies.clear();
        assert!(value.validate().is_err());
    }
    let mut focus = current_profile(Stage::Focus)?;
    focus.abi.inputs.remove(2);
    assert!(focus.validate().is_err());
    let mut focus = current_profile(Stage::Focus)?;
    focus.abi.output.axes = vec!["batch".into(), "options".into()];
    assert!(focus.validate().is_err());
    let mut focus = current_profile(Stage::Focus)?;
    focus.abi.readout = Readout::DecisionSymbol;
    assert!(focus.validate().is_err());
    Ok(())
}
#[test]
fn qwen_compiler_requires_own_symbols_and_never_admits_pointer_aliases() -> Result<()> {
    let mut compiler = CompilerProfile {
        schema: COMPILER_SCHEMA.into(),
        recipe: CompilerRecipe::QwenRerankYesNoV1,
        tokenizer: digest(1),
        max_tokens: 2048,
        max_options: 2,
        controls: vec![],
        symbols: vec![
            BoundToken {
                text: "no".into(),
                id: 2152,
            },
            BoundToken {
                text: "yes".into(),
                id: 9693,
            },
        ],
    };
    compiler.validate()?;
    compiler.symbols.swap(0, 1);
    assert!(compiler.validate().is_err());
    compiler.symbols.swap(0, 1);
    compiler.symbols[1].id = 2152;
    assert!(compiler.validate().is_err());
    compiler.recipe = CompilerRecipe::QwenSingleChoiceV1;
    compiler.symbols = vec![
        BoundToken {
            text: "A".into(),
            id: 32,
        },
        BoundToken {
            text: "B".into(),
            id: 33,
        },
    ];
    compiler.validate()?;
    compiler.symbols[1].id = 32;
    assert!(compiler.validate().is_err());
    Ok(())
}
#[test]
fn stage_status_never_turns_selection_into_readiness_or_cross_stage_fit() -> Result<()> {
    let configured = configuration(true, true);
    for availability in [
        Availability::Unqualified,
        Availability::MissingAssets,
        Availability::Constrained,
        Availability::Failed,
    ] {
        let actual = status(configured.clone(), availability);
        actual.validate()?;
        assert!(actual.focus.active_profile.is_none());
        assert!(actual.context.active_profile.is_none());
    }
    let mut value = status(configured, Availability::Unqualified);
    value.context.availability = Availability::Ready;
    assert!(value.validate().is_err());
    let mut value = status(configuration(false, false), Availability::Disabled);
    value.focus.profiles.push(profile_status(
        ModelChoice::ContextQwenLarge,
        Availability::Unqualified,
    ));
    assert!(value.validate().is_err());
    Ok(())
}

// Exact prior owner serialization, retained here only for schema-identity proof.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedProfile {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    task_policies: BTreeMap<Task, Sha256Digest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    qualified_targets: Vec<QualifiedTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cpu_fit: Option<CpuFit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    downloads: Vec<RetainedAssetDownload>,
    model: ModelChoice,
    profile_id: String,
    manifest: Sha256Digest,
    runtime: String,
    compiler: Sha256Digest,
    calibration: Sha256Digest,
    policy: Sha256Digest,
    qualification: Option<Sha256Digest>,
    tasks: BTreeSet<Task>,
    abi: ScoreAbi,
    max_tokens: usize,
    max_candidates: usize,
    memory_reservation_bytes: u64,
    peak_disk_bytes: u64,
    assets: Vec<Asset>,
}
// Retain the old nested owner too: using current AssetPart here would mask
// accidental insertion into an existing persisted profile's canonical bytes.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedAssetPart {
    url: String,
    bytes: u64,
    sha256: Sha256Digest,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedAssetDownload {
    asset_name: String,
    parts: Vec<RetainedAssetPart>,
}
#[test]
fn retained_profile_bytes_have_no_stage_insertion_or_digest_reidentification() -> Result<()> {
    let raw = include_bytes!("../../../../fixtures/arbiter/profile-downloads.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "d0064974be46e7cb56099a71c248587d81cd93eccd1405ec9e736664c199d593"
    );
    let prior: RetainedProfile = serde_json::from_slice(raw)?;
    let current: Profile = serde_json::from_slice(raw)?;
    current.validate()?;
    assert!(current.stage.is_none());
    assert_eq!(serde_json::to_vec(&prior)?, serde_json::to_vec(&current)?);
    let old = include_bytes!("../../../../fixtures/arbiter/configuration-disabled-v2.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(old)),
        "a7c21c9b902a3c64a06817f8ac92c37d38a88dba27fa348fd1f07bf22d42031a"
    );
    Ok(())
}

#[test]
fn native_stage_semantic_fixture_emitter() -> Result<()> {
    let mut cases = Vec::new();
    for (id, focus, context) in [
        ("both_disabled", false, false),
        ("focus_only", true, false),
        ("context_only", false, true),
        ("both_enabled", true, true),
    ] {
        let config = configuration(focus, context);
        config.validate()?;
        cases.push(json!({"id":id,"configuration":config,"status":status(config.clone(),Availability::Unqualified),
            "expected_focus_eligible":focus,"expected_context_eligible":context}));
    }
    let configured = configuration(true, true);
    for (id, availability) in [
        ("missing_assets", Availability::MissingAssets),
        ("constrained", Availability::Constrained),
        ("failed", Availability::Failed),
        ("draining", Availability::Draining),
    ] {
        let observed = status(configured.clone(), availability);
        observed.validate()?;
        cases.push(json!({"id":id,"status":observed}));
    }
    for (id, legacy, reason) in [
        (
            "initialization_missing",
            None,
            "No current configuration is installed",
        ),
        (
            "initialization_legacy",
            Some(digest(50)),
            "Retained legacy configuration observed",
        ),
        (
            "initialization_ambiguous",
            None,
            "Ambiguous retained configuration; native initialization may refuse",
        ),
        (
            "initialization_corrupt",
            None,
            "Current configuration could not be decoded",
        ),
    ] {
        let mut observed = status(configuration(false, false), Availability::Failed);
        observed.initialization = Some(InitializationStatus {
            actual_legacy_digest: legacy,
            reason: reason.into(),
        });
        if id == "initialization_corrupt" {
            observed.focus.active_profile = Some(digest(7));
            observed.focus.queued = 2;
            observed.focus.running = 1;
            observed.focus.availability = Availability::Draining;
        }
        observed.validate()?;
        cases.push(json!({"id":id,"status":observed,"configuration_writable":false}));
    }
    let commands = [
        ManagementRequest::Configure {
            expected_revision: 1,
            stage: Stage::Focus,
            enabled: true,
            selected_profile: Some(digest(7)),
            indexes: configured.focus.indexes.clone(),
        },
        ManagementRequest::Configure {
            expected_revision: 1,
            stage: Stage::Context,
            enabled: true,
            selected_profile: Some(digest(8)),
            indexes: configured.context.indexes.clone(),
        },
        ManagementRequest::Cancel {
            expected_revision: 1,
            stage: Stage::Focus,
        },
        ManagementRequest::Recover {
            expected_revision: 1,
            stage: Stage::Context,
        },
        ManagementRequest::Initialize {
            expected_legacy_configuration: None,
        },
    ];
    let document = json!({"schema":"airk.arbiter.stage-semantic-cases/1","qualification":false,"focus_core_policy":focus::semantic_policy_digest()?,"context_core_policy":context_action::semantic_policy_digest()?,"cases":cases,"management_requests":commands,
        "current_models":["arbiter-focus-qwen3-reranker-06b","arbiter-context-qwen3-17b","arbiter-context-qwen3-4b"]});
    if let Some(path) = std::env::var_os("AIRK_WRITE_ARBITER_STAGE_CASES") {
        std::fs::write(path, serde_json::to_vec_pretty(&document)?)?;
    }
    if let Some(path) = std::env::var_os("AIRK_WRITE_ARBITER_SCHEMA") {
        std::fs::write(path, serde_json::to_vec_pretty(&schema::document())?)?;
    }
    assert_eq!(document["cases"].as_array().unwrap().len(), 12);
    assert_eq!(document["management_requests"].as_array().unwrap().len(), 5);
    Ok(())
}

#[test]
fn initialization_status_is_explicit_bounded_and_normal_bytes_are_retained() -> Result<()> {
    #[derive(Serialize)]
    struct RetainedStatus<'a> {
        schema: &'a str,
        configuration: &'a Configuration,
        focus: &'a StageStatus,
        context: &'a StageStatus,
    }
    let normal = status(configuration(true, true), Availability::Failed);
    normal.validate()?;
    assert_eq!(
        serde_json::to_vec(&normal)?,
        serde_json::to_vec(&RetainedStatus {
            schema: &normal.schema,
            configuration: &normal.configuration,
            focus: &normal.focus,
            context: &normal.context,
        })?
    );
    let mut placeholder = status(configuration(false, false), Availability::Failed);
    placeholder.initialization = Some(InitializationStatus {
        actual_legacy_digest: Some(digest(50)),
        reason: "Retained legacy state".into(),
    });
    placeholder.focus.active_profile = Some(digest(7));
    placeholder.focus.queued = 2;
    placeholder.focus.running = 1;
    placeholder.focus.availability = Availability::Draining;
    placeholder.context.active_profile = Some(digest(8));
    placeholder.context.queued = 1;
    placeholder.context.running = 1;
    placeholder.context.availability = Availability::Failed;
    placeholder.validate()?;
    let wire = serde_json::to_value(&placeholder)?;
    let decoded: Status = serde_json::from_value(wire.clone())?;
    decoded.validate()?;
    assert_eq!(decoded.focus.active_profile, Some(digest(7)));
    assert_eq!((decoded.focus.queued, decoded.focus.running), (2, 1));
    assert_eq!(decoded.context.active_profile, Some(digest(8)));
    assert_eq!((decoded.context.queued, decoded.context.running), (1, 1));
    assert_eq!(
        decoded.initialization.unwrap().actual_legacy_digest,
        Some(digest(50))
    );
    let mut malformed = wire.clone();
    malformed["initialization"]["unknown"] = json!(true);
    assert!(serde_json::from_value::<Status>(malformed).is_err());
    let mut malformed = wire;
    malformed["initialization"]["actual_legacy_digest"] = json!("not-a-digest");
    assert!(serde_json::from_value::<Status>(malformed).is_err());
    placeholder.configuration = normal.configuration;
    assert!(placeholder.validate().is_err());
    placeholder.configuration = configuration(false, false);
    placeholder.initialization.as_mut().unwrap().reason.clear();
    assert!(placeholder.validate().is_err());
    placeholder.initialization.as_mut().unwrap().reason = "x".repeat(1025);
    assert!(placeholder.validate().is_err());
    Ok(())
}

#[test]
fn context_qwen_six_input_abi_refuses_incomplete_foreign_or_reordered_tensors() -> Result<()> {
    let abi = context_qwen_abi();
    abi.validate_context_qwen()?;
    let mut cases = vec![json!({"id":"complete_six_input","abi":abi,"valid":true})];
    let mut invalid = Vec::new();
    let mut value = abi.clone();
    value.inputs.pop();
    invalid.push(("missing_input", value));
    let mut value = abi.clone();
    value.inputs.swap(0, 1);
    invalid.push(("reordered_input", value));
    let mut value = abi.clone();
    value.inputs.push(value.inputs[0].clone());
    invalid.push(("extra_input", value));
    let mut value = abi.clone();
    value.inputs[5].element = TensorType::Int64;
    invalid.push(("wrong_mask_type", value));
    let mut value = abi.clone();
    value.inputs[0].axes = vec!["tokens".into()];
    invalid.push(("historical_1d", value));
    let mut value = abi.clone();
    value.inputs[3].axes.push("tokens".into());
    invalid.push(("wrong_decision_axes", value));
    let mut value = abi.clone();
    value.output.name = "logits".into();
    invalid.push(("wrong_output_name", value));
    let mut value = abi.clone();
    value.output.axes = vec!["options".into()];
    invalid.push(("wrong_output_axes", value));
    let mut value = abi.clone();
    value.output.element = TensorType::Int64;
    invalid.push(("wrong_output_type", value));
    let mut value = abi;
    value.readout = Readout::FocusYesNo;
    invalid.push(("wrong_readout", value));
    for (id, value) in invalid {
        assert!(value.validate_context_qwen().is_err(), "{id}");
        let mut profile = current_profile(Stage::Context)?;
        profile.abi = value.clone();
        assert!(profile.validate_stage(Stage::Context).is_err(), "{id}");
        cases.push(json!({"id":id,"abi":value,"valid":false}));
    }
    if let Some(path) = std::env::var_os("AIRK_WRITE_ARBITER_CONTEXT_ABI_CASES") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({
            "schema":"airk.arbiter.context-qwen-abi-cases/1","qualification":false,"cases":cases}))?,
        )?;
    }
    Ok(())
}
