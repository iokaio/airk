// SPDX-License-Identifier: Apache-2.0
//! Notebook source discovery only. Cells, outputs and attachments are never executed.
use crate::{adapters::registry::AdapterRegistry, build_facts::Facts};
use airk_core::{
    Fact,
    extraction::LocalRegion,
    location::{ByteSpan, HostBudget, MapSegment, Mapping, RegionLocation, SourceMap},
};
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::{Node, Parser};

pub struct NotebookDiscovery {
    pub regions: Vec<LocalRegion>,
    pub facts: Vec<Fact>,
    pub diagnostics: Vec<String>,
    pub partial: bool,
}
impl NotebookDiscovery {
    fn diagnostic(&mut self, message: impl Into<String>) {
        self.partial = true;
        if self.diagnostics.len() < 256 {
            let mut message = message.into();
            let mut end = message.len().min(4096);
            while !message.is_char_boundary(end) {
                end -= 1;
            }
            message.truncate(end);
            self.diagnostics.push(message);
        }
    }
}

pub fn discover(path: &str, text: &str, budget: &mut HostBudget) -> Result<NotebookDiscovery> {
    ensure!(
        text.len() <= 16_777_216,
        "notebook source exceeds byte limit"
    );
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_json::LANGUAGE.into())?;
    let tree = parser
        .parse(text, None)
        .context("notebook parser returned no tree")?;
    let mut pending = vec![tree.root_node()];
    while let Some(node) = pending.pop() {
        budget.charge(1, 0, 0, 0)?;
        let mut cursor = node.walk();
        pending.extend(node.named_children(&mut cursor));
    }
    let mut result = NotebookDiscovery {
        regions: vec![],
        facts: vec![],
        diagnostics: vec![],
        partial: false,
    };
    if tree.root_node().has_error() {
        result.diagnostic("Malformed notebook JSON; no cells admitted");
        return Ok(result);
    }
    let root = tree
        .root_node()
        .named_child(0)
        .context("empty notebook JSON")?;
    let ambiguous_root_metadata = duplicate_field(root, text, Some("metadata"))?;
    let root = object(root, text, &mut result)?;
    ensure!(
        root.get("nbformat")
            .is_some_and(|n| n.utf8_text(text.as_bytes()).ok() == Some("4")),
        "only notebook format 4 is supported"
    );
    let cells = *root
        .get("cells")
        .context("notebook cells missing or ambiguous")?;
    ensure!(cells.kind() == "array", "notebook cells must be an array");
    let root_language = if ambiguous_root_metadata {
        Some(Language { adapter: None })
    } else {
        metadata_language(root.get("metadata").copied(), text, true, &mut result)?
    };
    let mut cursor = cells.walk();
    let cells: Vec<_> = cells.named_children(&mut cursor).collect();
    let mut ids = BTreeMap::new();
    // Count IDs before emitting any region, so every duplicate loses identity qualification.
    for cell in cells.iter().take(256) {
        let fields = object(*cell, text, &mut result)?;
        if let Some(id) = fields
            .get("id")
            .and_then(|n| string(*n, text).ok())
            .filter(|id| !id.is_empty() && id.len() <= 4096 && !id.contains('\0'))
        {
            *ids.entry(id).or_insert(0usize) += 1;
        }
    }
    let registry = AdapterRegistry::baseline();
    let mut facts = Facts::new(path, text, "notebook")?;
    for (position, cell) in cells.into_iter().enumerate() {
        budget.enter_region(1)?;
        let fields = object(cell, text, &mut result)?;
        let cell_type = fields.get("cell_type").and_then(|n| string(*n, text).ok());
        let Some(cell_type) = cell_type else {
            result.diagnostic(format!(
                "Cell {} has missing or ambiguous cell_type",
                position + 1
            ));
            continue;
        };
        let local_language = if duplicate_field(cell, text, Some("metadata"))? {
            Some(Language { adapter: None })
        } else {
            metadata_language(fields.get("metadata").copied(), text, false, &mut result)?
        };
        let language = match cell_type.as_str() {
            "markdown" => Some("markdown"),
            "code" => local_language
                .as_ref()
                .or(root_language.as_ref())
                .and_then(|language| language.adapter),
            "raw" => {
                result.diagnostic(format!("Raw cell {} is not indexed", position + 1));
                None
            }
            _ => {
                result.diagnostic(format!("Unknown cell type at position {}", position + 1));
                None
            }
        };
        let Some(language) = language else {
            result.diagnostic(format!(
                "Cell {} language is unknown or ambiguous; source omitted",
                position + 1
            ));
            continue;
        };
        let Some(source) = fields.get("source") else {
            result.diagnostic(format!("Cell {} source missing or ambiguous", position + 1));
            continue;
        };
        let mut decoded = String::new();
        let mut segments = Vec::new();
        let sources = if source.kind() == "string" {
            vec![*source]
        } else if source.kind() == "array" {
            let mut cursor = source.walk();
            source.named_children(&mut cursor).collect()
        } else {
            result.diagnostic(format!(
                "Cell {} source is not string/string-array",
                position + 1
            ));
            continue;
        };
        let mut valid = true;
        for source in sources {
            if source.kind() != "string" {
                result.diagnostic(format!(
                    "Cell {} contains a non-string source element",
                    position + 1
                ));
                valid = false;
                break;
            }
            if let Err(error) = string(source, text) {
                result.diagnostic(format!(
                    "Cell {} source decode failed: {error}",
                    position + 1
                ));
                valid = false;
                break;
            }
            decode(source, text, &mut decoded, &mut segments, budget)?;
        }
        if !valid {
            continue;
        }
        let id = fields.get("id").and_then(|n| string(*n, text).ok());
        let cell_id = match id {
            Some(id)
                if !id.is_empty()
                    && id.len() <= 4096
                    && !id.contains('\0')
                    && ids.get(&id) == Some(&1) =>
            {
                Some(id)
            }
            Some(_) => {
                result.diagnostic(format!(
                    "Cell {} ID is invalid or duplicated; only current position is retained",
                    position + 1
                ));
                None
            }
            None => None,
        };
        for field in ["cell_type", "id"] {
            if let Some(node) = fields.get(field) {
                let value =
                    string(*node, text).unwrap_or_else(|_| "invalid written metadata".into());
                budget.charge(0, 1, value.len().min(4096), 0)?;
                facts.push(
                    node.start_byte(),
                    node.end_byte(),
                    if field == "id" {
                        "notebook_cell_id"
                    } else {
                        "notebook_cell_type"
                    },
                    "notebook_metadata",
                    &value,
                )?;
            }
        }
        if decoded.is_empty() {
            continue;
        }
        if registry.get(language).is_none() {
            result.diagnostic(format!(
                "Cell {} language has no admitted adapter",
                position + 1
            ));
            continue;
        }
        let map = SourceMap {
            version: 1,
            segments,
        };
        map.validate(&decoded, text)?;
        let original_spans = map
            .original_spans(
                ByteSpan {
                    start: 0,
                    end: decoded.len(),
                },
                &decoded,
                text,
            )?
            .context("notebook source unexpectedly unmapped")?;
        let lines =
            decoded.bytes().filter(|b| *b == b'\n').count() + usize::from(!decoded.ends_with('\n'));
        result.regions.push(LocalRegion {
            id: (position + 1) as u32,
            parent: None,
            profile: registry.effective_profile(path, language)?,
            text: decoded,
            source_map: map,
            location: RegionLocation::NotebookCell {
                cell_id,
                position: position + 1,
                local_start_line: 1,
                local_end_line: lines.max(1),
                original_spans,
            },
        });
    }
    for fact in &mut facts.facts {
        if let Some(code) = &mut fact.code {
            code.producer = "airk-notebook-metadata".into();
            code.producer_version = "composite-facts/1".into();
            code.coverage="Written notebook cell metadata; outputs excluded, execution counts are not identity".into();
        }
    }
    result.facts = facts.facts;
    Ok(result)
}

