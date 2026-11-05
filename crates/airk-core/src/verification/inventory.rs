// SPDX-License-Identifier: Apache-2.0
//! Explicit native evidence shapes only; source/producer text is never interpreted as a locator.
use super::MAX_REFERENCES;
use crate::identity::{RevisionToken, Sha256Digest};
use crate::index_catalog::{IndexId, QueryProjectId, SourceRootId};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct SavedSourceReference {
    /// Set only by an explicit combined-member container, never source text.
    pub index_id: Option<IndexId>,
    pub source_root_id: Option<SourceRootId>,
    pub project_ids: Vec<QueryProjectId>,
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
    /// Combined evidence must be checked by its originating member owners.
    pub contains_combined: bool,
    bytes: usize,
    visited: usize,
    member_index: Option<IndexId>,
    member_origin: Option<ReferenceOrigin>,
}

/// Structural native origin only. The inventory never parses producer text or
/// guesses an original root from a virtual path/alias.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReferenceOrigin {
    index_id: IndexId,
    source_root_id: SourceRootId,
    project_ids: Vec<QueryProjectId>,
    source_relative_path: String,
    publication: crate::evidence::RevisionEnvelope,
}
impl ReferenceOrigin {
    fn valid(&self) -> bool {
        !self.project_ids.is_empty()
            && crate::index_catalog::unique_bounded(&self.project_ids, 32).is_ok()
            && crate::index_catalog::relative_path(&self.source_relative_path, false).is_ok()
            && self.publication.incarnation == self.publication.token.incarnation
            && self.publication.generation == self.publication.token.generation
            && self.publication.revision == self.publication.token.revision
            && !self.publication.collection.is_empty()
            && self.publication.collection.len() <= 128
    }
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
        let relative = self
            .member_origin
            .as_ref()
            .map(|origin| origin.source_relative_path.as_str())
            .or_else(|| {
                original_root
                    .and_then(|_| {
                        value
                            .get("source")
                            .and_then(|source| source.get("relative_path"))
                            .or_else(|| value.get("path"))
                    })
                    .and_then(Value::as_str)
            });
        let bytes = path.map_or(0, str::len)
            + original_root.map_or(0, str::len)
            + relative.map_or(0, str::len)
            + pointer.len()
            + self
                .member_origin
                .as_ref()
                .map_or(0, |origin| 72 + 36 * origin.project_ids.len())
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
            index_id: self
                .member_origin
                .as_ref()
                .map(|origin| origin.index_id)
                .or(self.member_index),
            source_root_id: self
                .member_origin
                .as_ref()
                .map(|origin| origin.source_root_id),
            project_ids: self
                .member_origin
                .as_ref()
                .map_or_else(Vec::new, |origin| origin.project_ids.clone()),
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
            publication: self
                .member_origin
                .as_ref()
                .map(|origin| origin.publication.token)
                .or(publication),
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
        if object.get("schema").and_then(Value::as_str) == Some("airk.combined-read/1") {
            self.contains_combined = true;
            let Some(members) = object.get("members").and_then(Value::as_array) else {
                self.complete = false;
                return;
            };
            let mut indexes = std::collections::BTreeSet::new();
            if members.len() > crate::index_catalog::MAX_SELECTION {
                self.complete = false;
            }
            for (index, member) in members
                .iter()
                .take(crate::index_catalog::MAX_SELECTION)
                .enumerate()
            {
                let member_index = member
                    .get("index_id")
                    .and_then(Value::as_str)
                    .and_then(|id| id.parse::<IndexId>().ok());
                let Some(member_index) = member_index.filter(|id| indexes.insert(*id)) else {
                    self.complete = false;
                    continue;
                };
                if let Some(result) = member.get("result").filter(|result| !result.is_null()) {
                    let previous = self.member_index.replace(member_index);
                    let materialized =
                        crate::combined_reads::shared_values::materialize_combined_member_result(
                            value,
                            member_index,
                        );
                    match materialized {
                        Ok(logical) => self.visit(
                            &logical,
                            &format!("{pointer}/members/{index}/result"),
                            None,
                            historical,
                            depth + 1,
                        ),
                        Err(_) => {
                            // Preserve honest known inline locator lower bounds,
                            // but never call a malformed header a complete page or
                            // substitute its missing publication from another row.
                            self.complete = false;
                            self.visit(
                                result,
                                &format!("{pointer}/members/{index}/result"),
                                None,
                                historical,
                                depth + 1,
                            );
                        }
                    }
                    self.member_index = previous;
                }
            }
            if let Some(ranking) = object.get("ranking").and_then(Value::as_array) {
                if ranking.len() > 100 {
                    self.complete = false;
                }
                for rank in ranking.iter().take(100) {
                    if crate::combined_reads::materialize_ranked_origin(value, rank).is_err() {
                        self.complete = false;
                    }
                }
            }
            // Ranking references point into these same retained native bodies;
            // visiting them as evidence would count and check sources twice.
            return;
        }
        if let Some(body) = object.get("structuredContent") {
            self.visit(
                body,
                &format!("{pointer}/structuredContent"),
                inherited,
                historical,
                depth + 1,
            );
        }
        let previous_origin = self.member_origin.clone();
        if object.contains_key("citation")
            && let Some(raw_origin) = object.get("origin")
        {
            self.contains_combined = true;
            match serde_json::from_value::<ReferenceOrigin>(raw_origin.clone()) {
                Ok(origin)
                    if origin.valid()
                        && self
                            .member_index
                            .is_none_or(|index| index == origin.index_id) =>
                {
                    if let Some(publication) = object.get("publication") {
                        let token = publication
                            .get("token")
                            .or_else(|| publication.as_str().map(|_| publication))
                            .and_then(Value::as_str)
                            .and_then(|token| token.parse::<RevisionToken>().ok());
                        if token != Some(origin.publication.token) {
                            self.complete = false;
                        }
                    }
                    self.member_origin = Some(origin);
                }
                _ => self.complete = false,
            }
        }
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
        self.member_origin = previous_origin;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn combined_inventory_preserves_member_identity_and_counts_retained_evidence_once() {
        let index_id = IndexId::from_identity(crate::identity::Incarnation::from_bytes([1; 16]));
        let other_id = IndexId::from_identity(crate::identity::Incarnation::from_bytes([2; 16]));
        let publication = crate::evidence::RevisionEnvelope::new(
            "fixture".into(),
            RevisionToken {
                incarnation: crate::identity::Incarnation::from_bytes([3; 16]),
                generation: 1,
                revision: Sha256Digest::from_bytes([4; 32]),
            },
        );
        let member = json!({"content":[],"structuredContent":{"hits":[{"citation":{
            "path":"same.rs","content_hash":"a".repeat(64)}}],"publication":publication}});
        let value = json!({"schema":"airk.combined-read/1","members":[
            {"index_id":index_id,"result":member.clone()},
            {"index_id":other_id,"result":member}],
            "publication_vector":{"members":[{"index_id":index_id,"publication":publication},{"index_id":other_id,"publication":publication}]},
            "ranking":[{"index_id":index_id,"result_pointer":"/members/0/result/structuredContent/hits/0",
                "origin":{"index_id":index_id,"source_root_id":SourceRootId::from_identity(crate::identity::Incarnation::from_bytes([5;16])),
                    "project_ids":[QueryProjectId::from_identity(crate::identity::Incarnation::from_bytes([6;16]))],"source_relative_path":"same.rs","publication":publication}}]});
        let inventory = ReferenceInventory::collect(&value);
        assert!(inventory.complete && inventory.contains_combined);
        assert_eq!(inventory.references.len(), 2);
        assert_eq!(inventory.references[0].index_id, Some(index_id));
        assert_eq!(inventory.references[1].index_id, Some(other_id));
        assert_ne!(
            inventory.references[0].pointer,
            inventory.references[1].pointer
        );
        for reference in &inventory.references {
            assert!(value.pointer(&reference.pointer).is_some());
        }
        let mut duplicate = value.clone();
        duplicate["members"][1]["index_id"] = json!(index_id);
        assert!(!ReferenceInventory::collect(&duplicate).complete);
        let untrusted_text = json!({"facts":[{"statement":value.to_string()}]});
        assert!(!ReferenceInventory::collect(&untrusted_text).contains_combined);
    }

