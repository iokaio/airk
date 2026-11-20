// SPDX-License-Identifier: Apache-2.0
//! Identity of the effective Context graph, optimizer and complete live native placement.

use super::{Observation, PlacementObservation};
use airk_core::{
    arbiter::{
        AssetRole, ModelChoice, Profile,
        device::{DevicePrecision, GpuBackend},
    },
    identity::{Sha256Digest, framed_digest},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Concrete Level3 session options. Select the recipe before creating the session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundedContextOptimizer {
    Level3,
    ProtectedProjections,
}

impl BoundedContextOptimizer {
    #[cfg(all(
        target_arch = "x86_64",
        any(windows, all(target_os = "linux", target_env = "gnu"))
    ))]
    pub(in crate::arbiter::runtime) fn from_profile(profile: &Profile) -> Result<Self> {
        let device = profile
            .device_execution
            .as_ref()
            .context("bounded CUDA device descriptor missing")?;
        ensure!(
            device.precision == DevicePrecision::Float16 && !device.tf32,
            "bounded CUDA Context requires FP16 with TF32 disabled"
        );
        for candidate in [Self::Level3, Self::ProtectedProjections] {
            if candidate.provider_options(profile.model)? == device.provider_options {
                return Ok(candidate);
            }
        }
        anyhow::bail!("unknown measured CUDA Context optimizer")
    }

    #[cfg(all(
        target_arch = "x86_64",
        any(windows, all(target_os = "linux", target_env = "gnu"))
    ))]
    pub(in crate::arbiter::runtime) fn verify_execution(
        self,
        profile: &Profile,
        placement: &PlacementObservation,
        observed: &Observation,
        expected: Sha256Digest,
    ) -> Result<()> {
        #[cfg(windows)]
        let maximum_payload = match profile.model {
            ModelChoice::ContextQwenCompact => 9_443,
            ModelChoice::ContextQwenLarge => 12_067,
            _ => anyhow::bail!("Windows bounded CPU payload requires a Context model"),
        };
        ensure!(
            observed.bounded_context_execution_policy(profile, self, placement)? == expected,
            "qualified bounded Context final graph, optimizer or placement differs"
        );
        #[cfg(windows)]
        {
            observed.validate_bounded_fp16_cpu_values(maximum_payload)
        }
        #[cfg(target_os = "linux")]
        super::super::linux::check_product_cpu_bounds(profile, observed)
    }

    fn name(self) -> &'static [u8] {
        match self {
            Self::Level3 => b"ort-level3",
            Self::ProtectedProjections => b"ort-level3-protected-projections",
        }
    }

    fn overrides(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Level3 => &[],
            Self::ProtectedProjections => &[
                (
                    "optimization.constant_folding_max_output_size_in_bytes",
                    "1048576",
                ),
                ("optimization.enable_cast_chain_elimination", "0"),
                (
                    "optimization.disable_specified_optimizers",
                    "FuseFp16InitializerToFp32NodeTransformer",
                ),
            ],
        }
    }

    pub fn configure(
        self,
        mut builder: ort::session::builder::SessionBuilder,
    ) -> Result<ort::session::builder::SessionBuilder> {
        for &(key, value) in self.overrides() {
            builder = builder.with_config_entry(key, value)?;
        }
        Ok(builder.with_config_entry("session.disable_cpu_ep_fallback", "0")?)
    }

    /// Identifies the configured native options, independently of the eventual graph assignment.
    pub fn provider_options(self, model: ModelChoice) -> Result<Sha256Digest> {
        Ok(framed_digest(b"airk.native.cuda.bounded-context-provider-options/1", &[
            host_scope()?, model_scope(model)?, self.name(), &serde_json::to_vec(self.overrides())?,
            b"cuda-required;cpu-control-values-only;tf32=0;cuda-graph=0;arena=declared-cold-peak;flash-attention;ort-level3;host-fit-threads;sequential;memory-pattern=0;spinning=0;ORT_CUDA_GEMM_OPTIONS=2;original-deadline",
        ]))
    }

    /// Identifies the code implementing the effective mixed-precision recipe.
    pub fn math_policy(self, model: ModelChoice) -> Result<Sha256Digest> {
        Ok(framed_digest(b"airk.native.cuda.bounded-context-math/1", &[
            host_scope()?, model_scope(model)?, self.name(), self.provider_options(model)?.as_bytes(),
            super::super::fp16_math_policy_digest(model)?.as_bytes(),
            include_bytes!("bounded_execution.rs"), include_bytes!("cpu_values.rs"),
            include_bytes!("../graph_metadata.rs"), include_bytes!("../placement.rs"), platform_loader_identity().as_bytes(),
            b"exact-authenticated-model-and-external-weight-precision;ieee754;f32-scores;tf32=0;no-runtime-casts-outside-measured-final-graph",
        ]))
    }
}

