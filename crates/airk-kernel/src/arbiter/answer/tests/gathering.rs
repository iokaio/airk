// SPDX-License-Identifier: Apache-2.0
//! The gathering rules (kernel CONTRACT "Gathering rules") on the real loop
//! with canned reads: one project whose guidance, cited files, code facts
//! and references are known, so every expected read is written out by hand.

use super::*;

const GUARD: &str = "crates/k/src/guard.rs";
const NEEDS: [&str; 2] = [
    "Where the restart guard decides whether to replay",
    "Which callers invoke the restart guard",
];

fn rules(text: &str) -> GatheringRules {
    GatheringRules::parse(text).expect("rules")
}

/// The canned project: listings, code facts, references and a project rule
/// that names two files; every other read is `canned`.
fn project(
    publication: &RevisionEnvelope,
    tool: &str,
    arguments: &Map<String, Value>,
) -> Result<Value> {
    let text = |key: &str| arguments.get(key).and_then(Value::as_str);
    let base =
        json!({"publication":publication,"generation":1,"collection":"fixture","truncated":false});
    let page = |items: Value, path: &str, hash: &str| {
        let mut body = base.clone();
        body["minimal"] = json!({"items":items,"files":[{"path":path,"content_hash":hash,
            "language":"rust","parse_status":"parsed","line_basis":"utf8","provenance":0}],
            "provenance":[]});
        body
    };
    Ok(match tool {
        "list_files" => {
            let paths: &[&str] = match text("prefix").unwrap_or("") {
                "" => &["AGENTS.md", "CLAUDE.md", "Cargo.toml", GUARD],
                "crates/" => &["crates/README.md", GUARD],
                "crates/k/src/" => &[GUARD, "crates/k/src/CONTRACT.md"],
                _ => &[],
            };
            let mut body = base.clone();
            body["paths"] = json!(paths);
            body
        }
        "code_facts" => match (text("path"), text("name"), text("kind").unwrap_or("")) {
            (Some(GUARD), None, "definition") => page(
                json!([{"name":"restart_guard","kind":"definition","lines":[10,40],"signature":null},
                       {"name":"helper","kind":"definition","lines":[50,60],"signature":null}]),
                GUARD,
                "guard-hash",
            ),
            (Some(GUARD), None, "call") => page(
                json!([{"name":"self.helper","kind":"call","lines":[20,20],"signature":null},
                       {"name":"remote_check","kind":"call","lines":[22,22],"signature":null}]),
                GUARD,
                "guard-hash",
            ),
            (Some(GUARD), None, "inherits") => page(
                json!([{"name":"Guard","kind":"inherits","lines":[10,10],"signature":null}]),
                GUARD,
                "guard-hash",
            ),
            (None, Some("remote_check"), "definition") => page(
                json!([{"name":"remote_check","kind":"definition","lines":[5,15],"signature":null,"file":0}]),
                "crates/net/src/check.rs",
                "check-hash",
            ),
            (None, Some("restart_guard"), "call") => page(
                json!([{"name":"restart_guard","kind":"call","lines":[30,30],"signature":null,"file":0}]),
                "crates/app/src/main.rs",
                "main-hash",
            ),
            _ => page(json!([]), GUARD, "guard-hash"),
        },
        "references" => {
            let mut body = base.clone();
            body["facts"] = json!([{"text":"Guard","citation":{"path":"crates/app/src/use.rs",
                "start_line":7,"end_line":7,"content_hash":"use-hash"}}]);
            body
        }
        "context" if text("path") == Some("CLAUDE.md") => {
            let mut body = base.clone();
            body["hits"] = json!([hit(
                "CLAUDE.md",
                1,
                3,
                "Every change runs `scripts/run-tests.ps1 -All` and updates docs/ledger.md."
            )]);
            body
        }
        _ => canned(publication, tool, arguments)?,
    })
}

