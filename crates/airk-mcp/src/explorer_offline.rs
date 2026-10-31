// SPDX-License-Identifier: Apache-2.0
//! Explorer Assistance tools for the headless CLI. Drafts are checked against the
//! same generated schemas the MCP server embeds and are returned for a file;
//! nothing is admitted to an editor Inbox, and no draft is executed. The
//! editor's additional semantic and live checks need a running window.
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Map, Value, json};

const SCHEMAS: &str = include_str!("unified_explorer.json");
const TEMPLATES: &str = include_str!("explorer_templates.json");
pub const DRAFT_FILE_SCHEMA: &str = "airk.explorer.draft-file/1";

/// Draft kind, public tool name and embedded schema name.
const KINDS: &[(&str, &str, &str)] = &[
    ("search", "draft_search", "airk_explorer_suggest_search"),
    (
        "strategy",
        "draft_strategy",
        "airk_explorer_suggest_strategy",
    ),
    ("plan", "draft_plan", "airk_explorer_suggest_plan"),
    (
        "navigation",
        "suggest_view",
        "airk_explorer_suggest_navigation",
    ),
    ("review", "suggest_view", "airk_explorer_suggest_review"),
    (
        "management",
        "suggest_view",
        "airk_explorer_suggest_management",
    ),
];

fn schema(name: &str) -> Result<Value> {
    let tools: Vec<Value> = serde_json::from_str(SCHEMAS)?;
    tools
        .into_iter()
        .find(|tool| tool["name"] == name)
        .map(|tool| tool["inputSchema"].clone())
        .with_context(|| format!("embedded Explorer schema {name} missing"))
}

fn identity() -> Result<Value> {
    let pack: Value = serde_json::from_str(TEMPLATES)?;
    Ok(
        json!({"source":"airk CLI (offline)","catalog_version":pack["catalog_version"],"extension_version":pack["extension_version"]}),
    )
}

/// Structural JSON-Schema check for the keywords the generated Explorer schemas use.
fn check(schema: &Value, value: &Value, path: &str, errors: &mut Vec<String>) {
    let at = if path.is_empty() { "(draft)" } else { path };
    if let Some(expected) = schema.get("const")
        && expected != value
    {
        errors.push(format!("{at}: must be {expected}"));
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array)
        && !options.contains(value)
    {
        errors.push(format!(
            "{at}: must be one of {}",
            Value::Array(options.clone())
        ));
    }
    let types: Vec<&str> = match schema.get("type") {
        Some(Value::String(kind)) => vec![kind.as_str()],
        Some(Value::Array(kinds)) => kinds.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let matches = |kind: &str| match kind {
        "string" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "null" => value.is_null(),
        _ => true,
    };
    if !types.is_empty() && !types.iter().any(|kind| matches(kind)) {
        errors.push(format!("{at}: must be {}", types.join(" or ")));
        return;
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as u64;
        if schema["minLength"].as_u64().is_some_and(|min| length < min) {
            errors.push(format!("{at}: is shorter than {}", schema["minLength"]));
        }
        if schema["maxLength"].as_u64().is_some_and(|max| length > max) {
            errors.push(format!("{at}: is longer than {}", schema["maxLength"]));
        }
        if let Some(pattern) = schema["pattern"].as_str() {
            match regex::Regex::new(pattern) {
                Ok(expression) if !expression.is_match(text) => {
                    errors.push(format!("{at}: does not match {pattern}"))
                }
                Ok(_) => {}
                Err(_) => errors.push(format!("{at}: schema pattern {pattern} is unsupported")),
            }
        }
    }
    if let Some(number) = value.as_f64() {
        if schema["minimum"].as_f64().is_some_and(|min| number < min) {
            errors.push(format!("{at}: is below {}", schema["minimum"]));
        }
        if schema["maximum"].as_f64().is_some_and(|max| number > max) {
            errors.push(format!("{at}: is above {}", schema["maximum"]));
        }
    }
    if let Some(items) = value.as_array() {
        if schema["minItems"]
            .as_u64()
            .is_some_and(|min| (items.len() as u64) < min)
        {
            errors.push(format!("{at}: needs at least {} items", schema["minItems"]));
        }
        if schema["maxItems"]
            .as_u64()
            .is_some_and(|max| items.len() as u64 > max)
        {
            errors.push(format!("{at}: allows at most {} items", schema["maxItems"]));
        }
        if let Some(item) = schema.get("items") {
            for (index, entry) in items.iter().enumerate() {
                check(item, entry, &format!("{path}[{index}]"), errors);
            }
        }
    }
    if let Some(fields) = value.as_object() {
        if schema["maxProperties"]
            .as_u64()
            .is_some_and(|max| fields.len() as u64 > max)
        {
            errors.push(format!(
                "{at}: allows at most {} fields",
                schema["maxProperties"]
            ));
        }
        for required in schema["required"].as_array().into_iter().flatten() {
            if let Some(name) = required.as_str()
                && !fields.contains_key(name)
            {
                errors.push(format!("{}: is required", join(path, name)));
            }
        }
        let properties = schema["properties"].as_object();
        for (name, field) in fields {
            match properties.and_then(|properties| properties.get(name)) {
                Some(property) => check(property, field, &join(path, name), errors),
                None if schema["additionalProperties"] == json!(false) => {
                    errors.push(format!("{}: is not allowed", join(path, name)))
                }
                None => {
                    if let Some(extra) = schema
                        .get("additionalProperties")
                        .filter(|extra| extra.is_object())
                    {
                        check(extra, field, &join(path, name), errors);
                    }
                }
            }
        }
    }
}
fn join(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.into()
    } else {
        format!("{path}.{name}")
    }
}

