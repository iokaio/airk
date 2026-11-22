// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn layers_parse_all_none_lists_and_removals() -> Result<()> {
    assert_eq!(Layers::parse("all")?, Layers::all());
    assert_eq!(Layers::parse("none")?, Layers::default());
    let without_focus = Layers::parse("all,-focus")?;
    assert!(!without_focus.focus && without_focus.followups && without_focus.device);
    let gathering = Layers::parse("followups,spread,pool,present,device")?;
    assert_eq!(
        gathering.names(),
        vec!["followups", "spread", "pool", "present", "device"]
    );
    assert!(!Layers::default().any());
    for refused in ["", "all,all", "focus,unknown", "-unknown", "all,"] {
        assert!(Layers::parse(refused).is_err(), "{refused:?}");
    }
    Ok(())
}

#[test]
fn identifiers_keep_code_names_in_first_seen_order_and_drop_english() {
    let found = identifiers(
        "Make `Gathering::observe` keep the queue when a FollowUp read fails; see plan_followups, \
         airk_core::arbiter::Budget and the README. Update GitHub docs for Gathering.",
        8,
    );
    assert_eq!(
        found,
        vec!["Gathering", "observe", "FollowUp", "plan_followups", "Budget"]
    );
    assert_eq!(identifiers("Rename `x` and `do it`", 4), Vec::<String>::new());
    assert_eq!(identifiers("snake_case CamelCase other_name", 2).len(), 2);
    assert!(identifiers("Where is the constraint defined?", 4).is_empty());
}

#[test]
fn declared_names_come_from_definition_lines() {
    let text = "pub struct Gathering {\n    queue: VecDeque<Dispatch>,\n}\nimpl Gathering {\n    pub fn plan_followups(&mut self) {}\n}\npublic class ChapterService : IChapterService\n";
    assert_eq!(
        declared_names(text, 8),
        vec!["Gathering", "plan_followups", "ChapterService"]
    );
}

#[test]
fn test_and_document_paths_cover_rust_typescript_csharp_and_python() {
    for test in [
        "crates/airk-kernel/src/arbiter/tests.rs",
        "crates/airk-kernel/tests/answer.rs",
        "crates/x/src/lib_test.rs",
        "web/src/store/chapter.spec.ts",
        "web/src/__tests__/view.tsx",
        "src/App.test.tsx",
        "WritingForge.Tests/ChapterServiceTests.cs",
        "tests/test_parser.py",
    ] {
        assert!(is_test_path(test), "{test}");
    }
    for code in [
        "crates/airk-kernel/src/arbiter.rs",
        "web/src/store/chapter.ts",
        "src/Services/ChapterService.cs",
        "src/contest.rs",
        "docs/testing.md",
    ] {
        assert!(!is_test_path(code), "{code}");
    }
    assert!(is_document_path("docs/arbiter/final-plan.md"));
    assert!(is_document_path("crates/airk-kernel/src/arbiter/CONTRACT.md"));
    assert!(!is_document_path("crates/airk-kernel/src/arbiter.rs"));
    assert!(is_governing_path("crates/x/CONTRACT.md"));
    assert!(!is_governing_path("crates/x/notes.md"));
}

#[test]
fn governing_candidates_are_nearest_first_and_exclude_the_root() {
    assert_eq!(
        governing_candidates("crates/airk-kernel/src/arbiter.rs"),
        vec![
            "crates/airk-kernel/src/arbiter/CONTRACT.md",
            "crates/airk-kernel/src/arbiter/README.md",
            "crates/airk-kernel/src/CONTRACT.md",
            "crates/airk-kernel/src/README.md",
            "crates/airk-kernel/CONTRACT.md",
            "crates/airk-kernel/README.md",
            "crates/CONTRACT.md",
            "crates/README.md",
        ]
    );
    assert_eq!(
        governing_candidates("src/Services/ChapterService.cs"),
        vec![
            "src/Services/CONTRACT.md",
            "src/Services/README.md",
            "src/CONTRACT.md",
            "src/README.md",
        ]
    );
    assert!(governing_candidates("README.md").is_empty());
    assert!(!governing_candidates("docs/README.md").contains(&"docs/README.md".to_owned()));
}

