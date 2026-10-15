// SPDX-License-Identifier: Apache-2.0
//! Read-only, exact-revision observed changes and remembered evidence validation.
use crate::Kernel;
use airk_core::{changes::*, evidence::*, identity::*};
use airk_store::blobs::Catalog;
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

fn check_alias(catalog: &Catalog, path: &str, prefix: bool) -> Result<()> {
    if let Some(alias_path) = path.strip_prefix('@') {
        let alias = alias_path.split('/').next().unwrap_or_default();
        let literal = catalog.snapshot.artifacts.contains_key(path)
            || prefix
                && catalog
                    .snapshot
                    .artifacts
                    .keys()
                    .any(|indexed| indexed.starts_with(path));
        let historical = catalog.lineage.as_ref().is_some_and(|lineage| {
            lineage
                .events
                .iter()
                .flat_map(|event| [&event.before, &event.after].into_iter().flatten())
                .chain(
                    lineage
                        .resolvers
                        .iter()
                        .flat_map(|record| std::iter::once(&record.from).chain(record.to.iter())),
                )
                .filter_map(|endpoint| endpoint.citation_path.as_deref())
                .any(|known| {
                    known == path
                        || prefix && known.starts_with(path)
                        || known
                            .strip_prefix('@')
                            .and_then(|known| known.split_once('/'))
                            .is_some_and(|(known_alias, _)| known_alias == alias)
                })
        });
        ensure!(
            catalog.snapshot.roots.contains_key(alias) || literal || historical,
            "unknown source alias"
        );
    }
    Ok(())
}

fn citation_path(catalog: &Catalog, endpoint: &ChangeEndpoint) -> Option<String> {
    if let Some(path) = &endpoint.citation_path {
        return Some(path.clone());
    }
    if endpoint.root == catalog.snapshot.root {
        return Some(endpoint.path.clone());
    }
    let mut matches = catalog
        .snapshot
        .roots
        .iter()
        .filter(|(_, root)| **root == endpoint.root);
    let (alias, _) = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(format!("@{alias}/{}", endpoint.path))
}

fn endpoint_is_bound(catalog: &Catalog, endpoint: &ChangeEndpoint) -> bool {
    citation_path(catalog, endpoint)
        .and_then(|path| {
            airk_store::roots::source_location(
                &catalog.snapshot.root,
                &catalog.snapshot.roots,
                &path,
            )
            .ok()
        })
        .is_some_and(|location| {
            location.root == endpoint.root && location.relative_path == endpoint.path
        })
}

fn resolvers_at_path<'a>(catalog: &'a Catalog, path: &str) -> Result<Vec<&'a ResolverRecord>> {
    let lineage = catalog.lineage.as_ref().context("v2 lineage missing")?;
    Ok(lineage
        .resolvers
        .iter()
        .filter(|record| citation_path(catalog, &record.from).as_deref() == Some(path))
        .collect())
}

fn position(position: &ChangePosition) -> (u64, u64) {
    (position.generation, position.event_id)
}

fn candidate_endpoints(
    catalog: &Catalog,
    records: &[&ResolverRecord],
    prefix: &str,
) -> Result<(Vec<ChangeEndpoint>, bool)> {
    let inventory = catalog.inventory.as_ref().context("v2 inventory missing")?;
    let mut equivalent = std::collections::BTreeSet::new();
    let mut exact = std::collections::BTreeSet::new();
    for record in records {
        if !endpoint_is_bound(catalog, &record.from) {
            continue;
        }
        if record.basis == ContinuityBasis::EquivalentContentCandidate {
            equivalent.insert((record.from.source_hash, record.from.profile_id));
        }
        if let Some(to) = &record.to {
            exact.insert((
                to.root.as_str(),
                to.path.as_str(),
                to.source_hash,
                to.profile_id,
            ));
        }
    }
    let mut candidates = vec![];
    let mut truncated = false;
    for (path, file) in &inventory.files {
        let state = &file.state;
        if !path.starts_with(prefix)
            || !(equivalent.contains(&(state.source_hash, state.profile_id))
                || exact.contains(&(
                    state.root.as_str(),
                    state.path.as_str(),
                    state.source_hash,
                    state.profile_id,
                )))
        {
            continue;
        }
        if candidates.len() == 8 {
            truncated = true;
            break;
        }
        candidates.push(ChangeEndpoint {
            citation_path: Some(path.clone()),
            root: state.root.clone(),
            path: state.path.clone(),
            source_hash: state.source_hash,
            profile_id: state.profile_id,
        });
    }
    Ok((candidates, truncated))
}

