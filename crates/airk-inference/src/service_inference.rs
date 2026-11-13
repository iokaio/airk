// SPDX-License-Identifier: Apache-2.0
//! Service inference ownership, independent of predecessor process recovery.
// Every remaining test here needs embedded inference (split step 2b moved the
// two that drive the catalog Gate or the supervisor to those modules).
#[cfg(all(test, feature = "local-inference"))]
mod tests;
/// Native lease exclusion for an earlier in-process provider. This is never a
/// process-reap receipt and is valid only for a trusted InProcess obligation.
#[derive(Clone)]
pub struct PredecessorExclusionWitness {
    current: airk_core::identity::Incarnation,
    prior: airk_core::identity::Incarnation,
    generation: airk_core::identity::Incarnation,
    namespace: airk_core::identity::Incarnation,
    _ownership: std::sync::Arc<airk_store::service::ServiceLease>,
}
impl PredecessorExclusionWitness {
    pub fn acquire(
        ownership: std::sync::Arc<airk_store::service::ServiceLease>,
        current: airk_core::identity::Incarnation,
        prior: airk_core::identity::Incarnation,
        generation: airk_core::identity::Incarnation,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(current != prior, "current inference owner is not excluded");
        let namespace = airk_store::service::registry::retained_namespace(ownership.root())?;
        Ok(Self {
            current,
            prior,
            generation,
            namespace,
            _ownership: ownership,
        })
    }
    pub fn matches(
        &self,
        current: airk_core::identity::Incarnation,
        namespace: airk_core::identity::Incarnation,
        prior: airk_core::identity::Incarnation,
        generation: airk_core::identity::Incarnation,
    ) -> bool {
        self.current == current
            && self.namespace == namespace
            && self.prior == prior
            && self.generation == generation
    }
    pub fn key(
        &self,
    ) -> (
        airk_core::identity::Incarnation,
        airk_core::identity::Incarnation,
    ) {
        (self.prior, self.generation)
    }
}
#[cfg(not(feature = "local-inference"))]
pub use crate::embedding_pool_process::Pool;

#[cfg(feature = "local-inference")]
mod native {
    use crate::embedding_pool_process::{
        PoolHealth, ProviderDrainState, ProviderReapWitness, Renewal,
    };
    use crate::{
        embedding_pool_protocol as protocol,
        inference_engine::{InferenceOwner, NativeCompletionWitness},
    };
    use airk_core::{
        identity::{Incarnation, framed_digest},
        inference_provider::{
            CompletionBasis, CompletionObservation, CompletionState, ProviderIdentity,
        },
        service::{ProjectRegistration, WorkerIdentity},
    };
    use airk_kernel::service_broker::new_instance_id;
    use anyhow::{Context, Result, ensure};
    use std::{
        collections::{BTreeMap, BTreeSet},
        path::{Path, PathBuf},
        sync::{Arc, Mutex, Weak},
        time::Instant,
    };

