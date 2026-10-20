// SPDX-License-Identifier: Apache-2.0
//! Execution settings do not participate in extraction or embedding identity.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    #[default]
    Automatic,
    Interactive,
    Throughput,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub mode: ExecutionMode,
    pub logical_cpus: usize,
    pub detected_memory_bytes: Option<u64>,
    pub requested_extraction_workers: usize,
    pub extraction_workers: usize,
    pub requested_inference_workers: usize,
    pub inference_workers: usize,
    pub working_memory_bytes: usize,
    pub resource_slots: usize,
    pub coordination: String,
}