/// One Answer under `rules`, with `alter` over the canned project.
fn run_ruled(
    fixture: &Fixture,
    request: v3::ContextRequestV3,
    rules: GatheringRules,
    alter: &Alter,
) -> Result<(AnswerOutcome, Vec<String>)> {
    let clock = Clock::new();
    let reads = Mutex::new(Vec::new());
    let publication = fixture.publication.clone();
    let outcome = fixture.kernel.answer_with_rules(
        request,
        AnswerEnv {
            deadline: clock.now() + Duration::from_secs(600),
            require_live: &|| Ok(()),
            reader: &|tool, arguments, revision| {
                ensure!(revision == Some(publication.token.to_string().as_str()));
                clock.advance(5);
                reads
                    .lock()
                    .unwrap()
                    .push(format!("{tool} {}", Value::Object(arguments.clone())));
                alter(tool, arguments, project(&publication, tool, arguments)?)
            },
            clock: &|| clock.now(),
            progress: &|_| {},
        },
        rules,
    )?;
    outcome
        .response
        .validate_with_log(&outcome.custody.log, &v3::BudgetV3::default())?;
    Ok((outcome, reads.into_inner().unwrap()))
}

fn same(_: &str, _: &Map<String, Value>, body: Value) -> Result<Value> {
    Ok(body)
}

fn asked(deliverable: Option<v3::Deliverable>) -> v3::ContextRequestV3 {
    let mut request = request("Fix the restart guard so it refuses a stale replay");
    request.needs = NEEDS.map(String::from).to_vec();
    request.deliverable = deliverable;
    request
}

/// Reads of `tool` whose arguments hold every one of `parts`.
fn reads_of<'a>(reads: &'a [String], tool: &str, parts: &[&str]) -> Vec<&'a String> {
    reads
        .iter()
        .filter(|read| {
            read.starts_with(&format!("{tool} ")) && parts.iter().all(|part| read.contains(part))
        })
        .collect()
}

fn query(text: &str) -> String {
    format!("\"query\":{}", serde_json::to_string(text).unwrap())
}

#[test]
fn standing_needs_follow_the_callers_by_deliverable() -> Result<()> {
    let (outcome, _) = run_ruled(
        &fixture()?,
        asked(Some(v3::Deliverable::CodeChange)),
        rules("standing"),
        &same,
    )?;
    let part = &outcome.response.parts[0];
    let texts: Vec<&str> = part
        .sub_needs
        .iter()
        .map(|need| need.text.as_str())
        .collect();
    assert_eq!(
        texts,
        [
            NEEDS[0],
            NEEDS[1],
            "Project rules that say what a change of this kind must include before it is done: required tests, test commands to run, logs or documents to update, review.",
            "The script or command that runs this project's tests, its options, and which tests each option includes.",
            "The build or project file that configures which tests run and what they need.",
            "Where an exception thrown by this code is caught and turned into the message the user sees.",
        ]
    );
    let origins: Vec<v3::SubNeedOrigin> = part.sub_needs.iter().map(|need| need.origin).collect();
    use v3::SubNeedOrigin::{Caller, Standing};
    assert_eq!(
        origins,
        [Caller, Caller, Standing, Standing, Standing, Standing]
    );
    let raw = serde_json::to_value(&outcome.response)?;
    assert_eq!(raw["parts"][0]["sub_needs"][2]["origin"], "standing");

    // A test takes three; analysis takes three only beside caller needs.
    let (outcome, _) = run_ruled(
        &fixture()?,
        asked(Some(v3::Deliverable::Test)),
        rules("standing"),
        &same,
    )?;
    assert_eq!(outcome.response.parts[0].sub_needs.len(), 5);
    let mut bare = request("Why does the restart guard refuse a replay?");
    bare.deliverable = Some(v3::Deliverable::Analysis);
    let (outcome, _) = run_ruled(&fixture()?, bare, rules("all"), &same)?;
    assert!(
        outcome.response.parts[0]
            .sub_needs
            .iter()
            .all(|need| need.origin == v3::SubNeedOrigin::Derived)
    );
    // A code change without caller needs still takes all four after its
    // derived sub-needs.
    let mut change = request("Why does the restart guard refuse a replay?");
    change.deliverable = Some(v3::Deliverable::CodeChange);
    let (outcome, _) = run_ruled(&fixture()?, change, rules("standing"), &same)?;
    let needs = &outcome.response.parts[0].sub_needs;
    assert_eq!(
        needs
            .iter()
            .filter(|need| need.origin == v3::SubNeedOrigin::Standing)
            .count(),
        4
    );
    assert_eq!(needs[0].origin, v3::SubNeedOrigin::Derived);
    Ok(())
}

