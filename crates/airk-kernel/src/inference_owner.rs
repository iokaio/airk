// SPDX-License-Identifier: Apache-2.0
//! Native service-lease adapter for the service's dedicated compute thread.
use anyhow::Result;
use std::sync::Arc;

#[cfg(feature = "local-inference")]
pub fn run_service_inference<T>(
    ownership: Arc<airk_store::service::ServiceLease>,
    work: impl FnOnce() -> T,
) -> Result<T> {
    ownership.admit_inference_owner()?;
    // SAFETY: ServiceLease can only be acquired by the native exclusive file
    // owner. Keep that actual object alive across every synchronous FFI call;
    // the non-Send scope cannot escape this thread through the return value.
    let scope = unsafe { airk_retrieval::native_inference::ServiceComputeScope::enter()? };
    let result = work();
    drop(scope);
    drop(ownership);
    Ok(result)
}

#[cfg(not(feature = "local-inference"))]
pub fn run_service_inference<T>(
    _ownership: Arc<airk_store::service::ServiceLease>,
    _work: impl FnOnce() -> T,
) -> Result<T> {
    anyhow::bail!("native inference is not linked into this image")
}
