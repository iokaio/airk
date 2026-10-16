// SPDX-License-Identifier: Apache-2.0
//! Activate after P7 registry admission; source fixtures are independent originals.
use airk_core::FactQuery;
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::Result;
use std::{fs, path::PathBuf};

#[test]
fn project_schemas_public_reads_reuse_moves_and_profile_invalidation() -> Result<()> {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../airk-ingest/tests/fixtures/project-schemas");
    for (source, path, adapter, kind) in [
        (
            "resources.yaml",
            "resources.yaml",
            "kubernetes",
            "resource_name",
        ),
        ("openapi.yaml", "api.yaml", "openapi", "api_operation"),
        ("gitlab.yml", ".gitlab-ci.yml", "gitlab-ci", "pipeline_job"),
        (
            "package.json",
            "package.json",
            "project-manifest",
            "package_dependency",
        ),
        (
            "tsconfig.json",
            "tsconfig.json",
            "project-manifest",
            "compiler_option",
        ),
        (
            "composer.json",
            "composer.json",
            "project-manifest",
            "package_dependency",
        ),
        (
            "Cargo.toml",
            "Cargo.toml",
            "project-manifest",
            "package_dependency",
        ),
        (
            "pyproject.toml",
            "pyproject.toml",
            "project-manifest",
            "build_system",
        ),
    ] {
        let root = tempfile::tempdir()?;
        let state = tempfile::tempdir()?;
        let original = fs::read(fixtures.join(source))?;
        fs::write(root.path().join(path), &original)?;
        let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
        let first = kernel.index()?;
        let store = FileStore::new(state.path().join("collections/default"));
        let initial = store.catalog()?.unwrap();
        let initial_file = initial.inventory.as_ref().unwrap().files[path].clone();
        assert_eq!(initial_file.language, adapter);
        let query = |path: &str, limit, max_bytes| FactQuery {
            path_prefix: path.into(),
            name: None,
            kind: None,
            language: None,
            exact_name: false,
            limit,
            max_bytes,
            path_exact: true,
            document_only: false,
        };
        let result = kernel.facts(&query(path, 100, 65536))?;
        assert!(result.facts.iter().any(|f| f.kind == kind), "{path}");
        assert!(
            result.facts.iter().all(|f| f.citation.path == path
                && f.code.as_ref().and_then(|c| c.span.as_ref()).is_some())
        );
        assert_eq!(kernel.index()?.generation, first.generation);
        let tiny = kernel.facts(&query(path, 1, 1))?;
        assert!(tiny.truncated && tiny.facts.is_empty());
        fs::create_dir(root.path().join("moved"))?;
        let moved = format!("moved/{path}");
        fs::rename(root.path().join(path), root.path().join(&moved))?;
        assert_eq!(kernel.index()?.indexed, 0);
        let current = store.catalog()?.unwrap();
        let file = &current.inventory.as_ref().unwrap().files[&moved];
        assert_eq!(file.state.payload_digest, initial_file.state.payload_digest);
        assert_eq!(
            file.state.artifact_id, initial_file.state.artifact_id,
            "{path}"
        );
        let policy = serde_json::from_value(
            serde_json::json!({"version":1,"rules":[{"selector":{"kind":"exact","path":moved},"case_sensitive":true,"adapter_id":"text"}]}),
        )?;
        kernel.configure_extraction_policy(policy)?;
        kernel.index()?;
        let plain = store.catalog()?.unwrap();
        assert_eq!(
            plain.inventory.as_ref().unwrap().files[&moved].language,
            "text"
        );
        assert!(
            !kernel
                .facts(&query(&moved, 100, 65536))?
                .facts
                .iter()
                .any(|f| f.kind == kind)
        );
        assert_eq!(fs::read(root.path().join(&moved))?, original);
    }
    Ok(())
}