#[test]
fn targets_state_the_read_in_words_and_end_with_the_anchor_range() {
    let text = target(
        FollowUp::Definition,
        "Gathering",
        "crates/airk-kernel/src/arbiter.rs",
        491,
        504,
    );
    assert_eq!(
        text,
        "Find the definition of Gathering (symbols) named at crates/airk-kernel/src/arbiter.rs:491-504"
    );
    let (_, range) = text.rsplit_once(':').unwrap();
    assert_eq!(range, "491-504");
}

#[test]
fn per_read_split_stays_inside_the_frame_plan() {
    assert_eq!(per_read_bytes(0), FRAME_PLAN / 2 / (KEPT + 4));
    assert_eq!(per_read_bytes(26_000), 1536);
    assert!(per_read_bytes(4096) * (KEPT + 4) + 4096 <= FRAME_PLAN / 2 + 1536);
}

#[test]
fn initial_search_uses_the_pool_with_focused_snippets() {
    let budget = Budget::default();
    let search = initial_search("Where is Gathering defined?", &budget);
    assert_eq!(search["limit"], 16);
    assert_eq!(search["max_bytes"], 12_288);
    assert_eq!(search["full_chunks"], false);
    let small = Budget {
        candidates: 6,
        ..Budget::default()
    };
    let search = initial_search("q", &small);
    assert_eq!(search["limit"], 6);
    assert_eq!(search["max_bytes"], 4608);
    assert_eq!(search["full_chunks"], true);
}

#[test]
fn focus_window_is_bounded_char_aligned_and_starts_at_a_named_line() {
    let text = format!("{}fn plan_followups() {{}}\n{}", "x\n".repeat(900), "é".repeat(900));
    let (start, end) = focus_window(&text, "Change plan_followups", 512);
    assert!(text.is_char_boundary(start) && text.is_char_boundary(end));
    assert!(end - start <= 1024 - 42 - 320 && end > start);
    assert!(text[start..end].starts_with("fn plan_followups"));
    let short = "short hit";
    assert_eq!(focus_window(short, "anything", 512), (0, short.len()));
    let (start, end) = focus_window(&"é".repeat(2000), "q", 512);
    assert_eq!(start, 0);
    assert!(end <= 702 && end % 2 == 0);
}

#[test]
fn follow_up_choice_ids_depend_on_kind_revision_tool_and_arguments() -> Result<()> {
    let arguments = json!({"name":"Gathering"}).as_object().unwrap().clone();
    let base = choice_id(FollowUp::Definition, "r1", "symbols", &arguments)?;
    assert_eq!(base, choice_id(FollowUp::Definition, "r1", "symbols", &arguments)?);
    assert_ne!(base, choice_id(FollowUp::References, "r1", "symbols", &arguments)?);
    assert_ne!(base, choice_id(FollowUp::Definition, "r2", "symbols", &arguments)?);
    let other = json!({"name":"Other"}).as_object().unwrap().clone();
    assert_ne!(base, choice_id(FollowUp::Definition, "r1", "symbols", &other)?);
    assert!(FollowUp::Literal.keep_rank() < FollowUp::Governing.keep_rank());
    assert!(FollowUp::Outline.keep_rank() < FollowUp::WholeChunk.keep_rank());
    Ok(())
}

