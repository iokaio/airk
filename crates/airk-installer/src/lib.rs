// SPDX-License-Identifier: Apache-2.0
//! The installer part of the former `airk-cli` crate (docs/simplify/airk-cli-split.md).
extern crate self as airk_installer;
pub(crate) use airk_host::runtime_pair;
pub use airk_ipc as ipc;
pub use ipc::endpoint as local_ipc;
/// Client side of the native service control transport (moved from `service_runtime`).
pub mod control_client;
pub mod legacy_processes;
pub mod legacy_startup;
pub mod manual_start;
pub mod runtime_install;
pub mod runtime_legacy_origin;
pub mod service_manager;
pub mod universal_launcher;
