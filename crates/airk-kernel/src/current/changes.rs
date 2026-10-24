// SPDX-License-Identifier: Apache-2.0
//! Current evidence validation; expired baselines never claim an empty change set.
use crate::Kernel;
use airk_core::{changes::*, discovery, evidence::*};
use anyhow::Result;

impl Kernel {
    pub fn changes(&self, request: &ChangesRequest) -> Result<ChangesResponse> {
        request.validate()?;
        let read = self.current_reader()?;
        let roots = read
            .collection_state()?
            .ok_or_else(|| anyhow::anyhow!("current configuration missing"))?
            .configuration
            .roots;
        for selector in &request.selectors {
            if let RememberedSelector::Path { path } | RememberedSelector::Citation { path, .. } =
                selector
            {
                airk_store::roots::source_location(&self.root_id(), &roots, path)?;
                if let Some((alias, _)) = path.strip_prefix('@').and_then(|p| p.split_once('/')) {
                    anyhow::ensure!(
                        request.since.is_some()
                            || roots.contains_key(alias)
                            || read.artifact(path)?.is_some(),
                        "unknown indexed source alias"
                    );
                }
            }
        }
        let token = super::runtime::revision(read.token()?);
        let publication = RevisionEnvelope::new(self.collection.clone(), token);
        let recent = match read.recent_changes(airk_store::current::recent::unix_seconds()) {
            Ok(view) => view,
            Err(error)
                if error
                    .downcast_ref::<airk_store::current::recent::InvalidRecentChanges>()
                    .is_some() =>
            {
                let floor = CoverageFloor {
                    complete_after: token.generation,
                    reason: Some(CoverageReason::Corrupt),
                };
                airk_store::current::recent::RecentChanges {
                    coverage: ChangeCoverage {
                        files: floor.clone(),
                        code: floor.clone(),
                        documents: floor.clone(),
                        ancestry: floor.clone(),
                        resolution: floor,
                    },
                    parents: vec![],
                    events: vec![],
                    view_digest: airk_core::identity::framed_digest(
                        b"recent-change-view/1",
                        &[b"corrupt"],
                    ),
                }
            }
            Err(error) => return Err(error),
        };
        let mut response = ChangesResponse {
            status: ChangesStatus::Ready,
            publication,
            from: request.since,
            to: token,
            current: token,
            baseline_status: match request.since {
                None => BaselineStatus::NotRequested,
                Some(t) if t == token || recent.verifies(t) => BaselineStatus::Verified,
                _ => BaselineStatus::BaselineUnavailable,
            },
            coverage: Some(recent.coverage.clone()),
            changes: vec![],
            invalidations: vec![],
            validations: vec![],
            suggestions: vec![],
            suggestion_coverage: request.include_suggestions.then(|| SuggestionCoverage {
                version: 1,
                status: SuggestionStatus::Unavailable,
                baseline_representation: None,
                compared_pairs: 0,
                examined_files: 0,
                examined_text_bytes: 0,
                reason: Some(
                    "historical similarity candidates are unavailable in the current index".into(),
                ),
            }),
            freshness: self.changes_freshness(),
            record_bytes: 0,
            truncated: false,
            cursor: None,
        };
        let binding = discovery::binding(
            &token.to_string(),
            "current-changes/2",
            (
                &request.selectors,
                &request.path_prefix,
                request.since,
                request.include_suggestions,
                request.include_symbols,
                request.include_documents,
                recent.view_digest,
            ),
        )?;
        let changes = if response.baseline_status == BaselineStatus::Verified {
            request
                .since
                .map(|since| coalesce(&recent.events, since.generation, request))
                .transpose()?
                .unwrap_or_default()
        } else {
            vec![]
        };
        let after = match discovery::decode(request.cursor.as_deref(), &binding) {
            Ok(after) => match after {
                None => 0,
                Some(key) => match usize::try_from(key.ordinal)
                    .ok()
                    .and_then(|n| n.checked_add(1))
                    .filter(|n| *n <= changes.len() + request.selectors.len())
                {
                    Some(n) => n,
                    None => {
                        response.status = ChangesStatus::CursorExpired;
                        return Ok(response);
                    }
                },
            },
            Err(_) if request.cursor.is_some() => {
                response.status = ChangesStatus::CursorExpired;
                return Ok(response);
            }
            Err(e) => return Err(e),
        };
        for (ordinal, change) in changes.iter().enumerate().skip(after) {
            if !admit(&mut response, change, ordinal, request, &binding)? {
                return Ok(response);
            }
            response.changes.push(change.clone());
        }
        for (ordinal, selector) in request
            .selectors
            .iter()
            .enumerate()
            .skip(after.saturating_sub(changes.len()))
        {
            let requested = match selector {
                RememberedSelector::Path { path } | RememberedSelector::Citation { path, .. } => {
                    Some(path.clone())
                }
                _ => None,
            };
            let path = match selector {
                RememberedSelector::Path { path } | RememberedSelector::Citation { path, .. } => {
                    Some(path.clone())
                }
                RememberedSelector::Artifact { artifact_id } => {
                    read.artifact_path(&self.store, *artifact_id)?
                }
                RememberedSelector::Symbol { symbol_key } => read
                    .unit_owner(
                        &self.store,
                        airk_core::continuity::ContinuityKey::Symbol(*symbol_key),
                    )?
                    .map(|v| v.0),
                RememberedSelector::Document { document_key } => read
                    .unit_owner(
                        &self.store,
                        airk_core::continuity::ContinuityKey::Document(*document_key),
                    )?
                    .map(|v| v.0),
            }
            .filter(|path| path.starts_with(&request.path_prefix));
            let file = path
                .as_ref()
                .map(|path| read.file_metadata(&self.store, path))
                .transpose()?
                .flatten();
            let mut validation = EvidenceValidation {
                ordinal, selector: selector.clone(), outcome: ValidationOutcome::Unknown,
                artifact_id: file.as_ref().map(|f| f.state.artifact_id), current: file.as_ref().map(|f| f.state.clone()),
                requested_path: requested, resolved_path: file.as_ref().and(path), basis: ContinuityBasis::Unknown,
                validated_dimensions: vec![], candidates: vec![], candidates_truncated: false,
                reason: Some("identity is absent or outside the requested scope; current evidence cannot prove historical removal".into()),
            };
            if let Some(file) = file {
                validation.basis = ContinuityBasis::SamePathObservation;
                validation.reason = None;
                validation.outcome = match selector {
                    RememberedSelector::Citation { source_hash, .. } => {
                        validation
                            .validated_dimensions
                            .push(ChangeDimension::Content);
                        if *source_hash == file.state.source_hash {
                            ValidationOutcome::Unchanged
                        } else {
                            ValidationOutcome::Modified
                        }
                    }
                    _ => ValidationOutcome::Current,
                };
            }
            if !admit(
                &mut response,
                &validation,
                changes.len() + ordinal,
                request,
                &binding,
            )? {
                break;
            }
            response.validations.push(validation);
        }
        Ok(response)
    }
}

