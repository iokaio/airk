// SPDX-License-Identifier: Apache-2.0
//! Bundled, bounded usage guidance. No filesystem, process execution or collection access.
use crate::response::ResponseArgs;
use rmcp::{
    model::{CallToolResult, ContentBlock},
    schemars,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::OnceLock};

mod topics;
type CliCatalog = BTreeMap<String, String>;
static CLI_FACTORY: OnceLock<fn() -> CliCatalog> = OnceLock::new();
static CLI: OnceLock<CliCatalog> = OnceLock::new();

/// The native executable supplies help generated from its own parser, without executing commands.
pub fn install_cli_help(factory: fn() -> CliCatalog) {
    let _ = CLI_FACTORY.set(factory);
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HelpArgs {
    /// Omit for overview and the topic catalog; use topics to list available guidance.
    pub topic: Option<String>,
    /// Exact CLI command path, e.g. runtime retire-previous, tools call or usage report. Never executed.
    pub command: Option<String>,
    /// Known MCP tool name or its single airk_ compatibility alias, e.g. search or airk_search; returns the selected catalog's current description and input schema. Exact catalog names take precedence.
    pub tool: Option<String>,
    /// 1-based continuation line in the selected help content.
    #[serde(default = "first_line")]
    pub start_line: usize,
    /// Complete JSON payload budget, 4096..32768 bytes; default 8192.
    #[serde(default = "default_budget")]
    pub max_bytes: usize,
    #[serde(flatten)]
    pub response: ResponseArgs,
}
fn first_line() -> usize {
    1
}
fn default_budget() -> usize {
    8192
}

#[derive(Serialize)]
pub(super) struct Topic {
    pub id: &'static str,
    pub title: &'static str,
    #[serde(skip)]
    pub text: &'static str,
}

fn answer(args: HelpArgs) -> Result<Value, &'static str> {
    answer_with_catalog(args, None)
}
fn answer_with_catalog(
    mut args: HelpArgs,
    tools: Option<Vec<rmcp::model::Tool>>,
) -> Result<Value, &'static str> {
    if !(4096..=32768).contains(&args.max_bytes) || args.start_line == 0 {
        return Err("invalid_help_budget_or_start_line");
    }
    if args.topic.as_ref().is_some_and(|topic| topic.len() > 80)
        || args
            .command
            .as_ref()
            .is_some_and(|command| command.len() > 160)
        || args.tool.as_ref().is_some_and(|tool| tool.len() > 128)
        || args.command.is_some() && args.tool.is_some()
    {
        return Err("invalid_help_selector");
    }
    let topic = args
        .topic
        .as_deref()
        .unwrap_or(if args.command.is_some() {
            "cli"
        } else if args.tool.is_some() {
            "tools"
        } else {
            "overview"
        })
        .to_ascii_lowercase()
        .replace('_', "-");
    let selected = topics::TOPICS
        .iter()
        .find(|entry| entry.id == topic)
        .ok_or("unknown_help_topic")?;
    let catalog = CLI_FACTORY.get().map(|factory| CLI.get_or_init(*factory));
    let content = if let Some(command) = &args.command {
        if args.topic.is_some() && !matches!(topic.as_str(), "cli" | "commands") {
            return Err("conflicting_help_selectors");
        }
        let words: Vec<_> = command.split_whitespace().collect();
        if words.iter().any(|word| {
            !word
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        }) {
            return Err("invalid_cli_command_path");
        }
        let name = words.strip_prefix(&["airk"]).unwrap_or(&words).join(" ");
        catalog
            .ok_or("cli_help_not_supplied_by_native_host")?
            .get(&name)
            .ok_or("unknown_cli_command")?
            .clone()
    } else if let Some(tool) = &args.tool {
        if args.topic.is_some() && !matches!(topic.as_str(), "tools" | "mcp") {
            return Err("conflicting_help_selectors");
        }
        let tools = tools.unwrap_or_else(crate::local::tools);
        let tool = tools
            .iter()
            .find(|entry| entry.name == *tool)
            .or_else(|| {
                if let Some(public) = tool.strip_prefix("airk_") {
                    // One known compatibility prefix only; never recursively
                    // strip a malformed or unknown tool name.
                    (!public.starts_with("airk_"))
                        .then(|| tools.iter().find(|entry| entry.name == public))
                        .flatten()
                } else {
                    tools
                        .iter()
                        .find(|entry| entry.name.strip_prefix("airk_") == Some(tool.as_str()))
                }
            })
            .ok_or("unknown_mcp_tool")?;
        let content =
            serde_json::to_string_pretty(&tool).map_err(|_| "help_serialization_failed")?;
        // Disclose the resolved catalog identity, so compatibility spellings
        // produce the same complete native help document.
        args.tool = Some(tool.name.to_string());
        content
    } else if topic == "commands" {
        let names = catalog
            .map(|commands| {
                commands
                    .keys()
                    .map(|name| {
                        if name.is_empty() {
                            "airk".into()
                        } else {
                            format!("airk {name}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        format!("{}\n\n{name}", selected.text, name = names)
    } else {
        selected.text.into()
    };
    let lines: Vec<_> = content.lines().collect();
    if args.start_line > lines.len().max(1) {
        return Err("help_start_line_out_of_range");
    }
    let show_topics =
        args.topic.is_none() && args.command.is_none() && args.tool.is_none() || topic == "topics";
    let mut result = json!({"schema":1,"version":env!("CARGO_PKG_VERSION"),"source":"bundled_usage_guidance",
        "topic":selected.id,"title":selected.title,"command":args.command,"tool":args.tool,
        "cli_catalog_available":catalog.is_some(),"content":"","start_line":args.start_line,
        "end_line":args.start_line-1,"total_lines":lines.len(),"next_line":args.start_line,"truncated":true,
        "topics":if show_topics { serde_json::to_value(topics::TOPICS).map_err(|_| "help_serialization_failed")? } else {json!([])}});
    let mut text = String::new();
    let mut end = args.start_line - 1;
    for line in lines.iter().skip(end) {
        let previous = text.len();
        if end >= args.start_line {
            text.push('\n');
        }
        text.push_str(line);
        result["end_line"] = json!(end + 1);
        result["next_line"] = if end + 1 < lines.len() {
            json!(end + 2)
        } else {
            Value::Null
        };
        result["truncated"] = json!(end + 1 < lines.len());
        if candidate_bytes(&result, &text).map_err(|_| "help_serialization_failed")?
            > args.max_bytes
        {
            text.truncate(previous);
            result["end_line"] = json!(end);
            result["next_line"] = json!(end + 1);
            result["truncated"] = json!(true);
            break;
        }
        end += 1;
    }
    if end < args.start_line {
        return Err("help_line_exceeds_budget");
    }
    result["content"] = Value::String(text);
    Ok(result)
}

/// Serialize the exact prospective object without cloning content or retaining wire bytes.
fn candidate_bytes(base: &Value, content: &str) -> Result<usize, serde_json::Error> {
    struct Candidate<'a> {
        base: &'a Value,
        content: &'a str,
    }
    impl Serialize for Candidate<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeMap;
            let fields = self.base.as_object().expect("help reply object");
            let mut map = serializer.serialize_map(Some(fields.len()))?;
            for (key, value) in fields {
                if key == "content" {
                    map.serialize_entry(key, self.content)?;
                } else {
                    map.serialize_entry(key, value)?;
                }
            }
            map.end()
        }
    }
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("help size overflow"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, &Candidate { base, content })?;
    Ok(counter.0)
}

pub fn reply(args: HelpArgs) -> CallToolResult {
    match answer(args) {
        Ok(value) => CallToolResult::success(vec![ContentBlock::text(value.to_string())]),
        Err(code) => error(code),
    }
}
fn error(code: &str) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(json!({"schema":1,"version":env!("CARGO_PKG_VERSION"),
        "error":{"code":code},"guidance":"Call airk_help with no arguments for the topic catalog; use airk tools list and airk --help for native command discovery."}).to_string())])
}
/// `detail` belongs to Explorer guidance, not native topic/CLI/tool selectors.
/// This validation is shared by public unified dispatch and native CLI help.
pub(crate) fn validate_detail_selector(
    arguments: &serde_json::Map<String, Value>,
) -> anyhow::Result<()> {
    if arguments.contains_key("detail")
        && !arguments.contains_key("query")
        && !arguments.contains_key("topic_id")
    {
        return Err(airk_core::read_arguments::ReadArgumentError::new(
            "invalid_help_arguments", "detail",
            "detail is valid only with Explorer query or topic_id help; omit it for native topic, command or tool help, which already returns full bounded content",
        ).into());
    }
    Ok(())
}
fn detail_selector_refusal(arguments: &Value) -> Option<CallToolResult> {
    let arguments = arguments.as_object()?;
    validate_detail_selector(arguments)
        .err()
        .map(|error| crate::response::error_result(&error))
}
pub fn call(arguments: Value) -> CallToolResult {
    if let Some(refusal) = detail_selector_refusal(&arguments) {
        return refusal;
    }
    match serde_json::from_value(arguments) {
        Ok(args) => reply(args),
        Err(_) => error("invalid_help_arguments"),
    }
}

