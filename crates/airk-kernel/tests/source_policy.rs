// SPDX-License-Identifier: Apache-2.0
//! Permanent exclusions through real native readers and publication paths.
use airk_core::{
    FactQuery, RelationshipQuery, SearchQuery,
    changes::{ChangesRequest, RememberedSelector},
    evidence::EvidenceReadOptions,
    extraction::IndexDetail,
    identity::Incarnation,
    index_catalog::{CatalogRevision, DenyId, IndexId, PathScope, SourceRootId},
    service::ProjectRegistration,
    source_policy::{DenyReason, DenyRule, POLICY_SCHEMA, SourcePolicy, SourceRoot},
};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result, ensure};
use protobuf::{Message, MessageField};
use serde::Serialize;
use std::{collections::BTreeMap, fs, path::Path};

#[test]
fn stable_owner_requires_a_missing_capsule_even_without_a_retained_publication() -> Result<()> {
    let owned = owner()?;
    let root = owned.path().join("source");
    let data = owned.path().join("data");
    fs::create_dir(&root)?;
    fs::create_dir(&data)?;
    let source = root.join("private.rs");
    fs::write(&source, b"pub struct MustNotBeRecaptured;\n")?;
    let kernel = Kernel::open_service_collection(&root, Some(data.clone()), "default")?;
    kernel.configure_vectors(false)?;
    let policy = policy(&kernel, "private.rs", PathScope::Exact)?;
    let bound = kernel.with_expected_source_policy(Some(policy.clone()))?;
    let report = bound.apply_source_policy(&policy, Incarnation::from_bytes([85; 16]))?;
    assert!(report.no_index);
    drop(bound);
    drop(kernel);
    let capsule = data.join("collections-1.4/default/source-policy.json");
    fs::remove_file(capsule)?;
    let reopened = Kernel::open_service_collection(&root, Some(data.clone()), "default")?;
    assert!(
        reopened
            .index()
            .unwrap_err()
            .is::<airk_kernel::SourcePolicyReadUnavailable>()
    );
    assert!(
        reopened
            .prepare_full_rebuild()
            .unwrap_err()
            .is::<airk_kernel::SourcePolicyReadUnavailable>()
    );
    assert!(
        reopened
            .with_expected_source_policy(None)?
            .index()
            .unwrap_err()
            .is::<airk_kernel::SourcePolicyReadUnavailable>()
    );
    let observation = reopened
        .source_policy_observation()?
        .context("required policy was presented as absent")?;
    assert_eq!(observation["state"], "authority_mismatch");
    assert_eq!(
        observation["policy_digest"],
        serde_json::to_value(policy.identity()?)?
    );
    assert_eq!(fs::read(&source)?, b"pub struct MustNotBeRecaptured;\n");
    Ok(())
}

#[cfg(any(windows, target_os = "macos"))]
#[test]
fn unresolved_native_names_refuse_removal_and_new_capture_without_source_changes() -> Result<()> {
    let owned = owner()?;
    let root = owned.path().join("source");
    let data = owned.path().join("data");
    fs::create_dir_all(root.join("allowed"))?;
    fs::create_dir_all(root.join("private"))?;
    fs::create_dir_all(&data)?;
    fs::write(
        root.join("allowed/caf\u{e9}.md"),
        "Unicode content and filename remain allowed under a disjoint ancestor.\n",
    )?;
    fs::write(
        root.join("private/secret.md"),
        "Blocked source remains unchanged.\n",
    )?;
    let kernel = Kernel::open(&root, Some(data))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let denied = policy(&kernel, "private", PathScope::Subtree)?;
    kernel.apply_source_policy(&denied, Incarnation::from_bytes([61; 16]))?;
    assert!(
        kernel
            .list_files("", 100, 65536)?
            .paths
            .iter()
            .any(|path| path == "allowed/caf\u{e9}.md")
    );
    // The boolean walker API must not swallow the unsupported comparison.
    let uncertain = root.join("pr\u{ed}vate.md");
    fs::write(
        &uncertain,
        "New source must not be captured under unknown name equivalence.\n",
    )?;
    let error = kernel.index().unwrap_err();
    assert!(
        format!("{error:#}").contains("unicode_name_equivalence"),
        "{error:#}"
    );
    let error = kernel.coverage(true).unwrap_err();
    assert!(
        format!("{error:#}").contains("unicode_name_equivalence"),
        "{error:#}"
    );
    assert!(
        !kernel
            .list_files("", 100, 65536)?
            .paths
            .iter()
            .any(|path| path == "pr\u{ed}vate.md")
    );
    assert_eq!(
        fs::read_to_string(&uncertain)?,
        "New source must not be captured under unknown name equivalence.\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("private/secret.md"))?,
        "Blocked source remains unchanged.\n"
    );
    // A fresh policy removal must fail before it publishes a partial inventory.
    drop(kernel);
    let separate = owned.path().join("other");
    fs::create_dir_all(&separate)?;
    fs::write(separate.join("caf\u{e9}.md"), "unchanged\n")?;
    let other = Kernel::open(&separate, Some(owned.path().join("other-data")))?;
    other.configure_vectors(false)?;
    other.index()?;
    let uncertain_policy = policy(&other, "cafe", PathScope::Subtree)?;
    let error = other
        .apply_source_policy(&uncertain_policy, Incarnation::from_bytes([62; 16]))
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("unicode_name_equivalence"),
        "{error:#}"
    );
    assert!(other.list_files("", 100, 65536).is_err());
    assert_eq!(
        fs::read_to_string(separate.join("caf\u{e9}.md"))?,
        "unchanged\n"
    );
    Ok(())
}

fn owner() -> Result<tempfile::TempDir> {
    Ok(match std::env::var_os("AIRK_PRIVATE_CENSUS_BASE") {
        Some(base) => tempfile::Builder::new()
            .prefix("airk-policy-readers-")
            .tempdir_in(base)?,
        None => tempfile::tempdir()?,
    })
}