fn admit(
    response: &mut ChangesResponse,
    value: &impl serde::Serialize,
    ordinal: usize,
    request: &ChangesRequest,
    binding: &str,
) -> Result<bool> {
    let bytes = serde_json::to_vec(value)?.len();
    let count = response.changes.len() + response.validations.len();
    if count >= request.limit || bytes > request.max_bytes.saturating_sub(response.record_bytes) {
        anyhow::ensure!(
            count != 0,
            "change/validation record exceeds response budget"
        );
        response.truncated = true;
        response.cursor = Some(discovery::encode(
            binding,
            &discovery::DiscoveryKey {
                ordinal: (ordinal - 1) as i64,
                ..Default::default()
            },
        )?);
        return Ok(false);
    }
    response.record_bytes += bytes;
    Ok(true)
}

fn coalesce(
    events: &[ChangeEvent],
    since: u64,
    request: &ChangesRequest,
) -> Result<Vec<CoalescedChange>> {
    let mut rows = std::collections::BTreeMap::<String, CoalescedChange>::new();
    for event in events.iter().filter(|event| event.generation > since) {
        if matches!(event.subject, ChangeSubject::Symbol { .. }) && !request.include_symbols {
            continue;
        }
        if matches!(event.subject, ChangeSubject::Document { .. }) && !request.include_documents {
            continue;
        }
        if !request.path_prefix.is_empty()
            && ![&event.before, &event.after]
                .into_iter()
                .flatten()
                .any(|endpoint| {
                    endpoint
                        .citation_path
                        .as_deref()
                        .unwrap_or(&endpoint.path)
                        .starts_with(&request.path_prefix)
                })
        {
            continue;
        }
        let key = serde_json::to_string(&event.subject)?;
        let position = ChangePosition {
            generation: event.generation,
            event_id: event.event_id,
        };
        if let Some(row) = rows.get_mut(&key) {
            row.last = position;
            row.observations += 1;
            row.after = event.after.clone();
            row.after_detail = event.after_detail.clone();
            for dimension in &event.dimensions {
                if !row.dimensions.contains(dimension) {
                    row.dimensions.push(*dimension);
                }
            }
            row.dimensions.sort();
            if !row.causes.contains(&event.cause) {
                row.causes.push(event.cause);
            }
            if row.basis != event.basis {
                row.basis = ContinuityBasis::Unknown;
            }
        } else {
            rows.insert(
                key,
                CoalescedChange {
                    subject: event.subject.clone(),
                    first_generation: event.generation,
                    last: position,
                    observations: 1,
                    before: event.before.clone(),
                    after: event.after.clone(),
                    before_detail: event.before_detail.clone(),
                    after_detail: event.after_detail.clone(),
                    dimensions: event.dimensions.clone(),
                    causes: vec![event.cause],
                    basis: event.basis,
                },
            );
        }
    }
    let mut rows: Vec<_> = rows.into_values().collect();
    rows.sort_by_key(|row| (row.last.generation, row.last.event_id));
    Ok(rows)
}