#[test]
fn each_need_searches_code_and_standing_build_needs_search_configuration() -> Result<()> {
    let (outcome, reads) = run_ruled(
        &fixture()?,
        asked(Some(v3::Deliverable::CodeChange)),
        rules("lanes,standing"),
        &same,
    )?;
    let needs = &outcome.response.parts[0].sub_needs;
    for need in needs {
        let code = reads_of(
            &reads,
            "search",
            &[&query(&need.text), "\"evidence_kind\":\"code\""],
        );
        assert_eq!(code.len(), 1, "one code lane for {}", need.text);
        // Bounded snippets, not whole chunks (change-log entry 76).
        assert!(code[0].contains("\"full_chunks\":false"));
        assert!(code[0].contains("\"max_bytes\":4096"));
        let configuration = reads_of(
            &reads,
            "search",
            &[&query(&need.text), "\"evidence_kind\":\"configuration\""],
        );
        // Only the test-command and build-file standing needs.
        let expected = usize::from(need.id == 4 || need.id == 5);
        assert_eq!(
            configuration.len(),
            expected,
            "configuration lane for {}",
            need.text
        );
    }
    // Lanes off: no search names an evidence kind.
    let (_, reads) = run_ruled(
        &fixture()?,
        asked(Some(v3::Deliverable::CodeChange)),
        rules("standing"),
        &same,
    )?;
    assert!(reads_of(&reads, "search", &["evidence_kind"]).is_empty());
    Ok(())
}

#[test]
fn guidance_is_listed_per_folder_read_and_the_files_it_names_are_read() -> Result<()> {
    let (outcome, reads) = run_ruled(&fixture()?, asked(None), rules("guidance,cited"), &same)?;
    // One listing per folder: the root, the top-level folders, then the
    // passages' own folders.
    let listings: Vec<&String> = reads_of(&reads, "list_files", &[]);
    for prefix in [
        "\"prefix\":\"\"",
        "\"prefix\":\"crates/\"",
        "\"prefix\":\"crates/k/src/\"",
    ] {
        assert_eq!(
            listings.iter().filter(|read| read.contains(prefix)).count(),
            1,
            "{prefix}: {listings:?}"
        );
    }
    assert!(
        reads_of(&reads, "file_info", &["CLAUDE.md"]).is_empty(),
        "no existence checks"
    );
    // The guidance files the listings show, each read from its first line.
    for path in ["CLAUDE.md", "AGENTS.md", "crates/README.md"] {
        let head = reads_of(
            &reads,
            "context",
            &[
                &format!("\"path\":\"{path}\""),
                "\"line\":1",
                "\"after\":100",
            ],
        );
        assert_eq!(head.len(), 1, "{path}");
    }
    assert!(reads_of(&reads, "context", &["\"path\":\"Cargo.toml\""]).is_empty());
    // The files CLAUDE.md names.
    for path in ["scripts/run-tests.ps1", "docs/ledger.md"] {
        assert_eq!(
            reads_of(&reads, "context", &[&format!("\"path\":\"{path}\"")]).len(),
            1,
            "{path}"
        );
    }
    let report = outcome.response.gathering.as_ref().context("report")?;
    let stage = |kind| report.stages.iter().find(|stage| stage.stage == kind);
    let guidance = stage(v3::GatheringStageKind::Guidance).context("guidance stage")?;
    assert_eq!(guidance.lookups, listings.len());
    assert_eq!(
        guidance.reads,
        [v3::GatheringReads {
            operation: v3::LoopRead::Context,
            count: 3
        }]
    );
    let cited = stage(v3::GatheringStageKind::Cited).context("cited stage")?;
    assert_eq!(cited.reads[0].count, 2);
    Ok(())
}

