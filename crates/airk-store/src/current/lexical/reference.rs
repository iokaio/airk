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
            self.require_path(&after.path)?;
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
        use crate::index::{LexicalAfter, LexicalOrder};
        let order = if path_order {
            CandidateOrder::Keyset(
                LexicalOrder::Path,
                after
                    .map(|position| -> Result<LexicalAfter> {
                        Ok(LexicalAfter::Path {
                            path: position.path.clone(),
                            ordinal: u32::try_from(position.local)?,
                            inclusive,
                        })
                    })
                    .transpose()?,
            )
        } else {
            CandidateOrder::Keyset(
                LexicalOrder::StableId,
                after.map(|position| LexicalAfter::StableId {
                    id: position.stable_id,
                    inclusive,
                }),
            )
        };
        let result = self.lexical_candidates(
            &query.text,
            query.path_prefix.as_deref(),
            paths,
            query.limit,
            order,
        )?;
        self.token()?;
        Ok(result)
    }
}
