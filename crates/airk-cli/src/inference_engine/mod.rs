// SPDX-License-Identifier: Apache-2.0
//! Service-owned inference execution, sharing the canonical model handlers.
mod request;
use request::handle;
pub(crate) mod pool;
#[cfg(test)]
pub(crate) use pool::ComputeHook;
pub(crate) use pool::{InferenceOwner, NativeCompletionWitness};
