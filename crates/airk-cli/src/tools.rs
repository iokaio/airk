// SPDX-License-Identifier: Apache-2.0
use airk_mcp::local::tools;
use anyhow::{Context, Result, ensure};
use clap::Subcommand;
use serde_json::{Map, Value, json};
use std::{
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
};

const MAX_ARGUMENT_BYTES: usize = 64 * 1024;

trait ToolSession {
    async fn call(&self, tool: String, arguments: Map<String, Value>) -> Result<Value>;
}

struct RemoteSession(std::sync::Arc<dyn airk_mcp::remote::RemoteTools>);
impl ToolSession for RemoteSession {
    async fn call(&self, tool: String, mut arguments: Map<String, Value>) -> Result<Value> {
        arguments.entry("response_mode").or_insert_with(|| {
            serde_json::to_value(airk_mcp::response::ResponseMode::from_environment())
                .expect("scalar response mode")
        });
        airk_mcp::remote::call_observed(self.0.as_ref(), tool, arguments).await
    }
}

#[cfg(test)]
mod parity_tests {
    use super::*;
    use airk_mcp::remote::{RemoteFuture, RemoteTools};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    struct Backend {
        limit: AtomicUsize,
    }
    impl RemoteTools for Backend {
        fn query_concurrency(&self) -> usize {
            self.limit.load(Ordering::SeqCst)
        }
        fn call(&self, tool: String, arguments: Map<String, Value>) -> RemoteFuture<'_> {
            Box::pin(async move {
                if arguments.get("query") == Some(&json!("refuse")) {
                    return Ok(airk_mcp::unified::failure(
                        "fixture_retry",
                        "Preserve this failure",
                    ));
                }
                let result = json!({"publication":{"token":"fixture"},"native_field":{"complete":true},"arguments":arguments});
                if tool == "airk_parse" {
                    let bytes = serde_json::to_vec(&result)?.len();
                    let mut reply = airk_mcp::unified::success(
                        json!({"schema":"airk.workbench.result/1","collection":"fixture","publication":null,"status":"complete","executed":true,"plan":airk_core::workbench::Plan::default(),"requires_input":[],"steps":[{"id":"query","tool":"status","arguments":{},"status":"complete","elapsed_ms":1,"result":result,"error":null}],"evidence":[],"warnings":[],"truncated":false,"completeness":[],"calls":1,"payload_bytes":bytes}),
                    );
                    reply.as_object_mut().unwrap().remove("structuredContent");
                    return Ok(reply);
                }
                let mut reply = airk_mcp::unified::success(result);
                if matches!(tool.as_str(), "airk_status" | "airk_diagnostics") {
                    reply.as_object_mut().unwrap().remove("structuredContent");
                }
                Ok(reply)
            })
        }
    }

    #[tokio::test]
    async fn cli_tool_calls_keep_all_fields_and_share_mcp_capacity_observations() -> Result<()> {
        let backend = Arc::new(Backend {
            limit: AtomicUsize::new(16),
        });
        let session = RemoteSession(backend.clone());
        for mode in ["terse", "full"] {
            for limit in [4, 8, 16, 32, 64] {
                backend.limit.store(limit, Ordering::SeqCst);
                for tool in tools().into_iter().map(|tool| tool.name.to_string()) {
                    let arguments = json!({"response_mode":mode}).as_object().unwrap().clone();
                    let reply = session.call(tool.clone(), arguments.clone()).await?;
                    let body = &reply["structuredContent"];
                    assert_eq!(
                        serde_json::from_str::<Value>(
                            reply["content"][0]["text"].as_str().unwrap()
                        )?,
                        *body
                    );
                    let result = if tool == "airk_parse" {
                        &body["steps"][0]["result"]
                    } else {
                        body
                    };
                    assert_eq!(result["native_field"], json!({"complete":true}), "{tool}");
                    assert_eq!(result["arguments"], json!(arguments), "{tool}");
                    if matches!(
                        tool.as_str(),
                        "airk_parse" | "airk_status" | "airk_diagnostics"
                    ) {
                        assert_eq!(result["query_concurrency"], limit, "{tool}");
                    }
                }
            }
        }
        let error = session
            .call(
                "airk_status".into(),
                json!({"query":"refuse"}).as_object().unwrap().clone(),
            )
            .await?;
        assert_eq!(
            error,
            airk_mcp::unified::failure("fixture_retry", "Preserve this failure")
        );
        Ok(())
    }
}

