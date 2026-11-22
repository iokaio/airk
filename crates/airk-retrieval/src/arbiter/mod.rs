// SPDX-License-Identifier: Apache-2.0
//! Optional decision execution is separate from the strict MiniLM catalog.
#[cfg(feature = "local-inference")]
pub mod compiler;
#[cfg(feature = "local-inference")]
pub mod context_action;
/// Private evaluation change set 2: Context option B under its own task policy.
#[cfg(feature = "local-inference")]
pub mod context_wide;
#[cfg(all(test, feature = "local-inference"))]
mod context_wide_tests;
#[cfg(feature = "local-inference")]
mod external_data;
/// The Answer loop's card task policies. Their identities are available
/// without native inference, so the no-model loop records the same policy.
pub mod loop_cards;
pub mod loop_coverage;
pub mod loop_follow_up;
pub mod loop_plan;
pub mod loop_split;
#[cfg(feature = "local-inference")]
pub mod runtime;
#[cfg(all(feature = "local-inference", feature = "test-support"))]
pub mod scientific_cal;
#[cfg(feature = "local-inference")]
pub mod tasks;

#[cfg(feature = "local-inference")]
pub mod publication;
