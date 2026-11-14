// SPDX-License-Identifier: Apache-2.0
//! Compatibility surface for the shared raw no-replace publisher.
pub use airk_process::directory::publication::{publish_new, publish_new_at};

#[cfg(test)]
mod tests;