    const MAX_ACTIVE_COMPLETIONS: usize = 128;
    struct FactoryOwners {
        ownership: Arc<airk_store::service::ServiceLease>,
        provider: ProviderIdentity,
        unresolved: BTreeSet<u64>,
        next_slot: u64,
        outstanding_deliveries: usize,
        events: Vec<CompletedProvider>,
        ledger: Option<crate::inference_completion_ledger::NativeLedger>,
    }
    pub struct CompletedProvider {
        pub completion: NativeCompletionWitness,
        pub ledger: crate::inference_completion_ledger::NativeLedgerWitness,
        owners: Weak<Mutex<FactoryOwners>>,
    }
    impl CompletedProvider {
        pub fn acknowledge(self) -> Result<()> {
            let owners = self
                .owners
                .upgrade()
                .context("native completion delivery owner missing")?;
            let mut owners = owners
                .lock()
                .map_err(|_| anyhow::anyhow!("native factory history unavailable"))?;
            owners.outstanding_deliveries = owners
                .outstanding_deliveries
                .checked_sub(1)
                .context("native completion delivery underflow")?;
            Ok(())
        }
    }
    struct FactorySlot {
        owners: Arc<Mutex<FactoryOwners>>,
        index: u64,
        published: bool,
    }
    impl FactorySlot {
        fn publish(&mut self, witness: NativeCompletionWitness) -> Result<()> {
            if self.published {
                return Ok(());
            }
            let mut owners = self
                .owners
                .lock()
                .map_err(|_| anyhow::anyhow!("native factory history unavailable"))?;
            ensure!(
                owners.unresolved.contains(&self.index),
                "native factory owner slot missing"
            );
            if owners.ledger.is_none() {
                owners.ledger = Some(crate::inference_completion_ledger::NativeLedger::new(
                    owners.ownership.clone(),
                    owners.provider,
                )?);
            }
            let ledger = owners
                .ledger
                .as_mut()
                .context("native completion ledger missing")?
                .append(&witness)?;
            owners.events.push(CompletedProvider {
                completion: witness,
                ledger,
                owners: Arc::downgrade(&self.owners),
            });
            owners.outstanding_deliveries += 1;
            owners.unresolved.remove(&self.index);
            self.published = true;
            Ok(())
        }
    }
    pub struct Pool {
        owner: Option<InferenceOwner>,
        #[cfg(any(test, feature = "test-support"))]
        legacy: Option<crate::embedding_pool_process::Pool>,
        root: PathBuf,
        service: Incarnation,
        generation: Incarnation,
        health: PoolHealth,
        workers: BTreeMap<Incarnation, WorkerIdentity>,
        tickets: BTreeMap<Incarnation, protocol::ProviderDrainTicket>,
        stop_operation: Option<Incarnation>,
        factory_slot: Option<FactorySlot>,
    }
    /// Sole production factory for this service incarnation. A zero-provider
    /// proof is possible only before this actual factory ever creates an owner.
    pub struct InferenceFactory {
        ownership: Arc<airk_store::service::ServiceLease>,
        provider: ProviderIdentity,
        allocated: bool,
        closed: bool,
        empty_witness: Option<CompletionObservation>,
        owners: Arc<Mutex<FactoryOwners>>,
    }
    impl InferenceFactory {
        pub fn new(
            ownership: Arc<airk_store::service::ServiceLease>,
            service: Incarnation,
        ) -> Result<Self> {
            ownership.claim_inference_factory()?;
            let provider = ProviderIdentity {
                schema: 1,
                service_instance: service,
                approved_execution_sha256: airk_host::runtime_pair::current_executable_sha256()?
                    .parse()?,
                provider_generation: new_instance_id()?,
            };
            let owners = FactoryOwners {
                ownership: ownership.clone(),
                provider,
                unresolved: BTreeSet::new(),
                next_slot: 0,
                outstanding_deliveries: 0,
                events: Vec::new(),
                ledger: None,
            };
            Ok(Self {
                ownership,
                provider,
                allocated: false,
                closed: false,
                empty_witness: None,
                owners: Arc::new(Mutex::new(owners)),
            })
        }
        pub fn take_completed(&mut self) -> Result<Vec<CompletedProvider>> {
            let mut owners = self
                .owners
                .lock()
                .map_err(|_| anyhow::anyhow!("native factory history unavailable"))?;
            Ok(std::mem::take(&mut owners.events))
        }
        pub fn start(&mut self, threads: u16) -> Result<Pool> {
            ensure!(!self.closed, "inference factory admission closed");
            let index = {
                let mut owners = self
                    .owners
                    .lock()
                    .map_err(|_| anyhow::anyhow!("native factory history unavailable"))?;
                ensure!(
                    owners.unresolved.is_empty(),
                    "native previous owner completion remains unresolved"
                );
                ensure!(
                    owners.unresolved.len() + owners.outstanding_deliveries
                        < MAX_ACTIVE_COMPLETIONS,
                    "native factory concurrent completion capacity exhausted"
                );
                owners.next_slot = owners
                    .next_slot
                    .checked_add(1)
                    .context("native factory owner sequence overflow")?;
                let index = owners.next_slot;
                owners.unresolved.insert(index);
                index
            };
            self.allocated = true;
            let mut pool = Pool::start(
                self.ownership.root(),
                self.provider.service_instance,
                threads,
                self.ownership.clone(),
            )?;
            pool.factory_slot = Some(FactorySlot {
                owners: self.owners.clone(),
                index,
                published: false,
            });
            Ok(pool)
        }

