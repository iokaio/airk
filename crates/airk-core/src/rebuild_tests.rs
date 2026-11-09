// SPDX-License-Identifier: Apache-2.0
use super::*;

fn observation() -> Result<RebuildObservation> {
    let index = |name: &str| -> Result<_> {
        Ok(RebuildIndexObservation {
            original: ProjectRegistration::new(
                format!("/source/{name}"),
                format!("/old/{name}"),
                name.into(),
            )?,
            replacement: Some(ProjectRegistration::new(
                format!("/source/{name}"),
                format!("/new/{name}"),
                name.into(),
            )?),
            sources: BTreeMap::from([("docs".into(), format!("/source/{name}-docs"))]),
            settings_available: None,
            discarded_preferences: Vec::new(),
            settings_sha256: Sha256Digest::from_bytes([7; 32]),
            original_paused: true,
            paused: true,
            watching: false,
            initial_job: None,
            job: None,
            readiness: RebuildReadiness::Paused,
            failure: None,
        })
    };
    Ok(RebuildObservation {
        schema: 1,
        mode: None,
        indexes_complete: None,
        expected_indexes: None,
        cleanup: None,
        operation_id: Incarnation::from_bytes([1; 16]),
        stage: RebuildStage::Paused,
        provenance: RebuildProvenance::NativeRebuildFromSavedConfiguration,
        candidate: Sha256Digest::from_bytes([2; 32]),
        predecessor: Sha256Digest::from_bytes([3; 32]),
        original_namespace: Incarnation::from_bytes([4; 16]),
        preserved_state: "/old/state".into(),
        preserved_control: "/old/state/control".into(),
        preserved_index_roots: vec!["/old/alpha".into(), "/old/beta".into()],
        successor_state: "/new/state".into(),
        successor_control: "/new/state/control".into(),
        successor_namespace: Some(Incarnation::from_bytes([5; 16])),
        original_failure: None,
        failure: None,
        indexes: vec![index("alpha")?, index("beta")?],
        index_scope: None,
    })
}

#[test]
fn scoped_rebuild_observation_discloses_exact_subset_and_preserves_complete_index_value()
-> Result<()> {
    let original = observation()?;
    original.validate()?;
    let binding = original.indexes[0].replacement.as_ref().unwrap().binding;
    let selected = original.scoped_for(binding)?.unwrap();
    assert_eq!(selected.index_scope, Some(binding));
    assert_eq!(selected.indexes.len(), 1);
    assert_eq!(selected.preserved_index_roots, vec!["/old/alpha"]);
    assert_eq!(
        serde_json::to_value(&selected.indexes[0])?,
        serde_json::to_value(&original.indexes[0])?
    );
    assert!(
        selected
            .scoped_for(original.indexes[1].original.binding)?
            .is_none()
    );
    assert!(
        original
            .scoped_for(Sha256Digest::from_bytes([99; 32]))?
            .is_none()
    );
    assert_eq!(original.indexes.len(), 2);
    Ok(())
}

