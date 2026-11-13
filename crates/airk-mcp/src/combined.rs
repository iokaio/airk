// SPDX-License-Identifier: Apache-2.0
//! Selected-session catalog. The native owner resolves and checks authority.
pub use airk_core::combined_reads::query_scope::{QueryScopeArgs, QueryScopeView};
use rmcp::model::Tool;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Deserialize, Serialize)]
#[serde(transparent)]
pub struct QueryScopeParameters(pub QueryScopeArgs);
impl rmcp::schemars::JsonSchema for QueryScopeParameters {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "QueryScopeArgs".into()
    }
    fn json_schema(_: &mut rmcp::schemars::SchemaGenerator) -> rmcp::schemars::Schema {
        serde_json::from_value(airk_core::combined_reads::query_scope::schema())
            .expect("static query_scope schema")
    }
}

pub fn query_scope_tool(name: &str) -> Tool {
    serde_json::from_value(json!({
        "name":name,
        "description":"Discover only the connection's reviewed Project/index grant. Projects, indexes and sources have bounded pages and revision-bound continuations. recommendations returns bounded metadata-only advice and catalog-checked scope instructions, with optional explicit selection/situation and no cursor or limit. Legacy connections report configuration_required. Never enumerates the machine library, queries evidence or opens sources.",
        "inputSchema":airk_core::combined_reads::query_scope::schema(),
        "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}
    })).expect("static query_scope catalog")
}

/// U107: the per-index count selector of a multi-index read tool (one
/// that takes `limit`).
pub fn limit_per_index_schema() -> Value {
    json!({"type":"integer","minimum":1,"maximum":100,"description":"Results each selected index returns, exactly; results stay grouped by index in members. limit becomes the overall cap and defaults to the number of indexes x limit_per_index; refused (combined_limit_per_index_too_large) when that exceeds limit or 100. Byte, deadline and verification budgets stay shared evenly across indexes. Omit to split limit evenly."})
}

fn controls(mut tool: Tool) -> Tool {
    let schema = Arc::make_mut(&mut tool.input_schema);
    let properties = schema.entry("properties").or_insert_with(|| json!({}));
    // A selected combined session cannot switch into a legacy binding.
    properties
        .as_object_mut()
        .expect("tool properties")
        .remove("project");
    properties["project_ids"] = json!({"type":"array","items":{"type":"string","pattern":"^prj_[0-9a-f]{32}$"},"minItems":1,"maxItems":32,"uniqueItems":true,"description":"Explicit Projects within the frozen reviewed connection grant. Omission keeps the connection's captured selection; no workspace or saved-default expansion."});
    properties["index_ids"] = json!({"type":"array","items":{"type":"string","pattern":"^idx_[0-9a-f]{32}$"},"minItems":1,"maxItems":32,"uniqueItems":true,"description":"Optional explicit index subset of the selected Projects. Qualified follow-up reads require one originating index."});
    if properties.get("limit").is_some() {
        properties["limit_per_index"] = limit_per_index_schema();
    }
    properties["member_availability"] = json!({"type":"string","enum":["report","require"],"default":"report","description":"Report retains usable data and complete member failures. Require returns non-success if any selected member cannot answer; independent of saved-source verification."});
    if !matches!(
        tool.name.as_ref(),
        "help" | "airk_help" | "parse" | "airk_parse"
    ) {
        properties["origin_publication"] = json!({"type":"string","minLength":1,"maxLength":256,"description":"Exact originating member publication token. Requires one selected index; a different member publication expires the follow-up rather than substituting newer evidence."});
    }
    if properties.get("cursor").is_some() {
        properties["cursor"] = json!({"type":"string","minLength":1,"maxLength":32768,"description":"Combined continuation bound to all selected member publications, grant/scope identity, filters, representation and shared budgets. Repeat the same request inputs; any changed member expires this cursor."});
    }
    // Keep composition keywords out of tools' root schema for clients that
    // require an object-shaped catalog; the dependent selectors remain explicit.
    schema.insert(
        "dependentRequired".into(),
        json!({
            "index_ids":["project_ids"], "member_availability":["project_ids"]
        }),
    );
    tool
}

/// Same native handler catalog with selectors resolved by the combined owner.
pub fn catalog() -> Vec<Tool> {
    let mut tools: Vec<_> = crate::local::tools()
        .into_iter()
        .map(|tool| {
            if tool.name == "airk_query_scope" {
                tool
            } else {
                controls(tool)
            }
        })
        .collect();
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    tools
}

