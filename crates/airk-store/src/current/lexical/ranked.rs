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
use airk_core::{Citation, identity::Sha256Digest};
use anyhow::{Result, ensure};
use rusqlite::params;
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

/// Path prefix (?3), path selector (?4) and read scope (?5 atoms, ?6 aliases)
/// over `d.path`, as in the conjunctive lexical query.
const FILTERS: &str = "(?3 IS NULL OR substr(d.path,1,length(?3))=?3 COLLATE BINARY) AND (?4 IS NULL OR d.path IN (SELECT value FROM json_each(?4))) AND (?5 IS NULL OR EXISTS (SELECT 1 FROM json_each(?5) AS a WHERE NOT EXISTS (SELECT 1 FROM json_each(?6) AS alias WHERE ((d.path='@'||alias.value COLLATE BINARY OR substr(d.path,1,length(alias.value)+2)='@'||alias.value||'/' COLLATE BINARY) != (json_extract(a.value,'$.path')='@'||alias.value COLLATE BINARY OR substr(json_extract(a.value,'$.path'),1,length(alias.value)+2)='@'||alias.value||'/' COLLATE BINARY))) AND (d.path=json_extract(a.value,'$.path') COLLATE BINARY OR (json_extract(a.value,'$.kind')='subtree' AND (json_extract(a.value,'$.path')='' OR substr(d.path,1,length(json_extract(a.value,'$.path'))+1)=json_extract(a.value,'$.path')||'/' COLLATE BINARY)))))";

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
        if !self.private_query_eligible()? {
            self.verify_lexical()?;
        }
        let scope = self.scope_atoms_json()?;
        let scope_aliases = self.scope_aliases_json()?;
        let paths = paths.map(serde_json::to_string).transpose()?;
        ensure!(
            paths.as_ref().is_none_or(|paths| paths.len() <= 4 << 20),
            "filtered lexical path selector exceeds byte budget"
        );
        let result = self.with_connection(|connection| {
            let total: i64 =
                connection.query_row("SELECT count(*) FROM current_lexical_docs", [], |row| {
                    row.get(0)
                })?;
            let total = total.max(1) as f64;
            let mut scores = BTreeMap::<i64, f64>::new();
            let mut dropped_groups = Vec::new();
            let mut statement = connection.prepare(&format!(
                "SELECT d.id FROM current_fts JOIN current_lexical_docs AS d NOT INDEXED ON d.id=current_fts.rowid WHERE current_fts MATCH ?1 AND {FILTERS} LIMIT ?2"
            ))?;
            for (index, group) in groups.iter().enumerate() {
                let mut rows = statement.query(params![
                    group.expression(),
                    (MAX_GROUP_ROWS + 1) as i64,
                    path_prefix,
                    paths,
                    scope,
                    scope_aliases
                ])?;
                let mut matched = Vec::new();
                while let Some(row) = rows.next()? {
                    matched.push(row.get::<_, i64>(0)?);
                }
                if matched.len() > MAX_GROUP_ROWS {
                    dropped_groups.push(index);
                    continue;
                }
                let frequency = matched.len() as f64;
                let weight = (1.0 + (total - frequency + 0.5) / (frequency + 0.5)).ln();
                for id in matched {
                    *scores.entry(id).or_default() += weight;
                }
            }
            // Every scored row is already in scope; truncation cannot drop an
            // in-scope row in favour of an out-of-scope one.
            let mut ranked: Vec<(i64, f64)> = scores.into_iter().collect();
            ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            let prefilter = (limit * 4).min(4000);
            let incomplete = !dropped_groups.is_empty() || ranked.len() > prefilter;
            ranked.truncate(prefilter);
            let score_of: BTreeMap<i64, f64> = ranked.iter().copied().collect();
            let ids = serde_json::to_string(&ranked.iter().map(|(id, _)| *id).collect::<Vec<_>>())?;
            let mut select = connection.prepare(&format!(
                "SELECT d.id,d.path,d.local,d.stable_id,d.payload FROM current_lexical_docs AS d WHERE d.id IN (SELECT value FROM json_each(?1)) AND ?2 IS NULL AND {FILTERS}"
            ))?;
            let mut rows = select.query(params![
                ids,
                Option::<i64>::None,
                path_prefix,
                paths,
                scope,
                scope_aliases
            ])?;
            let mut found = Vec::new();
            while let Some(row) = rows.next()? {
                let id: i64 = row.get(0)?;
                let path: String = row.get(1)?;
                let local = u64::try_from(row.get::<_, i64>(2)?)?;
                let stable_id: Sha256Digest = row.get::<_, String>(3)?.parse()?;
                ensure!(
                    stable_id == super::stable_id(&path, local)?,
                    "FTS5 candidate identity mismatch"
                );
                let payload: String = row.get(4)?;
                ensure!(
                    payload.len() <= super::MAX_ROW_BYTES,
                    "FTS5 candidate byte bound exceeded"
                );
                let citation: Citation = serde_json::from_str(&payload)?;
                ensure!(citation.path == path, "invalid FTS5 candidate citation");
                let score = *score_of
                    .get(&id)
                    .ok_or_else(|| anyhow::anyhow!("ranked FTS5 row outside its selection"))?;
                found.push((
                    score,
                    CurrentLexicalCandidate {
                        stable_id,
                        local,
                        citation,
                    },
                ));
            }
            ensure!(
                found.len() == score_of.len(),
                "ranked FTS5 rows changed between selection and read"
            );
            found.sort_by(|a, b| {
                b.0.total_cmp(&a.0)
                    .then(a.1.stable_id.cmp(&b.1.stable_id))
            });
            let incomplete = incomplete || found.len() > limit;
            found.truncate(limit);
            Ok(RankedLexicalCandidates {
                candidates: CurrentLexicalCandidates {
                    manifest_digest: self.head().lexical_commit,
                    values: found.into_iter().map(|(_, candidate)| candidate).collect(),
                    incomplete,
                },
                dropped_groups,
            })
        });
        let result = self.observe_private_integrity(result)?;
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
