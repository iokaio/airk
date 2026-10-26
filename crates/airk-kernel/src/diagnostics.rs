// SPDX-License-Identifier: Apache-2.0
//! Read-only, revision-bound verification of retained source and extraction inputs.
use crate::Kernel;
use airk_core::{evidence::RevisionEnvelope, identity::RevisionToken};
use airk_store::{blobs::MissingRetainedObject, current::postings::Lookup};
use anyhow::{Context, Result, ensure};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct IntegrityFailure {
    pub path: String,
    pub component: &'static str,
    pub kind: &'static str,
}

#[derive(Debug, Serialize)]
pub struct IntegrityReport {
    pub publication: RevisionEnvelope,
    pub from_start: bool,
    pub total_files: u64,
    /// Files for which verification was attempted on this page.
    pub checked_files: usize,
    pub verified_sources: usize,
    pub verified_extractions: usize,
    pub checked_source_bytes: u64,
    pub checked_extraction_bytes: u64,
    pub failed_objects: usize,
    pub failures: Vec<IntegrityFailure>,
    pub failures_truncated: usize,
    pub skipped_large: usize,
    /// Enumeration reached the end; skipped or failed objects remain unverified.
    pub complete: bool,
    pub next_after: Option<String>,
}

impl IntegrityReport {
    fn failure(&mut self, path: &str, component: &'static str, error: &anyhow::Error) {
        self.failed_objects += 1;
        if self.failures.len() < 8 {
            self.failures.push(IntegrityFailure {
                path: path.into(),
                component,
                kind: if error.downcast_ref::<MissingRetainedObject>().is_some() {
                    "missing"
                } else if error.downcast_ref::<std::io::Error>().is_some() {
                    "unavailable"
                } else {
                    "invalid"
                },
            });
        } else {
            self.failures_truncated += 1;
        }
    }
}