    #[test]
    fn flattened_member_origin_retains_exact_identity_without_guessing_native_roots() {
        use crate::{
            identity::Incarnation,
            index_catalog::{QueryProjectId, SourceRootId},
        };
        let index = IndexId::from_identity(Incarnation::from_bytes([1; 16]));
        let source = SourceRootId::from_identity(Incarnation::from_bytes([2; 16]));
        let project = QueryProjectId::from_identity(Incarnation::from_bytes([3; 16]));
        let token = RevisionToken {
            incarnation: Incarnation::from_bytes([4; 16]),
            generation: 7,
            revision: Sha256Digest::from_bytes([5; 32]),
        };
        let origin = json!({"index_id":index,"source_root_id":source,"project_ids":[project],"source_relative_path":"public/a.rs",
            "publication":crate::evidence::RevisionEnvelope::new("fixture".into(),token)});
        let mut value = json!({"evidence":[{"citation":{"path":"@docs/public/a.rs","content_hash":"ab".repeat(32)},"origin":origin}]});
        let inventory = ReferenceInventory::collect(&value);
        assert!(inventory.complete && inventory.contains_combined);
        assert_eq!(inventory.references.len(), 1);
        let reference = &inventory.references[0];
        assert_eq!(reference.index_id, Some(index));
        assert_eq!(reference.source_root_id, Some(source));
        assert_eq!(reference.project_ids, vec![project]);
        assert_eq!(reference.publication, Some(token));
        assert_eq!(
            reference.source_relative_path.as_deref(),
            Some("public/a.rs")
        );
        assert!(
            reference.original_root.is_none(),
            "an alias spelling is not an original root"
        );
        value["evidence"][0]["origin"]["source_relative_path"] = json!("../escape.rs");
        assert!(!ReferenceInventory::collect(&value).complete);
        value["evidence"][0]["origin"]["source_relative_path"] = json!("public/a.rs");
        value["evidence"][0]["origin"]["publication"]["generation"] = json!(8);
        assert!(
            !ReferenceInventory::collect(&value).complete,
            "token and expanded identity must agree"
        );
        let source_text = json!({"facts":[{"statement":origin,"attributes":{"origin":origin}}]});
        assert!(!ReferenceInventory::collect(&source_text).contains_combined);
    }

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
