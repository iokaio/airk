// SPDX-License-Identifier: Apache-2.0
use super::*;

fn identity() -> WorkerIdentity {
    WorkerIdentity {
        service: Incarnation::from_bytes([1; 16]),
        worker: Incarnation::from_bytes([2; 16]),
        project: ProjectBinding::from_bytes([3; 32]),
        fencing_generation: 4,
    }
}
fn observation() -> protocol::WorkerCheckpointObservation {
    protocol::WorkerCheckpointObservation {
        schema: 1,
        identity: identity(),
        operation_id: Incarnation::from_bytes([5; 16]),
        engine_sha256: airk_core::identity::Sha256Digest::from_bytes([6; 32]),
        admission_closed: true,
        sealed: true,
        active_work_grant: None,
        active_queries: 0,
        durable_boundary: true,
    }
}

#[test]
fn only_correlated_sealed_actual_quiescence_allows_native_stop() -> Result<()> {
    for mutation in 0..11 {
        let mut observed = observation();
        match mutation {
            0 => observed.identity.service = Incarnation::from_bytes([7; 16]),
            1 => observed.identity.worker = Incarnation::from_bytes([7; 16]),
            2 => observed.identity.project = ProjectBinding::from_bytes([7; 32]),
            3 => observed.identity.fencing_generation += 1,
            4 => observed.operation_id = Incarnation::from_bytes([7; 16]),
            5 => observed.engine_sha256 = airk_core::identity::Sha256Digest::from_bytes([7; 32]),
            6 => observed.admission_closed = false,
            7 => observed.sealed = false,
            8 => observed.active_work_grant = Some(Incarnation::from_bytes([7; 16])),
            9 => observed.active_queries = 1,
            _ => observed.durable_boundary = false,
        }
        let mut state = ReadSettlement::new(observation().operation_id);
        assert!(
            !state
                .accept(identity(), observation().engine_sha256, observed)
                .unwrap_or(false)
        );
        assert!(!matches!(state.phase, Phase::Sealed));
    }
    let mut state = ReadSettlement::new(observation().operation_id);
    assert!(state.accept(identity(), observation().engine_sha256, observation())?);
    assert!(matches!(state.phase, Phase::Sealed));
    assert!(!state.control_pending());
    Ok(())
}

#[test]
fn settlement_requests_are_bounded_exact_and_do_not_own_supervisor_lifetime() -> Result<()> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let sender = Arc::new(sender);
    let request = requester(Some(Arc::downgrade(&sender)));
    request(identity())?;
    assert!(
        request(identity()).is_err(),
        "full actor queue must not authorize settlement"
    );
    match receiver.try_recv()? {
        Envelope::SettleRead {
            identity: received, ..
        } => assert_eq!(received, identity()),
        _ => anyhow::bail!("unexpected supervisor message"),
    }
    drop(sender);
    assert!(request(identity()).is_err());
    assert!(matches!(
        receiver.try_recv(),
        Err(mpsc::TryRecvError::Disconnected)
    ));
    Ok(())
}
