// SPDX-License-Identifier: Apache-2.0
//! Answer /3: Split, then per part Plan, Retrieve, Read and check coverage,
//! Follow up and Compose, then Union (final plan section 12). Every read is a
//! canonical dispatch through the one shared read owner (`gather`); models
//! only choose among cards deterministic code proposes.
use super::*;
use airk_core::arbiter::answer_loop as v3;

// The stage modules land one commit at a time; the driver (L3-7) is their
// only product caller, and this allowance is removed with it.
#[allow(dead_code)]
mod gather;
#[allow(dead_code)]
mod retrieve;
#[allow(dead_code)]
mod plan;
#[allow(dead_code)]
mod split;

#[cfg(test)]
mod tests;