fn platform_loader_identity() -> Sha256Digest {
    #[cfg(all(windows, target_arch = "x86_64"))]
    let sources: &[&[u8]] = &[include_bytes!("../windows.rs")];
    #[cfg(all(target_os = "linux", target_arch = "x86_64", target_env = "gnu"))]
    let sources: &[&[u8]] = &[
        include_bytes!("../linux.rs"),
        include_bytes!("../linux/driver.rs"),
        include_bytes!("../linux/held.rs"),
        include_bytes!("../linux/mappings.rs"),
        include_bytes!("../linux/product.rs"),
    ];
    #[cfg(not(any(
        all(windows, target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64", target_env = "gnu")
    )))]
    let sources: &[&[u8]] = &[];
    framed_digest(b"airk.native.cuda.bounded-context-loader/1", sources)
}

fn host_scope() -> Result<&'static [u8]> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        Ok(b"windows-x86_64-cuda")
    } else if cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) {
        Ok(b"linux-x86_64-gnu-cuda")
    } else {
        anyhow::bail!("bounded CUDA Context execution unavailable on this target")
    }
}

fn model_scope(model: ModelChoice) -> Result<&'static [u8]> {
    match model {
        ModelChoice::ContextQwenCompact => Ok(b"qwen3-1.7b"),
        ModelChoice::ContextQwenLarge => Ok(b"qwen3-4b"),
        _ => anyhow::bail!("bounded CUDA execution requires a Context model"),
    }
}

/// Hash a complete multiset of typed rows, retaining duplicate rows and every coordinate.
/// Sorting row hashes makes callback ordering irrelevant after the native semantic closure check.
fn roster<T: Serialize>(domain: &[u8], rows: &[T]) -> Result<Sha256Digest> {
    let mut hashes = Vec::with_capacity(rows.len());
    for row in rows {
        let mut value = serde_json::to_value(row)?;
        value.sort_all_objects();
        hashes.push(<[u8; 32]>::from(Sha256::digest(serde_json::to_vec(
            &value,
        )?)));
    }
    hashes.sort_unstable();
    let fields: Vec<&[u8]> = hashes.iter().map(|hash| hash.as_slice()).collect();
    Ok(framed_digest(domain, &fields))
}

fn product_cpu_origins(rows: &[super::Row]) -> Result<()> {
    use super::Row;
    use std::collections::BTreeSet;
    let cpu: BTreeSet<_> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Node {
                graph, node, ep, ..
            } if ep == "CPUExecutionProvider" => Some((*graph, *node)),
            _ => None,
        })
        .collect();
    let known: BTreeSet<_> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Value {
                graph,
                value,
                provenance,
                ..
            } if matches!(
                provenance.as_str(),
                "graph_input" | "node_output" | "immutable_session_initializer"
            ) =>
            {
                Some((*graph, *value))
            }
            _ => None,
        })
        .collect();
    for row in rows {
        if let Row::Edge {
            graph,
            node: Some(node),
            value: Some(value),
            ..
        } = row
            && cpu.contains(&(*graph, *node))
        {
            ensure!(
                known.contains(&(*graph, *value)),
                "product CPU control value origin unresolved or overridable"
            );
        }
    }
    Ok(())
}

impl Observation {
    /// Validate observed CPU control tensor shapes against the target's independently reviewed aggregate cap.
    pub fn validate_bounded_fp16_cpu_values(&self, maximum_payload: u64) -> Result<()> {
        ensure!(
            maximum_payload > 0,
            "missing measured CPU tensor payload cap"
        );
        super::cpu_values::validate_cpu_values(
            &self.rows,
            super::cpu_values::CpuValueBounds::fp16(maximum_payload),
        )
    }

