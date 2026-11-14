// SPDX-License-Identifier: Apache-2.0
//! Native service-lease adapter for the service's dedicated compute thread.
use anyhow::Result;
use std::sync::Arc;

/// One actual provider owner may drive separate bounded native lanes. The
/// provider's completion barrier must join every lane before releasing this
/// capability; thread count is not a second independently checkpointed owner.
#[cfg(feature = "local-inference")]
#[derive(Clone)]
pub struct NativeInferenceOwner(Arc<airk_store::service::ServiceLease>);
#[cfg(feature = "local-inference")]
impl NativeInferenceOwner {
    /// Private native calibration fixture: acquire the actual exclusive lease,
    /// never a serialized role or execution flag.
    #[cfg(feature = "test-support")]
    pub fn admit_private_fixture(control: &std::path::Path) -> Result<Self> {
        Self::admit(Arc::new(airk_store::service::ServiceLease::acquire(
            control,
        )?))
    }
    pub fn admit(ownership: Arc<airk_store::service::ServiceLease>) -> Result<Self> {
        ownership.admit_inference_owner()?;
        Ok(Self(ownership))
    }
    /// Controlled original-weight baseline under the same native adapter. A
    /// fresh private database captures distinct ORT1.30/weight pipeline IDs.
    #[cfg(feature = "test-support")]
    pub fn run_encoder_evaluation<T>(
        &self,
        sealed_model_json: airk_core::identity::Sha256Digest,
        work: impl FnOnce() -> T,
    ) -> Result<T> {
        self.run(|| -> Result<T> {
            // SAFETY: this owner retains the actual exclusive native lease;
            // run() already entered this thread's normal compute scope.
            let scope = unsafe {
                airk_retrieval::native_inference::EvaluationScope::enter(
                    self.0.clone(),
                    sealed_model_json,
                )?
            };
            let result = work();
            drop(scope);
            Ok(result)
        })?
    }
    pub fn run<T>(&self, work: impl FnOnce() -> T) -> Result<T> {
        // SAFETY: the opaque owner retains the actual exclusive service lease
        // across this synchronous call. Each lane has its own non-Send scope.
        let scope = unsafe { airk_retrieval::native_inference::ServiceComputeScope::enter()? };
        let result = work();
        drop(scope);
        let _ = &self.0;
        Ok(result)
    }
}

#[cfg(feature = "local-inference")]
pub fn run_service_inference<T>(
    ownership: Arc<airk_store::service::ServiceLease>,
    work: impl FnOnce() -> T,
) -> Result<T> {
    NativeInferenceOwner::admit(ownership)?.run(work)
}

#[cfg(not(feature = "local-inference"))]
pub fn run_service_inference<T>(
    _ownership: Arc<airk_store::service::ServiceLease>,
    _work: impl FnOnce() -> T,
) -> Result<T> {
    anyhow::bail!("native inference is not linked into this image")
}

/// Only a private native fixture holding the actual lease can attach candidates.
#[cfg(all(feature = "local-inference", feature = "test-support"))]
pub fn evaluation_catalog(
    lease: Arc<airk_store::service::ServiceLease>,
    profiles: Vec<airk_core::arbiter::Profile>,
) -> Result<airk_retrieval::arbiter::runtime::EvaluationCatalog> {
    // SAFETY: the typed lease is the actual exclusive service owner and remains
    // strongly retained in every derived execution capability and scorer.
    unsafe {
        airk_retrieval::arbiter::runtime::EvaluationCatalog::from_native_lease(lease, profiles)
    }
}

#[cfg(all(test, feature = "local-inference", feature = "test-support"))]
mod evaluation_tests;
