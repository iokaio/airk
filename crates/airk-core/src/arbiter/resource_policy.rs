// SPDX-License-Identifier: Apache-2.0
//! Additive software-v2 resource evidence; declarations never grant execution.
use crate::identity::{Sha256Digest, framed_digest};
use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResourceQuantile {
    #[serde(rename = "nearest_rank_95")]
    NearestRank95,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourcePolicy {
    pub schema: String,
    pub shared_input_tokens: usize,
    pub context_input_tokens: usize,
    pub focus_pair_input_tokens: usize,
    pub maximum_context_candidates: usize,
    pub deadline_ms: u64,
    pub p95_limit_us: u64,
    pub required_warm_samples: usize,
    pub quantile: ResourceQuantile,
    pub stop_on_first_overrun: bool,
}
impl ResourcePolicy {
    pub fn software_v2() -> Self {
        Self {
            schema: "airk.arbiter.resource-policy/2".into(),
            shared_input_tokens: 8192,
            context_input_tokens: 8192,
            focus_pair_input_tokens: 512,
            maximum_context_candidates: 8,
            deadline_ms: 120_000,
            p95_limit_us: 120_000_000,
            required_warm_samples: 20,
            quantile: ResourceQuantile::NearestRank95,
            stop_on_first_overrun: true,
        }
    }
    /// Selected static1024 Focus candidate under the original shared Answer budget.
    pub fn focus_static1024() -> Self {
        Self {
            schema: "airk.arbiter.resource-policy/3".into(),
            focus_pair_input_tokens: 1024,
            ..Self::software_v2()
        }
    }
    pub fn validate(&self) -> Result<()> {
        let selected = matches!(
            (self.schema.as_str(), self.focus_pair_input_tokens),
            ("airk.arbiter.resource-policy/2", 512) | ("airk.arbiter.resource-policy/3", 1024)
        );
        ensure!(
            selected
                && self.shared_input_tokens == 8192
                && self.context_input_tokens == 8192
                && self.maximum_context_candidates == 8
                && self.deadline_ms == 120_000
                && self.p95_limit_us == 120_000_000
                && self.required_warm_samples == 20
                && self.quantile == ResourceQuantile::NearestRank95
                && self.stop_on_first_overrun,
            "resource policy differs from frozen software-v2 limits"
        );
        Ok(())
    }
    /// Exact Rust field order, compact JSON, no newline; the validated schema selects its framing domain.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec(self)?)
    }
    pub fn digest(&self) -> Result<Sha256Digest> {
        Ok(framed_digest(
            self.schema.as_bytes(),
            &[&self.canonical_bytes()?],
        ))
    }
}

/// One separately pinned private file. Paths are inert input descriptions;
/// only the native measurement owner may authenticate and consume them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceFilePin {
    pub path: String,
    pub sha256: Sha256Digest,
}
impl ResourceFilePin {
    pub fn validate(&self) -> Result<()> {
        resource_path(&self.path)
    }
}
fn resource_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && path.len() <= 4096
            && !path.chars().any(char::is_control)
            && std::path::Path::new(path).is_absolute(),
        "absolute bounded native resource input path required"
    );
    Ok(())
}

