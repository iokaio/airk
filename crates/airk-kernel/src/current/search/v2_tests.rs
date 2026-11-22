// SPDX-License-Identifier: Apache-2.0
//! The evaluation search switch: product search is unchanged when it is off,
//! and a prose instruction reaches named source when it is on.
use super::*;
use crate::Kernel;
use sha2::{Digest, Sha256};
use std::fs;

const PROSE: &str =
    "Add Mistral as another provider next to DeepSeek and wire the factory class from the guide";

fn fixture() -> Result<(tempfile::TempDir, Kernel)> {
    let directory = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let root = directory.path().join("source");
    let data = directory.path().join("data");
    let model = data.join("models/local");
    fs::create_dir_all(root.join("docs"))?;
    fs::create_dir_all(root.join("src/generated"))?;
    fs::create_dir_all(&model)?;
    // Prose files sit near the instruction in vector space; source does not.
    fs::write(
        root.join("docs/guide.md"),
        "Provider guide. The provider guide explains provider setup.\n",
    )?;
    fs::write(root.join("notes.txt"), "provider provider\n")?;
    fs::write(
        root.join("src/ProviderFactory.cs"),
        "class ProviderFactory { Mistral DeepSeekClient code }\n",
    )?;
    fs::write(
        root.join("src/generated/Client.g.cs"),
        "class ProviderFactory { Mistral DeepSeekClient code }\n",
    )?;
    let vocabulary = serde_json::to_vec(&serde_json::json!({
        "provider":[1.0,0.0],"guide":[1.0,0.0],"class":[0.0,1.0],"code":[0.0,1.0]
    }))?;
    fs::write(model.join("vocabulary.json"), &vocabulary)?;
    fs::write(
        model.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1,"name":"search-v2-fixture","license":"Apache-2.0",
            "dimensions":2,"sha256":format!("{:x}", Sha256::digest(&vocabulary))
        }))?,
    )?;
    let kernel = Kernel::open(&root, Some(data))?;
    kernel.index()?;
    Ok((directory, kernel))
}

fn query(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        path_prefix: None,
        limit: 10,
        max_bytes: 65_536,
    }
}

fn switched(kernel: &Kernel, text: &str, on: bool) -> Result<CurrentSearchEvidence> {
    search_with_switch(
        &kernel.store,
        kernel.current_reader()?,
        None,
        CurrentSearchMode::Hybrid,
        &query(text),
        SearchOptions {
            requested_mode: None,
            snippet_bytes: None,
            filters: &SearchFilters::default(),
        },
        on,
    )
}

fn paths(hits: &[Hit]) -> Vec<&str> {
    hits.iter().map(|hit| hit.citation.path.as_str()).collect()
}

/// Pin this test process's switch off, whatever its environment holds: the
/// binding fails loudly if the process already read a different value.
fn product_switch() -> Result<()> {
    #[cfg(feature = "test-support")]
    v2::bind_evaluation_search(false)?;
    anyhow::ensure!(!v2::active()?, "product search switch is on");
    Ok(())
}

#[test]
fn switch_off_hybrid_search_is_the_product_search() -> Result<()> {
    product_switch()?;
    let (_owner, kernel) = fixture()?;
    for text in [PROSE, "provider", "class code"] {
        let product = kernel.hybrid_search(&query(text))?;
        let off = switched(&kernel, text, false)?;
        assert_eq!(
            serde_json::to_vec(&product.hits)?,
            serde_json::to_vec(&off.hits)?,
            "{text}"
        );
        assert_eq!(product.truncated, off.truncated, "{text}");
    }
    // Independent of the fusion code: a prose instruction matches no chunk
    // conjunctively, so the product's hybrid order is the vector order.
    let vector = kernel.semantic_search(&query(PROSE))?;
    let hybrid = kernel.hybrid_search(&query(PROSE))?;
    assert_eq!(paths(&hybrid.hits), paths(&vector.hits));
    assert_eq!(hybrid.hits.len(), 4);
    assert!(matches!(
        hybrid.hits[0].citation.path.as_str(),
        "docs/guide.md" | "notes.txt"
    ));
    Ok(())
}

