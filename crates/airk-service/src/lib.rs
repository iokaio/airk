// SPDX-License-Identifier: Apache-2.0
//! The service part of the former `airk-cli` crate (docs/simplify/airk-cli-split.md).
extern crate self as airk_service;
pub(crate) use airk_catalog_control::catalog_admission;
pub(crate) use airk_catalog_control::catalog_authority;
#[cfg(test)]
pub(crate) use airk_catalog_control::catalog_coordinator;
pub(crate) use airk_frontend::project_client;
pub use airk_ipc as ipc;
pub(crate) use airk_supervisor::project_supervisor;
pub use ipc::endpoint as local_ipc;
pub use ipc::frame as service_ipc;
pub mod native_admin_connections;
pub mod service_management;
pub mod service_runtime;
/// Phase 7: the private `setup` and `service-login` roles (I4 writer).
pub mod setup_role;
