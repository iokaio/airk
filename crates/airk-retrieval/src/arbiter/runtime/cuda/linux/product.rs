// SPDX-License-Identifier: Apache-2.0
//! GNU product CPU shapes come from reviewed Linux Context graph observations.
use super::super::graph_metadata::{Observation, cpu_values};
use airk_core::{
    arbiter::{AssetRole, ModelChoice, Profile},
    identity::Sha256Digest,
};
use anyhow::{Context, Result, ensure};

struct ReviewedGraph {
    model: ModelChoice,
    graph: Sha256Digest,
    limits: cpu_values::CpuValueBounds,
}

// A release row requires independently observed Linux sizes for this model and graph.
const REVIEWED: &[ReviewedGraph] = &[];

fn limits(
    rows: &[ReviewedGraph],
    model: ModelChoice,
    graph: Sha256Digest,
) -> Result<cpu_values::CpuValueBounds> {
    let mut found = rows
        .iter()
        .filter(|row| row.model == model && row.graph == graph);
    let row = found
        .next()
        .context("no reviewed Linux CPU bounds for this Context graph")?;
    ensure!(
        found.next().is_none(),
        "ambiguous Linux Context graph limits"
    );
    Ok(row.limits)
}

pub(in super::super) fn check_product_cpu_bounds(
    profile: &Profile,
    observed: &Observation,
) -> Result<()> {
    let mut graphs = profile
        .assets
        .iter()
        .filter(|asset| asset.role == AssetRole::Model);
    let graph = graphs.next().context("Linux Context model graph missing")?;
    ensure!(
        graphs.next().is_none(),
        "multiple Linux Context model graphs"
    );
    check_model_cpu_bounds(profile.model, graph.sha256, observed)
}

pub(in super::super) fn check_model_cpu_bounds(
    model: ModelChoice,
    graph: Sha256Digest,
    observed: &Observation,
) -> Result<()> {
    cpu_values::validate_cpu_values(&observed.rows, limits(REVIEWED, model, graph)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_cpu_limits_require_the_reviewed_model_and_graph() -> Result<()> {
        let model = ModelChoice::ContextQwenCompact;
        let graph = Sha256Digest::from_bytes([7; 32]);
        let fixture = [ReviewedGraph {
            model,
            graph,
            limits: cpu_values::CpuValueBounds {
                maximum_payload: 48,
                maximum_vector_elements: 2,
                maximum_value_bytes: 16,
                allow_int32: true,
            },
        }];
        assert_eq!(limits(&fixture, model, graph)?.maximum_payload, 48);
        assert!(limits(&fixture, ModelChoice::ContextQwenLarge, graph).is_err());
        assert!(limits(&fixture, model, Sha256Digest::from_bytes([8; 32])).is_err());
        assert!(limits(&[], model, graph).is_err());
        Ok(())
    }
}
