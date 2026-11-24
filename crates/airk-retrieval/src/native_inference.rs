// SPDX-License-Identifier: Apache-2.0
//! Native service compute-thread admission; feature unification grants no role.
use anyhow::{Result, ensure};
use std::{cell::Cell, marker::PhantomData, rc::Rc};

thread_local! { static OWNER: Cell<bool> = const { Cell::new(false) }; }

/// Non-Send scope. Moving a model or a closure to another thread never transfers
/// this authorization; the receiving native owner must hold its own lease.
pub struct ServiceComputeScope {
    _thread: PhantomData<Rc<()>>,
}
impl ServiceComputeScope {
    /// # Safety
    /// The adapter must retain its actual exclusive native service lease (or
    /// the explicitly qualified historical provider's native embedding lease) until
    /// this scope and every synchronous FFI call have completed. This function
    /// accepts no serialized role flag, PID, model descriptor or receipt.
    pub unsafe fn enter() -> Result<Self> {
        OWNER.with(|owner| {
            ensure!(!owner.get(), "inference compute role already entered");
            owner.set(true);
            Ok(Self {
                _thread: PhantomData,
            })
        })
    }
}
impl Drop for ServiceComputeScope {
    fn drop(&mut self) {
        OWNER.with(|owner| owner.set(false));
    }
}
pub(crate) fn require_service_compute() -> Result<()> {
    ensure!(
        OWNER.with(Cell::get),
        "native inference requires the service-owned compute thread"
    );
    Ok(())
}

/// Runs `work` once for each entry of `workers` on helper threads scoped inside
/// the calling compute thread's synchronous call. A helper holds the compute
/// role only while its closure runs, and every helper is joined before this
/// returns, including on failure, so the caller's lease covers each native
/// call a helper makes. Results keep the order of `workers`; the first failure
/// in that order is returned after all helpers have finished.
pub(crate) fn scoped_compute<W: Send, R: Send>(
    workers: Vec<W>,
    work: impl Fn(W) -> Result<R> + Sync,
) -> Result<Vec<R>> {
    require_service_compute()?;
    struct Role;
    impl Drop for Role {
        fn drop(&mut self) {
            OWNER.with(|owner| owner.set(false));
        }
    }
    let results = std::thread::scope(|scope| {
        let handles = workers
            .into_iter()
            .map(|worker| {
                let work = &work;
                std::thread::Builder::new()
                    .name("airk-inference-session".into())
                    .stack_size(8 << 20)
                    .spawn_scoped(scope, move || {
                        OWNER.with(|owner| owner.set(true));
                        let _role = Role;
                        work(worker)
                    })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .map_err(anyhow::Error::from)?
                    .join()
                    .map_err(|_| anyhow::anyhow!("inference session worker panicked"))?
            })
            .collect::<Vec<_>>()
    });
    results.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scope_is_thread_local_and_does_not_survive_owner_exit() -> Result<()> {
        assert!(require_service_compute().is_err());
        // SAFETY: this private fixture performs no native call, owns its entire
        // scope, and joins the child before releasing the explicit capability.
        let scope = unsafe { ServiceComputeScope::enter()? };
        require_service_compute()?;
        assert!(unsafe { ServiceComputeScope::enter() }.is_err());
        assert!(
            std::thread::spawn(|| require_service_compute().is_err())
                .join()
                .unwrap()
        );
        drop(scope);
        assert!(require_service_compute().is_err());
        Ok(())
    }

    #[test]
    fn scoped_helpers_hold_the_role_only_inside_the_owner_call() -> Result<()> {
        assert!(scoped_compute(vec![0], |_| require_service_compute()).is_err());
        // SAFETY: this private fixture performs no native call and joins every
        // helper (inside scoped_compute) before releasing the capability.
        let scope = unsafe { ServiceComputeScope::enter()? };
        let threads = scoped_compute(vec![3, 1, 2], |value| {
            require_service_compute()?;
            Ok((value, std::thread::current().id()))
        })?;
        assert_eq!(
            threads.iter().map(|(value, _)| *value).collect::<Vec<_>>(),
            [3, 1, 2]
        );
        assert!(
            threads
                .iter()
                .all(|(_, id)| *id != std::thread::current().id())
        );
        let finished = std::sync::atomic::AtomicUsize::new(0);
        let failed = scoped_compute(vec![0, 1, 2, 3], |value| {
            if value == 1 {
                anyhow::bail!("original session failure");
            }
            finished.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(value)
        });
        assert!(
            failed
                .unwrap_err()
                .to_string()
                .contains("original session failure")
        );
        assert_eq!(finished.load(std::sync::atomic::Ordering::SeqCst), 3);
        assert!(
            scoped_compute(vec![0], |value| -> Result<usize> {
                assert_ne!(value, 0, "fictional session panic");
                Ok(value)
            })
            .is_err()
        );
        // The owner keeps its own role after its helpers have ended.
        require_service_compute()?;
        drop(scope);
        assert!(require_service_compute().is_err());
        Ok(())
    }
}

#[cfg(feature = "test-support")]
mod evaluation {
    use super::*;
    use airk_core::identity::Sha256Digest;
    use sha2::{Digest, Sha256};
    use std::{cell::RefCell, sync::Arc};
    #[derive(Clone)]
    struct Admission {
        lease: Arc<dyn Send + Sync>,
        manifest: Sha256Digest,
    }
    thread_local! {static EVALUATION:RefCell<Option<Admission>>=const {RefCell::new(None)};}
    pub struct EvaluationScope {
        _thread: PhantomData<Rc<()>>,
    }
    impl EvaluationScope {
        /// # Safety
        /// The adapter must pass its actual exclusive ServiceLease, retain its
        /// ordinary ServiceComputeScope, and use an isolated private fixture.
        /// The digest seals the actual model.json bytes, never a serialized role.
        pub unsafe fn enter(lease: Arc<dyn Send + Sync>, manifest: Sha256Digest) -> Result<Self> {
            require_service_compute()?;
            EVALUATION.with(|slot| -> Result<Self> {
                let mut slot = slot.borrow_mut();
                ensure!(slot.is_none(), "encoder evaluation scope already entered");
                *slot = Some(Admission { lease, manifest });
                Ok(Self {
                    _thread: PhantomData,
                })
            })
        }
    }
    impl Drop for EvaluationScope {
        fn drop(&mut self) {
            EVALUATION.with(|slot| *slot.borrow_mut() = None);
        }
    }
    pub(crate) fn lease(bytes: &[u8]) -> Option<Arc<dyn Send + Sync>> {
        if require_service_compute().is_err() {
            return None;
        }
        let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
        EVALUATION.with(|slot| {
            slot.borrow()
                .as_ref()
                .filter(|admission| admission.manifest == digest)
                .map(|admission| admission.lease.clone())
        })
    }
}
#[cfg(feature = "test-support")]
pub use evaluation::EvaluationScope;
#[cfg(feature = "test-support")]
pub(crate) use evaluation::lease as evaluation_model_lease;

#[cfg(test)]
pub(crate) mod fixture;
