// SPDX-License-Identifier: Apache-2.0
//! Real publication pressure with separate completion and deadline contracts.
use super::*;
use airk_core::identity::RevisionToken;
use airk_ingest::providers::{LiveRequest, ProviderCommand};
use airk_store::{FileStore, quota::QuotaPolicy};
use anyhow::ensure;
use std::{
    collections::BTreeMap,
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Fixture {
    source: tempfile::TempDir,
    data: tempfile::TempDir,
    kernel: Kernel,
}

fn completion_budget() -> MaintenanceBudget {
    MaintenanceBudget {
        max_duration_ms: 30_000,
        ..Default::default()
    }
}

impl Fixture {
    fn new() -> Result<Self> {
        let source = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        Quota::open(data.path())?.configure(QuotaPolicy {
            root_bytes: 512 * 1024 * 1024,
            default_collection_bytes: 128 * 1024 * 1024,
            collections: BTreeMap::new(),
        })?;
        let mut kernel = Kernel::open(source.path(), Some(data.path().into()))?;
        kernel.configure_retention(RetentionPolicy::default())?;
        kernel.retention_test_budget = Some(completion_budget());
        // The first index creates the first real revision. No empty initialization
        // generation or speed-dependent preparatory cleanup can consume the
        // obsolete payloads needed by the pressure scenario.
        Ok(Self {
            source,
            data,
            kernel,
        })
    }

    fn head(&self) -> Result<RevisionToken> {
        self.kernel
            .store
            .persistent_revision()?
            .unwrap()
            .token
            .context("missing head")
    }

    fn tighten(&self, extra: u64) -> Result<()> {
        let quota = Quota::open(self.data.path())?;
        let mut policy = quota.policy()?;
        policy.collections.insert(
            "default".into(),
            quota.status()?.collections["default"].used_bytes + extra,
        );
        quota.configure(policy)?;
        Ok(())
    }

    fn automatic(&mut self, expire: bool) -> Result<Arc<AtomicUsize>> {
        self.kernel.configure_retention(RetentionPolicy {
            mode: RetentionMode::Automatic,
            searchable_predecessors: 1,
            ..Default::default()
        })?;
        let head = self.head()?;
        let store = self.kernel.store.clone();
        let completed = Arc::new(AtomicUsize::new(0));
        let observed = completed.clone();
        self.kernel.retention_test_budget = if expire {
            Some(MaintenanceBudget {
                max_duration_ms: 1000,
                ..Default::default()
            })
        } else {
            Some(completion_budget())
        };
        self.kernel = self.kernel.with_retention_observer(Arc::new(move |stage| {
            if expire && stage == MaintenanceStage::IntentDurable {
                // Exhaust the explicitly shortened test budget before authorization.
                std::thread::sleep(std::time::Duration::from_millis(1100));
            }
            if stage == MaintenanceStage::IntentComplete
                && store.persistent_revision()?.unwrap().token == Some(head)
            {
                observed.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }));
        Ok(completed)
    }

    fn no_reservations(&self) -> Result<()> {
        let status = Quota::open(self.data.path())?.status()?;
        assert_eq!(status.active_reservations, 0);
        assert_eq!(status.reserved_bytes, 0);
        assert!(
            status.collections["default"].used_bytes <= status.collections["default"].limit_bytes
        );
        Ok(())
    }

    fn pending_pressure(&self, error: &anyhow::Error, head: RevisionToken) -> Result<()> {
        ensure!(
            error.chain().any(|cause| cause.is::<QuotaPressure>()),
            "{error:#}"
        );
        ensure!(
            error
                .to_string()
                .contains("after one automatic quota cleanup-and-retry cycle")
                || error
                    .to_string()
                    .contains("after one automatic pre-build cleanup pass"),
            "{error:#}"
        );
        assert_eq!(self.head()?, head);
        let receipt = self
            .kernel
            .retention_status()?
            .last_maintenance
            .context("missing pressure receipt")?;
        assert!(matches!(receipt.trigger, MaintenanceTrigger::Pressure));
        assert_eq!(receipt.head, Some(head));
        assert_eq!(receipt.state, MaintenanceState::Pending);
        assert!(receipt.pending);
        assert!(receipt.pressure.is_none(), "{receipt:?}");
        assert_eq!(receipt.retired_receipts, 0);
        self.no_reservations()
    }

    fn finish(&mut self) -> Result<()> {
        self.kernel.retention_observer = None;
        self.kernel.retention_test_budget = Some(completion_budget());
        let head = self.head()?;
        for _ in 0..8 {
            let run = self
                .kernel
                .maintain_retention_with_budget(completion_budget())?;
            assert!(run.receipt_persisted, "{run:?}");
            assert_eq!(run.receipt.head, Some(head));
            assert!(run.receipt.pressure.is_none(), "{run:?}");
            assert!(
                matches!(
                    run.receipt.state,
                    MaintenanceState::Complete | MaintenanceState::Pending
                ),
                "{run:?}"
            );
            assert_eq!(self.head()?, head);
            self.no_reservations()?;
            if !run.receipt.pending {
                assert_eq!(run.receipt.state, MaintenanceState::Complete);
                assert!(run.receipt.error.is_none(), "{run:?}");
                assert_eq!(self.kernel.retention_status()?.pending_intents, 0);
                assert_eq!(generations(&self.kernel.store)?.len(), 2);
                return Ok(());
            }
        }
        anyhow::bail!("pressure cleanup did not converge")
    }
}

fn generations(store: &FileStore) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(store.directory().join("generations"))? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if airk_store::blobs::valid_digest(&name) {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

fn indexing_pressure(expire: bool) -> Result<()> {
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
    assert_eq!(generations(&fixture.kernel.store)?.len(), 3);
    fixture.tighten(3 * 1024 * 1024)?;
    let before = fixture.head()?;
    fs::write(
        fixture.source.path().join("evidence.md"),
        format!("# Revision 9999\n{body}"),
    )?;
    let retained = generations(&fixture.kernel.store)?;
    let error = fixture
        .kernel
        .index()
        .expect_err("manual publication must hit quota pressure");
    assert!(
        error.chain().any(|cause| cause.is::<QuotaPressure>()),
        "{error:#}"
    );
    assert_eq!(fixture.head()?, before);
    assert_eq!(generations(&fixture.kernel.store)?, retained);
    fixture.no_reservations()?;
    let reclaimed = fixture.automatic(expire)?;
    let result = fixture.kernel.index();
    if expire {
        fixture.pending_pressure(
            &result.expect_err("expired cleanup must not fund publication"),
            before,
        )?;
        assert_eq!(generations(&fixture.kernel.store)?, retained);
        assert_eq!(reclaimed.load(Ordering::SeqCst), 0);
        fixture.finish()?;
        // This is a new explicit request after completed maintenance, not an
        // unbounded automatic retry of the failed request.
        fixture.kernel.index()?;
    } else {
        result?;
        assert!(
            reclaimed.load(Ordering::SeqCst) > 0,
            "publication must require pressure reclamation"
        );
    }
    assert_eq!(fixture.head()?.generation, before.generation + 1);
    fixture.finish()?;
    assert!(fixture.kernel.file_info("evidence.md").is_ok());
    Ok(())
}

#[test]
fn automatic_pressure_rebuilds_with_fresh_admission_after_reclaim() -> Result<()> {
    indexing_pressure(false)
}

#[test]
fn expired_pressure_preserves_head_and_recovers_after_explicit_maintenance() -> Result<()> {
    indexing_pressure(true)
}

fn provider_pressure(expire: bool) -> Result<()> {
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
    let mut fixture = Fixture::new()?;
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
    assert_eq!(generations(&fixture.kernel.store)?.len(), 4);
    fixture.tighten(160 * 1024 * 1024 + 64 * 1024)?;
    let before = fixture.head()?;
    let retained = generations(&fixture.kernel.store)?;
    let reclaimed = fixture.automatic(expire)?;
    let request = LiveRequest {
        command: ProviderCommand {
            program: executable,
            args: vec![],
            timeout_ms: 30_000,
        },
        producer: "original-retention-counter".into(),
        producer_version: "1".into(),
        build_inputs: vec!["Cargo.toml".into()],
        paths: vec!["src/lib.rs".into()],
    };
    let result = fixture.kernel.run_provider("ctags", &request);
    // Both the successful and exhausted automatic retry must reuse the output
    // of exactly one external process execution.
    assert_eq!(
        fs::read_to_string(tools.path().join("invocations"))?,
        "run\n"
    );
    if expire {
        fixture.pending_pressure(
            &result.expect_err("expired cleanup must not fund provider publication"),
            before,
        )?;
        assert_eq!(generations(&fixture.kernel.store)?, retained);
        assert_eq!(reclaimed.load(Ordering::SeqCst), 0);
        fixture.finish()?;
        fixture.kernel.run_provider("ctags", &request)?;
        assert_eq!(
            fs::read_to_string(tools.path().join("invocations"))?,
            "run\nrun\n"
        );
    } else {
        result?;
        assert!(
            reclaimed.load(Ordering::SeqCst) > 0,
            "provider publication must require pressure reclamation"
        );
    }
    assert_eq!(fixture.head()?.generation, before.generation + 1);
    assert!(fixture.kernel.status()?.imported_facts > 0);
    fixture.finish()?;
    Ok(())
}

#[test]
fn quota_retry_reuses_successful_provider_output_without_reexecuting_process() -> Result<()> {
    provider_pressure(false)
}

#[test]
fn expired_provider_retry_runs_once_and_preserves_head() -> Result<()> {
    provider_pressure(true)
}
