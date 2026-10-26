// SPDX-License-Identifier: Apache-2.0
//! Materialization of current-index candidates against retained exact evidence.
//! Runtime routing and capability responses remain separate S8 work.
use airk_core::{Hit, SearchQuery};
use airk_retrieval::{terms, truncate_utf8};
use airk_store::{
    FileStore,
    current::lexical::{CurrentLexicalCandidates, stable_id},
    current::{CurrentRead, CurrentScope, CurrentToken, TransactionPhase},
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
mod metadata;
pub(crate) mod progress;
mod publication;
pub(crate) mod runtime;
mod search;
pub use search::{CurrentSearchEvidence, CurrentSearchMode, search_current};

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

/// Resolve Ready, Prepared or Applying after a writer restart. Prepared may be
/// abandoned only while its original current files still verify; Applying is
/// replayed or left unavailable on failure. No live source is consulted.
pub fn recover_current(
    store: &FileStore,
    quota: &Quota,
    scope: &CurrentScope,
) -> Result<CurrentRecovery> {
    let lock = store.lock()?;

    let writer = store.redb_writer(&lock, quota, scope)?;
    match writer.phase()? {
        TransactionPhase::Ready => {
            writer.verify_ready()?;
            writer.release_orphan_grants()?;
            let token = writer.head()?.token()?;
            Ok(CurrentRecovery {
                token,
                action: CurrentRecoveryAction::AlreadyReady,
                aborted_reason: None,
                maintenance_error: None,
            })
        }
        phase @ (TransactionPhase::Prepared | TransactionPhase::Applying) => {
            let pending = writer
                .pending()?
                .context("redb current transaction descriptor missing")?;
            if let Err(error) = writer.mark_applying(&pending) {
                if phase == TransactionPhase::Prepared
                    && writer.verify_pending_base(&pending).is_ok()
                {
                    let base = writer.head()?;
                    writer.abort_prepared(&pending)?;
                    return Ok(CurrentRecovery {
                        token: base.token()?,
                        action: CurrentRecoveryAction::AbortedPrepared,
                        aborted_reason: Some(format!("{error:#}")),
                        maintenance_error: None,
                    });
                }
                return Err(error.context("native current publication recovery failed"));
            }
            let token = writer.commit(&pending)?;
            Ok(CurrentRecovery {
                token,
                action: CurrentRecoveryAction::Applied,
                aborted_reason: None,
                maintenance_error: None,
            })
        }
    }
}

/// Request-scoped native lexical reader; the current gate spans candidate materialization.
pub struct VerifiedCurrentLexical<'a> {
    read: &'a CurrentRead,
}
impl VerifiedCurrentLexical<'_> {
    pub fn search(&self, query: &SearchQuery) -> Result<CurrentLexicalCandidates> {
        self.read.lexical_search(query)
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
    read.verify_lexical()?;
    Ok(VerifiedCurrentLexical { read })
}

pub struct CurrentLexicalEvidence {
    pub token: CurrentToken,
    pub collection: String,
    pub hits: Vec<Hit>,
    pub truncated: bool,
}

/// Verify every selected native lexical candidate against the authenticated current
/// catalog, its immutable retained extraction and original source bytes. The
/// caller must hold this read handle and its collection gate for the full call.
pub fn materialize_lexical(
    store: &FileStore,
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
            let full = store.load_retrieval_artifact(&compact)?.artifact;
            store.read_retained_source(&compact)?;
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

// Opt-in development timings contain stage names and durations only.
pub(crate) fn profile_start() -> Option<std::time::Instant> {
    (cfg!(debug_assertions) && std::env::var_os("AIRK_STORAGE_PROFILE").is_some())
        .then(std::time::Instant::now)
}
pub(crate) fn profile_tick(stage: &str, start: &mut Option<std::time::Instant>) {
    if let Some(start) = start {
        eprintln!(
            "airk-storage-profile: {stage} {:.3}ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
        *start = std::time::Instant::now();
    }
}
