// SPDX-License-Identifier: Apache-2.0
//! Release AIRK's process-global OrtEnv before Darwin runs library destructors.
use anyhow::{Result, ensure};
use ort::AsPointer;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

static ENVIRONMENT: AtomicPtr<ort::sys::OrtEnv> = AtomicPtr::new(std::ptr::null_mut());
static USERS: AtomicUsize = AtomicUsize::new(0);
const CLOSING: usize = 1 << (usize::BITS - 1);

unsafe extern "C" {
    fn atexit(callback: extern "C" fn()) -> std::ffi::c_int;
}

pub(super) struct SessionLease;
impl SessionLease {
    pub(super) fn acquire() -> Result<Self> {
        USERS
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |users| {
                if users & CLOSING != 0 {
                    None
                } else {
                    users.checked_add(1).filter(|next| *next < CLOSING)
                }
            })
            .map_err(|_| anyhow::anyhow!("INT8 runtime is shutting down"))?;
        Ok(Self)
    }
}
impl Drop for SessionLease {
    fn drop(&mut self) {
        USERS.fetch_sub(1, Ordering::Release);
    }
}

extern "C" fn release_environment() {
    // Close admission before testing the count: a late model load cannot race
    // ReleaseEnv. Live sessions on an abnormal exit must not lose their env.
    if USERS.fetch_or(CLOSING, Ordering::AcqRel) & !CLOSING != 0 {
        return;
    }
    let environment = ENVIRONMENT.swap(std::ptr::null_mut(), Ordering::AcqRel);
    if !environment.is_null() {
        // SAFETY: AIRK exclusively created this environment, its dynamic library
        // stays pinned by ort, and every AIRK session has released its lease.
        // ort 2.0.0-rc.10 retains its Environment in a Rust static which is never
        // dropped. Release the C object once before ONNX's C++ static mutexes.
        unsafe { (ort::api().ReleaseEnv)(environment) };
    }
}

/// Called once after AIRK successfully commits its exclusively owned OrtEnv,
/// before any session is built. Registering after initialization places this
/// cleanup before the runtime's C++ destructors in reverse registration order.
pub(super) fn install() -> Result<()> {
    let environment = ort::environment::get_environment()?.ptr().cast_mut();
    ENVIRONMENT.store(environment, Ordering::Release);
    // SAFETY: the callback has process lifetime, uses only pinned API/static
    // storage, cannot unwind, and releases no environment with active sessions.
    let registered = unsafe { atexit(release_environment) } == 0;
    if !registered {
        // No session can exist yet; fail initialization after releasing ownership.
        release_environment();
    }
    ensure!(registered, "INT8 runtime shutdown registration failed");
    Ok(())
}
