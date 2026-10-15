// SPDX-License-Identifier: Apache-2.0
//! Original build/configuration fixtures; activate only with the P6 adapters.
use airk_core::{EvidenceBasis, FactQuery};
use airk_kernel::{Kernel, extraction::ExtractionPolicy};
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

struct Fixture {
    path: &'static str,
    moved: &'static str,
    adapter: &'static str,
    source: &'static str,
    expected: &'static [(&'static str, &'static str)],
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        path: "CMakeLists.txt",
        moved: "moved.cmake",
        adapter: "cmake",
        source: "# original beacon build\nadd_library(beacon STATIC src/beacon.cc)\ntarget_link_libraries(beacon PRIVATE ${DEPENDENCIES})\ninclude(local.cmake)\n",
        expected: &[
            ("build_target", "beacon"),
            ("dependency_argument", "${DEPENDENCIES}"),
            ("include_argument", "local.cmake"),
        ],
    },
    Fixture {
        path: "Makefile",
        moved: "moved.mk",
        adapter: "make",
        source: "SOURCES := beacon.c\nbeacon: $(SOURCES) | output\n\t@echo 'written recipe only'\ninclude local.mk\n",
        expected: &[
            ("build_target", "beacon"),
            ("order_only_prerequisite", "output"),
            ("include_argument", "local.mk"),
        ],
    },
    Fixture {
        path: "beacon.csproj",
        moved: "moved.csproj",
        adapter: "msbuild",
        source: "<Project>\n<PropertyGroup Condition=\"'$(Flavor)' == 'small'\"><TargetFramework>net8.0</TargetFramework></PropertyGroup>\n<ItemGroup><ProjectReference Include=\"other.csproj\"/><PackageReference Include=\"Fiction.Core\" Version=\"1.0\"/></ItemGroup>\n<Import Project=\"$(Root)/local.props\"/></Project>\n",
        expected: &[
            ("target_framework", "net8.0"),
            ("project_reference", "other.csproj"),
            ("package_reference", "Fiction.Core"),
            ("import_argument", "$(Root)/local.props"),
        ],
    },
    Fixture {
        path: ".github/workflows/beacon.yml",
        moved: ".github/workflows/moved.yml",
        adapter: "github-actions",
        source: "on: [push]\npermissions: {contents: read}\njobs:\n  build:\n    steps:\n      - uses: example/action@v1\n      - run: echo 'unexecuted'\n  verify:\n    needs: build\n    uses: ./local.yml\n",
        expected: &[
            ("workflow_trigger", "push"),
            ("workflow_job", "build"),
            ("workflow_needs", "build"),
            ("action_reference", "example/action@v1"),
        ],
    },
    Fixture {
        path: "compose.yaml",
        moved: "nested/compose.yaml",
        adapter: "compose",
        source: "services:\n  beacon:\n    image: example/beacon:${TAG}\n    build: ./beacon\n    depends_on: [database]\n    volumes: [cache:/cache]\n  database:\n    image: example/database:v1\nvolumes: {cache: {}}\n",
        expected: &[
            ("compose_service", "beacon"),
            ("compose_image", "example/beacon:${TAG}"),
            ("build_context", "./beacon"),
            ("compose_dependency", "database"),
        ],
    },
];

fn query(path: &str, limit: usize, max_bytes: usize) -> FactQuery {
    FactQuery {
        path_prefix: path.into(),
        name: None,
        kind: None,
        language: None,
        exact_name: false,
        limit,
        max_bytes,
        path_exact: true,
        document_only: false,
    }
}

#[test]
fn actions_selection_uses_each_source_root_and_logical_override_precedence() -> Result<()> {
    let root = tempfile::tempdir()?;
    let external = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let source = "on: [push]\njobs:\n  original:\n    steps:\n      - run: echo unchanged\n";
    for relative in [
        ".github/workflows/check.yml",
        "nested/.github/workflows/check.yml",
    ] {
        let path = external.path().join(relative);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, source)?;
    }
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.add_root("vendor", external.path())?;
    kernel.index()?;
    let store = FileStore::new(state.path().join("collections/default"));
    let direct = "@vendor/.github/workflows/check.yml";
    let nested = "@vendor/nested/.github/workflows/check.yml";
    let catalog = store.catalog()?.context("additional root catalog")?;
    let files = &catalog.inventory.as_ref().unwrap().files;
    assert_eq!(files[direct].language, "github-actions");
    assert_eq!(files[nested].language, "yaml");
    assert!(
        kernel
            .facts(&query(direct, 100, 65536))?
            .facts
            .iter()
            .any(|f| f.kind == "workflow_job" && f.name == "original" && f.citation.path == direct)
    );
    assert!(
        !kernel
            .facts(&query(nested, 100, 65536))?
            .facts
            .iter()
            .any(|f| f.kind == "workflow_job")
    );
    let policy: ExtractionPolicy = serde_json::from_value(
        serde_json::json!({"version":1,"rules":[
            {"selector":{"kind":"exact","path":direct},"case_sensitive":true,"adapter_id":"yaml"},
            {"selector":{"kind":"exact","path":nested},"case_sensitive":true,"adapter_id":"github-actions"}
        ]}),
    )?;
    kernel.configure_extraction_policy(policy)?;
    kernel.index()?;
    let overridden = store.catalog()?.context("overridden catalog")?;
    let files = &overridden.inventory.as_ref().unwrap().files;
    assert_eq!(files[direct].language, "yaml");
    assert_eq!(files[nested].language, "github-actions");
    assert!(
        kernel
            .facts(&query(nested, 100, 65536))?
            .facts
            .iter()
            .any(|f| f.kind == "workflow_job" && f.citation.path == nested)
    );
    assert_eq!(
        fs::read_to_string(external.path().join(".github/workflows/check.yml"))?,
        source
    );
    Ok(())
}

