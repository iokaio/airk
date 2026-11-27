// SPDX-License-Identifier: Apache-2.0
use super::*;

fn args(mode: Option<&str>) -> Map<String, Value> {
    let request = airk_core::arbiter::answer_loop::ContextRequestV3 {
        needs: Vec::new(),
        anchors: Vec::new(),
        deliverable: None,
        known: Vec::new(),
        schema: airk_core::arbiter::answer_loop::REQUEST_SCHEMA_V3.into(),
        question: "exact evidence".into(),
        budget: Default::default(),
        continuation: None,
    };
    let mut args = serde_json::to_value(request)
        .unwrap()
        .as_object()
        .unwrap()
        .clone();
    if let Some(mode) = mode {
        args.insert("verification".into(), json!(mode));
    }
    args
}
#[test]
fn whole_answer_controls_have_one_budget_and_children_never_receive_defaults() -> Result<()> {
    for mode in [None, Some("report"), Some("require"), Some("off")] {
        let original = args(mode);
        let owner = AnswerVerification::parse(&original)?;
        assert_eq!(owner.budget.is_some(), mode != Some("off"));
        let mut child = original.clone();
        owner.child_arguments(&mut child);
        assert_eq!(child["verification"], "off");
        assert_eq!(child["budget"], original["budget"]);
        assert_eq!(child["question"], original["question"]);
        assert!(!child.contains_key("verification_max_sources"));
        if let Some(budget) = owner.budget {
            assert_eq!(
                (budget.max_sources, budget.max_bytes, budget.deadline_ms),
                (8, 16777216, 2000)
            );
        }
    }
    Ok(())
}
#[test]
fn explicit_source_byte_deadline_limits_and_exact_residual_receipt_capacity_are_preserved()
-> Result<()> {
    let mut original = args(Some("require"));
    original.insert("verification_max_sources".into(), json!(1));
    original.insert("verification_max_bytes".into(), json!(7));
    original.insert("verification_deadline_ms".into(), json!(9));
    let mut owner = AnswerVerification::parse(&original)?;
    owner.maximum = 4096;
    let body = json!({"native":{"future":[1,2,3]}});
    let capacity = owner.receipt_capacity(&body)?;
    assert_eq!(
        capacity,
        4096 - serde_json::to_vec(&body)?.len()
            - serde_json::to_vec("member_verification")?.len()
            - 2
            - owner.required_failure_bytes()?
    );
    let request = owner.arguments(body.clone(), 3, capacity)?;
    assert_eq!(request["verification_max_sources"], 1);
    assert_eq!(request["verification_max_bytes"], 7);
    assert_eq!(request["verification_deadline_ms"], 9);
    assert_eq!(request["remaining_deadline_ms"], 3);
    assert_eq!(request["projection"], body);
    let mut child = original;
    owner.child_arguments(&mut child);
    assert!(!child.contains_key("verification_max_bytes"));
    let minimum = owner.minimum_attachment_bytes()?;
    assert!(minimum > 1024 && minimum < 2048);
    owner.reserve_delivery_bytes(100)?;
    assert_eq!(owner.receipt_capacity(&body)?, capacity - 100);
    assert!(owner.reserve_delivery_bytes(4096).is_err());
    Ok(())
}

#[test]
fn two_members_share_one_exact_source_raw_byte_and_deadline_allocation() -> Result<()> {
    use airk_core::{
        combined_reads::{ResolvedMember, ResolvedSelection},
        index_catalog::{PathScope, ScopeAtom, SourceRootId},
        scoped_reads::{ScopeSelectionObservation, SelectionOwnerKind},
    };
    let selected = ResolvedSelection {
        project_ids: Vec::new(),
        selection: ScopeSelectionObservation {
            owner: SelectionOwnerKind::Agent,
            scope_identity: Sha256Digest::from_bytes([8; 32]),
            catalog_revision: CatalogRevision::INITIAL,
        },
        member_availability: MemberAvailability::Report,
        members: (1..=2)
            .map(|n| ResolvedMember {
                index_id: IndexId::from_identity(Incarnation::from_bytes([n; 16])),
                memberships: Vec::new(),
                scope: vec![ScopeAtom {
                    source_root_id: SourceRootId::from_identity(Incarnation::from_bytes([3; 16])),
                    kind: PathScope::Subtree,
                    path: String::new(),
                }],
            })
            .collect(),
    };
    let allocation = SharedBudgets {
        limit: 2,
        max_bytes: 65536,
        deadline_ms: 30000,
        limit_per_index: None,
        verification_max_sources: 1,
        verification_max_bytes: 7,
        verification_deadline_ms: 3,
    }
    .allocate(&selected)?;
    assert_eq!(
        allocation
            .iter()
            .map(|part| part.verification_max_sources)
            .collect::<Vec<_>>(),
        vec![1, 0]
    );
    assert_eq!(
        allocation
            .iter()
            .map(|part| part.verification_max_bytes)
            .collect::<Vec<_>>(),
        vec![4, 3]
    );
    assert_eq!(
        allocation
            .iter()
            .map(|part| part.verification_deadline_ms)
            .collect::<Vec<_>>(),
        vec![2, 1]
    );
    Ok(())
}
#[test]
fn report_and_require_preserve_full_native_data_and_cannot_claim_unperformed_checks() -> Result<()>
{
    let original = json!({"content":[{"type":"text","text":"{\"future\":{\"kept\":[1,2,3]}}"}],"structuredContent":{"future":{"kept":[1,2,3]}},"future_transport":{"retained":true},"isError":false});
    let failure = json!({"error":{"code":"verification_receipt_budget_exhausted","verification_performed":false,"retryable":false}});
    for mode in ["report", "require"] {
        let owner = AnswerVerification::parse(&args(Some(mode)))?;
        let result = owner.attach(original.clone(), failure.clone())?;
        assert_eq!(
            result["structuredContent"]["future"],
            original["structuredContent"]["future"]
        );
        assert_eq!(result["future_transport"], original["future_transport"]);
        assert_eq!(result["isError"], mode == "require");
        assert_eq!(result["structuredContent"]["member_verification"], failure);
        assert!(
            result["structuredContent"]
                .get("source_verification")
                .is_none()
        );
        let text: Value = serde_json::from_str(result["content"][0]["text"].as_str().unwrap())?;
        assert_eq!(text, result["structuredContent"]);
    }
    Ok(())
}
