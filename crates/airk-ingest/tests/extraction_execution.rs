// SPDX-License-Identifier: Apache-2.0
use airk_core::{Snapshot, extraction::IndexDetail};
use airk_ingest::{
    ExtractionExecution, IndexReport, ScanLimits,
    adapters::profile::ExtractionPolicy,
    observations::{ReuseBasis, ScanObservation, VerifiedReuse},
    scan_payloads_with_execution,
};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Write, path::Path, time::Instant};

type Captured = BTreeMap<String, String>;

fn empty(root: &Path) -> Result<Snapshot> {
    Ok(Snapshot::empty(
        root.canonicalize()?.to_string_lossy().into_owned(),
    ))
}

fn policy(detail: IndexDetail) -> ExtractionPolicy {
    ExtractionPolicy {
        version: 3,
        detail: Some(detail),
        rules: vec![],
    }
}

fn run(
    root: &Path,
    data: &Path,
    previous: &Snapshot,
    detail: IndexDetail,
    execution: ExtractionExecution,
) -> Result<(Snapshot, IndexReport, ScanObservation, Captured)> {
    let mut payloads = BTreeMap::new();
    let (snapshot, report, observation) = scan_payloads_with_execution(
        root,
        data,
        previous,
        &previous.roots,
        ScanLimits::default(),
        &policy(detail),
        false,
        execution,
        |request| {
            Ok(previous
                .artifacts
                .get(request.path)
                .filter(|old| old.content_hash == request.source_hash.to_string())
                .cloned()
                .map(|artifact| VerifiedReuse {
                    artifact,
                    basis: ReuseBasis::SamePath,
                }))
        },
        |_, artifact, _, payload| {
            if let Some(payload) = payload {
                payloads.insert(
                    artifact.path.clone(),
                    format!("{:x}", Sha256::digest(serde_json::to_vec(payload)?)),
                );
            }
            Ok(artifact.clone())
        },
    )?;
    Ok((snapshot, report, observation, payloads))
}

fn mixed(root: &Path) -> Result<()> {
    for (path, text) in [
        (
            "a.rs",
            "pub fn alpha(value: i32) -> i32 { helper(value) }\nfn helper(v:i32)->i32{v+1}\n",
        ),
        (
            "b.md",
            "# Guide\n\nA paragraph with exact source locations.\n\n```rust\nfn nested() { alpha(2); }\n```\n",
        ),
        (
            "c.py",
            "class Worker:\n    def run(self):\n        return helper('value')\n",
        ),
        (
            "d.ipynb",
            r##"{"cells":[{"cell_type":"markdown","source":["# Escaped\n","A paragraph with \\\"quoted\\\" content."]},{"cell_type":"code","source":["def notebook():\n","    return 7\n"],"metadata":{},"outputs":[],"execution_count":null}],"metadata":{"kernelspec":{"language":"python"}},"nbformat":4,"nbformat_minor":5}"##,
        ),
    ] {
        fs::write(root.join(path), text)?;
    }
    Ok(())
}

#[test]
fn one_two_four_workers_preserve_exact_payloads_selected_facts_and_locations() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    mixed(root.path())?;
    let previous = empty(root.path())?;
    for detail in [
        IndexDetail::Essential,
        IndexDetail::Balanced,
        IndexDetail::Detailed,
    ] {
        let (baseline, serial, _, payloads) = run(
            root.path(),
            data.path(),
            &previous,
            detail,
            ExtractionExecution::default(),
        )?;
        assert_eq!(serial.indexed, 4, "{:?}", serial.skipped);
        assert_eq!(serial.extraction_workers_spawned, 0);
        assert_eq!(serial.extraction_serial_jobs, 4);
        for workers in [2, 4] {
            let (parallel, report, observation, actual) = run(
                root.path(),
                data.path(),
                &previous,
                detail,
                ExtractionExecution {
                    workers,
                    ..Default::default()
                },
            )?;
            assert_eq!(
                actual, payloads,
                "{detail:?}/{workers} payload bytes changed"
            );
            assert_eq!(
                serde_json::to_vec(&parallel)?,
                serde_json::to_vec(&baseline)?
            );
            assert_eq!(report.extraction_workers_spawned, workers);
            assert_eq!(report.extraction_parallel_jobs, 4);
            assert_eq!(
                report.extraction_result_bound_bytes,
                (workers * ExtractionExecution::MAX_RESULT_BYTES) as u64
            );
            assert_eq!(observation.usage.parser_calls, 4);
            let (_, warm, observed, fresh) = run(
                root.path(),
                data.path(),
                &parallel,
                detail,
                ExtractionExecution {
                    workers,
                    ..Default::default()
                },
            )?;
            assert!(fresh.is_empty());
            assert_eq!(warm.reused, 4);
            assert_eq!(warm.extraction_workers_spawned, 0);
            assert_eq!(
                warm.extraction_parallel_jobs + warm.extraction_serial_jobs,
                0
            );
            assert_eq!(
                observed.usage.source_read_bytes,
                observation.usage.source_read_bytes
            );
            assert_eq!(observed.usage.parser_calls, 0);
        }
    }
    Ok(())
}