fn baseline(catalog: &Catalog, token: Option<&RevisionToken>) -> Result<BaselineStatus> {
    let Some(token) = token else {
        return Ok(BaselineStatus::NotRequested);
    };
    let target = catalog
        .receipt
        .as_ref()
        .context("v2 receipt missing")?
        .token;
    token.validate_against(&target)?;
    if token == &target {
        return Ok(BaselineStatus::Verified);
    }
    let lineage = catalog.lineage.as_ref().context("v2 lineage missing")?;
    let mut parent = lineage.observation.parent;
    for header in lineage.ancestry.iter().rev() {
        if let Some(parent_token) = parent {
            if parent_token == *token {
                return Ok(BaselineStatus::Verified);
            }
            if parent_token.generation == token.generation {
                anyhow::bail!("forked baseline token");
            }
            if parent_token != header.token {
                return Ok(BaselineStatus::BaselineUnavailable);
            }
        } else {
            return Ok(BaselineStatus::BaselineUnavailable);
        }
        parent = header.parent;
    }
    if parent == Some(*token) {
        return Ok(BaselineStatus::Verified);
    }
    Ok(BaselineStatus::BaselineUnavailable)
}

impl Kernel {
    fn change_suggestions(
        &self,
        catalog: &Catalog,
        request: &ChangesRequest,
        baseline_status: BaselineStatus,
    ) -> Result<(Vec<ChangeSuggestion>, SuggestionCoverage)> {
        let mut coverage = SuggestionCoverage {
            version: crate::suggestions::SKETCH_VERSION,
            status: SuggestionStatus::Unavailable,
            baseline_representation: None,
            compared_pairs: 0,
            examined_files: 0,
            examined_text_bytes: 0,
            reason: Some(
                "suggestions require verified ancestry and an exact retained baseline".into(),
            ),
        };
        let Some(since) = request
            .since
            .filter(|_| baseline_status == BaselineStatus::Verified)
        else {
            return Ok((vec![], coverage));
        };
        // Retain either lease for the entire source read and comparison. No live
        // workspace paths are consulted, even when original bytes are absent.
        let full = self
            .store
            .catalog_at(&since, airk_store::blobs::MAX_CATALOG_BYTES)?;
        let history = if full.is_none() {
            self.store
                .history_at(&since, airk_store::blobs::MAX_CATALOG_BYTES)?
        } else {
            None
        };
        let (snapshot, inventory, representation) = if let Some(full) = &full {
            (
                &full.snapshot,
                full.inventory
                    .as_ref()
                    .context("suggestion baseline inventory missing")?,
                "full",
            )
        } else if let Some(history) = &history {
            (&history.snapshot, &history.inventory, "history")
        } else {
            return Ok((vec![], coverage));
        };
        coverage.baseline_representation = Some(representation.into());
        let current = catalog
            .inventory
            .as_ref()
            .context("suggestion target inventory missing")?;
        let mut loaded_bytes = 0;
        let mut omitted = false;
        let before = self.suggestion_inputs(
            snapshot,
            inventory,
            &catalog.snapshot,
            current,
            &request.path_prefix,
            &mut loaded_bytes,
            &mut omitted,
        )?;
        let after = self.suggestion_inputs(
            &catalog.snapshot,
            current,
            snapshot,
            inventory,
            &request.path_prefix,
            &mut loaded_bytes,
            &mut omitted,
        )?;
        let old_views = suggestion_views(&before);
        let new_views = suggestion_views(&after);
        let result = crate::suggestions::suggest(&old_views, &new_views);
        coverage.status = if omitted || result.partial {
            SuggestionStatus::Partial
        } else {
            SuggestionStatus::Complete
        };
        coverage.reason = (omitted || result.partial).then(|| "some endpoints, source bytes, sketches, pairs or outputs exceeded eligibility or bounds".into());
        coverage.compared_pairs = result.compared_pairs;
        coverage.examined_files = result.examined_files;
        coverage.examined_text_bytes = loaded_bytes;
        let mut output = result
            .files
            .iter()
            .map(|candidate| ChangeSuggestion::File {
                before: before[candidate.before].endpoint.clone(),
                after: after[candidate.after].endpoint.clone(),
                score_basis_points: candidate.score_basis_points,
                shared_shingles: candidate.shared_shingles,
                union_shingles: candidate.union_shingles,
            })
            .collect::<Vec<_>>();
        if !omitted {
            output.extend(result.directories.into_iter().map(|directory| {
                ChangeSuggestion::Directory {
                    root: directory.root,
                    before: directory.before,
                    after: directory.after,
                    children: directory
                        .children
                        .into_iter()
                        .map(|(a, b)| (before[a].path.clone(), after[b].path.clone()))
                        .collect(),
                }
            }));
        }
        Ok((output, coverage))
    }