#[test]
fn scoped_clean_break_cleanup_redacts_sibling_paths_and_keeps_administrative_proof() -> Result<()> {
    use crate::service::{JobKind, JobOutcome, JobState};
    let mut administrative = observation()?;
    administrative.mode = Some(RebuildMode::CleanBreak);
    administrative.indexes_complete = Some(true);
    administrative.expected_indexes = Some(2);
    administrative.stage = RebuildStage::Ready;
    for (position, index) in administrative.indexes.iter_mut().enumerate() {
        index.readiness = RebuildReadiness::Ready;
        index.job = Some(JobRecord {
            id: Sha256Digest::from_bytes([11 + position as u8; 32]),
            project: index.replacement.as_ref().unwrap().binding,
            kind: JobKind::Index,
            state: JobState::Succeeded,
            fencing_generation: 1,
            attempts: 1,
            outcome: Some(JobOutcome::Succeeded { revision: None }),
        });
    }
    let private = "/private/sibling-cleanup-path";
    administrative.cleanup = Some(RebuildCleanup {
        complete: false,
        state: "blocked".into(),
        scope: "administrative".into(),
        failure: Some(RebuildFailure {
            code: "clean_break_cleanup_blocked".into(),
            message: format!("Unknown retained file at {private}"),
        }),
        observation: Some(serde_json::json!({
            "complete": false,
            "retired_collections": ["/old/alpha", private],
            "native_failure": {"path": private}
        })),
    });
    administrative.validate()?;
    let original = serde_json::to_value(&administrative)?;
    let binding = administrative.indexes[0]
        .replacement
        .as_ref()
        .unwrap()
        .binding;
    let selected = administrative.scoped_for(binding)?.unwrap();
    let cleanup = selected.cleanup.as_ref().unwrap();
    assert_eq!(selected.index_scope, Some(binding));
    assert_eq!(selected.expected_indexes, Some(1));
    assert_eq!(selected.indexes_complete, Some(true));
    assert_eq!(cleanup.scope, "selected_index_summary");
    assert!(!cleanup.complete);
    assert_eq!(cleanup.state, "blocked");
    assert!(cleanup.observation.is_none());
    assert_eq!(
        cleanup.failure.as_ref().unwrap().code,
        "clean_break_cleanup_blocked"
    );
    assert!(
        cleanup
            .failure
            .as_ref()
            .unwrap()
            .message
            .contains("unavailable")
    );
    assert!(!serde_json::to_string(&selected)?.contains(private));
    assert!(!serde_json::to_string(&selected)?.contains("/old/beta"));
    assert_eq!(
        serde_json::to_value(&selected.indexes[0])?,
        original["indexes"][0]
    );
    assert_eq!(
        serde_json::to_value(&administrative)?,
        original,
        "scoping must not remove the administrator's complete native receipt"
    );
    let mut crossed = selected;
    crossed.cleanup.as_mut().unwrap().observation = administrative.cleanup.unwrap().observation;
    assert!(
        crossed.validate().is_err(),
        "a selected-index summary cannot carry administrative proof paths"
    );
    Ok(())
}

#[test]
fn rebuild_observation_rejects_changed_scope_job_and_false_readiness() -> Result<()> {
    let original = observation()?;
    let mut changed = original.clone();
    changed.successor_control = changed.preserved_control.clone();
    assert!(changed.validate().is_err());
    let mut changed = original.clone();
    changed.indexes[0].replacement.as_mut().unwrap().root = "/foreign".into();
    assert!(changed.validate().is_err());
    let mut changed = original.clone();
    changed.index_scope = Some(changed.indexes[0].original.binding);
    assert!(
        changed.validate().is_err(),
        "a scoped observation must not include another index"
    );
    let mut changed = original.clone();
    changed.indexes[0].readiness = RebuildReadiness::Ready;
    assert!(
        changed.validate().is_err(),
        "readiness requires an actual native job result"
    );
    let mut changed = original.clone();
    changed.stage = RebuildStage::Failed;
    assert!(
        changed.validate().is_err(),
        "terminal failure requires its complete reason"
    );
    Ok(())
}

