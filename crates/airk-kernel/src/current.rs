// SPDX-License-Identifier: Apache-2.0
//! Materialization of current-index candidates against retained exact evidence.
//! Runtime routing and capability responses remain separate S8 work.
use airk_core::{Hit, SearchQuery};
use airk_retrieval::{terms, truncate_utf8};
use airk_store::{
    FileStore,
    current::lexical::{CurrentLexicalCandidates, stable_id},
    current::{CurrentRead, CurrentScope, CurrentToken},
    quota::Quota,
};
use anyhow::{Context, Result, ensure};

const MAX_LEXICAL_VALIDATION_BYTES: u64 = 256 * 1024 * 1024;

mod update;
pub use update::{CurrentUpdateResult, CurrentUpdateStage, CurrentUpdater, SemanticUpdate};
mod changes;
mod conditional;
pub(crate) mod enrichment;
mod indexing;
pub(crate) mod literal;
mod metadata;
#[cfg(test)]
mod plain_read_tests;
pub(crate) mod progress;
mod publication;
pub(crate) mod runtime;
mod search;
/// Private Answer evaluation only: binds the `search_v2` switch for the process.
#[cfg(feature = "test-support")]
pub use search::v2::bind_evaluation_search;
pub use search::{
    CurrentSearchEvidence, CurrentSearchMode, DefinitionHint, DefinitionPointer,
    SearchWithDefinitionsResponse, search_current,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurrentRecoveryAction {
    AlreadyReady,
    Applied,
    AbortedPrepared,
}

pub struct CurrentRecovery {
    pub token: CurrentToken,
    pub action: CurrentRecoveryAction,
    pub aborted_reason: Option<String>,
    pub maintenance_error: Option<String>,
}

/// Stable recovery stage retained in a job's failure code without source or paths.
#[derive(Debug)]
pub struct RecoveryFailure(pub &'static str);
impl std::fmt::Display for RecoveryFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for RecoveryFailure {}

/// The plain index has no journal: a batch is one SQLite transaction and
/// SQLite itself rolls back an interrupted one. Recovery only reports the
/// committed publication. No live source is consulted.
pub fn recover_current(
    store: &FileStore,
    _quota: &Quota,
    scope: &CurrentScope,
) -> Result<CurrentRecovery> {
    let sequence = if store.plain_index_present()? {
        let read = store
            .plain_read()
            .context(RecoveryFailure("recovery_open_reader"))?;
        ensure!(
            read.scope() == scope,
            "current index belongs to another owner"
        );
        read.sequence()
    } else {
        0
    };
    Ok(CurrentRecovery {
        token: airk_store::current::plain_writer::plain_token(scope, sequence),
        action: CurrentRecoveryAction::AlreadyReady,
        aborted_reason: None,
        maintenance_error: None,
    })
}

/// Request-scoped FTS5 reader; the current gate spans candidate materialization.
pub struct VerifiedCurrentLexical<'a> {
    read: &'a CurrentRead,
}
impl VerifiedCurrentLexical<'_> {
    pub fn search(&self, query: &SearchQuery) -> Result<CurrentLexicalCandidates> {
        self.read.lexical_search(query)
    }

    pub fn search_paths(
        &self,
        query: &SearchQuery,
        paths: Option<&std::collections::BTreeSet<String>>,
    ) -> Result<CurrentLexicalCandidates> {
        self.read.lexical_search_paths(query, paths)
    }
}
pub fn open_selected_lexical<'a>(
    store: &FileStore,
    read: &'a CurrentRead,
) -> Result<VerifiedCurrentLexical<'a>> {
    ensure!(
        store.current_scope(&read.head().scope.root, &read.head().scope.collection)?
            == Some(read.head().scope.clone()),
        "foreign selected current lexical collection"
    );
    if !read.private_query_eligible()? {
        read.verify_lexical()?;
    }
    Ok(VerifiedCurrentLexical { read })
}

