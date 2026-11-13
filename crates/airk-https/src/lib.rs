// SPDX-License-Identifier: Apache-2.0
//! The https part of the former `airk-cli` crate (docs/simplify/airk-cli-split.md).
extern crate self as airk_https;
pub(crate) use airk_host::runtime_pair;
pub(crate) use airk_installer::control_client;
pub use airk_ipc as ipc;
pub use ipc::endpoint as local_ipc;
/// The service's `/admin` plane (split Phase 5): launch codes, sessions,
/// embedded assets and admin dispatch.
pub mod admin_plane;
pub mod http_backend;
pub mod http_gateway;
/// Setup-only HTTP discovery and enrollment handlers (split step 2b).
pub mod runtime_http_commands;
