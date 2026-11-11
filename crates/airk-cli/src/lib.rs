// SPDX-License-Identifier: Apache-2.0
//! Native adapter primitives shared by service/client executable modes.
extern crate self as airk_cli;
/// I3 admin HTTP contract (split C5): leaf types, pure admission decisions and
/// the generated-schema owner. No handler uses it before Phase 5.
pub mod admin_contract;
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
mod catalog_test_fixture;
pub mod combined_frontend;
pub mod embedding_client;
pub mod embedding_pool_bootstrap;
pub mod embedding_pool_client;
mod embedding_pool_process;
pub mod embedding_pool_protocol;
mod http_backend;
pub mod http_endpoint;
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
pub(crate) mod read_attempt;
mod reference_refresh_control;
pub(crate) mod service_inference;
/// Leaf Windows file-ACL check shared with the Explorer Assistance forwarder,
/// which must not depend on service or IPC protocol code.
#[cfg(windows)]
pub(crate) mod owner_acl {
    pub(crate) use crate::local_ipc::require_owner_private;
}
mod runtime_metadata;
pub mod runtime_pair;
pub mod service_configuration;
pub mod service_frontend;
/// Compatibility path (split G1): the frame codec moved to `ipc::frame`.
pub use ipc::frame as service_ipc;
pub mod service_logs;
mod service_management;
pub mod service_registration;
pub mod service_runtime;
pub mod standalone_setup;
mod worker_host;
mod worker_scope;
mod worker_transport;

pub mod bundled_assets;
pub mod derived_worker;
pub mod project_client;
pub mod project_protocol;
mod project_supervisor;
pub mod project_worker;
mod project_worker_split;

mod service_resource_policy;

pub mod install_files;
mod install_layout;
mod legacy_processes;
pub mod legacy_startup;
pub mod runtime_install;
mod runtime_legacy_origin;
pub mod service_manager;

mod checkpoint_fence;
mod child_job;

pub mod client_owner;
mod native_command;
mod native_process;
mod process_diagnostics;
mod process_roles;

pub mod source_protocol;

pub mod source_worker;

mod source_supervisor;
#[cfg(unix)]
pub mod unix_guardian;
mod user_paths;

pub mod cli_io;
pub mod client_requests;
pub mod console_client;
pub mod explorer_assistance;
pub mod session;
pub mod session_protocol;
pub mod unified_mcp;
pub mod universal_launcher;
pub mod usage_client;

/// The private project-owner process role, wired with the service hooks
/// (see `project_worker::Hooks`).
pub async fn run_project_worker() -> anyhow::Result<()> {
    project_worker::run(runtime_metadata::project_worker_hooks()).await
}
