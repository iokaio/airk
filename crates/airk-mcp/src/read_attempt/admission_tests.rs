// SPDX-License-Identifier: Apache-2.0
use super::*;
fn arguments(ms: u64) -> serde_json::Map<String, serde_json::Value> {
    let request = airk_core::arbiter::context_action::ContextRequestV2 {
        schema: airk_core::arbiter::context_action::REQUEST_SCHEMA_V2.into(),
        question: "original ceiling".into(),
        budget: airk_core::arbiter::Budget {
            deadline_ms: ms,
            ..Default::default()
        },
    };
    serde_json::to_value(request)
        .unwrap()
        .as_object()
        .unwrap()
        .clone()
}
#[test]
fn answer_ceiling_is_anchored_before_setup_and_can_only_narrow_original_parent() {
    let admitted = Instant::now();
    let parent = ReadAttemptContext::new(admitted + Duration::from_secs(90), Default::default());
    let narrowed = admit_tool_context_at("airk_answer", &arguments(2000), parent.clone(), admitted);
    let after_setup = admitted + Duration::from_millis(1500);
    let repeated = admit_tool_context_at(
        "airk_answer",
        &arguments(2000),
        narrowed.clone(),
        after_setup,
    );
    assert_eq!(repeated.deadline(), admitted + Duration::from_secs(2));
    assert_eq!(
        repeated.deadline().saturating_duration_since(after_setup),
        Duration::from_millis(500)
    );
    assert_eq!(
        admit_tool_context_at("airk_search", &arguments(2000), parent.clone(), after_setup)
            .deadline(),
        parent.deadline()
    );
    let short = ReadAttemptContext::new(admitted + Duration::from_secs(2), Default::default());
    assert_eq!(
        admit_tool_context_at("airk_answer", &arguments(30000), short, admitted).deadline(),
        admitted + Duration::from_secs(2)
    );
    parent.cancellation().cancel();
    assert!(
        repeated.cancellation().is_cancelled(),
        "narrowing retains the same parent cancellation"
    );
}
#[test]
fn malformed_answer_arguments_keep_the_existing_canonical_error_owner() {
    let admitted = Instant::now();
    let parent = ReadAttemptContext::new(admitted + Duration::from_secs(90), Default::default());
    let invalid = serde_json::json!({"budget":{"deadline_ms":0},"unexpected":true})
        .as_object()
        .unwrap()
        .clone();
    assert_eq!(
        admit_tool_context_at("airk_answer", &invalid, parent.clone(), admitted).deadline(),
        parent.deadline()
    );
}

#[test]
fn public_routing_controls_share_one_original_answer_ceiling_before_setup() {
    let admitted = Instant::now();
    for routing in [
        serde_json::json!({"project":"ab".repeat(32)}),
        serde_json::json!({"project_ids":[format!("prj_{}","ab".repeat(16))],"index_ids":[format!("idx_{}","cd".repeat(16))],"member_availability":"require"}),
        serde_json::json!({"origin_publication":"original-publication"}),
    ] {
        let mut request = arguments(2000);
        request.extend(routing.as_object().unwrap().clone());
        let unchanged = request.clone();
        let parent =
            ReadAttemptContext::new(admitted + Duration::from_secs(90), Default::default());
        let original = admit_tool_context_at("answer", &request, parent.clone(), admitted);
        let after_setup = admitted + Duration::from_millis(1500);
        let repeated = admit_tool_context_at("airk_answer", &request, original, after_setup);
        assert_eq!(repeated.deadline(), admitted + Duration::from_secs(2));
        assert_eq!(
            repeated.deadline().saturating_duration_since(after_setup),
            Duration::from_millis(500)
        );
        assert_eq!(
            request, unchanged,
            "admission must not strip selectors from the canonical call"
        );
        assert_eq!(
            admit_tool_context_at("airk_search", &request, parent.clone(), admitted).deadline(),
            parent.deadline()
        );
        request.insert("unrecognized_argument".into(), true.into());
        assert_eq!(
            admit_tool_context_at("airk_answer", &request, parent.clone(), admitted).deadline(),
            parent.deadline(),
            "unknown arguments retain canonical validation and error ownership"
        );
    }
}