/// Change set 2 switches off: `all` is change set 1's layers, serialized to the
/// same `change-set.json` bytes with the same names, and every new value falls
/// back to change set 1's constant.
#[test]
fn change_set_2_switches_off_leave_change_set_1_layers_byte_identical() -> Result<()> {
    let cs1 = json!({"followups":true,"spread":true,"pool":true,"focus":true,"present":true,"device":true});
    for layers in [Layers::all(), Layers::parse("all")?] {
        // Exactly the bytes change set 1 wrote into `change-set.json`.
        assert_eq!(
            serde_json::to_string(&layers)?,
            r#"{"followups":true,"spread":true,"pool":true,"focus":true,"present":true,"device":true}"#
        );
        assert_eq!(layers.names(), Layers::NAMES.to_vec());
        assert_eq!(layers.kept(), KEPT);
        assert_eq!(layers.keep_threshold().to_bits(), FOCUS_KEEP.to_bits());
        assert!(!layers.context_b);
    }
    assert_eq!(
        serde_json::to_string(&Layers::default())?,
        r#"{"followups":false,"spread":false,"pool":false,"focus":false,"present":false,"device":false}"#
    );
    let read_back: Layers = serde_json::from_value(cs1)?;
    assert_eq!(read_back, Layers::all());
    for initial in [0, 4096, 12_288, 26_000] {
        assert_eq!(per_read_bytes_for(initial, KEPT), per_read_bytes(initial));
    }
    let id = |byte| Sha256Digest::from_bytes([byte; 32]);
    let order: Vec<_> = (1..=6).map(id).collect();
    let scores = std::collections::BTreeMap::from([(id(1), 0.05), (id(2), 0.5), (id(3), 0.2), (id(4), 0.1)]);
    assert_eq!(
        plan::select_kept_with(&order, &scores, KEPT, FOCUS_KEEP),
        plan::select_kept(&order, &scores)
    );
    Ok(())
}

#[test]
fn change_set_2_switches_parse_name_and_tune_without_rebuilds() -> Result<()> {
    let layers = Layers::parse("all,context_b,focus_keep=6,focus_threshold=0.25")?;
    assert!(layers.context_b && layers.followups);
    assert_eq!(layers.kept(), 6);
    assert_eq!(layers.keep_threshold(), 0.25);
    assert_eq!(
        layers.names(),
        ["followups", "spread", "pool", "focus", "present", "device", "context_b", "focus_keep=6", "focus_threshold=0.250"]
    );
    let json = serde_json::to_value(layers)?;
    assert_eq!(json["context_b"], true);
    assert_eq!(json["focus_keep"], 6);
    assert_eq!(json["focus_threshold_milli"], 250);
    assert_eq!(serde_json::from_value::<Layers>(json)?, layers);
    assert_eq!(Layers::parse("focus_threshold=0")?.names(), ["focus_threshold=0.000"]);
    assert_eq!(Layers::parse("focus_threshold=1")?.keep_threshold(), 1.0);
    // `none` and `all` reset change set 2 values too; later tokens set them.
    assert_eq!(Layers::parse("context_b,focus_keep=3,none")?, Layers::default());
    assert_eq!(Layers::parse("focus_keep=3,all")?, Layers::all());
    assert_eq!(Layers::parse("all,focus_keep=3")?.kept(), 3);
    for refused in [
        "focus_keep=0", "focus_keep=17", "focus_keep=x", "focus_keep=",
        "focus_threshold=1.5", "focus_threshold=-0.1", "focus_threshold=0.1234",
        "focus_threshold=nan", "-focus_keep=3", "context_b,context_b",
        // A tuned value is given once (`kept=` is the same value).
        "focus_keep=3,focus_keep=4", "focus_keep=3,kept=4", "kept=3,none,focus_keep=4",
        "focus_threshold=0.1,focus_threshold=0.2",
    ] {
        assert!(Layers::parse(refused).is_err(), "{refused:?}");
    }
    // The tuned values drive kept selection and the per-read split.
    let id = |byte| Sha256Digest::from_bytes([byte; 32]);
    let order: Vec<_> = (1..=6).map(id).collect();
    let scores = std::collections::BTreeMap::from([(id(1), 0.05), (id(2), 0.5), (id(3), 0.2), (id(4), 0.3)]);
    assert_eq!(plan::select_kept_with(&order, &scores, 2, 0.25), vec![1, 3]);
    assert_eq!(plan::select_kept_with(&order, &scores, 6, 0.0), vec![0, 1, 2, 3, 4, 5]);
    assert!(per_read_bytes_for(0, 8) < per_read_bytes_for(0, KEPT));
    Ok(())
}
/// Replay tuning tokens: every omitted or "change set 1" value leaves `all`
/// byte-identical to change set 1's `change-set.json` layers and behaviour.
#[test]
fn tuning_token_defaults_leave_change_set_1_byte_identical() -> Result<()> {
    let cs1 = r#"{"followups":true,"spread":true,"pool":true,"focus":true,"present":true,"device":true}"#;
    for text in ["all", "all,subjects=loose,governing=any", "subjects=strict,governing=project,all"] {
        let layers = Layers::parse(text)?;
        assert_eq!(layers, Layers::all(), "{text}");
        assert_eq!(serde_json::to_string(&layers)?, cs1, "{text}");
        assert_eq!(layers.names(), Layers::NAMES.to_vec());
        assert_eq!(layers.pool_size(), POOL);
        assert_eq!(layers.read_floor(), READ_FLOOR);
        for budget in [Budget::default(), Budget { candidates: 6, ..Budget::default() }] {
            assert_eq!(
                initial_search_for("q", &budget, layers),
                initial_search_for("q", &budget, Layers::all())
            );
        }
    }
    // The constants change set 1 shipped, and its pool arguments.
    assert_eq!((POOL, KEPT, READ_FLOOR, WHOLE_CHUNK_LINES), (16, 4, 1536, 40));
    assert_eq!(
        initial_search_for("q", &Budget::default(), Layers::all()),
        json!({"query":"q","limit":16,"max_bytes":12_288,"full_chunks":false})
    );
    for initial in [0, 4096, 12_288, 26_000, 60_000] {
        assert_eq!(per_read_bytes_with(initial, KEPT, READ_FLOOR), per_read_bytes(initial));
    }
    let read_back: Layers = serde_json::from_str(cs1)?;
    assert_eq!(read_back, Layers::all());
    Ok(())
}