#[test]
fn native_expansion_and_large_text_are_serial_and_admission_does_not_reject_valid_inputs()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("a.rs"), "fn alpha() {}\n")?;
    let mut archive = zip::ZipWriter::new(fs::File::create(root.path().join("b.docx"))?);
    archive.start_file(
        "word/document.xml",
        zip::write::SimpleFileOptions::default(),
    )?;
    archive.write_all(br#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>Native document paragraph.</w:t></w:r></w:p></w:body></w:document>"#)?;
    archive.finish()?;
    fs::write(root.path().join("c.md"), "# Heading\n\nBody.\n")?;
    fs::write(
        root.path().join("d.txt"),
        "plain bounded text\n".repeat(240_000),
    )?;
    let initial = empty(root.path())?;
    let (serial, _, _, expected) = run(
        root.path(),
        data.path(),
        &initial,
        IndexDetail::Essential,
        ExtractionExecution::default(),
    )?;
    let (parallel, report, _, actual) = run(
        root.path(),
        data.path(),
        &initial,
        IndexDetail::Essential,
        ExtractionExecution {
            workers: 4,
            max_in_flight_source_bytes: 32,
        },
    )?;
    assert_eq!(actual, expected);
    assert_eq!(serde_json::to_vec(&parallel)?, serde_json::to_vec(&serial)?);
    assert_eq!(report.indexed, 4, "{:?}", report.skipped);
    assert_eq!(report.extraction_parallel_jobs, 2);
    assert_eq!(report.extraction_serial_jobs, 2);
    assert!(report.extraction_source_peak_bytes <= 32);
    assert!(
        report.extraction_coordinator_source_peak_bytes
            > ExtractionExecution::MAX_PARALLEL_SOURCE_BYTES as u64
    );
    let (_, tiny, _, actual) = run(
        root.path(),
        data.path(),
        &initial,
        IndexDetail::Essential,
        ExtractionExecution {
            workers: 4,
            max_in_flight_source_bytes: 1,
        },
    )?;
    assert_eq!(actual, expected);
    assert_eq!(tiny.extraction_workers_spawned, 0);
    assert_eq!(tiny.extraction_serial_jobs, 4);
    Ok(())
}

#[test]
fn retention_failure_stops_later_callbacks_and_precedes_later_lookup_failure() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    for name in ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"] {
        fs::write(root.path().join(name), "fn example() {}\n")?;
    }
    let previous = empty(root.path())?;
    for workers in [1, 2, 4] {
        let mut callbacks = vec![];
        let error = scan_payloads_with_execution(
            root.path(),
            data.path(),
            &previous,
            &BTreeMap::new(),
            ScanLimits::default(),
            &policy(IndexDetail::Essential),
            false,
            ExtractionExecution {
                workers,
                ..Default::default()
            },
            |request| {
                anyhow::ensure!(request.path != "c.rs", "later lookup failed");
                Ok(None)
            },
            |_, artifact, _, _| {
                callbacks.push(artifact.path.clone());
                anyhow::bail!("first retention quota failure")
            },
        )
        .unwrap_err();
        assert_eq!(callbacks, ["a.rs"]);
        assert!(error.to_string().contains("first retention quota failure"));
    }
    Ok(())
}

#[test]
fn corpus_budget_failure_and_callbacks_follow_the_same_order_with_all_worker_counts() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    for name in ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"] {
        fs::write(root.path().join(name), "fn example() {}\n")?;
    }
    let previous = empty(root.path())?;
    let mut baseline = None;
    for workers in [1, 2, 4] {
        let mut callbacks = vec![];
        let error = scan_payloads_with_execution(
            root.path(),
            data.path(),
            &previous,
            &BTreeMap::new(),
            ScanLimits {
                files: 2,
                ..Default::default()
            },
            &policy(IndexDetail::Essential),
            false,
            ExtractionExecution {
                workers,
                ..Default::default()
            },
            |_| Ok(None),
            |_, artifact, _, _| {
                callbacks.push(artifact.path.clone());
                Ok(artifact.clone())
            },
        )
        .unwrap_err()
        .to_string();
        assert_eq!(callbacks.len(), 2);
        assert!(error.contains("configured file count"));
        if let Some(expected) = &baseline {
            assert_eq!(&(callbacks, error), expected);
        } else {
            baseline = Some((callbacks, error));
        }
    }
    Ok(())
}

