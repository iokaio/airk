// SPDX-License-Identifier: Apache-2.0
//! Exact retained reads; no live/current byte fallback or historical search.
use crate::{FileInfo, Kernel};
use airk_core::{
    Artifact, ArtifactSource, Citation, FactHit, FactsResponse, SearchResponse, Snapshot,
    changes::ContinuityBasis,
    evidence::{
        ConditionalRead, EvidenceReadOptions, EvidenceReadStatus, ReadEvidence, RevisionEnvelope,
    },
    identity::{RevisionToken, framed_digest},
    publication::{InventoryFile, InventoryV2},
};
use airk_store::{blobs::Catalog, history::HistoryCatalog};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryAvailabilityRequest {
    pub revision: Option<RevisionToken>,
    pub cursor: Option<String>,
    pub limit: usize,
    pub max_bytes: usize,
}
#[derive(Serialize)]
pub struct HistoryAvailability {
    pub current_publication: RevisionEnvelope,
    pub revisions: Vec<RevisionToken>,
    /// Receipts prove retained publication identity; exact reads verify representations.
    pub representation_verification: &'static str,
    pub next_cursor: Option<String>,
    pub complete: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AvailabilityCursor {
    head: RevisionToken,
    before: u64,
    limit: usize,
    max_bytes: usize,
}

enum RetainedView {
    Full(Catalog),
    History(HistoryCatalog),
}
impl RetainedView {
    fn snapshot(&self) -> &Snapshot {
        match self {
            Self::Full(c) => &c.snapshot,
            Self::History(c) => &c.snapshot,
        }
    }
    fn inventory(&self) -> &InventoryV2 {
        match self {
            Self::Full(c) => c.inventory.as_ref().expect("v2 retained inventory"),
            Self::History(c) => &c.inventory,
        }
    }
    fn artifact(&self, path: &str) -> Result<Artifact> {
        match self {
            Self::Full(c) => c.load_artifact(path),
            Self::History(c) => c.load_artifact(path),
        }
    }
    fn source_lines(&self, kernel: &Kernel, path: &str) -> Result<Artifact> {
        let mut artifact = self.artifact(path)?;
        match self {
            Self::Full(c) => kernel.source_line_artifact(c, artifact),
            Self::History(c) => {
                if artifact.extraction_method == "utf8" {
                    artifact.chunks = airk_ingest::chunk(std::str::from_utf8(&c.source(path)?)?);
                }
                Ok(artifact)
            }
        }
    }
}

impl Kernel {
    pub fn history_availability(
        &self,
        request: &HistoryAvailabilityRequest,
    ) -> Result<HistoryAvailability> {
        ensure!(
            (1..=64).contains(&request.limit) && (1..=65536).contains(&request.max_bytes),
            "invalid history availability budget"
        );
        let current = self.catalog()?;
        let envelope = Self::catalog_envelope(&current)
            .context("history availability requires v2 migration")?;
        if let Some(revision) = request.revision {
            ensure!(
                request.cursor.is_none(),
                "exact revision availability cannot use a cursor"
            );
            revision.validate_against(&envelope.token)?;
            let retained = self
                .store
                .catalog_at(&revision, airk_store::blobs::MAX_CATALOG_BYTES)?
                .is_some()
                || self
                    .store
                    .history_at(&revision, airk_store::blobs::MAX_CATALOG_BYTES)?
                    .is_some();
            let fits = serde_json::to_vec(&revision)?.len() <= request.max_bytes;
            return Ok(HistoryAvailability {
                current_publication: envelope,
                revisions: if retained && fits {
                    vec![revision]
                } else {
                    vec![]
                },
                representation_verification: "exact representation verified",
                next_cursor: None,
                complete: !retained || fits,
            });
        }
        let before = if let Some(cursor) = &request.cursor {
            ensure!(cursor.len() <= 1024, "history cursor too large");
            let cursor: AvailabilityCursor = serde_json::from_str(cursor)?;
            ensure!(
                cursor.head == envelope.token
                    && cursor.limit == request.limit
                    && cursor.max_bytes == request.max_bytes
                    && cursor.before > 0
                    && cursor.before <= envelope.generation,
                "history cursor expired or mismatched"
            );
            Some(cursor.before)
        } else {
            None
        };
        let page = self.store.retained_receipts(before, request.limit)?;
        ensure!(
            page.head == envelope.token,
            "history listing changed; retry from first page"
        );
        let mut revisions = Vec::new();
        let mut remaining = request.max_bytes;
        let mut stopped = false;
        for receipt in &page.receipts {
            let size = serde_json::to_vec(&receipt.token)?.len();
            if size > remaining {
                stopped = true;
                break;
            }
            remaining -= size;
            revisions.push(receipt.token);
        }
        let complete = !stopped && page.complete;
        let next = if complete {
            None
        } else {
            revisions.last().map(|token| token.generation)
        };
        Ok(HistoryAvailability {
            current_publication: envelope.clone(),
            revisions,
            representation_verification: "receipt identity only; exact reads verify retained bytes",
            complete,
            next_cursor: next
                .map(|before| {
                    serde_json::to_string(&AvailabilityCursor {
                        head: envelope.token,
                        before,
                        limit: request.limit,
                        max_bytes: request.max_bytes,
                    })
                })
                .transpose()?,
        })
    }

