// SPDX-License-Identifier: Apache-2.0
//! Native resource ownership after a child exits; never an authorization ledger.
use super::{ChildWorker, Incarnation, Pool, WorkerIdentity};
use crate::embedding_pool_process::ProviderDrainState;
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

const MAX_DRAINING_WORKERS: usize = 128;

#[cfg(test)]
#[path = "worker_resource_drains_tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DrainProgress {
    ChildPending,
    ProviderPending,
    Ready,
}

struct ResourceOwner {
    identity: WorkerIdentity,
    helpers: Vec<Incarnation>,
    in_process: bool,
    completed: Option<airk_core::inference_provider::CompletionLedgerObservation>,
    child_reaped: bool,
    providers_done: bool,
    released: bool,
}

#[derive(Default)]
pub(super) struct WorkerResourceDrains {
    owners: BTreeMap<Incarnation, ResourceOwner>,
}

impl WorkerResourceDrains {
    /// Reserve before losing the original ChildWorker. The caller then marks
    /// that child as exiting, preventing new jobs/grants/provider deliveries.
    /// Capacity errors leave the original owning child and charges intact.
    pub fn reserve(&mut self, worker: &ChildWorker) -> Result<WorkerIdentity> {
        let identity = worker
            .identity
            .context("native exit identity unavailable")?;
        self.reserve_history_completed(
            identity,
            worker.provider_helpers.clone(),
            worker.provider_in_process,
            worker.completed_authorizations(),
        )?;
        Ok(identity)
    }

    /// Only an existing native ChildWorker history or the exact validated
    /// durable retirement obligation may supply this history. Missing history
    /// must never be represented by an empty list.
    #[cfg(test)]
    pub fn reserve_history(
        &mut self,
        identity: WorkerIdentity,
        helpers: Vec<Incarnation>,
    ) -> Result<()> {
        self.reserve_history_kind(identity, helpers, false)
    }
    #[cfg(test)]
    pub fn reserve_history_kind(
        &mut self,
        identity: WorkerIdentity,
        helpers: Vec<Incarnation>,
        in_process: bool,
    ) -> Result<()> {
        self.reserve_history_completed(identity, helpers, in_process, None)
    }
    pub fn reserve_history_completed(
        &mut self,
        identity: WorkerIdentity,
        mut helpers: Vec<Incarnation>,
        in_process: bool,
        completed: Option<airk_core::inference_provider::CompletionLedgerObservation>,
    ) -> Result<()> {
        if let Some(anchor) = &completed {
            anchor.validate()?;
            ensure!(
                in_process && anchor.service_instance == identity.service,
                "foreign native exit completion ledger"
            );
        }
        helpers.sort();
        helpers.dedup();
        ensure!(
            helpers.len() <= 32,
            "native exit helper history exceeds bound"
        );
        if let Some(old) = self.owners.get(&identity.worker) {
            ensure!(
                old.identity == identity
                    && old.helpers == helpers
                    && old.in_process == in_process
                    && old.completed == completed,
                "native exit owner changed"
            );
            return Ok(());
        }
        ensure!(
            self.owners.len() < MAX_DRAINING_WORKERS,
            "native exit owner capacity exceeded"
        );
        self.owners.insert(
            identity.worker,
            ResourceOwner {
                identity,
                helpers,
                in_process,
                completed,
                child_reaped: false,
                providers_done: false,
                released: false,
            },
        );
        Ok(())
    }

    /// Start the exact shared-engine barrier BEFORE fast revoke or child
    /// removal. An absent/replaced helper is unresolved here, not a proof.
    pub fn begin(&self, identity: WorkerIdentity, pool: Option<&mut Pool>) -> Result<()> {
        let owner = self.owner(identity)?;
        if let Some(pool) = pool
            && owner.helpers.contains(&pool.identity())
            && pool.reap_witness().is_none()
            && pool.retirement_ticket(identity)?.is_none()
        {
            let _ = pool.begin_retirement(identity)?;
        }
        Ok(())
    }

    /// Only the existing native child owner calls this after Drop completed its
    /// actual process/descendant/reader drain. EOF and deadlines never call it.
    pub fn child_reaped(&mut self, identity: WorkerIdentity) -> Result<()> {
        self.owner_mut(identity)?.child_reaped = true;
        Ok(())
    }

