// SPDX-License-Identifier: Apache-2.0
//! Original public lifecycle qualification: real publications, quotas and leases.
use airk_core::identity::RevisionToken;
use airk_kernel::Kernel;
use airk_store::{
    FileStore,
    quota::Quota,
    retention::{MaintenanceStage, RetentionMode, RetentionPolicy},
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct Fixture {
    source: tempfile::TempDir,
    data: tempfile::TempDir,
    kernel: Kernel,
    store: FileStore,
}
impl Fixture {
    fn new() -> Result<Self> {
        let source = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        // Size the entire fixture, including initialization and other collections,
        // before any SQLite catalog is admitted. The default 4 GiB allowance
        // tests physical free-space races on hosted runners instead of retention.
        let quota = Quota::open(data.path())?;
        quota.configure(airk_store::quota::QuotaPolicy {
            root_bytes: 512 * 1024 * 1024,
            default_collection_bytes: 128 * 1024 * 1024,
            collections: BTreeMap::new(),
        })?;
        let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
        kernel.initialize()?;
        let store = FileStore::new(data.path().join("collections/default"));
        kernel.configure_retention(RetentionPolicy {
            mode: RetentionMode::Manual,
            ..Default::default()
        })?;
        Ok(Self {
            source,
            data,
            kernel,
            store,
        })
    }
    fn edit(&self, sequence: usize) -> Result<RevisionToken> {
        let text = format!(
            "# Publication {sequence:04}\n{}",
            "Original local evidence remains bounded and independent of external sources.\n"
                .repeat(48)
        );
        fs::write(self.source.path().join("evidence.md"), text)?;
        self.kernel.index()?;
        self.store
            .persistent_revision()?
            .context("published header missing")?
            .token
            .context("v2 token missing")
    }
    fn automatic(&self) -> Result<()> {
        self.kernel.configure_retention(RetentionPolicy {
            mode: RetentionMode::Automatic,
            ..Default::default()
        })?;
        Ok(())
    }
}

fn files(directory: &Path) -> Result<BTreeMap<PathBuf, u64>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(parent) = pending.pop() {
        if !parent.exists() {
            continue;
        }
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            ensure!(!kind.is_symlink(), "unexpected linked test state");
            if kind.is_dir() {
                pending.push(entry.path());
            } else {
                result.insert(
                    entry.path().strip_prefix(directory)?.to_path_buf(),
                    entry.metadata()?.len(),
                );
            }
        }
    }
    Ok(result)
}
fn generations(store: &FileStore) -> Result<Vec<String>> {
    let mut names = vec![];
    for entry in fs::read_dir(store.directory().join("generations"))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if airk_store::blobs::valid_digest(&name) {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}
fn content_and_search_bytes(store: &FileStore) -> Result<u64> {
    Ok(files(store.directory())?
        .iter()
        .filter(|(path, _)| {
            path.starts_with("objects")
                || path
                    .components()
                    .any(|component| component.as_os_str() == "lexical")
                || path.file_name().is_some_and(|name| name == "vectors.bin")
        })
        .map(|(_, bytes)| *bytes)
        .sum())
}

fn retained_object_paths(store: &FileStore) -> Result<BTreeSet<PathBuf>> {
    let current = store.catalog()?.context("active catalog missing")?;
    let parent = current
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.observation.parent);
    let mut catalogs = vec![current];
    if let Some(parent) = parent {
        catalogs.push(
            store
                .catalog_at(&parent, 16 * 1024 * 1024)?
                .context("searchable predecessor missing")?,
        );
    }
    let mut expected = BTreeSet::new();
    for catalog in catalogs {
        for file in catalog
            .inventory
            .context("v2 inventory missing")?
            .files
            .values()
        {
            expected.insert(PathBuf::from("artifacts").join(file.state.payload_digest.to_string()));
            if file.raw_source {
                expected.insert(PathBuf::from("sources").join(file.state.source_hash.to_string()));
            }
        }
    }
    Ok(expected)
}

