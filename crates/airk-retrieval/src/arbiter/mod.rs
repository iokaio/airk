// SPDX-License-Identifier: Apache-2.0
//! Optional decision execution is separate from the strict MiniLM catalog.
#[cfg(feature = "local-inference")]
pub mod compiler;
#[cfg(feature = "local-inference")]
mod external_data;
#[cfg(feature = "local-inference")]
pub mod runtime;
#[cfg(feature = "local-inference")]
pub mod tasks;
#[cfg(feature = "local-inference")]
pub mod context_action;

#[cfg(feature = "local-inference")]
pub mod publication;
