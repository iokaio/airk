// SPDX-License-Identifier: Apache-2.0
//! A bounded installation continuation inside the existing metadata owner.
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) fn installed_root(executable: &Path) -> Option<PathBuf> {
    let name = executable.file_name()?.to_str()?;
    if name != "airk" && name != "airk.exe" {
        return None;
    }
    let parent = executable.parent()?;
    let payload = if parent.file_name()? == "MacOS" {
        let contents = parent.parent()?;
        let bundle = contents.parent()?;
        if contents.file_name()? != "Contents" || bundle.file_name()? != "AIRK.app" {
            return None;
        }
        bundle.parent()?
    } else {
        parent
    };
    let identity = payload.file_name()?.to_str()?;
    if identity.len() != 64
        || !identity
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let runtimes = payload.parent()?;
    if runtimes.file_name()? != "runtimes" {
        return None;
    }
    Some(runtimes.parent()?.to_path_buf())
}

pub(crate) struct Continuation {
    root: Option<PathBuf>,
    next: Instant,
    pending: bool,
}
impl Continuation {
    pub(crate) fn new(executable: Option<PathBuf>, now: Instant) -> Self {
        Self {
            root: executable.as_deref().and_then(installed_root),
            next: now,
            pending: false,
        }
    }
    pub(crate) fn pending(&self) -> bool {
        self.pending
    }
    pub(crate) fn tick(
        &mut self,
        owner: &airk_store::service::ServiceLease,
        namespace: airk_core::identity::Incarnation,
        now: Instant,
    ) -> anyhow::Result<bool> {
        self.tick_with(owner.root(), now, |root, _control| {
            super::continue_clean_break(root, owner, namespace)
        })
    }

    fn tick_with(
        &mut self,
        control: &Path,
        now: Instant,
        observe: impl FnOnce(&Path, &Path) -> anyhow::Result<bool>,
    ) -> anyhow::Result<bool> {
        if now < self.next {
            return Ok(self.pending);
        }
        self.next = now + Duration::from_secs(1);
        if let Some(root) = &self.root {
            // Native admission rechecks the exact selected executable, control,
            // state contract and reset operation under a nonblocking install lock.
            match observe(root, control) {
                Ok(pending) => self.pending = pending,
                Err(error) => {
                    self.pending =
                        super::clean_break_continuation_pending(root, control).unwrap_or(true);
                    return Err(error);
                }
            }
        }
        Ok(self.pending)
    }
}

#[cfg(test)]
#[path = "runtime_clean_break_background_tests.rs"]
mod tests;