#[test]
fn selected_rebuild_stage_does_not_inherit_sibling_failure_or_pause() -> Result<()> {
    use crate::service::{JobKind, JobState};
    let mut value = observation()?;
    let binding = value.indexes[0].replacement.as_ref().unwrap().binding;
    value.indexes[0].paused = false;
    value.indexes[0].readiness = RebuildReadiness::Ready;
    value.indexes[0].job = Some(JobRecord {
        id: Sha256Digest::from_bytes([8; 32]),
        project: binding,
        kind: JobKind::Index,
        state: JobState::Succeeded,
        fencing_generation: 1,
        attempts: 1,
        outcome: None,
    });
    value.stage = RebuildStage::Indexing;
    assert_eq!(
        value.scoped_for(binding)?.unwrap().stage,
        RebuildStage::Ready
    );
    value.indexes[1].readiness = RebuildReadiness::Failed;
    value.indexes[1].failure = Some(RebuildFailure {
        code: "source_unavailable".into(),
        message: "Sibling source unavailable".into(),
    });
    value.stage = RebuildStage::Failed;
    let selected = value.scoped_for(binding)?.unwrap();
    assert_eq!(selected.stage, RebuildStage::Ready);
    assert!(selected.failure.is_none());
    assert!(selected.indexes[0].failure.is_none());
    value.failure = Some(RebuildFailure {
        code: "preparation_failed".into(),
        message: "Another index at /private/sibling failed".into(),
    });
    let selected = value.scoped_for(binding)?.unwrap();
    assert_eq!(selected.stage, RebuildStage::Failed);
    assert_eq!(
        selected.failure.as_ref().unwrap().code,
        "preparation_failed"
    );
    assert!(!serde_json::to_string(&selected)?.contains("/private/sibling"));
    assert!(
        selected
            .failure
            .as_ref()
            .unwrap()
            .message
            .contains("unavailable")
    );
    Ok(())
}

#[test]
fn unavailable_rebuild_history_keeps_exact_scope_and_reason_without_inventing_a_job_result()
-> Result<()> {
    let mut value = observation()?;
    value.stage = RebuildStage::Unavailable;
    value.indexes[0].readiness = RebuildReadiness::Unavailable;
    value.indexes[0].failure = Some(RebuildFailure {
        code: "rebuild_job_unavailable".into(),
        message: "Original native job receipt is unavailable; its execution outcome is unknown."
            .into(),
    });
    value.validate()?;
    let original = serde_json::to_value(&value.indexes[0])?;
    let selected = value
        .scoped_for(value.indexes[0].original.binding)?
        .unwrap();
    assert_eq!(selected.stage, RebuildStage::Unavailable);
    assert_eq!(selected.indexes[0].readiness, RebuildReadiness::Unavailable);
    assert!(selected.indexes[0].job.is_none());
    assert_eq!(serde_json::to_value(&selected.indexes[0])?, original);
    assert_eq!(
        selected.indexes[0].failure.as_ref().unwrap().code,
        "rebuild_job_unavailable"
    );
    let serialized = serde_json::to_value(&selected)?;
    assert_eq!(serialized["stage"], "unavailable");
    assert_eq!(serialized["indexes"][0]["readiness"], "unavailable");
    let round_trip: RebuildObservation = serde_json::from_value(serialized)?;
    round_trip.validate()?;
    assert_eq!(
        value
            .scoped_for(value.indexes[1].original.binding)?
            .unwrap()
            .stage,
        RebuildStage::Paused
    );
    // A missing observation must retain a reason, and cannot be promoted to
    // either successful execution or a failed native job by changing a label.
    let mut changed = selected.clone();
    changed.indexes[0].failure = None;
    assert!(changed.validate().is_err());
    let mut changed = selected.clone();
    changed.stage = RebuildStage::Ready;
    assert!(changed.validate().is_err());
    let mut changed = selected;
    changed.stage = RebuildStage::Failed;
    assert!(changed.validate().is_err());
    Ok(())
}

#[test]
fn operation_wide_observation_unavailability_remains_unavailable_in_selected_index() -> Result<()> {
    let mut value = observation()?;
    value.stage = RebuildStage::Unavailable;
    assert!(
        value.validate().is_err(),
        "unavailability without any reason was accepted"
    );
    value.failure = Some(RebuildFailure {
        code: "rebuild_registry_unavailable".into(),
        message: "Registry metadata at /private/operation cannot currently be read.".into(),
    });
    value.validate()?;
    let selected = value
        .scoped_for(value.indexes[1].original.binding)?
        .unwrap();
    assert_eq!(selected.stage, RebuildStage::Unavailable);
    assert_eq!(
        selected.failure.as_ref().unwrap().code,
        "rebuild_registry_unavailable"
    );
    assert!(!serde_json::to_string(&selected)?.contains("/private/operation"));
    assert_eq!(selected.indexes[0].readiness, RebuildReadiness::Paused);
    Ok(())
}