fn finish_pending_objects(fixture: &Fixture) -> Result<()> {
    let head = fixture.store.persistent_revision()?.unwrap().token;
    let expected = retained_object_paths(&fixture.store)?;
    for _ in 0..8 {
        let actual: BTreeSet<_> = files(&fixture.store.directory().join("objects"))?
            .into_keys()
            .collect();
        ensure!(
            expected.is_subset(&actual),
            "maintenance removed a retained object"
        );
        if actual == expected {
            assert_eq!(fixture.store.persistent_revision()?.unwrap().token, head);
            return Ok(());
        }
        let status = fixture.kernel.retention_status()?;
        ensure!(
            status
                .last_maintenance
                .as_ref()
                .is_some_and(|receipt| receipt.pending),
            "unreferenced objects require truthful pending maintenance: actual={actual:?}, expected={expected:?}, receipt={:?}",
            status.last_maintenance
        );
        fixture.kernel.maintain_retention_with_budget(
            airk_store::retention::MaintenanceBudget {
                max_duration_ms: 10_000,
                ..Default::default()
            },
        )?;
    }
    anyhow::bail!("bounded maintenance continuations did not converge to exact retained objects")
}

fn finish_pending_generations(fixture: &Fixture) -> Result<usize> {
    if generations(&fixture.store)?.len() <= 2 {
        return Ok(0);
    }
    let catalog = fixture
        .store
        .catalog()?
        .context("published catalog missing")?;
    let head = catalog
        .receipt
        .as_ref()
        .context("publication receipt missing")?
        .token;
    let mut protected = BTreeSet::from([head.revision.to_string()]);
    if let Some(parent) = catalog
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.observation.parent)
    {
        protected.insert(parent.revision.to_string());
    }
    drop(catalog);
    for continuation in 0..=8 {
        let retained = generations(&fixture.store)?;
        ensure!(
            protected.iter().all(|revision| retained.contains(revision)),
            "maintenance removed active/predecessor generation"
        );
        ensure!(
            fixture.store.persistent_revision()?.unwrap().token == Some(head),
            "maintenance advanced publication head"
        );
        if retained.len() <= 2 {
            return Ok(continuation);
        }
        let receipt = fixture
            .kernel
            .retention_status()?
            .last_maintenance
            .context("deferred generation cleanup has no receipt")?;
        ensure!(
            receipt.head == Some(head)
                && receipt.pending
                && receipt.state == airk_store::retention::MaintenanceState::Pending
                && receipt.pressure.is_none(),
            "extra generations require truthful, same-publication bounded-work deferral: {receipt:?}"
        );
        ensure!(
            continuation < 8,
            "bounded generation continuations failed to reach two retained revisions: {retained:?}"
        );
        let run = fixture.kernel.maintain_retention_with_budget(
            airk_store::retention::MaintenanceBudget {
                max_duration_ms: 10_000,
                ..Default::default()
            },
        )?;
        ensure!(
            run.receipt_persisted,
            "continuation receipt was not persisted"
        );
    }
    unreachable!("bounded continuation loop returns or fails")
}

#[test]
fn deferred_postcommit_generation_retirement_is_pending_and_converges_without_publication()
-> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.automatic()?;
    fixture.edit(0)?;
    fixture.edit(1)?;
    finish_pending_generations(&fixture)?;
    let ordinary = fixture.kernel.clone();
    let observed = Arc::new(AtomicUsize::new(0));
    let calls = observed.clone();
    fixture.kernel = fixture
        .kernel
        .with_retention_observer(Arc::new(move |stage| {
            if stage == MaintenanceStage::IntentDurable {
                calls.fetch_add(1, Ordering::SeqCst);
                // Deadline exhaustion before authorization must leave the full
                // representation intact, with a recoverable pending receipt.
                std::thread::sleep(std::time::Duration::from_millis(1100));
            }
            Ok(())
        }));
    let head = fixture.edit(2)?;
    fixture.kernel = ordinary;
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert_eq!(generations(&fixture.store)?.len(), 3);
    assert!(finish_pending_generations(&fixture)? > 0);
    assert_eq!(generations(&fixture.store)?.len(), 2);
    assert_eq!(
        fixture.store.persistent_revision()?.unwrap().token,
        Some(head)
    );
    finish_pending_objects(&fixture)?;
    assert_eq!(files(&fixture.store.directory().join("objects"))?.len(), 4);
    assert_eq!(fixture.kernel.retention_status()?.pending_intents, 0);
    Ok(())
}