impl Kernel {
    /// Verifies only retained derived inputs. It never scans live sources,
    /// recovers a journal, obtains a writer, or repairs missing objects.
    pub fn diagnose_current(
        &self,
        revision: Option<RevisionToken>,
        after: Option<&str>,
        max_files: usize,
        max_bytes: u64,
    ) -> Result<IntegrityReport> {
        crate::service_execution::progress_phase(airk_core::service_progress::WorkPhase::Verifying);
        ensure!(
            (1..=128).contains(&max_files)
                && (1..=512 * 1024 * 1024).contains(&max_bytes)
                && after.is_none_or(|value| !value.is_empty() && value.len() <= 4096),
            "invalid retained integrity diagnostic bounds"
        );
        ensure!(
            after.is_none() || revision.is_some(),
            "continuing retained integrity verification requires its exact revision"
        );
        let read = self.current_reader()?;
        read.verify_lexical_integrity()?;
        let publication = crate::current::runtime::envelope(&read)?;
        ensure!(
            revision.is_none_or(|expected| expected == publication.token),
            "retained integrity revision changed; restart verification at the current head"
        );
        let total_files = read
            .collection_state()?
            .context("current collection configuration missing")?
            .counts
            .files;
        let page = read.lookup_paths(Lookup::Files, "", after, max_files + 1)?;
        let mut report = IntegrityReport {
            publication,
            from_start: after.is_none(),
            total_files,
            checked_files: 0,
            verified_sources: 0,
            verified_extractions: 0,
            checked_source_bytes: 0,
            checked_extraction_bytes: 0,
            failed_objects: 0,
            failures: Vec::new(),
            failures_truncated: 0,
            skipped_large: 0,
            complete: false,
            next_after: after.map(str::to_owned),
        };
        let mut admitted_bytes = 0u64;
        let mut visited = 0usize;
        for path in page.paths.iter().take(max_files) {
            let artifact = match read.artifact(path) {
                Ok(Some(artifact)) => artifact,
                result => {
                    let error = result
                        .err()
                        .unwrap_or_else(|| anyhow::anyhow!("current catalog artifact missing"));
                    report.failure(path, "catalog", &error);
                    report.checked_files += 1;
                    visited += 1;
                    report.next_after = Some(path.clone());
                    continue;
                }
            };
            let Some(reference) = artifact.retained.as_ref() else {
                report.failure(
                    path,
                    "catalog",
                    &anyhow::anyhow!("current retained reference missing"),
                );
                report.checked_files += 1;
                visited += 1;
                report.next_after = Some(path.clone());
                continue;
            };
            let bytes = (artifact.bytes as u64)
                .checked_add(reference.object_bytes as u64)
                .context("retained diagnostic byte overflow")?;
            if bytes > max_bytes {
                report.skipped_large += 1;
                visited += 1;
                report.next_after = Some(path.clone());
                continue;
            }
            if admitted_bytes.saturating_add(bytes) > max_bytes {
                break;
            }
            admitted_bytes += bytes;
            report.checked_files += 1;
            visited += 1;
            report.next_after = Some(path.clone());
            match self.store.verify_retained_source(&artifact) {
                Ok(()) => {
                    report.verified_sources += 1;
                    report.checked_source_bytes += artifact.bytes as u64;
                }
                Err(error) => report.failure(path, "source", &error),
            }
            match self.store.load_retained_bounded(&artifact, max_bytes) {
                Ok(_) => {
                    report.verified_extractions += 1;
                    report.checked_extraction_bytes += reference.object_bytes as u64;
                }
                Err(error) => report.failure(path, "extraction", &error),
            }
        }
        report.complete = visited == page.paths.len() && page.next_after.is_none();
        if report.complete {
            report.next_after = None;
        }
        crate::service_execution::progress_verified(
            (report.verified_sources + report.verified_extractions) as u64,
        );
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn retained_diagnostics_verify_both_inputs_without_live_sources_and_expire_pages() -> Result<()>
    {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        fs::create_dir(&root)?;
        for (name, content) in [
            ("a.txt", "first retained evidence"),
            ("b.txt", "second retained evidence"),
        ] {
            fs::write(root.join(name), content)?;
        }
        let kernel = Kernel::open(&root, Some(directory.path().join("state")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let read = kernel.current_reader()?;
        let artifact = read.artifact("a.txt")?.unwrap();
        let extraction = artifact.retained.as_ref().unwrap().object.clone();
        drop(read);
        let first = kernel.diagnose_current(None, None, 1, 1 << 20)?;
        assert!(!first.complete);
        assert_eq!(first.checked_files, 1);
        assert_eq!(first.verified_sources, 1);
        assert!(
            kernel
                .diagnose_current(None, first.next_after.as_deref(), 1, 1 << 20)
                .is_err()
        );
        fs::remove_file(root.join("a.txt"))?;
        fs::remove_file(root.join("b.txt"))?;
        let next = kernel.diagnose_current(
            Some(first.publication.token),
            first.next_after.as_deref(),
            1,
            1 << 20,
        )?;
        assert!(next.complete);
        assert_eq!(next.failed_objects, 0);
        assert_eq!(
            kernel.diagnose_current(None, None, 128, 1)?.skipped_large,
            2
        );
        assert_eq!(kernel.status()?.publication.unwrap(), first.publication);

        fs::write(root.join("a.txt"), "first retained evidence")?;
        fs::write(root.join("b.txt"), "second retained evidence")?;
        fs::write(root.join("new.txt"), "new generation")?;
        kernel.index()?;
        assert!(
            kernel
                .diagnose_current(
                    Some(first.publication.token),
                    first.next_after.as_deref(),
                    1,
                    1 << 20,
                )
                .is_err()
        );
        let current = kernel.status()?.publication.unwrap();

        let source = kernel
            .store
            .directory()
            .join("objects")
            .join("sources")
            .join(&artifact.content_hash);
        let payload = kernel
            .store
            .directory()
            .join("objects")
            .join("artifacts")
            .join(&extraction);
        fs::remove_file(&source)?;
        let missing = kernel.diagnose_current(None, None, 128, 1 << 20)?;
        assert!(
            missing
                .failures
                .iter()
                .any(|failure| failure.path == "a.txt"
                    && failure.component == "source"
                    && failure.kind == "missing")
        );
        fs::write(&payload, b"corrupt")?;
        let corrupt = kernel.diagnose_current(None, None, 128, 1 << 20)?;
        assert!(
            corrupt
                .failures
                .iter()
                .any(|failure| failure.component == "extraction" && failure.kind == "invalid")
        );
        fs::remove_file(&payload)?;
        assert!(
            kernel
                .diagnose_current(None, None, 128, 1 << 20)?
                .failures
                .iter()
                .any(|failure| failure.component == "extraction" && failure.kind == "missing")
        );
        assert_eq!(kernel.status()?.publication.unwrap(), current);
        Ok(())
    }

    #[test]
    fn diagnostic_open_preserves_an_interrupted_applying_journal() -> Result<()> {
        use crate::current::{CurrentUpdateStage, CurrentUpdater};
        use airk_store::{
            current::{ArtifactChange, CapturedInput, TransactionPhase},
            quota::Quota,
        };

        fn retained_state(
            root: &std::path::Path,
        ) -> Result<std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>> {
            let mut files = std::collections::BTreeMap::new();
            let mut pending = vec![root.to_owned()];
            while let Some(directory) = pending.pop() {
                for entry in fs::read_dir(directory)? {
                    let entry = entry?;
                    if entry.file_type()?.is_dir() {
                        pending.push(entry.path());
                    } else {
                        files.insert(
                            entry.path().strip_prefix(root)?.to_owned(),
                            fs::read(entry.path())?,
                        );
                    }
                }
            }
            Ok(files)
        }

        let directory = tempfile::tempdir()?;
        let root = directory.path().join("source");
        let data = directory.path().join("state");
        fs::create_dir(&root)?;
        let source = root.join("alpha.txt");
        fs::write(&source, "original captured evidence\n")?;
        let kernel = Kernel::open(&root, Some(data.clone()))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let read = kernel.current_reader()?;
        let artifact = read.artifact("alpha.txt")?.unwrap();
        let original_hash = artifact.content_hash.clone();
        drop(read);
        let scope = kernel.current_scope()?.unwrap();
        let quota = Quota::open(&data)?;
        let writer = kernel.store.lock()?;
        let reservation = kernel.publication_reservation()?;
        let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            CurrentUpdater {
                store: &kernel.store,
                quota: &quota,
                scope: &scope,
                writer: &writer,
                reservation: &reservation,
                configuration: None,
            }
            .apply_observed(
                &[ArtifactChange::Upsert(Box::new(CapturedInput::new(
                    artifact,
                )))],
                None,
                &mut |stage| {
                    if stage == CurrentUpdateStage::Applying {
                        panic!("injected interruption before captured journal replay");
                    }
                },
            )
        }));
        assert!(interrupted.is_err());
        drop(reservation);
        let capacity = kernel.store.current_database_capacity(&scope)?;
        let current = kernel
            .store
            .current_write(&writer, &quota, &scope, capacity)?;
        assert_eq!(current.phase()?, TransactionPhase::Applying);
        let pending = current.pending()?.unwrap();
        drop(current);
        drop(writer);
        drop(kernel);
        fs::write(&source, "changed live bytes must not enter diagnostics\n")?;
        let before = retained_state(&data)?;

        let diagnostic =
            Kernel::open_collection_for_diagnostics(&root, Some(data.clone()), "default")?;
        let error = diagnostic
            .diagnose_current(None, None, 128, 1 << 20)
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("requires writer recovery before reads"),
            "{error:#}"
        );
        assert_eq!(retained_state(&data)?, before);
        let writer = diagnostic.store.lock()?;
        let current = diagnostic
            .store
            .current_write(&writer, &quota, &scope, capacity)?;
        assert_eq!(current.phase()?, TransactionPhase::Applying);
        assert_eq!(current.pending()?.as_ref(), Some(&pending));
        drop(current);
        drop(writer);
        drop(diagnostic);

        // This journal is recoverable. Ordinary open would have replayed it,
        // while the diagnostic constructor must leave that choice explicit.
        let recovered = Kernel::open(&root, Some(data))?;
        let evidence = recovered.read_file("alpha.txt", 1, 1, 4096)?;
        assert_eq!(evidence.hits[0].text, "original captured evidence\n");
        assert_eq!(evidence.hits[0].citation.content_hash, original_hash);
        assert_eq!(
            fs::read_to_string(source)?,
            "changed live bytes must not enter diagnostics\n"
        );
        Ok(())
    }
}
