// SPDX-License-Identifier: Apache-2.0
use super::super::tests::{fixture, open};
use super::*;
struct ResumeOnDrop(Option<std::sync::mpsc::SyncSender<()>>);
impl Drop for ResumeOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}
fn identity(grant: &protocol::Grant) -> WorkerIdentity {
    WorkerIdentity {
        service: grant.service,
        worker: grant.worker,
        project: grant.project.binding,
        fencing_generation: 1,
    }
}
fn ticket(grant: &protocol::Grant) -> protocol::ProviderDrainTicket {
    protocol::ProviderDrainTicket {
        helper: grant.pool,
        worker: identity(grant),
        ticket: Incarnation::from_bytes([99; 16]),
    }
}
fn phase(observation: protocol::Observation) -> protocol::ProviderDrainObservation {
    let protocol::Observation::Drain { observation, .. } = observation else {
        panic!("missing correlated drain reply")
    };
    observation
}
fn request(
    grant: &protocol::Grant,
    session: Incarnation,
    alive: Arc<AtomicBool>,
    worker_alive: Arc<AtomicBool>,
) -> (Work, oneshot::Receiver<protocol::ResultEnvelope>) {
    let operation = protocol::Operation::Call {
        request: Box::new(Request::Embed {
            texts: vec!["cobalt".into()],
            workers: 1,
            memory_bytes: 8 << 20,
        }),
    };
    let (reply, receiver) = oneshot::channel();
    let envelope = protocol::Envelope {
        project: grant.project.binding,
        worker: grant.worker,
        session,
        request_id: Incarnation::from_bytes([88; 16]),
        input_digest: protocol::input_digest(&operation).unwrap(),
        operation,
    };
    (
        Work::Request {
            grant: Box::new(grant.clone()),
            envelope: Box::new(envelope),
            alive,
            worker_alive,
            reply,
            native_worker: Some(identity(grant)),
        },
        receiver,
    )
}

#[test]
fn native_compute_context_is_exact_and_cleared_after_failed_and_successful_calls() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (grant, location, model) = fixture(root.path(), "compute-context", "shared")?;
    let session = Incarnation::from_bytes([5; 16]);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let observed = seen.clone();
    let mut engine =
        Engine::new(root.path())?.with_compute_hook(Box::new(move |context, request| {
            assert!(matches!(request, Request::Embed { .. }));
            observed
                .lock()
                .unwrap()
                .push(*context.expect("native call observation"));
        }));
    engine.request(&grant, session, open(location, model))?;
    let alive = Arc::new(AtomicBool::new(true));
    let worker_alive = Arc::new(AtomicBool::new(true));
    for (number, valid) in [(81u8, false), (82u8, true)] {
        let (mut work, reply) = request(&grant, session, alive.clone(), worker_alive.clone());
        let Work::Request { envelope, .. } = &mut work else {
            unreachable!()
        };
        envelope.request_id = Incarnation::from_bytes([number; 16]);
        if !valid {
            envelope.operation = protocol::Operation::Call {
                request: Box::new(Request::Embed {
                    texts: Vec::new(),
                    workers: 1,
                    memory_bytes: 8 << 20,
                }),
            };
            envelope.input_digest = protocol::input_digest(&envelope.operation)?;
        }
        process_work(&mut engine, work, &AtomicBool::new(false));
        let response = reply.blocking_recv()?.response;
        assert_eq!(matches!(response, Response::Embedded { .. }), valid);
        assert!(
            engine.compute_context.is_none(),
            "call context survived a normal Result outcome"
        );
    }
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    for (context, number) in seen.iter().zip([81u8, 82u8]) {
        assert_eq!(context.worker, Some(identity(&grant)));
        assert_eq!(context.helper, grant.pool);
        assert_eq!(context.request_id, Incarnation::from_bytes([number; 16]));
        assert_eq!(context.session, session);
    }
    Ok(())
}

#[test]
fn revocation_during_model_selection_never_starts_native_inference() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (grant, location, model) = fixture(root.path(), "selection-race", "shared")?;
    let session = Incarnation::from_bytes([8; 16]);
    let mut engine = Engine::new(root.path())?;
    engine.request(&grant, session, open(location, model))?;
    let worker = Arc::new(AtomicBool::new(true));
    let revoked = worker.clone();
    engine.after_model_selection = Some(Box::new(move || revoked.store(false, Ordering::Release)));
    let computes = Arc::new(AtomicUsize::new(0));
    let counted = computes.clone();
    engine.before_compute = Some(Box::new(move |_, _| {
        counted.fetch_add(1, Ordering::AcqRel);
    }));
    let connection = AtomicBool::new(true);
    let closing = AtomicBool::new(false);
    let response = engine.request_guarded(
        &grant,
        session,
        Request::Embed {
            texts: vec!["cobalt".into()],
            workers: 1,
            memory_bytes: 8 << 20,
        },
        Some(RequestOwner {
            connection: &connection,
            worker: &worker,
            closing: &closing,
        }),
    );
    assert!(response.is_err());
    assert_eq!(computes.load(Ordering::Acquire), 0);
    Ok(())
}