#[test]
fn deferred_postcommit_object_gc_is_pending_and_converges_without_publication() -> Result<()> {
    let mut fixture = Fixture::new()?;
    fixture.automatic()?;
    fixture.edit(0)?;
    fixture.edit(1)?;
    let ordinary = fixture.kernel.clone();
    let observed = Arc::new(AtomicUsize::new(0));
    let calls = observed.clone();
    fixture.kernel = fixture
        .kernel
        .with_retention_observer(Arc::new(move |stage| {
            if stage == MaintenanceStage::IntentComplete {
                calls.fetch_add(1, Ordering::SeqCst);
                // Deliberately consume the cooperative 1000ms allowance after the
                // obsolete representation is retired, before object GC can start.
                std::thread::sleep(std::time::Duration::from_millis(1100));
            }
            Ok(())
        }));
    fixture.edit(2)?;
    fixture.kernel = ordinary;
    assert_eq!(observed.load(Ordering::SeqCst), 1);
    assert_eq!(generations(&fixture.store)?.len(), 2);
    assert!(files(&fixture.store.directory().join("objects"))?.len() > 4);
    assert!(
        fixture
            .kernel
            .retention_status()?
            .last_maintenance
            .unwrap()
            .pending
    );
    finish_pending_objects(&fixture)?;
    assert_eq!(files(&fixture.store.directory().join("objects"))?.len(), 4);
    Ok(())
}

#[test]
fn two_hundred_real_edits_reach_searchable_and_content_plateau() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.automatic()?;
    let quota = Quota::open(fixture.data.path())?;
    let mut previous = None;
    let mut tail = vec![];
    let mut total_tail = vec![];
    let mut continuations = 0;
    for edit in 0..200 {
        let token = fixture.edit(edit)?;
        if let Some(previous) = previous {
            assert_eq!(token.generation, previous + 1);
        }
        previous = Some(token.generation);
        // The postcommit deadline is cooperative, not a promise that all
        // retirement completes synchronously under arbitrary build contention.
        // Require truthful pending state and bounded convergence before measuring
        // the same strict two-generation/content plateau.
        continuations += finish_pending_generations(&fixture)?;
        let retained = generations(&fixture.store)?;
        assert!(
            retained.len() <= 2,
            "edit {edit} retained {} full revisions",
            retained.len()
        );
        assert!(retained.contains(&token.revision.to_string()));
        let observed = quota.status()?;
        assert_eq!(
            observed.active_reservations, 0,
            "publication leaked reservation"
        );
        assert_eq!(observed.reserved_bytes, 0);
        if edit >= 180 {
            // Postcommit object cleanup may be deferred independently of full
            // generation retirement. Measure only after its bounded continuation.
            finish_pending_objects(&fixture)?;
            tail.push(content_and_search_bytes(&fixture.store)?);
            total_tail.push(quota.status()?.used_bytes);
        }
        if (edit + 1) % 25 == 0 {
            eprintln!(
                "retention plateau: completed {} real publications",
                edit + 1
            );
        }
    }
    let min = *tail.iter().min().unwrap();
    let max = *tail.iter().max().unwrap();
    assert!(
        max <= min + min / 2 + 16 * 1024,
        "content/search bytes did not plateau: {min}..{max}"
    );
    // Postcommit work has a cooperative deadline. Any deferred object cleanup
    // must be reported and converge through bounded maintenance, without edits.
    finish_pending_objects(&fixture)?;
    let object_files = files(&fixture.store.directory().join("objects"))?;
    assert!(
        object_files.len() <= 4,
        "only two source/payload pairs should survive"
    );
    let status = fixture.kernel.retention_status()?;
    assert!(status.history_supported && status.checkpoints_supported);
    assert_eq!(status.pending_intents, 0);
    eprintln!("200-edit retention: {continuations} bounded generation cleanup continuations");
    eprintln!(
        "200-edit retention: content/search tail={min}..{max}; total logical tail={}..{} (includes independently bounded growing lineage/catalog)",
        total_tail.iter().min().unwrap(),
        total_tail.iter().max().unwrap()
    );
    Ok(())
}

