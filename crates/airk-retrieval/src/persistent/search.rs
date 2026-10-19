// SPDX-License-Identifier: Apache-2.0
//! Receipt-verified candidate selection, fusion, and final payload materialization.
use super::*;
use crate::{terms, truncate_utf8};
use airk_core::{SearchQuery, SearchResponse};
use tantivy::{
    DocAddress, Order, Term,
    collector::TopDocs,
    query::{BooleanQuery, Occur, Query, RegexQuery, TermQuery},
    schema::{IndexRecordOption, Value},
};

const HYBRID_CANDIDATES: usize = 100;
// Exact scoped comparisons operate on at most this many chunks. The path query
// fetches one lookahead; remaining scope is explicitly reported as incomplete.
const MAX_SCOPED_CHUNKS: usize = crate::vector::MAX_SCOPED_VECTOR_READS;

#[derive(Clone, Copy)]
struct Candidate {
    // Already persisted in every supported lexical schema; unique across the
    // generation, even when multiple chunks have identical display coordinates.
    ordinal: u64,
    address: DocAddress,
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
                merged
                    .entry(candidate.ordinal)
                    .or_insert((0.0, candidate))
                    .0 += 1.0 / (61.0 + rank as f64);
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
            self.fields.path,
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
                        address,
                    })
                })
                .collect::<Result<_>>()?,
            incomplete,
        })
    }

    fn vector_candidate(&self, id: u32) -> Result<Candidate> {
        let candidates = self.ordered_candidates(
            &TermQuery::new(
                Term::from_field_u64(self.fields.vector, u64::from(id)),
                IndexRecordOption::Basic,
            ),
            1,
        )?;
        ensure!(
            !candidates.incomplete && candidates.values.len() == 1,
            "vector/lexical identity mismatch"
        );
        Ok(candidates.values[0])
    }

    // Every supported builder assigns sequential vector IDs in chunk ordinal
    // order, skipping unembedded chunks. Binary search resolves that existing
    // relation in <=32 postings lookups without reading any stored text and
    // without adding a new persisted field or scanning all vectors.
    fn vector_id(&self, ordinal: u64) -> Result<Option<u32>> {
        let mut low = 1u64;
        let mut high = self.vectors as u64 + 1;
        while low < high {
            let middle = low + (high - low) / 2;
            let id = u32::try_from(middle).context("vector id overflow")?;
            match self.vector_candidate(id)?.ordinal.cmp(&ordinal) {
                std::cmp::Ordering::Equal => return Ok(Some(id)),
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
            }
        }
        Ok(None)
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
            *cached = Some(match &self.model_objects {
                Some(set) => EmbeddingModel::open_objects(&self.collection_directory, set)?,
                None => EmbeddingModel::open(&self.directory.join("model"))?,
            });
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
        if let Some(prefix) = query
            .path_prefix
            .as_deref()
            .filter(|prefix| !prefix.is_empty())
        {
            let scoped = self.ordered_candidates(&self.path_query(prefix)?, MAX_SCOPED_CHUNKS)?;
            let mut mapping = BTreeMap::new();
            for candidate in scoped.values {
                if let Some(id) = self.vector_id(candidate.ordinal)? {
                    mapping.insert(id, candidate);
                }
            }
            let ranked = graph.search_subset(&vector, mapping.keys().copied(), limit + 1)?;
            let incomplete = scoped.incomplete || ranked.len() > limit;
            let values = ranked
                .into_iter()
                .take(limit)
                .map(|(id, _)| mapping[&id])
                .collect();
            return Ok(Candidates { values, incomplete });
        }
        let mut ranked = graph.search(&vector, (limit * 4).clamp(64, 1000))?;
        let incomplete = ranked.len() < self.vectors || ranked.len() > limit;
        ranked.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        ranked.truncate(limit);
        let values = ranked
            .into_iter()
            .map(|(id, _)| self.vector_candidate(id))
            .collect::<Result<_>>()?;
        Ok(Candidates { values, incomplete })
    }

    // Cached/mapped handles must reject changed bytes just like fresh readers.
    // Never infer authenticity from file length or timestamps. This intentionally
    // pays whole-component verification costs until authenticated block reads
    // are available. Verification is performed once per hybrid request.
    fn verify_retained_components(&self) -> Result<()> {
        if let Some(receipt) = &self.v2_receipt {
            let manifest = verify_directory_v2(&self.directory, receipt, &self.collection)?;
            ensure!(
                manifest.chunks as u64 == self.reader.searcher().num_docs()
                    && manifest.vectors == self.vectors
                    && manifest.model_sha256 == self.model_sha256,
                "cached v2 generation manifest mismatch"
            );
            if let Some(set) = &self.model_objects {
                crate::model::inspect_objects_cache_descriptor(&self.collection_directory, set)?;
            }
            return Ok(());
        }
        let manifest = read_manifest(&self.directory)?;
        ensure!(
            manifest.schema == 1
                && manifest.engine == ENGINE
                && Some(manifest.revision.as_str())
                    == self.directory.file_name().and_then(|name| name.to_str())
                && manifest.generation == self.generation
                && manifest.collection == self.collection
                && manifest.chunks as u64 == self.reader.searcher().num_docs()
                && manifest.vectors == self.vectors
                && manifest.model_sha256 == self.model_sha256,
            "cached generation manifest identity mismatch"
        );
        Ok(())
    }

    fn read_hit(&self, address: DocAddress) -> Result<Hit> {
        let document: TantivyDocument = self.reader.searcher().doc(address)?;
        let payload = document
            .get_first(self.fields.payload)
            .and_then(|value| value.as_str())
            .context("missing lexical citation")?;
        Ok(serde_json::from_str(payload)?)
    }

    fn materialize(&self, candidates: Candidates, query: &SearchQuery) -> Result<SearchResponse> {
        let mut response = SearchResponse {
            publication: self.v2_receipt.as_ref().map(|receipt| {
                airk_core::evidence::RevisionEnvelope::new(self.collection.clone(), receipt.token)
            }),
            generation: self.generation,
            collection: self.collection.clone(),
            hits: Vec::new(),
            truncated: candidates.incomplete,
        };
        let mut remaining = query.max_bytes;
        for candidate in candidates.values {
            if remaining == 0 || response.hits.len() == query.limit {
                response.truncated = true;
                break;
            }
            // At most one complete indexed chunk is decoded at a time; all
            // other candidates retain only their ordinal and document address.
            let mut hit = self.read_hit(candidate.address)?;
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
