// SPDX-License-Identifier: Apache-2.0
use super::current_inspection::InspectCurrent;
use airk_core::{FactQuery, extraction::IndexDetail};
use airk_kernel::{Kernel, extraction::ExtractionPolicy, limits::ScanLimits};
use airk_store::FileStore;
use anyhow::Result;
use std::fs;

fn write_retention_observations(
    name: &str,
    observations: &[serde_json::Value],
    limits: &str,
) -> Result<()> {
    let Some(directory) = std::env::var_os("AIRK_P11B_RESOURCE_OUTPUT_DIR") else {
        return Ok(());
    };
    let directory = std::path::PathBuf::from(directory);
    anyhow::ensure!(
        directory.is_absolute()
            && fs::symlink_metadata(&directory)?.is_dir()
            && !fs::symlink_metadata(&directory)?.file_type().is_symlink(),
        "explicit existing owned report directory required"
    );
    let bytes = serde_json::to_vec_pretty(
        &serde_json::json!({"schema":"airk.p11b.retention-observations/1","observations":observations,"limits":limits}),
    )?;
    anyhow::ensure!(
        bytes.len() <= 1 << 20,
        "bounded retention observations exceeded1MiB"
    );
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(directory.join(name))?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn record_kernel_retention(
    observations: &mut Vec<serde_json::Value>,
    phase: &str,
    started: std::time::Instant,
    data: &std::path::Path,
    report: Option<&airk_ingest::IndexReport>,
) -> Result<()> {
    if std::env::var_os("AIRK_P11B_RESOURCE_OUTPUT_DIR").is_some() {
        let wall_us = started.elapsed().as_micros();
        let quota = airk_store::quota::Quota::open_service(data)?.status()?;
        observations.push(serde_json::json!({"phase":phase,"wall_us":wall_us,"quota":quota,"index_report":report}));
    }
    Ok(())
}

fn facts() -> FactQuery {
    FactQuery {
        path_prefix: String::new(),
        name: None,
        kind: None,
        language: None,
        exact_name: false,
        path_exact: false,
        document_only: false,
        limit: 100,
        max_bytes: 65536,
    }
}

#[test]
fn levels_rebuild_cache_preserve_citations_and_report_published_coverage() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = "pub fn detail_fixture() { println!(\"exact original text\"); other(); }\n";
    fs::write(root.path().join("sample.rs"), source)?;
    fs::write(
        root.path().join("notes.md"),
        "# Original heading\n\nThe client must keep exact citations.\n",
    )?;
    fs::write(
        root.path().join("host.md"),
        "# Embedded\n\n```javascript\nfunction embedded() { console.log('original'); }\n```\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(
        kernel.extraction_policy()?.configured_detail,
        IndexDetail::AgentLean
    );
    kernel.index()?;
    let before = kernel.facts(&facts())?;
    assert_eq!(before.indexing.level, IndexDetail::AgentLean);
    assert!(before.facts.iter().any(|f| f.kind == "definition"));
    assert!(before.facts.iter().any(|f| f.kind == "call"));
    assert_eq!(kernel.index()?.reused, 3);
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let lean = store.current_test_catalog()?.unwrap();
    let first_profile = lean.inventory.as_ref().unwrap().files["sample.rs"]
        .state
        .profile_id;
    drop(lean);
    kernel.configure_index_detail(IndexDetail::Detailed)?;
    assert!(kernel.extraction_policy()?.pending_reindex);
    assert_eq!(
        kernel.facts(&facts())?.indexing.level,
        IndexDetail::AgentLean
    );
    let report = kernel.index()?;
    assert_eq!(report.indexed, 3);
    assert_eq!(report.reused, 0);
    let detailed = kernel.facts(&facts())?;
    assert_eq!(detailed.indexing.level, IndexDetail::Detailed);
    assert!(detailed.facts.iter().any(|f| f.kind == "call"));
    let catalog = store.current_test_catalog()?.unwrap();
    assert_ne!(
        catalog.inventory.as_ref().unwrap().files["sample.rs"]
            .state
            .profile_id,
        first_profile
    );
    drop(catalog);
    kernel.configure_index_detail(IndexDetail::Essential)?;
    // A rejected rebuild must retain the previously published level and facts.
    let limits = kernel.scan_limits()?;
    kernel.configure_scan_limits(ScanLimits { files: 1, ..limits })?;
    assert!(kernel.index().is_err());
    assert_eq!(
        kernel.facts(&facts())?.indexing.level,
        IndexDetail::Detailed
    );
    assert!(kernel.extraction_policy()?.pending_reindex);
    kernel.configure_scan_limits(limits)?;
    assert_eq!(kernel.index()?.indexed, 3);
    let essential = kernel.facts(&facts())?;
    assert!(essential.facts.is_empty());
    assert_eq!(essential.indexing.level, IndexDetail::Essential);
    assert!(essential.indexing.note.contains("not indexed"));
    let catalog = store.current_test_catalog()?.unwrap();
    use airk_core::ArtifactSource;
    let artifact = catalog.load_artifact("sample.rs")?;
    assert_eq!(
        artifact
            .chunks
            .iter()
            .map(|c| c.text.as_str())
            .collect::<String>(),
        source
    );
    assert!(
        catalog
            .inventory
            .as_ref()
            .unwrap()
            .files
            .values()
            .all(|f| f.continuity.is_empty())
    );
    assert_eq!(fs::read_to_string(root.path().join("sample.rs"))?, source);
    assert_eq!(
        Kernel::open(root.path(), Some(data.path().into()))?
            .extraction_policy()?
            .configured_detail,
        IndexDetail::Essential
    );
    let other = Kernel::open_collection(root.path(), Some(data.path().into()), "other")?;
    assert_eq!(
        other.extraction_policy()?.configured_detail,
        IndexDetail::AgentLean
    );
    assert_eq!(other.extraction_policy()?.published_detail, None);
    Ok(())
}