/// Public composition supplies its catalog while retaining the same byte/line contract.
pub fn call_with_catalog(arguments: Value, tools: Vec<rmcp::model::Tool>) -> CallToolResult {
    if let Some(refusal) = detail_selector_refusal(&arguments) {
        return refusal;
    }
    match serde_json::from_value(arguments) {
        Ok(args) => match answer_with_catalog(args, Some(tools)) {
            Ok(value) => CallToolResult::success(vec![ContentBlock::text(value.to_string())]),
            Err(code) => error(code),
        },
        Err(_) => error("invalid_help_arguments"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(value: Value) -> HelpArgs {
        serde_json::from_value(value).unwrap()
    }
    #[test]
    fn tool_aliases_resolve_known_catalog_entries_with_exact_match_precedence() {
        let native = crate::local::tools()
            .into_iter()
            .find(|t| t.name == "airk_search")
            .unwrap();
        let mut public = native.clone();
        public.name = "search".into();
        public.description = Some("Explicit public catalog entry".into());
        for catalog in [vec![native.clone()], vec![public.clone()]] {
            let exact = catalog[0].name.to_string();
            let expected = answer_with_catalog(
                args(json!({"tool":exact,"max_bytes":32768})),
                Some(catalog.clone()),
            )
            .unwrap();
            for name in ["search", "airk_search"] {
                assert_eq!(
                    answer_with_catalog(
                        args(json!({"tool":name,"max_bytes":32768})),
                        Some(catalog.clone())
                    )
                    .unwrap(),
                    expected
                );
            }
            for name in ["airk_airk_search", "unknown", "airk_unknown"] {
                assert_eq!(
                    answer_with_catalog(args(json!({"tool":name})), Some(catalog.clone()))
                        .unwrap_err(),
                    "unknown_mcp_tool"
                );
            }
        }
        let catalog = vec![native.clone(), public.clone()];
        for selected in [native, public] {
            let result = answer_with_catalog(
                args(json!({"tool":selected.name,"max_bytes":32768})),
                Some(catalog.clone()),
            )
            .unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(result["content"].as_str().unwrap()).unwrap(),
                serde_json::to_value(selected).unwrap()
            );
        }
    }
    #[test]
    fn supplied_tool_catalog_remains_request_local_and_unknown_names_still_fail() {
        let mut supplied = crate::local::tools().remove(0);
        supplied.name = "request_local_fixture".into();
        supplied.description = Some("Original caller-specific description".into());
        std::sync::Arc::make_mut(&mut supplied.input_schema)
            .insert("fixture_root".into(), json!("fictional-root"));
        let expected = serde_json::to_string_pretty(&supplied).unwrap();
        let result = answer_with_catalog(
            args(json!({"tool":"request_local_fixture","max_bytes":32768})),
            Some(vec![supplied]),
        )
        .unwrap();
        assert_eq!(result["content"], expected);
        assert_eq!(
            answer(args(json!({"tool":"request_local_fixture"}))).unwrap_err(),
            "unknown_mcp_tool"
        );
    }

    #[test]
    fn candidate_size_matches_complete_reply_serialization_at_escape_and_digit_boundaries() {
        let texts = [
            "",
            "plain",
            r#"quotes" and slash\"#,
            "é中😀",
            "line\nnext\tcolumn",
        ];
        for end in [1usize, 9, 10, 99, 100, 999, 1000] {
            for next in [Value::Null, json!(end + 1)] {
                let base = json!({"content":"", "end_line":end, "next_line":next,
                    "truncated":!next.is_null(), "title":"é中", "start_line":1,
                    "command":null,"tool":"airk_status","topics":[{"id":"fixture"}]});
                for text in texts {
                    let mut actual = base.clone();
                    actual["content"] = Value::String(text.into());
                    assert_eq!(
                        candidate_bytes(&base, text).unwrap(),
                        serde_json::to_vec(&actual).unwrap().len()
                    );
                    assert_eq!(
                        base["content"], "",
                        "sizing cannot mutate the candidate template"
                    );
                }
                let controls: String = (0u8..=31).map(char::from).collect();
                let mut actual = base.clone();
                actual["content"] = Value::String(controls.clone());
                assert_eq!(
                    candidate_bytes(&base, &controls).unwrap(),
                    serde_json::to_vec(&actual).unwrap().len()
                );
            }
        }
    }

    #[test]
    fn every_topic_is_bounded_and_can_be_read_without_a_collection() {
        assert!(topics::TOPICS.len() >= 25);
        for topic in topics::TOPICS {
            let value = answer(args(json!({"topic":topic.id,"max_bytes":4096}))).unwrap();
            assert!(serde_json::to_vec(&value).unwrap().len() <= 4096);
            assert!(!value["content"].as_str().unwrap().is_empty());
        }
        assert!(
            answer(args(json!({}))).unwrap()["topics"]
                .as_array()
                .unwrap()
                .len()
                >= 25
        );
    }
    #[test]
    fn role_and_purpose_guidance_distinguishes_evidence_from_execution() {
        let roles = answer(args(json!({"topic":"roles","max_bytes":32768}))).unwrap();
        let text = roles["content"].as_str().unwrap();
        for role in ["Developer", "Tester", "Architect", "Writer"] {
            assert!(text.contains(role));
        }
        let choices = answer(args(json!({"topic":"indexing-choices","max_bytes":32768}))).unwrap();
        let text = choices["content"].as_str().unwrap();
        for choice in [
            "essential",
            "agent_lean",
            "balanced",
            "detailed",
            "independent",
            "OCR",
        ] {
            assert!(text.contains(choice));
        }
        let explorer = answer(args(json!({"topic":"explorer","max_bytes":32768}))).unwrap();
        let text = explorer["content"].as_str().unwrap();
        assert!(text.contains("one airk MCP connection with 28 unprefixed tools"));
        assert!(text.contains("offline"));
        assert!(text.contains("Suggestions wait for the user; applied never means executed"));
    }
    #[test]
    fn tool_help_uses_live_schema_and_rejects_unknown_or_unsafe_selectors() {
        let value = answer(args(json!({"tool":"airk_status"}))).unwrap();
        assert!(
            value["content"]
                .as_str()
                .unwrap()
                .contains("Quick AIRK version, basic health and project-index check")
        );
        for value in [
            json!({"topic":"made-up"}),
            json!({"command":"runtime; whoami"}),
            json!({"tool":"made-up"}),
            json!({"tool":"airk_status","command":"status"}),
            json!({"start_line":0}),
            json!({"max_bytes":32769}),
        ] {
            assert!(answer(args(value)).is_err());
        }
        assert!(call(json!({"unknown":true})).is_error.unwrap_or(false));
    }

    #[test]
    fn long_tool_help_continues_without_losing_or_repeating_lines() {
        let tool = crate::local::tools()
            .into_iter()
            .find(|tool| tool.name == "airk_code_facts")
            .unwrap();
        let expected = serde_json::to_string_pretty(&tool).unwrap();
        let mut line = 1;
        let mut pages = Vec::new();
        loop {
            let value = answer(args(
                json!({"tool":"airk_code_facts","max_bytes":4096,"start_line":line}),
            ))
            .unwrap();
            assert!(serde_json::to_vec(&value).unwrap().len() <= 4096);
            pages.push(value["content"].as_str().unwrap().to_owned());
            match value["next_line"].as_u64() {
                Some(next) => {
                    assert!(next as usize > line);
                    line = next as usize;
                }
                None => break,
            }
        }
        assert_eq!(pages.join("\n"), expected);
    }
}
