// SPDX-License-Identifier: Apache-2.0
//! P9 original integration qualification for admitted general language profiles.
use airk_core::{EvidenceBasis, FactQuery};
use airk_ingest::adapters::profile::{ExtractionPolicy, RuleMatch, SelectionRule};
use airk_kernel::Kernel;
use airk_store::FileStore;
use anyhow::{Context, Result};
use std::fs;

fn qualify(path: &str, moved: &str, source: &str, name: &str) -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let source = source.replace("\r\n", "\n").replace('\n', "\r\n");
    fs::write(root.path().join(path), &source)?;
    fs::write(
        root.path().join("unrelated.rs"),
        "pub fn unaffected() -> u32 { 17 }\n",
    )?;
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    assert_eq!(kernel.index()?.indexed, 2);
    let store = FileStore::new(data.path().join("collections/default"));
    let catalog = store.catalog()?.context("catalog")?;
    assert_eq!(
        store.read_retained_source(&catalog.snapshot.artifacts[path])?,
        source.as_bytes()
    );
    let before = catalog.inventory.as_ref().context("inventory")?.files[path]
        .state
        .clone();
    let untouched = catalog.inventory.as_ref().unwrap().files["unrelated.rs"]
        .state
        .clone();
    let outline = kernel.outline(path, 100, 65_536)?;
    assert!(outline.facts.iter().any(|f| f.name.contains(name)));
    for fact in &outline.facts {
        let span = fact
            .code
            .as_ref()
            .context("native code")?
            .span
            .as_ref()
            .context("span")?;
        assert!(source.get(span.start_byte..span.end_byte).is_some());
        assert_eq!(
            fact.citation.start_line,
            source[..span.start_byte]
                .bytes()
                .filter(|b| *b == b'\n')
                .count()
                + 1
        );
        if fact.code.as_ref().unwrap().basis == EvidenceBasis::StructuralRule {
            assert!(
                fact.continuity_key.is_none(),
                "literal Elixir macro interpretation has no native declaration key"
            );
        }
        if let Some(key) = fact.continuity_key {
            let keyed = kernel.continuity_facts(key, 1, 4096)?;
            assert_eq!(keyed.facts.len(), 1);
            assert_eq!(keyed.facts[0].continuity_key, Some(key));
        }
    }
    let keys: std::collections::BTreeSet<_> = outline
        .facts
        .iter()
        .filter_map(|fact| fact.continuity_key)
        .collect();
    if !path.ends_with(".ex") {
        assert!(!keys.is_empty(), "native declarations need queryable keys");
    }
    let bounded = kernel.facts(&FactQuery {
        path_prefix: path.into(),
        name: None,
        kind: None,
        language: None,
        exact_name: false,
        limit: 1,
        max_bytes: 4096,
        path_exact: true,
        document_only: false,
    })?;
    assert!(bounded.facts.len() <= 1);
    assert!(bounded.truncated);
    let warm = kernel.index()?;
    assert_eq!((warm.indexed, warm.reused), (0, 2));
    fs::rename(root.path().join(path), root.path().join(moved))?;
    let renamed = kernel.index()?;
    assert_eq!((renamed.indexed, renamed.reused), (0, 2));
    let after = store.catalog()?.context("renamed")?;
    assert_eq!(
        after.inventory.as_ref().unwrap().files[moved]
            .state
            .payload_digest,
        before.payload_digest
    );
    assert!(
        kernel
            .outline(moved, 100, 65_536)?
            .facts
            .iter()
            .all(|f| f.citation.path == moved)
    );
    if !path.ends_with(".ex") && !path.ends_with(".mm") {
        let moved_keys: std::collections::BTreeSet<_> = kernel
            .outline(moved, 100, 65_536)?
            .facts
            .iter()
            .filter_map(|fact| fact.continuity_key)
            .collect();
        assert_eq!(keys, moved_keys);
        fs::write(
            root.path().join(moved),
            format!("// line shift with café\r\n{source}"),
        )?;
        assert_eq!(kernel.index()?.indexed, 1);
        let shifted_keys: std::collections::BTreeSet<_> = kernel
            .outline(moved, 100, 65_536)?
            .facts
            .iter()
            .filter_map(|fact| fact.continuity_key)
            .collect();
        assert_eq!(
            keys, shifted_keys,
            "qualified native identity survives line shifts"
        );
    }
    kernel.configure_extraction_policy(ExtractionPolicy {
        version: 1,
        rules: vec![SelectionRule {
            selector: RuleMatch::Exact { path: moved.into() },
            case_sensitive: true,
            adapter_id: "text".into(),
        }],
    })?;
    let overridden = kernel.index()?;
    assert_eq!((overridden.indexed, overridden.reused), (1, 1));
    let after = store.catalog()?.context("overridden")?;
    assert_ne!(
        after.inventory.as_ref().unwrap().files[moved]
            .state
            .profile_id,
        before.profile_id
    );
    assert_eq!(
        after.inventory.as_ref().unwrap().files["unrelated.rs"].state,
        untouched
    );
    Ok(())
}

macro_rules! qualification {
    ($test:ident,$path:literal,$moved:literal,$fixture:literal,$name:literal) => {
        #[test]
        fn $test() -> Result<()> {
            qualify(
                $path,
                $moved,
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../airk-ingest/tests/fixtures/languages-a/",
                    $fixture
                )),
                $name,
            )
        }
    };
}
qualification!(
    dart_pipeline,
    "original.dart",
    "moved.dart",
    "original.dart",
    "Archive"
);
qualification!(
    scala_pipeline,
    "original.scala",
    "moved.scala",
    "original.scala",
    "Archive"
);
qualification!(
    elixir_pipeline,
    "original.ex",
    "moved.ex",
    "original.ex",
    "render/1"
);
qualification!(
    objc_pipeline,
    "original.m",
    "moved.m",
    "original.m",
    "render:with:"
);
qualification!(
    objective_cpp_partial_pipeline,
    "original.mm",
    "moved.mm",
    "original.mm",
    "Archive"
);