fn string(node: Node<'_>, text: &str) -> Result<String> {
    ensure!(node.kind() == "string", "expected JSON string");
    Ok(serde_json::from_str(node.utf8_text(text.as_bytes())?)?)
}
fn object<'a>(
    node: Node<'a>,
    text: &str,
    result: &mut NotebookDiscovery,
) -> Result<BTreeMap<String, Node<'a>>> {
    if node.kind() != "object" {
        result.diagnostic("Expected notebook metadata/cell object");
        return Ok(BTreeMap::new());
    }
    let mut fields = BTreeMap::new();
    let mut duplicate = BTreeSet::new();
    let mut cursor = node.walk();
    for pair in node.named_children(&mut cursor) {
        let (Some(key), Some(value)) = (
            pair.child_by_field_name("key"),
            pair.child_by_field_name("value"),
        ) else {
            continue;
        };
        let key = string(key, text)?;
        ensure!(key.len() <= 4096, "notebook key exceeds byte limit");
        if fields.insert(key.clone(), value).is_some() {
            duplicate.insert(key);
        }
    }
    for key in duplicate {
        fields.remove(&key);
        result.diagnostic(format!("Duplicate notebook key {key}; no value selected"));
    }
    Ok(fields)
}
struct Language {
    adapter: Option<&'static str>,
}
fn duplicate_field(node: Node<'_>, text: &str, only: Option<&str>) -> Result<bool> {
    if node.kind() != "object" {
        return Ok(true);
    }
    let mut names = BTreeSet::new();
    let mut cursor = node.walk();
    for pair in node.named_children(&mut cursor) {
        if let Some(key) = pair.child_by_field_name("key") {
            let name = string(key, text)?;
            if only.is_none_or(|only| only == name) && !names.insert(name) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn language(name: &str) -> Option<&'static str> {
    match name.to_ascii_lowercase().as_str() {
        "python" | "python3" => Some("python"),
        "javascript" | "js" => Some("javascript"),
        "typescript" | "ts" => Some("typescript"),
        "julia" => Some("julia"),
        "r" => Some("r"),
        "rust" => Some("rust"),
        "sql" => Some("sql"),
        "bash" | "sh" => Some("bash"),
        "c" => Some("c"),
        "c++" | "cpp" => Some("cpp"),
        _ => None,
    }
}
fn metadata_language(
    node: Option<Node<'_>>,
    text: &str,
    root: bool,
    result: &mut NotebookDiscovery,
) -> Result<Option<Language>> {
    let Some(node) = node else {
        return Ok(None);
    };
    let mut ambiguous = duplicate_field(node, text, None)?;
    let fields = object(node, text, result)?;
    let mut names = Vec::new();
    if let Some(value) = fields.get("language") {
        names.push(string(*value, text).unwrap_or_default());
    }
    for (parent, key) in if root {
        [("language_info", "name"), ("kernelspec", "language")]
    } else {
        [("vscode", "languageId"), ("language_info", "name")]
    } {
        if let Some(parent) = fields.get(parent) {
            ambiguous |= duplicate_field(*parent, text, None)?;
            let child = object(*parent, text, result)?;
            if let Some(value) = child.get(key) {
                names.push(string(*value, text).unwrap_or_default());
            }
        }
    }
    if names.is_empty()
        && root
        && let Some(parent) = fields.get("kernelspec")
    {
        ambiguous |= duplicate_field(*parent, text, None)?;
        let child = object(*parent, text, result)?;
        if let Some(value) = child.get("name") {
            names.push(string(*value, text).unwrap_or_default());
        }
    }
    if ambiguous {
        return Ok(Some(Language { adapter: None }));
    }
    if names.is_empty() {
        return Ok(None);
    }
    let adapters: BTreeSet<_> = names.iter().filter_map(|name| language(name)).collect();
    let adapter = if adapters.len() == 1 && names.iter().all(|name| language(name).is_some()) {
        adapters.first().copied()
    } else {
        result.diagnostic("Notebook language metadata is unsupported or conflicting");
        None
    };
    Ok(Some(Language { adapter }))
}

fn decode(
    node: Node<'_>,
    host: &str,
    text: &mut String,
    segments: &mut Vec<MapSegment>,
    budget: &mut HostBudget,
) -> Result<()> {
    let raw = node.utf8_text(host.as_bytes())?;
    ensure!(
        raw.starts_with('"') && raw.ends_with('"'),
        "invalid source string"
    );
    let mut offset = 1;
    let end = raw.len() - 1;
    while offset < end {
        let start = offset;
        let local = text.len();
        if raw.as_bytes()[offset] == b'\\' {
            offset += 1;
            ensure!(offset < end, "truncated JSON escape");
            if raw.as_bytes()[offset] == b'u' {
                ensure!(offset + 5 <= end, "truncated Unicode escape");
                let high = u16::from_str_radix(&raw[offset + 1..offset + 5], 16)?;
                offset += 5;
                if (0xD800..=0xDBFF).contains(&high) {
                    ensure!(
                        offset + 6 <= end && &raw[offset..offset + 2] == "\\u",
                        "unpaired high surrogate"
                    );
                    offset += 6;
                }
            } else {
                offset += 1;
            }
            let quoted = format!("\"{}\"", &raw[start..offset]);
            let decoded: String = serde_json::from_str(&quoted)?;
            budget.charge(1, 0, decoded.len(), 1)?;
            text.push_str(&decoded);
            segments.push(MapSegment {
                local: ByteSpan {
                    start: local,
                    end: text.len(),
                },
                mapping: Mapping::Decoded {
                    host: ByteSpan {
                        start: node.start_byte() + start,
                        end: node.start_byte() + offset,
                    },
                },
            });
        } else {
            while offset < end && raw.as_bytes()[offset] != b'\\' {
                let character = raw[offset..].chars().next().unwrap();
                ensure!(!character.is_control(), "unescaped JSON control");
                offset += character.len_utf8();
            }
            budget.charge(1, 0, offset - start, 1)?;
            text.push_str(&raw[start..offset]);
            segments.push(MapSegment {
                local: ByteSpan {
                    start: local,
                    end: text.len(),
                },
                mapping: Mapping::Verbatim {
                    host: ByteSpan {
                        start: node.start_byte() + start,
                        end: node.start_byte() + offset,
                    },
                },
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn budget() -> HostBudget {
        HostBudget::new(500_000, 10_000, 16_777_216).unwrap()
    }
    #[test]
    fn original_sources_map_escapes_without_outputs_or_inserted_separators() -> Result<()> {
        let text = include_str!("../tests/fixtures/notebooks/original.ipynb");
        let result = discover("original.ipynb", text, &mut budget())?;
        assert!(!result.partial);
        assert_eq!(result.regions.len(), 2);
        assert_eq!(
            result.regions[1].text,
            "def beacon_value():\r\n    return \"😀\"\n"
        );
        assert_eq!(result.regions[0].profile.adapter_id, "markdown");
        assert_eq!(result.regions[1].profile.adapter_id, "python");
        for region in &result.regions {
            region.source_map.validate(&region.text, text)?;
            region
                .location
                .validate(&region.text, text, &region.source_map)?;
            assert!(!region.text.contains("SENTINEL"));
        }
        assert!(
            !result
                .facts
                .iter()
                .any(|fact| fact.name.contains("SENTINEL") || fact.name == "912")
        );
        let joined = r#"{"nbformat":4,"metadata":{"language_info":{"name":"python"}},"cells":[{"cell_type":"code","source":["be","acon"]}]}"#;
        assert_eq!(
            discover("joined.ipynb", joined, &mut budget())?.regions[0].text,
            "beacon"
        );
        Ok(())
    }
    #[test]
    fn duplicate_ids_metadata_and_bad_surrogates_never_gain_identity() -> Result<()> {
        let duplicate = r#"{"nbformat":4,"metadata":{"language_info":{"name":"python"}},"cells":[{"id":"same","cell_type":"code","source":"x=1"},{"id":"same","cell_type":"code","source":"y=2"}]}"#;
        let result = discover("original.ipynb", duplicate, &mut budget())?;
        assert!(result.partial);
        assert_eq!(result.regions.len(), 2);
        assert!(result.regions.iter().all(|r| matches!(
            &r.location,
            RegionLocation::NotebookCell { cell_id: None, .. }
        )));
        for bad in [
            r#"{"nbformat":4,"metadata":{"language_info":{"name":"python"}},"cells":[{"cell_type":"code","source":"\uD800"}]}"#,
            r#"{"nbformat":4,"metadata":{"language_info":{"name":"python"}},"cells":[{"cell_type":"code","metadata":{"language":"python","language":"rust"},"source":"x=1"}]}"#,
            r#"{"nbformat":4,"metadata":{"language_info":{"name":"python"}},"cells":[{"cell_type":"code","source":"x=1","source":"x=2"}]}"#,
        ] {
            let result = discover("bad.ipynb", bad, &mut budget())?;
            assert!(result.partial);
            assert!(result.regions.is_empty());
        }
        assert!(discover("bad.ipynb", "{broken", &mut budget())?.partial);
        Ok(())
    }
    #[test]
    fn empty_unknown_and_failed_cells_consume_shared_attempt_and_text_budgets() -> Result<()> {
        let cells = vec![serde_json::json!({"cell_type":"markdown","source":[]}); 257];
        let text = serde_json::json!({"nbformat":4,"cells":cells}).to_string();
        assert!(discover("too-many.ipynb", &text, &mut budget()).is_err());
        let source =
            r#"{"nbformat":4,"cells":[{"cell_type":"markdown","source":"original payload"}]}"#;
        assert!(
            discover(
                "limited.ipynb",
                source,
                &mut HostBudget::new(500_000, 10_000, 4)?
            )
            .is_err()
        );
        Ok(())
    }
}
