// SPDX-License-Identifier: Apache-2.0
//! Native adapter primitives shared by service/client executable modes.
pub mod embedding_client;
pub mod embedding_pool_client;
mod embedding_pool_process;
pub mod embedding_pool_protocol;
pub mod local_ipc;
pub mod runtime_pair;
pub mod service_ipc;
mod service_management;
pub mod service_runtime;

pub mod bundled_assets;
pub mod project_client;
pub mod project_protocol;
mod project_supervisor;
pub mod project_worker;

mod service_resource_policy;

pub mod runtime_install;
pub mod service_manager;

mod child_job;

mod native_process;

pub mod source_protocol;

pub mod source_worker;

mod source_supervisor;
mod user_paths;
