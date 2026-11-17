// SPDX-License-Identifier: Apache-2.0
//! Retain actual GPU session and lease on uncertain native/device settlement.
use super::*;
use std::{
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    time::Instant,
};
pub(super) struct SessionOwner {
    session: ManuallyDrop<Session>,
    #[cfg(feature = "gpu-cuda")]
    gpu: Option<cuda::Binding>,
}
impl SessionOwner {
    pub fn new(session: Session, #[cfg(feature = "gpu-cuda")] gpu: Option<cuda::Binding>) -> Self {
        Self {
            session: ManuallyDrop::new(session),
            #[cfg(feature = "gpu-cuda")]
            gpu,
        }
    }
    pub fn device_settled(&self) -> bool {
        #[cfg(feature = "gpu-cuda")]
        if let Some(g) = &self.gpu {
            return !g.device.poisoned() && g.device.pending_claims() == Some(0);
        }
        true
    }
    pub fn synchronize(&self, deadline: Instant) -> Result<()> {
        #[cfg(feature = "gpu-cuda")]
        if let Some(g) = &self.gpu {
            return g.device.synchronize(deadline);
        }
        let _ = deadline;
        Ok(())
    }
}
impl Deref for SessionOwner {
    type Target = Session;
    fn deref(&self) -> &Session {
        &self.session
    }
}
impl DerefMut for SessionOwner {
    fn deref_mut(&mut self) -> &mut Session {
        &mut self.session
    }
}
impl Drop for SessionOwner {
    fn drop(&mut self) {
        #[cfg(feature = "gpu-cuda")]
        if let Some(gpu) = &self.gpu {
            gpu.end_session_eligibility();
            let _ = gpu.device.synchronize(gpu.load_deadline);
            if gpu.device.poisoned() {
                std::mem::forget(gpu.clone());
                return;
            }
        }
        // SAFETY: exactly once, only after native work is actually settled.
        unsafe { ManuallyDrop::drop(&mut self.session) };
        #[cfg(feature = "gpu-cuda")]
        if let Some(gpu) = &self.gpu {
            let _ = gpu.device.synchronize(gpu.load_deadline);
            if gpu.device.poisoned() {
                std::mem::forget(gpu.clone());
            }
        }
    }
}
