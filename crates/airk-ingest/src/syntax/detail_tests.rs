// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    Artifact,
    extraction::{ExtractionPayload, ExtractionProfile},
};
use sha2::{Digest, Sha256};
use std::cell::Cell;

thread_local! {
    static WORK: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

pub(super) fn record_evidence(kind: &str) {
    WORK.with(|work| {
        let (evidence, fingerprints) = work.get();
        work.set((
            evidence + 1,
            fingerprints + usize::from(kind == "definition"),
        ));
    });
}

fn measured<T>(run: impl FnOnce() -> anyhow::Result<T>) -> anyhow::Result<(T, (usize, usize))> {
    WORK.with(|work| work.set((0, 0)));
    let output = run()?;
    Ok((output, WORK.with(Cell::get)))
}

fn profile(adapter: &str, detail: IndexDetail) -> ExtractionProfile {
    let mut profile = crate::adapters::registry::AdapterRegistry::baseline()
        .get(adapter)
        .unwrap()
        .profile
        .clone();
    profile
        .options
        .insert("index_detail".into(), detail.as_str().into());
    profile
}

fn payload(
    path: &str,
    text: &str,
    profile: &ExtractionProfile,
    analysis: Analysis,
    seeds: Option<&SemanticSeeds>,
) -> anyhow::Result<ExtractionPayload> {
    let artifact = Artifact {
        path: path.into(),
        content_hash: format!("{:x}", Sha256::digest(text.as_bytes())),
        bytes: text.len(),
        chunks: crate::chunk(text),
        language: analysis.language,
        extraction_method: "utf8".into(),
        parse_status: analysis.status,
        facts: analysis.facts,
        retained: None,
    };
    crate::continuity::prepare_native_with_seeds(&artifact, text.as_bytes(), profile, seeds)
}

fn assert_selected_parity(path: &str, text: &str, adapter: &str) -> anyhow::Result<()> {
    for detail in [
        IndexDetail::Essential,
        IndexDetail::Balanced,
        IndexDetail::Detailed,
    ] {
        let selected_profile = profile(adapter, detail);
        let reference = payload(
            path,
            text,
            &selected_profile,
            analyze_selected(path, text, adapter)?,
            None,
        )?;
        let selected = analyze_selected_with_detail(path, text, adapter, detail)?;
        assert!(
            selected.seeds.is_some(),
            "{adapter} must qualify its own seeds"
        );
        let actual = payload(
            path,
            text,
            &selected_profile,
            selected.analysis,
            selected.seeds.as_ref(),
        )?;
        assert_eq!(
            serde_json::to_value(actual)?,
            serde_json::to_value(reference)?,
            "{path}: {detail:?}"
        );
    }
    Ok(())
}

#[test]
fn common_grammar_selected_payloads_match_full_reference_including_ids_and_locations()
-> anyhow::Result<()> {
    for (path, adapter, text) in [
        (
            "sample.rs",
            "rust",
            "// café\r\nuse std::fmt;\r\nstruct Service;\r\nimpl Service { #[test] fn run(&self) { ordinary(); db.query(); } }\r\n",
        ),
        (
            "sample.py",
            "python",
            "import os\nclass Service:\n    @app.get('/café')\n    def run(self):\n        ordinary()\n        db.query()\n        def inner():\n            return 1\n        return inner()\n",
        ),
        (
            "sample.js",
            "javascript",
            "import x from 'x';\nclass Service { run() { ordinary(); db.query(); } }\nrouter.get('/café', handler); test('ok', () => ordinary());\n",
        ),
        (
            "sample.ts",
            "typescript",
            "interface Token { value: string }\nclass Service { run(token: Token): boolean { ordinary(); db.query(token); return true; } }\nrouter.get('/a', handler); test('ok', () => ordinary());\n",
        ),
        (
            "sample.tsx",
            "tsx",
            "export const render = () => <div>café</div>;\nfunction run() { ordinary(); router.get('/a', handler); }\n",
        ),
        (
            "sample.go",
            "go",
            "package p\nimport \"fmt\"\nfunc run() { ordinary(); db.Query() }\n",
        ),
        (
            "Sample.java",
            "java",
            "import java.util.List; class Sample { @GetMapping(\"/x\") void run() { ordinary(); } }",
        ),
        (
            "sample.c",
            "c",
            "#include <stdio.h>\nint run(void) { ordinary(); return 1; }\n",
        ),
        (
            "sample.cpp",
            "cpp",
            "namespace sample { class Service { public: int run() { ordinary(); return 1; } }; }\n",
        ),
        (
            "Sample.cs",
            "csharp",
            "using System; class Sample { [HttpGet(\"/x\")] void Run() { ordinary(); } }",
        ),
        ("sample.sh", "bash", "run() { ordinary; }\n"),
        ("sample.ps1", "powershell", "function Run { ordinary }\n"),
        ("sample.lua", "lua", "function run() ordinary() end\n"),
        (
            "sample.swift",
            "swift",
            "class Service { func run() { ordinary() } }\n",
        ),
        (
            "sample.kt",
            "kotlin",
            "class Service { fun run() { ordinary() } }\n",
        ),
        ("sample.json", "json", "{\"café\": {\"key\": true}}\n"),
        ("sample.yaml", "yaml", "café:\n  key: true\n"),
        ("sample.toml", "toml", "[café]\nkey = true\n"),
        ("sample.css", "css", ".café { color: blue; }\n"),
        (
            "broken.rs",
            "rust",
            "fn first() { ordinary(); }\nfn broken( {\nfn tail() {}\n",
        ),
        (
            "broken.ts",
            "typescript",
            "function first() { db.query(); }\nfunction broken( {\n",
        ),
        ("empty.rs", "rust", ""),
    ] {
        assert_selected_parity(path, text, adapter)?;
    }
    Ok(())
}

#[test]
fn essential_avoids_rich_evidence_and_balanced_only_builds_selected_calls() -> anyhow::Result<()> {
    let text =
        "function run() { ordinary(); another(); db.query(); }\nrouter.get('/x', handler);\n";
    let (full, detailed_work) = measured(|| {
        analyze_selected_with_detail("sample.ts", text, "typescript", IndexDetail::Detailed)
    })?;
    let (balanced, balanced_work) = measured(|| {
        analyze_selected_with_detail("sample.ts", text, "typescript", IndexDetail::Balanced)
    })?;
    let (essential, essential_work) = measured(|| {
        analyze_selected_with_detail("sample.ts", text, "typescript", IndexDetail::Essential)
    })?;
    assert_eq!(detailed_work, (5, 1));
    assert_eq!(balanced_work, (3, 1));
    assert_eq!(essential_work, (0, 0));
    assert!(essential.analysis.facts.is_empty());
    assert_eq!(essential.seeds.as_ref().unwrap().len(), 1);
    let retained: Vec<_> = full
        .analysis
        .facts
        .into_iter()
        .filter(|fact| fact.kind != "call")
        .collect();
    assert_eq!(
        serde_json::to_value(balanced.analysis.facts)?,
        serde_json::to_value(retained)?
    );
    Ok(())
}

#[test]
fn detail_modes_retain_node_and_hidden_seed_limits_but_skip_unused_fingerprints()
-> anyhow::Result<()> {
    for detail in [
        IndexDetail::Essential,
        IndexDetail::Balanced,
        IndexDetail::Detailed,
    ] {
        let mut budget = HostBudget::new(3, 10, 0)?;
        analyze_selected_budgeted_with_detail("a.js", "", "javascript", detail, &mut budget)?;
        assert!(
            analyze_selected_budgeted_with_detail("b.js", "", "javascript", detail, &mut budget)
                .is_err()
        );
    }
    let mut budget = HostBudget::new(1000, 1, 0)?;
    analyze_selected_budgeted_with_detail(
        "a.rs",
        "fn first() {}",
        "rust",
        IndexDetail::Essential,
        &mut budget,
    )?;
    assert!(
        analyze_selected_budgeted_with_detail(
            "b.rs",
            "fn second() {}",
            "rust",
            IndexDetail::Essential,
            &mut budget
        )
        .is_err()
    );
    let mut budget = HostBudget::new(1000, 10, 0)?;
    budget.charge_fingerprints(MAX_CONTINUITY_HASH_BYTES)?;
    analyze_selected_budgeted_with_detail(
        "a.rs",
        "fn first() {}",
        "rust",
        IndexDetail::Essential,
        &mut budget,
    )?;
    assert!(
        analyze_selected_budgeted_with_detail(
            "a.rs",
            "fn first() {}",
            "rust",
            IndexDetail::Balanced,
            &mut budget
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn specialized_adapters_explicitly_keep_full_analysis_fallback() -> anyhow::Result<()> {
    for (path, adapter, text) in [
        ("sample.rb", "ruby", "def run\n ordinary()\nend\n"),
        ("sample.php", "php", "<?php function run() { ordinary(); }"),
        ("sample.sql", "sql", "CREATE TABLE users (id INTEGER);"),
    ] {
        let reference = analyze_selected(path, text, adapter)?;
        let selected = analyze_selected_with_detail(path, text, adapter, IndexDetail::Essential)?;
        assert!(selected.seeds.is_none());
        assert_eq!(selected.analysis.status, reference.status);
        assert_eq!(
            serde_json::to_value(selected.analysis.facts)?,
            serde_json::to_value(reference.facts)?
        );
    }
    Ok(())
}

#[test]
#[ignore = "manual model-free extraction timing qualification"]
fn qualify_selected_syntax_work_and_time() -> anyhow::Result<()> {
    let text: String = (0..150).map(|i| format!("// Written service {i}\nfunction service{i}(token: string) {{ {} db.query(token); }}\n", "ordinary(token); ".repeat(20))).collect();
    assert_selected_parity("sample.ts", &text, "typescript")?;
    for detail in [
        IndexDetail::Detailed,
        IndexDetail::Balanced,
        IndexDetail::Essential,
    ] {
        let mut samples = Vec::new();
        let mut work = (0, 0);
        let mut facts = 0;
        for _ in 0..7 {
            let start = std::time::Instant::now();
            let (selected, counted) = measured(|| {
                analyze_selected_with_detail("sample.ts", &text, "typescript", detail)
            })?;
            samples.push(start.elapsed().as_micros());
            work = counted;
            facts = selected.analysis.facts.len();
        }
        samples.sort_unstable();
        eprintln!(
            "S6 syntax detail={detail:?} bytes={} median_us={} rich_evidence={} fingerprint_sets={} facts={facts}",
            text.len(),
            samples[3],
            work.0,
            work.1
        );
    }
    Ok(())
}