        pub fn empty_checkpoint(
            &mut self,
            operation: Incarnation,
            workers: Vec<WorkerIdentity>,
        ) -> Result<Option<CompletionObservation>> {
            let mut owners = self
                .owners
                .lock()
                .map_err(|_| anyhow::anyhow!("native factory history unavailable"))?;
            ensure!(
                owners.unresolved.is_empty(),
                "native factory owner completion remains unresolved"
            );
            if owners.outstanding_deliveries != 0 {
                return Ok(None);
            }
            let completion_ledger = if self.allocated {
                Some(
                    owners
                        .ledger
                        .as_ref()
                        .and_then(|ledger| ledger.observation())
                        .context("native completion ledger missing")?
                        .clone(),
                )
            } else {
                None
            };
            let observation = CompletionObservation {
                schema: 1,
                completion_basis: if self.allocated {
                    CompletionBasis::NativeFactoryCompletionLedger
                } else {
                    CompletionBasis::NativeFactoryNeverAllocated
                },
                provider: self.provider,
                operation_id: operation,
                captured_workers: workers,
                admitted_high_water: completion_ledger
                    .as_ref()
                    .map_or(0, |ledger| ledger.admitted_high_water),
                admission_closed: true,
                queued_outstanding: 0,
                running_outstanding: 0,
                callback_outstanding: 0,
                delivery_outstanding: 0,
                model_references_outstanding: 0,
                state: CompletionState::Complete,
                completion_ledger,
            };
            observation.validate()?;
            if let Some(prior) = &self.empty_witness {
                ensure!(
                    prior == &observation,
                    "inference factory checkpoint changed"
                );
            }
            if self.allocated {
                let admitted = self.ownership.seal_inference_admission()?;
                self.closed = true;
                ensure!(
                    admitted
                        == observation
                            .completion_ledger
                            .as_ref()
                            .unwrap()
                            .completed_owners,
                    "native factory does not cover every admitted compute owner"
                );
            } else {
                self.ownership.seal_unused_inference()?;
            }
            self.closed = true;
            self.empty_witness = Some(observation.clone());
            if self.allocated
                && !owners
                    .ledger
                    .as_mut()
                    .context("native completion ledger missing")?
                    .verify()?
            {
                return Ok(None);
            }
            Ok(Some(observation))
        }
    }

