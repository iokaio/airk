// SPDX-License-Identifier: Apache-2.0
//! Process-local math default before any application worker or ORT initialization.
use std::ffi::OsString;

fn supply_default(
    enabled: bool,
    read: impl FnOnce() -> Option<OsString>,
    write: impl FnOnce(&'static str),
) {
    if enabled && read().is_none() {
        write("2");
    }
}

/// Establish only an absent process-local value; preserve explicit user policy.
///
/// # Safety
/// Call only as the first operation in synchronous main, before starting AIRK
/// workers, Tokio, ORT, or other application initialization. Never call later
/// to repair a captured or active math environment.
#[cfg(all(windows, feature = "gpu-cuda"))]
pub(super) unsafe fn initialize_before_workers() {
    supply_default(
        cfg!(all(windows, feature = "gpu-cuda")),
        || std::env::var_os("ORT_CUDA_GEMM_OPTIONS"),
        |value| {
            // SAFETY: synchronous main calls this before application threads or
            // ORT initialization. Only this process changes, on Windows only.
            unsafe { std::env::set_var("ORT_CUDA_GEMM_OPTIONS", value) };
        },
    );
}

#[cfg(test)]
mod tests;
