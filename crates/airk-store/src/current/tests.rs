// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    FileStore,
    quota::{Quota, Reservation},
};
use airk_core::{
    graph_storage::{
        GraphHead, GraphRecord, GraphRecordOverlay, GraphRecordReader, GraphScope,
        PreparedGraphDelta,
    },
    identity::{Incarnation, Sha256Digest},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const CAPACITY: u64 = 8 * 1024 * 1024;

struct Fixture {
    directory: tempfile::TempDir,
    store: FileStore,
    quota: Quota,
    scope: CurrentScope,
    head: CurrentHead,
}
impl Fixture {
    fn new() -> Result<Self> {
        let fixture = Self::empty()?;
        let lock = fixture.store.lock()?;
        fixture
            .store
            .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY)?
            .initialize(fixture.head.clone())?;
        drop(lock);
        Ok(fixture)
    }
    fn empty() -> Result<Self> {
        let directory = tempfile::tempdir()?;
        fs::create_dir(directory.path().join("source"))?;
        let quota = Quota::open(directory.path().join("data"))?;
        let store = FileStore::new(quota.root().join("collections/project-current"));
        let scope = CurrentScope {
            root: directory
                .path()
                .join("source")
                .to_string_lossy()
                .into_owned(),
            collection: "project-current".into(),
            incarnation: Incarnation::from_bytes([1; 16]),
        };
        let graph_scope = GraphScope {
            root: scope.root.clone(),
            collection: scope.collection.clone(),
            incarnation: scope.incarnation,
            graph_instance: Incarnation::from_bytes([2; 16]),
            specification: Sha256Digest::from_bytes([3; 32]),
            record_bytes: 16,
        };
        let head = CurrentHead {
            version: 1,
            scope: scope.clone(),
            sequence: 0,
            artifact_root: Sha256Digest::from_bytes([4; 32]),
            lexical_commit: Sha256Digest::from_bytes([5; 32]),
            graph_head: GraphHead::empty(graph_scope)?,
        };
        let lock = store.lock()?;
        drop(lock);
        Ok(Self {
            directory,
            store,
            quota,
            scope,
            head,
        })
    }
    fn sealed(&self) -> Result<(Reservation, graph::SealedGraph)> {
        let reservation = self
            .quota
            .reserve(&self.scope.collection, 4 * 1024 * 1024)?;
        let read = self.store.current_read(&self.scope)?;
        let overlay = graph::GraphOverlay::new(
            &reservation,
            read,
            graph::GraphOverlayLimits {
                maximum_bytes: 1024 * 1024,
                ..Default::default()
            },
        )?;
        overlay.append(
            0,
            GraphRecord {
                embedding_key: None,
                live: false,
                bytes: vec![0; 16],
            },
        )?;
        overlay.append(
            1,
            GraphRecord {
                embedding_key: Some(Sha256Digest::from_bytes([6; 32])),
                live: true,
                bytes: vec![7; 16],
            },
        )?;
        Ok((reservation, overlay.seal()?))
    }
    fn target(&self, sealed: &graph::SealedGraph) -> CurrentHead {
        let mut head = self.head.clone();
        head.sequence = 1;
        head.graph_head = sealed.descriptor().target.clone();
        head
    }
    fn prepare(&self, inputs: &dyn CapturedInputs) -> Result<PendingTransaction> {
        let (_reservation, sealed) = self.sealed()?;
        let lock = self.store.lock()?;
        self.store
            .current_write(&lock, &self.quota, &self.scope, CAPACITY)?
            .prepare(
                &self.head.token()?,
                self.target(&sealed),
                &sealed,
                inputs,
                None,
            )
    }
}