/// The native owner emits this private request from reviewed original Core
/// profiles and actual assets. It grants no execution or release authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceRunInput {
    pub schema: String,
    pub policy: ResourcePolicy,
    pub policy_digest: Sha256Digest,
    pub source_manifest: ResourceFilePin,
    pub profile: ResourceFilePin,
    pub files: BTreeMap<String, String>,
    pub numeric_fixture: Option<ResourceFilePin>,
}
impl ResourceRunInput {
    pub fn validate(&self) -> Result<()> {
        self.policy.validate()?;
        ensure!(
            self.schema == "airk.arbiter.resource-run/2"
                && self.policy_digest == self.policy.digest()?
                && !self.files.is_empty()
                && self.files.len() <= 128,
            "software-v2 resource request identity differs"
        );
        self.source_manifest.validate()?;
        self.profile.validate()?;
        if let Some(fixture) = &self.numeric_fixture {
            fixture.validate()?;
        }
        for (name, path) in &self.files {
            ensure!(
                !name.is_empty() && name.len() <= 128 && !name.chars().any(char::is_control),
                "resource asset name bound"
            );
            resource_path(path)?;
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= 1 << 20,
            "private resource request exceeds metadata bound"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResourceFailure {
    DeadlineOverrun,
    RssOverrun,
    NativeFailure,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceSample {
    pub elapsed_us: u64,
    pub peak_process_rss_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceSeries {
    pub attempted_samples: usize,
    pub samples: Vec<ResourceSample>,
    pub reservation_bytes: u64,
    pub complete: bool,
    pub nearest_rank_p95_us: Option<u64>,
    pub maximum_us: Option<u64>,
    pub failure: Option<ResourceFailure>,
}
impl ResourceSeries {
    pub fn new(reservation_bytes: u64) -> Result<Self> {
        ensure!(
            reservation_bytes > 0,
            "positive native reservation required"
        );
        Ok(Self {
            attempted_samples: 0,
            samples: vec![],
            reservation_bytes,
            complete: false,
            nearest_rank_p95_us: None,
            maximum_us: None,
            failure: None,
        })
    }
    pub fn validate(&self, policy: &ResourcePolicy) -> Result<()> {
        policy.validate()?;
        let count = self.samples.len();
        ensure!(
            self.reservation_bytes > 0
                && count <= 20
                && self.attempted_samples <= 20
                && self.attempted_samples >= count
                && self.attempted_samples <= count + 1,
            "resource observation or attempt bound differs"
        );
        let first_failure = self.samples.iter().position(|sample| {
            sample.elapsed_us > policy.p95_limit_us
                || sample.peak_process_rss_bytes > self.reservation_bytes
        });
        if self.complete {
            ensure!(
                count == 20
                    && self.attempted_samples == 20
                    && self.failure.is_none()
                    && first_failure.is_none(),
                "complete resource series requires twenty valid actual observations"
            );
            let mut ordered = self
                .samples
                .iter()
                .map(|row| row.elapsed_us)
                .collect::<Vec<_>>();
            ordered.sort_unstable();
            ensure!(
                self.nearest_rank_p95_us == Some(ordered[18])
                    && self.maximum_us == Some(ordered[19]),
                "resource quantiles differ from complete native observations"
            );
        } else {
            ensure!(
                self.nearest_rank_p95_us.is_none() && self.maximum_us.is_none(),
                "partial resource observations cannot claim p95 or maximum"
            );
            match self.failure {
                Some(ResourceFailure::NativeFailure) => ensure!(
                    self.attempted_samples == count + 1 && first_failure.is_none(),
                    "unknown native failure requires one unobserved attempt"
                ),
                Some(ResourceFailure::DeadlineOverrun | ResourceFailure::RssOverrun) => {
                    ensure!(
                        count > 0
                            && self.attempted_samples == count
                            && first_failure == Some(count - 1),
                        "first known failed sample must be retained and terminate the series"
                    );
                    let last = self.samples[count - 1];
                    let actual = if last.elapsed_us > policy.p95_limit_us {
                        ResourceFailure::DeadlineOverrun
                    } else {
                        ResourceFailure::RssOverrun
                    };
                    ensure!(
                        self.failure == Some(actual),
                        "resource failure class differs"
                    );
                }
                None => ensure!(
                    first_failure.is_none() && count < 20,
                    "incomplete resource series contains an unreported outcome"
                ),
            }
        }
        Ok(())
    }
    pub fn begin_sample(&mut self, policy: &ResourcePolicy) -> Result<()> {
        self.validate(policy)?;
        ensure!(
            !self.complete
                && self.failure.is_none()
                && self.attempted_samples == self.samples.len(),
            "resource series cannot admit another native attempt"
        );
        self.attempted_samples += 1;
        Ok(())
    }
    /// Only an actual finite native return and observed clock/RSS call this method.
    pub fn finish_sample(&mut self, sample: ResourceSample, policy: &ResourcePolicy) -> Result<()> {
        self.validate(policy)?;
        ensure!(
            !self.complete
                && self.failure.is_none()
                && self.attempted_samples == self.samples.len() + 1,
            "resource sample lacks its original admitted attempt"
        );
        self.samples.push(sample);
        self.failure = if sample.elapsed_us > policy.p95_limit_us {
            Some(ResourceFailure::DeadlineOverrun)
        } else if sample.peak_process_rss_bytes > self.reservation_bytes {
            Some(ResourceFailure::RssOverrun)
        } else {
            None
        };
        if self.failure.is_none() && self.samples.len() == 20 {
            let mut ordered = self
                .samples
                .iter()
                .map(|row| row.elapsed_us)
                .collect::<Vec<_>>();
            ordered.sort_unstable();
            self.nearest_rank_p95_us = Some(ordered[18]);
            self.maximum_us = Some(ordered[19]);
            self.complete = true;
        }
        self.validate(policy)?;
        ensure!(
            self.failure.is_none(),
            "first known native resource failure"
        );
        Ok(())
    }
    pub fn fail_native(&mut self, policy: &ResourcePolicy) -> Result<()> {
        self.validate(policy)?;
        ensure!(
            !self.complete
                && self.failure.is_none()
                && self.attempted_samples == self.samples.len() + 1,
            "native failure lacks its original admitted attempt"
        );
        self.failure = Some(ResourceFailure::NativeFailure);
        self.validate(policy)
    }
    pub fn require_complete(&self, policy: &ResourcePolicy) -> Result<()> {
        self.validate(policy)?;
        ensure!(self.complete, "complete resource evidence is unavailable");
        Ok(())
    }
}

#[cfg(test)]
mod tests;
