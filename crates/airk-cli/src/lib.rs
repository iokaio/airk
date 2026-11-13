// SPDX-License-Identifier: Apache-2.0
//! Native adapter primitives shared by service/client executable modes.
extern crate self as airk_cli;
/// I3 admin HTTP contract (split C5): leaf types, pure admission decisions and
/// the generated-schema owner. `admin_plane` is its handler (Phase 5).
pub use airk_endpoint::admin_contract;
/// The service's `/admin` plane (split Phase 5): launch codes, sessions,
/// embedded assets and admin dispatch.
pub(crate) mod admin_plane;
pub(crate) use airk_catalog_control::catalog_admission;
pub(crate) use airk_catalog_control::catalog_authority;
pub(crate) use airk_catalog_control::catalog_coordinator;
pub(crate) use airk_catalog_control::catalog_grant_draft;
pub(crate) use airk_catalog_control::catalog_host_context;
pub(crate) use airk_catalog_control::catalog_index_setup_control;
pub(crate) use airk_catalog_control::catalog_source_change_control;
pub(crate) use airk_catalog_control::catalog_split_control;
pub use airk_frontend::combined_frontend;
#[cfg(test)]
pub(crate) use airk_host::catalog_test_fixture;
pub use airk_host::embedding_pool_protocol;
pub use airk_inference::embedding_client;
pub use airk_inference::embedding_pool_bootstrap;
pub use airk_inference::embedding_pool_client;
mod http_backend;
pub use airk_endpoint::http_endpoint;
mod http_gateway;
// Only the gateway's inference-owner tests use the engine through this path.
#[cfg(all(test, feature = "local-inference"))]
pub(crate) use airk_inference::inference_engine;
/// The local transport lives in `airk-ipc` (split G6); `ipc` keeps the old path.
pub use airk_ipc as ipc;
/// Compatibility path (split G1): the transport moved to `ipc::endpoint`.
pub use ipc::endpoint as local_ipc;
mod native_admin_connections;
pub(crate) use airk_catalog_control::reference_refresh_control;
pub(crate) use airk_catalog_control::registry_console;
/// Private role admission (split plan §5.1, step 3.0).
pub mod role_admission;
pub use airk_host::runtime_pair;
pub(crate) use airk_supervisor::runtime_metadata;
pub mod service_configuration;
pub use airk_frontend::service_frontend;
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
pub use airk_frontend::tool_call;
/// M2 tool catalog documents (split step 3a.1).
pub use airk_frontend::tool_catalog;

pub use airk_frontend::project_client;
pub use airk_host::bundled_assets;
pub use airk_host::project_protocol;
pub use airk_supervisor::derived_worker;
pub(crate) use airk_supervisor::project_supervisor;
pub use airk_supervisor::project_worker;

pub(crate) use airk_host::service_resource_policy;

pub use airk_host::install_files;
pub(crate) use airk_host::install_layout;
pub(crate) use airk_installer::legacy_processes;
pub use airk_installer::legacy_startup;
pub use airk_installer::manual_start;
/// The `airk runtime` dispatcher (split step 2b): above the service it serves.
pub mod runtime_commands;
/// Setup-only HTTP discovery and enrollment handlers (split step 2b).
mod runtime_http_commands;
pub use airk_installer::runtime_install;
pub use airk_installer::service_manager;

pub(crate) use airk_host::checkpoint_fence;
/// The native upgrade checkpoint packet and fence writers (split step 2b).
pub(crate) use airk_host::checkpoint_packet;

pub(crate) use airk_host::native_process;
/// Native read result classification shared by clients and tests (split step 2b).
pub(crate) use airk_host::process_roles;

pub use airk_host::source_protocol;

pub use airk_supervisor::source_worker;

pub(crate) use airk_host::user_paths;
#[cfg(unix)]
pub use airk_inference::unix_guardian;
pub(crate) use airk_supervisor::source_supervisor;

pub use airk_endpoint::explorer_assistance;
pub use airk_frontend::client_requests;
pub use airk_frontend::console_client;
pub use airk_frontend::session;
pub use airk_frontend::unified_mcp;
pub use airk_frontend::usage_client;
pub use airk_host::cli_io;
pub(crate) use airk_host::request_journal;
pub use airk_host::session_protocol;
/// Client side of the native service control transport (moved from `service_runtime`).
pub(crate) use airk_installer::control_client;
pub use airk_installer::universal_launcher;

/// The private project-owner process role, wired with the service hooks
/// (see `project_worker::Hooks`).
pub async fn run_project_worker() -> anyhow::Result<()> {
    project_worker::run(runtime_metadata::project_worker_hooks()).await
}