    impl Pool {
        fn start(
            root: &Path,
            service: Incarnation,
            threads: u16,
            ownership: Arc<airk_store::service::ServiceLease>,
        ) -> Result<Self> {
            ensure!(root == ownership.root(), "foreign inference service owner");
            let owner = InferenceOwner::start(ownership, service, threads)?;
            let generation = owner.generation();
            let now = Instant::now();
            Ok(Self {
                owner: Some(owner),
                #[cfg(any(test, feature = "test-support"))]
                legacy: None,
                root: root.to_path_buf(),
                service,
                generation,
                health: PoolHealth {
                    instance: generation,
                    ready_at: now,
                    last_ack_at: now,
                },
                workers: BTreeMap::new(),
                tickets: BTreeMap::new(),
                stop_operation: None,
                factory_slot: None,
            })
        }
        #[cfg(any(test, feature = "test-support"))]
        pub fn start_fixture(root: &Path, service: Incarnation, main: &Path) -> Result<Self> {
            let legacy = crate::embedding_pool_process::Pool::start_fixture(root, service, main)?;
            let generation = legacy.identity();
            let health = legacy.health();
            Ok(Self {
                owner: None,
                legacy: Some(legacy),
                root: root.to_path_buf(),
                service,
                generation,
                health,
                workers: BTreeMap::new(),
                tickets: BTreeMap::new(),
                stop_operation: None,
                factory_slot: None,
            })
        }
        pub fn identity(&self) -> Incarnation {
            self.generation
        }
        #[cfg(any(test, feature = "test-support"))]
        pub fn set_compute_hook(&self, hook: crate::inference_engine::ComputeHook) -> Result<()> {
            self.owner
                .as_ref()
                .context("native inference owner unavailable")?
                .set_compute_hook(hook)
        }
        pub fn provider_status(
            &self,
        ) -> anyhow::Result<Option<airk_core::inference_provider::ProviderStatus>> {
            self.owner
                .as_ref()
                .map(InferenceOwner::provider_status)
                .transpose()
        }
        pub(crate) fn in_process(&self) -> bool {
            #[cfg(any(test, feature = "test-support"))]
            if self.legacy.is_some() {
                return false;
            }
            true
        }
        pub fn process_id(&self) -> Option<u32> {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(legacy) = &self.legacy {
                return Some(legacy.process_id());
            }
            None
        }
        #[cfg(windows)]
        pub fn descendant_rss(&self) -> Result<Vec<Option<u64>>> {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(legacy) = &self.legacy {
                return legacy.descendant_rss();
            }
            // Native compute and model pages are already in the service RSS.
            Ok(Vec::new())
        }
        pub fn health(&self) -> PoolHealth {
            self.health
        }
        pub fn failure(&mut self) -> Result<Option<String>> {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(legacy) = self.legacy.as_mut() {
                return Ok(legacy
                    .exit_status()?
                    .map(crate::embedding_pool_process::exit_code));
            }
            Ok(self
                .owner
                .as_ref()
                .filter(|owner| owner.failed())
                .map(|_| "inference_owner_failed".into()))
        }
        pub fn retiring(&self) -> bool {
            self.stop_operation.is_some()
        }
        pub fn terminate(&mut self) -> Result<()> {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(legacy) = self.legacy.as_mut() {
                return legacy.terminate();
            }
            let owner = self.owner.as_mut().context("inference owner unavailable")?;
            let (operation, workers) = match owner.checkpoint_context()? {
                Some(capture) => (capture.operation_id, capture.captured_workers),
                None => (
                    match self.stop_operation {
                        Some(operation) => operation,
                        None => new_instance_id()?,
                    },
                    self.workers.values().copied().collect(),
                ),
            };
            self.stop_operation = Some(operation);
            let _ = owner.checkpoint_drain(operation, workers.clone())?;
            ensure!(owner.stop()?, "inference native execution still draining");
            ensure!(
                owner.checkpoint_drain(operation, workers)?.is_some(),
                "inference stopped without native completion witness"
            );
            if let Some(slot) = &mut self.factory_slot {
                slot.publish(
                    owner
                        .completion()
                        .context("joined native owner completion missing")?
                        .clone(),
                )?;
            }
            Ok(())
        }
        #[cfg(any(test, feature = "test-support"))]
        pub fn completion_witness(
            &self,
        ) -> Option<crate::inference_engine::NativeCompletionWitness> {
            self.owner
                .as_ref()
                .and_then(InferenceOwner::completion)
                .cloned()
        }
        pub fn reap_witness(&self) -> Option<ProviderReapWitness> {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(legacy) = &self.legacy {
                return legacy.reap_witness();
            }
            // An in-process barrier can never manufacture a process-reap fact.
            None
        }
        #[cfg(any(test, feature = "test-support"))]
        pub fn access(
            &mut self,
            worker: Incarnation,
            project: ProjectRegistration,
        ) -> Result<protocol::Access> {
            if let Some(legacy) = self.legacy.as_mut() {
                return legacy.access(worker, project);
            }
            self.access_worker(
                WorkerIdentity {
                    service: self.service,
                    worker,
                    project: project.binding,
                    fencing_generation: 1,
                },
                project,
            )
        }
        #[cfg(any(test, feature = "test-support"))]
        pub fn revoke(&mut self, worker: Incarnation) -> Result<()> {
            if let Some(legacy) = self.legacy.as_mut() {
                return legacy.revoke(worker);
            }
            self.owner
                .as_ref()
                .context("inference owner unavailable")?
                .revoke(worker)
        }
        pub fn access_worker(
            &mut self,
            identity: WorkerIdentity,
            project: ProjectRegistration,
        ) -> Result<protocol::Access> {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(legacy) = self.legacy.as_mut() {
                return legacy.access_worker(identity, project);
            }
            ensure!(
                identity.service == self.service
                    && identity.project == project.binding
                    && identity.fencing_generation > 0,
                "foreign inference worker"
            );
            ensure!(
                !self.tickets.contains_key(&identity.worker),
                "inference worker retirement pending"
            );
            ensure!(
                self.workers
                    .get(&identity.worker)
                    .is_none_or(|old| *old == identity)
                    && (self.workers.contains_key(&identity.worker)
                        || self.workers.len() < protocol::MAX_PROVIDER_DRAINS),
                "inference membership changed or full"
            );
            let first = new_instance_id()?;
            let second = new_instance_id()?;
            let grant = protocol::Grant {
                service: self.service,
                pool: self.generation,
                worker: identity.worker,
                project,
                token: framed_digest(
                    b"airk-pool-capability/1",
                    &[first.as_bytes(), second.as_bytes()],
                ),
            };
            self.owner
                .as_ref()
                .context("inference owner unavailable")?
                .register(grant.clone(), identity)?;
            self.workers.insert(identity.worker, identity);
            self.health.last_ack_at = Instant::now();
            Ok(protocol::Access {
                runtime: self.root.clone(),
                role: protocol::role(self.generation),
                grant,
                in_process: true,
            })
        }
        pub fn renew(&mut self, worker: Incarnation) -> Result<Renewal> {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(legacy) = self.legacy.as_mut() {
                return legacy.renew(worker);
            }
            let renewed = self
                .owner
                .as_ref()
                .context("inference owner unavailable")?
                .renew(worker)?;
            self.health.last_ack_at = Instant::now();
            Ok(if renewed {
                Renewal::Renewed
            } else {
                Renewal::Missing
            })
        }
        pub fn retirement_ticket(
            &self,
            identity: WorkerIdentity,
        ) -> Result<Option<protocol::ProviderDrainTicket>> {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(legacy) = self.legacy.as_ref() {
                return legacy.retirement_ticket(identity);
            }
            if let Some(ticket) = self.tickets.get(&identity.worker) {
                ensure!(
                    ticket.worker == identity,
                    "inference retirement identity changed"
                );
                return Ok(Some(*ticket));
            }
            Ok(None)
        }
        pub fn begin_retirement(
            &mut self,
            identity: WorkerIdentity,
        ) -> Result<protocol::ProviderDrainTicket> {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(legacy) = self.legacy.as_mut() {
                return legacy.begin_retirement(identity);
            }
            ensure!(
                self.workers.get(&identity.worker) == Some(&identity),
                "inference retirement requires exact native member"
            );
            if let Some(ticket) = self.retirement_ticket(identity)? {
                return Ok(ticket);
            }
            let ticket = protocol::ProviderDrainTicket {
                helper: self.generation,
                worker: identity,
                ticket: new_instance_id()?,
            };
            self.owner
                .as_ref()
                .context("inference owner unavailable")?
                .begin_retirement(ticket)?;
            self.tickets.insert(identity.worker, ticket);
            Ok(ticket)
        }
        pub fn poll_retirement(
            &mut self,
            ticket: &protocol::ProviderDrainTicket,
        ) -> Result<ProviderDrainState> {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(legacy) = self.legacy.as_mut() {
                return legacy.poll_retirement(ticket);
            }
            ensure!(
                self.retirement_ticket(ticket.worker)? == Some(*ticket),
                "foreign inference retirement ticket"
            );
            let owner = self.owner.as_ref().context("inference owner unavailable")?;
            match owner.poll_retirement(*ticket)? {
                protocol::Observation::Drain {
                    ticket: observed,
                    observation: protocol::ProviderDrainObservation::Drained,
                } if observed == *ticket && owner.worker_settled(ticket.worker.worker)? => {
                    Ok(ProviderDrainState::Drained)
                }
                protocol::Observation::Drain {
                    ticket: observed,
                    observation:
                        protocol::ProviderDrainObservation::Pending
                        | protocol::ProviderDrainObservation::Drained,
                } if observed == *ticket => Ok(ProviderDrainState::Pending),
                _ => Ok(ProviderDrainState::Unknown),
            }
        }
        pub fn finish_retirement(&mut self, ticket: &protocol::ProviderDrainTicket) -> Result<()> {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(legacy) = self.legacy.as_mut() {
                return legacy.finish_retirement(ticket);
            }
            ensure!(
                self.poll_retirement(ticket)? == ProviderDrainState::Drained,
                "inference worker still charged"
            );
            let owner = self.owner.as_ref().context("inference owner unavailable")?;
            ensure!(
                matches!(owner.finish_retirement(*ticket)?,protocol::Observation::Drain{ticket:observed,observation:protocol::ProviderDrainObservation::Forgotten} if observed==*ticket),
                "inference retirement forget refused"
            );
            owner.forget_worker(ticket.worker.worker)?;
            self.workers.remove(&ticket.worker.worker);
            self.tickets.remove(&ticket.worker.worker);
            Ok(())
        }
        pub fn status(&mut self) -> Result<protocol::Observation> {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(legacy) = self.legacy.as_mut() {
                return legacy.status();
            }
            let status = self
                .owner
                .as_ref()
                .context("inference owner unavailable")?
                .status()?;
            self.health.last_ack_at = Instant::now();
            Ok(status)
        }
        pub fn configure_threads(&mut self, threads: u16) -> Result<()> {
            #[cfg(any(test, feature = "test-support"))]
            if let Some(legacy) = self.legacy.as_mut() {
                return legacy.configure_threads(threads);
            }
            self.owner
                .as_ref()
                .context("inference owner unavailable")?
                .configure_threads(threads)
        }
        pub fn checkpoint_drain(
            &mut self,
            operation: Incarnation,
            workers: Vec<WorkerIdentity>,
        ) -> Result<Option<CompletionObservation>> {
            #[cfg(any(test, feature = "test-support"))]
            ensure!(
                self.legacy.is_none(),
                "legacy helper requires verified process-exit checkpoint"
            );
            ensure!(
                self.workers.values().all(|worker| workers.contains(worker)),
                "checkpoint omits admitted inference worker"
            );
            Ok(self
                .owner
                .as_mut()
                .context("inference owner unavailable")?
                .checkpoint_drain(operation, workers)?
                .map(|witness| witness.observation().clone()))
        }
    }
}
#[cfg(feature = "local-inference")]
pub use native::{CompletedProvider, InferenceFactory, Pool};

pub fn process_id(pool: &Pool) -> Option<u32> {
    #[cfg(feature = "local-inference")]
    {
        pool.process_id()
    }
    #[cfg(not(feature = "local-inference"))]
    {
        Some(pool.process_id())
    }
}
pub fn in_process(pool: &Pool) -> bool {
    #[cfg(feature = "local-inference")]
    {
        pool.in_process()
    }
    #[cfg(not(feature = "local-inference"))]
    {
        let _ = pool;
        false
    }
}
pub fn provider_status(
    pool: &Pool,
) -> anyhow::Result<Option<airk_core::inference_provider::ProviderStatus>> {
    #[cfg(feature = "local-inference")]
    {
        pool.provider_status()
    }
    #[cfg(not(feature = "local-inference"))]
    {
        let _ = pool;
        Ok(None)
    }
}