    #[allow(clippy::too_many_arguments)]
    fn suggestion_inputs(
        &self,
        snapshot: &airk_core::Snapshot,
        inventory: &airk_core::publication::InventoryV2,
        other_snapshot: &airk_core::Snapshot,
        other: &airk_core::publication::InventoryV2,
        prefix: &str,
        loaded_bytes: &mut usize,
        omitted: &mut bool,
    ) -> Result<Vec<SuggestionInput>> {
        let exact: std::collections::BTreeSet<_> = other
            .files
            .values()
            .map(|file| {
                (
                    file.state.root.as_str(),
                    file.state.profile_id,
                    file.state.source_hash,
                )
            })
            .collect();
        let identities: std::collections::BTreeSet<_> = other
            .files
            .values()
            .map(|file| file.state.artifact_id)
            .collect();
        let mut output = vec![];
        let mut attempted = 0;
        for (path, file) in &inventory.files {
            if !path.starts_with(prefix)
                || other_snapshot.artifacts.contains_key(path)
                || identities.contains(&file.state.artifact_id)
                || exact.contains(&(
                    file.state.root.as_str(),
                    file.state.profile_id,
                    file.state.source_hash,
                ))
            {
                continue;
            }
            if attempted == 256 {
                *omitted = true;
                break;
            }
            attempted += 1;
            let artifact = snapshot
                .artifacts
                .get(path)
                .context("suggestion artifact missing from projection")?;
            if !file.raw_source
                || artifact.extraction_method != "utf8"
                || file.source_bytes > 65_536
                || file.source_bytes > 8 * 1024 * 1024 - *loaded_bytes
            {
                *omitted = true;
                continue;
            }
            let bytes = match self.store.read_retained_source_bounded(artifact, 65_536) {
                Ok(bytes) => bytes,
                Err(error)
                    if error.chain().any(|cause| {
                        cause
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
                    }) =>
                {
                    *omitted = true;
                    continue;
                }
                Err(error) => return Err(error),
            };
            *loaded_bytes += bytes.len();
            let Ok(text) = String::from_utf8(bytes) else {
                *omitted = true;
                continue;
            };
            output.push(SuggestionInput {
                path: path.clone(),
                text,
                endpoint: ChangeEndpoint {
                    citation_path: Some(path.clone()),
                    root: file.state.root.clone(),
                    path: file.state.path.clone(),
                    source_hash: file.state.source_hash,
                    profile_id: file.state.profile_id,
                },
            });
        }
        Ok(output)
    }

