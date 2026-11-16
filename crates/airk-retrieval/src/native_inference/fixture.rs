// SPDX-License-Identifier: Apache-2.0
//! Real native lease retained through component model calls and teardown.
use super::ServiceComputeScope;
use anyhow::Result;

/// Test-only adapter; the guard is declared before models so they drop first.
pub(crate) struct NativeCompute {
    // Field order releases the thread role, actual lease, then private directory.
    _scope: ServiceComputeScope,
    _lease: airk_store::service::ServiceLease,
    _control: tempfile::TempDir,
}
impl NativeCompute {
    pub(crate) fn admit() -> Result<Self> {
        let control = tempfile::Builder::new()
            .prefix("airk-retrieval-native-")
            .tempdir_in(std::env::temp_dir().canonicalize()?)?;
        let lease = airk_store::service::ServiceLease::acquire(control.path())?;
        lease.admit_inference_owner()?;
        // SAFETY: the actual exclusive native lease remains in this non-Send
        // guard until every synchronous call/model teardown and scope finish.
        let scope = unsafe { ServiceComputeScope::enter()? };
        Ok(Self {
            _scope: scope,
            _lease: lease,
            _control: control,
        })
    }
}
