// SPDX-License-Identifier: Apache-2.0
//! Bounded keyset navigation over authenticated lexical candidates.
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LexicalReferencePosition {
    pub path: String,
    pub local: u64,
    pub stable_id: Sha256Digest,
}

impl From<&CurrentLexicalCandidate> for LexicalReferencePosition {
    fn from(candidate: &CurrentLexicalCandidate) -> Self {
        Self {
            path: candidate.citation.path.clone(),
            local: candidate.local,
            stable_id: candidate.stable_id,
        }
    }
}

impl CurrentRead {
    /// The continuation key changes only candidate selection. Every selected
    /// candidate must still be checked against retained source by its caller.
    pub fn lexical_reference_page(
        &self,
        query: &SearchQuery,
        after: Option<&LexicalReferencePosition>,
        inclusive: bool,
        path_order: bool,
        paths: Option<&std::collections::BTreeSet<String>>,
    ) -> Result<CurrentLexicalCandidates> {
        query.validate()?;
        if let Some(after) = after {
            ensure!(
                valid_current_path(&after.path)
                    && after.stable_id == stable_id(&after.path, after.local)?
                    && query
                        .path_prefix
                        .as_ref()
                        .is_none_or(|prefix| after.path.starts_with(prefix)),
                "invalid reference candidate continuation"
            );
        }
        let query_terms = terms(&query.text);
        ensure!(
            !query_terms.is_empty(),
            "query must contain a letter or number"
        );
        if !self.private_query_eligible()? {
            self.verify_lexical()?;
        }
        let expression = query_terms
            .iter()
            .map(|term| format!("\"{}\"", token(term)))
            .collect::<Vec<_>>()
            .join(" AND ");
        let paths = paths.map(serde_json::to_string).transpose()?;
        ensure!(
            paths.as_ref().is_none_or(|paths| paths.len() <= 4 << 20),
            "reference filtered selector exceeds byte budget"
        );
        let result = self.with_connection(|connection| {
            let sql = if path_order {
                "SELECT d.path,d.local,d.stable_id,d.payload FROM current_fts JOIN current_lexical_docs AS d NOT INDEXED ON d.id=current_fts.rowid WHERE current_fts MATCH ?1 AND (?2 IS NULL OR substr(d.path,1,length(?2))=?2 COLLATE BINARY) AND (?3 IS NULL OR d.path>?3 COLLATE BINARY OR (d.path=?3 COLLATE BINARY AND (d.local>?4 OR (?5 AND d.local=?4)))) AND (?7 IS NULL OR d.path IN (SELECT value FROM json_each(?7))) ORDER BY d.path COLLATE BINARY,d.local LIMIT ?6"
            } else {
                "SELECT d.path,d.local,d.stable_id,d.payload FROM current_fts JOIN current_lexical_docs AS d NOT INDEXED ON d.id=current_fts.rowid WHERE current_fts MATCH ?1 AND (?2 IS NULL OR substr(d.path,1,length(?2))=?2 COLLATE BINARY) AND (?3 IS NULL OR d.stable_id>?3 COLLATE BINARY OR (?5 AND d.stable_id=?3 COLLATE BINARY)) AND (?7 IS NULL OR d.path IN (SELECT value FROM json_each(?7))) ORDER BY d.stable_id COLLATE BINARY LIMIT ?6"
            };
            let after_key = after.map(|p| if path_order {p.path.clone()} else {p.stable_id.to_string()});
            let after_local = after.map(|p| i64::try_from(p.local)).transpose()?;
            let mut statement = connection.prepare(sql)?;
            let mut rows = statement.query(params![expression,query.path_prefix,after_key,after_local,inclusive,(query.limit + 1) as i64,paths])?;
            let mut values = Vec::new();
            let mut incomplete = false;
            while let Some(row) = rows.next()? {
                if values.len() == query.limit { incomplete = true; break; }
                let path: String = row.get(0)?;
                let local = u64::try_from(row.get::<_, i64>(1)?)?;
                let stable_id: Sha256Digest = row.get::<_, String>(2)?.parse()?;
                ensure!(stable_id == self::stable_id(&path,local)?, "FTS5 reference candidate identity mismatch");
                let payload: String = row.get(3)?;
                ensure!(payload.len() <= MAX_ROW_BYTES, "FTS5 reference candidate byte bound exceeded");
                let citation: Citation = serde_json::from_str(&payload)?;
                ensure!(citation.path == path, "invalid FTS5 reference candidate citation");
                values.push(CurrentLexicalCandidate {stable_id,local,citation});
            }
            Ok(CurrentLexicalCandidates {manifest_digest:self.head().lexical_commit,values,incomplete})
        });
        let result = self.observe_private_integrity(result)?;
        self.token()?;
        Ok(result)
    }
}