fn coverage_fixture() -> Result<(tempfile::TempDir, Kernel)> {
    let owned = owner()?;
    let root = owned.path().join("source");
    fs::create_dir_all(root.join("private"))?;
    fs::write(root.join("public.md"), "A small public requirement.\n")?;
    fs::write(root.join("private/secret.md"), "A private requirement.\n")?;
    let kernel = Kernel::open_service_collection(
        &canonical(&root)?,
        Some(owned.path().join("data")),
        "coverage",
    )?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    Ok((owned, kernel))
}

#[test]
fn permanent_trim_coverage_cache_cannot_reuse_pre_policy_paths() -> Result<()> {
    let (_owned, kernel) = coverage_fixture()?;
    let before = kernel.coverage(true)?;
    assert_eq!(before.eligible, 2);
    assert!(
        before
            .largest_sources
            .iter()
            .any(|entry| entry.path.starts_with("private/"))
    );
    let denied = policy(&kernel, "private", PathScope::Subtree)?;
    kernel.apply_source_policy(&denied, Incarnation::from_bytes([65; 16]))?;
    let cached = kernel.coverage(false)?;
    assert!(
        cached.largest_sources.is_empty(),
        "pre-policy coverage paths were reused"
    );
    assert!(!cached.scan_complete);
    assert_eq!(cached.percent, None);
    Ok(())
}

#[cfg(unix)]
#[test]
fn permanent_trim_rejects_redirected_ignore_control_before_discovery_or_coverage() -> Result<()> {
    let (owned, kernel) = coverage_fixture()?;
    let denied = policy(&kernel, "private", PathScope::Subtree)?;
    kernel.apply_source_policy(&denied, Incarnation::from_bytes([68; 16]))?;
    let source = owned.path().join("source");
    fs::write(source.join("private/secret.md"), "public.md\n")?;
    std::os::unix::fs::symlink("private/secret.md", source.join(".airkignore"))?;
    let error = kernel.index().unwrap_err();
    assert!(format!("{error:#}").contains("ignore_control"), "{error:#}");
    let error = kernel.coverage(true).unwrap_err();
    assert!(format!("{error:#}").contains("ignore_control"), "{error:#}");
    assert!(
        airk_ingest::source_scope::DirectoryIgnore::with_source_policy(&source, Some(&denied))
            .is_err()
    );
    let evidence = serde_json::to_value(
        kernel.file_info_conditional("public.md", &EvidenceReadOptions::default())?,
    )?;
    let verified = kernel.verify_returned_sources(
        &evidence,
        airk_core::verification::VerificationBudget {
            mode: airk_core::verification::VerificationMode::Report,
            max_sources: 4,
            max_bytes: 65536,
            deadline_ms: 2000,
        },
    )?;
    assert_eq!(verified.counts.bytes_read, 0);
    assert_eq!(verified.sources.len(), 1);
    assert_eq!(
        verified.sources[0].state,
        airk_core::verification::SourceCheckState::Unsupported
    );
    assert_eq!(kernel.list_files("", 100, 65536)?.paths, vec!["public.md"]);
    assert_eq!(
        fs::read_to_string(source.join("private/secret.md"))?,
        "public.md\n"
    );
    Ok(())
}

#[test]
fn permanent_trim_preserves_nested_ignore_precedence_and_explicit_credential_whitelists()
-> Result<()> {
    let (owned, kernel) = coverage_fixture()?;
    let source = owned.path().join("source");
    fs::create_dir(source.join("nested"))?;
    fs::create_dir(source.join("ignored"))?;
    for (path, contents) in [
        (".gitignore", "blocked*.md\nignored/\n"),
        (".airkignore", "!blocked-permit.md\n!.npmrc\n"),
        ("nested/.ignore", "*.md\n"),
        ("nested/.airkignore", "!keep.md\n"),
        ("blocked-drop.md", "An ignored fictional requirement.\n"),
        (
            "blocked-permit.md",
            "A whitelisted fictional requirement.\n",
        ),
        (".npmrc", "fictional-fixture=1\n"),
        ("nested/drop.md", "An ignored nested requirement.\n"),
        ("nested/keep.md", "A whitelisted nested requirement.\n"),
        ("ignored/hidden.md", "An ignored directory requirement.\n"),
    ] {
        fs::write(source.join(path), contents)?;
    }
    kernel.index()?;
    let before = kernel.list_files("", 100, 65536)?.paths;
    assert!(before.iter().any(|path| path == "blocked-permit.md"));
    assert!(before.iter().any(|path| path == ".npmrc"));
    assert!(before.iter().any(|path| path == "nested/keep.md"));
    let denied = policy(&kernel, "private", PathScope::Subtree)?;
    kernel.apply_source_policy(&denied, Incarnation::from_bytes([69; 16]))?;
    kernel.reindex()?;
    let after = kernel.list_files("", 100, 65536)?.paths;
    assert_eq!(
        after,
        before
            .into_iter()
            .filter(|path| !path.starts_with("private/"))
            .collect::<Vec<_>>()
    );
    let evidence = serde_json::to_value(
        kernel.file_info_conditional("blocked-permit.md", &EvidenceReadOptions::default())?,
    )?;
    let verified = kernel.verify_returned_sources(
        &evidence,
        airk_core::verification::VerificationBudget {
            mode: airk_core::verification::VerificationMode::Report,
            max_sources: 4,
            max_bytes: 65536,
            deadline_ms: 2000,
        },
    )?;
    assert_eq!(verified.counts.matched_sources, 1);
    Ok(())
}

