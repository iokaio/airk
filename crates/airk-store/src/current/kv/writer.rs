// SPDX-License-Identifier: Apache-2.0
//! Admitted publication owner for the redb current engine.
use super::{Engine, journal::Prepared, owner};
use crate::{
    FileStore, WriterLock,
    current::{
        CurrentCapacityChange, CurrentHead, CurrentScope, CurrentToken, TransactionPhase,
        gate::Gate,
        journal::verify_model,
        kv::{catalog, lexical_plan, publication},
    },
    quota::Quota,
};
use anyhow::{Context, Result, ensure};

/// Keeps the shared request gate and worker-owned engine alive for the entire
/// publication lifecycle. In particular, no write transaction outlives Engine.
pub struct RedbWriter<'a> {
    store: FileStore,
    writer: &'a WriterLock,
    quota: Quota,
    scope: CurrentScope,
    capacity: u64,
    gate: Gate,
    engine: Option<Engine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompactionReport {
    pub token: CurrentToken,
    pub compacted: bool,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub bytes_reclaimed: u64,
}

impl FileStore {
    pub fn redb_writer<'a>(
        &self,
        writer: &'a WriterLock,
        quota: &Quota,
        scope: &CurrentScope,
    ) -> Result<RedbWriter<'a>> {
        let (engine, gate, capacity) = owner::writer_engine(self, writer, quota, scope)?;
        Ok(RedbWriter {
            store: self.clone(),
            writer,
            quota: quota.clone(),
            scope: scope.clone(),
            capacity,
            gate,
            engine: Some(engine),
        })
    }
}