pub struct CurrentLexicalEvidence {
    pub token: CurrentToken,
    pub collection: String,
    pub hits: Vec<Hit>,
    pub truncated: bool,
}

/// Verify every selected FTS5 candidate against the authenticated current
/// catalog, its immutable retained extraction and original source bytes. The
/// caller must hold this read handle and its collection gate for the full call.
pub fn materialize_lexical(
    _store: &FileStore,
    read: &CurrentRead,
    query: &SearchQuery,
    candidates: CurrentLexicalCandidates,
) -> Result<CurrentLexicalEvidence> {
    query.validate()?;
    ensure!(
        candidates.manifest_digest == read.head().lexical_commit
            && candidates.values.len() <= query.limit,
        "current lexical candidate revision or count differs from request"
    );
    let wanted = terms(&query.text);
    ensure!(!wanted.is_empty(), "query must contain a letter or number");
    let mut cached: Option<(String, airk_core::Artifact)> = None;
    let mut work = 0u64;
    let mut remaining = query.max_bytes;
    let mut hits = Vec::with_capacity(candidates.values.len());
    let mut truncated = candidates.incomplete;
    for candidate in candidates.values {
        let path = &candidate.citation.path;
        ensure!(
            query
                .path_prefix
                .as_ref()
                .is_none_or(|prefix| path.starts_with(prefix))
                && candidate.stable_id == stable_id(path, candidate.local)?,
            "current lexical candidate path or identity mismatch"
        );
        if cached
            .as_ref()
            .is_none_or(|(cached_path, _)| cached_path != path)
        {
            let compact = read
                .artifact(path)?
                .context("current lexical candidate has no catalog artifact")?;
            let retained = compact
                .retained
                .as_ref()
                .context("current lexical artifact has no retained reference")?;
            let cost = u64::try_from(compact.bytes)?
                .checked_add(u64::try_from(retained.object_bytes)?)
                .context("current lexical validation byte overflow")?;
            work = work
                .checked_add(cost)
                .context("current lexical validation work overflow")?;
            ensure!(
                work <= MAX_LEXICAL_VALIDATION_BYTES,
                "current lexical validation read budget exceeded"
            );
            let full = load_lexical_artifact(read, &compact)?;
            cached = Some((path.clone(), full));
        }
        let artifact = &cached.as_ref().expect("artifact loaded above").1;
        let chunk = artifact
            .chunks
            .get(usize::try_from(candidate.local)?)
            .context("current lexical candidate local chunk missing")?;
        ensure!(
            artifact.content_hash == candidate.citation.content_hash
                && chunk.start_line == candidate.citation.start_line
                && chunk.end_line == candidate.citation.end_line
                && chunk.location == candidate.citation.location,
            "current lexical candidate differs from retained evidence"
        );
        let indexed_terms = terms(&format!("{path} {}", chunk.text));
        ensure!(
            wanted.is_subset(&indexed_terms),
            "current lexical candidate does not match the query"
        );
        if remaining == 0 {
            truncated = true;
            break;
        }
        let (text, cut) = truncate_utf8(&chunk.text, remaining);
        remaining -= text.len();
        truncated |= cut;
        hits.push(Hit {
            citation: candidate.citation,
            text,
            truncated: cut,
        });
    }
    Ok(CurrentLexicalEvidence {
        token: read.token()?,
        collection: read.head().scope.collection.clone(),
        hits,
        truncated,
    })
}

/// The full artifact of a lexical candidate from the reader's snapshot. The
/// source is not read: most files keep none (Q5); the stored record must
/// still name the indexed source hash.
pub(super) fn load_lexical_artifact(
    read: &CurrentRead,
    compact: &airk_core::Artifact,
) -> Result<airk_core::Artifact> {
    let artifact = read.load_retrieval_artifact(compact)?.artifact;
    read.verify_retained_source(compact)?;
    Ok(artifact)
}
