// SPDX-License-Identifier: Apache-2.0
//! Native source predicates shared by discovery, capture and saved-source checks.
//! A predicate is not authority: the native owner must bind its policy revision.
use airk_core::{index_catalog::PathScope, source_policy::SourcePolicy};

use crate::native_name_boundary::NativeNameBoundaryUnavailable;
use anyhow::Result;

fn same(left: &str, right: &str) -> bool {
    left == right
        || cfg!(any(windows, target_os = "macos"))
            && left.is_ascii()
            && right.is_ascii()
            && left.eq_ignore_ascii_case(right)
}

/// Unknown Unicode collation is never proof of disjoint physical names.
fn deny_same(left: &str, right: &str) -> Result<bool> {
    if same(left, right) {
        return Ok(true);
    }
    if cfg!(any(windows, target_os = "macos")) && (!left.is_ascii() || !right.is_ascii()) {
        return Err(NativeNameBoundaryUnavailable::new("unicode_name_equivalence").into());
    }
    Ok(false)
}

fn denied_prefix(candidate: &[String], prefix: &[String]) -> Result<bool> {
    if candidate.len() < prefix.len() {
        return Ok(false);
    }
    let mut unknown = None;
    for (left, right) in candidate.iter().zip(prefix) {
        match deny_same(left, right) {
            Ok(false) => return Ok(false),
            Ok(true) => (),
            Err(error) => unknown = Some(error),
        }
    }
    if let Some(error) = unknown {
        return Err(error);
    }
    Ok(true)
}

pub fn restricts(policy: &SourcePolicy) -> bool {
    !policy.denies.is_empty() || policy.positive_includes.is_some()
}

/// Static dependency contract, not an observation of files or captured bytes.
/// Reporting this descriptor performs no filesystem or corpus work.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ControlDependencies {
    pub version: u32,
    pub basis: &'static str,
    pub positive_selection: &'static str,
    pub permanent_denies: &'static str,
    pub does_not_expand_document_eligibility: bool,
    pub ignore_names: &'static [&'static str],
    pub global_configuration: &'static str,
    pub ignore_file_bytes: usize,
    pub ignore_snapshot_bytes: usize,
    pub ignore_cached_directories: usize,
    pub declared_header_configurations: &'static str,
    pub header_configuration_files: usize,
    pub header_configuration_file_bytes: usize,
    pub capture: &'static str,
}

pub fn control_dependencies() -> ControlDependencies {
    ControlDependencies {
        version: 1,
        basis: "metadata_only",
        positive_selection: "Qualified ignore controls in admitted directories and their ancestors, bounded global ignore configuration, and explicitly declared header configurations may be dependencies outside positive document selection.",
        permanent_denies: "authoritative_for_every_control_dependency",
        does_not_expand_document_eligibility: true,
        ignore_names: &[".airkignore", ".ignore", ".gitignore", ".git/info/exclude"],
        global_configuration: "Bounded Git global/system configuration and its selected excludes file; ambiguous, redirected or unsupported controls refuse.",
        ignore_file_bytes: crate::policy_ignore::MAX_FILE_BYTES,
        ignore_snapshot_bytes: crate::policy_ignore::MAX_CONTROL_BYTES,
        ignore_cached_directories: crate::policy_ignore::MAX_DIRECTORIES,
        declared_header_configurations: "Only explicitly declared exact configuration paths; existing discovery/source byte budgets still apply; no sibling asset or model directory admission.",
        header_configuration_files: crate::header_context::MAX_CONFIGURATIONS,
        header_configuration_file_bytes: crate::header_context::MAX_CONFIGURATION_BYTES,
        capture: "protected_no_follow_with_native_name_qualification",
    }
}

/// Metadata-only platform case-mode proof. Does not open source file contents.
pub fn qualify(policy: &SourcePolicy, root: &str, path: &str, directory: bool) -> Result<()> {
    if restricts(policy) {
        crate::native_name_boundary::qualify(std::path::Path::new(root), path, directory)?;
    }
    Ok(())
}

