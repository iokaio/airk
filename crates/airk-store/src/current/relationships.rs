// SPDX-License-Identifier: Apache-2.0
//! Native relationship traversal over the exact current source/target postings.
use super::{CurrentRead, CurrentToken, discovery::fact_hit, postings::Lookup};
use crate::FileStore;
use airk_core::{Artifact, EvidenceBasis, RelationshipHit, RelationshipQuery};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::collections::BTreeSet;

const MAX_FILES: usize = 256;
const MAX_SYMBOLS: usize = 100;
const MAX_EVIDENCE_BYTES: usize = 256 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct CurrentRelationships {
    pub revision: CurrentToken,
    pub collection: String,
    pub relationships: Vec<RelationshipHit>,
    pub truncated: bool,
    pub coverage: &'static str,
}

fn edges(artifact: &Artifact) -> Vec<RelationshipHit> {
    let mut edges = Vec::new();
    for fact in &artifact.facts {
        let Some(code) = &fact.code else { continue };
        let citation = fact_hit(artifact, fact).citation;
        if let Some(owner) = &code.owner_id {
            edges.push(RelationshipHit {
                source: owner.clone(),
                target: code.symbol_id.clone(),
                kind: "contains".into(),
                resolution: "syntax_candidate".into(),
                basis: EvidenceBasis::Syntax,
                producer: "airk-syntax-containment".into(),
                producer_version: "native-facts/4".into(),
                citation: citation.clone(),
            });
        }
        if let Some(target) = &code.target {
            edges.push(RelationshipHit {
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
            });
        }
    }
    edges.sort_by(|a, b| {
        (a.citation.start_line, &a.source, &a.target, &a.kind).cmp(&(
            b.citation.start_line,
            &b.source,
            &b.target,
            &b.kind,
        ))
    });
    edges
}

impl CurrentRead {
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
        let imported = self.analysis(store)?;
        let mut response = CurrentRelationships {
            revision: self.token()?,
            collection: self.head().scope.collection.clone(),
            relationships: Vec::new(),
            truncated: false,
            coverage: "Native syntax candidates and source-bound producer assertions. Written targets and imported resolutions are not compiler-certified. Impact is incomplete.",
        };
        let incoming = query.direction == "incoming";
        let mut pending = BTreeSet::from([query.symbol.clone()]);
        let mut visited = BTreeSet::new();
        let mut returned = BTreeSet::new();
        let mut files = 0;
        let mut work = self.resource_reference("analysis")?.map_or(0, |r| r.bytes);
        let mut remaining = query.max_bytes;
        for _ in 0..query.hops {
            let mut next = BTreeSet::new();
            for symbol in pending {
                if visited.contains(&symbol) {
                    continue;
                }
                if visited.len() == MAX_SYMBOLS || files == MAX_FILES {
                    response.truncated = true;
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
                    let bytes = compact
                        .bytes
                        .checked_add(retained.object_bytes)
                        .context("current relationship evidence size overflow")?;
                    if bytes > MAX_EVIDENCE_BYTES.saturating_sub(work) {
                        response.truncated = true;
                        return Ok(response);
                    }
                    files += 1;
                    work += bytes;
                    let artifact =
                        store.load_retained_bounded(&compact, retained.object_bytes as u64)?;
                    store.verify_retained_source(&compact)?;
                    let mut candidates = edges(&artifact);
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
        Ok(response)
    }
}