#[test]
fn gates_pin_one_request_prepared_is_readable_and_old_tokens_expire() -> Result<()> {
    let fixture = Fixture::new()?;
    let read = fixture.store.current_read(&fixture.scope)?;
    let clone = read.clone();
    let writer = fixture.store.lock()?;
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)
            .is_err()
    );
    drop(read);
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)
            .is_err()
    );
    drop(clone);
    drop(writer);
    let pending = fixture.prepare(&[])?;
    assert_eq!(
        fixture.store.current_read(&fixture.scope)?.token()?,
        fixture.head.token()?
    );
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    current.mark_applying(&pending)?;
    drop(current);
    drop(writer);
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert_eq!(current.pending()?, Some(pending.clone()));
    current.mark_applying(&pending)?;
    let committed = current.commit(&pending, pending.target.lexical_commit)?;
    assert!(committed.maintenance_error.is_none());
    let token = committed.token;
    assert!(current.pending()?.is_none());
    assert!(
        current
            .commit(&pending, pending.target.lexical_commit)
            .is_err()
    );
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(read.token()?, token);
    assert!(read.require_token(&fixture.head.token()?).is_err());
    assert_eq!(read.read(1)?.bytes, vec![7; 16]);
    assert_eq!(read.counts()?.allocated_count, 2);
    Ok(())
}

#[test]
fn failed_capture_rolls_back_graph_rows_and_preserves_orphan_capacity() -> Result<()> {
    struct Failure;
    impl CapturedInputs for Failure {
        fn visit(&self, _: &mut dyn FnMut(&CapturedInput) -> Result<()>) -> Result<()> {
            anyhow::bail!("injected capture failure")
        }
    }
    let fixture = Fixture::new()?;
    let error = fixture.prepare(&Failure).unwrap_err();
    assert!(error.to_string().contains("injected"));
    assert_eq!(
        fixture.store.current_read(&fixture.scope)?.head(),
        &fixture.head
    );
    fixture.quota.status()?;
    let writer = fixture.store.lock()?;
    assert_eq!(fixture.quota.current_grants(&writer)?.len(), 1);
    let current = fixture
        .store
        .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert_eq!(current.phase()?, TransactionPhase::Ready);
    assert!(current.pending()?.is_none());
    assert_eq!(current.release_orphan_grants()?, 1);
    assert!(fixture.quota.current_grants(&writer)?.is_empty());
    Ok(())
}

fn retained_input(fixture: &Fixture, text: &str) -> Result<CapturedInput> {
    use airk_core::extraction::{ExtractionPayload, ExtractionProfile, LocalChunk, LocalSpan};
    use std::collections::BTreeMap;
    let profile = ExtractionProfile {
        contract_version: 1,
        adapter_id: "text".into(),
        adapter_version: "current-test/1".into(),
        language: "text".into(),
        dialect: None,
        grammar_identity: None,
        mapping_version: 1,
        chunk_version: 3,
        options: BTreeMap::new(),
        configuration_inputs: BTreeMap::new(),
    };
    let payload = ExtractionPayload {
        semantic_projection: Default::default(),
        continuity_coverage: Default::default(),
        contract_version: 1,
        extraction_method: "utf8".into(),
        parse_status: "plain".into(),
        provenance: vec![],
        text: text.into(),
        facts: vec![],
        syntax_identities: vec![],
        chunks: vec![LocalChunk {
            span: LocalSpan {
                start: 0,
                end: text.len(),
            },
            start_line: 1,
            end_line: 1,
            location: airk_core::DocumentLocation::Lines { start: 1, end: 1 },
            region: None,
        }],
        regions: vec![],
        relationships: vec![],
        continuity: vec![],
        diagnostics: vec![],
        partial: false,
    };
    let artifact = airk_core::binding::bind_payload(
        &payload,
        &profile,
        airk_core::binding::ArtifactBinding {
            path: "evidence.txt",
            source_hash: Sha256Digest::from_bytes(Sha256::digest(text.as_bytes()).into()),
            source_bytes: text.len(),
        },
    )?;
    let reservation = fixture
        .quota
        .reserve(&fixture.scope.collection, 1024 * 1024)?;
    let artifact = fixture.store.retain_payload_with_budget(
        &artifact,
        &payload,
        &profile,
        Some(text.as_bytes()),
        Some(&reservation),
    )?;
    Ok(CapturedInput { artifact })
}