    pub(crate) fn resolve_evidence_path(
        &self,
        catalog: &Catalog,
        path: &str,
        options: &EvidenceReadOptions,
    ) -> Result<ReadEvidence> {
        validate_selectors(None, &[RememberedSelector::Path { path: path.into() }], "")?;
        check_alias(catalog, path, false)?;
        let Some(receipt) = catalog.receipt.as_ref() else {
            ensure!(
                options.if_evidence.is_none()
                    && !options.follow_moves
                    && options.known_hash.is_none()
                    && options.artifact_id.is_none(),
                "conditional evidence options require v2 migration"
            );
            let present = catalog.snapshot.artifacts.contains_key(path);
            return Ok(ReadEvidence {
                publication: None,
                current_publication: None,
                historical: false,
                original_root: None,
                status: if present {
                    EvidenceReadStatus::Content
                } else {
                    EvidenceReadStatus::Unknown
                },
                requested_path: path.into(),
                resolved_path: present.then(|| path.into()),
                validator: None,
                basis: if present {
                    ContinuityBasis::SamePathObservation
                } else {
                    ContinuityBasis::Unknown
                },
                reason: None,
            });
        };
        let mut result = ReadEvidence {
            current_publication: Some(RevisionEnvelope::new(
                self.collection.clone(),
                receipt.token,
            )),
            historical: false,
            original_root: None,
            publication: Some(RevisionEnvelope::new(
                self.collection.clone(),
                receipt.token,
            )),
            status: EvidenceReadStatus::Unknown,
            requested_path: path.into(),
            resolved_path: None,
            validator: None,
            basis: ContinuityBasis::Unknown,
            reason: None,
        };
        if let Some(file) = catalog.file(path)? {
            if options
                .artifact_id
                .is_none_or(|id| id == file.state.artifact_id)
                && options
                    .known_hash
                    .is_none_or(|hash| hash == file.state.source_hash)
            {
                result.status = EvidenceReadStatus::Content;
                result.resolved_path = Some(path.into());
                result.basis = ContinuityBasis::SamePathObservation;
            } else {
                result.reason = Some("live path does not match remembered identity".into());
            }
            return Ok(result);
        }
        let records: Vec<_> = resolvers_at_path(catalog, path)?
            .into_iter()
            .filter(|record| {
                options
                    .artifact_id
                    .is_none_or(|id| record.artifact_id == id)
                    && options
                        .known_hash
                        .is_none_or(|hash| record.from.source_hash == hash)
            })
            .collect();
        if records.is_empty() {
            return Ok(result);
        }
        if records.len() != 1 {
            result.status = EvidenceReadStatus::Ambiguous;
            return Ok(result);
        }
        let mut record = records[0];
        let lineage = catalog.lineage.as_ref().context("lineage missing")?;
        let mut visited = std::collections::BTreeSet::new();
        for _ in 0..MAX_LINEAGE_RECORDS {
            if !visited.insert((record.from.root.clone(), record.from.path.clone())) {
                result.status = EvidenceReadStatus::Ambiguous;
                result.reason = Some("resolver cycle".into());
                return Ok(result);
            }
            match record.state {
                ResolutionState::DeletedObserved | ResolutionState::RemovedFromIndex => {
                    result.status = EvidenceReadStatus::RemovedFromIndex;
                    return Ok(result);
                }
                ResolutionState::Ambiguous => {
                    result.status = EvidenceReadStatus::Ambiguous;
                    return Ok(result);
                }
                ResolutionState::Unknown => return Ok(result),
                ResolutionState::ExactMove => {}
            }
            if !endpoint_is_bound(catalog, &record.from) {
                result.reason = Some("remembered source alias binding changed".into());
                return Ok(result);
            }
            if record.observed_generation <= lineage.coverage.resolution.complete_after
                || record.basis != ContinuityBasis::UniqueCompatibleContent
            {
                result.reason = Some("complete compatible resolution unavailable".into());
                return Ok(result);
            }
            let Some(to) = &record.to else {
                return Ok(result);
            };
            if !endpoint_is_bound(catalog, to) {
                return Ok(result);
            }
            let Some(target_path) = citation_path(catalog, to) else {
                return Ok(result);
            };
            if let Some(file) = catalog.file(&target_path)? {
                if file.state.artifact_id != record.artifact_id
                    || file.state.source_hash != to.source_hash
                    || file.state.profile_id != to.profile_id
                {
                    return Ok(result);
                }
                result.status = if options.follow_moves {
                    EvidenceReadStatus::Content
                } else {
                    EvidenceReadStatus::MoveCandidate
                };
                result.resolved_path = Some(target_path);
                result.basis = record.basis;
                return Ok(result);
            }
            let next: Vec<_> = catalog
                .resolvers_for_path(&to.root, &to.path)?
                .into_iter()
                .filter(|next| {
                    next.artifact_id == record.artifact_id
                        && next.from.source_hash == to.source_hash
                        && next.from.profile_id == to.profile_id
                })
                .collect();
            if next.len() != 1 {
                result.status = if next.is_empty() {
                    EvidenceReadStatus::Unknown
                } else {
                    EvidenceReadStatus::Ambiguous
                };
                return Ok(result);
            }
            record = next[0];
        }
        result.reason = Some("resolver traversal bound exceeded".into());
        Ok(result)
    }