impl RedbWriter<'_> {
    /// Publish the first collection policy under its exact initial token.
    pub fn initialize_collection(
        &self,
        expected: &CurrentToken,
        configuration: &crate::current::collection::CollectionConfiguration,
    ) -> Result<crate::current::CurrentCommit> {
        ensure!(
            self.head()?.token()? == *expected && expected.sequence == 0,
            "collection initialization token expired"
        );
        let head = self
            .engine()?
            .initialize_collection(&self.store, configuration)?;
        Ok(crate::current::CurrentCommit {
            token: head.token()?,
            maintenance_error: None,
        })
    }

    pub fn prepare_repack(
        &self,
        base: &super::Snapshot,
        graph: &dyn airk_core::graph_storage::PreparedGraphDelta,
        inputs: &[crate::current::CapturedInput],
        model: Option<airk_core::model_objects::ModelObjectSet>,
    ) -> Result<Prepared> {
        let head = base.head()?;
        ensure!(
            graph.descriptor().base == head.graph_head
                && graph.descriptor().target == head.graph_head,
            "repack changed graph"
        );
        let catalog = catalog::Plan::repack(base, &head, &self.store, inputs)?;
        self.prepare_components(base, &catalog, graph, None, &inputs.to_vec(), model)
    }
    fn engine(&self) -> Result<&Engine> {
        self.engine.as_ref().context("redb writer engine is closed")
    }

    #[cfg(test)]
    pub(crate) fn test_write_meta(&self, key: u8, value: &[u8]) -> Result<()> {
        let engine = self.engine()?;
        (|| -> Result<()> {
            let transaction = engine.write()?;
            transaction
                .open_table(super::tables::META)?
                .insert(key, value)?;
            transaction.commit()?;
            Ok(())
        })()
        .map_err(|error| engine.map_growth_error(error))
    }

    pub fn capacity(&self) -> u64 {
        self.engine
            .as_ref()
            .map_or(self.capacity, |engine| engine.capacity())
    }

    pub fn head(&self) -> Result<CurrentHead> {
        self.engine()?.snapshot()?.head()
    }

    pub fn phase(&self) -> Result<TransactionPhase> {
        self.engine()?.snapshot()?.phase()
    }

    pub fn pending(&self) -> Result<Option<Prepared>> {
        self.engine()?.pending_publication()
    }

    pub fn verify_ready(&self) -> Result<()> {
        ensure!(
            self.phase()? == TransactionPhase::Ready && self.pending()?.is_none(),
            "native verification requires Ready with no pending publication"
        );
        let head = self.head()?;
        let transaction = self.engine()?.write()?;
        publication::verify(
            &transaction,
            &head,
            &mut publication::Verification::default(),
        )
    }

    pub fn release_orphan_grants(&self) -> Result<usize> {
        ensure!(
            self.phase()? == TransactionPhase::Ready && self.pending()?.is_none(),
            "redb orphan grants cannot be released during publication"
        );
        owner::settle_replay(&self.store, &self.scope, self.head()?.token()?)?;
        let lifetime = owner::grant_scope(&self.scope).transaction;
        let grants = self.quota.current_grants(self.writer)?;
        let mut released = 0;
        for grant in grants {
            // Sibling rebuilds retain their own durable grants until the last
            // old request releases retirement. This writer may settle only its
            // exact incarnation; other grants remain charged and untouched.
            if grant.scope().incarnation != self.scope.incarnation {
                continue;
            }
            if grant.scope().transaction != lifetime {
                grant.release(self.writer)?;
                released += 1;
            }
        }
        Ok(released)
    }

    pub fn reclaim_objects(
        &self,
        plan: &crate::current::maintenance::ReclamationPlan,
    ) -> Result<crate::current::maintenance::ReclamationReport> {
        ensure!(
            self.phase()? == TransactionPhase::Ready
                && self.pending()?.is_none()
                && self.head()?.token()? == plan.token,
            "redb reclamation proof expired or publication is pending"
        );
        if self.writer.has_object_captures() {
            return Ok(crate::current::maintenance::ReclamationReport {
                pending: true,
                ..Default::default()
            });
        }
        if !self.gate.try_exclusive()? {
            return Ok(crate::current::maintenance::ReclamationReport {
                pending: true,
                ..Default::default()
            });
        }
        let result = crate::current::maintenance::reclaim_files(self.store.directory(), plan);
        let restored = self.gate.restore_shared();
        match (result, restored) {
            (Ok(report), Ok(())) => Ok(report),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    pub fn verify_pending_base(&self, pending: &Prepared) -> Result<()> {
        ensure!(
            self.phase()? == TransactionPhase::Prepared
                && self.pending()?.as_ref() == Some(pending)
                && self.head()?.token()? == pending.expected,
            "native prepared base changed"
        );
        let transaction = self.engine()?.write()?;
        publication::verify(
            &transaction,
            &self.head()?,
            &mut publication::Verification::default(),
        )
    }

    pub fn prepare_components(
        &self,
        base: &super::Snapshot,
        catalog: &catalog::Plan<'_>,
        graph: &dyn airk_core::graph_storage::PreparedGraphDelta,
        lexical: Option<&lexical_plan::Sealed>,
        inputs: &dyn crate::current::CapturedInputs,
        model: Option<airk_core::model_objects::ModelObjectSet>,
    ) -> Result<Prepared> {
        ensure!(
            self.phase()? == TransactionPhase::Ready && self.pending()?.is_none(),
            "native preparation requires Ready with no pending publication"
        );
        ensure!(
            base.head()?.scope == self.scope && base.head()?.token()? == self.head()?.token()?,
            "redb writer base is stale or foreign"
        );
        owner::settle_replay(&self.store, &self.scope, self.head()?.token()?)?;
        let engine = self.engine()?;
        engine
            .prepare_native_components(
                base,
                catalog,
                graph,
                lexical,
                publication::Capture {
                    inputs,
                    model,
                    capacity: self.capacity(),
                },
            )
            .map_err(|error| engine.map_growth_error(error))
    }

    /// Verifies all source-independent captured object identities and model
    /// bindings before making a prepared journal replayable.
    pub fn mark_applying(&self, pending: &Prepared) -> Result<()> {
        self.mark_applying_with_admission_observer(pending, || Ok(()))
    }
    #[cfg(test)]
    pub(crate) fn mark_applying_observed(
        &self,
        pending: &Prepared,
        after_admission: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        self.mark_applying_with_admission_observer(pending, after_admission)
    }
    fn mark_applying_with_admission_observer(
        &self,
        pending: &Prepared,
        after_admission: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        ensure!(
            pending.intent.is_some(),
            "native redb publication has no captured intention"
        );
        self.engine()?
            .visit_captured(pending, &mut |input| input.verify(&self.store))?;
        verify_model(&self.store, &pending.intent.as_ref().unwrap().model)?;
        let engine = self.engine()?;
        let admitted = owner::replay_admitted(&self.store, &self.scope, pending)?;
        if self.phase()? == TransactionPhase::Applying {
            ensure!(
                admitted,
                "Applying publication lacks durable replay admission"
            );
            return Ok(());
        }
        if !admitted {
            let mut verification = publication::Verification::default();
            let high_water = engine
                .probe_application(
                    pending,
                    |transaction, target| {
                        publication::verify(transaction, target, &mut verification)
                    },
                    || Ok(std::fs::metadata(self.store.directory().join("current.airk"))?.len()),
                )
                .map_err(|error| engine.map_growth_error(error))?;
            owner::admit_replay(
                &self.store,
                self.writer,
                &self.quota,
                &self.scope,
                engine,
                pending,
                high_water,
            )?;
        }
        after_admission()?;
        engine
            .mark_applying(pending)
            .map_err(|error| engine.map_growth_error(error))
    }

    /// Applies authenticated catalog, graph and lexical afterimages in the
    /// same transaction that installs the target head.
    pub fn commit(&self, pending: &Prepared) -> Result<CurrentToken> {
        ensure!(
            pending.intent.is_some(),
            "native redb publication has no captured intention"
        );
        if self.phase()? != TransactionPhase::Ready {
            ensure!(
                owner::replay_admitted(&self.store, &self.scope, pending)?,
                "publication lacks durable replay admission"
            );
            self.engine()?
                .visit_captured(pending, &mut |input| input.verify(&self.store))?;
            verify_model(&self.store, &pending.intent.as_ref().unwrap().model)?;
        }
        let mut verification = publication::Verification::default();
        let engine = self.engine()?;
        let token = engine
            .apply(pending, |transaction, target| {
                publication::verify(transaction, target, &mut verification)
            })
            .map_err(|error| engine.map_growth_error(error))?;
        owner::settle_replay(&self.store, &self.scope, token)?;
        Ok(token)
    }

    pub fn abort_prepared(&self, pending: &Prepared) -> Result<()> {
        let engine = self.engine()?;
        engine
            .abort_prepared(pending)
            .map_err(|error| engine.map_growth_error(error))?;
        owner::settle_replay(&self.store, &self.scope, self.head()?.token()?)
    }

    pub fn compact(&mut self) -> Result<CompactionReport> {
        ensure!(
            self.phase()? == TransactionPhase::Ready && self.pending()?.is_none(),
            "redb compaction requires a Ready collection with no pending publication"
        );
        if let Some(engine) = &self.engine {
            engine.clear_capacity_admitter();
        }
        self.engine.take();
        let result = owner::compact(
            &self.store,
            self.writer,
            &self.quota,
            &self.scope,
            &self.gate,
        );
        let reopened = owner::writer_engine(&self.store, self.writer, &self.quota, &self.scope);
        match (result, reopened) {
            (Ok((compacted, bytes_before, bytes_after, token)), Ok(reopened)) => {
                self.engine = Some(reopened.0);
                self.capacity = reopened.2;
                Ok(CompactionReport {
                    token,
                    compacted,
                    bytes_before,
                    bytes_after,
                    bytes_reclaimed: bytes_before.saturating_sub(bytes_after),
                })
            }
            (Err(error), Ok(reopened)) => {
                self.engine = Some(reopened.0);
                self.capacity = reopened.2;
                Err(error)
            }
            (Ok(_), Err(error)) => Err(error),
            (Err(error), Err(reopen)) => {
                Err(error.context(format!("redb writer reopen also failed: {reopen:#}")))
            }
        }
    }

    /// Growth requires every engine and snapshot pin to be released. The owner
    /// upgrades the durable grant under exclusive gate protection, then this
    /// writer reopens the matching engine before returning to normal service.
    pub fn grow_capacity(&mut self, next: u64) -> Result<CurrentCapacityChange> {
        ensure!(
            self.phase()? == TransactionPhase::Ready && self.pending()?.is_none(),
            "redb capacity cannot change during publication"
        );
        if let Some(engine) = &self.engine {
            engine.clear_capacity_admitter();
        }
        self.engine.take();
        let changed = owner::grow(
            &self.store,
            self.writer,
            &self.quota,
            &self.scope,
            next,
            &self.gate,
            &mut |_| Ok(()),
        );
        // Reconcile from the durable owner even when a fault was reported
        // after its replacement was synced.
        let reopened = owner::writer_engine(&self.store, self.writer, &self.quota, &self.scope);
        match (changed, reopened) {
            (Ok(changed), Ok(reopened)) => {
                self.engine = Some(reopened.0);
                self.capacity = reopened.2;
                Ok(changed)
            }
            (Err(error), Ok(reopened)) => {
                self.capacity = reopened.2;
                self.engine = Some(reopened.0);
                Err(error)
            }
            (Ok(_), Err(error)) => Err(error),
            (Err(error), Err(reopen)) => {
                Err(error.context(format!("redb writer reopen also failed: {reopen:#}")))
            }
        }
    }
}

impl Drop for RedbWriter<'_> {
    fn drop(&mut self) {
        if let Some(engine) = &self.engine {
            engine.clear_capacity_admitter();
        }
    }
}