    fn historical_read<T: Serialize>(
        &self,
        path: &str,
        options: &EvidenceReadOptions,
        selection: impl Serialize,
        render: impl FnOnce(&RetainedView, &InventoryFile, RevisionEnvelope) -> Result<T>,
    ) -> Result<ConditionalRead<T>> {
        airk_ingest::adapters::profile::validate_relative_path(path)?;
        ensure!(
            !options.follow_moves,
            "historical reads require an exact original path; move following is unsupported"
        );
        let requested = options.revision.context("historical revision required")?;
        let current = self.catalog()?;
        let envelope =
            Self::catalog_envelope(&current).context("historical reads require v2 migration")?;
        requested.validate_against(&envelope.token)?;
        let mut read = ReadEvidence {
            publication: None,
            current_publication: Some(envelope.clone()),
            historical: requested != envelope.token,
            original_root: None,
            status: EvidenceReadStatus::HistoryUnavailable,
            requested_path: path.into(),
            resolved_path: None,
            validator: None,
            basis: ContinuityBasis::Unknown,
            reason: Some("exact retained revision unavailable".into()),
        };
        // Each store entry point pins the canonical revision lease before inspecting
        // its representation. A retired revision never resolves to current content.
        let retained = match self
            .store
            .catalog_at(&requested, airk_store::blobs::MAX_CATALOG_BYTES)?
        {
            Some(catalog) => Some(RetainedView::Full(catalog)),
            None => self
                .store
                .history_at(&requested, airk_store::blobs::MAX_CATALOG_BYTES)?
                .map(RetainedView::History),
        };
        let Some(retained) = retained else {
            return Ok(ConditionalRead {
                read,
                content: None,
            });
        };
        let served = RevisionEnvelope::new(self.collection.clone(), requested);
        read.publication = Some(served.clone());
        let Some(file) = retained.inventory().files.get(path) else {
            read.status = EvidenceReadStatus::Unknown;
            read.reason = Some("path absent from requested revision".into());
            return Ok(ConditionalRead {
                read,
                content: None,
            });
        };
        read.original_root = Some(file.state.root.clone());
        if options
            .artifact_id
            .is_some_and(|id| id != file.state.artifact_id)
            || options
                .known_hash
                .is_some_and(|hash| hash != file.state.source_hash)
        {
            read.status = EvidenceReadStatus::Unknown;
            read.reason =
                Some("requested historical path does not match remembered identity".into());
            return Ok(ConditionalRead {
                read,
                content: None,
            });
        }
        let payload = render(&retained, file, served)?;
        let mut representation = serde_json::to_value(&payload)?;
        if let Some(fields) = representation.as_object_mut() {
            fields.remove("publication");
            fields.remove("generation");
        }
        let validator = framed_digest(
            b"read-representation/1",
            &[
                file.state.payload_digest.as_bytes(),
                file.state.evidence_digest.as_bytes(),
                file.state.profile_id.as_bytes(),
                file.state.root.as_bytes(),
                file.state.path.as_bytes(),
                &serde_json::to_vec(&selection)?,
                &serde_json::to_vec(&representation)?,
            ],
        );
        read.resolved_path = Some(path.into());
        read.validator = Some(validator);
        read.reason = None;
        read.basis = ContinuityBasis::SamePathObservation;
        let content = if options.if_evidence == Some(validator) {
            read.status = EvidenceReadStatus::NotModified;
            None
        } else {
            read.status = EvidenceReadStatus::Content;
            Some(payload)
        };
        Ok(ConditionalRead { read, content })
    }

