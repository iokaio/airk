// SPDX-License-Identifier: Apache-2.0
//! Pure legacy-spec projection plus actual current native WordVector readiness.
use super::*;

#[test]
fn saved_unsupported_graph_spec_never_reports_ready_and_preserves_scoped_metadata() -> Result<()> {
    let (_owner, kernel) = pending_fixture()?;
    kernel.run_scheduled_enrichment()?;
    assert_eq!(
        kernel.status()?.semantic_readiness.state,
        SemanticState::Ready
    );
    let read = kernel.current_reader()?;
    let state = read.collection_state()?.context("fixture state missing")?;
    let pipeline = state.configuration.model.as_ref().unwrap().pipeline_digest;
    let graph = &read.head().graph_head.scope;
    assert!(
        graph_specification_readiness(graph, 2, &state, DefaultSearchPolicy::RequireComplete)?
            .is_none()
    );
    // Independent retained v1 recipe; production owner now advertises v2.
    let mut old = Sha256::new();
    old.update(b"airk-mutable-diskann-1\0diskann-0.59.0\0le-f32-adjacency-48\0l2\0pruned-degree-32\0build-list-64\0triangle-prune\0");
    old.update(2u32.to_be_bytes());
    old.update(pipeline.as_bytes());
    let mut legacy = graph.clone();
    legacy.specification = Sha256Digest::from_bytes(old.finalize().into());
    assert_ne!(legacy.specification, graph.specification);
    let unavailable =
        graph_specification_readiness(&legacy, 2, &state, DefaultSearchPolicy::RequireComplete)?
            .unwrap();
    assert_eq!(unavailable.state, SemanticState::Unavailable);
    assert_eq!(
        unavailable.default_search_policy,
        DefaultSearchPolicy::RequireComplete
    );
    assert_eq!(unavailable.selected_artifacts, state.counts.vector_eligible);
    assert_eq!(
        (unavailable.completed_artifacts, unavailable.completed_units),
        (0, 0)
    );
    assert_eq!(unavailable.pipeline_digest, Some(pipeline));
    assert_eq!(
        unavailable.selection_digest,
        Some(state.configuration.vector_selection.digest()?)
    );
    assert!(
        unavailable.selected_units.is_none()
            && unavailable.first_ready_unix_ms.is_none()
            && unavailable.fully_ready_unix_ms.is_none()
    );
    assert!(unavailable.failure.unwrap().contains("Rebuild the index"));
    let mut malformed = graph.clone();
    malformed.record_bytes -= 4;
    assert_eq!(
        graph_specification_readiness(&malformed, 2, &state, DefaultSearchPolicy::RequireComplete)?
            .unwrap()
            .state,
        SemanticState::Unavailable
    );
    let mut disabled = state.clone();
    disabled.configuration.model = None;
    assert!(
        graph_specification_readiness(&legacy, 2, &disabled, DefaultSearchPolicy::RequireComplete)?
            .is_none()
    );
    Ok(())
}
