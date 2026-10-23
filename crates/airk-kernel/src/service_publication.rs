// SPDX-License-Identifier: Apache-2.0
//! Replacement coordination; selection, ownership and bytes stay in store.
use crate::Kernel;
use airk_core::identity::{RevisionToken, Sha256Digest};
use airk_store::{quota::Quota, service::collection};
use anyhow::{Context, Result, ensure};
use std::sync::Arc;

impl Kernel {
    pub fn service_rebuild_receipt(&self) -> Result<Option<collection::RebuildReceipt>> {
        self.service_collection
            .as_ref()
            .map(|collection| collection.last_rebuild_receipt())
            .transpose()
            .map(Option::flatten)
    }
    pub fn retire_service_incarnations(&self) -> Result<collection::RetirementReport> {
        self.service_collection
            .as_ref()
            .context("service collection required")?
            .retire()
    }
    /// One request selects and pins its incarnation atomically with manifest
    /// handoff. Ordinary/legacy kernels remain unchanged; idle kernels pin none.
    pub fn refresh_service_snapshot(&self) -> Result<Self> {
        let Some(collection) = &self.service_collection else {
            return Ok(self.clone());
        };
        let selection = Arc::new(collection.select()?);
        let mut selected = self.clone();
        selected.store = selection.store.clone();
        selected.service_selection = Some(selection);
        Ok(selected)
    }
    pub fn rebuild_service(&self, job: Sha256Digest) -> Result<RevisionToken> {
        let collection = self
            .service_collection
            .as_ref()
            .context("replacement rebuilding requires a service project owner")?;
        collection.recover_abandoned_replacement()?;
        let previous = self.refresh_service_snapshot()?;
        let scope = previous.current_scope()?;
        let expected = scope.as_ref().and_then(|scope| {
            previous
                .store
                .current_read(scope)
                .and_then(|read| read.token())
                .ok()
        });
        let mut replacement =
            collection.begin_replacement(crate::service_broker::new_instance_id()?, job)?;
        let mut next = self.clone();
        next.store = replacement.store();
        next.service_selection = None;
        let settings = collection::copy_settings(
            &previous.store,
            &next.store,
            &self.root_id(),
            &Quota::open(&self.data_root)?,
        )?;
        next.rebuild_extraction_policy()?;
        next.vector_selection()?;
        next.scan_limits()?;
        next.configured_roots()?;
        next.reindex()?;
        // Bounded pages authenticate retained sources/extractions and identify
        // one exact target revision. No live-source substitution is permitted.
        let mut after = None;
        let mut revision = None;
        loop {
            let report = next.diagnose_current(revision, after.as_deref(), 32, 64 << 20)?;
            ensure!(
                report.failed_objects == 0 && report.skipped_large == 0,
                "replacement retained verification incomplete"
            );
            revision = Some(report.publication.token);
            if report.complete {
                break;
            }
            after = Some(
                report
                    .next_after
                    .context("replacement verification continuation missing")?,
            );
        }
        let coverage = next.coverage(true)?;
        ensure!(
            coverage.indexed
                && coverage.scan_complete
                && !coverage.budget_exhausted
                && !coverage.source_configuration_pending
                && coverage.stale == 0
                && coverage.missing == 0
                && coverage.extraction_failed == 0
                && coverage.capacity_blocked == 0
                && coverage.unreadable == 0
                && coverage.pending_removals == 0
                && coverage.current == coverage.eligible,
            "sources or configuration changed during replacement; keep the current index and reconcile before retrying"
        );
        ensure!(
            collection::settings_digest(&previous.store, &self.root_id())? == settings
                && collection::settings_digest(&next.store, &self.root_id())? == settings,
            "replacement configuration plan expired"
        );
        let token = replacement.commit(expected, settings)?;
        Ok(RevisionToken {
            incarnation: token.incarnation,
            generation: token.sequence,
            revision: token.digest,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::SearchQuery;
    fn query(text: &str) -> SearchQuery {
        SearchQuery {
            text: text.into(),
            path_prefix: None,
            limit: 5,
            max_bytes: 4096,
        }
    }

    #[test]
    fn service_replacement_switches_fresh_incarnation_while_old_request_remains_readable()
    -> Result<()> {
        let source = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(
            source.path().join("memo.md"),
            "# Amber harbor\nOriginal amber harbor source.\n",
        )?;
        let kernel =
            Kernel::open_service_collection(source.path(), Some(data.path().into()), "fixture")?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let policy = kernel.configure_usage(airk_core::usage::RecordingMode::AggregatesOnly)?;
        let usage_directory = kernel.store.managed_collection_directory()?.to_path_buf();
        airk_store::usage::UsageStore::open(&usage_directory, &kernel.usage_scope(), &policy)?
            .context("usage ledger missing")?;
        let usage = std::fs::read(usage_directory.join("usage.sqlite3"))?;
        let old = kernel.refresh_service_snapshot()?;
        let old_token = old
            .status()?
            .publication
            .context("old publication missing")?
            .token;
        assert!(!old.search(&query("amber"))?.hits.is_empty());
        let changed = "# Cobalt harbor\nReplacement cobalt harbor source.\n";
        std::fs::write(source.path().join("memo.md"), changed)?;
        let token = kernel.rebuild_service(Sha256Digest::from_bytes([11; 32]))?;
        assert_ne!(old_token.incarnation, token.incarnation);
        let current = kernel.refresh_service_snapshot()?;
        assert_eq!(
            current
                .status()?
                .publication
                .context("new publication missing")?
                .token,
            token
        );
        assert!(!current.search(&query("cobalt"))?.hits.is_empty());
        assert!(current.search(&query("amber"))?.hits.is_empty());
        assert!(!old.search(&query("amber"))?.hits.is_empty());
        assert!(!current.vectors_enabled()?);
        assert_eq!(std::fs::read(usage_directory.join("usage.sqlite3"))?, usage);
        assert_eq!(
            current.usage_policy()?.mode,
            airk_core::usage::RecordingMode::AggregatesOnly
        );
        assert_eq!(
            std::fs::read_to_string(source.path().join("memo.md"))?,
            changed
        );
        let reopened =
            Kernel::open_service_collection(source.path(), Some(data.path().into()), "fixture")?;
        assert_eq!(
            reopened
                .status()?
                .publication
                .context("reopened publication missing")?
                .token,
            token
        );
        let old_directory = old.store.directory().to_path_buf();
        assert_eq!(kernel.retire_service_incarnations()?.removed_directories, 0);
        drop(old);
        assert_eq!(kernel.retire_service_incarnations()?.removed_directories, 1);
        assert!(!old_directory.exists());
        Ok(())
    }

    #[test]
    fn service_replacement_quota_failure_preserves_last_readable_head_and_source() -> Result<()> {
        let source = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(source.path().join("memo.md"), "Original quartz harbor.\n")?;
        let kernel =
            Kernel::open_service_collection(source.path(), Some(data.path().into()), "fixture")?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let before = kernel
            .status()?
            .publication
            .context("publication missing")?
            .token;
        let quota = Quota::open(data.path())?;
        let used = quota.status()?;
        let cap = used.used_bytes + used.reserved_bytes + (2 << 20);
        quota.configure(crate::storage::QuotaPolicy {
            root_bytes: cap,
            default_collection_bytes: cap,
            collections: Default::default(),
        })?;
        std::fs::write(source.path().join("memo.md"), "Later original user edit.\n")?;
        assert!(
            kernel
                .rebuild_service(Sha256Digest::from_bytes([12; 32]))
                .is_err()
        );
        assert_eq!(
            kernel
                .refresh_service_snapshot()?
                .status()?
                .publication
                .context("prior publication missing")?
                .token,
            before
        );
        assert!(!kernel.search(&query("quartz"))?.hits.is_empty());
        let reopened =
            Kernel::open_service_collection(source.path(), Some(data.path().into()), "fixture")?;
        assert_eq!(
            reopened
                .status()?
                .publication
                .context("preserved publication missing")?
                .token,
            before
        );
        reopened.retire_service_incarnations()?;
        assert_eq!(
            std::fs::read_to_string(source.path().join("memo.md"))?,
            "Later original user edit.\n"
        );
        Ok(())
    }
}
