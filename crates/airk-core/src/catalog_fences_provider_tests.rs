// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn completed_native_history_is_explicit_and_helper_wire_is_unchanged() -> Result<()> {
    let service = Incarnation::from_bytes([1; 16]);
    let worker = WorkerIdentity {
        service,
        worker: Incarnation::from_bytes([2; 16]),
        project: Sha256Digest::from_bytes([3; 32]),
        fencing_generation: 1,
    };
    let anchor = crate::inference_provider::CompletionLedgerObservation {
        schema: 1,
        namespace: Incarnation::from_bytes([4; 16]),
        service_instance: service,
        approved_execution_sha256: Sha256Digest::from_bytes([5; 32]),
        factory_generation: Incarnation::from_bytes([6; 16]),
        completed_owners: 100,
        admitted_high_water: 200,
        head_sha256: Sha256Digest::from_bytes([7; 32]),
    };
    let mut plan =
        FencePlan::new(service, 1, Vec::new())?.with_worker_obligation(WorkerDrainObligation {
            index_id: IndexId::from_identity(Incarnation::from_bytes([8; 16])),
            backing: worker.project,
            workers: vec![worker],
            providers: vec![WorkerProviderObligation {
                worker,
                history: WorkerProviderHistory::InProcess {
                    generations: Vec::new(),
                    completed_authorizations: Some(anchor.clone()),
                },
            }],
        })?;
    let bytes = serde_json::to_vec(&plan)?;
    let decoded: FencePlan = serde_json::from_slice(&bytes)?;
    assert_eq!(decoded, plan);
    assert!(String::from_utf8(bytes)?.contains("completed_authorizations"));
    let provider = &mut plan.worker_obligation.as_mut().unwrap().providers[0];
    provider.history = WorkerProviderHistory::InProcess {
        generations: Vec::new(),
        completed_authorizations: None,
    };
    assert!(
        plan.validate().is_err(),
        "unknown authorization cannot become never-authorized"
    );
    let provider = &mut plan.worker_obligation.as_mut().unwrap().providers[0];
    let mut foreign = anchor;
    foreign.service_instance = Incarnation::from_bytes([9; 16]);
    provider.history = WorkerProviderHistory::InProcess {
        generations: Vec::new(),
        completed_authorizations: Some(foreign),
    };
    assert!(plan.validate().is_err());
    let old = WorkerProviderHistory::Helpers {
        generations: vec![Incarnation::from_bytes([10; 16])],
    };
    assert_eq!(
        serde_json::to_value(&old)?,
        serde_json::json!({"kind":"helpers","generations":[Incarnation::from_bytes([10;16]).to_string()]})
    );
    Ok(())
}
