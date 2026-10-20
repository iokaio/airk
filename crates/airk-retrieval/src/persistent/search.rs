// SPDX-License-Identifier: Apache-2.0
//! Receipt-verified candidate selection, fusion, and final payload materialization.
use super::*;
use crate::{focus_span, terms, truncate_utf8};
use airk_core::{SearchQuery, SearchResponse};
use std::{collections::BTreeSet, sync::Arc};
use tantivy::{
    DocAddress, Order, Term,
    collector::TopDocs,
    query::{BooleanQuery, Occur, Query, RegexQuery, TermQuery},
    schema::{IndexRecordOption, Value},
};

const HYBRID_CANDIDATES: usize = 100;
// Exact scoped comparisons operate on at most this many graph nodes. The path query
// fetches one lookahead; remaining scope is explicitly reported as incomplete.
const MAX_SCOPED_NODES: usize = crate::vector::MAX_SCOPED_VECTOR_READS;

#[derive(Clone)]
struct Candidate {
    // Already persisted in every supported lexical schema; unique across the
    // generation, even when multiple chunks have identical display coordinates.
    ordinal: u64,
    address: Option<DocAddress>,
    window: Option<Arc<vector_map::Occurrence>>,
    semantic_rank: Option<usize>,
}

struct Candidates {
    values: Vec<Candidate>,
    incomplete: bool,
}

