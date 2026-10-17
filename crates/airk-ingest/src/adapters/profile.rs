// SPDX-License-Identifier: Apache-2.0
//! Bounded deterministic selection and persisted collection overrides.
use super::registry::AdapterRegistry;
use airk_core::extraction::ExtractionProfile;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuleMatch {
    Exact {
        path: String,
    },
    Scoped {
        prefix: String,
        suffix: Option<String>,
        basename: Option<String>,
    },
    HeaderContext {
        prefix: String,
        configuration: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionRule {
    pub selector: RuleMatch,
    pub case_sensitive: bool,
    pub adapter_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<airk_core::extraction::IndexDetail>,
    pub version: u32,
    pub rules: Vec<SelectionRule>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub profile: ExtractionProfile,
    pub basis: String,
    pub ambiguity: Vec<String>,
}

pub fn validate_relative_path(path: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !path.is_empty()
            && path.len() <= 4096
            && !path.contains(['\\', ':', '\0'])
            && !path.starts_with('/'),
        "invalid relative selection path"
    );
    anyhow::ensure!(
        path.split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".."),
        "invalid relative path component"
    );
    Ok(())
}

impl ExtractionPolicy {
    pub fn parse(bytes: &[u8], registry: &AdapterRegistry) -> anyhow::Result<Self> {
        anyhow::ensure!(bytes.len() <= 131_072, "extraction policy exceeds 128 KiB");
        let policy: Self = serde_json::from_slice(bytes)?;
        policy.validate(registry)?;
        Ok(policy)
    }

    pub fn validate(&self, registry: &AdapterRegistry) -> anyhow::Result<()> {
        anyhow::ensure!(
            matches!(self.version, 1..=3)
                && (self.detail.is_none() || self.version == 3)
                && self.rules.len() <= 256,
            "unsupported or oversized extraction policy"
        );
        anyhow::ensure!(
            serde_json::to_vec(self)?.len() <= 131_072,
            "extraction policy exceeds 128 KiB"
        );
        for rule in &self.rules {
            anyhow::ensure!(
                registry.get(&rule.adapter_id).is_some(),
                "unknown override adapter"
            );
            match &rule.selector {
                RuleMatch::Exact { path } => validate_relative_path(path)?,
                RuleMatch::Scoped {
                    prefix,
                    suffix,
                    basename,
                } => {
                    if !prefix.is_empty() {
                        validate_relative_path(prefix)?;
                    }
                    if let Some(value) = suffix {
                        anyhow::ensure!(
                            !value.is_empty()
                                && value.len() <= 256
                                && !value.contains(['/', '\\', '\0', ':']),
                            "invalid suffix"
                        );
                    }
                    if let Some(value) = basename {
                        validate_relative_path(value)?;
                        anyhow::ensure!(!value.contains('/'), "invalid basename");
                    }
                }
                RuleMatch::HeaderContext {
                    prefix,
                    configuration,
                } => {
                    anyhow::ensure!(
                        self.version >= 2,
                        "header context requires policy version 2"
                    );
                    anyhow::ensure!(
                        rule.case_sensitive && matches!(rule.adapter_id.as_str(), "c" | "cpp"),
                        "header context requires case-sensitive C/C++ qualification"
                    );
                    if !prefix.is_empty() {
                        validate_relative_path(prefix)?;
                    }
                    validate_relative_path(configuration)?;
                }
            }
        }
        anyhow::ensure!(
            self.declared_configurations().len() <= crate::header_context::MAX_CONFIGURATIONS,
            "too many captured header configurations"
        );
        Ok(())
    }

    /// Explicit logical collection paths only; this method performs no reads.
    pub fn declared_configurations(&self) -> Vec<&str> {
        let mut paths = std::collections::BTreeSet::new();
        for rule in &self.rules {
            if let RuleMatch::HeaderContext { configuration, .. } = &rule.selector {
                paths.insert(configuration.as_str());
            }
        }
        paths.into_iter().collect()
    }

