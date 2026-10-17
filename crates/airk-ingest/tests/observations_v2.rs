// SPDX-License-Identifier: Apache-2.0
use airk_core::{
    Snapshot,
    binding::{ArtifactBinding, bind_payload},
};
use airk_ingest::{
    IndexReport, ScanLimits,
    adapters::{legacy::convert_native_legacy, profile::ExtractionPolicy},
    observations::{
        DisappearanceCause as Cause, ReuseBasis, ReuseRequest, ScanObservation, VerifiedReuse,
    },
    scan_observed_v2_with_budgets,
};
use anyhow::Result;
use std::{collections::BTreeMap, fs, path::Path};

fn run(
    root: &Path,
    data: &Path,
    previous: &Snapshot,
    roots: &BTreeMap<String, String>,
    limits: ScanLimits,
) -> Result<(Snapshot, IndexReport, ScanObservation)> {
    scan_observed_v2_with_budgets(
        root,
        data,
        previous,
        roots,
        limits,
        &ExtractionPolicy {
            detail: None,
            version: 1,
            rules: vec![],
        },
        false,
        |_| Ok(None),
        |_, artifact, _| Ok(artifact.clone()),
    )
}

fn empty(root: &Path) -> Result<Snapshot> {
    Ok(Snapshot::empty(
        root.canonicalize()?.to_string_lossy().into_owned(),
    ))
}

#[test]
fn complete_scan_records_deletion_exclusion_oversize_and_extraction_causes() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    for name in [
        "gone.txt",
        "ignored.txt",
        "oversized.txt",
        "binary.txt",
        "broken.pdf",
        "directory.txt",
    ] {
        fs::write(root.path().join(name), "original bounded fixture\n")?;
    }
    // An old PDF artifact can legitimately have existed before a malformed edit.
    let (mut previous, _, _) = run(
        root.path(),
        data.path(),
        &empty(root.path())?,
        &BTreeMap::new(),
        ScanLimits::default(),
    )?;
    let mut old_pdf = previous.artifacts["gone.txt"].clone();
    old_pdf.path = "broken.pdf".into();
    previous.artifacts.insert("broken.pdf".into(), old_pdf);
    fs::remove_file(root.path().join("gone.txt"))?;
    fs::write(root.path().join(".airkignore"), "ignored.txt\n")?;
    fs::write(root.path().join("oversized.txt"), vec![b'x'; 65])?;
    fs::write(root.path().join("binary.txt"), [0, 255])?;
    fs::remove_file(root.path().join("directory.txt"))?;
    fs::create_dir(root.path().join("directory.txt"))?;
    let (_, report, observed) = run(
        root.path(),
        data.path(),
        &previous,
        &BTreeMap::new(),
        ScanLimits {
            file_bytes: 64,
            ..Default::default()
        },
    )?;
    assert!(observed.complete);
    assert_eq!(observed.roots[""], previous.root);
    for (path, cause) in [
        ("gone.txt", Cause::DeletedObserved),
        ("ignored.txt", Cause::ExcludedFromDiscovery),
        ("oversized.txt", Cause::Oversized),
        ("binary.txt", Cause::Unsupported),
        ("broken.pdf", Cause::ExtractionFailed),
        ("directory.txt", Cause::NonRegular),
    ] {
        assert_eq!(observed.disappeared.get(path), Some(&cause), "{path}");
    }
    assert_eq!(report.removed, 6);
    assert!(observed.usage.metadata_probes >= 5);
    Ok(())
}

#[test]
fn removed_and_rebound_aliases_are_not_source_deletions() -> Result<()> {
    let root = tempfile::tempdir()?;
    let first = tempfile::tempdir()?;
    let second = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    for directory in [first.path(), second.path()] {
        fs::write(directory.join("note.txt"), "same bytes\n")?;
    }
    let roots = BTreeMap::from([(
        "docs".into(),
        first.path().canonicalize()?.to_string_lossy().into_owned(),
    )]);
    let (previous, _, _) = run(
        root.path(),
        data.path(),
        &empty(root.path())?,
        &roots,
        ScanLimits::default(),
    )?;
    let (_, _, removed) = run(
        root.path(),
        data.path(),
        &previous,
        &BTreeMap::new(),
        ScanLimits::default(),
    )?;
    assert_eq!(removed.disappeared["@docs/note.txt"], Cause::AliasRemoved);
    let rebound = BTreeMap::from([(
        "docs".into(),
        second.path().canonicalize()?.to_string_lossy().into_owned(),
    )]);
    let (next, _, observed) = run(
        root.path(),
        data.path(),
        &previous,
        &rebound,
        ScanLimits::default(),
    )?;
    assert!(next.artifacts.contains_key("@docs/note.txt"));
    assert_eq!(observed.disappeared["@docs/note.txt"], Cause::AliasRebound);
    assert!(fs::metadata(first.path().join("note.txt"))?.is_file());
    Ok(())
}

