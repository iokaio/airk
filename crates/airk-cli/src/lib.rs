// SPDX-License-Identifier: Apache-2.0
//! Native adapter primitives shared by service/client executable modes.
pub mod embedding_client;
pub mod local_ipc;
pub mod runtime_pair;
pub mod service_ipc;
mod service_management;
pub mod service_runtime;

pub mod bundled_assets;
