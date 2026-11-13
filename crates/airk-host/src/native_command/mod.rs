// SPDX-License-Identifier: Apache-2.0
//! Bounded native command adapter: one contained child, a fixed deadline and
//! capped input and output. Callers own each command's meaning.
//! Moved to `airk-process` (`bounded_command`, split U90) so `airkctl` runs
//! the same bounded commands; re-exported for existing callers.
#[cfg(any(windows, target_os = "macos", test))]
pub use airk_process::bounded_command::execute_native_observation;
#[cfg(any(target_os = "macos", test, feature = "test-support"))]
pub use airk_process::bounded_command::execute_native_program;
#[cfg(any(test, feature = "test-support"))]
pub use airk_process::bounded_command::execute_with_timeout;
pub use airk_process::bounded_command::{Invocation, MAX_CONFIG, Output, execute, windows_tool};
