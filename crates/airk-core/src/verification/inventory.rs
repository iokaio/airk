// SPDX-License-Identifier: Apache-2.0
//! Explicit native evidence shapes only; source/producer text is never interpreted as a locator.
use super::MAX_REFERENCES;
use crate::identity::{RevisionToken, Sha256Digest};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct SavedSourceReference {
    pub path: Option<String>,
    pub original_root: Option<String>,
    pub source_relative_path: Option<String>,
    pub profile_digest: Option<Sha256Digest>,
    pub expected_hash: Option<Sha256Digest>,
    pub pointer: String,
    pub kind: &'static str,
    pub publication: Option<RevisionToken>,
    pub historical: bool,
}

#[derive(Default, Debug)]
pub struct ReferenceInventory {
    pub references: Vec<SavedSourceReference>,
    pub complete: bool,
    pub references_lower_bound: usize,
    bytes: usize,
    visited: usize,
}

impl ReferenceInventory {
    /// Expects the final evidence projection (before status compression). Traverses
    /// typed evidence fields only, with independent node/depth/identity bounds.
    pub fn collect(value: &Value) -> Self {
        let mut inventory = Self {
            complete: true,
            ..Self::default()
        };
        inventory.visit(value, "", None, false, 0);
        inventory
    }

    fn add(
        &mut self,
        value: &Value,
        pointer: String,
        kind: &'static str,
        publication: Option<RevisionToken>,
        historical: bool,
    ) {
        self.references_lower_bound += 1;
        let path = value
            .get("citation_path")
            .or_else(|| value.get("path"))
            .or_else(|| value.as_str().map(|_| value))
            .and_then(Value::as_str);
        let hash = value
            .get("content_hash")
            .or_else(|| value.get("source_hash"))
            .and_then(Value::as_str)
            .and_then(|hash| hash.parse().ok());
        let original_root = value
            .get("root")
            .or_else(|| value.get("source").and_then(|source| source.get("root")))
            .and_then(Value::as_str);
        let relative = original_root
            .and_then(|_| {
                value
                    .get("source")
                    .and_then(|source| source.get("relative_path"))
                    .or_else(|| value.get("path"))
            })
            .and_then(Value::as_str);
        let bytes = path.map_or(0, str::len)
            + original_root.map_or(0, str::len)
            + relative.map_or(0, str::len)
            + pointer.len()
            + 128;
        if self.references.len() == MAX_REFERENCES
            || self.bytes + bytes > 512 * 1024
            || pointer.len() > 4096
            || path.is_some_and(|path| path.len() > 4096)
            || original_root.is_some_and(|root| root.len() > 4096)
            || relative.is_some_and(|path| path.len() > 4096)
        {
            self.complete = false;
            return;
        }
        self.bytes += bytes;
        self.references.push(SavedSourceReference {
            path: path.map(str::to_owned),
            original_root: original_root.map(str::to_owned),
            source_relative_path: relative.map(str::to_owned),
            profile_digest: value
                .get("profile_id")
                .and_then(Value::as_str)
                .and_then(|profile| profile.parse().ok()),
            expected_hash: hash,
            pointer,
            kind,
            publication,
            historical,
        });
    }

