// SPDX-License-Identifier: Apache-2.0
//! Fresh-setup boundary. Old derived databases are never read or imported.
use crate::FileStore;
use anyhow::{Result, ensure};
use std::path::Path;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexFormat {
    Empty,
    Current,
    Obsolete,
    ResetRequired,
}
pub(crate) fn ensure_ready(directory: &Path) -> Result<()> {
    ensure!(
        !directory.join("index-reset.json").try_exists()?,
        "fresh setup reset is incomplete"
    );
    Ok(())
}
impl FileStore {
    pub fn ensure_index_ready(&self) -> Result<()> {
        ensure_ready(self.directory())
    }
}
