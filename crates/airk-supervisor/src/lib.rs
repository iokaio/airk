// SPDX-License-Identifier: Apache-2.0
//! The supervisor part of the former `airk-cli` crate (docs/simplify/airk-cli-split.md).
extern crate self as airk_supervisor;
pub(crate) use airk_catalog_control::catalog_admission;
pub(crate) use airk_catalog_control::catalog_authority;
pub(crate) use airk_catalog_control::catalog_coordinator;
pub(crate) use airk_host::project_protocol;
pub(crate) use airk_host::source_protocol;
pub(crate) use airk_inference::service_inference;
pub use airk_ipc as ipc;
pub use ipc::endpoint as local_ipc;
pub use ipc::frame as service_ipc;
pub mod derived_worker;
pub mod project_supervisor;
pub mod project_worker;
pub mod project_worker_split;
pub mod runtime_metadata;
pub mod source_supervisor;
pub mod source_worker;
pub mod worker_host;
pub mod worker_scope;
pub mod worker_transport;
