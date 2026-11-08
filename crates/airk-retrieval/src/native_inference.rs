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
