// SPDX-License-Identifier: Apache-2.0
//! Service-owned inference execution, sharing the canonical model handlers.
mod request;
use request::handle;
pub mod pool;
#[cfg(any(test, feature = "test-support"))]
pub(crate) use pool::ComputeHook;
pub use pool::{InferenceOwner, NativeCompletionWitness};
