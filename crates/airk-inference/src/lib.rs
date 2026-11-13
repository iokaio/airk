// SPDX-License-Identifier: Apache-2.0
//! The inference part of the former `airk-cli` crate (docs/simplify/airk-cli-split.md).
extern crate self as airk_inference;
pub(crate) use airk_host::embedding_pool_protocol;
pub(crate) use airk_host::runtime_pair;
pub use airk_ipc as ipc;
pub use ipc::endpoint as local_ipc;
pub use ipc::frame as service_ipc;
pub mod embedding_client;
pub mod embedding_pool_bootstrap;
pub mod embedding_pool_client;
pub mod embedding_pool_process;
#[cfg(feature = "local-inference")]
pub mod inference_completion_ledger;
#[cfg(feature = "local-inference")]
pub mod inference_engine;
pub mod service_inference;
#[cfg(unix)]
pub mod unix_guardian;