#[test]
fn structure_follows_enclosing_definitions_callees_callers_and_type_users() -> Result<()> {
    let (outcome, reads) = run_ruled(&fixture()?, asked(None), rules("structure"), &same)?;
    // The enclosing definition (10-40) and the local callee `helper` (50-60)
    // are one read of the guard file, bound to the index's file hash.
    let guard = reads_of(
        &reads,
        "read_file",
        &[
            &format!("\"path\":\"{GUARD}\""),
            "\"start_line\":10",
            "\"end_line\":60",
        ],
    );
    assert_eq!(guard.len(), 1, "{reads:?}");
    assert!(guard[0].contains("\"known_hash\":\"guard-hash\""));
    // The callee defined elsewhere, found by name.
    let callee = reads_of(
        &reads,
        "read_file",
        &[
            "crates/net/src/check.rs",
            "\"start_line\":5",
            "\"end_line\":15",
        ],
    );
    assert_eq!(callee.len(), 1);
    assert!(callee[0].contains("\"known_hash\":\"check-hash\""));
    // A call site of the enclosing function, with lines around it.
    assert_eq!(
        reads_of(
            &reads,
            "context",
            &["crates/app/src/main.rs", "\"line\":30", "\"before\":12"]
        )
        .len(),
        1
    );
    // A user of the inherited type outside the implementing folder.
    let users = reads_of(&reads, "references", &["\"name\":\"Guard\""]);
    assert_eq!(users.len(), 1);
    assert!(users[0].contains("\"exclude_path_prefix\":\"crates/k/src/\""));
    assert_eq!(
        reads_of(&reads, "context", &["crates/app/src/use.rs", "\"line\":7"]).len(),
        1
    );
    // The lookups ask for line ranges and minimal rows; none guesses a name.
    let calls = reads_of(
        &reads,
        "code_facts",
        &["\"kind\":\"call\"", "\"start_line\":10"],
    );
    assert_eq!(calls.len(), 1);
    assert!(calls[0].contains("\"minimal\":true") && calls[0].contains("\"end_line\":40"));
    // Every structure read is a record the reply can cite; lookups are not.
    let report = outcome.response.gathering.as_ref().context("report")?;
    let structure = report
        .stages
        .iter()
        .find(|stage| stage.stage == v3::GatheringStageKind::Structure)
        .context("structure stage")?;
    assert!(structure.lookups >= 5, "{structure:?}");
    assert!(
        outcome
            .custody
            .nominations
            .iter()
            .all(|row| row["tool"] != "read_file")
    );
    Ok(())
}

#[test]
fn a_passage_fetched_for_one_need_is_offered_to_every_need() -> Result<()> {
    let mut request = request("Fix how quokka state is parsed");
    let first = "Where quokka state is parsed";
    let second = "Which wombat callers exist";
    request.needs = vec![first.into(), second.into()];
    // Only the second need's searches find the zoo file, which answers the
    // first need in its own words.
    let zoo = |tool: &str, arguments: &Map<String, Value>, mut body: Value| {
        if tool == "search" && arguments.get("query").and_then(Value::as_str) == Some(second) {
            body["hits"].as_array_mut().context("hits")?.push(hit(
                "crates/z/src/zoo.rs",
                1,
                9,
                "Where quokka state is parsed: fn parse_quokka_state() {} // wombat",
            ));
        }
        Ok(body)
    };
    let cites_zoo = |outcome: &AnswerOutcome| {
        outcome.response.paragraphs[0]
            .text
            .contains("crates/z/src/zoo.rs")
    };
    let (outcome, _) = run_ruled(&fixture()?, request.clone(), rules("cross,delivery"), &zoo)?;
    assert!(
        cites_zoo(&outcome),
        "{}",
        outcome.response.paragraphs[0].text
    );
    let (outcome, _) = run_ruled(&fixture()?, request, rules("delivery"), &zoo)?;
    assert!(
        !cites_zoo(&outcome),
        "{}",
        outcome.response.paragraphs[0].text
    );
    Ok(())
}