#[test]
fn replay_uses_captured_bytes_and_detects_corruption_without_live_fallback() -> Result<()> {
    let fixture = Fixture::new()?;
    let original = "Captured source α.\n";
    fs::write(
        std::path::Path::new(&fixture.scope.root).join("evidence.txt"),
        original,
    )?;
    let inputs = [retained_input(&fixture, original)?];
    let pending = fixture.prepare(&inputs)?;
    fs::write(
        std::path::Path::new(&fixture.scope.root).join("evidence.txt"),
        "Later live edit",
    )?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let mut observed = 0;
    current.visit_captured(&pending, &mut |captured| {
        assert_eq!(
            fixture.store.read_retained_source(&captured.artifact)?,
            original.as_bytes()
        );
        observed += 1;
        Ok(())
    })?;
    assert_eq!(observed, 1);
    let path = fixture
        .store
        .directory()
        .join("objects/sources")
        .join(&inputs[0].artifact.content_hash);
    fs::write(path, b"corrupt")?;
    assert!(current.mark_applying(&pending).is_err());
    assert_eq!(current.phase()?, TransactionPhase::Prepared);
    assert_eq!(
        fs::read_to_string(std::path::Path::new(&fixture.scope.root).join("evidence.txt"))?,
        "Later live edit"
    );
    Ok(())
}

#[test]
fn current_scope_and_owned_header_reject_foreign_state_before_recovery() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut foreign = fixture.scope.clone();
    foreign.root.push_str("/other");
    assert!(fixture.store.current_read(&foreign).is_err());
    let writer = fixture.store.lock()?;
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &foreign, CAPACITY)
            .is_err()
    );
    let before = fs::read(fixture.store.directory().join("index.sqlite3"))?;
    fs::write(
        fixture.store.directory().join("index.sqlite3-journal"),
        b"not a journal",
    )?;
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &foreign, CAPACITY)
            .is_err()
    );
    assert_eq!(
        before,
        fs::read(fixture.store.directory().join("index.sqlite3"))?
    );
    Ok(())
}