#[test]
fn same_size_restored_timestamp_changes_are_hashed_even_without_a_watch_event() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let path = root.path().join("a.rs");
    fs::write(&path, "fn before() {}\n")?;
    let modified = fs::metadata(&path)?.modified()?;
    let execution = ExtractionExecution {
        workers: 4,
        ..Default::default()
    };
    let (previous, _, _, _) = run(
        root.path(),
        data.path(),
        &empty(root.path())?,
        IndexDetail::Balanced,
        execution,
    )?;
    fs::write(&path, "fn after_() {}\n")?;
    fs::File::options()
        .write(true)
        .open(&path)?
        .set_times(fs::FileTimes::new().set_modified(modified))?;
    assert_eq!(fs::metadata(&path)?.modified()?, modified);
    let (next, report, observed, _) = run(
        root.path(),
        data.path(),
        &previous,
        IndexDetail::Balanced,
        execution,
    )?;
    assert_eq!(report.indexed, 1);
    assert_eq!(report.reused, 0);
    assert_eq!(
        observed.usage.source_read_bytes,
        previous.artifacts["a.rs"].bytes
    );
    assert_ne!(
        previous.artifacts["a.rs"].content_hash,
        next.artifacts["a.rs"].content_hash
    );
    assert!(
        next.artifacts["a.rs"]
            .facts
            .iter()
            .any(|fact| fact.name == "after_")
    );
    Ok(())
}

/// Run alone in release mode after builds/tests stop; output is measurement data,
/// not a default-selection assertion or a process RSS qualification.
#[test]
#[ignore = "isolated release 1/2/4 extraction throughput qualification"]
fn release_worker_qualification() -> Result<()> {
    anyhow::ensure!(
        !cfg!(debug_assertions),
        "run this qualification with --release"
    );
    let data = tempfile::tempdir()?;
    for corpus in ["code", "prose"] {
        let root = tempfile::tempdir()?;
        for file in 0..96 {
            let (extension, text) = if corpus == "code" {
                (
                    "rs",
                    (0..48)
                        .map(|i| format!("pub fn item_{file}_{i}(x:i32)->i32{{ helper(x)+{i} }}\n"))
                        .collect::<String>(),
                )
            } else {
                ("md", (0..36).map(|i| format!("## Section {file}-{i}\n\nExact source paragraph {i} discusses local indexing, source integrity, and retrieval evidence.\n\n")).collect::<String>())
            };
            fs::write(root.path().join(format!("{file:04}.{extension}")), text)?;
        }
        let initial = empty(root.path())?;
        let mut reference = None;
        for repeat in 0..5 {
            let order = if repeat % 2 == 0 {
                [1, 2, 4]
            } else {
                [4, 2, 1]
            };
            for workers in order {
                let execution = ExtractionExecution {
                    workers,
                    ..Default::default()
                };
                let started = Instant::now();
                let (snapshot, cold, observed, payloads) = run(
                    root.path(),
                    data.path(),
                    &initial,
                    IndexDetail::Balanced,
                    execution,
                )?;
                let cold_us = started.elapsed().as_micros();
                assert_eq!(cold.indexed, 96);
                if let Some(expected) = &reference {
                    assert_eq!(&payloads, expected);
                } else {
                    reference = Some(payloads);
                }
                let started = Instant::now();
                let (_, warm, warm_observed, _) = run(
                    root.path(),
                    data.path(),
                    &snapshot,
                    IndexDetail::Balanced,
                    execution,
                )?;
                let warm_us = started.elapsed().as_micros();
                assert_eq!(warm.reused, 96);
                assert_eq!(
                    warm_observed.usage.source_read_bytes,
                    observed.usage.source_read_bytes
                );
                assert_eq!(warm.extraction_workers_spawned, 0);
                println!(
                    "{}",
                    serde_json::json!({"corpus":corpus,"repeat":repeat,"workers":workers,
                    "cold_us":cold_us,"warm_us":warm_us,"job_wall_us":cold.extraction_work_us,
                    "wait_us":cold.extraction_wait_us,"source_peak_bytes":cold.extraction_source_peak_bytes,
                    "coordinator_source_peak_bytes":cold.extraction_coordinator_source_peak_bytes,
                    "result_bound_bytes":cold.extraction_result_bound_bytes})
                );
            }
        }
        reference.context("missing qualification baseline")?;
    }
    Ok(())
}