    /// One bounded polling turn, sharing the existing helper reply lane. The
    /// proof callback accepts only opaque native exact-generation reap facts.
    /// Never-authorized is known solely from the monotonic captured empty list.
    #[cfg(test)]
    pub fn poll(
        &mut self,
        identity: WorkerIdentity,
        pool: Option<&mut Pool>,
        mut actually_reaped: impl FnMut(Incarnation, Incarnation, bool) -> Result<bool>,
    ) -> Result<DrainProgress> {
        self.poll_completed(identity, pool, &mut actually_reaped, |_| Ok(false))
    }
    pub fn poll_completed(
        &mut self,
        identity: WorkerIdentity,
        pool: Option<&mut Pool>,
        mut actually_reaped: impl FnMut(Incarnation, Incarnation, bool) -> Result<bool>,
        mut ledger_completed: impl FnMut(
            &airk_core::inference_provider::CompletionLedgerObservation,
        ) -> Result<bool>,
    ) -> Result<DrainProgress> {
        let owner = self.owner_mut(identity)?;
        if !owner.child_reaped {
            return Ok(DrainProgress::ChildPending);
        }
        if owner.providers_done {
            return Ok(DrainProgress::Ready);
        }
        if let Some(anchor) = &owner.completed
            && !ledger_completed(anchor)?
        {
            return Ok(DrainProgress::ProviderPending);
        }
        let mut pool = pool;
        for helper in &owner.helpers {
            if actually_reaped(identity.service, *helper, owner.in_process)? {
                continue;
            }
            let Some(current) = pool.as_deref_mut().filter(|pool| {
                pool.identity() == *helper
                    && crate::service_inference::in_process(pool) == owner.in_process
            }) else {
                return Ok(DrainProgress::ProviderPending);
            };
            if !owner.in_process
                && current
                    .reap_witness()
                    .is_some_and(|proof| proof.matches(identity.service, *helper))
            {
                continue;
            }
            let ticket = match current.retirement_ticket(identity)? {
                Some(ticket) => ticket,
                None => current.begin_retirement(identity)?,
            };
            if !matches!(
                current.poll_retirement(&ticket)?,
                ProviderDrainState::Drained
            ) {
                return Ok(DrainProgress::ProviderPending);
            }
        }
        owner.providers_done = true;
        Ok(DrainProgress::Ready)
    }

    /// The caller releases the existing Resources/Broker charges exactly once,
    /// and marks this only after their canonical release succeeds.
    pub fn mark_released(&mut self, identity: WorkerIdentity) -> Result<()> {
        let owner = self.owner_mut(identity)?;
        ensure!(
            owner.child_reaped && owner.providers_done,
            "native exit resources are not complete"
        );
        owner.released = true;
        Ok(())
    }

    pub fn released(&self, identity: WorkerIdentity) -> Result<bool> {
        Ok(self.owner(identity)?.released)
    }
    pub fn identities(&self) -> Vec<WorkerIdentity> {
        self.owners.values().map(|owner| owner.identity).collect()
    }
    pub fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }
    pub fn contains(&self, identity: WorkerIdentity) -> bool {
        self.owners
            .get(&identity.worker)
            .is_some_and(|owner| owner.identity == identity)
    }
    pub fn for_project(&self, project: airk_core::service::ProjectBinding) -> Vec<WorkerIdentity> {
        self.owners
            .values()
            .filter(|owner| owner.identity.project == project)
            .map(|owner| owner.identity)
            .collect()
    }
    pub fn helpers(&self, identity: WorkerIdentity) -> Result<Vec<Incarnation>> {
        Ok(self.owner(identity)?.helpers.clone())
    }
    pub fn in_process(&self, identity: WorkerIdentity) -> Result<bool> {
        Ok(self.owner(identity)?.in_process)
    }
    pub fn completed_authorizations(
        &self,
        identity: WorkerIdentity,
    ) -> Result<Option<airk_core::inference_provider::CompletionLedgerObservation>> {
        Ok(self.owner(identity)?.completed.clone())
    }
    pub fn referenced_ledgers(&self) -> Vec<airk_core::identity::Sha256Digest> {
        self.owners
            .values()
            .filter_map(|owner| owner.completed.as_ref().map(|anchor| anchor.head_sha256))
            .collect()
    }
    pub fn referenced_helpers(&self) -> Vec<Incarnation> {
        self.owners
            .values()
            .flat_map(|owner| {
                owner.helpers.iter().copied().chain(
                    owner
                        .completed
                        .iter()
                        .map(|anchor| anchor.factory_generation),
                )
            })
            .collect()
    }

    /// Invoke only in the gate's pure, synchronous unclaimed-owner removal
    /// callback. All helper Forget writes happen AFTER that gate guard drops.
    pub fn remove_completed(&mut self, identity: WorkerIdentity) -> Result<()> {
        let owner = self.owner(identity)?;
        ensure!(
            owner.released && owner.child_reaped && owner.providers_done,
            "native exit owner still charged"
        );
        self.owners.remove(&identity.worker);
        Ok(())
    }
    fn owner(&self, identity: WorkerIdentity) -> Result<&ResourceOwner> {
        let owner = self
            .owners
            .get(&identity.worker)
            .context("native exit owner unavailable")?;
        ensure!(owner.identity == identity, "foreign native exit owner");
        Ok(owner)
    }
    fn owner_mut(&mut self, identity: WorkerIdentity) -> Result<&mut ResourceOwner> {
        let owner = self
            .owners
            .get_mut(&identity.worker)
            .context("native exit owner unavailable")?;
        ensure!(owner.identity == identity, "foreign native exit owner");
        Ok(owner)
    }
}