    /// Apply captured context only after explicit exact/scoped overrides.
    pub fn select_in_root_captured(
        &self,
        registry: &AdapterRegistry,
        path: &str,
        relative: &str,
        source: &[u8],
        captured: &crate::header_context::CapturedMap,
    ) -> anyhow::Result<Selection> {
        let mut selected = self.select_in_root(registry, path, relative, source)?;
        if matches!(
            selected.basis.as_str(),
            "exact_override" | "scoped_override"
        ) || !relative.to_ascii_lowercase().ends_with(".h")
        {
            return Ok(selected);
        }
        let rules: Vec<_> = self
            .rules
            .iter()
            .filter(|rule| match &rule.selector {
                RuleMatch::HeaderContext { prefix, .. } => {
                    prefix.is_empty()
                        || path
                            .strip_prefix(prefix)
                            .is_some_and(|rest| rest.starts_with('/'))
                }
                _ => false,
            })
            .collect();
        if rules.is_empty() {
            return Ok(selected);
        }
        let best = rules
            .iter()
            .filter_map(|rule| match &rule.selector {
                RuleMatch::HeaderContext { prefix, .. } => Some(prefix.len()),
                _ => None,
            })
            .max()
            .unwrap();
        let rules:Vec<_>=rules.into_iter().filter(|rule|matches!(&rule.selector,RuleMatch::HeaderContext{prefix,..}if prefix.len()==best)).collect();
        if rules.len() != 1 {
            selected
                .ambiguity
                .push("Conflicting captured header context rules; retained C default".into());
            return Ok(selected);
        }
        let rule = rules[0];
        let RuleMatch::HeaderContext { configuration, .. } = &rule.selector else {
            unreachable!()
        };
        if let Some(context) = captured
            .get(configuration)
            .filter(|context| context.path() == configuration)
            && context.adapter(path) == Some(rule.adapter_id.as_str())
        {
            selected.profile = registry.effective_profile(path, &rule.adapter_id)?;
            selected.profile.options.insert(
                "header_context".into(),
                crate::header_context::CONTEXT_VERSION.into(),
            );
            selected
                .profile
                .configuration_inputs
                .insert(configuration.clone(), context.digest());
            selected.basis = "captured_header_context".into();
            selected.ambiguity.clear();
        } else {
            selected.ambiguity.push("Header context missing, unsupported or conflicting; retained C default without C++ inference".into());
        }
        Ok(selected)
    }

    pub fn select(&self, registry: &AdapterRegistry, path: &str) -> anyhow::Result<Selection> {
        self.select_with_prefix(registry, path, &[])
    }

    /// Overrides address collection paths; repository conventions are relative
    /// to the owning source root, independent of its collection alias.
    pub(crate) fn select_in_root(
        &self,
        registry: &AdapterRegistry,
        path: &str,
        relative: &str,
        source: &[u8],
    ) -> anyhow::Result<Selection> {
        validate_relative_path(relative)?;
        self.select_with_default_path(registry, path, relative, source)
    }

    /// Caller supplies source bytes already charged to scan budgets. Only the
    /// first 4096 bytes can participate in declared content discrimination.
    pub fn select_with_prefix(
        &self,
        registry: &AdapterRegistry,
        path: &str,
        source: &[u8],
    ) -> anyhow::Result<Selection> {
        self.select_with_default_path(registry, path, path, source)
    }