#[test]
fn manual_mode_and_read_operations_preserve_obsolete_revisions() -> Result<()> {
    let fixture = Fixture::new()?;
    for edit in 0..5 {
        fixture.edit(edit)?;
    }
    let retained = generations(&fixture.store)?;
    assert!(retained.len() >= 5);
    let before_objects = files(&fixture.store.directory().join("objects"))?;
    assert_eq!(
        fixture.kernel.retention_status()?.policy.policy.mode,
        RetentionMode::Manual
    );
    let _ = fixture.kernel.retention_preview(None)?;
    let _ = fixture.kernel.file_info("evidence.md")?;
    assert_eq!(generations(&fixture.store)?, retained);
    assert_eq!(
        files(&fixture.store.directory().join("objects"))?,
        before_objects
    );
    Ok(())
}

#[test]
fn long_lived_reader_pins_evidence_until_explicit_maintenance_after_release() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.automatic()?;
    let first = fixture.edit(0)?;
    let pinned = fixture
        .store
        .catalog_at(&first, airk_store::blobs::MAX_CATALOG_BYTES)?
        .unwrap();
    for edit in 1..8 {
        fixture.edit(edit)?;
    }
    assert!(generations(&fixture.store)?.contains(&first.revision.to_string()));
    assert!(pinned.file("evidence.md")?.is_some());
    let status = fixture.kernel.retention_status()?;
    assert!(
        status
            .protected
            .iter()
            .any(|revision| revision.token == first)
    );
    drop(pinned);
    fixture.kernel.maintain_retention()?;
    assert!(
        fixture
            .store
            .catalog_at(&first, airk_store::blobs::MAX_CATALOG_BYTES)?
            .is_none()
    );
    assert!(generations(&fixture.store)?.len() <= 2);
    Ok(())
}

#[test]
fn postcommit_cleanup_failure_keeps_new_publication_and_recovers_after_reopen() -> Result<()> {
    let fixture = Fixture::new()?;
    for edit in 0..4 {
        fixture.edit(edit)?;
    }
    fixture.automatic()?;
    let observed = Arc::new(AtomicUsize::new(0));
    let calls = observed.clone();
    let worker = fixture
        .kernel
        .with_retention_observer(Arc::new(move |stage| {
            if stage == MaintenanceStage::IntentDurable {
                calls.fetch_add(1, Ordering::SeqCst);
                anyhow::bail!("original injected postcommit maintenance failure");
            }
            Ok(())
        }));
    let before = fixture.store.persistent_revision()?.unwrap().token.unwrap();
    fs::write(
        fixture.source.path().join("evidence.md"),
        "# Successful publication\nCleanup failure cannot undo committed evidence.\n",
    )?;
    worker.index()?;
    let after = fixture.store.persistent_revision()?.unwrap().token.unwrap();
    assert!(after.generation > before.generation);
    assert!(observed.load(Ordering::SeqCst) > 0);
    let status = worker.retention_status()?;
    assert!(
        status
            .last_maintenance
            .as_ref()
            .is_some_and(|receipt| receipt.pending && receipt.error.is_some())
    );
    assert!(status.pending_intents > 0);
    drop(worker);
    let reopened = Kernel::open(fixture.source.path(), Some(fixture.data.path().into()))?;
    reopened.maintain_retention()?;
    assert_eq!(
        fixture.store.persistent_revision()?.unwrap().token,
        Some(after)
    );
    assert_eq!(reopened.retention_status()?.pending_intents, 0);
    Ok(())
}

#[test]
fn manual_quota_exhaustion_is_typed_and_never_prunes_or_advances() -> Result<()> {
    let fixture = Fixture::new()?;
    for edit in 0..5 {
        fixture.edit(edit)?;
    }
    let quota = Quota::open(fixture.data.path())?;
    let used = quota.status()?.collections["default"].used_bytes;
    let mut policy = quota.policy()?;
    policy
        .collections
        .insert("default".into(), used + 64 * 1024);
    quota.configure(policy)?;
    let before = fixture.store.persistent_revision()?.unwrap().token;
    let retained = generations(&fixture.store)?;
    fs::write(
        fixture.source.path().join("evidence.md"),
        format!(
            "# Larger source\n{}",
            "Bounded quota fixture evidence.\n".repeat(8192)
        ),
    )?;
    let error = fixture
        .kernel
        .index()
        .err()
        .context("manual exhausted publication unexpectedly succeeded")?;
    assert!(
        error
            .chain()
            .any(|cause| cause.is::<airk_store::quota::QuotaPressure>()),
        "expected typed quota error: {error:#}"
    );
    assert_eq!(fixture.store.persistent_revision()?.unwrap().token, before);
    assert_eq!(generations(&fixture.store)?, retained);
    assert_eq!(quota.status()?.active_reservations, 0);
    Ok(())
}

