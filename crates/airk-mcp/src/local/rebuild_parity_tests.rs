// SPDX-License-Identifier: Apache-2.0
//! Canonical native settings/status and actual MCP SDK duplex transport.
use super::*;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    rebuild::*,
    service::ProjectRegistration,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

fn body(reply: CallToolResult) -> Result<Value> {
    ensure!(!reply.is_error.unwrap_or(false), "{reply:?}");
    Ok(serde_json::from_str(
        &reply.content[0]
            .as_text()
            .context("missing MCP result text")?
            .text,
    )?)
}

#[tokio::test]
async fn canonical_status_settings_and_mcp_retain_complete_scoped_rebuild_observation() -> Result<()>
{
    assert_rebuild_parity(RebuildStage::Paused, false).await
}

#[tokio::test]
async fn unavailable_rebuild_history_keeps_full_canonical_status_settings_and_mcp_results()
-> Result<()> {
    assert_rebuild_parity(RebuildStage::Unavailable, false).await
}

#[tokio::test]
async fn clean_break_scoped_cleanup_keeps_complete_native_and_mcp_results_without_sibling_paths()
-> Result<()> {
    assert_rebuild_parity(RebuildStage::Ready, true).await
}

async fn assert_rebuild_parity(stage: RebuildStage, clean_break: bool) -> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    std::fs::create_dir(&source)?;
    std::fs::write(source.join("sample.rs"), b"pub fn copper() {}\n")?;
    let kernel = Kernel::open_service_collection(
        &source,
        Some(owner.path().join("replacement")),
        "original",
    )?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let replacement = ProjectRegistration::new(
        kernel.root().to_str().unwrap().into(),
        kernel.data_dir().to_str().unwrap().into(),
        kernel.collection().into(),
    )?;
    let original = ProjectRegistration::new(
        replacement.root.clone(),
        owner
            .path()
            .join("preserved")
            .to_string_lossy()
            .into_owned(),
        replacement.collection.clone(),
    )?;
    let mut observation = RebuildObservation {
        schema: 1,
        mode: None,
        indexes_complete: None,
        expected_indexes: None,
        cleanup: None,
        operation_id: Incarnation::from_bytes([1; 16]),
        stage,
        provenance: RebuildProvenance::NativeRebuildFromSavedConfiguration,
        candidate: Sha256Digest::from_bytes([2; 32]),
        predecessor: Sha256Digest::from_bytes([3; 32]),
        original_namespace: Incarnation::from_bytes([4; 16]),
        preserved_state: owner
            .path()
            .join("old-state")
            .to_string_lossy()
            .into_owned(),
        preserved_control: owner
            .path()
            .join("old-state/control")
            .to_string_lossy()
            .into_owned(),
        preserved_index_roots: vec![original.data_root.clone()],
        successor_state: owner
            .path()
            .join("new-state")
            .to_string_lossy()
            .into_owned(),
        successor_control: owner
            .path()
            .join("new-state/control")
            .to_string_lossy()
            .into_owned(),
        successor_namespace: Some(Incarnation::from_bytes([5; 16])),
        original_failure: None,
        failure: None,
        index_scope: None,
        indexes: vec![RebuildIndexObservation {
            original,
            replacement: Some(replacement.clone()),
            sources: Default::default(),
            settings_available: None,
        discarded_preferences: Vec::new(),
        settings_sha256: Sha256Digest::from_bytes([6; 32]),
            original_paused: true,
            paused: true,
            watching: false,
            initial_job: None,
        job: None,
            readiness: if stage == RebuildStage::Unavailable {
                RebuildReadiness::Unavailable
            } else {
                RebuildReadiness::Paused
            },
            failure: (stage == RebuildStage::Unavailable).then(|| RebuildFailure {
                code: "rebuild_job_unavailable".into(),
                message: "The original native job receipt is no longer available; its outcome cannot be verified.".into(),
            }),
        }],
    };
    let sibling_private = "/private/sibling-cleanup-must-not-leak";
    if clean_break {
        use airk_core::service::{JobKind, JobOutcome, JobRecord, JobState};
        observation.mode = Some(RebuildMode::CleanBreak);
        observation.indexes_complete = Some(true);
        observation.expected_indexes = Some(2);
        let mut sibling = observation.indexes[0].clone();
        sibling.original = ProjectRegistration::new(
            "/private/sibling-source".into(),
            sibling_private.into(),
            "sibling".into(),
        )?;
        sibling.replacement = Some(ProjectRegistration::new(
            "/private/sibling-source".into(),
            "/private/sibling-new-data".into(),
            "sibling".into(),
        )?);
        observation
            .preserved_index_roots
            .push(sibling_private.into());
        observation.indexes.push(sibling);
        for (position, index) in observation.indexes.iter_mut().enumerate() {
            index.readiness = RebuildReadiness::Ready;
            index.job = Some(JobRecord {
                id: Sha256Digest::from_bytes([21 + position as u8; 32]),
                project: index.replacement.as_ref().unwrap().binding,
                kind: JobKind::Index,
                state: JobState::Succeeded,
                fencing_generation: 1,
                attempts: 1,
                outcome: Some(JobOutcome::Succeeded { revision: None }),
            });
        }
        observation.cleanup = Some(RebuildCleanup {
            complete: false,
            state: "blocked".into(),
            scope: "administrative".into(),
            failure: Some(RebuildFailure {
                code: "clean_break_cleanup_blocked".into(),
                message: format!("Retained sibling file at {sibling_private}"),
            }),
            observation: Some(json!({"complete":false,"retired_collections":[sibling_private]})),
        });
    }
    observation.validate()?;
    let expected = serde_json::to_value(observation.scoped_for(replacement.binding)?.unwrap())?;
    if clean_break {
        assert_eq!(expected["cleanup"]["scope"], "selected_index_summary");
        assert!(expected["cleanup"]["observation"].is_null());
        assert_eq!(
            expected["cleanup"]["failure"]["code"],
            "clean_break_cleanup_blocked"
        );
        assert_eq!(expected["expected_indexes"], 1);
        assert!(!expected.to_string().contains(sibling_private));
    }
    let kernel =
        kernel.with_rebuild_observer(replacement.binding, move || Ok(Some(observation.clone())));
    assert_eq!(kernel.status_summary()?["rebuild"], expected);
    assert_eq!(serde_json::to_value(kernel.status()?)?["rebuild"], expected);
    let plan = kernel.evidence_plan(serde_json::from_value(
        json!({"schema":"airk.parse.request/1","mode":"execute",
        "plan":{"steps":[{"id":"readiness","tool":"status","args":{}}]}}),
    )?)?;
    assert_eq!(
        plan.steps[0]
            .result
            .as_ref()
            .context("Workbench status result absent")?["rebuild"],
        expected,
        "actual Workbench native plan omitted recovery metadata"
    );
    let mut tools = LocalTools::new(kernel).await?;
    for tool in ["airk_status", "airk_diagnostics"] {
        let reply = tools
            .call(
                tool.into(),
                json!({"response_mode":"full"}).as_object().unwrap().clone(),
            )
            .await?;
        if clean_break {
            assert!(
                !serde_json::to_string(&reply)?.contains(sibling_private),
                "actual MCP text or structured transport leaked administrative sibling paths"
            );
        }
        let result = body(reply)?;
        assert_eq!(
            result["rebuild"], expected,
            "actual MCP {tool} changed native recovery metadata"
        );
    }
    let page: airk_kernel::usage_page::Page = serde_json::from_value(
        tools
            .read_native(
                NativeRead::Settings {
                    view: SettingsView::Roots,
                    cursor: None,
                    max_bytes: 48 << 10,
                },
                replacement.binding,
            )
            .await?,
    )?;
    assert!(page.cursor.is_none());
    ensure!(
        page.items.len() == 1 && page.items[0].path.is_empty(),
        "unexpected paginated fixture body"
    );
    assert_eq!(
        page.items[0].value["rebuild"], expected,
        "canonical NativeRead settings omitted recovery metadata"
    );
    if clean_break {
        assert!(!serde_json::to_string(&page)?.contains(sibling_private));
    }
    tools.close().await?;
    Ok(())
}