#[test]
fn actual_engine_barrier_waits_for_inflight_compute_and_cancels_expired_auth_waiters() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let (first, location, model) = fixture(root.path(), "drain-first", "shared")?;
    let (mut sibling, other, other_model) = fixture(root.path(), "drain-sibling", "shared")?;
    sibling.token = Sha256Digest::from_bytes([4; 32]);
    let authority = (first.service, first.pool);
    let mut grants = BTreeMap::new();
    let mut retirements = Retirements::default();
    for grant in [&first, &sibling] {
        assert!(matches!(
            retirements.register(
                &mut grants,
                authority,
                grant.clone(),
                Some(identity(grant)),
                Instant::now()
            ),
            protocol::Observation::Registered
        ));
    }
    let first_alive = grants[&first.token].alive.clone();
    let first_worker = grants[&first.token].worker_alive.clone();
    let sibling_alive = grants[&sibling.token].alive.clone();
    let sibling_worker = grants[&sibling.token].worker_alive.clone();
    let session = Incarnation::from_bytes([5; 16]);
    let mut engine = Engine::new(root.path())?;
    engine.admit_owner(first.worker, &first_alive)?;
    engine.request(&first, session, open(location, model))?;
    engine.admit_owner(sibling.worker, &sibling_alive)?;
    engine.request(&sibling, session, open(other, other_model))?;
    let (started_tx, started) = std::sync::mpsc::sync_channel(1);
    let (resume_tx, resume) = std::sync::mpsc::sync_channel(1);
    let mut resume_on_drop = ResumeOnDrop(Some(resume_tx));
    let computes = Arc::new(AtomicUsize::new(0));
    let counted = computes.clone();
    engine.before_compute = Some(Box::new(move |_, _| {
        if counted.fetch_add(1, Ordering::AcqRel) == 0 {
            started_tx.send(()).unwrap();
            resume.recv().unwrap();
        }
    }));
    let (sender, mut queue) = mpsc::channel(8);
    let thread = std::thread::spawn(move || {
        while let Some(work) = queue.blocking_recv() {
            process_work(&mut engine, work, &AtomicBool::new(false));
        }
        engine
    });
    let (running, canceled_waiter) =
        request(&first, session, first_alive.clone(), first_worker.clone());
    drop(canceled_waiter);
    sender
        .try_send(running)
        .map_err(|_| anyhow::anyhow!("fixture queue full"))?;
    started.recv_timeout(Duration::from_secs(5))?;
    // Drop the auth table entry while actual accepted work and its exact
    // connection flag survive. Native membership must still fence them.
    grants.remove(&first.token);
    assert!(first_alive.load(Ordering::Acquire));
    let (waiting, waiting_reply) =
        request(&first, session, first_alive.clone(), first_worker.clone());
    sender
        .try_send(waiting)
        .map_err(|_| anyhow::anyhow!("fixture queue full"))?;
    let ticket = ticket(&first);
    assert_eq!(
        phase(retirements.control(Control::Begin, authority, &mut grants, &sender, ticket)),
        protocol::ProviderDrainObservation::Pending
    );
    assert!(!first_worker.load(Ordering::Acquire));
    assert!(sibling_worker.load(Ordering::Acquire));
    assert_eq!(
        phase(retirements.control(Control::Poll, authority, &mut grants, &sender, ticket)),
        protocol::ProviderDrainObservation::Pending
    );
    assert_eq!(
        phase(retirements.control(Control::Forget, authority, &mut grants, &sender, ticket)),
        protocol::ProviderDrainObservation::Refused
    );
    let (sibling_work, sibling_reply) = request(&sibling, session, sibling_alive, sibling_worker);
    sender
        .try_send(sibling_work)
        .map_err(|_| anyhow::anyhow!("fixture queue full"))?;
    resume_on_drop.0.take().unwrap().send(())?;
    assert!(matches!(
        waiting_reply.blocking_recv()?.response,
        Response::Failed { .. }
    ));
    assert!(matches!(
        sibling_reply.blocking_recv()?.response,
        Response::Embedded { .. }
    ));
    assert_eq!(
        phase(retirements.control(Control::Poll, authority, &mut grants, &sender, ticket)),
        protocol::ProviderDrainObservation::Drained
    );
    assert_eq!(
        computes.load(Ordering::Acquire),
        2,
        "a revoked waiting inference reached compute"
    );
    assert_eq!(
        retirements.drains.len(),
        1,
        "canceling a waiter released its obligation"
    );
    drop(sender);
    let engine = thread
        .join()
        .map_err(|_| anyhow::anyhow!("fixture engine failed"))?;
    assert!(!engine.owners.contains_key(&first.worker));
    assert!(engine.sessions.contains_key(&(sibling.worker, session)));
    Ok(())
}

