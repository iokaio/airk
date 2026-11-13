// SPDX-License-Identifier: Apache-2.0
//! The os part of the former `airk-cli` crate (docs/simplify/airk-cli-split.md).
extern crate self as airk_host;
pub use airk_ipc as ipc;
pub use ipc::endpoint as local_ipc;
pub use ipc::frame as service_ipc;
pub mod bundled_assets;
#[cfg(any(test, feature = "test-support"))]
pub mod catalog_test_fixture;
pub mod checkpoint_fence;
/// The native upgrade checkpoint packet and fence writers (split step 2b).
pub mod checkpoint_packet;
pub mod child_job;
pub mod cli_io;
pub mod embedding_pool_protocol;
pub mod install_files;
pub mod install_layout;
pub mod native_command;
pub mod native_process;
/// Native read result classification shared by clients and tests (split step 2b).
pub mod native_read_result;
pub mod process_diagnostics;
pub mod process_roles;
pub mod project_protocol;
pub mod read_attempt;
pub mod request_journal;
pub mod runtime_pair;
pub mod service_configuration;
/// U89: the install record's locations for an installed service.
pub mod service_locations;
pub mod service_logs;
pub mod service_registration;
pub mod service_resource_policy;
pub mod session_protocol;
pub mod source_protocol;
pub mod user_paths;