#[test]
fn permanent_trim_coverage_refresh_prunes_denied_bytes_and_rebuilds_independently() -> Result<()> {
    let (owned, kernel) = coverage_fixture()?;
    let denied = policy(&kernel, "private", PathScope::Subtree)?;
    kernel.apply_source_policy(&denied, Incarnation::from_bytes([66; 16]))?;
    let secret = vec![b'x'; 1024 * 1024];
    let secret_path = owned.path().join("source/private/secret.md");
    fs::write(&secret_path, &secret)?;
    let coverage = kernel.coverage_with_limits(
        true,
        airk_ingest::inventory::InventoryLimits {
            read_bytes: 128,
            ..Default::default()
        },
    )?;
    assert!(
        coverage.scan_complete,
        "excluded bytes consumed the inventory budget"
    );
    assert!(!coverage.budget_exhausted);
    assert_eq!(coverage.eligible, 1);
    assert_eq!(coverage.current, 1);
    assert_eq!(coverage.percent, Some(100));
    assert_eq!(coverage.largest_sources.len(), 1);
    assert_eq!(coverage.largest_sources[0].path, "public.md");
    kernel.rebuild_service(airk_core::identity::Sha256Digest::from_bytes([67; 32]))?;
    let reopened = Kernel::open_service_collection(
        &canonical(&owned.path().join("source"))?,
        Some(owned.path().join("data")),
        "coverage",
    )?;
    assert_eq!(
        reopened.list_files("", 100, 65536)?.paths,
        vec!["public.md"]
    );
    assert_eq!(fs::read(secret_path)?, secret);
    Ok(())
}

#[test]
fn excluded_ignore_control_cannot_be_read_by_discovery_or_saved_source_verification() -> Result<()>
{
    let owned = owner()?;
    let root = owned.path().join("source");
    fs::create_dir_all(&root)?;
    fs::write(
        root.join(".airkignore"),
        "# Do not read after permanent exclusion.\n",
    )?;
    fs::write(root.join("public.md"), "An otherwise allowed document.\n")?;
    let kernel = Kernel::open(&root, Some(owned.path().join("data")))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let denied = policy(&kernel, ".airkignore", PathScope::Exact)?;
    kernel.apply_source_policy(&denied, Incarnation::from_bytes([63; 16]))?;
    assert_eq!(kernel.list_files("", 100, 65536)?.paths, vec!["public.md"]);
    let evidence = serde_json::to_value(
        kernel.file_info_conditional("public.md", &EvidenceReadOptions::default())?,
    )?;
    let verified = kernel.verify_returned_sources(
        &evidence,
        airk_core::verification::VerificationBudget {
            mode: airk_core::verification::VerificationMode::Report,
            max_sources: 4,
            max_bytes: 65536,
            deadline_ms: 2000,
        },
    )?;
    assert_eq!(
        verified.counts.bytes_read, 0,
        "verification must not read an excluded ignore control"
    );
    assert_eq!(verified.sources.len(), 1);
    assert_eq!(
        verified.sources[0].state,
        airk_core::verification::SourceCheckState::OutOfScope
    );
    let error = kernel.index().unwrap_err();
    assert!(
        format!("{error:#}").contains("excluded_ignore_control"),
        "{error:#}"
    );
    let error = kernel.coverage(true).unwrap_err();
    assert!(
        format!("{error:#}").contains("excluded_ignore_control"),
        "{error:#}"
    );
    assert!(
        airk_ingest::read_selected_source_with_policy(
            &root,
            kernel.data_dir(),
            "public.md",
            Some(&denied)
        )
        .is_err()
    );
    assert_eq!(
        fs::read_to_string(root.join(".airkignore"))?,
        "# Do not read after permanent exclusion.\n"
    );
    Ok(())
}

fn query(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        path_prefix: None,
        limit: 100,
        max_bytes: 65_536,
    }
}

fn facts(document_only: bool) -> FactQuery {
    FactQuery {
        path_prefix: String::new(),
        name: None,
        kind: None,
        language: None,
        exact_name: false,
        limit: 100,
        max_bytes: 65_536,
        path_exact: false,
        document_only,
    }
}

fn edges(symbol: &str, kind: Option<&str>) -> RelationshipQuery {
    RelationshipQuery {
        symbol: symbol.into(),
        direction: "incoming".into(),
        kind: kind.map(str::to_owned),
        path_prefix: String::new(),
        hops: 1,
        limit: 100,
        max_bytes: 65_536,
    }
}

fn canonical(path: &Path) -> Result<String> {
    Ok(path
        .canonicalize()?
        .to_str()
        .context("fixture path must be UTF-8")?
        .into())
}

fn policy(kernel: &Kernel, path: &str, kind: PathScope) -> Result<SourcePolicy> {
    let registration = ProjectRegistration::new(
        canonical(kernel.root())?,
        canonical(kernel.data_dir())?,
        kernel.collection().into(),
    )?;
    let root_id = SourceRootId::from_identity(Incarnation::from_bytes([12; 16]));
    let value = SourcePolicy {
        schema: POLICY_SCHEMA.into(),
        index_id: IndexId::from_identity(Incarnation::from_bytes([11; 16])),
        owner_binding: registration.binding,
        namespace: "collections-1.4".into(),
        collection: kernel.collection().into(),
        epoch: CatalogRevision::INITIAL,
        roots: vec![SourceRoot {
            id: root_id,
            canonical_path: registration.root,
            aliases: vec![],
            owner: true,
        }],
        positive_includes: None,
        denies: vec![DenyRule {
            source_root_id: root_id,
            kind,
            path: path.into(),
            rule_id: DenyId::from_identity(Incarnation::from_bytes([13; 16])),
            reason: DenyReason::Trim,
            created_epoch: CatalogRevision::INITIAL,
            inherited_from: None,
        }],
    };
    value.validate()?;
    Ok(value)
}

