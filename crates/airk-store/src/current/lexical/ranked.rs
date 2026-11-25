// SPDX-License-Identifier: Apache-2.0
//! Ranked any-term FTS5 candidates for the private evaluation search switch.
//!
//! The ordinary lexical path is a conjunctive match in stable-id order. This
//! reader answers a different question: which chunks match the most of a
//! bounded set of term groups, weighted by inverse document frequency. It reads
//! only FTS5 doclists and the authenticated document rows; it writes nothing and
//! needs no new index state. Only the kernel's evaluation-only search switch
//! calls it; product search never does.
use super::{CurrentLexicalCandidate, CurrentLexicalCandidates, token};
use crate::current::CurrentRead;
use airk_core::identity::Sha256Digest;
use anyhow::{Result, ensure};
use std::collections::{BTreeMap, BTreeSet};

/// At most this many term groups take part in one ranked request.
pub const MAX_RANKED_GROUPS: usize = 24;
/// A group matching more rows than this is treated as too common to rank by.
pub const MAX_GROUP_ROWS: usize = 50_000;
const MAX_ALTERNATIVE_TERMS: usize = 8;

/// One query word: it matches a chunk when every term of any alternative is
/// among the chunk's indexed terms (the whole word, or its identifier parts).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TermGroup {
    pub alternatives: Vec<Vec<String>>,
}

impl TermGroup {
    /// The FTS5 expression of this group, kept as the reference for `matches`.
    #[cfg(test)]
    fn expression(&self) -> String {
        self.alternatives
            .iter()
            .map(|alternative| {
                format!(
                    "({})",
                    alternative
                        .iter()
                        .map(|term| format!("\"{}\"", token(term)))
                        .collect::<Vec<_>>()
                        .join(" AND ")
                )
            })
            .collect::<Vec<_>>()
            .join(" OR ")
    }

    /// The same predicate as [`Self::expression`], over a chunk's term set.
    pub fn matches(&self, terms: &BTreeSet<String>) -> bool {
        self.alternatives
            .iter()
            .any(|alternative| alternative.iter().all(|term| terms.contains(term)))
    }

    /// Whether this group is a valid ranked request group: every alternative
    /// non-empty, at most eight terms, each a lowercase identifier term.
    pub fn valid(&self) -> bool {
        !self.alternatives.is_empty() && self.alternatives.iter().all(|a| valid_alternative(a))
    }
}

/// One alternative: one to eight terms, each accepted by [`valid_term`].
pub fn valid_alternative(alternative: &[String]) -> bool {
    !alternative.is_empty()
        && alternative.len() <= MAX_ALTERNATIVE_TERMS
        && alternative.iter().all(|term| valid_term(term))
}

/// A lowercase identifier term of at most 128 bytes.
pub fn valid_term(term: &str) -> bool {
    !term.is_empty()
        && term.len() <= 128
        && term.chars().all(|c| c.is_alphanumeric() || c == '_')
        && term.to_lowercase() == term
}

/// Ranked candidates plus the groups dropped as too common. A dropped group
/// also marks the candidates incomplete.
#[derive(Clone, Debug)]
pub struct RankedLexicalCandidates {
    pub candidates: CurrentLexicalCandidates,
    pub dropped_groups: Vec<usize>,
}

