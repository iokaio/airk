// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::arbiter::answer::{gather::Record, retrieve::Need, v3};
use crate::arbiter::software::FollowUp;

const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn need(kind: NeedKind, subject: Option<&str>) -> Need {
    Need {
        id: 1,
        kind,
        subject: subject.map(str::to_owned),
        query: "trace the configured operation".into(),
    }
}

fn citation(path: &str, line: usize) -> Value {
    json!({"path":path,"start_line":line,"end_line":line,"content_hash":HASH,
           "location":{"Lines":{"start":line,"end":line}}})
}

fn record(hits: &[&str]) -> Record {
    let hits: Vec<_> = hits
        .iter()
        .map(|path| json!({"citation":citation(path, 5),"text":"candidate","truncated":false}))
        .collect();
    Record {
        id: HASH.parse().unwrap(),
        tool: "search",
        operation: v3::LoopRead::Search,
        arguments: Map::new(),
        body: json!({"hits":hits}),
        received: Value::Null,
        bytes: 0,
        kind: FollowUp::Keyword,
        subject: String::new(),
        seq: 1,
        uses: Vec::new(),
        failure: false,
        stage: v3::GatheringStageKind::Retrieve,
    }
}

#[test]
fn a_verbatim_task_literal_uses_a_cited_folder_and_never_searches_an_unknown_scope() {
    let cue = need(NeedKind::Errors, Some("task.limit-reached: 30 ms"));
    assert_eq!(
        from_record(
            &cue,
            Step::Literal,
            &record(&["../wrong.txt", "docs/misleading.md", "changed/owner.ts"])
        ),
        vec![Read::Literal {
            literal: "task.limit-reached: 30 ms".into(),
            prefix: "changed/".into(),
        }]
    );
    assert!(from_record(&cue, Step::Literal, &record(&[])).is_empty());
    assert!(
        from_record(
            &need(NeedKind::Errors, None),
            Step::Literal,
            &record(&["changed/owner.ts"]),
        )
        .is_empty()
    );
}

#[test]
fn facts_use_a_code_owner_or_a_document_only_for_the_matching_need() {
    let previous = record(&["docs/rules.md", "renamed/dispatch/owner.rs"]);
    let [Read::Canonical { tool, arguments }]: [Read; 1] = from_record(
        &need(NeedKind::Definition, Some("Gate")),
        Step::Search,
        &previous,
    )
    .try_into()
    .unwrap() else {
        panic!("expected one code-fact nomination");
    };
    assert_eq!(tool, "code_facts");
    assert_eq!(arguments["name"], "Gate");
    assert_eq!(arguments["path_prefix"], "renamed/dispatch/");
    assert_eq!(arguments["compact"], false);
    assert_eq!(arguments["limit"], 4);

    let [Read::Canonical { tool, arguments }]: [Read; 1] =
        from_record(&need(NeedKind::Contract, None), Step::Search, &previous)
            .try_into()
            .unwrap()
    else {
        panic!("expected one document-fact nomination");
    };
    assert_eq!(tool, "document_facts");
    assert_eq!(arguments["path"], "docs/rules.md");
    assert!(!arguments.contains_key("name"));

    assert!(
        from_record(
            &need(NeedKind::Definition, Some("free form behavior")),
            Step::Search,
            &previous,
        )
        .is_empty(),
    );
}

#[test]
fn a_one_hop_query_requires_a_source_bound_symbol_from_a_real_definition() {
    let caller = need(NeedKind::Callers, Some("Gate"));
    let interface = need(NeedKind::Interfaces, Some("Gate"));
    let owner = json!({"name":"Gate","kind":"definition","citation":citation("renamed/mod.rs", 5),
                       "code":{"symbol_id":"scope::Gate","qualified_name":"Gate"}});
    let body = json!({"items":[owner.clone()]});
    for (need, direction) in [(caller.clone(), "incoming"), (interface, "outgoing")] {
        let [Read::Canonical { tool, arguments }]: [Read; 1] =
            from_facts(&need, &body).try_into().unwrap()
        else {
            panic!("expected one relationship nomination");
        };
        assert_eq!(tool, "relationships");
        assert_eq!(arguments["symbol"], "scope::Gate");
        assert_eq!(arguments["direction"], direction);
        assert_eq!(arguments["hops"], 1);
        assert_eq!(arguments["path_prefix"], "renamed/");
    }
    let mut different = owner.clone();
    different["code"]["symbol_id"] = json!("other::Gate");
    assert!(from_facts(&caller, &json!({"items":[owner.clone(),different]})).is_empty());
    let mut occurrence = owner;
    occurrence["kind"] = json!("call");
    assert!(from_facts(&caller, &json!({"items":[occurrence]})).is_empty());
}

#[test]
fn locations_check_path_hash_and_coordinates_and_ignore_suggested_targets() {
    let one = json!({"citation":citation("src/first.rs", 7)});
    let two = json!({"citation":citation("src/second.rs", 8)});
    let extra = json!({"citation":citation("src/extra.rs", 11)});
    let mut invalid = one.clone();
    invalid["citation"]["content_hash"] = json!("stale");
    let response = json!({
        "suggested_targets":[{"citation":citation("src/pointer.rs", 1)}],
        "relationships":[invalid,one.clone(),one,two,extra,
                         {"citation":citation("../outside.rs", 2)}],
    });
    assert_eq!(
        locations("relationships", &response),
        vec![
            Location {
                path: "src/first.rs".into(),
                start: 7,
                end: 7,
                known_hash: HASH.parse().unwrap()
            },
            Location {
                path: "src/second.rs".into(),
                start: 8,
                end: 8,
                known_hash: HASH.parse().unwrap()
            },
        ],
    );
    assert!(
        locations(
            "literal_scan",
            &json!({"matches":[{"citation":citation("src/unavailable.rs", 0)},
                               {"citation":citation("src//unavailable.rs", 1)}]}),
        )
        .is_empty(),
    );
}