#[test]
fn positive_partition_build_coverage_and_verification_use_controls_without_indexing_siblings()
-> Result<()> {
    use airk_core::index_catalog::ScopeAtom;
    for kind in [PathScope::Subtree, PathScope::Exact] {
        let owned = owner()?;
        let source = owned.path().join("source");
        fs::create_dir_all(source.join("docs"))?;
        fs::create_dir_all(source.join("private"))?;
        let original = [
            (".airkignore", "docs/skip.md\n"),
            ("docs/keep.md", "A selected partition requirement.\n"),
            ("docs/skip.md", "Ignored selected subtree document.\n"),
            ("outside.md", "Unselected sibling document.\n"),
            ("private/secret.md", "Denied document remains unchanged.\n"),
        ];
        for (path, bytes) in original {
            fs::write(source.join(path), bytes)?;
        }
        fs::write(owned.path().join(".ignore"), "*.ancestor-skip\n")?;
        fs::write(
            source.join("docs/omitted.ancestor-skip"),
            "Parent controls stay effective.\n",
        )?;
        let kernel = Kernel::open_service_collection(
            &canonical(&source)?,
            Some(owned.path().join("data")),
            "partition",
        )?;
        kernel.configure_vectors(false)?;
        let mut selected = policy(&kernel, "private", PathScope::Subtree)?;
        selected.positive_includes = Some(vec![ScopeAtom {
            source_root_id: selected.roots[0].id,
            kind,
            path: if kind == PathScope::Exact {
                "docs/keep.md"
            } else {
                "docs"
            }
            .into(),
        }]);
        let bound = kernel.with_expected_source_policy(Some(selected.clone()))?;
        assert!(
            bound
                .apply_source_policy(&selected, Incarnation::from_bytes([101; 16]))?
                .no_index
        );
        bound.index()?;
        assert_eq!(
            bound.list_files("", 100, 65536)?.paths,
            vec!["docs/keep.md"]
        );
        let coverage = bound.coverage_with_limits(
            true,
            airk_ingest::inventory::InventoryLimits {
                read_bytes: 128,
                ..Default::default()
            },
        )?;
        assert!(coverage.scan_complete && !coverage.budget_exhausted);
        assert_eq!((coverage.eligible, coverage.current), (1, 1));
        let evidence = serde_json::to_value(
            bound.file_info_conditional("docs/keep.md", &EvidenceReadOptions::default())?,
        )?;
        let verified = bound.verify_returned_sources(
            &evidence,
            airk_core::verification::VerificationBudget {
                mode: airk_core::verification::VerificationMode::Report,
                max_sources: 4,
                max_bytes: 65536,
                deadline_ms: 2000,
            },
        )?;
        assert_eq!(verified.counts.matched_sources, 1);
        assert_eq!(verified.counts.bytes_read, original[1].1.len() as u64);
        let status = bound.source_policy_observation()?.unwrap();
        assert_eq!(
            status["control_dependencies"],
            serde_json::to_value(airk_ingest::permanent_policy::control_dependencies())?
        );
        assert!(bound.file_info("outside.md").is_err());
        assert!(
            airk_ingest::read_selected_source_with_policy(
                &source,
                bound.data_dir(),
                "outside.md",
                Some(&selected)
            )
            .is_err()
        );
        fs::write(source.join("docs/new.md"), "Future subtree source.\n")?;
        bound.index()?;
        assert_eq!(
            bound
                .list_files("", 100, 65536)?
                .paths
                .iter()
                .any(|path| path == "docs/new.md"),
            kind == PathScope::Subtree
        );
        for (path, bytes) in original {
            assert_eq!(fs::read(source.join(path))?, bytes.as_bytes());
        }
        assert_eq!(
            fs::read_to_string(owned.path().join(".ignore"))?,
            "*.ancestor-skip\n"
        );
    }
    Ok(())
}