#[test]
fn automatic_pressure_rebuilds_with_fresh_admission_after_reclaim() -> Result<()> {
    let mut fixture = Fixture::new()?;
    let body =
        "Original bounded large evidence with distinct observed source bytes.\n".repeat(50_000);
    for generation in 0..3 {
        fs::write(
            fixture.source.path().join("evidence.md"),
            format!("# Revision {generation:04}\n{body}"),
        )?;
        fixture.kernel.index()?;
    }
    // Retire the empty initialization revision so the first pressure candidate
    // contains real payload bytes rather than only a tiny empty catalog.
    fixture.kernel.maintain_retention()?;
    let quota = Quota::open(fixture.data.path())?;
    let used = quota.status()?.collections["default"].used_bytes;
    let mut policy = quota.policy()?;
    policy
        .collections
        .insert("default".into(), used + 3 * 1024 * 1024);
    quota.configure(policy)?;
    let before = fixture.store.persistent_revision()?.unwrap().token.unwrap();
    fs::write(
        fixture.source.path().join("evidence.md"),
        format!("# Revision 9999\n{body}"),
    )?;
    // Prove that this exact publication cannot fit before reclamation, and
    // that the failed attempt releases admission without advancing authority.
    let retained = generations(&fixture.store)?;
    let error = fixture
        .kernel
        .index()
        .expect_err("fixture must hit quota pressure");
    assert!(
        error
            .chain()
            .any(|cause| cause.is::<airk_store::quota::QuotaPressure>()),
        "expected logical quota pressure, not physical exhaustion: {error:#}"
    );
    assert_eq!(
        fixture.store.persistent_revision()?.unwrap().token,
        Some(before)
    );
    assert_eq!(generations(&fixture.store)?, retained);
    assert_eq!(quota.status()?.active_reservations, 0);
    fixture.automatic()?;
    let reclaimed_before_publication = Arc::new(AtomicUsize::new(0));
    let observed = reclaimed_before_publication.clone();
    let store = fixture.store.clone();
    fixture.kernel = fixture
        .kernel
        .with_retention_observer(Arc::new(move |stage| {
            if stage == MaintenanceStage::IntentComplete
                && store.persistent_revision()?.unwrap().token == Some(before)
            {
                observed.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }));
    fixture.kernel.index()?;
    assert!(
        reclaimed_before_publication.load(Ordering::SeqCst) > 0,
        "the successful publication must require pressure reclamation, not just postcommit cleanup"
    );
    let after = fixture.store.persistent_revision()?.unwrap().token.unwrap();
    assert_eq!(after.generation, before.generation + 1);
    assert!(generations(&fixture.store)?.len() <= 2);
    let status = quota.status()?;
    assert_eq!(status.active_reservations, 0);
    assert_eq!(status.reserved_bytes, 0);
    assert!(status.collections["default"].used_bytes <= status.collections["default"].limit_bytes);
    assert!(fixture.kernel.file_info("evidence.md").is_ok());
    Ok(())
}

#[test]
fn automatic_maintenance_isolated_from_other_collection_and_sources() -> Result<()> {
    let fixture = Fixture::new()?;
    let other_source = tempfile::tempdir()?;
    fs::write(
        other_source.path().join("other.md"),
        "# Other collection\nOriginal independent evidence.\n",
    )?;
    let other = Kernel::open_collection(
        other_source.path(),
        Some(fixture.data.path().into()),
        "other",
    )?;
    other.index()?;
    let other_store = FileStore::new(fixture.data.path().join("collections/other"));
    let token = other_store.persistent_revision()?.unwrap().token;
    let other_files = files(other_store.directory())?;
    fixture.automatic()?;
    for edit in 0..8 {
        fixture.edit(edit)?;
    }
    fixture.kernel.maintain_retention()?;
    assert_eq!(other_store.persistent_revision()?.unwrap().token, token);
    assert_eq!(files(other_store.directory())?, other_files);
    assert_eq!(
        fs::read_to_string(other_source.path().join("other.md"))?,
        "# Other collection\nOriginal independent evidence.\n"
    );
    Ok(())
}

#[test]
fn quota_retry_reuses_successful_provider_output_without_reexecuting_process() -> Result<()> {
    use airk_ingest::providers::{LiveRequest, ProviderCommand};
    let tools = tempfile::tempdir()?;
    let executable = tools.path().join(if cfg!(windows) {
        "counter.exe"
    } else {
        "counter"
    });
    let helper = tools.path().join("counter.rs");
    fs::write(
        &helper,
        r#"
use std::{fs::OpenOptions, io::Write};
fn main() {
    let path = std::env::current_exe().unwrap().with_file_name("invocations");
    writeln!(OpenOptions::new().create(true).append(true).open(path).unwrap(), "run").unwrap();
    println!("{{\"_type\":\"tag\",\"name\":\"one\",\"path\":\"src/lib.rs\",\"line\":1,\"end\":1,\"kind\":\"function\",\"signature\":\"() -> u32\"}}");
}
"#,
    )?;
    let compiled = std::process::Command::new("rustc")
        .arg("--edition=2024")
        .arg(&helper)
        .arg("-o")
        .arg(&executable)
        .output()?;
    ensure!(
        compiled.status.success(),
        "counter fixture compile failed: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let fixture = Fixture::new()?;
    fs::create_dir(fixture.source.path().join("src"))?;
    fs::write(
        fixture.source.path().join("src/lib.rs"),
        "pub fn one() -> u32 { 1 }\n",
    )?;
    fs::write(
        fixture.source.path().join("Cargo.toml"),
        "[package]\nname=\"original-retention-fixture\"\nversion=\"0.0.0\"\n",
    )?;
    let body = "Original retained provider pressure evidence.\n".repeat(32_768);
    for revision in 0..4 {
        fs::write(
            fixture.source.path().join("evidence.md"),
            format!("# Revision {revision}\n{body}"),
        )?;
        fixture.kernel.index()?;
    }
    fixture.kernel.maintain_retention()?;
    fixture.automatic()?;
    let quota = Quota::open(fixture.data.path())?;
    let used = quota.status()?.collections["default"].used_bytes;
    let mut policy = quota.policy()?;
    // The provider's explicit allowance fits, but the first publication cannot.
    // Obsolete real payloads fund the single fresh reservation after maintenance.
    policy
        .collections
        .insert("default".into(), used + 160 * 1024 * 1024 + 64 * 1024);
    quota.configure(policy)?;
    let before = fixture.store.persistent_revision()?.unwrap().token.unwrap();
    let result = fixture.kernel.run_provider(
        "ctags",
        &LiveRequest {
            command: ProviderCommand {
                program: executable,
                args: vec![],
                timeout_ms: 30_000,
            },
            producer: "original-retention-counter".into(),
            producer_version: "1".into(),
            build_inputs: vec!["Cargo.toml".into()],
            paths: vec!["src/lib.rs".into()],
        },
    );
    if let Err(error) = result {
        anyhow::bail!(
            "provider retry failed: {error:#}; retained={:?}; maintenance={:?}; invocations={:?}",
            generations(&fixture.store)?,
            fixture.kernel.retention_status()?.last_maintenance,
            fs::read_to_string(tools.path().join("invocations"))
        );
    }
    assert_eq!(
        fs::read_to_string(tools.path().join("invocations"))?,
        "run\n"
    );
    assert_eq!(
        fixture
            .store
            .persistent_revision()?
            .unwrap()
            .token
            .unwrap()
            .generation,
        before.generation + 1
    );
    // The inherited manual backlog is drained in bounded maintenance passes.
    for _ in 0..4 {
        if generations(&fixture.store)?.len() <= 2 {
            break;
        }
        fixture.kernel.maintain_retention()?;
    }
    assert!(generations(&fixture.store)?.len() <= 2);
    assert_eq!(quota.status()?.active_reservations, 0);
    Ok(())
}

#[test]
fn concurrent_reader_remains_consistent_while_automatic_retirement_runs() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.automatic()?;
    let first = fixture.edit(0)?;
    let reader = Kernel::open(fixture.source.path(), Some(fixture.data.path().into()))?;
    let pinned = fixture.store.catalog_at(&first, 1_000_000)?.unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let ready = barrier.clone();
    let worker = std::thread::spawn(move || -> Result<()> {
        ready.wait();
        for _ in 0..100 {
            let metadata = reader.file_info("evidence.md")?;
            assert!(metadata.generation >= first.generation);
            assert!(pinned.file("evidence.md")?.is_some());
            assert_eq!(
                pinned.receipt.as_ref().map(|receipt| receipt.token),
                Some(first)
            );
        }
        Ok(())
    });
    barrier.wait();
    for edit in 1..=12 {
        fixture.edit(edit)?;
    }
    worker.join().expect("concurrent reader panicked")?;
    fixture.kernel.maintain_retention()?;
    assert!(fixture.store.catalog_at(&first, 1_000_000)?.is_none());
    assert!(generations(&fixture.store)?.len() <= 2);
    Ok(())
}

#[test]
fn retention_default_for_new_collection_is_automatic_with_recorded_origin() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let kernel = Kernel::open(source.path(), Some(data.path().into()))?;
    kernel.initialize()?;
    let status = kernel.retention_status()?;
    assert_eq!(status.policy.policy.mode, RetentionMode::Automatic);
    assert!(status.history_supported && status.checkpoints_supported);
    assert_eq!(status.pending_intents, 0);
    assert_eq!(
        status.policy.origin,
        airk_store::retention::PolicyOrigin::NewCollectionDefault
    );
    assert!(
        data.path()
            .join("collections/default/retention.json")
            .is_file()
    );
    Ok(())
}

