// SPDX-License-Identifier: Apache-2.0
//! Shared source-discovery ignore semantics for bounded directory planning.
use anyhow::{Result, ensure};
use ignore::{
    IncrementalIgnore, WalkBuilder,
    gitignore::{Gitignore, GitignoreBuilder},
};
use std::path::{Component, Path};

/// Common credential files skipped by default. Unlike the hard exclusions,
/// an explicit negation such as `!.npmrc` in `.airkignore` re-includes one.
/// This is a convenience default, not secret detection.
pub const CREDENTIAL_DEFAULTS: &[&str] = &[
    ".npmrc",
    ".netrc",
    "_netrc",
    ".pypirc",
    ".git-credentials",
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    "*.p12",
    "*.pfx",
    "*.jks",
    "*.keystore",
    "kubeconfig",
    "**/.kube/config",
    "*.tfstate",
    "*.tfstate.backup",
    "**/.docker/config.json",
];

pub(crate) fn builder(root: &Path) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);
    builder
        .require_git(false)
        .hidden(false)
        .follow_links(false)
        .add_custom_ignore_filename(".airkignore");
    builder
}

/// One plan's directory-only ignore snapshot. The caller retains directory,
/// entry, path, depth and process-memory admission; this is not a source walk.
/// Rules are cached by parent directory and reloaded by constructing a new plan.
/// No source document or derived index is opened by this adapter.
pub struct DirectoryIgnore {
    matcher: IncrementalIgnore,
}

impl DirectoryIgnore {
    pub fn new(root: &Path) -> Self {
        Self {
            matcher: builder(root).build_matchers().remove(0),
        }
    }

    /// Match a potential directory against its parents' rules before accessing it.
    /// Callers may query only direct children of already admitted directories;
    /// this keeps matcher caches inside their existing directory-plan budget.
    pub fn ignored(&mut self, path: &Path) -> Result<bool> {
        let relative = path.strip_prefix(self.matcher.root())?;
        ensure!(
            relative
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
            "watch ignore path is outside its source root"
        );
        let (matched, error) = self.matcher.matched_with_errors(relative, true);
        if let Some(error) = error {
            return Err(error.into());
        }
        Ok(matched.is_ignore())
    }
}

/// Applies [`CREDENTIAL_DEFAULTS`] after the user's ignore files so that an
/// explicit whitelist rule for the same path takes precedence.
pub(crate) struct CredentialDefaults {
    defaults: Gitignore,
    user: IncrementalIgnore,
}

impl CredentialDefaults {
    pub(crate) fn new(root: &Path) -> Self {
        let mut defaults = GitignoreBuilder::new(root);
        for pattern in CREDENTIAL_DEFAULTS {
            defaults
                .add_line(None, pattern)
                .expect("built-in credential patterns are valid");
        }
        Self {
            defaults: defaults
                .build()
                .expect("built-in credential patterns compile"),
            user: builder(root).build_matchers().remove(0),
        }
    }

    /// `relative` is root-relative, as produced by the source walk.
    pub(crate) fn excluded(&mut self, relative: &Path, is_dir: bool) -> bool {
        self.defaults.matched(relative, is_dir).is_ignore()
            && !self.user.matched(relative, is_dir).is_whitelist()
    }
}