    /// Compute a measurement identity only after the effective final graph and placement
    /// agree. Returning this digest grants no native eligibility or release qualification.
    pub fn bounded_context_execution_policy(
        &self,
        profile: &Profile,
        recipe: BoundedContextOptimizer,
        placement: &PlacementObservation,
    ) -> Result<Sha256Digest> {
        self.validate_against(placement)?;
        product_cpu_origins(&self.rows)?;
        let providers: std::collections::BTreeSet<_> = placement
            .nodes
            .iter()
            .map(|node| node.ep.as_str())
            .collect();
        ensure!(
            providers
                == std::collections::BTreeSet::from([
                    "CPUExecutionProvider",
                    "CUDAExecutionProvider"
                ]),
            "measured bounded Context requires both CPU and CUDA and no third provider"
        );
        let device = profile
            .device_execution
            .as_ref()
            .context("native CUDA descriptor missing")?;
        ensure!(
            device.backend == GpuBackend::Cuda
                && device.precision == DevicePrecision::Float16
                && !device.tf32,
            "bounded Context placement requires FP16 CUDA with TF32 disabled"
        );
        let model = profile.model;
        let mut graphs = profile
            .assets
            .iter()
            .filter(|asset| asset.role == AssetRole::Model);
        let graph = graphs.next().context("authenticated model graph absent")?;
        ensure!(
            graphs.next().is_none() && graph.sha256 == device.graph,
            "bounded Context model graph differs"
        );
        let weights: Vec<_> = profile
            .assets
            .iter()
            .filter(|a| a.role == AssetRole::ExternalWeights)
            .collect();
        ensure!(
            !weights.is_empty(),
            "bounded Context external weight closure absent"
        );
        let assets = roster(b"airk.native.cuda.bounded-context-weights/1", &weights)?;
        let nodes = roster(
            b"airk.native.cuda.bounded-context-placement/1",
            &placement.nodes,
        )?;
        let rows = roster(
            b"airk.native.cuda.bounded-context-final-graph/1",
            &self.rows,
        )?;
        Ok(framed_digest(
            b"airk.native.cuda.bounded-context-execution/1",
            &[
                host_scope()?,
                model_scope(model)?,
                &serde_json::to_vec(graph)?,
                assets.as_bytes(),
                device.runtime_build.as_bytes(),
                device.compatibility_policy.as_bytes(),
                &serde_json::to_vec(&device.host_fit)?,
                recipe.provider_options(model)?.as_bytes(),
                recipe.math_policy(model)?.as_bytes(),
                self.schema.as_bytes(),
                placement.scope.as_bytes(),
                &serde_json::to_vec(&self.reported_counts)?,
                nodes.as_bytes(),
                rows.as_bytes(),
            ],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(
        all(windows, target_arch = "x86_64"),
        all(target_os = "linux", target_env = "gnu", target_arch = "x86_64")
    ))]
    #[test]
    fn measured_options_separate_model_and_optimizer_without_profile_promotion() -> Result<()> {
        use BoundedContextOptimizer::{Level3, ProtectedProjections};
        use ModelChoice::{ContextQwenCompact, ContextQwenLarge, FocusQwenReranker06B};
        let compact = ProtectedProjections.provider_options(ContextQwenCompact)?;
        assert_ne!(compact, Level3.provider_options(ContextQwenCompact)?);
        assert_ne!(
            compact,
            ProtectedProjections.provider_options(ContextQwenLarge)?
        );
        assert_ne!(
            compact,
            super::super::super::fp16_provider_options_digest(ContextQwenCompact)?
        );
        assert!(Level3.provider_options(FocusQwenReranker06B).is_err());
        Ok(())
    }

    #[test]
    fn product_cpu_rejects_unresolved_or_overridable_small_scalar() -> Result<()> {
        use crate::arbiter::runtime::cuda::graph_metadata::{Row, cpu_values};
        let node = |node, ep: &str| Row::Node {
            graph: 0,
            node,
            name: format!("node-{node}"),
            domain: String::new(),
            op: "Identity".into(),
            ep: ep.into(),
            version: 1,
        };
        for (origin, admitted) in [
            ("graph_input", true),
            ("node_output", true),
            ("immutable_session_initializer", true),
            ("unresolved_outer_or_internal", false),
            ("overridable_initializer_unread", false),
        ] {
            let rows = vec![
                Row::Graph {
                    graph: 0,
                    parent_graph: None,
                    parent_node: None,
                    attribute: String::new(),
                    outer_bindings: "not_applicable".into(),
                },
                node(0, "CPUExecutionProvider"),
                node(1, "CUDAExecutionProvider"),
                Row::Value {
                    graph: 0,
                    value: 0,
                    name: "scalar".into(),
                    element_type: Some(7),
                    rank: Some(0),
                    type_state: "tensor".into(),
                    provenance: origin.into(),
                    producer: None,
                },
                Row::Edge {
                    graph: 0,
                    node: Some(0),
                    value: Some(0),
                    role: 0,
                    slot: 0,
                },
            ];
            cpu_values::validate_cpu_values(&rows, cpu_values::CpuValueBounds::fp16(8))?;
            assert_eq!(product_cpu_origins(&rows).is_ok(), admitted);
        }
        Ok(())
    }

    #[test]
    fn measured_roster_preserves_multiplicity_and_all_coordinates() -> Result<()> {
        let a = serde_json::json!({"node": 3,"ep":"CUDAExecutionProvider"});
        let b = serde_json::json!({"node": 7,"ep":"CPUExecutionProvider"});
        let ordered = roster(b"placement", &[&a, &b])?;
        assert_eq!(
            ordered.to_string(),
            "df00dbace2bf4932fc7b766f5e86ac8b741e7d5b76a163424ac5f242c7c66ece"
        );
        assert_eq!(ordered, roster(b"placement", &[&b, &a])?);
        assert_ne!(ordered, roster(b"placement", &[&b, &a, &b])?);
        assert_ne!(ordered, roster(b"other-domain", &[&a, &b])?);
        assert_ne!(
            ordered,
            roster(
                b"placement",
                &[
                    &a,
                    &serde_json::json!({"node": 7, "ep":"CUDAExecutionProvider"})
                ]
            )?
        );
        Ok(())
    }
}
