// SPDX-License-Identifier: Apache-2.0
//! Replacement coordination; selection, ownership and bytes stay in store.
use crate::Kernel;
use airk_core::identity::{RevisionToken, Sha256Digest};
use airk_store::service::collection;
use anyhow::{Context, Result, ensure};
use std::sync::Arc;

impl Kernel {
    pub fn pending_service_retirements(&self) -> Result<usize> {
        self.service_collection
            .as_ref()
            .context("service collection required")?
            .pending_retirements()
    }
    pub fn service_recovery_required(&self) -> Result<bool> {
        ensure!(
            self.service_collection.is_some(),
            "service collection required"
        );
        let selected = self.refresh_service_snapshot()?;
        if selected.store.runtime_bootstrap()?.is_some() {
            return Ok(true);
        }
        let Some(scope) = selected.current_scope()? else {
            return Ok(false);
        };
        Ok(selected.store.redb_read(&scope)?.snapshot().phase()?
            != airk_store::current::TransactionPhase::Ready)
    }
    pub fn recover_service_publication(&self) -> Result<Option<RevisionToken>> {
        ensure!(
            self.service_collection.is_some(),
            "service collection required"
        );
        crate::service_execution::checkpoint()?;
        let selected = self.refresh_service_snapshot()?;
        selected.recover_runtime_bootstrap()?;
        let Some(scope) = selected.current_scope()? else {
            return Ok(None);
        };
        let recovered = crate::current::recover_current(
            &selected.store,
            &selected.collection_quota()?,
            &scope,
        )?;
        Ok(Some(RevisionToken {
            incarnation: recovered.token.incarnation,
            generation: recovered.token.sequence,
            revision: recovered.token.digest,
        }))
    }
    pub fn maintain_service_collection(&self) -> Result<collection::RetirementReport> {
        let collection = self
            .service_collection
            .as_ref()
            .context("service collection required")?;
        crate::service_execution::checkpoint()?;
        collection.recover_abandoned_replacement()?;
        self.retire_service_incarnations()
    }
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
    pub(crate) fn active_service_view(&self) -> Result<Option<Self>> {
        if self.service_collection.is_some()
            && self.service_selection.is_none()
            && !self.service_candidate
        {
            Ok(Some(self.refresh_service_snapshot()?))
        } else {
            Ok(None)
        }
    }
    pub(crate) fn lock_current_writer(&self) -> Result<airk_store::WriterLock> {
        let selection = if !self.service_candidate {
            self.service_collection
                .as_ref()
                .map(|collection| collection.select())
                .transpose()?
        } else {
            None
        };
        if let Some(selection) = &selection {
            ensure!(
                selection.store.directory() == self.store.directory(),
                collection::SelectionContention
            );
        }
        let writer = self.store.lock()?;
        if let Some(collection) = &self.service_collection
            && !self.service_candidate
        {
            ensure!(
                collection.selected_store()?.directory() == self.store.directory(),
                collection::SelectionContention
            );
        }
        drop(selection);
        Ok(writer)
    }
    pub fn refresh_service_snapshot(&self) -> Result<Self> {
        let Some(collection) = &self.service_collection else {
            return Ok(self.clone());
        };
        let selection = Arc::new(collection.select()?);
        let mut selected = self.clone();
        // Keep the selected engine owner between requests after handoff too.
        // Retained requests own their previous store independently of this cache.
        let mut cached = self
            .service_active_store
            .lock()
            .map_err(|_| anyhow::anyhow!("active service store cache poisoned"))?;
        if cached
            .as_ref()
            .is_none_or(|store| store.directory() != selection.store.directory())
        {
            *cached = Some(self.inherit_shared_models(selection.store.clone())?);
        }
        selected.store = cached.as_ref().unwrap().clone();
        drop(cached);
        selected.service_selection = Some(selection);
        Ok(selected)
    }
    pub fn rebuild_service(&self, job: Sha256Digest) -> Result<RevisionToken> {
        self.require_service_sources()?;
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
        next.store = self.inherit_shared_models(replacement.store())?;
        next.service_selection = None;
        next.service_candidate = true;
        let settings = collection::copy_settings(
            &previous.store,
            &next.store,
            &self.root_id(),
            &self.collection_quota()?,
        )?;
        next.rebuild_extraction_policy()?;
        next.vector_selection()?;
        next.scan_limits()?;
        next.configured_roots()?;
        // Record actual replacement source work once, even if later verification
        // or manifest handoff fails. Job completion remains a separate receipt.
        next.observe_service_index(airk_core::usage::UsageOperation::Reindex, || {
            next.index_current(true, true)
        })?;
        // Bounded pages authenticate retained sources/extractions and identify
        // one exact target revision. No live-source substitution is permitted.
        let mut after = None;
        let mut revision = None;
        loop {
            crate::service_execution::checkpoint()?;
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
        crate::service_execution::checkpoint()?;
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
    use std::path::Path;
    fn query(text: &str) -> SearchQuery {
        SearchQuery {
            text: text.into(),
            path_prefix: None,
            limit: 5,
            max_bytes: 4096,
        }
    }

    #[test]
    fn concurrent_source_publications_and_queries_preserve_project_evidence() -> Result<()> {
        use std::sync::atomic::{AtomicBool, Ordering};
        let owner = tempfile::tempdir()?;
        let data = owner.path().join("data");
        std::fs::create_dir(&data)?;
        let mut kernels = Vec::new();
        for n in 0..4 {
            let source = owner.path().join(format!("source-{n}"));
            std::fs::create_dir(&source)?;
            for file in 0..16 {
                std::fs::write(
                    source.join(format!("file-{file}.txt")),
                    format!("projectmarker{n} Original fixture {file}.\n").repeat(32),
                )?;
            }
            let kernel = Kernel::open_service_collection(
                &source,
                Some(data.clone()),
                &format!("concurrent-{n}"),
            )?;
            kernel.configure_vectors(false)?;
            kernel.index_service_sources()?;
            kernels.push((source, kernel));
        }
        let stop = Arc::new(AtomicBool::new(false));
        let reader_kernel = kernels[0].1.clone();
        let reader_stop = stop.clone();
        let reader = std::thread::spawn(move || -> Result<()> {
            while !reader_stop.load(Ordering::Acquire) {
                match reader_kernel.search(&query("projectmarker0")) {
                    Ok(result) => ensure!(
                        !result.hits.is_empty(),
                        "concurrent reader lost its project evidence"
                    ),
                    Err(error) if Kernel::service_enrichment_contention(&error) => {}
                    Err(error) => return Err(error.context("concurrent project query")),
                }
            }
            Ok(())
        });
        let outcome = std::thread::scope(|scope| -> Result<()> {
            let mut writers = Vec::new();
            for (n, (source, kernel)) in kernels.iter().enumerate() {
                writers.push(scope.spawn(move || -> Result<()> {
                    for turn in 0..8 {
                        for file in 0..16 {
                            std::fs::write(
                                source.join(format!("file-{file}.txt")),
                                format!("projectmarker{n} Updated fixture {file} turn {turn}.\n")
                                    .repeat(64),
                            )?;
                        }
                        kernel.index_service_sources().with_context(|| {
                            format!("concurrent source publication project {n} turn {turn}")
                        })?;
                        ensure!(
                            crate::service_execution::with_database_admission(|| kernel.status())?
                                .files
                                == 16,
                            "publication lost its source inventory"
                        );
                    }
                    Ok(())
                }));
            }
            for writer in writers {
                writer
                    .join()
                    .map_err(|_| anyhow::anyhow!("source publication thread failed"))??;
            }
            Ok(())
        });
        stop.store(true, Ordering::Release);
        reader
            .join()
            .map_err(|_| anyhow::anyhow!("source query thread failed"))??;
        outcome
    }
    #[test]
    fn missing_source_keeps_retained_reads_and_rejects_scan_or_rebuild_without_deleting_evidence()
    -> Result<()> {
        let owner = tempfile::tempdir()?;
        let source = owner.path().join("source");
        let data = owner.path().join("data");
        std::fs::create_dir(&source)?;
        std::fs::create_dir(&data)?;
        std::fs::write(
            source.join("memo.md"),
            "# Copper harbor\nRetained copper harbor fixture.\n",
        )?;
        let project = crate::service_projects::registration(&source, &data, "retained")?;
        let kernel = Kernel::open_service_collection(&source, Some(data.clone()), "retained")?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let token = kernel
            .status()?
            .publication
            .context("missing initial token")?
            .token;
        let saved = owner.path().join("source temporarily unavailable");
        std::fs::rename(&source, &saved)?;
        assert!(crate::service_projects::validate(&project).is_err());
        crate::service_projects::validate_retained(&project)?;
        let reopened = Kernel::open_service_collection_for_diagnostics(
            Path::new(&project.root),
            Some(data.clone()),
            "retained",
        )?;
        assert!(!reopened.search(&query("copper"))?.hits.is_empty());
        assert!(matches!(
            reopened
                .status()?
                .source_availability
                .context("source observation missing")?
                .state,
            crate::service_projects::SourceState::Unavailable
        ));
        for result in [
            reopened.index().map(|_| ()),
            reopened.index_paths(&["memo.md".into()]).map(|_| ()),
            reopened
                .rebuild_service(Sha256Digest::from_bytes([67; 32]))
                .map(|_| ()),
        ] {
            assert!(
                result
                    .err()
                    .context("missing source permitted indexing")?
                    .is::<crate::service_projects::SourceUnavailable>()
            );
        }
        assert_eq!(
            reopened
                .status()?
                .publication
                .context("lost publication")?
                .token,
            token
        );
        assert!(!reopened.search(&query("copper"))?.hits.is_empty());
        std::fs::rename(saved, &source)?;
        crate::service_projects::validate(&project)?;
        reopened.index()?;
        assert!(matches!(
            reopened
                .status()?
                .source_availability
                .context("source observation missing")?
                .state,
            crate::service_projects::SourceState::Available
        ));
        assert_eq!(
            std::fs::read_to_string(source.join("memo.md"))?,
            "# Copper harbor\nRetained copper harbor fixture.\n"
        );
        Ok(())
    }

    #[test]
    fn missing_source_diagnostic_open_never_initializes_unbound_state() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().canonicalize()?.join("missing-source");
        let data = owner.path().join("new-data");
        assert!(
            Kernel::open_service_collection_for_diagnostics(&root, Some(data.clone()), "retained")
                .is_err()
        );
        assert!(!data.exists() && !root.exists());
        std::fs::create_dir(&data)?;
        assert!(
            Kernel::open_service_collection_for_diagnostics(&root, Some(data.clone()), "retained")
                .is_err()
        );
        assert_eq!(std::fs::read_dir(data)?.count(), 0);
        assert!(
            Kernel::open_service_collection_for_diagnostics(
                root.join("../alias"),
                Some(owner.path().into()),
                "retained"
            )
            .is_err()
        );
        Ok(())
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
        let span = kernel.usage_span(
            airk_core::usage::UsageOperation::Status,
            airk_core::usage::ClientFamily::Cli,
        );
        kernel.finish_usage(
            span,
            None,
            0,
            false,
            airk_core::usage::ResponseRepresentation::LogicalResultJson,
        );
        let usage = kernel.usage_report(30)?;
        assert_eq!(usage.report.calls, 1);
        let old = kernel.refresh_service_snapshot()?;
        let old_token = old
            .status()?
            .publication
            .context("old publication missing")?
            .token;
        assert!(!old.search(&query("amber"))?.hits.is_empty());
        let committed_old_read = old.current_reader()?;
        let changed = "# Cobalt harbor\nReplacement cobalt harbor source.\n";
        std::fs::write(source.path().join("memo.md"), changed)?;
        let token = kernel.rebuild_service(Sha256Digest::from_bytes([11; 32]))?;
        assert_ne!(old_token.incarnation, token.incarnation);
        assert_eq!(
            crate::current::runtime::revision(committed_old_read.token()?),
            old_token
        );
        let active_status = kernel.status()?;
        assert_eq!(active_status.publication.as_ref().unwrap().token, token);
        let active_store = kernel
            .service_collection
            .as_ref()
            .unwrap()
            .selected_store()?;
        assert_eq!(
            active_status.storage_sizes.current_index_bytes,
            Some(std::fs::metadata(active_store.directory().join("current.airk"))?.len())
        );
        assert!(!kernel.search(&query("cobalt"))?.hits.is_empty());
        assert!(!kernel.service_recovery_required()?);
        assert!(
            old.configure_vectors(true)
                .err()
                .unwrap()
                .is::<collection::SelectionContention>()
        );
        assert!(
            old.index_paths(&["memo.md".into()])
                .unwrap_err()
                .is::<collection::SelectionContention>()
        );
        let stale_store = old.store.clone();
        let stale_scope = old.current_scope()?.unwrap();
        let stale_writer = stale_store.lock()?;
        assert!(
            stale_store
                .redb_writer(&stale_writer, &kernel.collection_quota()?, &stale_scope)
                .err()
                .unwrap()
                .is::<collection::SelectionContention>()
        );
        drop(stale_writer);
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
        let historical = current.changes(&airk_core::changes::ChangesRequest {
            since: Some(old_token),
            ..Default::default()
        })?;
        assert_eq!(
            historical.baseline_status,
            airk_core::changes::BaselineStatus::BaselineUnavailable
        );
        assert!(historical.changes.is_empty());
        assert!(current.search(&query("amber"))?.hits.is_empty());
        assert!(!old.search(&query("amber"))?.hits.is_empty());
        assert!(!current.vectors_enabled()?);
        let after = current.usage_report(30)?;
        assert_eq!(after.report.calls, usage.report.calls + 1);
        assert_eq!(
            after.report.operations[&airk_core::usage::UsageOperation::Reindex].calls,
            1
        );
        for (operation, summary) in &usage.report.operations {
            assert_eq!(
                serde_json::to_value(after.report.operations.get(operation))?,
                serde_json::to_value(summary)?
            );
        }
        assert_eq!(
            serde_json::to_value(&after.report.scope)?,
            serde_json::to_value(&usage.report.scope)?
        );
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
        drop(committed_old_read);
        drop(old);
        assert_eq!(kernel.retire_service_incarnations()?.removed_directories, 1);
        assert!(!old_directory.exists());
        assert!(stale_store.lock().is_err());
        assert!(
            !old_directory.exists(),
            "stale writer recreated a retired slot"
        );
        assert_eq!(kernel.recover_service_publication()?, Some(token));
        std::fs::write(
            source.path().join("memo.md"),
            "# Saffron harbor\nLater saffron harbor source.\n",
        )?;
        kernel.index_paths(&["memo.md".into()])?;
        let updated = kernel.status()?.publication.unwrap().token;
        assert_eq!(updated.incarnation, token.incarnation);
        assert!(updated.generation > token.generation);
        assert!(!kernel.search(&query("saffron"))?.hits.is_empty());
        assert!(kernel.search(&query("cobalt"))?.hits.is_empty());
        kernel.configure_vectors(true)?;
        assert!(kernel.refresh_service_snapshot()?.vectors_enabled()?);
        kernel.configure_vectors(false)?;
        drop(current);
        drop(reopened);
        kernel.maintain_current()?;
        assert!(
            !old_directory.exists(),
            "recovery resurrected a retired initial slot"
        );
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
        let quota = airk_store::quota::Quota::open_service(data.path())?;
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