    fn select_with_default_path(
        &self,
        registry: &AdapterRegistry,
        path: &str,
        default_path: &str,
        source: &[u8],
    ) -> anyhow::Result<Selection> {
        self.validate(registry)?;
        validate_relative_path(path)?;
        let mut candidates = Vec::new();
        for rule in &self.rules {
            let candidate = if rule.case_sensitive {
                path.to_owned()
            } else {
                path.to_ascii_lowercase()
            };
            let normalized = |s: &str| {
                if rule.case_sensitive {
                    s.to_owned()
                } else {
                    s.to_ascii_lowercase()
                }
            };
            let rank = match &rule.selector {
                RuleMatch::Exact { path } if candidate == normalized(path) => {
                    Some((2, path.len(), rule.case_sensitive))
                }
                RuleMatch::Scoped {
                    prefix,
                    suffix,
                    basename,
                } => {
                    let prefix = normalized(prefix);
                    let under = prefix.is_empty()
                        || candidate
                            .strip_prefix(&prefix)
                            .is_some_and(|rest| rest.starts_with('/'));
                    let suffix_ok = suffix
                        .as_ref()
                        .is_none_or(|s| candidate.ends_with(&normalized(s)));
                    let basename_ok = basename.as_ref().is_none_or(|b| {
                        candidate.rsplit('/').next() == Some(normalized(b).as_str())
                    });
                    (under && suffix_ok && basename_ok).then_some((
                        1,
                        prefix.len(),
                        rule.case_sensitive,
                    ))
                }
                _ => None,
            };
            if let Some(rank) = rank {
                candidates.push((rank, rule));
            }
        }
        if let Some(best) = candidates.iter().map(|(rank, _)| *rank).max() {
            let matches: Vec<_> = candidates
                .iter()
                .filter(|(rank, _)| *rank == best)
                .collect();
            anyhow::ensure!(
                matches.len() == 1,
                "ambiguous equal-precedence extraction overrides"
            );
            let rule = matches[0].1;
            return Ok(Selection {
                profile: registry.effective_profile(path, &rule.adapter_id)?,
                basis: if best.0 == 2 {
                    "exact_override"
                } else {
                    "scoped_override"
                }
                .into(),
                ambiguity: vec![],
            });
        }
        registry.select_with_prefix(default_path, &source[..source.len().min(4096)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_header_inputs_are_verified_scoped_and_below_explicit_overrides()
    -> anyhow::Result<()> {
        let registry = AdapterRegistry::baseline();
        let mut policy = ExtractionPolicy {
            detail: None,
            version: 2,
            rules: vec![SelectionRule {
                selector: RuleMatch::HeaderContext {
                    prefix: "@vendor/build/include".into(),
                    configuration: "@vendor/build/compile_commands.json".into(),
                },
                case_sensitive: true,
                adapter_id: "cpp".into(),
            }],
        };
        let path = "@vendor/build/include/beacon.h";
        let source = b"class Beacon {};";
        let empty = crate::header_context::CapturedMap::new();
        let fallback = policy.select_in_root_captured(
            &registry,
            path,
            "build/include/beacon.h",
            source,
            &empty,
        )?;
        assert_eq!(fallback.profile.adapter_id, "c");
        assert!(!fallback.ambiguity.is_empty());
        let bytes = br#"[{"file":"include/beacon.h","arguments":["compiler","-x","c++"]}]"#;
        let context = crate::header_context::capture_configuration(
            "@vendor/build/compile_commands.json",
            bytes,
        )?;
        let digest = context.digest();
        let captured = std::collections::BTreeMap::from([(context.path().to_owned(), context)]);
        let selected = policy.select_in_root_captured(
            &registry,
            path,
            "build/include/beacon.h",
            source,
            &captured,
        )?;
        assert_eq!(selected.profile.adapter_id, "cpp");
        assert_eq!(selected.basis, "captured_header_context");
        assert!(selected.ambiguity.is_empty());
        assert_eq!(
            selected.profile.configuration_inputs["@vendor/build/compile_commands.json"],
            digest
        );
        let unrelated =
            policy.select_in_root_captured(&registry, "other.h", "other.h", source, &captured)?;
        assert!(unrelated.profile.configuration_inputs.is_empty());
        policy.rules.push(SelectionRule {
            selector: RuleMatch::Exact { path: path.into() },
            case_sensitive: true,
            adapter_id: "c".into(),
        });
        let explicit = policy.select_in_root_captured(
            &registry,
            path,
            "build/include/beacon.h",
            source,
            &captured,
        )?;
        assert_eq!(explicit.profile.adapter_id, "c");
        assert!(explicit.profile.configuration_inputs.is_empty());
        policy.version = 1;
        assert!(policy.validate(&registry).is_err());
        Ok(())
    }
    #[test]
    fn repository_selection_uses_source_root_but_overrides_use_collection_path()
    -> anyhow::Result<()> {
        let registry = AdapterRegistry::baseline();
        let mut policy = ExtractionPolicy {
            detail: None,
            version: 1,
            rules: vec![],
        };
        let logical = "@vendor/.github/workflows/check.yml";
        let relative = ".github/workflows/check.yml";
        assert_eq!(
            policy
                .select_in_root(&registry, logical, relative, b"")?
                .profile
                .adapter_id,
            "github-actions"
        );
        policy.rules.push(SelectionRule {
            selector: RuleMatch::Exact {
                path: logical.into(),
            },
            case_sensitive: true,
            adapter_id: "text".into(),
        });
        assert_eq!(
            policy
                .select_in_root(&registry, logical, relative, b"")?
                .profile
                .adapter_id,
            "text"
        );
        Ok(())
    }
    #[test]
    fn empty_scope_selects_root_headers_without_crossing_other_scopes() -> anyhow::Result<()> {
        let registry = AdapterRegistry::baseline();
        let policy = ExtractionPolicy {
            detail: None,
            version: 1,
            rules: vec![
                SelectionRule {
                    selector: RuleMatch::Scoped {
                        prefix: String::new(),
                        suffix: Some(".h".into()),
                        basename: None,
                    },
                    case_sensitive: true,
                    adapter_id: "cpp".into(),
                },
                SelectionRule {
                    selector: RuleMatch::Scoped {
                        prefix: "c".into(),
                        suffix: Some(".h".into()),
                        basename: None,
                    },
                    case_sensitive: true,
                    adapter_id: "c".into(),
                },
            ],
        };
        assert_eq!(policy.select(&registry, "a.h")?.profile.language, "cpp");
        assert_eq!(
            policy.select(&registry, "include/a.h")?.profile.language,
            "cpp"
        );
        assert_eq!(policy.select(&registry, "c/a.h")?.profile.language, "c");
        assert_eq!(policy.select(&registry, "cpp/a.h")?.profile.language, "cpp");
        assert_eq!(policy.select(&registry, "a.rs")?.profile.language, "rust");
        Ok(())
    }
    #[test]
    fn conflicts_traversal_and_case_are_explicit() -> anyhow::Result<()> {
        let registry = AdapterRegistry::baseline();
        let mut policy = ExtractionPolicy {
            detail: None,
            version: 1,
            rules: vec![],
        };
        assert!(policy.select(&registry, "../a.h").is_err());
        assert_eq!(policy.select(&registry, "a.h")?.profile.language, "c");
        assert!(!policy.select(&registry, "a.h")?.ambiguity.is_empty());
        let rule = SelectionRule {
            selector: RuleMatch::Exact { path: "a.H".into() },
            case_sensitive: true,
            adapter_id: "cpp".into(),
        };
        policy.rules.push(rule.clone());
        assert_eq!(policy.select(&registry, "a.H")?.profile.language, "cpp");
        assert_eq!(policy.select(&registry, "a.h")?.profile.language, "c");
        policy.rules.push(rule);
        assert!(policy.select(&registry, "a.H").is_err());
        Ok(())
    }
    #[test]
    fn exact_and_longest_scope_win() -> anyhow::Result<()> {
        let registry = AdapterRegistry::baseline();
        let policy = ExtractionPolicy {
            detail: None,
            version: 1,
            rules: vec![
                SelectionRule {
                    selector: RuleMatch::Scoped {
                        prefix: "src".into(),
                        suffix: Some(".h".into()),
                        basename: None,
                    },
                    case_sensitive: true,
                    adapter_id: "c".into(),
                },
                SelectionRule {
                    selector: RuleMatch::Scoped {
                        prefix: "src/cpp".into(),
                        suffix: None,
                        basename: None,
                    },
                    case_sensitive: true,
                    adapter_id: "cpp".into(),
                },
                SelectionRule {
                    selector: RuleMatch::Exact {
                        path: "src/cpp/a.h".into(),
                    },
                    case_sensitive: true,
                    adapter_id: "text".into(),
                },
            ],
        };
        assert_eq!(
            policy.select(&registry, "src/cpp/a.h")?.profile.language,
            "text"
        );
        assert_eq!(
            policy.select(&registry, "src/cpp/b.h")?.profile.language,
            "cpp"
        );
        assert_eq!(
            policy.select(&registry, "src/other/a.h")?.profile.language,
            "c"
        );
        Ok(())
    }
}