#[test]
fn legacy_policy_stays_detailed_and_detail_selection_keeps_adapter_rules() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("sample.rs"),
        "pub fn original() { other(); }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    let legacy: ExtractionPolicy = serde_json::from_str(
        r#"{"version":1,"rules":[{"selector":{"kind":"exact","path":"sample.rs"},"case_sensitive":true,"adapter_id":"rust"}]}"#,
    )?;
    kernel.configure_extraction_policy(legacy.clone())?;
    kernel.index()?;
    assert_eq!(
        kernel.extraction_policy()?.published_detail,
        Some(IndexDetail::Detailed)
    );
    kernel.configure_index_detail(IndexDetail::Balanced)?;
    assert_eq!(kernel.extraction_policy()?.policy.rules, legacy.rules);
    kernel.index()?;
    assert!(!kernel.extraction_policy()?.pending_reindex);
    Ok(())
}

#[test]
fn current_fact_reads_hold_publication_gate_until_request_release() -> Result<()> {
    use super::current_inspection::read_current;
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        root.path().join("sample.rs"),
        "pub fn before_publication() {}\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.index()?;
    let store = FileStore::new(data.path().join("collections-1.2/default"));
    let request = read_current(&store)?;
    let response = request.native_facts_page(&store, &facts(), false, &Default::default())?;
    assert_eq!(response.facts.len(), 1);
    fs::write(
        root.path().join("sample.rs"),
        "pub fn after_publication() {}\n",
    )?;
    assert!(kernel.index().is_err());
    assert_eq!(
        request
            .native_facts_page(&store, &facts(), false, &Default::default())?
            .facts[0]
            .name,
        "before_publication"
    );
    drop(request);
    kernel.index()?;
    assert_eq!(kernel.facts(&facts())?.facts[0].name, "after_publication");
    assert_eq!(kernel.maintain_current()?.removed_files, 0);
    Ok(())
}