fn validate(kind: &str, draft: &Value) -> Result<Vec<String>> {
    let (_, _, name) = KINDS
        .iter()
        .find(|(candidate, _, _)| *candidate == kind)
        .with_context(|| format!("unknown draft kind {kind}; use search, strategy, plan, navigation, review or management"))?;
    let mut errors = Vec::new();
    check(&schema(name)?, draft, "", &mut errors);
    Ok(errors)
}

/// A draft for a file: `tool` is draft_search, draft_strategy, draft_plan or suggest_view.
/// Arguments match the MCP tool (suggest_view takes `kind` and `draft`).
pub fn draft(
    tool: &str,
    mut arguments: Map<String, Value>,
    project: Option<String>,
    now_ms: u64,
) -> Result<Value> {
    let project = arguments
        .remove("project")
        .and_then(|value| value.as_str().map(str::to_owned))
        .or(project);
    let (kind, draft) = if tool == "suggest_view" {
        ensure!(
            arguments.keys().all(|key| key == "kind" || key == "draft"),
            "suggest_view accepts only kind and draft"
        );
        let kind = arguments
            .get("kind")
            .and_then(Value::as_str)
            .context("suggest_view needs kind: navigation, review or management")?
            .to_owned();
        ensure!(
            matches!(kind.as_str(), "navigation" | "review" | "management"),
            "suggest_view kind must be navigation, review or management"
        );
        (
            kind,
            arguments
                .remove("draft")
                .context("suggest_view needs draft")?,
        )
    } else {
        let (kind, _, _) = KINDS
            .iter()
            .find(|(_, public, _)| *public == tool)
            .with_context(|| format!("unknown draft tool {tool}"))?;
        ((*kind).to_owned(), Value::Object(arguments))
    };
    let errors = validate(&kind, &draft)?;
    Ok(
        json!({"schema":DRAFT_FILE_SCHEMA,"tool":tool,"kind":kind,"project":project,"draft":draft,
        "valid":errors.is_empty(),"errors":errors,"checked":"schema","semantic_checked":false,"live_checked":false,
        "admitted":false,"executed":false,"created_unix_ms":now_ms,"identity":identity()?,
        "note":"Written for review only. Nothing was sent to an Explorer Inbox or executed; the editor applies further semantic checks."}),
    )
}

/// Same arguments as the MCP validate_draft tool: `kind` and `draft`.
pub fn validate_draft(arguments: &Map<String, Value>) -> Result<Value> {
    ensure!(
        arguments.keys().all(|key| key == "kind" || key == "draft"),
        "validate_draft accepts only kind and draft"
    );
    let kind = arguments
        .get("kind")
        .and_then(Value::as_str)
        .context("validate_draft needs kind")?;
    let errors = validate(
        kind,
        arguments
            .get("draft")
            .context("validate_draft needs draft")?,
    )?;
    Ok(
        json!({"valid":errors.is_empty(),"kind":kind,"errors":errors.iter().map(|message|json!({"field":message.split(':').next().unwrap_or(""),"message":message})).collect::<Vec<_>>(),
        "checked":"schema","semantic_checked":false,"live_checked":false,"admitted":false,"executed":false,"identity":identity()?}),
    )
}

/// Same paging arguments as the MCP list_templates tool.
pub fn list_templates(arguments: &Map<String, Value>) -> Result<Value> {
    ensure!(
        arguments
            .keys()
            .all(|key| ["offset", "limit", "max_bytes"].contains(&key.as_str())),
        "list_templates accepts offset, limit and max_bytes"
    );
    let pack: Value = serde_json::from_str(TEMPLATES)?;
    let entries = pack["templates"]
        .as_array()
        .context("template pack missing")?;
    let offset = arguments.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let limit = arguments.get("limit").and_then(Value::as_u64).unwrap_or(10);
    let budget = arguments
        .get("max_bytes")
        .and_then(Value::as_u64)
        .unwrap_or(8192) as usize;
    ensure!(
        offset <= entries.len() && (1..=25).contains(&limit) && (1024..=32768).contains(&budget),
        "offset, limit (1..25) or max_bytes (1024..32768) out of range"
    );
    let mut templates = Vec::new();
    for entry in entries.iter().skip(offset).take(limit as usize) {
        let strategy = &entry["strategy"];
        let mut item = json!({"template_id":strategy["id"],"version":strategy["version"],"title":strategy["title"],
            "summary":strategy["summary"],"category":strategy["category"],"runnable":entry["runnable"]});
        if let Some(reason) = entry.get("reason") {
            item["reason"] = reason.clone();
        }
        templates.push(item);
        if serde_json::to_vec(&templates)?.len() > budget {
            templates.pop();
            break;
        }
    }
    let next = offset + templates.len();
    Ok(
        json!({"identity":identity()?,"templates":templates,"total":entries.len(),"next_offset":(next < entries.len()).then_some(next)}),
    )
}

