// SPDX-License-Identifier: Apache-2.0
//! Service source publication and semantic turns have independent durable jobs.
use crate::Kernel;
use anyhow::{Result, ensure};
impl Kernel {
    pub fn index_service_sources(&self) -> Result<airk_ingest::IndexReport> {
        ensure!(
            self.is_service_collection(),
            "source-only service indexing requires its project owner"
        );
        self.index_current(false, true)
    }
    /// One bounded 32-artifact semantic batch; the soft deadline is checked at
    /// publication boundaries. Native inference still has its own hard budgets.
    pub fn service_enrichment_turn(&self) -> Result<airk_ingest::IndexReport> {
        ensure!(
            self.is_service_collection(),
            "service enrichment requires its project owner"
        );
        self.resume_current_enrichment_batches_bounded(1, Some(std::time::Duration::from_secs(30)))
            .map(|(_, report)| report)
    }
}
