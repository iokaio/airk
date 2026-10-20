// SPDX-License-Identifier: Apache-2.0
//! Explicit semantic eligibility and transient exact-input graph identity.
use crate::{
    extraction::ExtractionProfile,
    identity::{Sha256Digest, framed_digest},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const VECTOR_SELECTION_VERSION: u32 = 1;
pub const MAX_POLICY_BYTES: usize = 128 * 1024;
pub const MAX_EXCLUSIONS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorSelectionPolicy {
    pub version: u32,
    pub exclusions: Vec<VectorExclusionRule>,
    #[serde(default)]
    pub inclusions: Vec<VectorExclusionRule>,
    #[serde(default)]
    pub agent_defaults: bool,
}

// Rule order has no meaning. Serialize borrowed rules in ID order without
// cloning the potentially large inventory that embeds this policy.
impl Serialize for VectorSelectionPolicy {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct CanonicalPolicy<'a> {
            version: u32,
            exclusions: Vec<&'a VectorExclusionRule>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            inclusions: Vec<&'a VectorExclusionRule>,
            #[serde(skip_serializing_if = "is_false")]
            agent_defaults: bool,
        }
        let mut exclusions: Vec<_> = self.exclusions.iter().collect();
        exclusions.sort_by(|a, b| a.id.cmp(&b.id));
        let mut inclusions: Vec<_> = self.inclusions.iter().collect();
        inclusions.sort_by(|a, b| a.id.cmp(&b.id));
        CanonicalPolicy {
            version: self.version,
            exclusions,
            inclusions,
            agent_defaults: self.agent_defaults,
        }
        .serialize(serializer)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorExclusionRule {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorSelectionDecision {
    pub eligible: bool,
    pub excluded_by: Option<String>,
}

impl Default for VectorSelectionPolicy {
    fn default() -> Self {
        Self {
            version: VECTOR_SELECTION_VERSION,
            exclusions: Vec::new(),
            inclusions: Vec::new(),
            agent_defaults: false,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

fn overlaps(a: &VectorExclusionRule, b: &VectorExclusionRule) -> bool {
    let equal = |a: &Option<String>, b: &Option<String>| {
        a.as_ref().zip(b.as_ref()).is_none_or(|(a, b)| a == b)
    };
    equal(&a.basename, &b.basename)
        && equal(&a.adapter, &b.adapter)
        && equal(&a.language, &b.language)
        && a.path_prefix
            .as_ref()
            .zip(b.path_prefix.as_ref())
            .is_none_or(|(a, b)| a.starts_with(b) || b.starts_with(a))
        && a.suffix
            .as_ref()
            .zip(b.suffix.as_ref())
            .is_none_or(|(a, b)| a.ends_with(b) || b.ends_with(a))
        && a.basename
            .as_ref()
            .zip(b.suffix.as_ref())
            .is_none_or(|(a, b)| a.ends_with(b))
        && b.basename
            .as_ref()
            .zip(a.suffix.as_ref())
            .is_none_or(|(a, b)| a.ends_with(b))
}

fn relative_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 4096
        && !value.starts_with('/')
        && !value.contains(['\\', ':', '\0'])
        && value
            .trim_end_matches('/')
            .split('/')
            .all(|part| !part.is_empty() && !matches!(part, "." | ".."))
}

impl VectorSelectionPolicy {
    pub fn agent_lean() -> Self {
        Self {
            version: 2,
            agent_defaults: true,
            ..Self::default()
        }
    }
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_POLICY_BYTES,
            "vector selection policy exceeds byte bound"
        );
        let policy: Self = serde_json::from_slice(bytes)?;
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            (self.version == VECTOR_SELECTION_VERSION
                && self.inclusions.is_empty()
                && !self.agent_defaults)
                || self.version == 2,
            "unsupported vector selection policy version"
        );
        ensure!(
            self.exclusions.len().saturating_add(self.inclusions.len()) <= MAX_EXCLUSIONS,
            "too many vector exclusion rules"
        );
        let mut ids = BTreeSet::new();
        for rule in self.exclusions.iter().chain(&self.inclusions) {
            ensure!(
                !rule.id.is_empty()
                    && rule.id.len() <= 64
                    && rule
                        .id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                    && ids.insert(&rule.id),
                "invalid or duplicate vector exclusion rule id"
            );
            ensure!(
                [
                    &rule.path_prefix,
                    &rule.basename,
                    &rule.suffix,
                    &rule.adapter,
                    &rule.language
                ]
                .iter()
                .any(|v| v.is_some()),
                "vector exclusion rule needs a selector"
            );
            if let Some(prefix) = &rule.path_prefix {
                ensure!(
                    relative_path(prefix),
                    "invalid vector exclusion path prefix"
                );
            }
            for (value, maximum) in [
                (&rule.basename, 256),
                (&rule.suffix, 256),
                (&rule.adapter, 128),
                (&rule.language, 128),
            ] {
                if let Some(value) = value {
                    ensure!(
                        !value.is_empty()
                            && value.len() <= maximum
                            && !value.chars().any(char::is_control),
                        "invalid vector exclusion selector"
                    );
                }
            }
            if let Some(basename) = &rule.basename {
                ensure!(
                    !basename.contains(['/', '\\']) && !matches!(basename.as_str(), "." | ".."),
                    "invalid vector exclusion basename"
                );
            }
            if let Some(suffix) = &rule.suffix {
                ensure!(
                    !suffix.contains(['/', '\\']),
                    "invalid vector exclusion suffix"
                );
            }
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_POLICY_BYTES,
            "vector selection policy exceeds byte bound"
        );
        for include in &self.inclusions {
            for exclude in &self.exclusions {
                ensure!(
                    !overlaps(include, exclude),
                    "potentially conflicting vector inclusion {} and exclusion {}; use disjoint explicit rules",
                    include.id,
                    exclude.id
                );
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec(self)?)
    }

    pub fn digest(&self) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"vector-selection-policy/1",
            &[&self.canonical_bytes()?],
        ))
    }

    pub fn validated(&self) -> Result<ValidatedVectorSelection<'_>> {
        self.validate()?;
        Ok(ValidatedVectorSelection(self))
    }

    pub fn decision(
        &self,
        path: &str,
        profile: &ExtractionProfile,
        language: &str,
    ) -> Result<VectorSelectionDecision> {
        self.validated()?.decision(path, profile, language)
    }
}

