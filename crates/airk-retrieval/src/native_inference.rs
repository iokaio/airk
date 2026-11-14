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
