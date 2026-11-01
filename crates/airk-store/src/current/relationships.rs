// SPDX-License-Identifier: Apache-2.0
//! Native relationship traversal over the exact current source/target postings.
use super::{
    CurrentRead, CurrentToken,
    discovery::{fact_hit, require_integrity},
    postings::Lookup,
};
use crate::FileStore;
use airk_core::{Artifact, EvidenceBasis, RelationshipHit, RelationshipQuery};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::collections::BTreeSet;

mod navigation;

const MAX_FILES: usize = 256;
const MAX_SYMBOLS: usize = 100;
const MAX_EVIDENCE_BYTES: usize = 256 * 1024 * 1024;
const MAX_NAVIGATION_EVIDENCE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct CurrentRelationships {
    pub revision: CurrentToken,
    pub collection: String,
    pub relationships: Vec<RelationshipHit>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suggested_targets: Vec<airk_core::RelationshipTargetSuggestion>,
    pub truncated: bool,
    pub coverage: &'static str,
}

pub(crate) fn visit_edges(
    artifact: &Artifact,
    mut visit: impl FnMut(RelationshipHit) -> Result<()>,
) -> Result<()> {
    for fact in &artifact.facts {
        let Some(code) = &fact.code else { continue };
        let citation = fact_hit(artifact, fact).citation;
        if let Some(owner) = &code.owner_id {
            visit(RelationshipHit {
                source: owner.clone(),
                target: code.symbol_id.clone(),
                kind: "contains".into(),
                resolution: "syntax_candidate".into(),
                basis: EvidenceBasis::Syntax,
                producer: "airk-syntax-containment".into(),
                producer_version: "native-facts/4".into(),
                citation: citation.clone(),
            })?;
        }
        if let Some(target) = &code.target {
            visit(RelationshipHit {
                source: code
                    .owner_id
                    .clone()
                    .unwrap_or_else(|| format!("file:{}", artifact.path)),
                target: target.clone(),
                kind: fact.kind.clone(),
                resolution: "syntax_candidate".into(),
                basis: code.basis.clone(),
                producer: code.producer.clone(),
                producer_version: code.producer_version.clone(),
                citation,
            })?;
        }
    }
    Ok(())
}
pub(crate) fn sort_edges(edges: &mut [RelationshipHit]) {
    edges.sort_by(|a, b| {
        (a.citation.start_line, &a.source, &a.target, &a.kind).cmp(&(
            b.citation.start_line,
            &b.source,
            &b.target,
            &b.kind,
        ))
    });
}
fn edges(artifact: &Artifact) -> Vec<RelationshipHit> {
    let mut edges = Vec::new();
    visit_edges(artifact, |edge| {
        edges.push(edge);
        Ok(())
    })
    .expect("infallible edge collector");
    sort_edges(&mut edges);
    edges
}

impl CurrentRead {
    fn type_navigation(
        &self,
        store: &FileStore,
        query: &RelationshipQuery,
        private: bool,
        files: &mut usize,
        work: &mut usize,
    ) -> Result<navigation::Navigation> {
        if !query.symbol.starts_with("syntax:")
            || ![":struct_item", ":enum_item", ":impl_item"]
                .iter()
                .any(|kind| query.symbol.ends_with(kind))
        {
            return Ok(Default::default());
        }
        // Resolve the exact returned symbol ID through authenticated postings;
        // do not interpret a caller-supplied path inside the opaque symbol ID.
        let paths =
            self.lookup_paths(Lookup::FactName(&query.symbol), &query.path_prefix, None, 2)?;
        if paths.paths.len() != 1 || paths.next_after.is_some() {
            return Ok(navigation::Navigation {
                truncated: !paths.paths.is_empty(),
                ..Default::default()
            });
        }
        let path = &paths.paths[0];
        let compact = self
            .artifact(path)?
            .context("type navigation artifact missing")?;
        if compact.language != "rust" {
            return Ok(Default::default());
        }
        let retained = compact
            .retained
            .as_ref()
            .context("type navigation extraction missing")?;
        let bytes = compact
            .bytes
            .checked_add(retained.object_bytes)
            .context("type navigation evidence size overflow")?;
        if bytes > MAX_NAVIGATION_EVIDENCE_BYTES
            || bytes > MAX_EVIDENCE_BYTES.saturating_sub(*work)
            || *files == MAX_FILES
        {
            return Ok(navigation::Navigation {
                truncated: true,
                ..Default::default()
            });
        }
        *files += 1;
        *work += bytes;
        let result = (|| {
            let artifact = store.load_retained_bounded(&compact, retained.object_bytes as u64)?;
            store.verify_retained_source(&compact)?;
            let mut navigation = navigation::derive(&artifact, &query.symbol);
            if artifact.facts.len() <= navigation::MAX_FACTS {
                navigation.verified = Some(navigation::VerifiedSource {
                    revision: self.token()?,
                    source_hash: compact.content_hash.clone(),
                    edges: edges(&artifact),
                });
            }
            Ok(navigation)
        })();
        if private {
            self.observe_private_integrity(result)
        } else {
            result
        }
    }

