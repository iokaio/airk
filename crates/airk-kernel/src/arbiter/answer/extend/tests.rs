// SPDX-License-Identifier: Apache-2.0
//! The pure helpers of rules 3 and 4; expectations written by hand.
use super::*;

#[test]
fn minimal_and_rich_fact_rows_both_become_citable_facts() {
    let minimal = json!({"minimal":{"items":[
            {"name":"run","kind":"definition","lines":[3,6],"signature":null,"owner":"Owner"},
            {"name":"helper","kind":"call","lines":[4,4],"signature":null,"file":1}],
        "files":[{"path":"src/lib.rs","content_hash":"h1"},{"path":"src/other.rs","content_hash":"h2"}]}});
    let found = facts(&minimal);
    assert_eq!(
        found,
        [
            Fact {
                name: "run".into(),
                kind: "definition".into(),
                path: "src/lib.rs".into(),
                start: 3,
                end: 6,
                hash: Some("h1".into())
            },
            Fact {
                name: "helper".into(),
                kind: "call".into(),
                path: "src/other.rs".into(),
                start: 4,
                end: 4,
                hash: Some("h2".into())
            }
        ]
    );
    let rich = json!({"facts":[{"text":"guard","fact":{"kind":"function"},
        "citation":{"path":"src/g.rs","start_line":10,"end_line":40,"content_hash":"h3"}}]});
    let found = facts(&rich);
    assert_eq!(found.len(), 1);
    assert_eq!(
        (found[0].name.as_str(), found[0].start, found[0].end),
        ("guard", 10, 40)
    );
    // A row without a safe path or ordered lines is no fact.
    let bad = json!({"items":[{"name":"x","kind":"call","lines":[5,4]}],"files":[{"path":"a.rs"}]});
    assert!(facts(&bad).is_empty());
}

#[test]
fn a_call_names_its_last_segment() {
    assert_eq!(terminal("self.helper"), "helper");
    assert_eq!(terminal("Foo::bar::<T>"), "bar");
    assert_eq!(terminal("client?.send(request)"), "send");
    assert_eq!(terminal("plain"), "plain");
}

#[test]
fn cited_paths_are_relative_files_with_a_folder_most_cited_first() {
    let texts = [
        "Run `scripts/run-tests.ps1 -All` before you commit; see docs/testing.md.",
        "The script scripts/run-tests.ps1 lists the suites. Also ./docs/testing.md.",
        "Not https://example.com/a/b.md, not ../outside/x.rs, not /abs/y.rs, not docs/folder/.",
        "Cargo.toml has no folder; src/lib is no file.",
    ];
    assert_eq!(
        cited_paths(&texts, 5),
        ["docs/testing.md", "scripts/run-tests.ps1"]
    );
    assert_eq!(cited_paths(&texts, 1), ["docs/testing.md"]);
}

#[test]
fn guidance_folders_start_at_the_root_then_top_level_then_own_folders() {
    let paths = [
        "crates/k/src/guard.rs",
        "crates/k/src/lib.rs",
        "docs/a.md",
        "README.md",
    ];
    assert_eq!(
        guidance_folders(&paths, 6),
        ["", "crates", "docs", "crates/k/src"]
    );
    assert_eq!(guidance_folders(&paths, 2), ["", "crates"]);
}

#[test]
fn nearby_ranges_of_a_file_merge_within_the_line_bound() {
    assert_eq!(
        // 25 is 5 lines past 20 and joins; 52 is 22 past 30 and does not;
        // 400 to 900 is cut to 240 lines.
        merge(vec![(52, 60, 2), (10, 20, 1), (25, 30, 1), (400, 900, 3)]),
        [(10, 30, 1), (52, 60, 2), (400, 639, 3)]
    );
}

#[test]
fn guidance_sections_run_heading_to_heading_and_the_matching_ones_are_read_first() {
    let section = |name: &str, start| Fact {
        name: name.into(),
        kind: "section".into(),
        path: "CLAUDE.md".into(),
        start,
        end: start,
        hash: None,
    };
    let facts = [
        section("Execution identity", 40),
        section("Working on the project", 3),
        section("Required tests and review", 380),
        Fact {
            path: "README.md".into(),
            ..section("Elsewhere", 10)
        },
    ];
    // Lines before the first heading are a section; the last runs to the end.
    let spans = section_spans("CLAUDE.md", &facts, 420);
    assert_eq!(
        spans,
        [
            (1, 2, String::new()),
            (3, 39, "Working on the project".to_owned()),
            (40, 379, "Execution identity".to_owned()),
            (380, 420, "Required tests and review".to_owned()),
        ]
    );
    // No facts: the file is one section.
    assert_eq!(
        section_spans("CLAUDE.md", &[], 30),
        [(1, 30, String::new())]
    );
    // The late section shares "required", "tests" and "review" with need 3,
    // the opening section "project"; the rest follow in document order for
    // the file's owner, need 1.
    let needs = [
        (1, words("Where the restart guard decides")),
        (
            3,
            words("Project rules: required tests, test commands to run, review."),
        ),
    ];
    assert_eq!(
        choose_sections(&spans, &needs, 1),
        [(380, 420, 3), (3, 39, 3), (1, 2, 1)]
    );
    assert!(words("What this does, with each").is_empty());
}