#[test]
fn full_engine_queue_keeps_pending_ticket_and_old_waiters_stay_revoked_after_forget() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let (first, _, _) = fixture(root.path(), "full-queue", "unused")?;
    let authority = (first.service, first.pool);
    let mut grants = BTreeMap::new();
    let mut retirement = Retirements::default();
    retirement.register(
        &mut grants,
        authority,
        first.clone(),
        Some(identity(&first)),
        Instant::now(),
    );
    let old_alive = grants[&first.token].alive.clone();
    let worker_alive = grants[&first.token].worker_alive.clone();
    let (stale, reply) = request(
        &first,
        Incarnation::from_bytes([8; 16]),
        old_alive,
        worker_alive,
    );
    let (sender, mut queue) = mpsc::channel(1);
    sender
        .try_send(Work::Sweep)
        .map_err(|_| anyhow::anyhow!("fixture queue full"))?;
    let ticket = ticket(&first);
    assert_eq!(
        phase(retirement.control(Control::Begin, authority, &mut grants, &sender, ticket)),
        protocol::ProviderDrainObservation::Pending
    );
    assert!(!retirement.drains[&ticket.ticket].queued);
    assert_eq!(
        phase(retirement.control(Control::Poll, authority, &mut grants, &sender, ticket)),
        protocol::ProviderDrainObservation::Pending
    );
    let mut engine = Engine::new(root.path())?;
    process_work(&mut engine, queue.try_recv()?, &AtomicBool::new(false));
    retirement.retry(&sender);
    assert!(retirement.drains[&ticket.ticket].queued);
    process_work(&mut engine, queue.try_recv()?, &AtomicBool::new(false));
    assert_eq!(
        phase(retirement.control(Control::Poll, authority, &mut grants, &sender, ticket)),
        protocol::ProviderDrainObservation::Drained
    );
    let replacement = protocol::Grant {
        token: Sha256Digest::from_bytes([8; 32]),
        ..first.clone()
    };
    assert!(matches!(
        retirement.register(
            &mut grants,
            authority,
            replacement.clone(),
            Some(identity(&first)),
            Instant::now()
        ),
        protocol::Observation::Failed
    ));
    assert_eq!(
        phase(retirement.control(Control::Forget, authority, &mut grants, &sender, ticket)),
        protocol::ProviderDrainObservation::Forgotten
    );
    assert!(matches!(
        retirement.register(
            &mut grants,
            authority,
            replacement,
            Some(identity(&first)),
            Instant::now()
        ),
        protocol::Observation::Registered
    ));
    process_work(&mut engine, stale, &AtomicBool::new(false));
    assert!(matches!(
        reply.blocking_recv()?.response,
        Response::Failed { .. }
    ));
    Ok(())
}

#[test]
fn drain_membership_bounds_and_foreign_full_identity_or_generation_refuse() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (template, _, _) = fixture(root.path(), "bounds", "unused")?;
    let authority = (template.service, template.pool);
    let mut grants = BTreeMap::new();
    let mut retirement = Retirements::default();
    let (sender, _queue) = mpsc::channel(1);
    let mut first_ticket = None;
    for value in 1u8..=32 {
        let grant = protocol::Grant {
            worker: Incarnation::from_bytes([value; 16]),
            token: Sha256Digest::from_bytes([value; 32]),
            ..template.clone()
        };
        assert!(matches!(
            retirement.register(
                &mut grants,
                authority,
                grant.clone(),
                Some(identity(&grant)),
                Instant::now()
            ),
            protocol::Observation::Registered
        ));
        let ticket = protocol::ProviderDrainTicket {
            ticket: Incarnation::from_bytes([value; 16]),
            ..ticket(&grant)
        };
        assert_eq!(
            phase(retirement.control(Control::Begin, authority, &mut grants, &sender, ticket)),
            protocol::ProviderDrainObservation::Pending
        );
        first_ticket.get_or_insert(ticket);
    }
    assert_eq!(retirement.drains.len(), protocol::MAX_PROVIDER_DRAINS);
    assert_eq!(retirement.members.len(), protocol::MAX_PROVIDER_DRAINS);
    let ticket = first_ticket.unwrap();
    assert_eq!(
        phase(retirement.control(Control::Begin, authority, &mut grants, &sender, ticket)),
        protocol::ProviderDrainObservation::Pending
    );
    for foreign in [
        protocol::ProviderDrainTicket {
            helper: Incarnation::from_bytes([90; 16]),
            ..ticket
        },
        protocol::ProviderDrainTicket {
            worker: WorkerIdentity {
                fencing_generation: 2,
                ..ticket.worker
            },
            ..ticket
        },
        protocol::ProviderDrainTicket {
            ticket: Incarnation::from_bytes([90; 16]),
            ..ticket
        },
    ] {
        assert_eq!(
            phase(retirement.control(Control::Begin, authority, &mut grants, &sender, foreign)),
            protocol::ProviderDrainObservation::Refused
        );
    }
    assert_eq!(retirement.drains.len(), protocol::MAX_PROVIDER_DRAINS);
    Ok(())
}
