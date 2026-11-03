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

/// One request's direct-path policy checks. Loads only ancestor ignore metadata;
/// never enumerates siblings or opens a source document. Protected capture is
/// still required after admission. Permanent native policy is an additional fence.
pub struct SelectedSourcePolicy {
    root: std::path::PathBuf,
    data: std::path::PathBuf,
    matcher: IncrementalIgnore,
    credentials: CredentialDefaults,
}

impl SelectedSourcePolicy {
    pub fn new(root: &Path, data: &Path) -> Self {
        Self {
            root: root.into(),
            data: data.into(),
            matcher: builder(root).build_matchers().remove(0),
            credentials: CredentialDefaults::new(root),
        }
    }

    pub fn allows(&mut self, logical: &str) -> Result<bool> {
        ensure!(
            !logical.is_empty()
                && logical.len() <= 4096
                && !logical.contains(['\\', ':', '\0'])
                && logical
                    .split('/')
                    .all(|part| !matches!(part, "" | "." | "..")),
            "invalid selected source locator"
        );
        let parts: Vec<_> = logical.split('/').collect();
        ensure!(
            parts.len() <= 256,
            "selected source depth exceeds policy budget"
        );
        let mut relative = std::path::PathBuf::new();
        for (index, part) in parts.iter().enumerate() {
            relative.push(part);
            if crate::excluded(part) || self.root.join(&relative).starts_with(&self.data) {
                return Ok(false);
            }
            let is_dir = index + 1 != parts.len();
            let (matched, error) = self.matcher.matched_with_errors(&relative, is_dir);
            if let Some(error) = error {
                return Err(error.into());
            }
            if matched.is_ignore() || self.credentials.excluded(&relative, is_dir) {
                return Ok(false);
            }
        }
        Ok(true)
    }
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

#[cfg(test)]
mod selected_tests {
    use super::*;

    #[test]
    fn selected_policy_matches_ancestor_ignores_without_discovery() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().canonicalize()?;
        std::fs::create_dir(root.join("nested"))?;
        std::fs::write(root.join(".gitignore"), "ignored/\n*.secret\n")?;
        std::fs::write(root.join(".airkignore"), "!.npmrc\n")?;
        std::fs::write(root.join("nested/.ignore"), "local.txt\n")?;
        let mut policy = SelectedSourcePolicy::new(&root, &root.join("private-data"));
        for path in [
            "ignored/missing.rs",
            "nested/local.txt",
            "nested/x.secret",
            "node_modules/a.js",
            ".env",
            "nested/.env.local",
            "private-data/a.txt",
            ".netrc",
        ] {
            assert!(!policy.allows(path)?, "{path}");
        }
        for path in [".npmrc", "src/not-yet-created.rs", "nested/visible.txt"] {
            assert!(policy.allows(path)?, "{path}");
        }
        assert!(policy.allows("../outside").is_err());
        Ok(())
    }
}
