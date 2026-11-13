// SPDX-License-Identifier: Apache-2.0
//! Where one worker keeps its owner-wide state.
//!
//! A child-process worker keeps its project lease, execution owner and helper
//! connector for the life of its process, as before. An in-process worker
//! keeps them in its own [`WorkerScope`]: bound on every thread that worker
//! runs, never visible to another worker, and released when the worker exits.
use airk_kernel::service_execution::{ExecutionScope, ScopeBinding};
use airk_retrieval::embedding_worker::{ConnectorBinding, ConnectorScope};
use airk_store::service::ProjectLease;
use std::{
    cell::RefCell,
    sync::{Arc, Mutex, PoisonError},
};

thread_local! { static LIFE: RefCell<Option<ThreadBinding>> = const { RefCell::new(None) }; }

pub(crate) enum Residency {
    /// The worker is its own process; process-wide slots are its slots.
    Process,
    /// The worker runs inside the service process.
    Service(WorkerScope),
}
impl Residency {
    pub(crate) fn scope(&self) -> Option<&WorkerScope> {
        match self {
            Self::Process => None,
            Self::Service(scope) => Some(scope),
        }
    }
}

/// One in-process worker's slots. Cloning shares the same slots.
#[derive(Clone, Default)]
pub(crate) struct WorkerScope {
    pub(crate) execution: ExecutionScope,
    pub(crate) connector: ConnectorScope,
    /// The owner's project lease. Its thread releases it only after every
    /// thread and runtime of that owner returned, as a process exit would; an
    /// early return from the owner's body never releases it under a native
    /// call that is still running.
    lease: Arc<Mutex<Option<Arc<ProjectLease>>>>,
}

/// Restores the thread's previous bindings when dropped.
pub(crate) struct ThreadBinding {
    _execution: ScopeBinding,
    _connector: ConnectorBinding,
}

impl WorkerScope {
    pub(crate) fn new() -> Self {
        Self::default()
    }
    /// Bind both slots to the calling thread until the guard drops.
    pub(crate) fn bind_thread(&self) -> ThreadBinding {
        ThreadBinding {
            _execution: self.execution.bind_thread(),
            _connector: self.connector.bind_thread(),
        }
    }
    /// Bind both slots for the rest of the calling thread's life. Used for an
    /// owner runtime's threads, which serve only that owner.
    pub(crate) fn bind_for_thread_life(&self) {
        let binding = self.bind_thread();
        LIFE.with(|slot| *slot.borrow_mut() = Some(binding));
    }
    /// Hold the owner's project lease in this scope, once.
    pub(crate) fn retain_lease(&self, lease: Arc<ProjectLease>) -> anyhow::Result<()> {
        let mut slot = self.lease.lock().unwrap_or_else(PoisonError::into_inner);
        anyhow::ensure!(slot.is_none(), "worker already owns a project lease");
        *slot = Some(lease);
        Ok(())
    }
    /// Called only by the owner's thread, after its last runtime returned.
    pub(crate) fn release_lease(&self) {
        self.lease
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }
}