#[test]
fn journal_bound_sector_adapter_and_statement_contract_are_enforced() -> Result<()> {
    assert_eq!(journal_bound(0, 512)?, 2048);
    assert_eq!(journal_bound(10, 4096)?, 10 * (4096 + 8 + 8192) + 16384);
    assert!(journal_bound(u64::MAX, 65536).is_err());
    assert!(journal_bound(2, 513).is_err());
    let fixture = Fixture::new()?;
    let mut connection = database::open_writer(
        &fixture.store.directory().join("index.sqlite3"),
        CAPACITY,
        false,
    )?;
    let sector = database::effective_sector(&mut connection)?;
    assert!(sector.is_power_of_two() && (32..=65536).contains(&sector));
    let mode: String = connection.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
    let sync: u32 = connection.pragma_query_value(None, "synchronous", |r| r.get(0))?;
    assert_eq!(mode, "delete");
    assert_eq!(sync, 3);
    database::restrict(&connection)?;
    for sql in [
        "SAVEPOINT forbidden",
        "ATTACH ':memory:' AS other",
        "PRAGMA journal_mode=OFF",
        "CREATE TEMP TABLE forbidden(a)",
        "CREATE TRIGGER forbidden AFTER UPDATE ON current_state BEGIN SELECT 1; END",
    ] {
        assert!(connection.execute_batch(sql).is_err(), "{sql}");
    }
    for sql in [
        "EXPLAIN UPDATE OR ROLLBACK current_state SET phase='applying' WHERE id=1",
        "EXPLAIN INSERT OR ROLLBACK INTO current_pending_inputs(path,metadata) VALUES('test',x'00')",
    ] {
        let mut statement = connection.prepare(sql)?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let opcode: String = row.get(1)?;
            let p1: i64 = row.get(2)?;
            let p2: i64 = row.get(3)?;
            assert!(
                !((opcode == "Halt" || opcode == "HaltIfNull") && p1 != 0 && p2 == 2),
                "statement contains ABORT conflict path"
            );
        }
    }
    connection.execute_batch(
        "BEGIN; UPDATE OR ROLLBACK current_state SET phase='prepared' WHERE id=1;",
    )?;
    assert!(
        connection
            .execute(
                "INSERT OR ROLLBACK INTO current_state(id,phase,head) VALUES(1,'ready',x'00')",
                []
            )
            .is_err()
    );
    assert!(connection.is_autocommit());
    assert_eq!(
        connection.query_row("SELECT phase FROM current_state", [], |r| r
            .get::<_, String>(0))?,
        "ready"
    );
    Ok(())
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn child(fixture: &Fixture, mode: &str) -> Result<ChildGuard> {
    let ready = fixture.directory.path().join("child-ready");
    if ready.exists() {
        fs::remove_file(&ready)?;
    }
    let mut child = ChildGuard(
        Command::new(std::env::current_exe()?)
            .arg("--exact")
            .arg("current::tests::current_child_process")
            .arg("--nocapture")
            .env("AIRK_CURRENT_CHILD_DATA", fixture.quota.root())
            .env(
                "AIRK_CURRENT_CHILD_SCOPE",
                serde_json::to_string(&fixture.scope)?,
            )
            .env(
                "AIRK_CURRENT_CHILD_HEAD",
                serde_json::to_string(&fixture.head)?,
            )
            .env("AIRK_CURRENT_CHILD_READY", &ready)
            .env("AIRK_CURRENT_CHILD_MODE", mode)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()?,
    );
    let started = Instant::now();
    while !ready.exists() {
        ensure!(
            child.0.try_wait()?.is_none(),
            "current test child exited before readiness"
        );
        ensure!(
            started.elapsed() < Duration::from_secs(20),
            "current test child readiness timeout"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(child)
}

#[test]
fn current_child_process() -> Result<()> {
    let Some(root) = std::env::var_os("AIRK_CURRENT_CHILD_DATA") else {
        return Ok(());
    };
    let quota = Quota::open(root)?;
    let scope: CurrentScope = serde_json::from_str(&std::env::var("AIRK_CURRENT_CHILD_SCOPE")?)?;
    let store = FileStore::new(quota.root().join("collections").join(&scope.collection));
    let ready = std::env::var_os("AIRK_CURRENT_CHILD_READY").context("ready path missing")?;
    let mode = std::env::var("AIRK_CURRENT_CHILD_MODE")?;
    if mode == "read" {
        let _read = store.current_read(&scope)?;
        fs::write(ready, b"ready")?;
        loop {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let writer = store.lock()?;
    let mut current = store.current_write(&writer, &quota, &scope, CAPACITY)?;
    if mode.starts_with("bootstrap-") {
        let head: CurrentHead = serde_json::from_str(&std::env::var("AIRK_CURRENT_CHILD_HEAD")?)?;
        return current
            .initialize_observed(head, &mut |stage| {
                let selected = matches!(
                    (mode.as_str(), stage),
                    (
                        "bootstrap-before-open",
                        journal::InitializationStage::BeforeDatabaseOpen
                    ) | (
                        "bootstrap-before-commit",
                        journal::InitializationStage::BeforeSchemaCommit
                    ) | (
                        "bootstrap-after-commit",
                        journal::InitializationStage::ReadyCommitted
                    )
                );
                if selected {
                    fs::write(&ready, b"ready")?;
                    loop {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
                Ok(())
            })
            .map(|_| ());
    }
    if mode.starts_with("grow-") {
        return current
            .grow_capacity_observed(CAPACITY * 2, &mut |stage| {
                let selected = matches!(
                    (mode.as_str(), stage),
                    ("grow-before-owner", journal::CapacityStage::BeforeOwner)
                        | ("grow-after-owner", journal::CapacityStage::AfterOwner)
                );
                if selected {
                    fs::write(&ready, b"ready")?;
                    loop {
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
                Ok(())
            })
            .map(|_| ());
    }
    let pending = current.pending()?.context("child pending missing")?;
    current.mark_applying(&pending)?;
    if mode == "hot-apply" {
        let mut connection =
            database::open_writer(&store.directory().join("index.sqlite3"), CAPACITY, false)?;
        let transaction = connection.transaction()?;
        graph::apply(&transaction, pending.id, &pending.graph)?;
        transaction.execute(
            "UPDATE OR ROLLBACK current_state SET phase='ready',head=?1 WHERE id=1",
            [pending.target.canonical_bytes()?],
        )?;
        // Force journal synchronization and dirty database pages to disk while
        // the transaction remains uncommitted, then let the parent kill us.
        transaction.cache_flush()?;
        fs::write(&ready, b"ready")?;
        loop {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    fs::write(ready, b"ready")?;
    loop {
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn process_death_releases_gate_but_never_bypasses_applying_marker() -> Result<()> {
    let fixture = Fixture::new()?;
    let reader_child = child(&fixture, "read")?;
    let writer = fixture.store.lock()?;
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)
            .is_err()
    );
    drop(writer);
    drop(reader_child);
    let pending = fixture.prepare(&[])?;
    let child = child(&fixture, "applying")?;
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    drop(child);
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    fixture.quota.status()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert_eq!(current.pending()?, Some(pending.clone()));
    current.commit(&pending, pending.target.lexical_commit)?;
    assert_eq!(current.release_orphan_grants()?, 0);
    drop(current);
    drop(writer);
    assert_eq!(
        fixture.store.current_read(&fixture.scope)?.read(1)?.bytes,
        vec![7; 16]
    );
    Ok(())
}

#[test]
fn initialization_replays_exact_captured_bootstrap_after_process_death() -> Result<()> {
    for mode in [
        "bootstrap-before-open",
        "bootstrap-before-commit",
        "bootstrap-after-commit",
    ] {
        let fixture = Fixture::empty()?;
        let child = child(&fixture, mode)?;
        drop(child);
        fixture.quota.status()?;
        let writer = fixture.store.lock()?;
        assert_eq!(fixture.quota.current_grants(&writer)?.len(), 1);
        let mut current =
            fixture
                .store
                .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
        assert_eq!(current.head()?, fixture.head);
        assert_eq!(current.initialize(fixture.head.clone())?, fixture.head);
        assert!(fixture.quota.current_grants(&writer)?.is_empty());
        drop(current);
        drop(writer);
        assert_eq!(
            fixture.store.current_read(&fixture.scope)?.head(),
            &fixture.head
        );
    }
    Ok(())
}

#[test]
fn oversized_recovery_journal_is_rejected_before_database_open() -> Result<()> {
    let fixture = Fixture::new()?;
    let writer = fixture.store.lock()?;
    let _grant = fixture.quota.reserve_current(
        &writer,
        crate::quota::CurrentGrantScope {
            collection: fixture.scope.collection.clone(),
            incarnation: fixture.scope.incarnation,
            transaction: Sha256Digest::from_bytes([99; 32]),
        },
        1024,
    )?;
    let before = fs::read(fixture.store.directory().join("index.sqlite3"))?;
    fs::write(
        fixture.store.directory().join("index.sqlite3-journal"),
        vec![0u8; 1025],
    )?;
    let error = fixture
        .store
        .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)
        .err()
        .context("expected recovery capacity failure")?;
    assert!(
        error
            .to_string()
            .contains("journal exceeds durable allowance")
    );
    assert_eq!(
        before,
        fs::read(fixture.store.directory().join("index.sqlite3"))?
    );
    Ok(())
}

#[test]
fn hot_final_apply_rolls_back_then_replays_exact_afterimages() -> Result<()> {
    let fixture = Fixture::new()?;
    let pending = fixture.prepare(&[])?;
    let child = child(&fixture, "hot-apply")?;
    drop(child);
    let journal = fs::read(fixture.store.directory().join("index.sqlite3-journal"))?;
    assert!(journal.len() > 28);
    assert_eq!(
        &journal[..8],
        &[0xd9, 0xd5, 0x05, 0xf9, 0x20, 0xa1, 0x63, 0xd7]
    );
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    let writer = fixture.store.lock()?;
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY * 2)
            .is_err()
    );
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert_eq!(current.head()?, fixture.head);
    assert_eq!(current.phase()?, TransactionPhase::Applying);
    assert_eq!(current.pending()?, Some(pending.clone()));
    let committed = current.commit(&pending, pending.target.lexical_commit)?;
    assert!(committed.maintenance_error.is_none());
    drop(current);
    drop(writer);
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(read.counts()?.allocated_count, 2);
    assert_eq!(read.read(1)?.bytes, vec![7; 16]);
    Ok(())
}

#[test]
fn admitted_capacity_growth_preserves_head_and_reopens_with_captured_limit() -> Result<()> {
    let fixture = Fixture::new()?;
    let before = fixture.store.current_read(&fixture.scope)?.token()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let changed = current.grow_capacity(CAPACITY * 2)?;
    assert_eq!(changed.capacity_bytes, CAPACITY * 2);
    assert!(changed.maintenance_error.is_none());
    assert_eq!(current.database_capacity_bytes(), CAPACITY * 2);
    assert_eq!(current.head()?, fixture.head);
    drop(current);
    assert!(
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)
            .is_err()
    );
    let current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY * 2)?;
    assert_eq!(current.release_orphan_grants()?, 0);
    drop(current);
    drop(writer);
    assert_eq!(fixture.quota.status()?.reserved_bytes, 0);
    assert_eq!(fixture.store.current_read(&fixture.scope)?.token()?, before);
    let (_reservation, sealed) = fixture.sealed()?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY * 2)?;
    let pending = current.prepare(&before, fixture.target(&sealed), &sealed, &[], None)?;
    assert_eq!(pending.database_capacity_bytes, CAPACITY * 2);
    current.mark_applying(&pending)?;
    let committed = current.commit(&pending, pending.target.lexical_commit)?;
    assert!(committed.maintenance_error.is_none());
    drop(current);
    drop(writer);
    assert!(
        fixture
            .store
            .current_read(&fixture.scope)?
            .require_token(&before)
            .is_err()
    );
    Ok(())
}

#[test]
fn capacity_growth_rejects_quota_pressure_and_prepared_transactions() -> Result<()> {
    let fixture = Fixture::new()?;
    let status = fixture.quota.status()?;
    fixture.quota.configure(crate::quota::QuotaPolicy {
        root_bytes: status.used_bytes + CAPACITY + 128 * 1024,
        default_collection_bytes: status.used_bytes + CAPACITY + 128 * 1024,
        collections: Default::default(),
    })?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    let error = current
        .grow_capacity(CAPACITY * 2)
        .err()
        .context("growth should fail under quota")?;
    assert!(error.is::<crate::quota::QuotaPressure>());
    assert_eq!(current.database_capacity_bytes(), CAPACITY);
    assert_eq!(
        database::check_owner(fixture.store.directory(), &fixture.scope)?,
        CAPACITY
    );
    drop(current);
    drop(writer);
    assert_eq!(fixture.quota.status()?.reserved_bytes, 0);
    assert_eq!(
        fixture.store.current_read(&fixture.scope)?.head(),
        &fixture.head
    );

    let fixture = Fixture::new()?;
    let pending = fixture.prepare(&[])?;
    let writer = fixture.store.lock()?;
    let mut current =
        fixture
            .store
            .current_write(&writer, &fixture.quota, &fixture.scope, CAPACITY)?;
    assert!(current.grow_capacity(CAPACITY * 2).is_err());
    current.abort_prepared(&pending)?;
    Ok(())
}

#[test]
fn killed_capacity_growth_keeps_old_or_new_bound_and_releases_only_orphans() -> Result<()> {
    for (mode, expected) in [
        ("grow-before-owner", CAPACITY),
        ("grow-after-owner", CAPACITY * 2),
    ] {
        let fixture = Fixture::new()?;
        let before = fixture.store.current_read(&fixture.scope)?.token()?;
        let child = child(&fixture, mode)?;
        assert!(fixture.quota.status()?.reserved_bytes > 0);
        drop(child);
        assert_eq!(fixture.store.current_read(&fixture.scope)?.token()?, before);
        assert_eq!(
            database::check_owner(fixture.store.directory(), &fixture.scope)?,
            expected
        );
        let writer = fixture.store.lock()?;
        let current =
            fixture
                .store
                .current_write(&writer, &fixture.quota, &fixture.scope, expected)?;
        assert_eq!(current.release_orphan_grants()?, 1);
        drop(current);
        drop(writer);
        assert_eq!(fixture.quota.status()?.reserved_bytes, 0);
    }
    Ok(())
}