/// Unified public aliases preserve each native input schema exactly.
pub fn public_catalog() -> Vec<Tool> {
    let native = crate::local::tools();
    let mut tools: Vec<_> = crate::unified::tools()
        .into_iter()
        .map(|mut tool| {
            if tool.name == "query_scope" {
                Arc::make_mut(&mut tool.input_schema)
                    .get_mut("properties")
                    .and_then(Value::as_object_mut)
                    .expect("query scope properties")
                    .remove("project");
            }
            if tool.name != "query_scope"
                && native
                    .iter()
                    .any(|entry| entry.name.strip_prefix("airk_") == Some(tool.name.as_ref()))
            {
                controls(tool)
            } else {
                tool
            }
        })
        .collect();
    if !tools.iter().any(|tool| tool.name == "query_scope") {
        tools.push(query_scope_tool("query_scope"));
    }
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    tools
}

/// A legacy connection cannot infer a reviewed Project scope from its binding.
pub fn legacy_discovery(arguments: Value) -> Value {
    let result = serde_json::from_value::<QueryScopeArgs>(arguments)
        .map_err(anyhow::Error::from)
        .and_then(|args| airk_core::combined_reads::query_scope::legacy_body(&args));
    match result {
        Ok(body) => crate::unified::success(body),
        Err(_) => crate::unified::failure(
            "INVALID_SCHEMA",
            "Invalid query_scope selectors, cursor or pagination budget.",
        ),
    }
}

