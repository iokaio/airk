// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    identity::Sha256Digest,
    install_record::{SETUP_RECEIPT_SCHEMA, SetupFailure, SetupStep},
};

fn receipt() -> SetupReceipt {
    SetupReceipt {
        schema: SETUP_RECEIPT_SCHEMA.into(),
        operation_id: Incarnation::from_bytes([1; 16]),
        operation_sha256: Sha256Digest::from_bytes([3; 32]),
        kind: OperationKind::Install,
        expires_unix_ms: 10_000,
        steps: [
            (SetupPhase::RecordWritten, "written"),
            (SetupPhase::PointerWritten, "written"),
            (SetupPhase::Complete, "closed"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (phase, outcome))| SetupStep {
            phase,
            outcome: outcome.into(),
            detail: format!("original native detail {index}"),
            at_unix_ms: index as u64 + 1,
        })
        .collect(),
        retired: vec!["retired fixture".into()],
        leftovers: vec![],
        failure: None,
        closed: true,
    }
}

#[test]
fn native_setup_observation_preserves_full_receipts_and_refuses_false_completion() {
    let original = receipt();
    let mut cases = vec![("complete", original.clone(), true)];
    let mut open = original.clone();
    open.closed = false;
    cases.push(("open", open, false));
    let mut failed = original.clone();
    failed.failure = Some(SetupFailure {
        code: "native_fixture_failure".into(),
        message: "original failure evidence".into(),
        at_unix_ms: 4,
    });
    cases.push(("closed_failure", failed, false));
    let mut missing = original.clone();
    missing.steps.remove(0);
    cases.push(("missing_record", missing, false));
    let mut reversed = original.clone();
    reversed.steps.swap(0, 1);
    cases.push(("reversed_publication", reversed, false));
    let mut unsuccessful = original.clone();
    unsuccessful.steps[1].outcome = "failed".into();
    cases.push(("failed_pointer", unsuccessful, false));
    let mut late = original.clone();
    late.steps.push(late.steps[0].clone());
    cases.push(("nonterminal_completion", late, false));
    let mut rows = Vec::new();
    for (name, input, expected) in cases {
        let full = serde_json::to_value(&input).unwrap();
        let observed = classify(
            input,
            original.operation_id,
            Incarnation::from_bytes([2; 16]),
        )
        .unwrap();
        assert_eq!(observed.successful, expected, "{name}");
        assert_eq!(
            observed.receipt, full,
            "{name}: full original native result survives"
        );
        rows.push(
            serde_json::json!({"name":name,"expected_successful":expected,"observation":observed}),
        );
    }
    if let Some(output) = std::env::var_os("AIRK_SETUP_RECEIPT_CASE_OUTPUT") {
        std::fs::write(
            output,
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema":"airk.setup-receipt-semantic-cases/1","cases":rows
            }))
            .unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn native_setup_observation_rejects_foreign_and_removal_receipts() {
    let original = receipt();
    let install = Incarnation::from_bytes([2; 16]);
    assert!(classify(original.clone(), Incarnation::from_bytes([4; 16]), install).is_err());
    for kind in [OperationKind::Uninstall, OperationKind::Purge] {
        let mut removal = original.clone();
        removal.kind = kind;
        assert!(classify(removal, original.operation_id, install).is_err());
    }
}
