// SPDX-License-Identifier: Apache-2.0
//! Shared source-discovery ignore semantics for bounded directory planning.
use anyhow::{Result, ensure};
use ignore::{
    IncrementalIgnore, WalkBuilder,
    gitignore::{Gitignore, GitignoreBuilder},
};
use std::path::{Component, Path};

/// Refuse reserved roots before loading their ignore controls or source bytes.
pub(crate) fn require_source_root(root: &Path) -> Result<()> {
    ensure!(
        !reserved_root(root)?,
        "AIRK-private source root is excluded"
    );
    Ok(())
}

fn reserved_root(root: &Path) -> Result<bool> {
    let reserved = |path: &Path| -> Result<bool> {
        Ok(airk_core::source_policy::source_root_is_reserved(
            path.to_str()
                .ok_or_else(|| anyhow::anyhow!("non-UTF-8 source root"))?,
        ))
    };
    if reserved(root)? {
        return Ok(true);
    }
    reserved(&root.canonicalize()?)
}

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
    matcher: SourceMatcher,
}

/// One request's direct-path policy checks. Loads only ancestor ignore metadata;
/// never enumerates siblings or opens a source document. Protected capture is
/// still required after admission. Permanent native policy is an additional fence.
pub struct SelectedSourcePolicy {
    root: std::path::PathBuf,
    data: std::path::PathBuf,
    matcher: SourceMatcher,
    credentials: CredentialDefaults,
}

impl SelectedSourcePolicy {
    pub fn new(root: &Path, data: &Path) -> Self {
        Self {
            root: root.into(),
            data: data.into(),
            matcher: SourceMatcher::ordinary(root),
            credentials: CredentialDefaults::new(root),
        }
    }

    pub fn with_source_policy(
        root: &Path,
        data: &Path,
        policy: Option<&airk_core::source_policy::SourcePolicy>,
    ) -> Result<Self> {
        Ok(Self {
            root: root.into(),
            data: data.into(),
            matcher: SourceMatcher::new(root, policy)?,
            credentials: if policy.is_some_and(crate::permanent_policy::restricts) {
                CredentialDefaults::guarded(root)
            } else {
                CredentialDefaults::new(root)
            },
        })
    }

    pub fn allows(&mut self, logical: &str) -> Result<bool> {
        match &self.matcher {
            SourceMatcher::Blocked(_) => return Ok(false),
            SourceMatcher::Unavailable(_) => anyhow::bail!("source root boundary unavailable"),
            _ => (),
        }
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
            let matched = self.matcher.decision(&relative, is_dir)?;
            if matched == crate::policy_ignore::Decision::Ignore
                || self.credentials.excluded_with_whitelist(
                    &relative,
                    is_dir,
                    matched == crate::policy_ignore::Decision::Whitelist,
                )
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl DirectoryIgnore {
    pub fn new(root: &Path) -> Self {
        Self {
            matcher: SourceMatcher::ordinary(root),
        }
    }

    pub fn with_source_policy(
        root: &Path,
        policy: Option<&airk_core::source_policy::SourcePolicy>,
    ) -> Result<Self> {
        Ok(Self {
            matcher: SourceMatcher::new(root, policy)?,
        })
    }

    /// Match a potential directory against its parents' rules before accessing it.
    /// Callers may query only direct children of already admitted directories;
    /// this keeps matcher caches inside their existing directory-plan budget.
    pub fn ignored(&mut self, path: &Path) -> Result<bool> {
        let relative = path.strip_prefix(self.matcher.root())?.to_path_buf();
        ensure!(
            relative
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
            "watch ignore path is outside its source root"
        );
        Ok(self.matcher.decision(&relative, true)? == crate::policy_ignore::Decision::Ignore)
    }
}

enum SourceMatcher {
    Blocked(std::path::PathBuf),
    Unavailable(std::path::PathBuf),
    Ordinary(IncrementalIgnore),
    Protected(Box<crate::policy_ignore::ProtectedIgnore>),
}
impl SourceMatcher {
    fn ordinary(root: &Path) -> Self {
        match reserved_root(root) {
            Ok(true) => return Self::Blocked(root.into()),
            Err(_) => return Self::Unavailable(root.into()),
            Ok(false) => (),
        }
        Self::Ordinary(builder(root).build_matchers().remove(0))
    }
    fn new(root: &Path, policy: Option<&airk_core::source_policy::SourcePolicy>) -> Result<Self> {
        if reserved_root(root)? {
            return Ok(Self::Blocked(root.into()));
        }
        Ok(
            match policy.filter(|policy| crate::permanent_policy::restricts(policy)) {
                Some(policy) => Self::Protected(Box::new(
                    crate::policy_ignore::ProtectedIgnore::new(root, policy)?,
                )),
                None => Self::ordinary(root),
            },
        )
    }
    fn root(&self) -> &Path {
        match self {
            Self::Blocked(root) | Self::Unavailable(root) => root,
            Self::Ordinary(matcher) => matcher.root(),
            Self::Protected(matcher) => matcher.root(),
        }
    }
    fn decision(&mut self, path: &Path, directory: bool) -> Result<crate::policy_ignore::Decision> {
        use crate::policy_ignore::Decision;
        match self {
            Self::Blocked(_) => Ok(Decision::Ignore),
            Self::Unavailable(_) => anyhow::bail!("source root boundary unavailable"),
            Self::Protected(matcher) => matcher.decision(path, directory),
            Self::Ordinary(matcher) => {
                let (matched, error) = matcher.matched_with_errors(path, directory);
                if let Some(error) = error {
                    return Err(error.into());
                }
                Ok(if matched.is_ignore() {
                    Decision::Ignore
                } else if matched.is_whitelist() {
                    Decision::Whitelist
                } else {
                    Decision::None
                })
            }
        }
    }
}

/// Applies [`CREDENTIAL_DEFAULTS`] after the user's ignore files so that an
/// explicit whitelist rule for the same path takes precedence.
pub(crate) struct CredentialDefaults {
    defaults: Gitignore,
    user: Option<IncrementalIgnore>,
}

impl CredentialDefaults {
    pub(crate) fn new(root: &Path) -> Self {
        let mut value = Self::guarded(root);
        if reserved_root(root).unwrap_or(true) {
            return value;
        }
        value.user = Some(builder(root).build_matchers().remove(0));
        value
    }

    pub(crate) fn guarded(root: &Path) -> Self {
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
            user: None,
        }
    }

    /// `relative` is root-relative, as produced by the source walk.
    pub(crate) fn excluded(&mut self, relative: &Path, is_dir: bool) -> bool {
        self.defaults.matched(relative, is_dir).is_ignore()
            && !self
                .user
                .as_mut()
                .is_some_and(|user| user.matched(relative, is_dir).is_whitelist())
    }

    pub(crate) fn excluded_with_whitelist(
        &mut self,
        relative: &Path,
        is_dir: bool,
        whitelist: bool,
    ) -> bool {
        !whitelist && self.excluded(relative, is_dir)
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