    fn validate_remembered(
        &self,
        catalog: &Catalog,
        selector: &RememberedSelector,
        ordinal: usize,
        since: Option<&RevisionToken>,
        baseline_status: BaselineStatus,
        path_prefix: &str,
    ) -> Result<EvidenceValidation> {
        let mut result = EvidenceValidation {
            ordinal,
            selector: selector.clone(),
            outcome: ValidationOutcome::Unknown,
            artifact_id: None,
            current: None,
            requested_path: None,
            resolved_path: None,
            basis: ContinuityBasis::Unknown,
            validated_dimensions: vec![],
            candidates: vec![],
            candidates_truncated: false,
            reason: None,
        };
        let inventory = catalog.inventory.as_ref().context("v2 inventory missing")?;
        let file = match selector {
            RememberedSelector::Symbol { .. } | RememberedSelector::Document { .. } => {
                use airk_core::continuity::ContinuityKey;
                let key = match selector {
                    RememberedSelector::Symbol { symbol_key } => ContinuityKey::Symbol(*symbol_key),
                    RememberedSelector::Document { document_key } => {
                        ContinuityKey::Document(*document_key)
                    }
                    _ => unreachable!(),
                };
                let Some((path, record)) = catalog.continuity_by_key(key)? else {
                    let events = catalog.events_since(since.map_or(0, |token| token.generation))?;
                    if let Some(event) =
                        events
                            .iter()
                            .rev()
                            .find(|event| match (key, &event.subject) {
                                (
                                    ContinuityKey::Symbol(key),
                                    ChangeSubject::Symbol { symbol_key, .. },
                                ) => key == *symbol_key,
                                (
                                    ContinuityKey::Document(key),
                                    ChangeSubject::Document { document_key, .. },
                                ) => key == *document_key,
                                _ => false,
                            })
                        && event.after.is_none()
                        && event
                            .before
                            .as_ref()
                            .and_then(|endpoint| citation_path(catalog, endpoint))
                            .is_some_and(|path| path.starts_with(path_prefix))
                    {
                        result.outcome = ValidationOutcome::RemovedFromIndex;
                        result.artifact_id = match event.subject {
                            ChangeSubject::Symbol { artifact_id, .. }
                            | ChangeSubject::Document { artifact_id, .. } => Some(artifact_id),
                            _ => None,
                        };
                        result.reason = Some("retained observation removed this key from the index; this does not establish source deletion".into());
                        return Ok(result);
                    }
                    result.reason = Some("continuity key is not in the current inventory; absence does not establish deletion".into());
                    return Ok(result);
                };
                if !path.starts_with(path_prefix) {
                    result.reason =
                        Some("continuity key is outside the requested path scope".into());
                    return Ok(result);
                }
                let file = inventory
                    .files
                    .get(&path)
                    .context("continuity file missing")?;
                result.artifact_id = Some(file.state.artifact_id);
                result.current = Some(file.state.clone());
                result.resolved_path = Some(path);
                result.basis = ContinuityBasis::SamePathObservation;
                result.validated_dimensions = record.changed.keys().copied().collect();
                result.outcome = ValidationOutcome::Current;
                if record.ambiguous_predecessor {
                    result.reason = Some(
                        "a new key was assigned because predecessor matching was ambiguous".into(),
                    );
                }
                if baseline_status == BaselineStatus::Verified
                    && let Some(since) = since
                {
                    result.outcome = if record.first_generation > since.generation
                        || record
                            .changed
                            .values()
                            .any(|stamp| *stamp > since.generation)
                    {
                        ValidationOutcome::Modified
                    } else if record.descriptor.complete
                        && record.descriptor.unsupported_dimensions.is_empty()
                    {
                        ValidationOutcome::Unchanged
                    } else {
                        result.reason =
                            Some("some continuity dimensions are unsupported or incomplete".into());
                        ValidationOutcome::Unknown
                    };
                }
                return Ok(result);
            }
            RememberedSelector::Artifact { artifact_id } => {
                result.artifact_id = Some(*artifact_id);
                catalog.file_by_artifact(*artifact_id)?
            }
            RememberedSelector::Path { path } | RememberedSelector::Citation { path, .. } => {
                result.requested_path = Some(path.clone());
                catalog.file(path)?
            }
        };
        if let Some(file) = file {
            result.artifact_id = Some(file.state.artifact_id);
            result.resolved_path = inventory
                .files
                .iter()
                .find(|(_, entry)| entry.state.artifact_id == file.state.artifact_id)
                .map(|(path, _)| path.clone());
            result.basis = ContinuityBasis::SamePathObservation;
            result.outcome = ValidationOutcome::Current;
            if let RememberedSelector::Citation { source_hash, path } = selector {
                result.validated_dimensions.push(ChangeDimension::Content);
                if source_hash == &file.state.source_hash {
                    result.outcome = ValidationOutcome::Unchanged;
                } else {
                    let displaced: Vec<_> = resolvers_at_path(catalog, path)?
                        .into_iter()
                        .filter(|record| {
                            record.from.source_hash == *source_hash
                                && record.artifact_id != file.state.artifact_id
                        })
                        .collect();
                    (result.candidates, result.candidates_truncated) =
                        candidate_endpoints(catalog, &displaced, path_prefix)?;
                    result.outcome = if !displaced.is_empty() {
                        ValidationOutcome::PathReused
                    } else {
                        ValidationOutcome::Modified
                    };
                }
            } else if baseline_status == BaselineStatus::Verified
                && let Some(since) = since
            {
                result.validated_dimensions = vec![
                    ChangeDimension::Path,
                    ChangeDimension::Content,
                    ChangeDimension::Evidence,
                ];
                result.outcome = if file.state.stamps.path_generation <= since.generation
                    && file.state.stamps.content_generation <= since.generation
                    && file.state.stamps.evidence_generation <= since.generation
                {
                    ValidationOutcome::Unchanged
                } else {
                    ValidationOutcome::Modified
                };
            }
            result.current = Some(file.state);
            return Ok(result);
        }
        let records = match selector {
            RememberedSelector::Artifact { artifact_id } => {
                catalog.resolvers_for_artifact(*artifact_id)?
            }
            RememberedSelector::Path { path } | RememberedSelector::Citation { path, .. } => {
                resolvers_at_path(catalog, path)?
                    .into_iter()
                    .filter(|record| match selector {
                        RememberedSelector::Citation { source_hash, .. } => {
                            record.from.source_hash == *source_hash
                        }
                        _ => true,
                    })
                    .collect()
            }
            _ => vec![],
        };
        if records.len() > 1 {
            result.outcome = ValidationOutcome::Ambiguous;
        } else if let Some(record) = records.first() {
            result.artifact_id = Some(record.artifact_id);
            result.basis = record.basis;
            result.outcome = match record.state {
                ResolutionState::ExactMove => ValidationOutcome::MoveCandidate,
                ResolutionState::DeletedObserved | ResolutionState::RemovedFromIndex => {
                    ValidationOutcome::RemovedFromIndex
                }
                ResolutionState::Ambiguous => ValidationOutcome::Ambiguous,
                ResolutionState::Unknown => ValidationOutcome::Unknown,
            };
            result.resolved_path = record
                .to
                .as_ref()
                .and_then(|endpoint| citation_path(catalog, endpoint));
        }
        (result.candidates, result.candidates_truncated) =
            candidate_endpoints(catalog, &records, path_prefix)?;
        Ok(result)
    }