#[test]
fn positive_partition_declared_header_configuration_is_bounded_and_deny_authoritative() -> Result<()>
{
    use airk_core::index_catalog::ScopeAtom;
    use airk_ingest::adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
    for denied_configuration in [false, true] {
        let owned = owner()?;
        let source = owned.path().join("source");
        fs::create_dir_all(source.join("include"))?;
        let header = "class SelectedHeader { public: int value(); };\n";
        let configuration = r#"[{"file":"include/app.h","arguments":["clang++","-x","c++"]}]"#;
        fs::write(source.join("include/app.h"), header)?;
        fs::write(source.join("compile_commands.json"), configuration)?;
        fs::create_dir(source.join("unselected"))?;
        fs::write(
            source.join("unselected/data.json"),
            "not a declared control\n",
        )?;
        let kernel = Kernel::open_service_collection(
            &canonical(&source)?,
            Some(owned.path().join("data")),
            "headers",
        )?;
        kernel.configure_vectors(false)?;
        kernel.configure_extraction_policy(ExtractionPolicy {
            version: 2,
            detail: None,
            rules: vec![SelectionRule {
                selector: RuleMatch::HeaderContext {
                    prefix: "include".into(),
                    configuration: "compile_commands.json".into(),
                },
                case_sensitive: true,
                adapter_id: "cpp".into(),
            }],
        })?;
        let mut selected = policy(
            &kernel,
            if denied_configuration {
                "compile_commands.json"
            } else {
                "unselected/.ignore"
            },
            PathScope::Exact,
        )?;
        selected.positive_includes = Some(vec![ScopeAtom {
            source_root_id: selected.roots[0].id,
            kind: PathScope::Exact,
            path: "include/app.h".into(),
        }]);
        let bound = kernel.with_expected_source_policy(Some(selected.clone()))?;
        bound.apply_source_policy(&selected, Incarnation::from_bytes([102; 16]))?;
        if denied_configuration {
            let error = bound.index().unwrap_err();
            assert!(
                format!("{error:#}").contains("excluded_configuration_dependency"),
                "{error:#}"
            );
        } else {
            bound.index()?;
            assert_eq!(
                bound.list_files("", 100, 65536)?.paths,
                vec!["include/app.h"]
            );
            assert_eq!(bound.file_info("include/app.h")?.language, "cpp");
            assert!(
                !airk_ingest::permanent_policy::allows_absolute_input_checked(
                    &selected,
                    &source.join("compile_commands.json")
                )?
            );
            assert!(
                !airk_ingest::permanent_policy::allows_directory_input_checked(&selected, &source)?
            );
        }
        assert_eq!(fs::read_to_string(source.join("include/app.h"))?, header);
        assert_eq!(
            fs::read_to_string(source.join("compile_commands.json"))?,
            configuration
        );
        assert_eq!(
            fs::read_to_string(source.join("unselected/data.json"))?,
            "not a declared control\n"
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn positive_partition_redirected_control_refuses_build_coverage_and_verification() -> Result<()> {
    use airk_core::index_catalog::ScopeAtom;
    let (owned, kernel) = coverage_fixture()?;
    let source = owned.path().join("source");
    let mut selected = policy(&kernel, "private", PathScope::Subtree)?;
    selected.positive_includes = Some(vec![ScopeAtom {
        source_root_id: selected.roots[0].id,
        kind: PathScope::Exact,
        path: "public.md".into(),
    }]);
    let bound = kernel.with_expected_source_policy(Some(selected.clone()))?;
    bound.apply_source_policy(&selected, Incarnation::from_bytes([103; 16]))?;
    bound.index()?;
    let evidence = serde_json::to_value(
        bound.file_info_conditional("public.md", &EvidenceReadOptions::default())?,
    )?;
    let original = fs::read(source.join("public.md"))?;
    fs::write(source.join("unselected-rules"), "public.md\n")?;
    std::os::unix::fs::symlink("unselected-rules", source.join(".airkignore"))?;
    for error in [
        bound.index().unwrap_err(),
        bound.coverage(true).unwrap_err(),
    ] {
        assert!(
            format!("{error:#}").contains("aliased_ignore_control"),
            "{error:#}"
        );
    }
    let verified = bound.verify_returned_sources(
        &evidence,
        airk_core::verification::VerificationBudget {
            mode: airk_core::verification::VerificationMode::Report,
            max_sources: 4,
            max_bytes: 65536,
            deadline_ms: 2000,
        },
    )?;
    assert_eq!(verified.counts.bytes_read, 0);
    assert_eq!(
        verified.sources[0].state,
        airk_core::verification::SourceCheckState::Unsupported
    );
    assert_eq!(bound.list_files("", 100, 65536)?.paths, vec!["public.md"]);
    assert_eq!(fs::read(source.join("public.md"))?, original);
    assert_eq!(
        fs::read_to_string(source.join("unselected-rules"))?,
        "public.md\n"
    );
    Ok(())
}

fn assert_no_private(value: &impl Serialize) -> Result<()> {
    let encoded = serde_json::to_string(value)?;
    for forbidden in ["private/", "HiddenOnly", "SecretContract"] {
        ensure!(
            !encoded.contains(forbidden),
            "blocked evidence escaped: {forbidden}"
        );
    }
    Ok(())
}

fn assert_native_denial<T>(result: Result<T>) {
    let error = match result {
        Ok(_) => panic!("blocked exact read unexpectedly succeeded"),
        Err(error) => error,
    };
    assert!(
        error
            .downcast_ref::<airk_core::scoped_reads::ScopeReadRefused>()
            .is_some()
            || error
                .downcast_ref::<airk_kernel::source_policy::SourcePolicyReadUnavailable>()
                .is_some(),
        "blocked read must carry its native scope/policy refusal: {error:#}"
    );
}

#[test]
fn permanent_trim_native_read_families_and_conditional_tokens_exclude_blocked_evidence()
-> Result<()> {
    let fixture = owner()?;
    let root = fixture.path().join("source");
    let data = fixture.path().join("state");
    fs::create_dir_all(root.join("private"))?;
    fs::create_dir(&data)?;
    let original = BTreeMap::from([
        (
            "public.ts",
            "// shared_marker\nclass VisibleOnly { run() { service.ping(); } }\n",
        ),
        (
            "public.md",
            "# Public requirement\nREQ-PUBLIC: The shared_marker shall remain visible.\n",
        ),
        (
            "private/secret.ts",
            "// shared_marker\nclass HiddenOnly { run() { service.ping(); } }\n",
        ),
        (
            "private/secret.md",
            "# SecretContract\nREQ-PRIVATE: The shared_marker shall remain private.\n",
        ),
    ]);
    for (path, text) in &original {
        fs::write(root.join(path), text)?;
    }
    let kernel = Kernel::open(&root, Some(data.clone()))?;
    kernel.configure_vectors(false)?;
    kernel.configure_index_detail(IndexDetail::Detailed)?;
    kernel.index()?;
    let old_token = kernel
        .status()?
        .publication
        .context("indexed publication")?
        .token;
    let remembered = kernel.read_file_conditional(
        "private/secret.ts",
        1,
        2,
        8192,
        &EvidenceReadOptions::default(),
    )?;
    let secret_validator = remembered
        .read
        .validator
        .context("secret representation validator")?;
    let secret_hash = kernel
        .file_info("private/secret.ts")?
        .content_hash
        .parse()?;
    assert_eq!(kernel.list_files("", 100, 65_536)?.paths.len(), 4);
    let before_graph = kernel.relationships(&edges("service.ping", Some("call")))?;
    assert_eq!(before_graph.relationships.len(), 2);
    assert!(
        before_graph
            .relationships
            .iter()
            .any(|edge| edge.citation.path.starts_with("private/"))
    );
    let secret_facts = kernel.outline("private/secret.ts", 100, 65_536)?;
    let public_facts = kernel.outline("public.ts", 100, 65_536)?;
    let public_owner = public_facts
        .facts
        .iter()
        .find(|fact| fact.kind == "definition" && fact.name == "VisibleOnly")
        .and_then(|fact| fact.code.as_ref())
        .context("public definition must retain its native endpoint")?
        .symbol_id
        .clone();
    let mut public_edges = edges(&public_owner, Some("contains"));
    public_edges.direction = "outgoing".into();
    assert_eq!(kernel.relationships(&public_edges)?.relationships.len(), 1);
    let remembered_key = secret_facts
        .facts
        .iter()
        .find_map(|fact| fact.continuity_key);
    assert!(
        remembered_key.is_some(),
        "fixture must expose a durable fact handle"
    );

    let deny = policy(&kernel, "private", PathScope::Subtree)?;
    let bound = kernel.with_expected_source_policy(Some(deny.clone()))?;
    let report = bound.apply_source_policy(&deny, Incarnation::from_bytes([14; 16]))?;
    report.validate()?;
    assert_eq!(report.removed_artifacts, 2);
    assert!(!report.no_index);
    assert_ne!(report.removal_publication, Some(old_token));
    assert_eq!(
        bound.list_files("", 100, 65_536)?.paths,
        ["public.md", "public.ts"]
    );

    let mut one = query("shared_marker");
    one.limit = 1;
    let selected = bound.search(&one)?;
    assert_eq!(
        selected.hits.len(),
        1,
        "deny filters must not consume the allowed top-k budget"
    );
    assert_no_private(&selected)?;
    let search = bound.search(&query("shared_marker"))?;
    assert!(!search.hits.is_empty());
    assert_no_private(&search)?;
    assert_no_private(&bound.facts(&facts(false))?)?;
    let documents = bound.facts(&facts(true))?;
    assert!(!documents.facts.is_empty());
    assert!(
        documents
            .facts
            .iter()
            .all(|fact| fact.citation.path == "public.md")
    );
    let code = bound.code_facts(&facts(false))?;
    assert!(!code.facts.is_empty());
    assert!(
        code.facts
            .iter()
            .all(|fact| fact.citation.path == "public.ts")
    );
    assert_no_private(&code)?;
    let references = bound.references("shared_marker", "", 100, 65_536)?;
    assert!(!references.evidence.hits.is_empty());
    assert_no_private(&references)?;
    let graph = bound.relationships(&edges("service.ping", Some("call")))?;
    assert!(
        graph.relationships.is_empty(),
        "unresolved endpoints require location proof under a restricted policy"
    );
    assert_no_private(&graph)?;
    let graph = bound.relationships(&public_edges)?;
    assert_eq!(
        graph.relationships.len(),
        1,
        "proven allowed endpoints must survive trim"
    );
    assert_eq!(graph.relationships[0].citation.path, "public.ts");
    assert_no_private(&graph)?;
    assert!(
        bound
            .continuity_facts(remembered_key.unwrap(), 100, 65_536)?
            .facts
            .is_empty()
    );
    for path in ["private/secret.ts", "private/secret.md"] {
        assert_native_denial(bound.file_info(path));
        assert_native_denial(bound.read_file(path, 1, 2, 8192));
        assert_native_denial(bound.context(path, 1, 0, 1, 8192));
        assert_native_denial(bound.outline(path, 100, 65_536));
        assert_native_denial(bound.source_location(path));
    }
    let remembered_options = EvidenceReadOptions {
        if_evidence: Some(secret_validator),
        known_hash: Some(secret_hash),
        follow_moves: true,
        ..Default::default()
    };
    assert_native_denial(bound.read_file_conditional(
        "private/secret.ts",
        1,
        2,
        8192,
        &remembered_options,
    ));
    assert_native_denial(bound.file_info_conditional("private/secret.ts", &remembered_options));
    assert_native_denial(bound.outline_conditional(
        "private/secret.ts",
        100,
        65_536,
        &remembered_options,
    ));
    assert_native_denial(bound.changes(&ChangesRequest {
        selectors: vec![RememberedSelector::Path {
            path: "private/secret.ts".into(),
        }],
        ..Default::default()
    }));
    let changes = bound.changes(&ChangesRequest {
        since: Some(old_token),
        include_symbols: true,
        include_documents: true,
        limit: 100,
        max_bytes: 65_536,
        ..Default::default()
    })?;
    assert_no_private(&changes)?;
    assert!(
        bound
            .read_file_conditional(
                "public.ts",
                1,
                2,
                8192,
                &EvidenceReadOptions {
                    revision: Some(old_token),
                    ..Default::default()
                }
            )
            .is_err(),
        "superseded publications cannot be reused after trim"
    );
    assert!(!bound.read_file("public.ts", 1, 2, 8192)?.hits.is_empty());
    assert_eq!(bound.source_location("public.ts")?.root, canonical(&root)?);

    let full_status = serde_json::to_value(bound.status()?)?;
    let summary = bound.status_summary()?;
    assert_eq!(full_status["source_policy"], summary["source_policy"]);
    assert_eq!(
        summary["source_policy"]["application"],
        serde_json::to_value(bound.source_policy_status()?)?
    );
    assert_eq!(
        summary["source_policy"]["policy_digest"],
        serde_json::to_value(deny.identity()?)?
    );
    for (path, text) in original {
        assert_eq!(
            fs::read(root.join(path))?,
            text.as_bytes(),
            "trim wrote source bytes: {path}"
        );
    }
    Ok(())
}

fn imported_cross_file_fixture(root: &Path) -> Result<Vec<u8>> {
    use scip::types::*;
    let visible = "scip npm trim-fixture 1 visible().";
    let hidden = "scip npm trim-fixture 1 HiddenOnly#";
    let mut index = Index::new();
    index.metadata = MessageField::some(Metadata {
        project_root: url::Url::from_directory_path(root)
            .map_err(|_| anyhow::anyhow!("fixture root URI"))?
            .into(),
        text_document_encoding: TextEncoding::UTF8.into(),
        tool_info: MessageField::some(ToolInfo {
            name: "policy-fixture-indexer".into(),
            version: "1".into(),
            ..Default::default()
        }),
        ..Default::default()
    });
    let mut symbol = SymbolInformation {
        symbol: visible.into(),
        display_name: "visible".into(),
        ..Default::default()
    };
    symbol.relationships.push(Relationship {
        symbol: hidden.into(),
        is_reference: true,
        ..Default::default()
    });
    index.documents.push(Document {
        relative_path: "public.ts".into(),
        language: "TypeScript".into(),
        position_encoding: PositionEncoding::UTF16CodeUnitOffsetFromLineStart.into(),
        occurrences: vec![Occurrence {
            symbol: visible.into(),
            symbol_roles: 1,
            range: vec![0, 9, 16],
            ..Default::default()
        }],
        symbols: vec![symbol],
        ..Default::default()
    });
    index.documents.push(Document {
        relative_path: "private/secret.ts".into(),
        language: "TypeScript".into(),
        position_encoding: PositionEncoding::UTF16CodeUnitOffsetFromLineStart.into(),
        occurrences: vec![Occurrence {
            symbol: hidden.into(),
            symbol_roles: 1,
            range: vec![0, 6, 16],
            ..Default::default()
        }],
        symbols: vec![SymbolInformation {
            symbol: hidden.into(),
            display_name: "HiddenOnly".into(),
            ..Default::default()
        }],
        ..Default::default()
    });
    Ok(index.write_to_bytes()?)
}

#[test]
fn permanent_trim_retires_imported_cross_file_endpoints_and_rejects_old_assertion_reimport()
-> Result<()> {
    let fixture = owner()?;
    let root = fixture.path().join("source");
    let data = fixture.path().join("state");
    fs::create_dir_all(root.join("private"))?;
    fs::create_dir(&data)?;
    fs::write(
        root.join("public.ts"),
        "function visible() {}\nvisible();\n",
    )?;
    fs::write(root.join("private/secret.ts"), "class HiddenOnly {}\n")?;
    fs::write(root.join("flags.bin"), [0, 255])?;
    let kernel = Kernel::open(&root, Some(data.clone()))?;
    kernel.configure_vectors(false)?;
    kernel.configure_index_detail(IndexDetail::Detailed)?;
    kernel.index()?;
    let bytes = imported_cross_file_fixture(&root.canonicalize()?)?;
    let mut manifest =
        kernel.prepare_analysis("policy-fixture-indexer", "1", &["flags.bin".into()])?;
    manifest.index_sha256 = Some(airk_ingest::semantic::digest(&bytes));
    let index_path = fixture.path().join("original.scip");
    let manifest_path = fixture.path().join("original-manifest.json");
    fs::write(&index_path, &bytes)?;
    fs::write(&manifest_path, serde_json::to_vec(&manifest)?)?;
    let imported = kernel.import_scip(&index_path, &manifest_path)?;
    assert!(imported.imported_facts >= 2);
    let target = "scip:policy-fixture-indexer:scip npm trim-fixture 1 HiddenOnly#";
    let cached = kernel.relationships(&edges(target, Some("reference")))?;
    assert_eq!(cached.relationships.len(), 1);
    assert_eq!(
        cached.relationships[0].citation.path, "public.ts",
        "fixture must exercise an allowed source pointing to a blocked endpoint"
    );
    assert_eq!(
        kernel
            .relationships(&edges(target, Some("reference")))?
            .relationships
            .len(),
        1
    );
    let deny = policy(&kernel, "private", PathScope::Subtree)?;
    let bound = kernel.with_expected_source_policy(Some(deny.clone()))?;
    let report = bound.apply_source_policy(&deny, Incarnation::from_bytes([21; 16]))?;
    assert!(report.retired_components.contains(&"analysis".into()));
    assert_eq!(bound.status()?.imported_facts, 0);
    assert_eq!(bound.status()?.imported_relationships, 0);
    assert!(
        bound
            .relationships(&edges(target, Some("reference")))?
            .relationships
            .is_empty()
    );
    assert_no_private(&bound.code_facts(&facts(false))?)?;
    let publication = bound.status()?.publication.context("trimmed publication")?;
    assert!(
        bound.import_scip(&index_path, &manifest_path).is_err(),
        "old producer assertions must not be rebound to trimmed sources"
    );
    assert_eq!(
        bound
            .status()?
            .publication
            .context("unchanged publication")?,
        publication
    );
    let store = FileStore::new(data.join("collections-1.2/default"));
    let scope = store
        .current_scope(&canonical(&root)?, "default")?
        .context("current store scope")?;
    let direct = store.current_read(&scope)?;
    assert!(direct.artifact("private/secret.ts")?.is_none());
    assert!(direct.artifact("public.ts")?.is_some());
    assert!(direct.resource_reference("analysis")?.is_none());
    assert!(direct.analysis(&store)?.is_none());
    assert_eq!(
        fs::read(root.join("private/secret.ts"))?,
        b"class HiddenOnly {}\n"
    );
    assert_eq!(fs::read(&index_path)?, bytes);
    Ok(())
}

#[test]
fn permanent_trim_canonical_source_survives_alias_changes_rebuild_and_reopen() -> Result<()> {
    let fixture = owner()?;
    let root = fixture.path().join("source");
    let external = fixture.path().join("external");
    let data = fixture.path().join("state");
    fs::create_dir(&root)?;
    fs::create_dir_all(external.join("private"))?;
    fs::create_dir(&data)?;
    fs::write(root.join("public.md"), "Primary public marker.\n")?;
    fs::write(external.join("public.md"), "External public marker.\n")?;
    fs::write(
        external.join("private/secret.md"),
        "HiddenOnly private external marker.\n",
    )?;
    let kernel = Kernel::open(&root, Some(data.clone()))?;
    kernel.configure_vectors(false)?;
    kernel.add_root("docs", &external)?;
    kernel.index()?;
    assert!(
        !kernel
            .read_file("@docs/private/secret.md", 1, 1, 8192)?
            .hits
            .is_empty()
    );
    let mut deny = policy(&kernel, "private", PathScope::Subtree)?;
    let external_id = SourceRootId::from_identity(Incarnation::from_bytes([32; 16]));
    deny.roots.push(SourceRoot {
        id: external_id,
        canonical_path: canonical(&external)?,
        aliases: vec!["docs".into()],
        owner: false,
    });
    deny.denies[0].source_root_id = external_id;
    deny.validate()?;
    let bound = kernel.with_expected_source_policy(Some(deny.clone()))?;
    bound.apply_source_policy(&deny, Incarnation::from_bytes([33; 16]))?;
    assert_native_denial(bound.read_file("@docs/private/secret.md", 1, 1, 8192));
    bound.remove_root("docs")?;
    bound.add_root("renamed", &external)?;
    bound.reindex()?;
    assert_eq!(
        bound.list_files("", 100, 65_536)?.paths,
        ["@renamed/public.md", "public.md"]
    );
    assert_native_denial(bound.read_file("@renamed/private/secret.md", 1, 1, 8192));
    let original_secret = fs::read(external.join("private/secret.md"))?;
    fs::write(
        external.join("private/new.md"),
        "HiddenOnly newly created blocked descendant.\n",
    )?;
    bound.index()?;
    assert!(bound.search(&query("HiddenOnly"))?.hits.is_empty());
    drop(bound);
    drop(kernel);
    let reopened = Kernel::open(&root, Some(data))?.with_expected_source_policy(Some(deny))?;
    reopened.reindex()?;
    assert!(reopened.search(&query("HiddenOnly"))?.hits.is_empty());
    assert!(
        !reopened
            .read_file("@renamed/public.md", 1, 1, 8192)?
            .hits
            .is_empty()
    );
    assert_eq!(
        fs::read(external.join("private/secret.md"))?,
        original_secret
    );
    assert_eq!(
        fs::read(external.join("private/new.md"))?,
        b"HiddenOnly newly created blocked descendant.\n"
    );
    Ok(())
}

#[test]
fn permanent_trim_policy_binding_is_index_local_and_refuses_foreign_roots() -> Result<()> {
    let fixture = owner()?;
    let root = fixture.path().join("source");
    let first_data = fixture.path().join("first-state");
    let second_data = fixture.path().join("second-state");
    let other_root = fixture.path().join("other-source");
    for directory in [&root, &first_data, &second_data, &other_root] {
        fs::create_dir(directory)?;
    }
    fs::write(
        root.join("secret.md"),
        "HiddenOnly source shared by independent indexes.\n",
    )?;
    fs::write(root.join("public.md"), "Allowed public source.\n")?;
    let first = Kernel::open(&root, Some(first_data))?;
    let second = Kernel::open(&root, Some(second_data))?;
    for kernel in [&first, &second] {
        kernel.configure_vectors(false)?;
        kernel.index()?;
    }
    let deny = policy(&first, "secret.md", PathScope::Exact)?;
    assert!(
        second
            .with_expected_source_policy(Some(deny.clone()))
            .is_err(),
        "a deny owned by one backing index cannot bind another"
    );
    let mut wrong_root = deny.clone();
    wrong_root.roots[0].canonical_path = canonical(&other_root)?;
    assert!(first.with_expected_source_policy(Some(wrong_root)).is_err());
    let mut wrong_collection = deny.clone();
    wrong_collection.collection = "foreign".into();
    assert!(
        first
            .with_expected_source_policy(Some(wrong_collection))
            .is_err()
    );
    let bound = first.with_expected_source_policy(Some(deny.clone()))?;
    bound.apply_source_policy(&deny, Incarnation::from_bytes([41; 16]))?;
    assert_native_denial(bound.read_file("secret.md", 1, 1, 8192));
    assert!(bound.search(&query("HiddenOnly"))?.hits.is_empty());
    assert_eq!(
        second.search(&query("HiddenOnly"))?.hits.len(),
        1,
        "a permanent deny is index-owned, not global source deletion"
    );
    assert!(!second.read_file("secret.md", 1, 1, 8192)?.hits.is_empty());
    assert_eq!(
        fs::read(root.join("secret.md"))?,
        b"HiddenOnly source shared by independent indexes.\n"
    );
    Ok(())
}

#[test]
fn service_policy_cleanup_releases_only_its_own_pin_and_preserves_actual_error_reports()
-> Result<()> {
    let owned = owner()?;
    let base = owned.path().canonicalize()?;
    let root = base.join("source");
    let data = base.join("data");
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("private.md"),
        b"private source remains unchanged\n",
    )?;
    fs::write(root.join("public.md"), b"healthy public evidence\n")?;
    let kernel = Kernel::open_service_collection(&root, Some(data.clone()), "default")?;
    kernel.configure_vectors(false)?;
    kernel.index_service_sources()?;
    let collection = airk_store::service::collection::Collection::existing(
        &data,
        &canonical(&root)?,
        "default",
    )?;
    let external = collection.select()?;
    let objects = external.store.directory().join("objects/sources");
    let orphan = objects.join("0".repeat(64));
    fs::write(&orphan, b"owned abandoned capture")?;
    let policy = policy(&kernel, "private.md", PathScope::Exact)?;
    let operation = Incarnation::from_bytes([103; 16]);
    let pending = kernel.apply_source_policy(&policy, operation)?;
    assert!(pending.physical_cleanup_pending);
    assert!(
        pending.cleanup_error.is_none(),
        "a genuine external pin defers cleanup without an I/O error"
    );
    assert!(orphan.exists());
    drop(external);
    // An actual malformed owned-object entry must be reported, rather than
    // being hidden forever behind the cleanup call's own selection lease.
    let malformed = objects.join("nonregular-derived-object");
    fs::create_dir(&malformed)?;
    let failed = kernel.apply_source_policy(&policy, operation)?;
    assert!(failed.physical_cleanup_pending);
    assert!(
        failed
            .cleanup_error
            .as_deref()
            .is_some_and(|error| error.contains("nonregular current object"))
    );
    assert_eq!(kernel.source_policy_status()?, Some(failed));
    assert!(malformed.is_dir());
    fs::remove_dir(malformed)?;
    let complete = kernel.apply_source_policy(&policy, operation)?;
    assert!(!complete.physical_cleanup_pending);
    assert!(complete.cleanup_error.is_none());
    assert_eq!(kernel.source_policy_status()?, Some(complete));
    assert!(!orphan.exists());
    assert_eq!(
        fs::read(root.join("private.md"))?,
        b"private source remains unchanged\n"
    );
    assert!(
        kernel.read_file("public.md", 1, 1, 4096)?.hits[0]
            .text
            .contains("healthy public evidence")
    );
    Ok(())
}