    /// Every candidate is reconstructed from retained native extraction under
    /// this request's gate. Budgets cap graph fan-out and source verification;
    /// omitted traversal is reported, never interpreted as absence.
    pub fn native_relationships(
        &self,
        store: &FileStore,
        query: &RelationshipQuery,
    ) -> Result<CurrentRelationships> {
        airk_core::validate_budget(query.limit, query.max_bytes)?;
        ensure!(
            !query.symbol.is_empty()
                && query.symbol.len() <= 4096
                && matches!(query.direction.as_str(), "incoming" | "outgoing")
                && (1..=3).contains(&query.hops)
                && query.path_prefix.len() <= 4096
                && query.kind.as_ref().is_none_or(|kind| kind.len() <= 128),
            "invalid current relationship query"
        );
        let private = self.private_query_eligible()?;
        let projected_import = if private {
            self.observe_private_integrity(self.imported_projection(store))?
        } else {
            None
        };
        let imported = if projected_import.is_none() {
            self.analysis(store)?
        } else {
            None
        };
        let mut response = CurrentRelationships {
            revision: self.token()?,
            collection: self.head().scope.collection.clone(),
            relationships: Vec::new(),
            suggested_targets: Vec::new(),
            truncated: false,
            coverage: "Native syntax candidates and source-bound producer assertions. Written targets and imported resolutions are not compiler-certified. Type navigation uses only unambiguous same-file retained Rust declarations; aliases, unsupported generic headers and external impls may be absent. Impact is incomplete.",
        };
        let incoming = query.direction == "incoming";
        let mut pending = BTreeSet::from([query.symbol.clone()]);
        let mut visited = BTreeSet::new();
        let mut returned = BTreeSet::new();
        let mut files = 0;
        let mut work = self.resource_reference("analysis")?.map_or(0, |r| r.bytes);
        let mut remaining = query.max_bytes;
        let navigation = self.type_navigation(store, query, private, &mut files, &mut work)?;
        response.truncated |= navigation.truncated;
        for _ in 0..query.hops {
            let mut next = BTreeSet::new();
            for symbol in pending {
                if visited.contains(&symbol) {
                    continue;
                }
                if visited.len() == MAX_SYMBOLS || files == MAX_FILES {
                    response.truncated = true;
                    self.token()?;
                    return Ok(response);
                }
                visited.insert(symbol.clone());
                let lookup = if incoming {
                    Lookup::EdgeTarget(&symbol)
                } else {
                    Lookup::EdgeSource(&symbol)
                };
                let page =
                    self.lookup_paths(lookup, &query.path_prefix, None, MAX_FILES - files)?;
                response.truncated |= page.next_after.is_some();
                let mut paths = page.paths;
                if symbol == query.symbol && !navigation.path.is_empty() {
                    paths.push(navigation.path.clone());
                }
                if let Some(imported) = &imported {
                    paths.extend(
                        imported
                            .relationships
                            .iter()
                            .filter(|e| {
                                e.citation.path.starts_with(&query.path_prefix)
                                    && (if incoming { &e.target } else { &e.source }) == &symbol
                            })
                            .map(|e| e.citation.path.clone()),
                    );
                }
                let selected_imports = projected_import
                    .as_ref()
                    .map(|projection| {
                        self.observe_private_integrity(projection.lookup_paths(
                            lookup,
                            &query.path_prefix,
                            None,
                            MAX_FILES - files + 1,
                        ))
                    })
                    .transpose()?
                    .unwrap_or_default();
                paths.extend(
                    selected_imports
                        .iter()
                        .map(|selected| selected.path.clone()),
                );
                paths.sort();
                paths.dedup();
                if paths.len() > MAX_FILES - files {
                    paths.truncate(MAX_FILES - files);
                    response.truncated = true;
                }
                for path in paths {
                    let compact = self
                        .artifact(&path)?
                        .context("current relationship artifact missing")?;
                    let retained = compact
                        .retained
                        .as_ref()
                        .context("current relationship extraction missing")?;
                    let verified = navigation
                        .verified
                        .as_ref()
                        .filter(|_| path == navigation.path);
                    if let Some(verified) = verified {
                        require_integrity(
                            self,
                            verified.revision == self.token()?
                                && verified.source_hash == compact.content_hash,
                            "request-local type navigation source expired",
                        )?;
                    }
                    // Source proof and retained decoding were already charged
                    // once for this exact path/hash/publication. Reuse only
                    // inside this CurrentRead and this request; projection
                    // validation below still runs normally.
                    let bytes = if verified.is_some() {
                        0
                    } else {
                        compact
                            .bytes
                            .checked_add(retained.object_bytes)
                            .context("current relationship evidence size overflow")?
                    };
                    if bytes > MAX_EVIDENCE_BYTES.saturating_sub(work) {
                        response.truncated = true;
                        self.token()?;
                        return Ok(response);
                    }
                    files += 1;
                    work += bytes;
                    let projection = if private {
                        self.observe_private_integrity(self.native_edge_projection(store, &path))?
                    } else {
                        None
                    };
                    let mut candidates = if let Some(projection) = &projection {
                        let selected = self.observe_private_integrity(
                            projection.lookup_paths(lookup, &path, None, 1),
                        )?;
                        let mut rows = std::collections::BTreeMap::new();
                        if let Some(selected) =
                            selected.first().filter(|selected| selected.path == path)
                        {
                            require_integrity(
                                self,
                                selected.source.to_string() == compact.content_hash,
                                "native relationship source expired",
                            )?;
                            for page in &selected.pages {
                                for row in self.observe_private_integrity(projection.page(*page))? {
                                    if let super::query_projection::ProjectionRow::Edge {
                                        ordinal,
                                        hit,
                                    } = row
                                    {
                                        require_integrity(
                                            self,
                                            hit.citation.path == path
                                                && hit.citation.content_hash
                                                    == compact.content_hash,
                                            "native relationship source expired",
                                        )?;
                                        rows.insert(ordinal, *hit);
                                    }
                                }
                            }
                        }
                        rows.into_values().collect()
                    } else if private && self.native_edges_declared_empty(&path)? {
                        Vec::new()
                    } else if let Some(verified) = verified {
                        verified
                            .edges
                            .iter()
                            .filter(|edge| {
                                (if incoming { &edge.target } else { &edge.source }) == &symbol
                            })
                            .cloned()
                            .collect()
                    } else {
                        let result = (|| {
                            let artifact = store
                                .load_retained_bounded(&compact, retained.object_bytes as u64)?;
                            store.verify_retained_source(&compact)?;
                            Ok(edges(&artifact))
                        })();
                        if private {
                            self.observe_private_integrity(result)?
                        } else {
                            result?
                        }
                    };
                    if let Some(projection) = &projected_import
                        && let Some(selected) = selected_imports
                            .iter()
                            .find(|selected| selected.path == path)
                    {
                        require_integrity(
                            self,
                            selected.source.to_string() == compact.content_hash,
                            "current analysis source expired",
                        )?;
                        let mut rows = std::collections::BTreeMap::new();
                        for page in &selected.pages {
                            for row in self
                                .observe_private_integrity(projection.query_page(*page))?
                                .iter()
                            {
                                if let super::query_projection::ProjectionRow::Edge { ordinal, hit } =
                                    row
                                    && hit.citation.path == path
                                {
                                    require_integrity(
                                        self,
                                        hit.citation.content_hash == compact.content_hash,
                                        "current analysis source expired",
                                    )?;
                                    rows.insert(*ordinal, hit.as_ref().clone());
                                }
                            }
                        }
                        candidates.extend(rows.into_values());
                    }
                    if let Some(imported) = &imported {
                        self.verify_analysis_path(imported, &path)?;
                        candidates.extend(
                            imported
                                .relationships
                                .iter()
                                .filter(|e| e.citation.path == path)
                                .cloned(),
                        );
                    }
                    if symbol == query.symbol && path == navigation.path && !incoming {
                        candidates.extend(navigation.contains.iter().cloned());
                    }
                    for edge in candidates {
                        if (if incoming { &edge.target } else { &edge.source }) != &symbol
                            || query.kind.as_ref().is_some_and(|kind| kind != &edge.kind)
                        {
                            continue;
                        }
                        let encoded = serde_json::to_vec(&edge)?;
                        if returned.contains(&encoded) {
                            continue;
                        }
                        if response.relationships.len() == query.limit || encoded.len() > remaining
                        {
                            response.truncated = true;
                            self.token()?;
                            return Ok(response);
                        }
                        remaining -= encoded.len();
                        returned.insert(encoded);
                        let endpoint = if incoming { &edge.source } else { &edge.target };
                        if !visited.contains(endpoint) && !next.contains(endpoint) {
                            if next.len() == MAX_SYMBOLS {
                                response.truncated = true;
                            } else {
                                next.insert(endpoint.clone());
                            }
                        }
                        response.relationships.push(edge);
                    }
                }
            }
            pending = next;
            if pending.is_empty() {
                break;
            }
        }
        for suggestion in navigation.suggestions {
            let bytes = serde_json::to_vec(&suggestion)?.len();
            if response.relationships.len() + response.suggested_targets.len() == query.limit
                || bytes > remaining
            {
                response.truncated = true;
                break;
            }
            remaining -= bytes;
            response.suggested_targets.push(suggestion);
        }
        self.token()?;
        Ok(response)
    }
}
