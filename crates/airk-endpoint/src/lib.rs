// SPDX-License-Identifier: Apache-2.0
//! The endpoint part of the former `airk-cli` crate (docs/simplify/airk-cli-split.md).
extern crate self as airk_endpoint;
pub use airk_ipc as ipc;
pub use ipc::endpoint as local_ipc;
#[cfg(windows)]
pub mod owner_acl {
    pub use crate::local_ipc::require_owner_private;
}
/// I3 admin HTTP contract (split C5): leaf types, pure admission decisions and
/// the generated-schema owner. `admin_plane` is its handler (Phase 5).
pub mod admin_contract;
pub mod explorer_assistance;
pub mod http_endpoint;
