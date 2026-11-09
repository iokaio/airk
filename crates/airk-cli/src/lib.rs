// SPDX-License-Identifier: Apache-2.0
//! Native adapter primitives shared by service/client executable modes.
extern crate self as airk_cli;
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
pub mod local_ipc;
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
pub mod service_ipc;
pub mod service_logs;
mod service_management;
pub mod service_registration;
pub mod service_runtime;
pub mod standalone_setup;

pub mod bundled_assets;
pub mod derived_worker;
pub mod project_client;
pub mod project_protocol;
mod project_supervisor;
pub mod project_worker;
mod project_worker_split;

mod service_resource_policy;

mod legacy_processes;
pub mod legacy_startup;
pub mod runtime_install;
mod runtime_legacy_authority;
mod runtime_legacy_origin;
pub mod service_manager;

mod child_job;

pub mod client_owner;
mod native_process;
mod process_diagnostics;
mod process_roles;

pub mod source_protocol;

pub mod source_worker;

mod source_supervisor;
#[cfg(unix)]
pub mod unix_guardian;
mod user_paths;

pub mod client_requests;
pub mod console_client;
pub mod explorer_assistance;
pub mod unified_mcp;
pub mod universal_launcher;
pub mod usage_client;