#[test]
fn retention_default_for_existing_missing_policy_stays_manual_without_read_writes() -> Result<()> {
    let fixture = Fixture::new()?;
    for edit in 0..4 {
        fixture.edit(edit)?;
    }
    let policy = fixture.store.directory().join("retention.json");
    fs::remove_file(&policy)?;
    let retained = generations(&fixture.store)?;
    let reopened = Kernel::open(fixture.source.path(), Some(fixture.data.path().into()))?;
    let status = reopened.retention_status()?;
    assert_eq!(status.policy.policy.mode, RetentionMode::Manual);
    assert_eq!(
        status.policy.origin,
        airk_store::retention::PolicyOrigin::ExistingManual
    );
    assert!(
        !policy.exists(),
        "read-only policy lookup must not persist defaults"
    );
    fs::write(
        fixture.source.path().join("evidence.md"),
        "# Existing collection\nManual retention remains explicit.\n",
    )?;
    reopened.index()?;
    assert_eq!(
        reopened.retention_status()?.policy.policy.mode,
        RetentionMode::Manual
    );
    for revision in retained {
        assert!(
            fixture
                .store
                .directory()
                .join("generations")
                .join(revision)
                .exists()
        );
    }
    Ok(())
}

#[test]
fn idle_worker_reclaims_released_obsolete_reader_without_new_publication() -> Result<()> {
    let fixture = Fixture::new()?;
    fixture.automatic()?;
    fixture.edit(0)?;
    fixture.kernel.scheduled_index(false, false)?;
    let first = fixture.store.persistent_revision()?.unwrap().token.unwrap();
    let reader = fixture
        .store
        .catalog_at(&first, airk_store::blobs::MAX_CATALOG_BYTES)?
        .unwrap();
    for edit in 1..=4 {
        fixture.edit(edit)?;
    }
    let head = fixture.store.persistent_revision()?.unwrap().token.unwrap();
    assert!(head.generation > first.generation + 1);
    let pending = fixture.kernel.retention_status()?;
    assert!(
        pending
            .last_maintenance
            .as_ref()
            .is_some_and(|receipt| receipt.pending)
    );
    assert!(reader.file("evidence.md")?.is_some());
    drop(reader);
    let outcome = fixture.kernel.run_scheduled_once()?;
    assert_eq!(outcome.state, "maintained");
    assert!(
        outcome.report.is_none(),
        "idle maintenance must not reindex sources"
    );
    let maintenance = outcome.maintenance.context("maintenance receipt missing")?;
    assert!(!maintenance.receipt.pending, "{:?}", maintenance.receipt);
    assert_eq!(
        fixture.store.persistent_revision()?.unwrap().token,
        Some(head)
    );
    assert!(
        fixture
            .store
            .catalog_at(&first, airk_store::blobs::MAX_CATALOG_BYTES)?
            .is_none()
    );
    Ok(())
}