impl CurrentRead {
    /// Chunks ranked by the summed inverse document frequency of the groups
    /// they match. Path prefix, path selector and read scope filter each
    /// group's matches before anything is ranked or truncated, so a scoped
    /// search ranks only in-scope chunks; ties break by stable id.
    pub fn lexical_search_ranked_any(
        &self,
        groups: &[TermGroup],
        path_prefix: Option<&str>,
        paths: Option<&BTreeSet<String>>,
        limit: usize,
    ) -> Result<RankedLexicalCandidates> {
        ensure!(
            !groups.is_empty()
                && groups.len() <= MAX_RANKED_GROUPS
                && groups.iter().all(TermGroup::valid)
                && (1..=1000).contains(&limit),
            "invalid ranked lexical request"
        );
        use crate::index::{LexicalAfter, LexicalOrder, LexicalQuery, MAX_LEXICAL_HITS};
        let paths: Option<Vec<String>> = paths.map(|paths| paths.iter().cloned().collect());
        ensure!(
            paths
                .as_ref()
                .is_none_or(|paths| paths.iter().map(String::len).sum::<usize>() <= 4 << 20),
            "filtered lexical path selector exceeds byte budget"
        );
        let total = self.with_index(|index| index.chunk_count())?.max(1) as f64;
        let mut scores = BTreeMap::<Sha256Digest, (f64, CurrentLexicalCandidate)>::new();
        let mut dropped_groups = Vec::new();
        'groups: for (index, group) in groups.iter().enumerate() {
            // Every alternative's in-scope matches, united by stable identity.
            let mut matched = BTreeMap::<Sha256Digest, CurrentLexicalCandidate>::new();
            for alternative in &group.alternatives {
                let mut after = None;
                loop {
                    let (hits, more) = self.with_index(|reader| {
                        reader.lexical_keyset(
                            &LexicalQuery {
                                terms: alternative.iter().map(|term| token(term)).collect(),
                                path_prefix: path_prefix.map(str::to_owned),
                                paths: paths.clone(),
                                limit: MAX_LEXICAL_HITS,
                                ranked: false,
                            },
                            LexicalOrder::StableId,
                            after.as_ref(),
                        )
                    })?;
                    after = hits.last().map(|hit| LexicalAfter::StableId {
                        id: hit.stable_id,
                        inclusive: false,
                    });
                    for hit in hits {
                        if self.accepts_path(&hit.path) {
                            let candidate = super::candidate(hit)?;
                            matched.insert(candidate.stable_id, candidate);
                        }
                    }
                    if matched.len() > MAX_GROUP_ROWS {
                        dropped_groups.push(index);
                        continue 'groups;
                    }
                    if !more {
                        break;
                    }
                }
            }
            let frequency = matched.len() as f64;
            let weight = (1.0 + (total - frequency + 0.5) / (frequency + 0.5)).ln();
            for (id, candidate) in matched {
                scores.entry(id).or_insert((0.0, candidate)).0 += weight;
            }
        }
        // Every scored row is already in scope; truncation cannot drop an
        // in-scope row in favour of an out-of-scope one.
        let mut found: Vec<(f64, CurrentLexicalCandidate)> = scores.into_values().collect();
        found.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.stable_id.cmp(&b.1.stable_id)));
        let prefilter = (limit * 4).min(4000);
        let incomplete =
            !dropped_groups.is_empty() || found.len() > prefilter || found.len() > limit;
        found.truncate(limit);
        let result = RankedLexicalCandidates {
            candidates: CurrentLexicalCandidates {
                manifest_digest: self.head().lexical_commit,
                values: found.into_iter().map(|(_, candidate)| candidate).collect(),
                incomplete,
            },
            dropped_groups,
        };
        self.token()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(alternatives: &[&[&str]]) -> TermGroup {
        TermGroup {
            alternatives: alternatives
                .iter()
                .map(|alternative| alternative.iter().map(|term| (*term).to_owned()).collect())
                .collect(),
        }
    }

    #[test]
    fn group_expression_and_predicate_agree() {
        let deep = group(&[&["deepseek"], &["deep", "seek"]]);
        assert_eq!(
            deep.expression(),
            "(\"xdeepseek\") OR (\"xdeep\" AND \"xseek\")"
        );
        let terms = |words: &[&str]| words.iter().map(|w| (*w).to_owned()).collect();
        assert!(deep.matches(&terms(&["deepseekprovider", "deep", "seek", "provider"])));
        assert!(deep.matches(&terms(&["deepseek"])));
        assert!(!deep.matches(&terms(&["deep", "provider"])));
    }

    #[test]
    fn groups_must_be_lowercase_bounded_identifier_terms() {
        assert!(group(&[&["mistral"]]).valid());
        assert!(!group(&[&["Mistral"]]).valid());
        assert!(!group(&[&["two words"]]).valid());
        assert!(!group(&[&[]]).valid());
        assert!(
            !TermGroup {
                alternatives: vec![]
            }
            .valid()
        );
    }
}