/// Same arguments as the MCP get_template tool: `template_id`.
pub fn get_template(arguments: &Map<String, Value>) -> Result<Value> {
    let id = arguments
        .get("template_id")
        .and_then(Value::as_str)
        .context("get_template needs template_id (see list_templates)")?;
    ensure!(
        arguments.len() == 1,
        "get_template accepts only template_id"
    );
    let pack: Value = serde_json::from_str(TEMPLATES)?;
    let Some(entry) = pack["templates"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|entry| entry["strategy"]["id"] == id)
    else {
        bail!("unknown template {id}; use list_templates");
    };
    let mut operators = Map::new();
    for step in entry["strategy"]["steps"].as_array().into_iter().flatten() {
        if let Some(op) = step["op"].as_str() {
            operators.insert(op.into(), pack["operators"][op].clone());
        }
    }
    let mut value = entry.clone();
    value["identity"] = identity()?;
    value["operators"] = Value::Object(operators);
    Ok(value)
}

/// Status of a draft file written by the CLI. It was never admitted to an Inbox.
pub fn proposal_status(file: &Value) -> Result<Value> {
    ensure!(
        file["schema"] == DRAFT_FILE_SCHEMA,
        "not an AIRK draft file ({DRAFT_FILE_SCHEMA})"
    );
    Ok(
        json!({"state":"written","admitted":false,"reviewed":false,"executed":false,"tool":file["tool"],"kind":file["kind"],
        "valid":file["valid"],"created_unix_ms":file["created_unix_ms"],
        "note":"CLI drafts live only in their file. Live proposal status exists only for drafts sent through an editor connection."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_an_embedded_schema_and_templates_round_trip() -> Result<()> {
        for (kind, _, name) in KINDS {
            assert!(schema(name)?.is_object(), "{kind}");
        }
        let page = list_templates(&Map::new())?;
        let first = page["templates"][0]["template_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let template = get_template(&json!({"template_id":first}).as_object().unwrap().clone())?;
        assert!(
            template["operators"]
                .as_object()
                .is_some_and(|ops| !ops.is_empty())
        );
        assert!(
            get_template(
                &json!({"template_id":"airk.core/missing"})
                    .as_object()
                    .unwrap()
                    .clone()
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn a_valid_search_draft_is_written_for_review_with_its_project() -> Result<()> {
        let arguments = json!({"schema_version":"1.0","idempotency_key":"cli-1","title":"Find copper",
            "draft":{"query":"copper harbor","mode":"lexical"},"project":"ab".repeat(32)});
        let written = draft(
            "draft_search",
            arguments.as_object().unwrap().clone(),
            None,
            7,
        )?;
        assert_eq!(written["valid"], true, "{written}");
        assert_eq!(written["project"], "ab".repeat(32));
        assert_eq!(
            (
                written["kind"].clone(),
                written["admitted"].clone(),
                written["executed"].clone()
            ),
            (json!("search"), json!(false), json!(false))
        );
        let view = draft(
            "suggest_view",
            json!({"kind":"navigation","draft":{}})
                .as_object()
                .unwrap()
                .clone(),
            None,
            7,
        )?;
        assert_eq!(view["kind"], "navigation");
        assert!(
            draft(
                "suggest_view",
                json!({"kind":"search","draft":{}})
                    .as_object()
                    .unwrap()
                    .clone(),
                None,
                7
            )
            .is_err()
        );
        Ok(())
    }

    #[test]
    fn invalid_drafts_are_reported_and_never_admitted() -> Result<()> {
        let written = draft(
            "draft_search",
            json!({"unexpected":true}).as_object().unwrap().clone(),
            None,
            1,
        )?;
        assert_eq!(written["valid"], false);
        assert_eq!(written["admitted"], false);
        assert!(
            written["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|error| error.as_str().unwrap().contains("unexpected"))
        );
        let checked = validate_draft(
            &json!({"kind":"search","draft":{"unexpected":true}})
                .as_object()
                .unwrap()
                .clone(),
        )?;
        assert_eq!(checked["valid"], false);
        assert!(
            validate_draft(
                &json!({"kind":"unknown","draft":{}})
                    .as_object()
                    .unwrap()
                    .clone()
            )
            .is_err()
        );
        let status = proposal_status(&written)?;
        assert_eq!(status["state"], "written");
        assert!(proposal_status(&json!({})).is_err());
        Ok(())
    }
}