#[test]
fn tuning_tokens_parse_name_round_trip_and_refuse_out_of_range() -> Result<()> {
    let text = "all,pool=8,pool_full=8,read_floor=3072,kept=3,subjects=strict,governing=project";
    let layers = Layers::parse(text)?;
    assert_eq!(
        (layers.pool_size, layers.pool_full, layers.read_floor, layers.focus_keep),
        (Some(8), Some(8), Some(3072), Some(3))
    );
    assert!(layers.subjects_strict && layers.governing_project && layers.followups);
    assert_eq!(layers.kept(), 3);
    assert_eq!(
        layers.names(),
        [
            "followups", "spread", "pool", "focus", "present", "device", "focus_keep=3",
            "pool=8", "pool_full=8", "read_floor=3072", "subjects=strict", "governing=project",
        ]
    );
    let json = serde_json::to_value(layers)?;
    assert_eq!(json["pool_size"], 8);
    assert_eq!(json["pool_full"], 8);
    assert_eq!(json["read_floor"], 3072);
    assert_eq!(json["subjects_strict"], true);
    assert_eq!(json["governing_project"], true);
    assert_eq!(serde_json::from_value::<Layers>(json)?, layers);
    // The names re-parse to the same layers; `kept` is `focus_keep`.
    assert_eq!(Layers::parse(&layers.names().join(","))?, layers);
    assert_eq!(Layers::parse("kept=5")?, Layers::parse("focus_keep=5")?);
    assert_eq!(Layers::parse("pool_full=0")?.pool_full, Some(0));
    assert!(!Layers::parse("subjects=strict,subjects=loose")?.subjects_strict);
    assert!(!Layers::parse("governing=project,governing=any")?.governing_project);
    for refused in [
        // Change set 3 raised the pool cap from 16 to MAX_POOL (48).
        "pool=0", "pool=49", "pool=", "pool=x", "pool_full=49", "pool_full=-1",
        "read_floor=511", "read_floor=8193", "read_floor=", "kept=0", "kept=17",
        "subjects=", "subjects=other", "governing=root", "-pool=8", "pool=8,pool=8",
    ] {
        assert!(Layers::parse(refused).is_err(), "{refused:?}");
    }
    Ok(())
}