    pub(crate) fn changes_freshness(&self) -> FreshnessObservation {
        let mut observation = FreshnessObservation {
            observed_unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |value| {
                    value.as_millis().min(u128::from(u64::MAX)) as u64
                }),
            state: FreshnessState::Unregistered,
            requested_watermark: None,
            completed_watermark: None,
            running_watermark: None,
            last_success_unix_ms: None,
            error: None,
            live_source_verified: false,
        };
        match airk_store::scheduler::Queue::new(&self.data_root)
            .status(&self.collection, &self.root_id())
        {
            Ok(Some(job)) => {
                observation.requested_watermark = Some(job.requested_revision);
                observation.completed_watermark = Some(job.completed_revision);
                observation.running_watermark = Some(job.running_revision);
                observation.last_success_unix_ms =
                    (job.last_completed_unix_ms != 0).then_some(job.last_completed_unix_ms);
                observation.state = if job.paused {
                    FreshnessState::Paused
                } else if job.blocked {
                    FreshnessState::Blocked
                } else if job.running_revision != 0 {
                    FreshnessState::Running
                } else if job.error.is_some() || job.scan_error.is_some() {
                    FreshnessState::Failed
                } else if job.requested_revision > job.completed_revision {
                    FreshnessState::Pending
                } else {
                    FreshnessState::Idle
                };
                observation.error = job
                    .error
                    .or(job.scan_error)
                    .map(|error| error.chars().take(512).collect());
            }
            Ok(None) => {}
            Err(error) => {
                observation.state = FreshnessState::Unavailable;
                observation.error = Some(error.to_string().chars().take(512).collect());
            }
        }
        observation
    }

    pub fn changes(&self, request: &ChangesRequest) -> Result<ChangesResponse> {
        request.validate()?;
        let mut bound_request = request.clone();
        bound_request.cursor = None;
        let request_digest = framed_digest(
            b"changes-request/1",
            &[&serde_json::to_vec(&bound_request)?],
        );
        let cursor: Option<ChangeCursor> = request
            .cursor
            .as_ref()
            .map(|cursor| serde_json::from_str(cursor))
            .transpose()
            .context("malformed change cursor")?;
        let current_catalog = self.catalog()?;
        let current = current_catalog
            .receipt
            .as_ref()
            .context("changes require a v2 collection")?
            .token;
        let target = cursor.as_ref().map_or(current, |cursor| cursor.target);
        target.validate_against(&current)?;
        if let Some(cursor) = &cursor {
            cursor.validate_binding(
                &target,
                request.since.as_ref(),
                &request_digest,
                request.selectors.len(),
            )?;
        }
        if let Some(since) = &request.since {
            since.validate_against(&target)?;
            self.store.validate_revision_token(since)?;
        }
        let catalog = if target == current {
            Some(current_catalog)
        } else {
            self.store
                .catalog_at(&target, airk_store::blobs::MAX_CATALOG_BYTES)?
                .map(Arc::new)
        };
        let mut response = ChangesResponse {
            status: ChangesStatus::Ready,
            publication: RevisionEnvelope::new(self.collection.clone(), target),
            from: request.since,
            to: target,
            current,
            baseline_status: BaselineStatus::BaselineUnavailable,
            coverage: None,
            changes: vec![],
            invalidations: vec![],
            validations: vec![],
            suggestions: vec![],
            suggestion_coverage: None,
            freshness: self.changes_freshness(),
            record_bytes: 0,
            truncated: false,
            cursor: None,
        };
        let Some(catalog) = catalog else {
            response.status = ChangesStatus::CursorExpired;
            return Ok(response);
        };
        check_alias(&catalog, &request.path_prefix, true)?;
        for selector in &request.selectors {
            if let RememberedSelector::Path { path } | RememberedSelector::Citation { path, .. } =
                selector
            {
                check_alias(&catalog, path, false)?;
            }
        }
        response.baseline_status = baseline(&catalog, request.since.as_ref())?;
        let suggestion_records = if request.include_suggestions {
            let (records, coverage) =
                self.change_suggestions(&catalog, request, response.baseline_status)?;
            response.suggestion_coverage = Some(coverage);
            records
        } else {
            vec![]
        };
        let suggestion_digest = if request.include_suggestions {
            Some(framed_digest(
                b"change-suggestions/1",
                &[&serde_json::to_vec(&(
                    &response.suggestion_coverage,
                    &suggestion_records,
                ))?],
            ))
        } else {
            None
        };
        if cursor
            .as_ref()
            .is_some_and(|cursor| cursor.suggestion_digest != suggestion_digest)
        {
            response.status = ChangesStatus::CursorExpired;
            return Ok(response);
        }
        let mut coverage = catalog
            .lineage
            .as_ref()
            .context("v2 lineage missing")?
            .coverage
            .clone();
        if response.baseline_status == BaselineStatus::BaselineUnavailable {
            coverage.ancestry.reason = Some(CoverageReason::BaselineUnavailable);
        }
        response.coverage = Some(coverage);
        let mut coalesced: BTreeMap<String, CoalescedChange> = BTreeMap::new();
        let mut matching = std::collections::BTreeSet::new();
        if response.baseline_status == BaselineStatus::Verified {
            for event in
                catalog.events_since(request.since.expect("verified baseline").generation)?
            {
                if matches!(event.subject, ChangeSubject::Symbol { .. }) && !request.include_symbols
                    || matches!(event.subject, ChangeSubject::Document { .. })
                        && !request.include_documents
                {
                    continue;
                }
                let key = serde_json::to_string(&event.subject)?;
                if request.path_prefix.is_empty()
                    || matches!(event.subject, ChangeSubject::Global { .. })
                    || [&event.before, &event.after]
                        .into_iter()
                        .flatten()
                        .any(|endpoint| {
                            citation_path(&catalog, endpoint)
                                .is_some_and(|path| path.starts_with(&request.path_prefix))
                        })
                {
                    matching.insert(key.clone());
                }
                let change = coalesced.entry(key).or_insert_with(|| CoalescedChange {
                    subject: event.subject.clone(),
                    first_generation: event.generation,
                    last: ChangePosition {
                        generation: event.generation,
                        event_id: event.event_id,
                    },
                    observations: 0,
                    before: event.before.clone(),
                    after: None,
                    before_detail: event.before_detail.clone(),
                    after_detail: None,
                    dimensions: vec![],
                    causes: vec![],
                    basis: event.basis,
                });
                change.last = ChangePosition {
                    generation: event.generation,
                    event_id: event.event_id,
                };
                change.observations += 1;
                change.after = event.after.clone();
                change.after_detail = event.after_detail.clone();
                for dimension in &event.dimensions {
                    if !change.dimensions.contains(dimension) {
                        change.dimensions.push(*dimension);
                    }
                }
                if !change.causes.contains(&event.cause) {
                    change.causes.push(event.cause);
                }
                if change.basis != event.basis {
                    change.basis = ContinuityBasis::Unknown;
                }
            }
        }
        let mut changes: Vec<_> = coalesced
            .into_iter()
            .filter(|(key, _)| matching.contains(key))
            .map(|(_, change)| change)
            .collect();
        changes.sort_by_key(|change| position(&change.last));
        let mut state = cursor.unwrap_or(ChangeCursor {
            version: CHANGE_CONTRACT_VERSION,
            target,
            baseline: request.since,
            request_digest,
            stage: CursorStage::Changes,
            after_change: None,
            next_validation: 0,
            next_suggestion: 0,
            suggestion_digest,
        });
        if let Some(after) = &state.after_change {
            ensure!(
                changes
                    .iter()
                    .any(|change| position(&change.last) == position(after)),
                "cursor change position is not in bound result list"
            );
        }
        if matches!(
            state.stage,
            CursorStage::Validations | CursorStage::Suggestions
        ) {
            ensure!(
                state.after_change.as_ref().map(position)
                    == changes.last().map(|change| position(&change.last)),
                "cursor skipped unfinished change list"
            );
        }
        let mut count = 0;
        if state.stage == CursorStage::Changes {
            for change in changes
                .iter()
                .filter(|change| {
                    state
                        .after_change
                        .as_ref()
                        .is_none_or(|after| position(&change.last) > position(after))
                })
                .cloned()
                .collect::<Vec<_>>()
            {
                let bytes = serde_json::to_vec(&change)?.len();
                ensure!(
                    bytes <= request.max_bytes,
                    "change record needs {bytes} bytes; increase max_bytes"
                );
                if count == request.limit || response.record_bytes + bytes > request.max_bytes {
                    response.truncated = true;
                    break;
                }
                response.record_bytes += bytes;
                count += 1;
                state.after_change = Some(change.last);
                if matches!(change.subject, ChangeSubject::Global { .. }) {
                    response.invalidations.push(change);
                } else {
                    response.changes.push(change);
                }
            }
            if !response.truncated {
                state.stage = CursorStage::Validations;
            }
        }
        if state.stage == CursorStage::Validations {
            while state.next_validation < request.selectors.len() {
                let mut validation = self.validate_remembered(
                    &catalog,
                    &request.selectors[state.next_validation],
                    state.next_validation,
                    request.since.as_ref(),
                    response.baseline_status,
                    &request.path_prefix,
                )?;
                if !request.path_prefix.is_empty()
                    && ![&validation.requested_path, &validation.resolved_path]
                        .into_iter()
                        .flatten()
                        .any(|path| path.starts_with(&request.path_prefix))
                    && validation.candidates.is_empty()
                {
                    validation.outcome = ValidationOutcome::Unknown;
                    validation.current = None;
                    validation.resolved_path = None;
                    validation.validated_dimensions.clear();
                    validation.basis = ContinuityBasis::Unknown;
                    validation.reason =
                        Some("remembered selector is outside requested path scope".into());
                }
                let mut bytes = serde_json::to_vec(&validation)?.len();
                while bytes > request.max_bytes && !validation.candidates.is_empty() {
                    validation.candidates.pop();
                    validation.candidates_truncated = true;
                    bytes = serde_json::to_vec(&validation)?.len();
                }
                ensure!(
                    bytes <= request.max_bytes,
                    "validation record needs {bytes} bytes; increase max_bytes"
                );
                if count == request.limit || response.record_bytes + bytes > request.max_bytes {
                    response.truncated = true;
                    break;
                }
                response.record_bytes += bytes;
                count += 1;
                response.validations.push(validation);
                state.next_validation += 1;
            }
            if !response.truncated {
                state.stage = CursorStage::Suggestions;
            }
        }
        if state.stage == CursorStage::Suggestions {
            ensure!(
                state.next_suggestion <= suggestion_records.len(),
                "invalid suggestion cursor position"
            );
            while let Some(suggestion) = suggestion_records.get(state.next_suggestion) {
                let bytes = serde_json::to_vec(suggestion)?.len();
                ensure!(
                    bytes <= request.max_bytes,
                    "suggestion record needs {bytes} bytes; increase max_bytes"
                );
                if count == request.limit || response.record_bytes + bytes > request.max_bytes {
                    response.truncated = true;
                    break;
                }
                response.record_bytes += bytes;
                count += 1;
                response.suggestions.push(suggestion.clone());
                state.next_suggestion += 1;
            }
        }
        if response.truncated {
            ensure!(count > 0, "max_bytes cannot fit the next record");
            response.cursor = Some(serde_json::to_string(&state)?);
        }
        let framing = serde_json::to_vec(&response)?
            .len()
            .saturating_sub(response.record_bytes);
        ensure!(
            framing <= 16 * 1024,
            "changes response framing exceeds 16 KiB"
        );
        Ok(response)
    }
}

struct SuggestionInput {
    path: String,
    text: String,
    endpoint: ChangeEndpoint,
}

fn suggestion_views(values: &[SuggestionInput]) -> Vec<crate::suggestions::FileView<'_>> {
    values
        .iter()
        .map(|value| crate::suggestions::FileView {
            root: value.endpoint.root.as_str(),
            path: value.endpoint.path.as_str(),
            profile: value.endpoint.profile_id,
            text: value.text.as_str(),
        })
        .collect()
}
