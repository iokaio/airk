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
    /// Exact MCP tool name, e.g. airk_search; returns its current description and input schema.
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
    args: HelpArgs,
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
        let tool = tools
            .unwrap_or_else(crate::local::tools)
            .into_iter()
            .find(|entry| entry.name == *tool)
            .ok_or("unknown_mcp_tool")?;
        serde_json::to_string_pretty(&tool).map_err(|_| "help_serialization_failed")?
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
        result["content"] = json!(text);
        result["end_line"] = json!(end + 1);
        result["next_line"] = if end + 1 < lines.len() {
            json!(end + 2)
        } else {
            Value::Null
        };
        result["truncated"] = json!(end + 1 < lines.len());
        if serde_json::to_vec(&result)
            .map_err(|_| "help_serialization_failed")?
            .len()
            > args.max_bytes
        {
            text.truncate(previous);
            result["content"] = json!(text);
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
    Ok(result)
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
pub fn call(arguments: Value) -> CallToolResult {
    match serde_json::from_value(arguments) {
        Ok(args) => reply(args),
        Err(_) => error("invalid_help_arguments"),
    }
}

/// Public composition supplies its catalog while retaining the same byte/line contract.
pub(crate) fn call_with_catalog(arguments: Value, tools: Vec<rmcp::model::Tool>) -> CallToolResult {
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
        assert!(text.contains("one airk MCP connection with 26 unprefixed tools"));
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
                .contains("runtime JSON bundle")
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