#[test]
fn pool_and_pool_full_shape_the_first_search() {
    let budget = Budget::default();
    let search = |text: &str| initial_search_for("q", &budget, Layers::parse(text).unwrap());
    assert_eq!(
        search("all,pool=8"),
        json!({"query":"q","limit":8,"max_bytes":6144,"full_chunks":true})
    );
    assert_eq!(search("all,pool=8,pool_full=0")["full_chunks"], false);
    assert_eq!(search("all,pool=8,pool_full=8")["full_chunks"], true);
    // A pool_full below the pool keeps snippets; the top hits come from one
    // more search planned after the first reply.
    assert_eq!(search("all,pool_full=8")["full_chunks"], false);
    assert_eq!(search("all,pool_full=16")["full_chunks"], true);
    assert_eq!(search("all,pool=12")["limit"], 12);
    let small = Budget { candidates: 6, ..Budget::default() };
    let layers = Layers::parse("all,pool=12,pool_full=6").unwrap();
    assert_eq!(initial_search_for("q", &small, layers)["limit"], 6);
    assert_eq!(initial_search_for("q", &small, layers)["full_chunks"], true);
}

#[test]
fn read_floor_raises_only_the_lower_clamp() {
    assert_eq!(per_read_bytes_with(26_000, KEPT, 3072), 3072);
    assert_eq!(per_read_bytes_with(26_000, KEPT, READ_FLOOR), 1536);
    assert_eq!(per_read_bytes_with(0, KEPT, 3072), per_read_bytes(0));
    assert_eq!(per_read_bytes_with(0, 1, 8192), 8192);
    assert_eq!(Layers::parse("read_floor=3072").unwrap().read_floor(), 3072);
}

#[test]
fn strict_subjects_are_snake_case_or_multi_word_camel_case() {
    for name in ["ChapterService", "IChapterService", "HTTPClient", "plan_followups", "FRAME_PLAN", "Utf8Reader"] {
        assert!(strict_subject(name), "{name}");
    }
    for name in ["Gathering", "Arbiter", "WRITING", "self_", "_x", "Answer"] {
        assert!(!strict_subject(name), "{name}");
    }
    assert!(names_identifier("fn plan_followups(x)", "plan_followups"));
    assert!(!names_identifier("fn plan_followups_more()", "plan_followups"));
    assert!(!names_identifier("MyChapterService", "ChapterService"));
    assert!(names_identifier("ChapterService", "ChapterService"));
}

#[test]
fn project_folders_follow_the_first_project_container() {
    assert_eq!(project_folder("crates/airk-kernel/src/arbiter.rs").as_deref(), Some("crates/airk-kernel"));
    assert_eq!(
        project_folder("pro/src/WritingForgePro.Services/Chapters/ChapterService.cs").as_deref(),
        Some("pro/src/WritingForgePro.Services")
    );
    assert_eq!(project_folder("docs/arbiter/plan.md").as_deref(), Some("docs"));
    assert_eq!(project_folder("src/lib.rs").as_deref(), Some("src"));
    assert_eq!(project_folder("README.md"), None);
    let path = "crates/airk-kernel/src/arbiter.rs";
    let kept: Vec<_> = governing_candidates(path)
        .into_iter()
        .filter(|candidate| in_project(path, candidate))
        .collect();
    assert_eq!(
        kept,
        [
            "crates/airk-kernel/src/arbiter/CONTRACT.md",
            "crates/airk-kernel/src/arbiter/README.md",
            "crates/airk-kernel/src/CONTRACT.md",
            "crates/airk-kernel/src/README.md",
            "crates/airk-kernel/CONTRACT.md",
            "crates/airk-kernel/README.md",
        ]
    );
    assert!(!in_project(path, "crates/airk-kernel-extra/README.md"));
}