#[test]
fn explicit_native_retention_migration_keeps_old_publication_then_reuses_only_equal_effective_files()
-> Result<()> {
    use airk_core::retention::RetainedProfile;
    let mut observations = Vec::new();
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let files = [
        ("sample.rs", "pub fn retained() { called(); }\n"),
        (
            "Cargo.toml",
            "[package]\nname = \"retained\"\nversion = \"0.1.0\"\n[dependencies]\nserde = \"1\"\n",
        ),
        (
            "notes.md",
            "# Requirements\n\nThe system must retain 200 ms and its date 2026-11-05.\n",
        ),
        ("fallback.txt", "The old text evidence stays unchanged.\n"),
    ];
    for (path, text) in files {
        fs::write(root.path().join(path), text)?;
    }
    let kernel =
        Kernel::open_service_collection(root.path(), Some(data.path().into()), "retained")?;
    kernel.configure_vectors(false)?;
    record_kernel_retention(
        &mut observations,
        "owned_before_cold",
        std::time::Instant::now(),
        data.path(),
        None,
    )?;
    let phase_started = std::time::Instant::now();
    let cold = kernel.index()?;
    record_kernel_retention(
        &mut observations,
        "legacy_cold",
        phase_started,
        data.path(),
        Some(&cold),
    )?;
    let registration = airk_core::service::ProjectRegistration::new(
        root.path().canonicalize()?.to_string_lossy().into_owned(),
        data.path().canonicalize()?.to_string_lossy().into_owned(),
        "retained".into(),
    )?;
    let collection = airk_store::service::collection::Collection::existing(
        data.path(),
        &registration.root,
        "retained",
    )?;
    let store = collection.selected_store()?;
    let before = store.current_test_catalog()?.unwrap();
    let token = before.token;
    let fallback_id = before.inventory.as_ref().unwrap().files["fallback.txt"]
        .state
        .profile_id;
    drop(before);
    let phase_started = std::time::Instant::now();
    let review = kernel.preview_retained_migration(RetainedProfile::Lean)?;
    let configured = kernel.configure_retained_migration(&review)?;
    assert!(configured.source_pass_requested);
    let pending = kernel.extraction_policy()?;
    assert_eq!(
        pending.configured_retained_profile,
        Some(RetainedProfile::Lean)
    );
    assert_eq!(pending.published_retained_profile, None);
    assert!(pending.pending_reindex);
    assert_eq!(pending.publication, Some(token));
    let migrated = kernel.index()?;
    record_kernel_retention(
        &mut observations,
        "lean_migration",
        phase_started,
        data.path(),
        Some(&migrated),
    )?;
    assert_eq!(migrated.indexed, 3);
    assert_eq!(migrated.reused, 1);
    let status = kernel.extraction_policy()?;
    assert_eq!(
        status.published_retained_profile,
        Some(RetainedProfile::Lean)
    );
    assert!(!status.pending_reindex);
    let after = store.current_test_catalog()?.unwrap();
    assert_eq!(
        after.inventory.as_ref().unwrap().files["fallback.txt"]
            .state
            .profile_id,
        fallback_id
    );
    let applied_refs = after
        .snapshot
        .artifacts
        .iter()
        .map(|(path, artifact)| (path.clone(), artifact.retained.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    drop(after);
    let retained_facts = kernel.facts(&facts())?;
    assert!(
        retained_facts
            .facts
            .iter()
            .any(|fact| fact.kind == "package_dependency")
    );
    assert_eq!(
        retained_facts.indexing.retained_profile,
        Some(RetainedProfile::Lean)
    );
    let pending_rich = kernel.preview_retained_migration(RetainedProfile::Rich)?;
    assert!(
        kernel
            .configure_retained_migration(&pending_rich)?
            .source_pass_requested
    );
    let pending_choice = kernel.extraction_policy()?;
    assert_eq!(
        pending_choice.configured_retained_profile,
        Some(RetainedProfile::Rich)
    );
    assert_eq!(
        pending_choice.published_retained_profile,
        Some(RetainedProfile::Lean)
    );
    // A real failed source pass must preserve the earlier selected publication,
    // while the saved desired policy remains independently observable/reversible.
    let quota = airk_store::quota::Quota::open_service(data.path())?;
    let ordinary = quota.policy()?;
    let observed = quota.status()?;
    let mut full = ordinary.clone();
    full.root_bytes = observed.used_bytes + observed.reserved_bytes;
    quota.configure(full)?;
    let phase_started = std::time::Instant::now();
    let failed = kernel.index();
    quota.configure(ordinary)?;
    record_kernel_retention(
        &mut observations,
        "rich_quota_refused",
        phase_started,
        data.path(),
        failed.as_ref().ok(),
    )?;
    assert!(
        failed.is_err(),
        "Rich source publication unexpectedly bypassed full native quota"
    );
    let preserved = store.current_test_catalog()?.unwrap();
    assert_eq!(Some(preserved.token), pending_choice.publication);
    for (path, expected) in &applied_refs {
        assert_eq!(
            &preserved.snapshot.artifacts[path].retained, expected,
            "failed migration altered retained publication"
        );
    }
    drop(preserved);
    let restore_lean = kernel.preview_retained_migration(RetainedProfile::Lean)?;
    assert!(
        !kernel
            .configure_retained_migration(&restore_lean)?
            .source_pass_requested
    );
    let restored = kernel.extraction_policy()?;
    assert_eq!(
        restored.configured_retained_profile,
        Some(RetainedProfile::Lean)
    );
    assert_eq!(
        restored.published_retained_profile,
        Some(RetainedProfile::Lean)
    );
    assert!(
        !restored.pending_reindex,
        "no-op publication choice left another pending desired policy saved"
    );
    let phase_started = std::time::Instant::now();
    let noop = kernel.preview_retained_migration(RetainedProfile::Lean)?;
    assert!(
        !kernel
            .configure_retained_migration(&noop)?
            .source_pass_requested
    );
    let unchanged = kernel.index()?;
    record_kernel_retention(
        &mut observations,
        "lean_noop",
        phase_started,
        data.path(),
        Some(&unchanged),
    )?;
    assert_eq!(unchanged.indexed, 0);
    assert_eq!(unchanged.reused, 4);
    let unchanged_snapshot = store.current_test_catalog()?.unwrap();
    for (path, expected) in &applied_refs {
        assert_eq!(
            &unchanged_snapshot.snapshot.artifacts[path].retained, expected,
            "no-op changed retained payload/profile/facts/semantic projection object"
        );
    }
    drop(unchanged_snapshot);
    let phase_started = std::time::Instant::now();
    let mut rules = kernel.extraction_policy()?.policy;
    rules
        .rules
        .push(airk_ingest::adapters::profile::SelectionRule {
            selector: airk_ingest::adapters::profile::RuleMatch::Exact {
                path: "notes.md".into(),
            },
            case_sensitive: true,
            adapter_id: "text".into(),
        });
    kernel.configure_extraction_policy(rules)?;
    let overridden = kernel.index()?;
    record_kernel_retention(
        &mut observations,
        "lean_one_rule_override",
        phase_started,
        data.path(),
        Some(&overridden),
    )?;
    assert_eq!(overridden.indexed, 1);
    assert_eq!(overridden.reused, 3);
    let overridden_snapshot = store.current_test_catalog()?.unwrap();
    for path in ["sample.rs", "Cargo.toml", "fallback.txt"] {
        assert_eq!(
            &overridden_snapshot.snapshot.artifacts[path].retained, &applied_refs[path],
            "unrelated policy rule invalidated retained evidence"
        );
    }
    let before_edit_refs = overridden_snapshot
        .snapshot
        .artifacts
        .iter()
        .map(|(path, artifact)| (path.clone(), artifact.retained.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    drop(overridden_snapshot);
    let phase_started = std::time::Instant::now();
    fs::write(
        root.path().join("sample.rs"),
        "pub fn changed() { called(); }\n",
    )?;
    let edited = kernel.index()?;
    record_kernel_retention(
        &mut observations,
        "lean_one_source_edit",
        phase_started,
        data.path(),
        Some(&edited),
    )?;
    assert_eq!(edited.indexed, 1);
    assert_eq!(edited.reused, 3);
    let edited_snapshot = store.current_test_catalog()?.unwrap();
    for path in ["notes.md", "Cargo.toml", "fallback.txt"] {
        assert_eq!(
            &edited_snapshot.snapshot.artifacts[path].retained, &before_edit_refs[path],
            "one source edit invalidated unrelated payload/profile/projection"
        );
    }
    drop(edited_snapshot);
    assert!(!kernel.status()?.vectors_enabled);
    assert!(
        kernel.configure_retained_migration(&review).is_err(),
        "old publication review must not become fresh retry authority"
    );
    for (path, text) in files.into_iter().filter(|(path, _)| *path != "sample.rs") {
        assert_eq!(fs::read_to_string(root.path().join(path))?, text);
    }
    write_retention_observations(
        "kernel-retention.json",
        &observations,
        "Wall time covers the named existing action, excluding quota observation. Quota is logical used/reserved including retained publication history; not allocated physical bytes, peak RSS, CPU or a pruned-format comparison. Vectors are disabled in this fixture; model cost is observed separately by actual helper.",
    )?;
    Ok(())
}