#[test]
fn missing_roots_and_reconciliation_budget_exhaustion_abort() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("gone.txt"), "original\n")?;
    let (previous, _, _) = run(
        root.path(),
        data.path(),
        &empty(root.path())?,
        &BTreeMap::new(),
        ScanLimits::default(),
    )?;
    fs::remove_file(root.path().join("gone.txt"))?;
    // Initial root probe + root walk entry consume two units. The previous path
    // probe cannot be hidden outside the configured discovery budget.
    assert!(
        run(
            root.path(),
            data.path(),
            &previous,
            &BTreeMap::new(),
            ScanLimits {
                discovery_entries: 2,
                ..Default::default()
            }
        )
        .is_err()
    );
    let missing = BTreeMap::from([(
        "missing".into(),
        root.path().join("absent").to_string_lossy().into_owned(),
    )]);
    assert!(
        run(
            root.path(),
            data.path(),
            &previous,
            &missing,
            ScanLimits::default()
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn copy_reuses_before_parsing_but_changes_inventory_and_full_scan_bypasses_lookup() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let bytes = b"pub fn original_publication_fixture() -> u32 { 42 }\n";
    fs::write(root.path().join("one.rs"), bytes)?;
    let (previous, _, _) = run(
        root.path(),
        data.path(),
        &empty(root.path())?,
        &BTreeMap::new(),
        ScanLimits::default(),
    )?;
    fs::write(root.path().join("two.rs"), bytes)?;
    let mut lookup_calls = 0;
    let mut lookup = |request: ReuseRequest<'_>| -> Result<Option<VerifiedReuse>> {
        lookup_calls += 1;
        assert_eq!(request.canonical_root, previous.root);
        assert_eq!(request.source_bytes, bytes.len());
        let payload = convert_native_legacy(&previous.artifacts["one.rs"], bytes, request.profile)?;
        let artifact = bind_payload(
            &payload,
            request.profile,
            ArtifactBinding {
                path: request.path,
                source_hash: *request.source_hash,
                source_bytes: request.source_bytes,
            },
        )?;
        Ok(Some(VerifiedReuse {
            artifact,
            basis: if request.path == "one.rs" {
                ReuseBasis::SamePath
            } else {
                ReuseBasis::PriorContent
            },
        }))
    };
    let policy = ExtractionPolicy {
        detail: None,
        version: 1,
        rules: vec![],
    };
    let (next, report, observed) = scan_observed_v2_with_budgets(
        root.path(),
        data.path(),
        &previous,
        &BTreeMap::new(),
        ScanLimits::default(),
        &policy,
        false,
        &mut lookup,
        |_, a, _| Ok(a.clone()),
    )?;
    assert_eq!(lookup_calls, 2);
    assert_eq!(observed.usage.parser_calls, 0);
    assert_eq!(observed.usage.same_path_hits, 1);
    assert_eq!(observed.usage.prior_content_hits, 1);
    assert_eq!(report.indexed, 0);
    assert_eq!(report.reused, 2);
    assert_eq!(next.generation, previous.generation + 1);
    assert!(
        observed.disappeared.is_empty(),
        "reuse does not assert a move"
    );
    assert!(
        next.artifacts["two.rs"]
            .facts
            .iter()
            .filter_map(|fact| fact.code.as_ref())
            .all(|code| code.symbol_id.starts_with("syntax:two.rs:"))
    );
    let (_, full_report, full) = scan_observed_v2_with_budgets(
        root.path(),
        data.path(),
        &next,
        &BTreeMap::new(),
        ScanLimits::default(),
        &policy,
        true,
        |_| -> Result<Option<VerifiedReuse>> { panic!("full scan must bypass lookup") },
        |_, a, _| Ok(a.clone()),
    )?;
    assert_eq!(full.usage.parser_calls, 2);
    assert_eq!(full_report.reused, 0);
    Ok(())
}

#[cfg(unix)]
#[test]
fn reconciliation_never_follows_a_replacement_directory_symlink() -> Result<()> {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    fs::create_dir(root.path().join("child"))?;
    fs::write(root.path().join("child/a.txt"), "original\n")?;
    let (previous, _, _) = run(
        root.path(),
        data.path(),
        &empty(root.path())?,
        &BTreeMap::new(),
        ScanLimits::default(),
    )?;
    fs::remove_file(root.path().join("child/a.txt"))?;
    fs::remove_dir(root.path().join("child"))?;
    symlink(outside.path(), root.path().join("child"))?;
    let (_, _, observed) = run(
        root.path(),
        data.path(),
        &previous,
        &BTreeMap::new(),
        ScanLimits::default(),
    )?;
    assert_eq!(observed.disappeared["child/a.txt"], Cause::NonRegular);
    Ok(())
}

#[test]
fn authoritative_causes_survive_diagnostic_truncation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(root.path().join("original.txt"), "original fixture\n")?;
    let (mut previous, _, _) = run(
        root.path(),
        data.path(),
        &empty(root.path())?,
        &BTreeMap::new(),
        ScanLimits::default(),
    )?;
    let template = previous.artifacts.remove("original.txt").unwrap();
    fs::remove_file(root.path().join("original.txt"))?;
    for number in 0..1002 {
        let path = format!("binary-{number}.txt");
        fs::write(root.path().join(&path), [0, 255])?;
        let mut artifact = template.clone();
        artifact.path = path.clone();
        previous.artifacts.insert(path, artifact);
    }
    let (_, report, observation) = run(
        root.path(),
        data.path(),
        &previous,
        &BTreeMap::new(),
        ScanLimits::default(),
    )?;
    assert_eq!(report.skipped.len(), 1000);
    assert_eq!(report.skipped_truncated, 2);
    assert_eq!(observation.disappeared.len(), 1002);
    assert_eq!(observation.usage.reconciled_paths, 1002);
    assert!(
        observation
            .disappeared
            .values()
            .all(|cause| *cause == Cause::Unsupported)
    );
    Ok(())
}