#[derive(Subcommand)]
#[command(disable_help_subcommand = true)]
pub enum ToolsCommand {
    /// List the 18 native prefixed tools; --json includes schemas. Unified MCP adds projects and nine Explorer Assistance tools.
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show a tool's description, input schema, defaults and argument template.
    Help {
        tool: String,
        #[arg(long)]
        json: bool,
    },
    /// Invoke an MCP tool and print its unchanged result envelope as JSON.
    #[command(
        after_help = "Examples (add --root, --data-dir and --collection for your workspace):\n  airk tools call airk_status\n  airk tools call airk_search --args-file query.json\nUse 'airk tools help airk_search' for the JSON format. Tool errors exit nonzero."
    )]
    Call {
        tool: String,
        /// JSON object; omit for {}. Maximum 64 KiB.
        #[arg(long, conflicts_with = "args_file")]
        args: Option<String>,
        /// Read a UTF-8 JSON object from a regular file (maximum 64 KiB).
        #[arg(long)]
        args_file: Option<PathBuf>,
    },
    /// Open a collection-bound interactive console with help and examples.
    Shell,
}

fn print_json(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Metadata inspection works even before a workspace has been indexed.
pub fn inspect(action: &ToolsCommand) -> Result<bool> {
    match action {
        ToolsCommand::Call {
            tool,
            args,
            args_file,
        } if tool == "airk_help" => {
            let result = serde_json::to_value(airk_mcp::help::call(Value::Object(arguments(
                args.as_deref(),
                args_file.as_deref(),
            )?)))?;
            print_json(&result)?;
            ensure!(result["isError"] != true, "AIRK help request failed");
        }
        ToolsCommand::List { json: true } => print_json(&json!({"tools": tools()}))?,
        ToolsCommand::List { json: false } => {
            for tool in tools() {
                println!(
                    "{}\n  {}",
                    tool.name,
                    tool.description.as_deref().unwrap_or_default()
                );
            }
            println!("\nUse: airk tools help <tool>\nOr in the tools console: help <tool>");
        }
        ToolsCommand::Help {
            tool: name,
            json: raw,
        } => {
            let tool = tools()
                .into_iter()
                .find(|tool| tool.name == *name)
                .with_context(|| format!("Unknown MCP tool '{name}'. Use 'airk tools list'."))?;
            if *raw {
                print_json(&tool)?;
            } else {
                println!(
                    "{}\n{}\n\nInput schema (required arguments and defaults):",
                    tool.name,
                    tool.description.as_deref().unwrap_or_default()
                );
                print_json(&tool.input_schema)?;
                let schema = Value::Object((*tool.input_schema).clone());
                let mut template = Map::new();
                if let Some(properties) = schema["properties"].as_object() {
                    for (key, property) in properties {
                        if let Some(default) = property.get("default") {
                            template.insert(key.clone(), default.clone());
                        } else if schema["required"]
                            .as_array()
                            .is_some_and(|items| items.contains(&json!(key)))
                        {
                            let value = match property["type"].as_str() {
                                Some("integer" | "number") => json!(1),
                                Some("boolean") => json!(false),
                                Some("array") => json!([]),
                                Some("object") => json!({}),
                                _ => json!(format!("<{key}>")),
                            };
                            template.insert(key.clone(), value);
                        }
                    }
                }
                println!("\nArgument template (replace placeholders; follow schema constraints):");
                print_json(&template)?;
                println!(
                    "\nSave arguments as query.json, then run with your collection options:\n  airk tools call {name} --args-file query.json\nInside the tools console:\n  call {name} --args-file query.json\n  call {name} {}",
                    Value::Object(template)
                );
                if name == "airk_search" {
                    println!(
                        "\nExplicit vector search (requires a published model/index and discloses partial coverage):\n  call airk_search {{\"query\":\"how is a forgotten password recovered\",\"mode\":\"vector\",\"limit\":5}}\nOmit mode for the saved collection search policy."
                    );
                }
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn arguments(text: Option<&str>, file: Option<&Path>) -> Result<Map<String, Value>> {
    let bytes = if let Some(file) = file {
        ensure!(
            std::fs::metadata(file)?.is_file(),
            "Arguments must be a regular file"
        );
        let mut bytes = Vec::new();
        std::fs::File::open(file)?
            .take((MAX_ARGUMENT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        bytes
    } else {
        text.unwrap_or("{}").as_bytes().to_vec()
    };
    ensure!(
        bytes.len() <= MAX_ARGUMENT_BYTES,
        "MCP arguments exceed 64 KiB"
    );
    let value: Value = serde_json::from_slice(&bytes)
        .context("Expected a UTF-8 JSON object; use --args-file to avoid shell quoting")?;
    value
        .as_object()
        .cloned()
        .context("MCP arguments must be a JSON object")
}

async fn call(
    session: &impl ToolSession,
    tool: String,
    args: Map<String, Value>,
    raw: bool,
) -> Result<()> {
    let result = session.call(tool, args).await?;
    ensure!(
        result.is_object() && result["content"].is_array(),
        "invalid native tool response"
    );
    if raw {
        print_json(&result)?;
    } else {
        // Make JSON text content readable in a terminal; one-shot calls retain the wire envelope.
        let envelope = &result;
        println!(
            "{}",
            if result["isError"].as_bool().unwrap_or(false) {
                "MCP error:"
            } else {
                "Result:"
            }
        );
        for content in envelope["content"]
            .as_array()
            .context("Missing MCP content")?
        {
            if content["type"] == "text" {
                let text = content["text"].as_str().context("Missing MCP text")?;
                print_json(&serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!(text)))?;
            } else {
                print_json(content)?;
            }
        }
    }
    ensure!(
        !result["isError"].as_bool().unwrap_or(false),
        "MCP tool failed; see the result above"
    );
    Ok(())
}

pub async fn run_remote(
    action: ToolsCommand,
    control: PathBuf,
    project: airk_core::service::ProjectBinding,
    root: &Path,
    data: &Path,
    collection: &str,
) -> Result<()> {
    if inspect(&action)? {
        return Ok(());
    }
    let session = RemoteSession(airk_cli::project_client::tools(control, project));
    match action {
        ToolsCommand::Call {
            tool,
            args,
            args_file,
        } => {
            call(
                &session,
                tool,
                arguments(args.as_deref(), args_file.as_deref())?,
                true,
            )
            .await
        }
        ToolsCommand::Shell => shell(&session, root, data, collection).await,
        _ => unreachable!("tool metadata handled before connection"),
    }
}

fn shell_help() {
    println!(
        "Commands:\n  list                         List MCP tools\n  help <tool>                  Arguments, defaults and examples\n  call <tool> <JSON object>     Call a tool; omit JSON for {{}}\n  call <tool> --args-file <path> Read JSON from a file\n  help                         Show this help\n  exit                         Close this console\n\nTry:\n  call airk_status\n  help airk_search\n  call airk_search {{\"query\":\"authentication\",\"mode\":\"vector\",\"limit\":5}}\n\nThis is an AIRK tools console, not an operating-system shell.\nCalls read the existing index; they do not index or download models.\nJSON is entered directly without shell quoting. File paths may contain spaces.\n"
    );
}

async fn shell(
    session: &impl ToolSession,
    root: &Path,
    data: &Path,
    collection: &str,
) -> Result<()> {
    // Debug formatting escapes control characters in user-selected paths/names.
    println!(
        "AIRK Tools\nCollection: {:?}\nWorkspace: {:?}\nStorage: {:?}\n",
        collection, root, data
    );
    shell_help();
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    loop {
        print!("airk> ");
        std::io::stdout().flush()?;
        let mut line = Vec::new();
        if input
            .by_ref()
            .take((MAX_ARGUMENT_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)?
            == 0
        {
            break;
        }
        ensure!(
            line.len() <= MAX_ARGUMENT_BYTES,
            "Console input exceeds 64 KiB; closing console"
        );
        let line = match std::str::from_utf8(&line) {
            Ok(line) => line.trim(),
            Err(_) => {
                eprintln!("Error: input must be UTF-8");
                continue;
            }
        };
        if line.is_empty() {
            continue;
        }
        if matches!(line, "exit" | "quit") {
            break;
        }
        if matches!(line, "help" | "--help" | "?") {
            shell_help();
            continue;
        }
        let result = console_command(session, root, line).await;
        if let Err(error) = result {
            eprintln!("Error: {error:#}");
        }
    }
    Ok(())
}

async fn console_command(session: &impl ToolSession, root: &Path, line: &str) -> Result<()> {
    let (command, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let rest = rest.trim();
    match command {
        "list" if rest.is_empty() => {
            inspect(&ToolsCommand::List { json: false })?;
        }
        "help" if !rest.is_empty() => {
            inspect(&ToolsCommand::Help {
                tool: rest.into(),
                json: false,
            })?;
        }
        "call" if !rest.is_empty() => {
            let (tool, args) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let args = args.trim();
            let args = if let Some(path) = args.strip_prefix("--args-file ") {
                let path = path.trim();
                let path = path
                    .strip_prefix('"')
                    .and_then(|p| p.strip_suffix('"'))
                    .unwrap_or(path);
                arguments(None, Some(&root.join(path)))?
            } else {
                arguments(if args.is_empty() { None } else { Some(args) }, None)?
            };
            call(session, tool.into(), args, false).await?;
        }
        _ => anyhow::bail!("Unknown command. Type help for instructions."),
    }
    Ok(())
}
