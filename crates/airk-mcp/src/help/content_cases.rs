// SPDX-License-Identifier: Apache-2.0
//! Build-time export of the exact native documentation owners.
use super::*;

#[test]
fn help_content_manifest_matches_native_owners() {
    for args in [
        json!({}),
        json!({"topic":null}),
        json!({"topic":"overview"}),
    ] {
        assert!(serde_json::from_value::<HelpArgs>(args).is_ok());
    }
    assert!(serde_json::from_value::<HelpArgs>(json!({"topic":42})).is_err());
    let topics = topics::TOPICS
        .iter()
        .map(|topic| json!({"id":topic.id,"title":topic.title,"text":topic.text}))
        .collect::<Vec<_>>();
    let ids = topics::TOPICS
        .iter()
        .map(|topic| topic.id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        ids.len(),
        topics.len(),
        "topic identities must remain unique"
    );
    for required in [
        "overview",
        "quickstart",
        "cli",
        "commands",
        "mcp",
        "tools",
        "installation",
        "agent-setup",
        "usage",
        "workbench",
        "request-recovery",
        "privacy",
        "portable",
        "trust",
        "arbiter",
    ] {
        assert!(
            ids.contains(required),
            "missing applicable native guide {required}"
        );
    }
    let commands = embedded_cli_help();
    for required in [
        "",
        "service status",
        "service start",
        "agents connect",
        "tools call",
        "usage report",
    ] {
        assert!(
            commands.contains_key(required),
            "missing parser command {required}"
        );
    }
    let tools = crate::unified::tools();
    for required in [
        "help",
        "search",
        "read_file",
        "status",
        "diagnostics",
        "parse",
    ] {
        assert!(
            tools.iter().any(|tool| tool.name == required),
            "missing native tool {required}"
        );
    }
    let value = json!({"schema":"airk.help.native-content/1","version":env!("CARGO_PKG_VERSION"),
        "topics":topics,"cliCommands":commands,"helpRequestSchema":rmcp::schemars::schema_for!(HelpArgs),"tools":tools});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/help/content.json");
    if std::env::var_os("AIRK_WRITE_HELP_CONTENT").as_deref() == Some(std::ffi::OsStr::new("1")) {
        std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap() + "\n").unwrap();
    }
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        saved, value,
        "regenerate AIRK_WRITE_HELP_CONTENT=1 and review complete native Help content/schema/catalog"
    );
}

#[test]
fn current_help_tasks_have_complete_native_guides_and_continuations() {
    // Independently required tasks/error terms, not derived from the topic catalog.
    for (topic, required) in [
        (
            "installation",
            vec![
                "--silent",
                "Windows",
                "Linux",
                "macOS",
                "--acknowledge-reset",
                "exit 11",
            ],
        ),
        (
            "portable",
            vec![
                "service setup --portable --program",
                "--data",
                "--state",
                "--cache",
                "--logs",
                "without adding PATH",
            ],
        ),
        (
            "trust",
            vec!["HTTPS", "single-use", "TLS", "Silent installers skip trust"],
        ),
        (
            "agent-setup",
            vec![
                "agent_default_scope_unset",
                "agent_scope_choice_disabled",
                "off by default",
            ],
        ),
        (
            "request-recovery",
            vec!["unknown outcome", "original", "identity"],
        ),
        (
            "arbiter",
            vec![
                "off by default",
                "immutable-index",
                "MiniLM",
                "excluded indexes",
                "gaps",
                "independent",
            ],
        ),
        (
            "usage",
            vec!["1..30 days", "31-day request refuses", "decimal strings"],
        ),
        (
            "index-health",
            vec![
                "not readable: no index set",
                "memberships",
                "Publication identity",
            ],
        ),
    ] {
        let guide = topics::TOPICS
            .iter()
            .find(|entry| entry.id == topic)
            .unwrap();
        for term in required {
            assert!(
                guide.text.contains(term),
                "{topic} lost applicable task term {term}"
            );
        }
        let mut line = 1;
        let mut loaded = Vec::new();
        loop {
            let args: HelpArgs =
                serde_json::from_value(json!({"topic":topic,"start_line":line,"max_bytes":4096}))
                    .unwrap();
            let page = answer(args).unwrap();
            assert!(serde_json::to_vec(&page).unwrap().len() <= 4096);
            assert_eq!(page["start_line"], line);
            loaded.push(page["content"].as_str().unwrap().to_owned());
            let Some(next) = page["next_line"].as_u64() else {
                break;
            };
            assert!(next > line as u64, "continuation must advance");
            line = usize::try_from(next).unwrap();
        }
        assert_eq!(
            loaded.join("\n"),
            guide.text,
            "continuation lost or replaced {topic} text"
        );
    }
}