    fn visit(
        &mut self,
        value: &Value,
        pointer: &str,
        inherited: Option<RevisionToken>,
        historical: bool,
        depth: usize,
    ) {
        self.visited += 1;
        if depth > 32 || self.visited > 32_768 {
            self.complete = false;
            return;
        }
        let Some(object) = value.as_object() else {
            return;
        };
        let publication = object
            .get("publication")
            .and_then(|publication| {
                publication
                    .get("token")
                    .or_else(|| publication.as_str().map(|_| publication))
            })
            .and_then(Value::as_str)
            .and_then(|token| token.parse().ok())
            .or(inherited);
        if let Some(citation) = object.get("citation") {
            self.add(
                citation,
                format!("{pointer}/citation"),
                "citation",
                publication,
                historical,
            );
        }
        if object.contains_key("path") && object.contains_key("content_hash") {
            self.add(value, pointer.into(), "file_path", publication, historical);
        }
        if let Some(read) = object.get("read").and_then(Value::as_object) {
            let read_publication = read
                .get("publication")
                .and_then(|publication| publication.get("token"))
                .and_then(Value::as_str)
                .and_then(|token| token.parse().ok())
                .or(publication);
            let key = if read.get("resolved_path").is_some_and(Value::is_string) {
                "resolved_path"
            } else {
                "requested_path"
            };
            if let Some(path) = read.get(key).filter(|value| value.is_string()) {
                self.add(
                    path,
                    format!("{pointer}/read/{key}"),
                    "conditional_target",
                    read_publication,
                    read.get("historical")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                );
            }
        }
        for key in ["paths"] {
            if let Some(paths) = object.get(key).and_then(Value::as_array) {
                for (index, path) in paths.iter().enumerate() {
                    self.add(
                        path,
                        format!("{pointer}/{key}/{index}"),
                        "file_path",
                        publication,
                        historical,
                    );
                    if self.references_lower_bound > MAX_REFERENCES {
                        self.complete = false;
                        break;
                    }
                }
            }
        }
        // These are producer-defined rows/containers. Never visit arbitrary
        // attributes, fact statements, code strings, arguments, plans or configs.
        for key in [
            "hits",
            "facts",
            "outline",
            "items",
            "relationships",
            "suggested_targets",
            "definitions",
            "matches",
            "files",
            "events",
            "resolvers",
            "changes",
            "invalidations",
            "validations",
            "suggestions",
        ] {
            if let Some(rows) = object.get(key).and_then(Value::as_array) {
                for (index, row) in rows.iter().enumerate() {
                    let row_pointer = format!("{pointer}/{key}/{index}");
                    if key == "files"
                        && row.get("path").is_some()
                        && row.get("content_hash").is_none()
                    {
                        self.add(row, row_pointer, "file_path", publication, historical);
                    } else {
                        self.visit(row, &row_pointer, publication, historical, depth + 1);
                    }
                    if self.visited > 32_768 || self.references_lower_bound > MAX_REFERENCES {
                        self.complete = false;
                        break;
                    }
                }
            }
        }
        for key in ["before", "after", "from", "to", "current"] {
            if let Some(endpoint) = object
                .get(key)
                .filter(|endpoint| endpoint.get("source_hash").is_some())
            {
                // Historical endpoints cannot authorize current saved-source opens.
                self.add(
                    endpoint,
                    format!("{pointer}/{key}"),
                    "graph_endpoint",
                    publication,
                    true,
                );
            }
        }
        if object.contains_key("selector") && object.contains_key("outcome") {
            for key in ["requested_path", "resolved_path"] {
                if let Some(path) = object.get(key).filter(|path| path.is_string()) {
                    self.add(
                        path,
                        format!("{pointer}/{key}"),
                        "unsupported_file_reference",
                        publication,
                        true,
                    );
                }
            }
            if let Some(selector) = object
                .get("selector")
                .filter(|selector| selector.get("path").is_some())
            {
                self.add(
                    selector,
                    format!("{pointer}/selector"),
                    "unsupported_file_reference",
                    publication,
                    true,
                );
            }
            if let Some(candidates) = object.get("candidates").and_then(Value::as_array) {
                for (index, candidate) in candidates.iter().enumerate() {
                    self.add(
                        candidate,
                        format!("{pointer}/candidates/{index}"),
                        "graph_endpoint",
                        publication,
                        true,
                    );
                    if self.references_lower_bound > MAX_REFERENCES {
                        self.complete = false;
                        break;
                    }
                }
            }
        }
        if object.get("kind").and_then(Value::as_str) == Some("directory")
            && object.get("root").is_some()
            && let Some(children) = object.get("children").and_then(Value::as_array)
        {
            for (index, pair) in children.iter().enumerate() {
                for endpoint in 0..2 {
                    if let Some(path) = pair.get(endpoint).filter(|path| path.is_string()) {
                        self.add(
                            path,
                            format!("{pointer}/children/{index}/{endpoint}"),
                            "unsupported_file_reference",
                            publication,
                            true,
                        );
                    }
                }
                if self.references_lower_bound > MAX_REFERENCES {
                    self.complete = false;
                    break;
                }
            }
        }
        for key in ["minimal", "page", "evidence"] {
            if let Some(child) = object.get(key) {
                if let Some(rows) = child.as_array() {
                    for (index, row) in rows.iter().enumerate() {
                        self.visit(
                            row,
                            &format!("{pointer}/{key}/{index}"),
                            publication,
                            historical,
                            depth + 1,
                        );
                        if self.visited > 32_768 || self.references_lower_bound > MAX_REFERENCES {
                            self.complete = false;
                            break;
                        }
                    }
                } else {
                    self.visit(
                        child,
                        &format!("{pointer}/{key}"),
                        publication,
                        historical,
                        depth + 1,
                    );
                }
            }
        }
        if let Some(steps) = object.get("steps").and_then(Value::as_array) {
            for (index, step) in steps.iter().enumerate() {
                if let Some(result) = step.get("result") {
                    self.visit(
                        result,
                        &format!("{pointer}/steps/{index}/result"),
                        publication,
                        historical,
                        depth + 1,
                    );
                }
                if self.visited > 32_768 || self.references_lower_bound > MAX_REFERENCES {
                    self.complete = false;
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn inventory_covers_supplements_compact_conditional_and_steps_without_reading_source_text() {
        let hash = "a".repeat(64);
        let payload = json!({"hits":[{"citation":{"path":"src/a.rs","content_hash":hash}}],
            "definitions":[{"matches":[{"citation":{"path":"src/b.rs","content_hash":hash}}]}],
            "minimal":{"files":[{"path":"src/c.rs","content_hash":hash}]},
            "read":{"resolved_path":"src/d.rs","status":"not_modified"},
            "steps":[{"arguments":{"path":"do-not-open"},"result":{"paths":["src/e.rs"]}}],
            "facts":[{"statement":"{path: secret}","attributes":{"path":"do-not-open","content_hash":hash}}]});
        let inventory = ReferenceInventory::collect(&payload);
        assert!(inventory.complete);
        assert_eq!(inventory.references.len(), 5);
        let paths: Vec<_> = inventory
            .references
            .iter()
            .filter_map(|reference| reference.path.as_deref())
            .collect();
        assert_eq!(
            paths,
            vec!["src/d.rs", "src/a.rs", "src/b.rs", "src/c.rs", "src/e.rs"]
        );
        for reference in inventory.references {
            assert!(payload.pointer(&reference.pointer).is_some());
        }
    }

    #[test]
    fn inventory_bounds_and_unresolvable_file_references_never_become_a_complete_subset() {
        let payload = json!({"paths":vec!["src/a.rs";MAX_REFERENCES+1]});
        let inventory = ReferenceInventory::collect(&payload);
        assert!(!inventory.complete);
        assert!(inventory.references.len() <= MAX_REFERENCES);
        assert!(inventory.references.len() < inventory.references_lower_bound);
        assert_eq!(inventory.references_lower_bound, MAX_REFERENCES + 1);
        let unknown = ReferenceInventory::collect(&json!({"hits":[{"citation":{"start_line":1}}]}));
        assert_eq!(unknown.references.len(), 1);
        assert!(unknown.references[0].path.is_none());
        assert!(unknown.references[0].expected_hash.is_none());
    }
}