#[test]
fn switch_on_leaves_short_queries_unchanged() -> Result<()> {
    let (_owner, kernel) = fixture()?;
    for text in ["provider", "class code", "provider factory class"] {
        let off = switched(&kernel, text, false)?;
        let on = switched(&kernel, text, true)?;
        assert_eq!(
            serde_json::to_vec(&off.hits)?,
            serde_json::to_vec(&on.hits)?,
            "{text}"
        );
        assert_eq!(off.truncated, on.truncated, "{text}");
    }
    Ok(())
}

#[test]
fn switch_on_ranks_named_source_first_and_generated_copies_below_prose() -> Result<()> {
    let (_owner, kernel) = fixture()?;
    let on = switched(&kernel, PROSE, true)?;
    let found = paths(&on.hits);
    assert_eq!(found[0], "src/ProviderFactory.cs", "{found:?}");
    let position = |path: &str| found.iter().position(|found| *found == path);
    let generated = position("src/generated/Client.g.cs").expect("generated copy retrieved");
    let guide = position("docs/guide.md").expect("guide retrieved");
    assert!(guide < generated, "{found:?}");
    // Each chunk is emitted once although it reached several branches.
    let mut unique = found.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), found.len());
    Ok(())
}

/// Lexical-only workspace: `out/` chunks match two groups, `in/` one.
fn scoped_fixture() -> Result<(tempfile::TempDir, Kernel)> {
    let directory = tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let root = directory.path().join("source");
    fs::create_dir_all(root.join("out"))?;
    fs::create_dir_all(root.join("in"))?;
    for index in 0..12 {
        fs::write(root.join(format!("out/o{index:02}.txt")), "alpha beta\n")?;
    }
    fs::write(root.join("in/target.txt"), "alpha gamma\n")?;
    let kernel = Kernel::open(&root, Some(directory.path().join("data")))?;
    kernel.index()?;
    Ok((directory, kernel))
}

fn groups(words: &[&str]) -> Vec<TermGroup> {
    words
        .iter()
        .map(|word| TermGroup {
            alternatives: vec![vec![(*word).to_owned()]],
        })
        .collect()
}

#[test]
fn ranked_lexical_filters_scope_before_taking_its_limit() -> Result<()> {
    let (_owner, kernel) = scoped_fixture()?;
    let read = kernel.current_reader()?;
    let wanted = groups(&["alpha", "beta"]);
    // Twelve out-of-scope chunks outscore the one in-scope chunk; with a
    // limit of 1 the old global cut (4 rows) held none of `in/`.
    let prefixed = read.lexical_search_ranked_any(&wanted, Some("in/"), None, 1)?;
    assert_eq!(paths_of(&prefixed.candidates.values), vec!["in/target.txt"]);
    let selector = std::collections::BTreeSet::from(["in/target.txt".to_owned()]);
    let selected = read.lexical_search_ranked_any(&wanted, None, Some(&selector), 1)?;
    assert_eq!(paths_of(&selected.candidates.values), vec!["in/target.txt"]);
    // Unscoped, the two-group matches rank first and the rest is disclosed.
    let all = read.lexical_search_ranked_any(&wanted, None, None, 1)?;
    assert!(all.candidates.values[0].citation.path.starts_with("out/"));
    assert!(all.candidates.incomplete);
    assert!(all.dropped_groups.is_empty());
    Ok(())
}

fn paths_of(values: &[airk_store::current::lexical::CurrentLexicalCandidate]) -> Vec<&str> {
    values
        .iter()
        .map(|value| value.citation.path.as_str())
        .collect()
}

#[test]
fn switch_on_prose_with_unrankable_words_still_searches() -> Result<()> {
    let (_owner, kernel) = fixture()?;
    let text = format!(
        "{PROSE} near İstanbul and {} and {}Part",
        "z".repeat(129),
        "y".repeat(130)
    );
    let on = switched(&kernel, &text, true)?;
    assert_eq!(paths(&on.hits)[0], "src/ProviderFactory.cs");
    Ok(())
}
