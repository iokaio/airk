// SPDX-License-Identifier: Apache-2.0
//! `airkctl tools list|help` (split step 3a.1): text rendered from the M2
//! catalog documents exactly as the pre-split `airk tools list|help` printed
//! it, with `airk tools` guidance renamed to `airkctl tools`. JSON output is the
//! service's document text, unchanged.
use serde_json::{Map, Value, json};
use std::path::PathBuf;

/// Arguments of `tools list|help`: positionals, `--json` and the explicit
/// `--runtime-dir` (the I4 pointer arrives with step 3b).
pub struct Arguments<'a> {
    pub positionals: Vec<&'a str>,
    pub json: bool,
    pub runtime_dir: Option<PathBuf>,
}

pub fn arguments<'a>(arguments: &[&'a str]) -> Result<Arguments<'a>, &'static str> {
    let mut parsed = Arguments {
        positionals: Vec::new(),
        json: false,
        runtime_dir: None,
    };
    let mut rest = arguments.iter();
    while let Some(&argument) = rest.next() {
        let runtime_dir = if argument == "--runtime-dir" {
            Some(*rest.next().ok_or("--runtime-dir takes a directory")?)
        } else {
            argument.strip_prefix("--runtime-dir=")
        };
        if let Some(directory) = runtime_dir {
            if parsed.runtime_dir.is_some() || directory.is_empty() {
                return Err("--runtime-dir takes one directory");
            }
            parsed.runtime_dir = Some(PathBuf::from(directory));
        } else if argument == "--json" {
            parsed.json = true;
        } else if argument.starts_with('-') {
            return Err("unknown option for tools");
        } else {
            parsed.positionals.push(argument);
        }
    }
    Ok(parsed)
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