#[test]
fn delivery_carries_only_each_needs_chosen_passages_and_no_need_is_empty() -> Result<()> {
    let spans = |outcome: &AnswerOutcome| -> usize {
        outcome
            .response
            .records
            .iter()
            .map(|record| {
                record.canonical_result["spans"]
                    .as_array()
                    .map_or(0, Vec::len)
            })
            .sum()
    };
    let (delivered, _) = run_ruled(
        &fixture()?,
        asked(Some(v3::Deliverable::CodeChange)),
        rules("all"),
        &same,
    )?;
    let (undelivered, _) = run_ruled(
        &fixture()?,
        asked(Some(v3::Deliverable::CodeChange)),
        rules("all,-delivery"),
        &same,
    )?;
    for need in &delivered.response.parts[0].sub_needs {
        assert_ne!(need.coverage, v3::Coverage::Missing, "{}", need.text);
        assert!(!need.evidence.is_empty(), "{}", need.text);
    }
    assert!(
        spans(&delivered) < spans(&undelivered),
        "{} delivered spans, {} without delivery",
        spans(&delivered),
        spans(&undelivered)
    );
    assert!(delivered.response.gathering.as_ref().is_some_and(|report| {
        report
            .stages
            .iter()
            .any(|stage| stage.passages_delivered > 0)
    }));
    Ok(())
}

#[test]
fn a_project_rule_fetched_for_the_rules_need_is_delivered_to_it() -> Result<()> {
    // The project's CLAUDE.md states its rules in the rules need's terms.
    let rule = "Project rules: a change of this kind must include required tests, run the test commands, update logs or documents, and pass review before it is done.";
    let alter = |tool: &str, arguments: &Map<String, Value>, mut body: Value| {
        if tool == "context" && arguments.get("path").and_then(Value::as_str) == Some("CLAUDE.md") {
            body["hits"] = json!([hit("CLAUDE.md", 1, 3, rule)]);
        }
        Ok(body)
    };
    let (outcome, _) = run_ruled(
        &fixture()?,
        asked(Some(v3::Deliverable::Test)),
        rules("standing,guidance,delivery"),
        &alter,
    )?;
    let response = &outcome.response;
    // Sub-need 3 is the standing rules need; its paragraph lists CLAUDE.md.
    assert_eq!(
        response.parts[0].sub_needs[2].origin,
        v3::SubNeedOrigin::Standing
    );
    let paragraph = response
        .paragraphs
        .iter()
        .find(|paragraph| paragraph.text.starts_with("Part 1 sub-need 3 "))
        .context("rules paragraph")?;
    assert!(
        paragraph.text.contains("CLAUDE.md:1-3"),
        "{}",
        paragraph.text
    );
    let report = response.gathering.as_ref().context("report")?;
    let guidance = report
        .stages
        .iter()
        .find(|stage| stage.stage == v3::GatheringStageKind::Guidance)
        .context("guidance stage")?;
    assert!(guidance.passages_delivered >= 1, "{guidance:?}");
    Ok(())
}

#[test]
fn the_report_counts_every_call_by_stage_and_names_the_rules_on() -> Result<()> {
    let (outcome, reads) = run_ruled(
        &fixture()?,
        asked(Some(v3::Deliverable::CodeChange)),
        rules("all"),
        &same,
    )?;
    let response = &outcome.response;
    let report = response.gathering.as_ref().context("report")?;
    // The report as the reply carries it, for a reader of the test output.
    println!("{}", serde_json::to_string(report)?);
    assert_eq!(
        report.rules,
        [
            v3::GatheringRule::SearchLanes,
            v3::GatheringRule::StandingNeeds,
            v3::GatheringRule::ProjectGuidance,
            v3::GatheringRule::CitedFiles,
            v3::GatheringRule::StructureFollowing,
            v3::GatheringRule::EveryPassageToEveryNeed,
            v3::GatheringRule::DeliveryByNeed,
        ]
    );
    let counted: usize = report
        .stages
        .iter()
        .map(|stage| stage.lookups + stage.reads.iter().map(|reads| reads.count).sum::<usize>())
        .sum();
    assert_eq!(counted, response.work.calls);
    // Every canonical read the reader saw is a counted call, except the
    // uncharged governing-document probes (`file_info`).
    let charged = reads
        .iter()
        .filter(|read| !read.starts_with("file_info "))
        .count();
    assert_eq!(counted, charged);
    let kinds: Vec<v3::GatheringStageKind> =
        report.stages.iter().map(|stage| stage.stage).collect();
    use v3::GatheringStageKind::*;
    for kind in [Retrieve, Guidance, Cited, Structure, Coverage, Delivery] {
        assert!(kinds.contains(&kind), "{kind:?} in {kinds:?}");
    }
    // The clock moves 5 ms a read, so the read-heavy stages took time.
    let retrieve = report
        .stages
        .iter()
        .find(|stage| stage.stage == Retrieve)
        .unwrap();
    assert!(retrieve.elapsed_ms > 0 && retrieve.passages_fetched > 0);
    Ok(())
}