    pub(crate) fn historical_info(
        &self,
        path: &str,
        options: &EvidenceReadOptions,
    ) -> Result<ConditionalRead<FileInfo>> {
        self.historical_read(path, options, "file_info/1", |view, file, served| {
            let snapshot = view.snapshot();
            Ok(FileInfo {
                publication: Some(served),
                source: airk_store::roots::source_location(&snapshot.root, &snapshot.roots, path)?,
                generation: snapshot.generation,
                collection: snapshot.collection.clone(),
                path: path.into(),
                content_hash: file.state.source_hash.to_string(),
                source_bytes: file.source_bytes,
                language: file.language.clone(),
                extraction_method: file.extraction_method.clone(),
                parse_status: file.parse_status.clone(),
                chunks: file.chunks,
                facts: file.facts,
                indexed_lines: file.indexed_lines,
                line_basis: if file.extraction_method == "utf8" {
                    "source"
                } else {
                    "extracted-text; cite original location"
                },
            })
        })
    }
    pub(crate) fn historical_file(
        &self,
        path: &str,
        start: usize,
        end: usize,
        max_bytes: usize,
        options: &EvidenceReadOptions,
    ) -> Result<ConditionalRead<SearchResponse>> {
        self.historical_read(
            path,
            options,
            ("read_file/1", start, end, max_bytes),
            |view, _, served| {
                Self::read_snapshot(
                    Some(served),
                    view.snapshot(),
                    &view.source_lines(self, path)?,
                    path,
                    start,
                    end,
                    max_bytes,
                )
            },
        )
    }
    pub(crate) fn historical_context(
        &self,
        path: &str,
        line: usize,
        before: usize,
        after: usize,
        max_bytes: usize,
        options: &EvidenceReadOptions,
    ) -> Result<ConditionalRead<SearchResponse>> {
        self.historical_read(
            path,
            options,
            ("context/1", line, before, after, max_bytes),
            |view, file, served| {
                ensure!(
                    line <= file.indexed_lines,
                    "line is outside historical indexed source"
                );
                Self::read_snapshot(
                    Some(served),
                    view.snapshot(),
                    &view.source_lines(self, path)?,
                    path,
                    line.saturating_sub(before).max(1),
                    line.saturating_add(after).min(file.indexed_lines),
                    max_bytes,
                )
            },
        )
    }
    pub(crate) fn historical_outline(
        &self,
        path: &str,
        limit: usize,
        max_bytes: usize,
        options: &EvidenceReadOptions,
        page: &airk_core::discovery::DiscoveryOptions,
    ) -> Result<ConditionalRead<FactsResponse>> {
        self.historical_read(
            path,
            options,
            ("outline/2", limit, max_bytes, page),
            |view, file, served| {
                let binding = airk_core::discovery::binding(
                    &served.token.to_string(),
                    "outline",
                    (
                        path,
                        page.compact,
                        options.follow_moves,
                        options.artifact_id,
                        options.known_hash,
                    ),
                )?;
                airk_core::discovery::decode(page.cursor.as_deref(), &binding)?;
                let artifact = view.artifact(path)?;
                let keys = airk_core::continuity::FactKeyIndex::new(&file.continuity);
                let mut facts: Vec<_> = artifact
                    .facts
                    .iter()
                    .enumerate()
                    .map(|(ordinal, fact)| FactHit {
                        continuity_key: keys.key(ordinal, fact),
                        name: fact.name.clone(),
                        kind: fact.kind.clone(),
                        syntax_kind: fact.syntax_kind.clone(),
                        language: artifact.language.clone(),
                        document: fact.document.clone(),
                        code: fact.code.clone(),
                        citation: Citation {
                            path: path.into(),
                            content_hash: artifact.content_hash.clone(),
                            start_line: fact.start_line,
                            end_line: fact.end_line,
                            location: fact.location.clone(),
                        },
                    })
                    .chain(
                        view.snapshot()
                            .imported_facts
                            .iter()
                            .filter(|fact| fact.citation.path == path)
                            .cloned()
                            .map(|mut fact| {
                                fact.continuity_key = None;
                                fact
                            }),
                    )
                    .collect();
                facts.sort_by(|a, b| {
                    (a.citation.start_line, &a.kind, &a.name).cmp(&(
                        b.citation.start_line,
                        &b.kind,
                        &b.name,
                    ))
                });
                let mut response = FactsResponse {
                    page: Default::default(),
                    outline: Vec::new(),
                    indexing: view.inventory().index_detail.unwrap_or_default().into(),
                    publication: Some(served),
                    generation: view.snapshot().generation,
                    collection: self.collection.clone(),
                    facts: vec![],
                    truncated: false,
                };
                airk_core::discovery::paginate_facts(
                    &mut response,
                    facts,
                    page,
                    &binding,
                    limit,
                    max_bytes,
                )?;
                Ok(response)
            },
        )
    }
}