/// The ignore adapter reads control files on entering an admitted directory.
/// Refuse when a denied control input cannot be omitted by that adapter; never
/// read its bytes or silently change ordinary ignore semantics.
pub fn qualify_discovery_directory(policy: &SourcePolicy, root: &str, path: &str) -> Result<()> {
    qualify(policy, root, path, true)?;
    if !restricts(policy) {
        return Ok(());
    }
    if !allows_checked(policy, root, path, true)? {
        return Err(NativeNameBoundaryUnavailable::new("excluded_control_directory").into());
    }
    for control in [
        ".airkignore",
        ".gitignore",
        ".ignore",
        ".git/config",
        ".git/info/exclude",
    ] {
        let relative = if path.is_empty() {
            control.to_owned()
        } else {
            format!("{path}/{control}")
        };
        if !allows_for_purpose(policy, root, &relative, false, false)? {
            return Err(NativeNameBoundaryUnavailable::new("excluded_ignore_control").into());
        }
    }
    Ok(())
}

pub fn qualify_source_access(policy: &SourcePolicy, root: &str, path: &str) -> Result<()> {
    qualify(policy, root, path, false)?;
    qualify_discovery_directory(policy, root, "")?;
    for (at, _) in path.match_indices('/') {
        qualify_discovery_directory(policy, root, &path[..at])?;
    }
    Ok(())
}

fn native_relative_path(path: &str, directory: bool) -> bool {
    path.len() <= 4096
        && (directory || !path.is_empty())
        && !path.starts_with('/')
        && !path.contains('\0')
        && (path.is_empty() || path.split('/').all(|part| !matches!(part, "" | "." | "..")))
        && {
            #[cfg(windows)]
            {
                !path.contains(['\\', ':'])
            }
            #[cfg(not(windows))]
            {
                true
            }
        }
}

fn beneath(path: &str, parent: &str) -> bool {
    parent.is_empty()
        || same(path, parent)
        || path
            .match_indices('/')
            .any(|(at, _)| same(&path[..at], parent))
}

