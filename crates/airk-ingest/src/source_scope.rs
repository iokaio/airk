// SPDX-License-Identifier: Apache-2.0
//! Shared source-discovery ignore semantics for bounded directory planning.
use anyhow::{Result, ensure};
use ignore::{IncrementalIgnore, WalkBuilder};
use std::path::{Component, Path};

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
