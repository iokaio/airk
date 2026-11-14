// SPDX-License-Identifier: Apache-2.0
use super::*;
#[test]
fn answer_application_requires_joint_task_in_actual_enrollment_and_profile() {
    use std::collections::BTreeSet;
    let relevance = BTreeSet::from([Task::EvidenceRelevance]);
    let next = BTreeSet::from([Task::ContextAction]);
    assert!(
        !supports_answer_task(&relevance, &next),
        "Another enrolled task cannot enable answer advice"
    );
    assert!(
        !supports_answer_task(&next, &relevance),
        "A relevance-only resident model is not an answer adviser"
    );
    assert!(
        supports_answer_task(&next, &next),
        "Explicit current task on both owners is eligible"
    );
}
#[test]
fn joint_and_legacy_tasks_never_authorize_each_others_advice() {
    use std::collections::BTreeSet;
    let legacy = BTreeSet::from([Task::ContextNextRead]);
    let joint = BTreeSet::from([Task::ContextAction]);
    assert!(
        !supports_answer_task(&legacy, &legacy),
        "a warm legacy-only profile is not current Answer/2 readiness"
    );
    assert!(!supports_answer_task(&legacy, &joint));
    assert!(!supports_answer_task(&joint, &legacy));
    assert!(supports_answer_task(&joint, &joint));
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
    configuration.selected_profile = Some(manifest);
    configuration.indexes.push(Enrollment {
        index_id: enrolled,
        tasks: std::collections::BTreeSet::from([Task::EvidenceRelevance]),
    });
    assert_eq!(
        profile_for_index(&configuration, enrolled),
        None,
        "Global off does not apply the saved selection"
    );
    configuration.enabled = true;
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
    configuration.indexes.clear();
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
