// SPDX-License-Identifier: Apache-2.0
//! Full indexing preserves native storage; old ownership requires fresh setup.
use crate::{FileStore, WriterLock, quota::Quota};
use anyhow::{Result, ensure};
impl FileStore {
    pub fn rebuild_obsolete_index(
        &self,
        writer: &WriterLock,
        quota: &Quota,
        root: &str,
        collection: &str,
    ) -> Result<bool> {
        crate::maintenance::check_writer(self, writer, quota)?;
        super::database::reject_legacy_collection(self.directory())?;
        if self
            .directory()
            .join(super::database::OWNER_FILE)
            .try_exists()?
        {
            let scope = super::database::owned_scope(self.directory())?;
            ensure!(
                scope.root == root && scope.collection == collection,
                "foreign current ownership header"
            );
        }
        Ok(false)
    }
}
