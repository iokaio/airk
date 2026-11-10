// SPDX-License-Identifier: Apache-2.0
//! Read side of the explicit recovery layout handoff: the immutable
//! `rebuild-layout-<operation>.json` receipt and the successor binding check.
//! The writer and admission stay in `runtime_install::layout::successor`.
use super::{Layout, MAX_LAYOUT, preflight_layout};
use crate::install_files::read_bounded;
use airk_core::identity::{Incarnation, Sha256Digest, framed_digest};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
const MAX_TRANSITION: u64 = MAX_LAYOUT * 8;
// Match the canonical Store private-path admission. Recovery does not widen it.
pub(crate) const MAX_PRESERVED_PATHS: usize = 16;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Transition {
    pub(crate) schema: u32,
    pub(crate) operation: Incarnation,
    pub(crate) install: PathBuf,
    /// Preserve the original encoding as recovery evidence as well as its value.
    pub(crate) previous_layout: String,
    pub(crate) successor: Layout,
    pub(crate) preserved_private_paths: Vec<PathBuf>,
    pub(crate) digest: Sha256Digest,
}

pub(crate) fn receipt_name(operation: Incarnation) -> String {
    format!("rebuild-layout-{operation}.json")
}

pub(crate) fn successor_state(root: &Path, operation: Incarnation) -> PathBuf {
    root.join(format!("rebuild-state-{operation}"))
}

pub(crate) fn operation_for(layout: &Layout) -> Result<Option<Incarnation>> {
    if layout.state.parent() != Some(layout.install.as_path()) {
        return Ok(None);
    }
    let Some(name) = layout.state.file_name().and_then(|name| name.to_str()) else {
        return Ok(None);
    };
    let Some(operation) = name.strip_prefix("rebuild-state-") else {
        return Ok(None);
    };
    Ok(Some(
        operation
            .parse()
            .context("invalid successor-state operation")?,
    ))
}

impl Transition {
    pub(crate) fn checksum(&self) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"airk/rebuild-layout/1",
            &[&serde_json::to_vec(&(
                self.schema,
                self.operation,
                &self.install,
                &self.previous_layout,
                &self.successor,
                &self.preserved_private_paths,
            ))?],
        ))
    }

    pub(crate) fn previous(&self) -> Result<Layout> {
        Ok(serde_json::from_str(&self.previous_layout)?)
    }

    pub(crate) fn validate(&self, root: &Path) -> Result<()> {
        let previous = self.previous()?;
        ensure!(
            self.schema == 1
                && self.digest == self.checksum()?
                && self.install == root.canonicalize()?
                && previous.schema == 1
                && previous.install == self.install
                && previous.ready
                && self.successor.schema == 1
                && self.successor.install == self.install
                && self.successor.state == successor_state(&self.install, self.operation)
                && self.successor.state != previous.state
                && self.successor.channel == previous.channel
                && self.successor.logs == Some(self.successor.state.join("logs"))
                && !self.successor.ready
                && self.successor.managed_catalog_origin.is_none(),
            "successor layout operation or preserved layout changed"
        );
        ensure!(
            !self.preserved_private_paths.is_empty()
                && self.preserved_private_paths.len() <= MAX_PRESERVED_PATHS
                && self.preserved_private_paths.contains(&previous.install)
                && self.preserved_private_paths.contains(&previous.state),
            "successor layout omitted preserved private state"
        );
        for path in &self.preserved_private_paths {
            let spelling = path.to_str().context("non-UTF-8 preserved state path")?;
            ensure!(
                path.is_absolute()
                    && spelling.len() <= 4096
                    && !spelling.chars().any(char::is_control)
                    && !path
                        .components()
                        .any(|part| matches!(part, Component::ParentDir | Component::CurDir)),
                "invalid preserved private state path"
            );
        }
        require_private_union(&self.preserved_private_paths, &self.successor.state)?;
        ensure!(
            self.previous_layout.len() as u64 <= MAX_LAYOUT
                && serde_json::to_vec(self)?.len() as u64 <= MAX_TRANSITION,
            "successor layout receipt exceeds bound"
        );
        preflight_layout(&self.successor)
    }
}

pub(crate) fn read(root: &Path, operation: Incarnation) -> Result<Transition> {
    let transition: Transition = serde_json::from_slice(&read_bounded(
        &root.join(receipt_name(operation)),
        MAX_TRANSITION,
    )?)?;
    transition.validate(root)?;
    ensure!(
        transition.operation == operation,
        "successor receipt operation changed"
    );
    Ok(transition)
}

/// This receipt identifies a transition, not its execution authority. Only the
/// activation owner's opaque permit can invoke the owning handoff below.
pub(super) fn validate_binding(root: &Path, layout: &Layout) -> Result<()> {
    if let Some(operation) = operation_for(layout)? {
        let transition = read(root, operation)?;
        ensure!(
            *layout == transition.successor || *layout == transition.successor.owner(),
            "selected successor layout differs from its preserved operation"
        );
    }
    Ok(())
}

pub(super) fn preserved_private_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let layout = Layout::read(root)?.context("runtime state layout missing")?;
    let Some(operation) = operation_for(&layout)? else {
        return Ok(Vec::new());
    };
    Ok(read(root, operation)?.preserved_private_paths)
}

pub(crate) fn require_private_union(preserved: &[PathBuf], state: &Path) -> Result<()> {
    let mut complete = preserved.to_vec();
    complete.push(state.into());
    complete.push(state.join("logs"));
    complete.sort();
    complete.dedup();
    ensure!(
        complete.len() <= MAX_PRESERVED_PATHS,
        "rebuild private-path union exceeds the existing sixteen-path bound"
    );
    Ok(())
}