#[test]
fn build_profiles_publication_reuse_move_policy_locations_and_budgets() -> Result<()> {
    for fixture in FIXTURES {
        let root = tempfile::tempdir()?;
        let state = tempfile::tempdir()?;
        let original = root.path().join(fixture.path);
        fs::create_dir_all(original.parent().unwrap())?;
        fs::write(&original, fixture.source)?;
        let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
        let first = kernel.index()?;
        assert_eq!(first.indexed, 1, "{}", fixture.adapter);
        let store = FileStore::new(state.path().join("collections/default"));
        let initial = store.catalog()?.context("initial catalog")?;
        let initial_file = initial.inventory.as_ref().unwrap().files[fixture.path].clone();
        assert_eq!(initial_file.language, fixture.adapter);
        assert!(initial_file.chunks > 0);
        assert!(
            initial_file.continuity.is_empty(),
            "unqualified continuity must not be invented"
        );
        let descriptor = kernel
            .extraction_policy()?
            .adapters
            .into_iter()
            .find(|d| d.profile.adapter_id == fixture.adapter)
            .context("adapter descriptor")?;
        assert!(!descriptor.capabilities.continuity);
        let facts = kernel.facts(&query(fixture.path, 100, 65536))?;
        for (kind, name) in fixture.expected {
            assert!(
                facts
                    .facts
                    .iter()
                    .any(|f| f.kind == *kind && f.name == *name),
                "{} missing {kind}/{name}",
                fixture.adapter
            );
        }
        for fact in &facts.facts {
            assert!(fact.continuity_key.is_none());
            assert_eq!(fact.citation.path, fixture.path);
            let code = fact.code.as_ref().context("exact structural evidence")?;
            assert_eq!(code.basis, EvidenceBasis::StructuralRule);
            let span = code.span.as_ref().context("original span")?;
            assert!(fixture.source.get(span.start_byte..span.end_byte).is_some());
            assert_eq!(
                fact.citation.start_line,
                fixture.source[..span.start_byte]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count()
                    + 1
            );
        }
        let bounded = kernel.facts(&query(fixture.path, 1, 65536))?;
        assert_eq!(bounded.facts.len(), 1);
        assert!(bounded.truncated);
        let tiny = kernel.facts(&query(fixture.path, 100, 1))?;
        assert!(tiny.facts.is_empty() && tiny.truncated);
        assert_eq!(kernel.index()?.generation, first.generation, "warm no-op");

        fs::write(
            root.path().join("unrelated.txt"),
            "An independent original fixture about quiet observatories.\n",
        )?;
        kernel.index()?;
        let unrelated = store.catalog()?.unwrap();
        assert_eq!(
            unrelated.inventory.as_ref().unwrap().files[fixture.path].state,
            initial_file.state
        );
        let moved = root.path().join(fixture.moved);
        fs::create_dir_all(moved.parent().unwrap())?;
        fs::rename(&original, &moved)?;
        let move_report = kernel.index()?;
        assert_eq!(move_report.indexed, 0, "compatible move avoids extraction");
        let moved_catalog = store.catalog()?.unwrap();
        let moved_file = &moved_catalog.inventory.as_ref().unwrap().files[fixture.moved];
        assert_eq!(
            moved_file.state.payload_digest,
            initial_file.state.payload_digest
        );
        assert_eq!(moved_file.state.artifact_id, initial_file.state.artifact_id);
        let rebound = kernel.facts(&query(fixture.moved, 100, 65536))?;
        assert_eq!(rebound.facts.len(), facts.facts.len());
        assert!(
            rebound
                .facts
                .iter()
                .all(|f| f.citation.path == fixture.moved)
        );

        let policy: ExtractionPolicy = serde_json::from_value(
            serde_json::json!({"version":1,"rules":[{"selector":{"kind":"exact","path":fixture.moved},"case_sensitive":true,"adapter_id":"text"}]}),
        )?;
        let configured = kernel.configure_extraction_policy(policy)?;
        assert!(configured.pending_reindex);
        assert_eq!(
            kernel.facts(&query(fixture.moved, 100, 65536))?.facts.len(),
            facts.facts.len(),
            "configuration has no effect before publication"
        );
        kernel.index()?;
        assert!(!kernel.extraction_policy()?.pending_reindex);
        let plain = kernel.facts(&query(fixture.moved, 100, 65536))?;
        assert!(
            plain.facts.iter().all(|fact| fact
                .code
                .as_ref()
                .is_none_or(|code| !code.producer.starts_with("airk-build-structure/"))),
            "text profile must not retain native build evidence"
        );
        for (kind, _) in fixture.expected {
            assert!(!plain.facts.iter().any(|fact| fact.kind == *kind));
        }
        let plain_catalog = store.catalog()?.context("text profile catalog")?;
        let plain_file = &plain_catalog.inventory.as_ref().unwrap().files[fixture.moved];
        assert_eq!(plain_file.language, "text");
        assert_ne!(plain_file.state.profile_id, moved_file.state.profile_id);
        assert_ne!(
            plain_file.state.payload_digest,
            moved_file.state.payload_digest
        );
        assert_eq!(plain_file.state.artifact_id, moved_file.state.artifact_id);
        assert_eq!(plain_file.state.source_hash, moved_file.state.source_hash);
        assert_eq!(
            fs::read_to_string(&moved)?,
            fixture.source,
            "source remains unchanged"
        );
    }
    Ok(())
}