#[test]
fn a_request_without_caller_inputs_keeps_its_reads_whatever_the_rules() -> Result<()> {
    let bare = request("Where is `restart_guard` defined and which tests cover it?");
    let (all, mut with_rules) = run_ruled(&fixture()?, bare.clone(), rules("all"), &same)?;
    let (none, mut without) = run_ruled(&fixture()?, bare, rules("none"), &same)?;
    // A wave's reads reach the reader concurrently, so the log is compared
    // as a set of reads; the records keep dispatch order.
    with_rules.sort();
    without.sort();
    assert_eq!(with_rules, without);
    // Each run has its own fixture publication, so record ids differ; the
    // records' reads, in dispatch order, do not.
    let records = |outcome: &AnswerOutcome| -> Vec<String> {
        outcome
            .response
            .records
            .iter()
            .map(|record| format!("{:?} {:?}", record.operation, record.arguments))
            .collect()
    };
    assert_eq!(records(&all), records(&none));
    let report = all.response.gathering.as_ref().context("report")?;
    assert!(report.rules.is_empty());
    assert!(
        all.response
            .parts
            .iter()
            .flat_map(|part| &part.sub_needs)
            .all(|need| need.origin == v3::SubNeedOrigin::Derived)
    );
    Ok(())
}

/// Evidence reads whose replies fill their own `max_bytes` bound, the most
/// a canonical read returns: whole chunks, contexts, excerpts and outlines
/// at nine tenths of it; a snippet search at 512 bytes a hit at most.
/// Private lookups (listings, file information, facts, references) keep
/// their natural size. Each search finds the guard and files of its own
/// query, so different sub-needs read different whole chunks and outlines.
fn sized(tool: &str, arguments: &Map<String, Value>, mut body: Value) -> Result<Value> {
    if tool == "search" {
        let query = arguments["query"].as_str().unwrap_or("");
        let limit = arguments["limit"].as_u64().unwrap_or(8) as usize;
        let tag = framed_digest(b"sized-search", &[query.as_bytes()]).to_string()[..8].to_owned();
        let mut hits = vec![hit(
            GUARD,
            10,
            40,
            &format!("pub fn restart_guard() {{}} // {query}"),
        )];
        for index in 1..limit {
            hits.push(hit(
                &format!("crates/q{tag}/src/m{index}.rs"),
                1,
                30,
                &format!("fn m{index}() {{}} // {query}"),
            ));
        }
        body["hits"] = json!(hits);
    }
    if !matches!(
        tool,
        "search" | "context" | "read_file" | "outline" | "symbols" | "imports"
    ) {
        return Ok(body);
    }
    let bound = arguments
        .get("max_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(4096) as usize
        * 9
        / 10;
    let hits = body["hits"].as_array().map_or(0, Vec::len);
    let bound = if tool == "search" && arguments["full_chunks"] == false {
        bound.min(hits * 512)
    } else {
        bound
    };
    let size = serde_json::to_vec(&body)?.len();
    let pad = bound.saturating_sub(size);
    if let Some(each) = pad.checked_div(hits) {
        for item in body["hits"].as_array_mut().context("hits")? {
            let text = item["text"].as_str().unwrap_or("").to_owned();
            item["text"] = json!(format!("{text}\n{}", "~".repeat(each)));
        }
    } else {
        body["padding"] = json!("~".repeat(pad));
    }
    Ok(body)
}

/// Bytes retained by each gathering stage, from the custody records and
/// private lookups in dispatch order: (stage, reads, read bytes, lookup bytes).
fn retained_by_stage(
    outcome: &AnswerOutcome,
) -> Vec<(v3::GatheringStageKind, usize, usize, usize)> {
    let Some(report) = &outcome.response.gathering else {
        return vec![];
    };
    let mut records = outcome.custody.records.iter();
    let mut lookups = outcome.custody.nominations.iter();
    report
        .stages
        .iter()
        .map(|stage| {
            let reads: usize = stage.reads.iter().map(|reads| reads.count).sum();
            let bytes = records
                .by_ref()
                .take(reads)
                .map(|record| {
                    serde_json::to_vec(&super::super::volatile::stable(&record.canonical_result))
                        .map_or(0, |bytes| bytes.len())
                })
                .sum();
            let charged = lookups
                .by_ref()
                .take(stage.lookups)
                .map(|row| row["outcome"]["charged_bytes"].as_u64().unwrap_or(0) as usize)
                .sum();
            (stage.stage, reads, bytes, charged)
        })
        .collect()
}

#[test]
fn the_default_retention_leaves_round_one_room_for_guidance_cited_and_structure() -> Result<()> {
    // Six caller needs and the four standing ones of a code change, two
    // anchors, the default budget: the shape of the first kernel baseline.
    let mut request = request("Fix the restart guard so it refuses a stale replay");
    request.needs = (1..=6)
        .map(|case| format!("Where the restart guard handles replay case {case}"))
        .collect();
    request.anchors = vec!["restart_guard".into(), "stale replay".into()];
    request.deliverable = Some(v3::Deliverable::CodeChange);
    assert_eq!(request.budget.max_bytes, v3::DEFAULT_RETENTION_BYTES);
    let (outcome, _) = run_ruled(&fixture()?, request, rules("all"), &sized)?;
    let response = &outcome.response;
    for (stage, reads, bytes, lookups) in retained_by_stage(&outcome) {
        println!("{stage:?}: {reads} reads, {bytes} bytes retained, {lookups} lookup bytes");
    }
    println!(
        "work: calls {}, candidates {}, bytes {}; stop {:?}",
        response.work.calls, response.work.candidates, response.work.bytes, response.parts[0].stop
    );
    assert_ne!(response.parts[0].stop, v3::PartStop::Budget);
    assert!(response.work.bytes <= v3::DEFAULT_RETENTION_BYTES);
    let report = response.gathering.as_ref().context("report")?;
    for kind in [
        v3::GatheringStageKind::Guidance,
        v3::GatheringStageKind::Cited,
        v3::GatheringStageKind::Structure,
    ] {
        let reads: usize = report
            .stages
            .iter()
            .filter(|stage| stage.stage == kind)
            .flat_map(|stage| &stage.reads)
            .map(|reads| reads.count)
            .sum();
        assert!(reads > 0, "{kind:?} made no read: {report:?}");
    }
    Ok(())
}

#[test]
fn the_rules_keep_working_with_focus_enabled() -> Result<()> {
    let (fixture, focused) = super::focused(false)?;
    let (outcome, _) = run_ruled(
        &fixture,
        asked(Some(v3::Deliverable::CodeChange)),
        rules("all"),
        &same,
    )?;
    assert!(focused.batches.load(Ordering::Acquire) > 0);
    assert!(outcome.response.model_work.focus_pairs > 0);
    let report = outcome.response.gathering.as_ref().context("report")?;
    let coverage = report
        .stages
        .iter()
        .find(|stage| stage.stage == v3::GatheringStageKind::Coverage)
        .context("coverage stage")?;
    assert_eq!(
        report
            .stages
            .iter()
            .map(|stage| stage.model_judgments)
            .sum::<usize>(),
        outcome.response.model_work.focus_pairs
    );
    assert!(coverage.model_judgments > 0);
    Ok(())
}
