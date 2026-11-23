// SPDX-License-Identifier: Apache-2.0
use super::*;
#[cfg(feature = "local-inference")]
pub(super) fn structural_context_abi() -> ScoreAbi {
    // Explicit current ABI fixture input; Core owns validation/refusal cases.
    ScoreAbi {
        readout: Readout::DecisionSymbol,
        inputs: [
            ("input_ids", TensorType::Int64, vec!["batch_1", "tokens"]),
            (
                "attention_mask",
                TensorType::Int64,
                vec!["batch_1", "tokens"],
            ),
            ("position_ids", TensorType::Int64, vec!["batch_1", "tokens"]),
            ("decision_positions", TensorType::Int64, vec!["batch_1"]),
            (
                "candidate_positions",
                TensorType::Int64,
                vec!["batch_1", "options"],
            ),
            ("option_mask", TensorType::Bool, vec!["batch_1", "options"]),
        ]
        .into_iter()
        .map(|(name, element, axes)| TensorContract {
            name: name.into(),
            element,
            axes: axes.into_iter().map(str::to_owned).collect(),
        })
        .collect(),
        output: TensorContract {
            name: "scores".into(),
            element: TensorType::Float32,
            axes: vec!["batch_1".into(), "options".into()],
        },
    }
}

#[test]
fn native_reason_limits_are_utf8_byte_bounds_without_breaking_scalars() {
    for value in [
        "a".repeat(2048),
        "λ".repeat(1024),
        "🙂".repeat(1024),
        format!("{}🙂", "a".repeat(1023)),
    ] {
        let bounded = bounded_reason(&value);
        assert!(!bounded.is_empty());
        assert!(bounded.len() <= 1024);
        assert!(value.starts_with(&bounded));
        assert!(std::str::from_utf8(bounded.as_bytes()).is_ok());
    }
    assert_eq!(bounded_reason("x".repeat(1024)), "x".repeat(1024));
    assert_eq!(
        bounded_reason(format!("{}🙂", "a".repeat(1023))),
        "a".repeat(1023)
    );
    assert!(!bounded_reason("").is_empty());
}