/// A borrowed validated policy avoids repeated serialization in artifact loops.
pub struct ValidatedVectorSelection<'a>(&'a VectorSelectionPolicy);

impl ValidatedVectorSelection<'_> {
    /// Selectors in a rule are ANDed; any matching rule excludes. Matching is
    /// case sensitive against the published slash-separated path, including alias.
    pub fn decision(
        &self,
        path: &str,
        profile: &ExtractionProfile,
        language: &str,
    ) -> Result<VectorSelectionDecision> {
        ensure!(
            relative_path(path) && !path.ends_with('/'),
            "invalid vector selection artifact path"
        );
        let basename = path.rsplit('/').next().unwrap_or(path);
        let matches = |rule: &&VectorExclusionRule| {
            rule.path_prefix
                .as_ref()
                .is_none_or(|v| path.starts_with(v))
                && rule.basename.as_ref().is_none_or(|v| basename == v)
                && rule.suffix.as_ref().is_none_or(|v| basename.ends_with(v))
                && rule
                    .adapter
                    .as_ref()
                    .is_none_or(|v| &profile.adapter_id == v)
                && rule.language.as_ref().is_none_or(|v| language == v)
        };
        let included = self.0.inclusions.iter().any(|rule| matches(&rule));
        let excluded_by = self
            .0
            .exclusions
            .iter()
            .filter(matches)
            .map(|rule| &rule.id)
            .min()
            .cloned();
        ensure!(
            !included || excluded_by.is_none(),
            "conflicting explicit vector selection rules for {path}"
        );
        let excluded_by = excluded_by.or_else(|| {
            if !self.0.agent_defaults || included {
                return None;
            }
            if matches!(
                basename,
                "Cargo.lock"
                    | "package-lock.json"
                    | "yarn.lock"
                    | "pnpm-lock.yaml"
                    | "poetry.lock"
                    | "uv.lock"
                    | "Gemfile.lock"
                    | "composer.lock"
            ) {
                Some("agent.lockfile".into())
            } else if basename.ends_with(".min.js") || basename.ends_with(".min.css") {
                Some("agent.minified".into())
            } else {
                None
            }
        });
        Ok(VectorSelectionDecision {
            eligible: excluded_by.is_none(),
            excluded_by,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparedVectorNode {
    /// None records a successful no-vector result; positive IDs identify graph nodes.
    pub graph_id: Option<u32>,
}

/// Collection-update-local exact-key registry. Failed computations must not be
/// remembered; remembering an existing key must agree with its prior result.
pub trait VectorNodeRegistry {
    fn lookup(&self, key: Sha256Digest) -> Result<Option<PreparedVectorNode>>;
    fn remember(&self, key: Sha256Digest, node: PreparedVectorNode) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> ExtractionProfile {
        serde_json::from_value(serde_json::json!({"contract_version":2,"adapter_id":"text","adapter_version":"1","language":"text","dialect":null,"grammar_identity":null,"mapping_version":1,"chunk_version":3,"options":{},"configuration_inputs":{}})).unwrap()
    }
    #[test]
    fn policy_order_is_canonical_and_selectors_are_conjunctive() {
        let mut policy = VectorSelectionPolicy::parse(br#"{"version":1,"exclusions":[{"id":"z","suffix":".lock"},{"id":"a","path_prefix":"@vendor/","language":"text","basename":"Cargo.lock"}]}"#).unwrap();
        let digest = policy.digest().unwrap();
        policy.exclusions.reverse();
        assert_eq!(digest, policy.digest().unwrap());
        assert_eq!(
            policy
                .decision("@vendor/Cargo.lock", &profile(), "text")
                .unwrap()
                .excluded_by
                .as_deref(),
            Some("a")
        );
        assert_eq!(
            policy
                .decision("@vendor/Cargo.lock", &profile(), "rust")
                .unwrap()
                .excluded_by
                .as_deref(),
            Some("z")
        );
        assert!(
            policy
                .decision("@vendor/Cargo.LOCK", &profile(), "text")
                .unwrap()
                .eligible
        );
        assert!(
            VectorSelectionPolicy::default()
                .decision("Cargo.lock", &profile(), "text")
                .unwrap()
                .eligible
        );
        assert_ne!(digest, VectorSelectionPolicy::default().digest().unwrap());
    }
    #[test]
    fn invalid_configuration_is_rejected_before_use() {
        for json in [
            r#"{"version":3,"exclusions":[]}"#,
            r#"{"version":1,"exclusions":[{"id":"a"}]}"#,
            r#"{"version":1,"exclusions":[{"id":"a","basename":"x"},{"id":"a","basename":"y"}]}"#,
            r#"{"version":1,"exclusions":[{"id":"a","path_prefix":"../"}]}"#,
            r#"{"version":1,"exclusions":[],"unknown":true}"#,
        ] {
            assert!(
                VectorSelectionPolicy::parse(json.as_bytes()).is_err(),
                "{json}"
            );
        }
        assert!(VectorSelectionPolicy::parse(&vec![b' '; MAX_POLICY_BYTES + 1]).is_err());
    }

    #[test]
    fn agent_defaults_preserve_text_eligibility_and_explicit_inclusion() {
        let mut policy = VectorSelectionPolicy::agent_lean();
        assert_eq!(
            policy
                .decision("Cargo.lock", &profile(), "text")
                .unwrap()
                .excluded_by
                .as_deref(),
            Some("agent.lockfile")
        );
        assert!(
            policy
                .decision("tests/configuration.rs", &profile(), "rust")
                .unwrap()
                .eligible
        );
        assert!(
            policy
                .decision("vendor/generated.rs", &profile(), "rust")
                .unwrap()
                .eligible
        );
        policy.inclusions.push(VectorExclusionRule {
            id: "needed".into(),
            basename: Some("Cargo.lock".into()),
            ..Default::default()
        });
        assert!(
            policy
                .decision("Cargo.lock", &profile(), "text")
                .unwrap()
                .eligible
        );
        policy.exclusions.push(VectorExclusionRule {
            id: "excluded".into(),
            basename: Some("Cargo.lock".into()),
            ..Default::default()
        });
        assert!(policy.decision("Cargo.lock", &profile(), "text").is_err());
        let old = VectorSelectionPolicy::default();
        assert_eq!(
            old.canonical_bytes().unwrap(),
            br#"{"version":1,"exclusions":[]}"#
        );
    }
}
