// SPDX-License-Identifier: Apache-2.0
//! Native installed GPU runtime adapter. No config/client path is authority.
use anyhow::{Context, Result};
use std::{path::PathBuf, sync::Arc, time::Instant};
pub fn installed_paths() -> Result<(PathBuf, PathBuf)> {
    let image = std::env::current_exe()?;
    let root = image
        .parent()
        .context("native service image has no parent")?
        .to_path_buf();
    Ok((root.clone(), root.join("airk-cuda-runtime.json")))
}
pub fn admit(
    lease: Arc<airk_store::service::ServiceLease>,
    root: &std::path::Path,
    manifest: &std::path::Path,
    deadline: Instant,
) -> Result<Arc<airk_retrieval::arbiter::runtime::cuda::CudaRuntime>> {
    anyhow::ensure!(
        (root.to_owned(), manifest.to_owned()) == installed_paths()?,
        "GPU runtime location is not the native image layout"
    );
    // SAFETY: real exclusive service lease; Retrieval independently requires
    // ServiceComputeScope and retains this exact lease through native work.
    unsafe {
        airk_retrieval::arbiter::runtime::cuda::CudaRuntime::from_native_lease(
            lease, root, manifest, deadline,
        )
    }
}