/// Parse the same closed native handler inputs before admitting any member.
/// The caller removes only combined controls; native cursors/validators retain
/// their native owner checks when the actual handler executes.
pub fn validate_native_arguments(
    tool: &str,
    args: &serde_json::Map<String, Value>,
) -> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    let name = tool.strip_prefix("airk_").unwrap_or(tool);
    let catalog = crate::local::tools();
    let definition = catalog
        .iter()
        .find(|entry| entry.name.strip_prefix("airk_") == Some(name))
        .context("unknown native read")?;
    let properties = definition.input_schema["properties"]
        .as_object()
        .context("native tool has no object input")?;
    ensure!(
        args.keys().all(|key| properties.contains_key(key)),
        "unknown native read argument"
    );
    let value = Value::Object(args.clone());
    macro_rules! typed {
        ($kind:ty) => {
            serde_json::from_value::<$kind>(value.clone())?
        };
    }
    macro_rules! evidence {
        ($args:ident) => {{
            $args.verification.controls().resolve()?;
        }};
    }
    macro_rules! facts {
        ($args:ident) => {{
            evidence!($args);
            airk_core::validate_budget($args.limit, $args.max_bytes)?;
            airk_core::reply::ReplyDetail::compact($args.response.reply_detail, $args.compact)?;
        }};
    }
    match name {
        "search" => {
            let args = typed!(crate::SearchArgs);
            evidence!(args);
            ensure!(
                !args.semantic || args.mode.is_none(),
                "choose mode or semantic, not both"
            );
            airk_core::SearchFilters {
                language: args.language,
                evidence_kind: args.evidence_kind.map(Into::into),
                exclude_path_prefix: args.exclude_path_prefix,
            }
            .validate()?;
            airk_core::SearchQuery {
                text: args.query,
                path_prefix: args.path_prefix,
                limit: args.limit,
                max_bytes: args.max_bytes,
            }
            .validate()?;
        }
        "symbols" | "imports" => {
            let args = typed!(crate::FactArgs);
            facts!(args);
        }
        "code_facts" => {
            let args = typed!(crate::CodeFactArgs);
            facts!(args);
        }
        "document_facts" => {
            let args = typed!(crate::DocumentFactArgs);
            facts!(args);
        }
        "outline" => {
            let args = typed!(crate::OutlineArgs);
            facts!(args);
            args.evidence.options()?;
        }
        "file_info" => {
            let args = typed!(crate::PathArgs);
            evidence!(args);
            args.evidence.options()?;
        }
        "read_file" => {
            let args = typed!(crate::ReadArgs);
            evidence!(args);
            args.evidence.options()?;
            airk_core::validate_budget(1, args.max_bytes)?;
            ensure!(
                args.start_line > 0 && args.end_line >= args.start_line,
                "invalid line range"
            );
        }
        "context" => {
            let args = typed!(crate::ContextArgs);
            evidence!(args);
            args.evidence.options()?;
            airk_core::validate_budget(1, args.max_bytes)?;
            ensure!(args.line > 0, "invalid context line");
        }
        "list_files" => {
            let args = typed!(crate::ListArgs);
            evidence!(args);
            airk_core::validate_budget(args.limit, args.max_bytes)?;
        }
        "references" => {
            let args = typed!(crate::ReferenceArgs);
            evidence!(args);
            airk_core::validate_budget(args.limit, args.max_bytes)?;
            airk_core::SearchFilters {
                language: args.language,
                evidence_kind: args.evidence_kind.map(Into::into),
                exclude_path_prefix: args.exclude_path_prefix,
            }
            .validate()?;
        }
        "relationships" => {
            let args = typed!(crate::RelationshipArgs);
            evidence!(args);
            airk_core::validate_budget(args.limit, args.max_bytes)?;
            ensure!(
                !args.symbol.is_empty() && args.symbol.len() <= 4096,
                "invalid relationship symbol"
            );
            ensure!(
                matches!(args.direction.as_str(), "incoming" | "outgoing"),
                "invalid relationship direction"
            );
            ensure!(
                (1..=3).contains(&args.hops),
                "relationship hops must be1..3"
            );
        }
        "changes" => {
            let args = typed!(crate::ChangesArgs);
            evidence!(args);
            args.request()?.validate()?;
        }
        "status" | "diagnostics" => {
            typed!(crate::StatusArgs);
        }
        "capabilities" => {
            typed!(crate::CapabilitiesArgs);
        }
        "help" => {
            let args = typed!(crate::help::HelpArgs);
            ensure!(
                (4096..=32768).contains(&args.max_bytes) && args.start_line > 0,
                "invalid help budget"
            );
        }
        "query_scope" => {
            let args = typed!(QueryScopeArgs);
            args.validate()?;
        }
        "parse" => {
            typed!(crate::ParseArgs);
        }
        _ => anyhow::bail!("unknown native read"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_admission_validation_uses_the_closed_handler_inputs() {
        for (tool, args) in [
            (
                "airk_search",
                json!({"query":"saved policy","mode":"lexical"}),
            ),
            ("airk_status", json!({"response_mode":"full"})),
            ("airk_query_scope", json!({"view":"projects"})),
            (
                "airk_symbols",
                json!({"name":"Policy","reply_detail":"rich","compact":false}),
            ),
            (
                "airk_read_file",
                json!({"path":"policy.md","start_line":1,"end_line":1}),
            ),
        ] {
            validate_native_arguments(tool, args.as_object().unwrap()).unwrap();
        }
        for (tool, args) in [
            ("airk_search", json!({})),
            ("airk_search", json!({"query":"x","mode":"unknown"})),
            ("airk_search", json!({"query":"x","limit":0})),
            (
                "airk_search",
                json!({"query":"x","mode":"lexical","semantic":true}),
            ),
            (
                "airk_search",
                json!({"query":"x","verification":"off","verification_max_sources":1}),
            ),
            ("airk_search", json!({"query":"x","extra":false})),
            (
                "airk_symbols",
                json!({"reply_detail":"rich","compact":true}),
            ),
            (
                "airk_read_file",
                json!({"path":"a","start_line":0,"end_line":1}),
            ),
            ("airk_file_info", json!({"path":"a","revision":"wrong"})),
            ("airk_status", json!({"query":"not a status argument"})),
            (
                "airk_query_scope",
                json!({"view":"projects","project_ids":[]}),
            ),
            ("airk_query_scope", json!({"view":"projects","cursor":null})),
        ] {
            assert!(
                validate_native_arguments(tool, args.as_object().unwrap()).is_err(),
                "{tool}: {args}"
            );
        }
    }
    #[test]
    fn scope_discovery_requires_exact_view_inputs_and_utf8_cursor_bound() {
        for valid in [
            json!({"view":"projects"}),
            json!({"view":"indexes","project_id":format!("prj_{}","01".repeat(16))}),
            json!({"view":"sources","project_id":format!("prj_{}","01".repeat(16)),"index_id":format!("idx_{}","02".repeat(16))}),
        ] {
            let args: QueryScopeArgs = serde_json::from_value(valid).unwrap();
            args.validate().unwrap();
        }
        for invalid in [
            json!({"view":"indexes"}),
            json!({"view":"sources"}),
            json!({"view":"projects","index_id":format!("idx_{}","02".repeat(16))}),
            json!({"view":"projects","limit":0}),
            json!({"view":"projects","max_bytes":65537}),
            json!({"view":"projects","cursor":"é".repeat(16385)}),
            json!({"view":"projects","extra":true}),
            json!({"view":"projects","limit":null}),
            json!({"view":"projects","cursor":null}),
            json!({"view":"projects","project_id":null}),
        ] {
            assert!(match serde_json::from_value::<QueryScopeArgs>(invalid) {
                Err(_) => true,
                Ok(args) => args.validate().is_err(),
            });
        }
    }
    #[test]
    fn legacy_discovery_does_not_invent_scope_or_publication() {
        let reply = legacy_discovery(json!({"view":"projects"}));
        assert_eq!(reply["isError"], false);
        assert_eq!(
            reply["structuredContent"]["status"],
            "configuration_required"
        );
        assert!(reply["structuredContent"].get("publication").is_none());
        assert_eq!(reply["structuredContent"]["projects"], json!([]));
    }
    #[tokio::test]
    async fn native_and_parse_legacy_discovery_share_the_pure_metadata_body() -> anyhow::Result<()>
    {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("source");
        std::fs::create_dir(&source)?;
        std::fs::write(
            source.join("untouched.txt"),
            "Original metadata-only fixture\n",
        )?;
        let kernel = airk_kernel::Kernel::open(&source, Some(directory.path().join("data")))?;
        let mut tools = crate::local::LocalTools::new(kernel).await?;
        let args = json!({"view":"projects"});
        let expected = legacy_discovery(args.clone())["structuredContent"].clone();
        let reply = tools
            .call("airk_query_scope".into(), args.as_object().unwrap().clone())
            .await?;
        assert_ne!(reply.is_error, Some(true), "{reply:?}");
        let direct: Value = serde_json::from_str(&reply.content[0].as_text().unwrap().text)?;
        assert_eq!(direct, expected);
        let reply=tools.call("airk_parse".into(),json!({"schema":"airk.parse.request/1","mode":"execute","plan":{"steps":[{"id":"scope","tool":"query_scope","args":{"view":"projects"}}]},"response_mode":"full"}).as_object().unwrap().clone()).await?;
        assert_ne!(reply.is_error, Some(true), "{reply:?}");
        let plan: Value = serde_json::from_str(&reply.content[0].as_text().unwrap().text)?;
        assert_eq!(plan["steps"][0]["status"], "complete", "{plan}");
        assert_eq!(plan["steps"][0]["result"], expected);
        assert!(plan.get("source_verification").is_none(), "{plan}");
        assert_eq!(
            std::fs::read_to_string(source.join("untouched.txt"))?,
            "Original metadata-only fixture\n"
        );
        tools.close().await?;
        Ok(())
    }
    #[test]
    fn combined_native_and_public_selector_catalogs_agree() {
        let public = public_catalog();
        for native in catalog() {
            let public_name = native.name.strip_prefix("airk_").unwrap();
            let public = public.iter().find(|tool| tool.name == public_name).unwrap();
            for key in if public_name == "query_scope" {
                vec![
                    "view",
                    "project_id",
                    "index_id",
                    "cursor",
                    "limit",
                    "max_bytes",
                ]
            } else {
                vec![
                    "project_ids",
                    "index_ids",
                    "member_availability",
                    "origin_publication",
                ]
            } {
                assert_eq!(
                    native.input_schema["properties"][key], public.input_schema["properties"][key],
                    "{public_name}.{key}"
                );
            }
            assert!(public.input_schema["properties"].get("project").is_none());
            if public_name == "query_scope" {
                assert_eq!(
                    Value::Object((*native.input_schema).clone())["properties"],
                    airk_core::combined_reads::query_scope::schema()["properties"]
                );
                assert_eq!(native.input_schema["additionalProperties"], false);
            }
            if public_name != "query_scope" {
                assert_eq!(
                    native.input_schema.get("dependentRequired"),
                    public.input_schema.get("dependentRequired")
                );
                assert_eq!(
                    native.input_schema["dependentRequired"]["index_ids"],
                    json!(["project_ids"])
                );
                assert_eq!(
                    native.input_schema["dependentRequired"]["member_availability"],
                    json!(["project_ids"])
                );
            }
        }
    }
}

#[cfg(test)]
mod limit_per_index_tests {
    use super::*;

    /// U107: exactly the combined read tools that take `limit` offer
    /// `limit_per_index`, in the native-name and the public catalogs.
    #[test]
    fn limit_per_index_follows_limit_in_both_combined_catalogs() {
        for catalog in [catalog(), public_catalog()] {
            let mut offered = 0;
            for tool in &catalog {
                let properties = tool.input_schema["properties"].as_object();
                let has = |key: &str| properties.is_some_and(|map| map.contains_key(key));
                let combined_read = has("member_availability");
                assert_eq!(
                    has("limit_per_index"),
                    combined_read && has("limit"),
                    "{}",
                    tool.name
                );
                offered += usize::from(has("limit_per_index"));
            }
            assert!(offered > 0);
        }
        let search = public_catalog()
            .into_iter()
            .find(|tool| tool.name == "search")
            .unwrap();
        assert_eq!(
            search.input_schema["properties"]["limit_per_index"],
            limit_per_index_schema()
        );
    }
}
