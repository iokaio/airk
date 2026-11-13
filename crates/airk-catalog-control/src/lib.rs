// SPDX-License-Identifier: Apache-2.0
//! The catalog part of the former `airk-cli` crate (docs/simplify/airk-cli-split.md).
extern crate self as airk_catalog_control;
pub use airk_ipc as ipc;
pub use ipc::endpoint as local_ipc;
pub use ipc::frame as service_ipc;
pub mod catalog_admission;
pub mod catalog_authority;
pub mod catalog_coordinator;
pub mod catalog_grant_draft;
pub mod catalog_host_context;
pub mod catalog_index_setup;
pub mod catalog_index_setup_control;
pub mod catalog_source_change_control;
pub mod catalog_split_control;
pub mod reference_refresh_control;
pub mod registry_console;
