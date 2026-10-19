// SPDX-License-Identifier: Apache-2.0
//! Materialization of current-index candidates against retained exact evidence.
//! Runtime routing and capability responses remain separate S8 work.
use airk_core::{Hit, SearchQuery};
use airk_retrieval::{
    bounded::Charge,
    current_lexical::{CurrentLexicalCandidates, VerifiedCurrentLexical, stable_id},
    terms, truncate_utf8,
};
use airk_store::{
    FileStore,
    current::{CurrentRead, CurrentToken},
};
use anyhow::{Context, Result, ensure};

const MAX_LEXICAL_VALIDATION_BYTES: u64 = 256 * 1024 * 1024;

/// Open only the physical Tantivy directory selected by the exact read head.
/// The `CurrentRead` keeps the shared collection gate through subsequent
/// search and materialization; no idle lexical handle is retained.
pub fn open_selected_lexical(
    store: &FileStore,
    read: &CurrentRead,
    charge: Charge,
) -> Result<VerifiedCurrentLexical> {
    ensure!(
        store.directory().file_name().and_then(|name| name.to_str())
            == Some(read.head().scope.collection.as_str()),
        "foreign selected current lexical collection"
    );
    let path = store
        .directory()
        .join(read.head().lexical_directory_name()?);
    let bytes = read.lexical_manifest()?;
    VerifiedCurrentLexical::open(&path, charge, &bytes, read.head().lexical_commit)
}

pub struct CurrentLexicalEvidence {
    pub token: CurrentToken,
    pub collection: String,
    pub hits: Vec<Hit>,
    pub truncated: bool,
}

/// Verify every selected Tantivy candidate against the authenticated current
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
        let path = &candidate.hit.citation.path;
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
            artifact.content_hash == candidate.hit.citation.content_hash
                && chunk.start_line == candidate.hit.citation.start_line
                && chunk.end_line == candidate.hit.citation.end_line
                && chunk.location == candidate.hit.citation.location
                && chunk.text == candidate.hit.text
                && !candidate.hit.truncated,
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
            citation: candidate.hit.citation,
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
