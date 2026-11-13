// SPDX-License-Identifier: Apache-2.0
//! Native adapter primitives shared by service/client executable modes.
extern crate self as airk_cli;
/// I3 admin HTTP contract (split C5): leaf types, pure admission decisions and
/// the generated-schema owner. `admin_plane` is its handler (Phase 5).
pub use airk_endpoint::admin_contract;
/// The service's `/admin` plane (split Phase 5): launch codes, sessions,
/// embedded assets and admin dispatch.
pub(crate) mod admin_plane;
pub(crate) mod catalog_admission;
pub(crate) mod catalog_authority;
pub(crate) mod catalog_coordinator;
pub(crate) mod catalog_grant_draft;
mod catalog_host_context;
mod catalog_index_setup;
mod catalog_index_setup_control;
mod catalog_source_change_control;
mod catalog_split_control;
#[cfg(test)]
pub(crate) use airk_host::catalog_test_fixture;
pub mod combined_frontend;
pub mod embedding_client;
pub mod embedding_pool_bootstrap;
pub mod embedding_pool_client;
mod embedding_pool_process;
pub use airk_host::embedding_pool_protocol;
mod http_backend;
pub use airk_endpoint::http_endpoint;
mod http_gateway;
#[cfg(feature = "local-inference")]
pub(crate) mod inference_completion_ledger;
#[cfg(feature = "local-inference")]
pub(crate) mod inference_engine;
/// The local transport lives in `airk-ipc` (split G6); `ipc` keeps the old path.
pub use airk_ipc as ipc;
/// Compatibility path (split G1): the transport moved to `ipc::endpoint`.
pub use ipc::endpoint as local_ipc;
mod native_admin_connections;
pub(crate) use airk_host::read_attempt;
mod reference_refresh_control;
mod registry_console;
/// Private role admission (split plan §5.1, step 3.0).
pub mod role_admission;
mod runtime_metadata;
pub(crate) mod service_inference;
pub use airk_host::runtime_pair;
pub mod service_configuration;
pub mod service_frontend;
/// Phase 7: the private `setup` and `service-login` roles (I4 writer).
pub mod setup_role;
/// U89: the install record's locations for an installed service.
pub(crate) use airk_host::service_locations;
pub use airk_host::service_logs;
/// Compatibility path (split G1): the frame codec moved to `ipc::frame`.
pub use ipc::frame as service_ipc;
mod service_management;
pub use airk_host::service_registration;
pub mod service_runtime;
pub mod standalone_setup;
/// The CLI tool call path shared with M3 (split step 3a.2b).
pub mod tool_call;
/// M2 tool catalog documents (split step 3a.1).
pub mod tool_catalog;
mod worker_host;
mod worker_scope;
mod worker_transport;

pub use airk_host::bundled_assets;
pub mod derived_worker;
pub mod project_client;
pub use airk_host::project_protocol;
mod project_supervisor;
pub mod project_worker;
mod project_worker_split;

pub(crate) use airk_host::service_resource_policy;

pub use airk_host::install_files;
pub(crate) use airk_host::install_layout;
mod legacy_processes;
pub mod legacy_startup;
pub mod manual_start;
/// The `airk runtime` dispatcher (split step 2b): above the service it serves.
pub mod runtime_commands;
/// Setup-only HTTP discovery and enrollment handlers (split step 2b).
mod runtime_http_commands;
pub mod runtime_install;
mod runtime_legacy_origin;
pub mod service_manager;

pub(crate) use airk_host::checkpoint_fence;
/// The native upgrade checkpoint packet and fence writers (split step 2b).
pub(crate) use airk_host::checkpoint_packet;
pub(crate) use airk_host::child_job;

pub(crate) use airk_host::native_command;
pub(crate) use airk_host::native_process;
/// Native read result classification shared by clients and tests (split step 2b).
pub(crate) use airk_host::native_read_result;
pub(crate) use airk_host::process_diagnostics;
pub(crate) use airk_host::process_roles;

pub use airk_host::source_protocol;

pub mod source_worker;

mod source_supervisor;
#[cfg(unix)]
pub mod unix_guardian;
pub(crate) use airk_host::user_paths;

pub use airk_host::cli_io;
pub mod client_requests;
pub mod console_client;
/// Client side of the native service control transport (moved from `service_runtime`).
pub(crate) mod control_client;
pub use airk_endpoint::explorer_assistance;
pub(crate) use airk_host::request_journal;
pub mod session;
pub use airk_host::session_protocol;
pub mod unified_mcp;
pub mod universal_launcher;
pub mod usage_client;

/// The private project-owner process role, wired with the service hooks
/// (see `project_worker::Hooks`).
pub async fn run_project_worker() -> anyhow::Result<()> {
    project_worker::run(runtime_metadata::project_worker_hooks()).await
}
