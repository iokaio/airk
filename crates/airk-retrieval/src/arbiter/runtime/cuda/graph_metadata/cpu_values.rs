// SPDX-License-Identifier: Apache-2.0
//! Limits on CPU-incident control tensors in a fully observed CUDA Context graph.
use super::Row as GraphMetadataRow;
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

const CPU: &str = "CPUExecutionProvider";
const CUDA: &str = "CUDAExecutionProvider";

#[derive(Clone, Copy)]
pub(in crate::arbiter::runtime) struct CpuValueBounds {
    pub(in crate::arbiter::runtime) maximum_payload: u64,
    pub(in crate::arbiter::runtime) maximum_vector_elements: i64,
    pub(in crate::arbiter::runtime) maximum_value_bytes: u64,
    pub(in crate::arbiter::runtime) allow_int32: bool,
}

impl CpuValueBounds {
    #[cfg(all(windows, target_arch = "x86_64", feature = "test-support"))]
    pub(in crate::arbiter::runtime) const COMPACT_FP16: Self = Self::fp16(9_443);

    pub(in crate::arbiter::runtime) const fn fp16(maximum_payload: u64) -> Self {
        Self {
            maximum_payload,
            maximum_vector_elements: 4,
            maximum_value_bytes: 32,
            allow_int32: true,
        }
    }

    #[cfg(all(windows, target_arch = "x86_64", feature = "test-support"))]
    pub(in crate::arbiter::runtime) const fn fp32(maximum_payload: u64) -> Self {
        Self {
            maximum_payload,
            maximum_vector_elements: 5,
            maximum_value_bytes: 40,
            allow_int32: false,
        }
    }
}

pub(in crate::arbiter::runtime) fn validate_cpu_values(
    rows: &[GraphMetadataRow],
    bounds: CpuValueBounds,
) -> Result<()> {
    use GraphMetadataRow::*;
    let mut root_count = 0;
    let mut cpu_nodes = BTreeSet::new();
    let mut cuda_nodes = 0;
    let mut tensors = BTreeMap::new();
    let mut dimensions = BTreeMap::new();
    let mut incident_values = BTreeSet::new();
    for row in rows {
        match row {
            Graph {
                graph,
                parent_graph,
                parent_node,
                outer_bindings,
                ..
            } => {
                ensure!(
                    *graph == 0
                        && parent_graph.is_none()
                        && parent_node.is_none()
                        && outer_bindings == "not_applicable",
                    "bounded hybrid requires one fully resolved root graph"
                );
                root_count += 1;
            }
            Node { node, ep, .. } if ep == CPU => {
                cpu_nodes.insert(*node);
            }
            Node { ep, .. } => {
                ensure!(ep == CUDA, "unexpected bounded hybrid execution provider");
                cuda_nodes += 1;
            }
            Value {
                value,
                element_type,
                rank,
                type_state,
                ..
            } => {
                tensors.insert(*value, (*element_type, *rank, type_state.as_str()));
            }
            Dimension {
                value,
                index: 0,
                number: Some(number),
                symbol: None,
                ..
            } => {
                dimensions.insert(*value, *number);
            }
            Edge {
                node: Some(node),
                value,
                role,
                ..
            } if cpu_nodes.contains(node) => {
                // ORT's kernel input profile omits implicit slots. The frozen
                // Context graphs have none on CPU, so require that exact scope.
                ensure!(*role != 1, "CPU implicit input cannot be profiled");
                incident_values.extend(*value);
            }
            _ => {}
        }
    }
    ensure!(
        root_count == 1 && !cpu_nodes.is_empty() && cuda_nodes > 0 && !incident_values.is_empty(),
        "bounded hybrid requires an observed CPU and CUDA root graph"
    );
    let mut total_bytes = 0_u64;
    for value in incident_values {
        let &(element, rank, state) = tensors.get(&value).context("missing CPU tensor value")?;
        let width = match (element, state) {
            (Some(6), "tensor") if bounds.allow_int32 => 4_u64,
            (Some(7), "tensor") => 8_u64,
            (Some(9), "tensor") => 1,
            _ => anyhow::bail!("CPU tensor element is outside the reviewed control types"),
        };
        let elements = match rank {
            Some(0) => 1,
            Some(1) => match dimensions.get(&value) {
                Some(elements) if (0..=bounds.maximum_vector_elements).contains(elements) => {
                    *elements as u64
                }
                _ => anyhow::bail!(
                    "CPU vector requires a fixed length at most {}",
                    bounds.maximum_vector_elements
                ),
            },
            _ => anyhow::bail!("CPU tensor must be scalar or a fixed vector"),
        };
        let bytes = elements * width;
        ensure!(
            bytes <= bounds.maximum_value_bytes,
            "CPU tensor exceeds observed byte bound"
        );
        total_bytes += bytes;
    }
    ensure!(
        total_bytes <= bounds.maximum_payload,
        "distinct CPU tensor payload exceeds observed aggregate bound"
    );
    Ok(())
}