/// Releases one collection's shared store caches when its owner's body
/// returns, on success, error or unwind. Declared before the owner's kernels
/// and jobs, it drops after them. Another live owner of the same collection
/// only loses reuse and re-verifies on its next read.
pub(crate) struct CollectionCaches {
    data_root: std::path::PathBuf,
    collection: String,
}
impl CollectionCaches {
    pub(crate) fn release_on_exit(data_root: &str, collection: &str) -> Self {
        Self {
            data_root: data_root.into(),
            collection: collection.into(),
        }
    }
}
impl Drop for CollectionCaches {
    fn drop(&mut self) {
        airk_kernel::release_collection_caches(&self.data_root, &self.collection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        identity::Incarnation,
        service::{ProjectBinding, WorkerIdentity},
    };

    fn identity(byte: u8) -> WorkerIdentity {
        WorkerIdentity {
            service: Incarnation::from_bytes([byte; 16]),
            worker: Incarnation::from_bytes([byte.wrapping_add(1); 16]),
            project: ProjectBinding::from_bytes([byte; 32]),
            fencing_generation: 1,
        }
    }

    fn grant(identity: WorkerIdentity, id: u8) -> airk_core::service_resources::WorkAllowance {
        airk_core::service_resources::WorkAllowance {
            id: Incarnation::from_bytes([id; 16]),
            identity,
            job: airk_core::identity::Sha256Digest::from_bytes([id; 32]),
            extraction_threads: 1,
            inference_threads: 1,
            working_memory_bytes: 128 << 20,
            background_io_bytes_per_second: 1 << 20,
            ttl_ms: 60_000,
        }
    }

    #[test]
    fn two_in_process_workers_observe_only_their_own_execution_owner() -> anyhow::Result<()> {
        use airk_kernel::service_execution as execution;
        let roots = [tempfile::tempdir()?, tempfile::tempdir()?];
        let scopes = [WorkerScope::new(), WorkerScope::new()];
        let identities = [identity(1), identity(7)];
        for ((scope, root), identity) in scopes.iter().zip(&roots).zip(identities) {
            scope.execution.install(root.path(), identity)?;
        }
        // An unbound service thread sees no owner; it never borrows a worker's.
        assert!(execution::renew(identities[0]).is_err());
        std::thread::scope(|threads| -> anyhow::Result<()> {
            let workers = scopes
                .iter()
                .zip(identities)
                .enumerate()
                .map(|(index, (scope, identity))| {
                    threads.spawn(move || -> anyhow::Result<()> {
                        let _bound = scope.bind_thread();
                        let other = identities[1 - index];
                        execution::activate(&grant(identity, 10 + index as u8))?;
                        anyhow::ensure!(execution::activate(&grant(other, 20)).is_err());
                        anyhow::ensure!(execution::renew(other).is_err());
                        execution::renew(identity)?;
                        anyhow::ensure!(execution::progress().is_some());
                        // A helper thread inherits only this worker's binding.
                        let inherited = airk_kernel::service_execution::ExecutionScope::current();
                        std::thread::scope(|helper| {
                            helper
                                .spawn(move || {
                                    let _bound = inherited.as_ref().map(|s| s.bind_thread());
                                    execution::renew(identity)
                                })
                                .join()
                                .map_err(|_| anyhow::anyhow!("helper panicked"))?
                        })
                    })
                })
                .collect::<Vec<_>>();
            for worker in workers {
                worker
                    .join()
                    .map_err(|_| anyhow::anyhow!("worker panicked"))??;
            }
            Ok(())
        })?;
        // A second installation into the same scope is refused, as in a process.
        assert!(
            scopes[0]
                .execution
                .install(roots[0].path(), identities[0])
                .is_err()
        );
        // Bindings are restored when their guard drops.
        assert!(airk_kernel::service_execution::ExecutionScope::current().is_none());
        Ok(())
    }

    #[test]
    fn a_drained_owner_stops_its_work_at_the_next_safe_point() -> anyhow::Result<()> {
        use airk_kernel::service_execution as execution;
        let root = tempfile::tempdir()?;
        let scope = WorkerScope::new();
        let identity = identity(3);
        scope.execution.install(root.path(), identity)?;
        let other = WorkerScope::new();
        other.execution.install(root.path(), self::identity(9))?;
        let _bound = scope.bind_thread();
        let grant = grant(identity, 4);
        execution::activate(&grant)?;
        let mut before_drain = false;
        let result: anyhow::Result<()> = execution::run(grant, || {
            execution::checkpoint()?;
            before_drain = true;
            // Draining another owner never reaches this one.
            other.execution.drain();
            execution::checkpoint()?;
            anyhow::ensure!(!airk_ingest::pdf_slot::interrupted());
            scope.execution.drain();
            // A PDF waiting for the parser slot observes the same drain.
            anyhow::ensure!(airk_ingest::pdf_slot::interrupted());
            execution::checkpoint()?;
            anyhow::bail!("work continued past a drained safe point")
        });
        assert!(before_drain);
        let error = result.unwrap_err();
        assert!(
            error.chain().any(|cause| cause.is::<execution::Draining>()),
            "{error:#}"
        );
        // Draining is not a cancellation: no settled outcome is implied.
        assert!(!error.chain().any(|cause| cause.is::<execution::Canceled>()));
        Ok(())
    }

    #[test]
    fn a_drain_requested_before_install_stops_startup_activation_and_reads() -> anyhow::Result<()> {
        use airk_kernel::service_execution as execution;
        let root = tempfile::tempdir()?;
        let scope = WorkerScope::new();
        let other = WorkerScope::new();
        let bound = scope.bind_thread();
        // Startup before installation: nothing asked this owner to stop yet.
        execution::checkpoint()?;
        scope.execution.drain();
        assert!(scope.execution.draining());
        let startup = execution::checkpoint().unwrap_err();
        assert!(startup.is::<execution::Draining>(), "{startup:#}");
        // Activation and reads run after installation but without a work
        // grant; they observe the same drain, which is never a cancellation.
        scope.execution.install(root.path(), identity(12))?;
        let activation = execution::checkpoint().unwrap_err();
        assert!(activation.is::<execution::Draining>(), "{activation:#}");
        assert!(!activation.is::<execution::Canceled>());
        // Another owner and an unbound thread never observe this drain.
        {
            let _other = other.bind_thread();
            execution::checkpoint()?;
        }
        std::thread::spawn(execution::checkpoint)
            .join()
            .map_err(|_| anyhow::anyhow!("unbound probe panicked"))??;
        assert!(!other.execution.draining());
        drop(bound);
        Ok(())
    }

    #[test]
    fn a_quarantined_scope_keeps_its_first_reason_and_drains_every_clone() {
        use airk_kernel::service_execution::StuckReason;
        let scope = WorkerScope::new();
        let observer = scope.clone();
        assert_eq!(observer.execution.stuck(), None);
        assert!(!observer.execution.draining());
        scope.execution.quarantine(StuckReason::QueryDeadline);
        scope.execution.quarantine(StuckReason::WorkStalled);
        assert_eq!(observer.execution.stuck(), Some(StuckReason::QueryDeadline));
        assert!(observer.execution.draining());
        assert_eq!(
            StuckReason::QueryDeadline.code(),
            "owner_quarantined_query_deadline"
        );
        assert_eq!(
            StuckReason::WorkStalled.code(),
            "owner_quarantined_work_stalled"
        );
        assert_eq!(WorkerScope::new().execution.stuck(), None);
    }

    #[test]
    fn a_scope_holds_its_owner_lease_until_the_owner_releases_it() -> anyhow::Result<()> {
        let runtime = tempfile::tempdir()?;
        let project = ProjectBinding::from_bytes([8; 32]);
        let scope = WorkerScope::new();
        let supervisor_view = scope.clone();
        scope.retain_lease(Arc::new(ProjectLease::acquire(runtime.path(), project)?))?;
        // Only one lease per owner; a second is refused, not replaced.
        assert!(
            scope
                .retain_lease(Arc::new(ProjectLease::acquire(
                    runtime.path(),
                    ProjectBinding::from_bytes([9; 32]),
                )?))
                .is_err()
        );
        // The supervisor's clone of the scope never releases the lease.
        drop(supervisor_view);
        assert!(ProjectLease::acquire(runtime.path(), project).is_err());
        scope.release_lease();
        drop(ProjectLease::acquire(runtime.path(), project)?);
        Ok(())
    }

    #[test]
    fn two_in_process_owners_of_one_project_exclude_each_other() -> anyhow::Result<()> {
        let runtime = tempfile::tempdir()?;
        let project = ProjectBinding::from_bytes([5; 32]);
        let held = airk_store::service::ProjectLease::acquire(runtime.path(), project)?;
        // Another owner thread of the same service process is refused while
        // the first holds the lease, exactly as another process was.
        let refused = std::thread::spawn({
            let runtime = runtime.path().to_owned();
            move || airk_store::service::ProjectLease::acquire(&runtime, project).is_err()
        })
        .join()
        .map_err(|_| anyhow::anyhow!("lease probe panicked"))?;
        assert!(refused);
        // A different project is independent.
        drop(airk_store::service::ProjectLease::acquire(
            runtime.path(),
            ProjectBinding::from_bytes([6; 32]),
        )?);
        drop(held);
        drop(airk_store::service::ProjectLease::acquire(
            runtime.path(),
            project,
        )?);
        Ok(())
    }

    #[test]
    fn a_bound_worker_never_falls_back_to_the_process_connector() {
        let scope = WorkerScope::new();
        let _bound = scope.bind_thread();
        let error = match airk_retrieval::embedding_worker::connector() {
            Ok(_) => panic!("an uninstalled scope exposed a connector"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("native adapter not installed"), "{error}");
    }
}
