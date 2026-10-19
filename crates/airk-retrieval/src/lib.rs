// SPDX-License-Identifier: Apache-2.0
//! Local persistent candidate engines and bounded evidence responses.
pub mod bounded;
pub mod embedding_batch;
pub mod lexical_experiment;
pub mod model;
mod onnx;
pub mod persistent;
pub mod vector;
pub mod vector_units;
use airk_core::{Citation, Hit, SearchQuery, SearchResponse, Snapshot};
use anyhow::{Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

pub struct LexicalIndex {
    postings: BTreeMap<String, BTreeSet<usize>>,
    chunks: Vec<(String, usize)>,
}

impl LexicalIndex {
    pub fn build(snapshot: &Snapshot) -> Self {
        let mut index = Self {
            postings: BTreeMap::new(),
            chunks: Vec::new(),
        };
        for (path, artifact) in &snapshot.artifacts {
            for (ordinal, chunk) in artifact.chunks.iter().enumerate() {
                let id = index.chunks.len();
                index.chunks.push((path.clone(), ordinal));
                for term in terms(&format!("{path} {}", chunk.text)) {
                    index.postings.entry(term).or_default().insert(id);
                }
            }
        }
        index
    }

    pub fn search(&self, snapshot: &Snapshot, query: &SearchQuery) -> Result<SearchResponse> {
        query.validate()?;
        let query_terms = terms(&query.text);
        ensure!(
            !query_terms.is_empty(),
            "query must contain a letter or number"
        );
        let mut sets = query_terms.iter().map(|term| self.postings.get(term));
        let mut candidates = sets.next().flatten().cloned().unwrap_or_default();
        for set in sets {
            match set {
                Some(set) => candidates.retain(|id| set.contains(id)),
                None => candidates.clear(),
            }
        }
        let mut response = SearchResponse {
            publication: None,
            generation: snapshot.generation,
            collection: snapshot.collection.clone(),
            hits: Vec::new(),
            truncated: false,
        };
        let mut remaining = query.max_bytes;
        for id in candidates {
            let (path, ordinal) = &self.chunks[id];
            if query
                .path_prefix
                .as_ref()
                .is_some_and(|prefix| !path.starts_with(prefix))
            {
                continue;
            }
            if remaining == 0 || response.hits.len() == query.limit {
                response.truncated = true;
                break;
            }
            let artifact = &snapshot.artifacts[path];
            let chunk = &artifact.chunks[*ordinal];
            let (text, truncated) = truncate_utf8(&chunk.text, remaining);
            remaining -= text.len();
            response.truncated |= truncated;
            response.hits.push(Hit {
                citation: Citation {
                    path: path.clone(),
                    content_hash: artifact.content_hash.clone(),
                    start_line: chunk.start_line,
                    end_line: chunk.end_line,
                    location: chunk.location.clone(),
                },
                text,
                truncated,
            });
        }
        Ok(response)
    }
}

/// Whole identifier aliases plus underscore and camel-case components. No stemming.
pub fn terms(text: &str) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    for word in text
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| !s.is_empty())
    {
        if !word.chars().any(char::is_alphanumeric) {
            continue;
        }
        result.insert(word.to_lowercase());
        for part in word.split('_').filter(|s| !s.is_empty()) {
            result.insert(part.to_lowercase());
            let mut start = 0;
            let chars: Vec<_> = part.char_indices().collect();
            for i in 1..chars.len() {
                if chars[i].1.is_uppercase() && chars[i - 1].1.is_lowercase() {
                    result.insert(part[start..chars[i].0].to_lowercase());
                    start = chars[i].0;
                }
            }
            result.insert(part[start..].to_lowercase());
        }
    }
    result
}

pub fn truncate_utf8(text: &str, max_bytes: usize) -> (String, bool) {
    let mut end = max_bytes.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), end < text.len())
}
