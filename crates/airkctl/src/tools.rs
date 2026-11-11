// SPDX-License-Identifier: Apache-2.0
//! `airkctl tools list|help` (split step 3a.1): text rendered from the M2
//! catalog documents exactly as the pre-split `airk tools list|help` printed
//! it, with `airk tools` guidance renamed to `airkctl tools`. JSON output is the
//! service's document text, unchanged.
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use std::{io::Read, path::Path};

/// The largest tool argument object, as the pre-split CLI bounded it.
pub const MAX_ARGUMENT_BYTES: usize = airk_core::service::MAX_TOOL_ARGUMENT_BYTES;

/// A tool's JSON object arguments from `--args`, `--args-file` or neither
/// (`{}`), read and bounded as the pre-split `airk tools call` read them.
pub fn read_arguments(text: Option<&str>, file: Option<&Path>) -> Result<Map<String, Value>> {
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

/// `tools list` text from a list document (names-only or complete).
pub fn list_text(document: &Value) -> Option<String> {
    let mut text = String::new();
    for tool in document["tools"].as_array()? {
        text.push_str(tool["name"].as_str()?);
        text.push_str("\n  ");
        text.push_str(tool["description"].as_str().unwrap_or_default());
        text.push('\n');
    }
    text.push_str("\nUse: airkctl tools help <tool>\nOr in the tools console: help <tool>\n");
    Some(text)
}

/// The argument template: each property's default, otherwise a placeholder
/// for a required property chosen by its declared type.
pub fn template(schema: &Value) -> Map<String, Value> {
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
    template
}

/// `tools help <name>` text from a help document; `name` is the name as typed.
pub fn help_text(tool: &Value, name: &str) -> Option<String> {
    let schema = tool
        .get("inputSchema")
        .filter(|schema| schema.is_object())?;
    let template = template(schema);
    let mut text = format!(
        "{}\n{}\n\nInput schema (required arguments and defaults):\n{}\n",
        tool["name"].as_str()?,
        tool["description"].as_str().unwrap_or_default(),
        serde_json::to_string_pretty(schema).ok()?
    );
    text.push_str("\nArgument template (replace placeholders; follow schema constraints):\n");
    text.push_str(&serde_json::to_string_pretty(&template).ok()?);
    text.push('\n');
    text.push_str(&format!(
        "\nSave arguments as query.json, then run with your collection options:\n  airkctl tools call {name} --args-file query.json\nInside the tools console:\n  call {name} --args-file query.json\n  call {name} {}\n",
        Value::Object(template)
    ));
    if matches!(name, "airk_search" | "search") {
        text.push_str(
            "\nExplicit vector search (requires a published model/index and discloses partial coverage):\n  call airk_search {\"query\":\"how is a forgotten password recovered\",\"mode\":\"vector\",\"limit\":5}\nOmit mode for the saved collection search policy.\n",
        );
    }
    Some(text)
}

#[cfg(test)]
mod tests;