fn status_owner_fixture() -> Result<(tempfile::TempDir, Owner)> {
    let root = tempfile::tempdir()?;
    let namespace = airk_core::identity::Incarnation::from_bytes([44; 16]);
    let gate = airk_catalog_control::catalog_admission::NativeCatalogAdmission::new(
        airk_core::identity::Incarnation::from_bytes([45; 16]),
        namespace,
        airk_core::index_catalog::CatalogRevision::INITIAL,
        1,
    )?;
    let owner = Owner::new(
        Arc::new(airk_store::service::ServiceLease::acquire(root.path())?),
        gate,
    )?;
    Ok((root, owner))
}
#[test]
fn native_initialization_status_marks_only_unwritable_configuration_and_preserves_legacy_bytes()
-> Result<()> {
    use sha2::{Digest, Sha256};
    let (root, owner) = status_owner_fixture()?;
    let normal = owner.status()?;
    normal.validate()?;
    assert!(normal.initialization.is_none());
    assert!(!root.path().join("arbiter-v3").exists());
    let legacy = include_bytes!("../../../../../fixtures/arbiter/configuration-disabled-v2.json");
    let expected = airk_core::identity::Sha256Digest::from_bytes(Sha256::digest(legacy).into());
    let directory = root.path().join("arbiter-v2");
    std::fs::create_dir(&directory)?;
    std::fs::write(directory.join("configuration.json"), legacy)?;
    let legacy_status = owner.status()?;
    legacy_status.validate()?;
    assert_eq!(
        legacy_status
            .initialization
            .as_ref()
            .unwrap()
            .actual_legacy_digest,
        Some(expected)
    );
    assert!(
        !legacy_status.configuration.focus.enabled && !legacy_status.configuration.context.enabled
    );
    assert!(!root.path().join("arbiter-v3").exists());
    assert!(
        owner
            .configuration
            .initialize_checked(None, || Ok(()))
            .is_err()
    );
    let checks = std::cell::Cell::new(0usize);
    assert!(
        owner
            .configuration
            .initialize_checked(Some(expected), || {
                checks.set(checks.get() + 1);
                ensure!(
                    checks.get() < 2,
                    "liveness ended after initialization directory creation"
                );
                Ok(())
            })
            .is_err()
    );
    assert_eq!(checks.get(), 2);
    let interrupted = owner.status()?;
    interrupted.validate()?;
    assert_eq!(
        interrupted
            .initialization
            .as_ref()
            .unwrap()
            .actual_legacy_digest,
        Some(expected)
    );
    assert!(!root.path().join("arbiter-v3/configuration.json").exists());
    owner
        .configuration
        .initialize_checked(Some(expected), || Ok(()))?;
    let initialized = owner.status()?;
    initialized.validate()?;
    assert!(initialized.initialization.is_none());
    assert_eq!(std::fs::read(directory.join("configuration.json"))?, legacy);
    Ok(())
}
#[test]
fn ambiguous_or_corrupt_configuration_status_never_invents_a_legacy_digest_or_writable_config()
-> Result<()> {
    for ambiguous in [false, true] {
        let (root, owner) = status_owner_fixture()?;
        let original = b"original malformed state, never adopted";
        if ambiguous {
            for name in ["arbiter-v1", "arbiter-v2"] {
                std::fs::create_dir(root.path().join(name))?;
                std::fs::write(root.path().join(name).join("configuration.json"), original)?;
            }
        } else {
            std::fs::create_dir(root.path().join("arbiter-v3"))?;
            std::fs::write(root.path().join("arbiter-v3/configuration.json"), original)?;
        }
        let observed = owner.status()?;
        observed.validate()?;
        let initialization = observed
            .initialization
            .context("missing nonwritable marker")?;
        assert!(initialization.actual_legacy_digest.is_none());
        assert!(!initialization.reason.is_empty() && initialization.reason.len() <= 1024);
        assert_eq!(observed.focus.availability, Availability::Failed);
        assert_eq!(observed.context.availability, Availability::Failed);
        assert!(
            owner
                .configuration
                .initialize_checked(None, || Ok(()))
                .is_err()
        );
        let preserved = if ambiguous {
            "arbiter-v2/configuration.json"
        } else {
            "arbiter-v3/configuration.json"
        };
        assert_eq!(std::fs::read(root.path().join(preserved))?, original);
    }
    Ok(())
}
#[test]
fn answer_application_requires_joint_task_in_actual_enrollment_and_profile() {
    use std::collections::BTreeSet;
    let relevance = BTreeSet::from([Task::EvidenceRelevance]);
    let next = BTreeSet::from([Task::ContextAction]);
    assert!(
        !supports_stage_task(Stage::Context, &relevance, &next),
        "Another enrolled task cannot enable answer advice"
    );
    assert!(
        !supports_stage_task(Stage::Context, &next, &relevance),
        "A relevance-only resident model is not an answer adviser"
    );
    assert!(
        supports_stage_task(Stage::Context, &next, &next),
        "Explicit current task on both owners is eligible"
    );
}
#[test]
fn joint_and_legacy_tasks_never_authorize_each_others_advice() {
    use std::collections::BTreeSet;
    let legacy = BTreeSet::from([Task::ContextNextRead]);
    let joint = BTreeSet::from([Task::ContextAction]);
    assert!(
        !supports_stage_task(Stage::Context, &legacy, &legacy),
        "a warm legacy-only profile is not current Answer/2 readiness"
    );
    assert!(!supports_stage_task(Stage::Context, &legacy, &joint));
    assert!(!supports_stage_task(Stage::Context, &joint, &legacy));
    assert!(supports_stage_task(Stage::Context, &joint, &joint));
}
#[test]
fn scoped_application_does_not_disclose_another_indexes_selected_profile() {
    use airk_core::{
        identity::{Incarnation, Sha256Digest},
        index_catalog::IndexId,
    };
    let enrolled = IndexId::from_identity(Incarnation::from_bytes([4; 16]));
    let other = IndexId::from_identity(Incarnation::from_bytes([5; 16]));
    let manifest = Sha256Digest::from_bytes([3; 32]);
    let mut configuration = Configuration::disabled(
        Sha256Digest::from_bytes([1; 32]),
        Incarnation::from_bytes([2; 16]),
    );
    configuration.context.selected_profile = Some(manifest);
    configuration.context.indexes.push(Enrollment {
        index_id: enrolled,
        tasks: std::collections::BTreeSet::from([Task::EvidenceRelevance]),
    });
    assert_eq!(
        profile_for_index(&configuration, enrolled),
        None,
        "Global off does not apply the saved selection"
    );
    configuration.context.enabled = true;
    assert_eq!(
        profile_for_index(&configuration, other),
        None,
        "Another index cannot observe the global selection"
    );
    assert_eq!(
        profile_for_index(&configuration, enrolled),
        Some(manifest),
        "The actual enrolled index retains its full applied identity"
    );
    configuration.context.indexes.clear();
    assert_eq!(
        profile_for_index(&configuration, enrolled),
        None,
        "No enrollment grants no profile disclosure"
    );
}
#[test]
fn arbiter_authority_loss_is_never_relabelled_optional_runtime_failure() {
    for (error, expected) in [
        (
            anyhow::Error::new(airk_core::scoped_reads::ScopeReadRefused),
            "scoped_read_unavailable",
        ),
        (
            anyhow::Error::new(airk_core::scoped_reads::ScopeReadRefused)
                .context("scope delivery changed"),
            "scoped_read_unavailable",
        ),
        (
            anyhow::anyhow!("optional asset digest differs"),
            "arbiter_unavailable",
        ),
    ] {
        let ControlResponse::Failure { code } = failure(error) else {
            panic!("expected exact native failure variant");
        };
        assert_eq!(code, expected);
    }
}

/// The loop card tasks run under the Context enrollment: a profile that
/// declares them serves an index enrolled for the Context task. Nothing
/// else is served that was not before.
#[test]
fn a_loop_profile_serves_an_index_enrolled_for_context() {
    use std::collections::BTreeSet;
    let cards = BTreeSet::from([
        Task::ContextSplit,
        Task::ContextPlan,
        Task::ContextCoverage,
        Task::ContextFollowUp,
    ]);
    let context = BTreeSet::from([Task::ContextAction]);
    let focus = BTreeSet::from([Task::FocusRank]);
    let next_read = BTreeSet::from([Task::ContextNextRead]);
    for (stage, enrollment, profile, served) in [
        (Stage::Context, &context, &context, true),
        (Stage::Context, &context, &cards, true),
        (Stage::Focus, &focus, &focus, true),
        // Card tasks never serve Focus, and the Focus task never serves
        // Context.
        (Stage::Focus, &focus, &cards, false),
        (Stage::Context, &context, &focus, false),
        // An index that is not enrolled for the stage's own task is not
        // served, whatever the profile declares.
        (Stage::Context, &focus, &cards, false),
        (Stage::Context, &cards, &cards, false),
        // A task of no current stage serves nothing.
        (Stage::Context, &context, &next_read, false),
    ] {
        assert_eq!(
            supports_stage_task(stage, enrollment, profile),
            served,
            "{stage:?}: enrolled {enrollment:?}, profile {profile:?}"
        );
    }
}