impl PersistentIndex {
    pub fn search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        query.validate()?;
        self.verify_retained_components()?;
        self.materialize(self.lexical_candidates(query, query.limit)?, query)
    }

    pub fn semantic_search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        query.validate()?;
        self.verify_retained_components()?;
        self.materialize(self.vector_candidates(query, query.limit)?, query)
    }

    /// Reciprocal-rank fusion operates on identities, never truncated payloads.
    /// Both branches and final materialization use this leased generation.
    pub fn hybrid_search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        query.validate()?;
        self.verify_retained_components()?;
        let lexical = self.lexical_candidates(query, HYBRID_CANDIDATES)?;
        let semantic = self.vector_candidates(query, HYBRID_CANDIDATES)?;
        let incomplete = lexical.incomplete || semantic.incomplete;
        let mut merged = BTreeMap::<u64, (f64, Candidate)>::new();
        for branch in [lexical, semantic] {
            for (rank, candidate) in branch.values.into_iter().enumerate() {
                let score = 1.0 / (61.0 + candidate.semantic_rank.unwrap_or(rank) as f64);
                match merged.entry(candidate.ordinal) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert((score, candidate));
                    }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        entry.get_mut().0 += score;
                        // A semantic tail match must survive an earlier lexical vote.
                        if candidate.window.is_some() {
                            entry.get_mut().1 = candidate;
                        }
                    }
                }
            }
        }
        let mut ranked: Vec<_> = merged.into_values().collect();
        ranked.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then_with(|| a.1.ordinal.cmp(&b.1.ordinal))
        });
        self.materialize(
            Candidates {
                values: ranked.into_iter().map(|(_, candidate)| candidate).collect(),
                incomplete,
            },
            query,
        )
    }

    fn path_query(&self, prefix: &str) -> Result<RegexQuery> {
        Self::path_query_field(prefix, self.fields.path)
    }

    fn path_query_field(prefix: &str, field: Field) -> Result<RegexQuery> {
        let escaped: String = prefix
            .chars()
            .flat_map(|ch| {
                if ".+*?()|[]{}^$\\".contains(ch) {
                    vec!['\\', ch]
                } else {
                    vec![ch]
                }
            })
            .collect();
        Ok(RegexQuery::from_pattern(
            &format!("(?s:{escaped}.*)"),
            field,
        )?)
    }

    fn lexical_candidates(&self, query: &SearchQuery, limit: usize) -> Result<Candidates> {
        let query_terms = terms(&query.text);
        ensure!(
            !query_terms.is_empty(),
            "query must contain a letter or number"
        );
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = query_terms
            .into_iter()
            .map(|term| {
                (
                    Occur::Must,
                    Box::new(TermQuery::new(
                        Term::from_field_text(self.fields.terms, &term),
                        IndexRecordOption::Basic,
                    )) as Box<dyn Query>,
                )
            })
            .collect();
        if let Some(prefix) = query
            .path_prefix
            .as_deref()
            .filter(|prefix| !prefix.is_empty())
        {
            clauses.push((Occur::Must, Box::new(self.path_query(prefix)?)));
        }
        self.ordered_candidates(&BooleanQuery::new(clauses), limit)
    }

    fn ordered_candidates(&self, query: &dyn Query, limit: usize) -> Result<Candidates> {
        let mut matches = self.reader.searcher().search(
            query,
            &TopDocs::with_limit(limit + 1).order_by_u64_field("ordinal", Order::Asc),
        )?;
        let incomplete = matches.len() > limit;
        matches.truncate(limit);
        Ok(Candidates {
            values: matches
                .into_iter()
                .map(|(ordinal, address)| {
                    Ok(Candidate {
                        ordinal: ordinal.context("missing generation chunk ordinal")?,
                        address: Some(address),
                        window: None,
                        semantic_rank: None,
                    })
                })
                .collect::<Result<_>>()?,
            incomplete,
        })
    }

    fn lexical_address(&self, ordinal: u64) -> Result<DocAddress> {
        let candidates = self.ordered_candidates(
            &TermQuery::new(
                Term::from_field_u64(self.fields.ordinal, ordinal),
                IndexRecordOption::Basic,
            ),
            1,
        )?;
        ensure!(
            !candidates.incomplete && candidates.values.len() == 1,
            "vector/lexical ordinal mismatch"
        );
        candidates.values[0]
            .address
            .context("lexical address missing")
    }

    fn occurrence_addresses(
        &self,
        query: &dyn Query,
        limit: usize,
    ) -> Result<(Vec<DocAddress>, bool)> {
        let mut matches = self.map_reader.searcher().search(
            query,
            &TopDocs::with_limit(limit + 1).order_by_u64_field("map_ordinal", Order::Asc),
        )?;
        let incomplete = matches.len() > limit;
        matches.truncate(limit);
        Ok((
            matches.into_iter().map(|(_, address)| address).collect(),
            incomplete,
        ))
    }

    fn vector_candidates(&self, query: &SearchQuery, limit: usize) -> Result<Candidates> {
        ensure!(
            self.model_sha256.is_some(),
            "semantic retrieval not ready: provision a local model and reindex"
        );
        let mut cached = self
            .model
            .lock()
            .map_err(|_| anyhow::anyhow!("local model cache poisoned"))?;
        if cached.is_none() {
            let set = self
                .model_objects
                .as_ref()
                .context("indexed model objects missing; rebuild with airk index --full")?;
            *cached = Some(EmbeddingModel::open_objects(
                &self.collection_directory,
                set,
            )?);
        }
        let model = cached.as_ref().context("local model unavailable")?;
        ensure!(
            Some(model.identity()) == self.model_sha256.as_deref(),
            "indexed model identity mismatch; rebuild the index with airk index --full"
        );
        let vector = model
            .embed_text(&query.text)?
            .context("semantic retrieval not ready: no recognized model tokens")?;
        drop(cached);
        let graph = DiskIndex::open(&self.directory.join("vectors.bin"))?;
        let mut decoded_remaining = vector_map::MAX_DECODED_BYTES;
        let mut members_remaining = vector_map::MAX_POSTINGS;
        let mut incomplete = false;
        let mut selected = BTreeMap::<u64, (f32, usize, Arc<vector_map::Occurrence>)>::new();
        let mut occurrence_rank = 0usize;
        let mut admit = |distance: f32, occurrence: vector_map::Occurrence| -> bool {
            let rank = occurrence_rank;
            occurrence_rank += 1;
            let occurrence = Arc::new(occurrence);
            for member in &occurrence.members {
                if members_remaining == 0 {
                    return false;
                }
                members_remaining -= 1;
                if member.blank {
                    continue;
                }
                selected
                    .entry(member.lexical_ordinal)
                    .or_insert_with(|| (distance, rank, occurrence.clone()));
            }
            true
        };
        let wanted = (limit * 4).clamp(64, 1000);
        if let Some(prefix) = query
            .path_prefix
            .as_deref()
            .filter(|prefix| !prefix.is_empty())
        {
            let (addresses, more) = self.occurrence_addresses(
                &Self::path_query_field(prefix, self.map_fields.path)?,
                vector_map::MAX_POSTINGS,
            )?;
            incomplete |= more;
            let mut scoped = BTreeMap::<u32, Vec<vector_map::Occurrence>>::new();
            let mut retained_members = 0usize;
            for address in addresses {
                let Some((id, occurrence)) = vector_map::read(
                    &self.map_reader,
                    &self.map_fields,
                    address,
                    self.vectors,
                    self.reader.searcher().num_docs(),
                    &mut decoded_remaining,
                )?
                else {
                    incomplete = true;
                    break;
                };
                ensure!(
                    occurrence.path.starts_with(prefix),
                    "vector occurrence path mismatch"
                );
                if (!scoped.contains_key(&id) && scoped.len() == MAX_SCOPED_NODES)
                    || retained_members + occurrence.members.len() > vector_map::MAX_POSTINGS
                {
                    incomplete = true;
                    break;
                }
                retained_members += occurrence.members.len();
                let entries = scoped.entry(id).or_default();
                if entries.len() == vector_map::MAX_NODE_OCCURRENCES {
                    incomplete = true;
                    continue;
                }
                entries.push(occurrence);
            }
            let mut ranked = graph.search_subset(&vector, scoped.keys().copied(), wanted)?;
            ranked.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            incomplete |= ranked.len() < scoped.len();
            for (id, distance) in ranked {
                for occurrence in scoped
                    .remove(&id)
                    .context("scoped vector mapping missing")?
                {
                    if !admit(distance, occurrence) {
                        incomplete = true;
                        break;
                    }
                }
            }
        } else {
            let mut ranked = graph.search(&vector, wanted)?;
            incomplete |= ranked.len() < self.vectors;
            ranked.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            let mut postings_remaining = vector_map::MAX_POSTINGS;
            'graphs: for (id, distance) in ranked {
                if postings_remaining == 0 {
                    incomplete = true;
                    break;
                }
                let (addresses, more) = self.occurrence_addresses(
                    &TermQuery::new(
                        Term::from_field_u64(self.map_fields.graph_id, u64::from(id)),
                        IndexRecordOption::Basic,
                    ),
                    postings_remaining.min(vector_map::MAX_NODE_OCCURRENCES),
                )?;
                incomplete |= more;
                ensure!(!addresses.is_empty(), "graph node has no vector occurrence");
                postings_remaining -= addresses.len();
                for address in addresses {
                    let Some((mapped_id, occurrence)) = vector_map::read(
                        &self.map_reader,
                        &self.map_fields,
                        address,
                        self.vectors,
                        self.reader.searcher().num_docs(),
                        &mut decoded_remaining,
                    )?
                    else {
                        incomplete = true;
                        break 'graphs;
                    };
                    ensure!(mapped_id == id, "vector occurrence graph identity mismatch");
                    if !admit(distance, occurrence) {
                        incomplete = true;
                        break 'graphs;
                    }
                }
            }
        }
        let mut ranked: Vec<_> = selected.into_iter().collect();
        ranked.sort_by(|a, b| {
            a.1.0
                .total_cmp(&b.1.0)
                .then_with(|| a.1.1.cmp(&b.1.1))
                .then_with(|| a.0.cmp(&b.0))
        });
        let mut chosen = BTreeSet::new();
        let mut values = Vec::new();
        for (ordinal, (_, rank, occurrence)) in ranked {
            if !chosen.contains(&occurrence.occurrence_id) && chosen.len() == limit {
                incomplete = true;
                continue;
            }
            chosen.insert(occurrence.occurrence_id);
            values.push(Candidate {
                ordinal,
                address: None,
                window: Some(occurrence),
                semantic_rank: Some(rank),
            });
        }
        Ok(Candidates { values, incomplete })
    }

    // Cached/mapped handles must reject changed bytes just like fresh readers.
    // Never infer authenticity from file length or timestamps. This intentionally
    // pays whole-component verification costs until authenticated block reads
    // are available. Verification is performed once per hybrid request.
    fn verify_retained_components(&self) -> Result<()> {
        let receipt = self
            .v2_receipt
            .as_ref()
            .context("publication receipt missing")?;
        let manifest = verify_directory_v2(&self.directory, receipt, &self.collection)?;
        ensure!(
            manifest.chunks as u64 == self.reader.searcher().num_docs()
                && manifest.vector_occurrences as u64 == self.map_reader.searcher().num_docs()
                && manifest.vector_occurrences == self.vector_occurrences
                && manifest.vector_selection_digest == self.vector_selection_digest
                && manifest.vector_eligible_artifacts == self.vector_eligible_artifacts
                && manifest.vector_excluded_artifacts == self.vector_excluded_artifacts
                && manifest.vectors == self.vectors
                && manifest.model_sha256 == self.model_sha256,
            "cached generation manifest mismatch"
        );
        if let Some(set) = &self.model_objects {
            crate::model::inspect_objects_cache_descriptor(&self.collection_directory, set)?;
        }
        Ok(())
    }

    fn read_hit(&self, address: DocAddress, remaining: &mut usize) -> Result<Option<Hit>> {
        let document: TantivyDocument = self.reader.searcher().doc(address)?;
        let payload = document
            .get_first(self.fields.payload)
            .and_then(|value| value.as_str())
            .context("missing lexical citation")?;
        ensure!(
            payload.len() <= airk_core::extraction::MAX_TEXT_BYTES + (1 << 20),
            "oversized lexical citation"
        );
        if payload.len() > *remaining {
            return Ok(None);
        }
        *remaining -= payload.len();
        Ok(Some(serde_json::from_str(payload)?))
    }

    fn occurrence_body(
        &self,
        occurrence: &vector_map::Occurrence,
        remaining: &mut usize,
    ) -> Result<Option<String>> {
        let mut text = String::with_capacity(occurrence.body.span.end - occurrence.body.span.start);
        let mut cached = None::<(u64, Hit)>;
        for member in &occurrence.members {
            if cached
                .as_ref()
                .is_none_or(|(ordinal, _)| *ordinal != member.lexical_ordinal)
            {
                let Some(hit) =
                    self.read_hit(self.lexical_address(member.lexical_ordinal)?, remaining)?
                else {
                    return Ok(None);
                };
                ensure!(
                    hit.citation.path == occurrence.path
                        && hit.citation.content_hash == occurrence.source_hash.to_string(),
                    "vector body source differs from lexical citation"
                );
                cached = Some((member.lexical_ordinal, hit));
            }
            let hit = &cached.as_ref().context("lexical body unavailable")?.1;
            member.span_in_chunk.validate(&hit.text)?;
            let part = &hit.text[member.span_in_chunk.start..member.span_in_chunk.end];
            ensure!(
                part.trim().is_empty() == member.blank,
                "vector evidence blank marker mismatch"
            );
            text.push_str(part);
        }
        ensure!(
            text.len() == occurrence.body.span.end - occurrence.body.span.start,
            "vector body byte count mismatch"
        );
        Ok(Some(text))
    }

    fn materialize(&self, candidates: Candidates, query: &SearchQuery) -> Result<SearchResponse> {
        let mut response = SearchResponse {
            publication: self.v2_receipt.as_ref().map(|receipt| {
                airk_core::evidence::RevisionEnvelope::new(self.collection.clone(), receipt.token)
            }),
            semantic_readiness: None,
            generation: self.generation,
            collection: self.collection.clone(),
            hits: Vec::new(),
            truncated: candidates.incomplete,
        };
        let mut remaining = query.max_bytes;
        let mut decoded_remaining = 32 << 20;
        let mut occurrences = BTreeSet::new();
        let mut covered_ordinals = BTreeSet::new();
        for candidate in candidates.values {
            if covered_ordinals.contains(&candidate.ordinal) {
                continue;
            }
            if let Some(occurrence) = &candidate.window
                && !occurrences.insert(occurrence.occurrence_id)
            {
                continue;
            }
            if remaining == 0 || response.hits.len() == query.limit {
                response.truncated = true;
                break;
            }
            let mut hit = if let Some(occurrence) = candidate.window {
                let Some(text) = self.occurrence_body(&occurrence, &mut decoded_remaining)? else {
                    response.truncated = true;
                    break;
                };
                let hit = focused_body(&occurrence, &text, &query.text, remaining)?;
                covered_ordinals.extend(
                    occurrence
                        .members
                        .iter()
                        .map(|member| member.lexical_ordinal),
                );
                hit
            } else {
                let Some(hit) = self.read_hit(
                    candidate
                        .address
                        .context("lexical candidate has no address")?,
                    &mut decoded_remaining,
                )?
                else {
                    response.truncated = true;
                    break;
                };
                covered_ordinals.insert(candidate.ordinal);
                hit
            };
            let (text, truncated) = truncate_utf8(&hit.text, remaining);
            hit.text = text;
            hit.truncated |= truncated;
            remaining -= hit.text.len();
            response.truncated |= hit.truncated;
            response.hits.push(hit);
        }
        Ok(response)
    }
}