fn native_components(value: &str) -> Vec<String> {
    #[cfg(windows)]
    let value = value
        .strip_prefix(r"\\?\UNC\")
        .map(|tail| format!(r"\\{tail}"))
        .unwrap_or_else(|| value.strip_prefix(r"\\?\").unwrap_or(value).to_owned());
    #[cfg(not(windows))]
    let value = value.to_owned();
    #[cfg(windows)]
    let value = value.replace('\\', "/");
    value
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn components_start_with(candidate: &[String], prefix: &[String]) -> bool {
    candidate.len() >= prefix.len()
        && candidate
            .iter()
            .zip(prefix)
            .all(|(left, right)| same(left, right))
}

/// Explicit administrative inputs outside source roots remain separately
/// authorized inputs. Inputs located within a source root cannot bypass its deny
/// merely by using a different Windows path/case spelling.
pub fn allows_absolute_input(policy: &SourcePolicy, path: &std::path::Path) -> bool {
    allows_absolute_input_checked(policy, path).unwrap_or(false)
}
pub fn allows_absolute_input_checked(
    policy: &SourcePolicy,
    path: &std::path::Path,
) -> Result<bool> {
    allows_absolute_for_purpose(policy, path, true)
}

/// Only the protected ignore reader may use this for its already selected
/// control dependency. This is not administrative or document input authority.
pub(crate) fn allows_absolute_control_input_checked(
    policy: &SourcePolicy,
    path: &std::path::Path,
) -> Result<bool> {
    allows_absolute_for_purpose(policy, path, false)
}

fn allows_absolute_for_purpose(
    policy: &SourcePolicy,
    path: &std::path::Path,
    positive_selection: bool,
) -> Result<bool> {
    let Some(path) = path.to_str() else {
        return Ok(false);
    };
    let candidate = native_components(path);
    if candidate
        .iter()
        .any(|part| matches!(part.as_str(), "." | ".."))
    {
        return Ok(false);
    }
    for root in &policy.roots {
        let prefix = native_components(&root.canonical_path);
        if denied_prefix(&candidate, &prefix)? {
            let relative = candidate[prefix.len()..].join("/");
            if !allows_for_purpose(
                policy,
                &root.canonical_path,
                &relative,
                false,
                positive_selection,
            )? {
                return Ok(false);
            }
            if !positive_selection {
                qualify(policy, &root.canonical_path, &relative, false)?;
            }
        }
    }
    Ok(true)
}

/// A model installer may enumerate declared assets only when its complete input
/// directory is disjoint from every permanent physical exclusion. This prevents
/// a permitted manifest from authorizing capture of a denied sibling asset.
pub fn allows_directory_input(policy: &SourcePolicy, path: &std::path::Path) -> bool {
    allows_directory_input_checked(policy, path).unwrap_or(false)
}
pub fn allows_directory_input_checked(
    policy: &SourcePolicy,
    path: &std::path::Path,
) -> Result<bool> {
    let Some(path) = path.to_str() else {
        return Ok(false);
    };
    let candidate = native_components(path);
    if candidate
        .iter()
        .any(|part| matches!(part.as_str(), "." | ".."))
    {
        return Ok(false);
    }
    if let Some(includes) = &policy.positive_includes {
        for root in &policy.roots {
            let source = native_components(&root.canonical_path);
            if !denied_prefix(&candidate, &source)? && !denied_prefix(&source, &candidate)? {
                continue;
            }
            // A directory input can enumerate arbitrary declared sibling assets.
            // A selected descendant or exact manifest cannot authorize that tree.
            let mut covered = false;
            for rule in includes
                .iter()
                .filter(|rule| rule.source_root_id == root.id && rule.kind == PathScope::Subtree)
            {
                let mut allowed = source.clone();
                allowed.extend(
                    rule.path
                        .split('/')
                        .filter(|part| !part.is_empty())
                        .map(str::to_owned),
                );
                if denied_prefix(&candidate, &allowed)? {
                    covered = true;
                    break;
                }
            }
            if !covered {
                return Ok(false);
            }
        }
    }
    for rule in &policy.denies {
        let Some(root) = policy
            .roots
            .iter()
            .find(|root| root.id == rule.source_root_id)
        else {
            return Ok(false);
        };
        let mut denied = native_components(&root.canonical_path);
        denied.extend(
            rule.path
                .split('/')
                .filter(|part| !part.is_empty())
                .map(str::to_owned),
        );
        if denied_prefix(&candidate, &denied)? || denied_prefix(&denied, &candidate)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Match canonical source identity, including Windows ordinal case equivalence.
/// Directory ancestors of positive includes remain traversable; a permanent
/// subtree deny prevents traversal and every later capture, regardless of ignores.
pub fn allows(policy: &SourcePolicy, root: &str, path: &str, directory: bool) -> bool {
    allows_checked(policy, root, path, directory).unwrap_or(false)
}
pub fn allows_checked(
    policy: &SourcePolicy,
    root: &str,
    path: &str,
    directory: bool,
) -> Result<bool> {
    allows_for_purpose(policy, root, path, directory, true)
}

fn allows_for_purpose(
    policy: &SourcePolicy,
    root: &str,
    path: &str,
    directory: bool,
    positive_selection: bool,
) -> Result<bool> {
    if !native_relative_path(path, directory)
        || airk_core::source_policy::validate_root_path(root).is_err()
    {
        return Ok(false);
    }
    let native_root = native_components(root);
    let Some(source) = policy.roots.iter().find(|source| {
        let configured = native_components(&source.canonical_path);
        configured.len() == native_root.len() && components_start_with(&native_root, &configured)
    }) else {
        return Ok(false);
    };
    let included = !positive_selection
        || policy.positive_includes.as_ref().is_none_or(|rules| {
            rules.iter().any(|rule| {
                rule.source_root_id == source.id
                    && (same(path, &rule.path)
                        || rule.kind == PathScope::Subtree && beneath(path, &rule.path)
                        || directory && beneath(&rule.path, path))
            })
        });
    let mut candidate = native_root;
    candidate.extend(
        path.split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_owned),
    );
    if !included {
        return Ok(false);
    }
    for rule in &policy.denies {
        let Some(owner) = policy
            .roots
            .iter()
            .find(|source| source.id == rule.source_root_id)
        else {
            return Ok(false);
        };
        let mut denied = native_components(&owner.canonical_path);
        denied.extend(
            rule.path
                .split('/')
                .filter(|part| !part.is_empty())
                .map(str::to_owned),
        );
        if (rule.kind == PathScope::Subtree || !directory && candidate.len() == denied.len())
            && denied_prefix(&candidate, &denied)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        identity::{Incarnation, Sha256Digest},
        index_catalog::{CatalogRevision, DenyId, IndexId, SourceRootId},
        source_policy::{DenyReason, DenyRule, POLICY_SCHEMA, SourceRoot},
    };
    #[test]
    fn exact_subtree_root_and_native_case_are_independent() {
        let id = SourceRootId::from_identity(Incarnation::from_bytes([1; 16]));
        let mut policy = SourcePolicy {
            schema: POLICY_SCHEMA.into(),
            index_id: IndexId::from_identity(Incarnation::from_bytes([1; 16])),
            owner_binding: Sha256Digest::from_bytes([1; 32]),
            namespace: "collections-1.4".into(),
            collection: "fixture".into(),
            epoch: CatalogRevision::INITIAL,
            roots: vec![SourceRoot {
                id,
                canonical_path: "/fixture".into(),
                aliases: vec![],
                owner: true,
            }],
            positive_includes: None,
            denies: vec![DenyRule {
                source_root_id: id,
                kind: PathScope::Subtree,
                path: "Private".into(),
                rule_id: DenyId::from_identity(Incarnation::from_bytes([1; 16])),
                reason: DenyReason::Trim,
                created_epoch: CatalogRevision::INITIAL,
                inherited_from: None,
            }],
        };
        assert!(!allows(&policy, "/fixture", "Private/new.rs", false));
        assert!(!allows(&policy, "/fixture", "Private", true));
        assert!(allows(&policy, "/fixture", "Private-other/new.rs", false));
        assert!(!allows(&policy, "/elsewhere", "Private/new.rs", false));
        assert_eq!(
            allows(&policy, "/fixture", "private/new.rs", false),
            !cfg!(any(windows, target_os = "macos"))
        );
        policy.denies[0].kind = PathScope::Exact;
        assert!(allows(&policy, "/fixture", "Private/new.rs", false));
        assert!(!allows(&policy, "/fixture", "Private", false));
        policy.denies[0].kind = PathScope::Subtree;
        policy.denies[0].path.clear();
        assert!(!allows(&policy, "/fixture", "anything.rs", false));
    }

    #[test]
    fn physical_denies_cannot_be_bypassed_through_overlapping_source_roots() {
        let primary = SourceRootId::from_identity(Incarnation::from_bytes([1; 16]));
        let child = SourceRootId::from_identity(Incarnation::from_bytes([2; 16]));
        let mut policy = SourcePolicy {
            schema: POLICY_SCHEMA.into(),
            index_id: IndexId::from_identity(Incarnation::from_bytes([1; 16])),
            owner_binding: Sha256Digest::from_bytes([1; 32]),
            namespace: "collections-1.4".into(),
            collection: "fixture".into(),
            epoch: CatalogRevision::INITIAL,
            roots: vec![
                SourceRoot {
                    id: primary,
                    canonical_path: "/fixture".into(),
                    aliases: vec![],
                    owner: true,
                },
                SourceRoot {
                    id: child,
                    canonical_path: "/fixture/docs".into(),
                    aliases: vec!["docs".into()],
                    owner: false,
                },
            ],
            positive_includes: None,
            denies: vec![DenyRule {
                source_root_id: primary,
                kind: PathScope::Subtree,
                path: "docs/private".into(),
                rule_id: DenyId::from_identity(Incarnation::from_bytes([1; 16])),
                reason: DenyReason::Trim,
                created_epoch: CatalogRevision::INITIAL,
                inherited_from: None,
            }],
        };
        assert!(!allows(
            &policy,
            "/fixture/docs",
            "private/secret.md",
            false
        ));
        assert!(!allows(
            &policy,
            "/fixture",
            "docs/private/secret.md",
            false
        ));
        assert!(allows(
            &policy,
            "/fixture/docs",
            "private-other/public.md",
            false
        ));
        policy.denies[0].source_root_id = child;
        policy.denies[0].path = "private".into();
        assert!(!allows(
            &policy,
            "/fixture",
            "docs/private/secret.md",
            false
        ));
        assert!(!allows(
            &policy,
            "/fixture/docs",
            "private/secret.md",
            false
        ));
        policy.roots[1].aliases = vec!["renamed".into()];
        assert!(!allows(
            &policy,
            "/fixture/docs",
            "private/secret.md",
            false
        ));
        policy.denies[0].kind = PathScope::Exact;
        policy.denies[0].path = "private/secret.md".into();
        assert!(!allows(
            &policy,
            "/fixture",
            "docs/private/secret.md",
            false
        ));
        assert!(allows(
            &policy,
            "/fixture",
            "docs/private/secret.md/sibling",
            false
        ));
        assert!(!allows_absolute_input(
            &policy,
            std::path::Path::new("/fixture/docs/private/secret.md")
        ));
        assert!(allows_absolute_input(
            &policy,
            std::path::Path::new("/outside/management.json")
        ));
        assert!(!allows_directory_input(
            &policy,
            std::path::Path::new("/fixture/docs")
        ));
        assert!(!allows_directory_input(
            &policy,
            std::path::Path::new("/fixture/docs/private")
        ));
        assert!(allows_directory_input(
            &policy,
            std::path::Path::new("/fixture/docs/public-model")
        ));
    }

    #[cfg(not(windows))]
    #[test]
    fn posix_native_components_preserve_literal_root_and_file_characters() {
        let primary = SourceRootId::from_identity(Incarnation::from_bytes([1; 16]));
        let policy = SourcePolicy {
            schema: POLICY_SCHEMA.into(),
            index_id: IndexId::from_identity(Incarnation::from_bytes([1; 16])),
            owner_binding: Sha256Digest::from_bytes([1; 32]),
            namespace: "collections-1.4".into(),
            collection: "fixture".into(),
            epoch: CatalogRevision::INITIAL,
            roots: vec![SourceRoot {
                id: primary,
                canonical_path: "/fixture\\literal".into(),
                aliases: vec![],
                owner: true,
            }],
            positive_includes: None,
            denies: vec![DenyRule {
                source_root_id: primary,
                kind: PathScope::Subtree,
                path: "private".into(),
                rule_id: DenyId::from_identity(Incarnation::from_bytes([1; 16])),
                reason: DenyReason::Trim,
                created_epoch: CatalogRevision::INITIAL,
                inherited_from: None,
            }],
        };
        assert!(allows(
            &policy,
            "/fixture\\literal",
            "outside:name.rs",
            false
        ));
        assert!(allows(
            &policy,
            "/fixture\\literal",
            r"outside\name.rs",
            false
        ));
        assert!(!allows(
            &policy,
            "/fixture/literal",
            "outside:name.rs",
            false
        ));
        assert!(!allows(
            &policy,
            "/fixture\\literal",
            "private/name:with-colon.rs",
            false
        ));
        assert!(!allows_absolute_input(
            &policy,
            std::path::Path::new("/fixture\\literal/private/name:with-colon.rs")
        ));
    }

    #[test]
    fn unknown_name_equivalence_is_not_a_successful_partial_result() -> Result<()> {
        assert!(deny_same("Private", "private")? == cfg!(any(windows, target_os = "macos")));
        if cfg!(any(windows, target_os = "macos")) {
            assert!(deny_same("caf\u{e9}", "cafe\u{301}").is_err());
            assert!(deny_same("Stra\u{df}e", "STRASSE").is_err());
            assert!(
                denied_prefix(
                    &["source".into(), "caf\u{e9}".into()],
                    &["source".into(), "cafe".into()]
                )
                .is_err()
            );
        }
        // A disjoint component proves the paths disjoint even if another
        // component's comparison would be unsupported.
        assert!(!denied_prefix(
            &["allowed".into(), "caf\u{e9}".into()],
            &["private".into(), "cafe".into()]
        )?);
        Ok(())
    }

    #[test]
    fn directory_inputs_require_complete_positive_subtrees_not_exact_assets() -> Result<()> {
        use airk_core::index_catalog::ScopeAtom;
        let root = SourceRootId::from_identity(Incarnation::from_bytes([1; 16]));
        let mut policy = SourcePolicy {
            schema: POLICY_SCHEMA.into(),
            index_id: IndexId::from_identity(Incarnation::from_bytes([2; 16])),
            owner_binding: Sha256Digest::from_bytes([3; 32]),
            namespace: "collections-1.4".into(),
            collection: "fixture".into(),
            epoch: CatalogRevision::INITIAL,
            roots: vec![SourceRoot {
                id: root,
                canonical_path: "/fixture".into(),
                aliases: vec![],
                owner: true,
            }],
            positive_includes: Some(vec![ScopeAtom {
                source_root_id: root,
                kind: PathScope::Exact,
                path: "model/manifest.json".into(),
            }]),
            denies: vec![],
        };
        assert!(allows_absolute_input_checked(
            &policy,
            std::path::Path::new("/fixture/model/manifest.json")
        )?);
        assert!(!allows_absolute_input_checked(
            &policy,
            std::path::Path::new("/fixture/model/unselected.bin")
        )?);
        for path in ["/fixture", "/fixture/model", "/", "/fixture/unselected"] {
            assert!(
                !allows_directory_input_checked(&policy, std::path::Path::new(path))?,
                "{path}"
            );
        }
        policy.positive_includes.as_mut().unwrap()[0] = ScopeAtom {
            source_root_id: root,
            kind: PathScope::Subtree,
            path: "model".into(),
        };
        assert!(allows_directory_input_checked(
            &policy,
            std::path::Path::new("/fixture/model")
        )?);
        assert!(allows_directory_input_checked(
            &policy,
            std::path::Path::new("/fixture/model/nested")
        )?);
        assert!(!allows_directory_input_checked(
            &policy,
            std::path::Path::new("/fixture")
        )?);
        policy.denies.push(DenyRule {
            source_root_id: root,
            kind: PathScope::Exact,
            path: "model/secret.bin".into(),
            rule_id: DenyId::from_identity(Incarnation::from_bytes([4; 16])),
            reason: DenyReason::Trim,
            created_epoch: policy.epoch,
            inherited_from: None,
        });
        assert!(!allows_directory_input_checked(
            &policy,
            std::path::Path::new("/fixture/model")
        )?);
        assert!(allows_directory_input_checked(
            &policy,
            std::path::Path::new("/fixture/model/nested")
        )?);
        assert!(allows_directory_input_checked(
            &policy,
            std::path::Path::new("/outside/model")
        )?);
        Ok(())
    }
}
