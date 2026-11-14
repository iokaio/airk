// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn default_disabled_lane_never_creates_a_warm_or_source_queue() -> Result<()> {
    let service = airk_kernel::service_broker::new_instance_id()?;
    let shared = Arc::new(Shared {
        queue: Mutex::new(Queue::default()),
        wake: Condvar::new(),
        accounting: Arc::new(accounting::Accounting::default()),
        closing: Arc::new(AtomicBool::new(false)),
        service,
        done: AtomicBool::new(false),
        running: AtomicUsize::new(0),
        changed: tokio::sync::Notify::new(),
    });
    let handle = Handle(shared.clone());
    let installation = Sha256Digest::from_bytes([1; 32]);
    handle.configure(Configuration::disabled(installation, service))?;
    let queue = shared.queue.lock().unwrap();
    assert!(queue.jobs.is_empty());
    assert!(queue.warm.is_none());
    assert!(!queue.configuration.as_ref().unwrap().enabled);
    drop(queue);
    handle.close();
    assert!(
        handle
            .configure(Configuration::disabled(installation, service))
            .is_err()
    );
    Ok(())
}

#[test]
fn optional_model_and_native_barrier_prevent_false_completion() -> Result<()> {
    use airk_core::inference_provider::{CompletionState, ProviderIdentity};
    let service = airk_kernel::service_broker::new_instance_id()?;
    let operation = airk_kernel::service_broker::new_instance_id()?;
    let provider = ProviderIdentity {
        schema: 1,
        service_instance: service,
        provider_generation: operation,
        approved_execution_sha256: Sha256Digest::from_bytes([3; 32]),
    };
    let accounting = Arc::new(accounting::Accounting::default());
    accounting.begin_arbiter();
    let (mut compute, delivery) = accounting.admit(service)?;
    compute.running();
    accounting.arbiter_cached(true);
    accounting.close(operation, vec![])?;
    accounting.mark_engine_barrier();
    assert_eq!(
        accounting.observation(provider, operation)?.state,
        CompletionState::Pending
    );
    drop(delivery);
    assert_eq!(
        accounting
            .observation(provider, operation)?
            .running_outstanding,
        1
    );
    drop(compute);
    accounting.mark_engine_barrier();
    assert_eq!(
        accounting
            .observation(provider, operation)?
            .model_references_outstanding,
        1
    );
    accounting.arbiter_cached(false);
    assert_eq!(
        accounting.observation(provider, operation)?.state,
        CompletionState::Pending
    );
    accounting.mark_arbiter_barrier();
    assert_eq!(
        accounting.observation(provider, operation)?.state,
        CompletionState::Complete
    );
    Ok(())
}

fn shared_fixture() -> Arc<Shared> {
    Arc::new(Shared {
        queue: Mutex::new(Queue::default()),
        wake: Condvar::new(),
        accounting: Arc::new(accounting::Accounting::default()),
        closing: Arc::new(AtomicBool::new(false)),
        service: Incarnation::from_bytes([1; 16]),
        done: AtomicBool::new(false),
        running: AtomicUsize::new(0),
        changed: tokio::sync::Notify::new(),
    })
}
fn request_fixture(
    shared: &Arc<Shared>,
    resources: &airk_kernel::service_resources::Resources,
    charge: u8,
) -> Result<ScoreRequest> {
    use airk_core::{
        arbiter::*, evidence::RevisionEnvelope, identity::RevisionToken, index_catalog::IndexId,
    };
    let reservation = {
        let mut queue = shared.queue.lock().unwrap();
        if let Some(resident) = queue.resident.upgrade() {
            resident
        } else {
            let resident = Arc::new(resources.reserve_optional(
                Incarnation::from_bytes([charge; 16]),
                1,
                Some(8 << 30),
            )?);
            queue.active_profile = Some(Sha256Digest::from_bytes([7; 32]));
            queue.resident = Arc::downgrade(&resident);
            resident
        }
    };
    let admission = resources.reuse_optional(&reservation, Some(8 << 30))?;
    Ok(ScoreRequest {
        worker: shared.service,
        scope: ScopeBinding {
            installation: Sha256Digest::from_bytes([2; 32]),
            catalog: shared.service,
            index_id: IndexId::from_identity(Incarnation::from_bytes([3; 16])),
            authorization: Sha256Digest::from_bytes([4; 32]),
            configuration_revision: 1,
            publication: RevisionEnvelope::new(
                "fixture".into(),
                RevisionToken {
                    incarnation: Incarnation::from_bytes([5; 16]),
                    generation: 1,
                    revision: Sha256Digest::from_bytes([6; 32]),
                },
            ),
        },
        task: Task::ContextNextRead,
        deadline: Instant::now() + Duration::from_secs(1),
        authority: Arc::new(|_| true),
        state: "source evidence".into(),
        question: "which next read?".into(),
        candidates: vec![Candidate {
            id: "c1".into(),
            text: "source".into(),
        }],
        resident: ResidentPermit {
            profile: Sha256Digest::from_bytes([7; 32]),
            reservation,
        },
        admission,
        max_input_tokens: 2048,
    })
}
#[test]
fn per_index_capacity_includes_the_actual_running_request() -> Result<()> {
    let shared = shared_fixture();
    let handle = Handle(shared.clone());
    let resources = airk_kernel::service_resources::Resources::new(
        airk_core::service_resources::ResourceConfiguration {
            memory_bytes: Some(8 << 30),
            ..Default::default()
        },
        8,
        Some(16 << 30),
    )?;
    let first = request_fixture(&shared, &resources, 10)?;
    let mut configuration = Configuration::disabled(first.scope.installation, shared.service);
    configuration.enabled = true;
    configuration.selected_profile = Some(Sha256Digest::from_bytes([7; 32]));
    configuration.indexes.push(airk_core::arbiter::Enrollment {
        index_id: first.scope.index_id,
        tasks: std::collections::BTreeSet::from([Task::ContextNextRead]),
    });
    handle.configure(configuration)?;
    shared.queue.lock().unwrap().running_index = Some(first.scope.index_id);
    let _accepted = handle.score(first)?;
    assert!(
        handle
            .score(request_fixture(&shared, &resources, 11)?)
            .is_err(),
        "one running plus one queued exhausts the original two-request per-index limit"
    );
    assert_eq!(handle.observation()?.queued, 1);
    Ok(())
}
#[test]
fn actual_lane_exit_closes_admission_and_discards_queued_deliveries() -> Result<()> {
    let shared = shared_fixture();
    let handle = Handle(shared.clone());
    shared.accounting.begin_arbiter();
    shared.accounting.arbiter_cached(true);
    shared.queue.lock().unwrap().active_profile = Some(Sha256Digest::from_bytes([9; 32]));
    shared.running.store(1, Ordering::Release);
    drop(LaneCompleted(shared.clone()));
    assert!(shared.done.load(Ordering::Acquire));
    assert!(shared.queue.lock().unwrap().closed);
    assert_eq!(handle.observation()?.active_profile, None);
    assert_eq!(handle.observation()?.running, 0);
    assert!(
        handle
            .configure(Configuration::disabled(
                Sha256Digest::from_bytes([2; 32]),
                shared.service
            ))
            .is_err()
    );
    Ok(())
}

