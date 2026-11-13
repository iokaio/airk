// SPDX-License-Identifier: Apache-2.0
//! The frontend part of the former `airk-cli` crate (docs/simplify/airk-cli-split.md).
extern crate self as airk_frontend;
pub(crate) use airk_host::cli_io;
pub(crate) use airk_host::project_protocol;
pub(crate) use airk_host::runtime_pair;
pub(crate) use airk_host::session_protocol;
pub(crate) use airk_installer::control_client;
pub use airk_ipc as ipc;
pub use ipc::endpoint as local_ipc;
pub use ipc::frame as service_ipc;
pub mod client_requests;
pub mod combined_frontend;
pub mod console_client;
pub mod project_client;
pub mod service_frontend;
pub mod session;
/// The CLI tool call path shared with M3 (split step 3a.2b).
pub mod tool_call;
/// M2 tool catalog documents (split step 3a.1).
pub mod tool_catalog;
pub mod unified_mcp;
pub mod usage_client;