/// Focus only when the response allowance cannot carry the complete original body.
/// Word/line matching selects an original byte span; repeated input context is absent.
fn focused_body(
    occurrence: &vector_map::Occurrence,
    text: &str,
    query: &str,
    allowance: usize,
) -> Result<Hit> {
    let span = focus_span(text, query, allowance);
    span.validate(text)?;
    ensure!(span.start < span.end, "empty semantic body focus");
    let body = &occurrence.body;
    let mut enclosing_citation = false;
    let (location, start_line, end_line) = if let Some(projection) = &body.projection {
        match projection.focus(text, span)? {
            Some(location) => location,
            None => {
                // Some decoded edges cannot be subdivided. Keep the authenticated
                // enclosing original-body citation and mark this excerpt incomplete.
                enclosing_citation = true;
                (body.location.clone(), body.start_line, body.end_line)
            }
        }
    } else {
        let first = body.start_line
            + text[..span.start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
        let last = first
            + text.as_bytes()[span.start..span.end - 1]
                .iter()
                .filter(|byte| **byte == b'\n')
                .count();
        let location = match &body.location {
            airk_core::DocumentLocation::Lines { .. } => airk_core::DocumentLocation::Lines {
                start: first,
                end: last,
            },
            location => location.clone(),
        };
        (location, first, last)
    };
    Ok(Hit {
        citation: Citation {
            path: occurrence.path.clone(),
            content_hash: occurrence.source_hash.to_string(),
            start_line,
            end_line,
            location,
        },
        text: text[span.start..span.end].into(),
        truncated: enclosing_citation || span.start > 0 || span.end < text.len(),
    })
}