#[test]
fn authority_crossing_original_deadline_or_cancellation_never_dispatches_and_keeps_compiled_receipt()
-> Result<()> {
    use std::cell::Cell;
    let start = Instant::now();
    let deadline = start + Duration::from_secs(1);
    let clock = Cell::new(start);
    let calls = Cell::new(0);
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    // An observed authority check crosses the existing deadline; the compiled
    // receipt has already been delivered and must survive dispatch refusal.
    clock.set(deadline);
    tx.send(23usize).unwrap();
    assert!(
        dispatch_before_original_deadline(
            deadline,
            || clock.get(),
            || false,
            || {
                calls.set(calls.get() + 1);
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(rx.try_recv()?, 23);
    assert_eq!(calls.get(), 0);
    clock.set(start);
    assert!(
        dispatch_before_original_deadline(
            deadline,
            || clock.get(),
            || true,
            || {
                calls.set(calls.get() + 1);
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(calls.get(), 0);
    dispatch_before_original_deadline(
        deadline,
        || clock.get(),
        || false,
        || {
            calls.set(calls.get() + 1);
            Ok(())
        },
    )?;
    assert_eq!(calls.get(), 1);
    Ok(())
}

#[test]
fn observed_resource_pressure_after_admission_never_reaches_native_score_counter() -> Result<()> {
    use std::cell::Cell;
    let mut resources = airk_kernel::service_resources::Resources::new(
        airk_core::service_resources::ResourceConfiguration::default(),
        32,
        Some(128 << 30),
    )?;
    resources.observe(Some(2 << 30), Some(80 << 30), true);
    let resident =
        resources.reserve_optional(Incarnation::from_bytes([95; 16]), 27 << 30, Some(80 << 30))?;
    let admission = resources.reuse_optional(&resident, Some(80 << 30))?;
    resources.observe(Some(33 << 30), Some(80 << 30), true);
    let scores = Cell::new(0);
    let now = Instant::now();
    assert!(
        dispatch_before_original_deadline(
            now + Duration::from_secs(1),
            || now,
            || false,
            || {
                admission.validate(&resident)?;
                scores.set(scores.get() + 1);
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(scores.get(), 0);
    Ok(())
}

#[test]
fn resident_session_retains_exact_asset_pins_until_actual_scorer_teardown() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Native(std::sync::Arc<AtomicBool>, std::sync::Arc<AtomicBool>);
    impl Drop for Native {
        fn drop(&mut self) {
            assert!(
                self.0.load(Ordering::Acquire),
                "asset pins released before native teardown"
            );
            self.1.store(true, Ordering::Release);
        }
    }
    struct Source(std::sync::Arc<AtomicBool>, std::sync::Arc<AtomicBool>);
    impl AssetSource for Source {
        fn read_asset(&self, _: &airk_core::arbiter::Asset) -> Result<Vec<u8>> {
            anyhow::bail!("drop-order probe does not load a model")
        }
    }
    impl Drop for Source {
        fn drop(&mut self) {
            assert!(
                self.1.load(Ordering::Acquire),
                "source released before native teardown"
            );
            self.0.store(false, Ordering::Release);
        }
    }
    let pins = Arc::new(AtomicBool::new(true));
    let native_done = Arc::new(AtomicBool::new(false));
    let resident = ResidentSession {
        scorer: Native(pins.clone(), native_done.clone()),
        _source: Box::new(Source(pins.clone(), native_done.clone())),
    };
    assert!(pins.load(Ordering::Acquire));
    assert!(!native_done.load(Ordering::Acquire));
    drop(resident);
    assert!(native_done.load(Ordering::Acquire));
    assert!(!pins.load(Ordering::Acquire));
}

fn observed_completion_fixture() -> CompiledReceipt {
    CompiledReceipt {
        input_digest: Sha256Digest::from_bytes([0x21; 32]),
        input_tokens: 137,
        queue_us: 5,
        compile_us: 9,
    }
}
fn expected_observed_completion() -> ScoreReceipt {
    ScoreReceipt {
        input_digest: Sha256Digest::from_bytes([0x21; 32]),
        input_tokens: 137,
        queue_us: 5,
        compile_us: 9,
        inference_us: 41,
    }
}

#[test]
fn actual_score_call_error_keeps_observed_work_and_original_cause() -> Result<()> {
    let started = Instant::now();
    let calls = std::cell::Cell::new(0);
    let result = observed_score_call(
        &observed_completion_fixture(),
        started,
        || started + Duration::from_micros(41),
        || {
            calls.set(calls.get() + 1);
            Err(anyhow::anyhow!("synthetic native score failure"))
        },
    );
    let Err(error) = result else {
        anyhow::bail!("native score error was lost");
    };
    let failure = error
        .downcast_ref::<KnownScoreFailure>()
        .context("completed failure lost its private receipt")?;
    assert_eq!(calls.get(), 1);
    assert_eq!(failure.receipt, expected_observed_completion());
    assert!(!failure.source_delivery_refused);
    assert_eq!(
        std::error::Error::source(failure).unwrap().to_string(),
        "synthetic native score failure"
    );
    Ok(())
}

#[test]
fn post_score_delivery_refusal_retains_known_success_or_failure_work() -> Result<()> {
    let started = Instant::now();
    for native_failed in [false, true] {
        let result = observed_score_call(
            &observed_completion_fixture(),
            started,
            || started + Duration::from_micros(41),
            || {
                if native_failed {
                    Err(anyhow::anyhow!("synthetic native score failure"))
                } else {
                    Ok(vec![0.25, 0.5])
                }
            },
        );
        let Err(error) = finish_score_delivery(
            result,
            Err(anyhow::anyhow!("synthetic source delivery revoked")),
        ) else {
            anyhow::bail!("revoked delivery returned scores");
        };
        let failure = error
            .downcast_ref::<KnownScoreFailure>()
            .context("revoked completed score lost known work")?;
        assert_eq!(failure.receipt, expected_observed_completion());
        assert!(failure.source_delivery_refused);
        assert!(
            error
                .chain()
                .any(|cause| cause.to_string() == "synthetic source delivery revoked")
        );
        if native_failed {
            assert!(
                error
                    .chain()
                    .any(|cause| cause.to_string() == "synthetic native score failure")
            );
        }
    }
    let scored = finish_score_delivery(
        observed_score_call(
            &observed_completion_fixture(),
            started,
            || started + Duration::from_micros(41),
            || Ok(vec![0.25, 0.5]),
        ),
        Ok(()),
    )?;
    assert_eq!(scored.scores, vec![0.25, 0.5]);
    assert_eq!(scored.input_tokens, 137);
    assert_eq!(scored.queue_us, 5);
    assert_eq!(scored.compile_us, 9);
    assert_eq!(scored.inference_us, 41);
    Ok(())
}

#[test]
fn canceled_or_expired_before_actual_score_cannot_mint_completion_receipt() -> Result<()> {
    let started = Instant::now();
    let deadline = started + Duration::from_secs(1);
    let calls = std::cell::Cell::new(0);
    for (now, canceled) in [(deadline, false), (started, true)] {
        let result = dispatch_before_original_deadline(
            deadline,
            || now,
            || canceled,
            || {
                observed_score_call(
                    &observed_completion_fixture(),
                    started,
                    || started + Duration::from_micros(41),
                    || {
                        calls.set(calls.get() + 1);
                        Ok(vec![0.25, 0.5])
                    },
                )
            },
        );
        let Err(error) = finish_score_delivery(
            result,
            Err(anyhow::anyhow!(
                "synthetic delivery revoked before dispatch"
            )),
        ) else {
            anyhow::bail!("pre-dispatch refusal returned scores");
        };
        assert!(error.downcast_ref::<KnownScoreFailure>().is_none());
    }
    assert_eq!(calls.get(), 0);
    Ok(())
}
