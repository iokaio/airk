// SPDX-License-Identifier: Apache-2.0
//! Bounded composition of complete canonical native reads. No transport or source I/O.
use airk_core::{
    combined_reads::{
        COMBINED_READ_SCHEMA, MemberAllocation, MemberAvailability, MemberPublication,
        PublicationVector, RANK_FUSION_POLICY, RankedOrigin, ResolvedMember, ResolvedSelection,
        SelectedScopeObservation, SharedBudgets,
        header_templates::{SharedNativeValues, share_native_headers},
        origins::factor_member_publications,
        rank_contribution,
        shared_values::share_capability_fields,
    },
    evidence::RevisionEnvelope,
    identity::{RevisionToken, Sha256Digest},
    index_catalog::{
        IndexId, MAX_SOURCE_ROOTS, SourceRootId, encoded_size, relative_path, unique_bounded,
    },
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompositionRequest {
    pub selection: ResolvedSelection,
    pub budgets: SharedBudgets,
    /// The native caller binds namespace and current grant/scope/source fences.
    pub authority: Sha256Digest,
    pub observed_from_unix_ms: u64,
    pub observed_to_unix_ms: u64,
    /// U108: each member's catalog display name, as the snapshot read it.
    /// Display text only: no identity input. A member without one composes
    /// without `index_name`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub index_names: BTreeMap<IndexId, String>,
}

/// A native-validated source projection, not a source spelling from a hit.
/// The native caller must prove published alias/root agreement before use.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberSource {
    pub source_root_id: SourceRootId,
    pub original_root: String,
    pub primary: bool,
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct MemberInput {
    pub index_id: IndexId,
    pub is_error: bool,
    /// An exact native publication observation for a member exhausted by a
    /// previous page, rather than another execution of the evidence query.
    pub exhausted: bool,
    /// Complete native envelope, or the complete canonical structured body.
    pub result: Value,
    pub sources: Vec<MemberSource>,
    /// U108: the member read's own elapsed time (absent for an exhausted
    /// member resumed from its retained publication).
    pub elapsed_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberOutcome {
    Complete,
    Exhausted,
    Failed,
    Unqueried,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposedMember {
    pub index_id: IndexId,
    /// U108: the index's catalog display name; display text only (no scope,
    /// snapshot, cursor or continuation identity input), absent when the
    /// composer was given none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_name: Option<String>,
    pub is_error: bool,
    pub allocation: MemberAllocation,
    pub outcome: MemberOutcome,
    /// Unmodified, including complete error/retry/readiness/verification data.
    pub result: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cutoff: Option<String>,
    /// U108: this member's own read time (its native call through its
    /// source projection and verification), present for a read member and
    /// absent for an unqueried or resumed-exhausted one. Documented timing:
    /// it is no input of any scope, snapshot, cursor or continuation
    /// identity; like the observation interval, it is part of the delivered
    /// bytes the delivery receipt's result digest covers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<u64>,
}

pub use airk_core::combined_reads::QualifiedOrigin;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RankedReference {
    /// JSON Pointer into this complete combined response, not a transport token.
    pub result_pointer: String,
    pub index_id: IndexId,
    /// U108: the index's catalog display name (display text only; never part
    /// of the origin, which follow-up reads reuse).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_name: Option<String>,
    pub local_rank: usize,
    pub rank_contribution: f64,
    pub origin: RankedOrigin,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombinedReadResult {
    pub schema: String,
    pub selection: SelectedScopeObservation,
    pub publication_vector: PublicationVector,
    pub rank_policy: String,
    pub members: Vec<ComposedMember>,
    pub ranking: Vec<RankedReference>,
    /// Capability-only lossless common fields. Member observations stay inline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared_values: Option<SharedNativeValues>,
    pub partial: bool,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CombinedReadFailure>,
    /// Safe only for the exact sole failed required member; all native detail remains inline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_retry: Option<airk_core::combined_reads::read_retry::SingleRequiredMemberRetry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombinedReadFailure {
    pub code: String,
    /// Partial dispatch is not permission to replay the whole query.
    pub retryable: bool,
    pub failed_members: Vec<IndexId>,
}

#[derive(Debug)]
pub struct CombinedReplyBudget;
impl std::fmt::Display for CombinedReplyBudget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("The complete combined result cannot fit the response frame; narrow the selection or result budgets.")
    }
}
impl std::error::Error for CombinedReplyBudget {}
impl CombinedReplyBudget {
    pub fn receipt(&self) -> Value {
        json!({"error":{"code":"combined_reply_budget","retryable":false,"query_replay_safe":false}})
    }
}

fn body(result: &Value) -> (&Value, &'static str) {
    match result.get("structuredContent") {
        Some(body) => (body, "/structuredContent"),
        None => (result, ""),
    }
}

fn canonicalize_envelope(result: &mut Value) -> Result<()> {
    if result.get("structuredContent").is_some() || result.get("content").is_none() {
        return Ok(());
    }
    let Some(content) = result.get("content").and_then(Value::as_array) else {
        anyhow::bail!("native result has an invalid content envelope");
    };
    let text = (content.len() == 1)
        .then(|| content[0].get("text").and_then(Value::as_str))
        .flatten();
    let canonical = text
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .filter(Value::is_object);
    if let Some(canonical) = canonical {
        // Materialize canonical JSON to give locators real JSON Pointer targets.
        // Original text, error flag and transport observations remain intact.
        result["structuredContent"] = canonical;
    } else {
        ensure!(
            result.get("isError").and_then(Value::as_bool) == Some(true),
            "native success envelope has no canonical JSON body"
        );
    }
    Ok(())
}

fn publication(body: &Value) -> Result<Option<RevisionEnvelope>> {
    if body.get("publication").is_none()
        && body.get("read").is_none()
        && let Some(evidence) = body.get("evidence").filter(|value| value.is_object())
    {
        return publication(evidence);
    }
    let Some(publication) = body
        .get("publication")
        .filter(|value| !value.is_null())
        .or_else(|| {
            body.pointer("/read/publication")
                .filter(|value| !value.is_null())
        })
    else {
        return Ok(None);
    };
    if let Ok(full) = serde_json::from_value::<RevisionEnvelope>(publication.clone()) {
        ensure!(
            full.incarnation == full.token.incarnation
                && full.generation == full.token.generation
                && full.revision == full.token.revision,
            "inconsistent native member publication"
        );
        return Ok(Some(full));
    }
    let token: RevisionToken = publication
        .get("token")
        .and_then(Value::as_str)
        .context("native member publication has no exact token")?
        .parse()?;
    let collection = body
        .get("collection")
        .and_then(Value::as_str)
        .context("native member publication has no collection")?;
    Ok(Some(RevisionEnvelope::new(collection.into(), token)))
}

fn validate_sources(sources: &[MemberSource]) -> Result<()> {
    ensure!(
        sources.len() <= MAX_SOURCE_ROOTS,
        "combined source capacity exceeded"
    );
    unique_bounded(
        &sources
            .iter()
            .map(|source| source.source_root_id)
            .collect::<Vec<_>>(),
        MAX_SOURCE_ROOTS,
    )?;
    let mut roots = BTreeSet::new();
    let mut aliases = BTreeSet::new();
    ensure!(
        sources.iter().filter(|source| source.primary).count() <= 1,
        "ambiguous primary source projection"
    );
    for source in sources {
        ensure!(
            roots.insert(&source.original_root)
                && source.original_root.len() <= 4096
                && std::path::Path::new(&source.original_root).is_absolute(),
            "invalid native source projection"
        );
        ensure!(
            source.aliases.len() <= 16 && (!source.primary || source.aliases.is_empty()),
            "invalid native source aliases"
        );
        for alias in &source.aliases {
            ensure!(
                !alias.is_empty()
                    && alias.len() <= 32
                    && alias.bytes().all(|byte| byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || b"_-".contains(&byte))
                    && aliases.insert(alias),
                "invalid or ambiguous native source alias"
            );
        }
    }
    ensure!(
        aliases.len() <= 16,
        "combined native alias capacity exceeded"
    );
    Ok(())
}

fn source_for_path<'a>(
    sources: &'a [MemberSource],
    path: &'a str,
) -> Result<(&'a MemberSource, &'a str)> {
    ensure!(path.len() <= 8192, "member citation path exceeds bound");
    if let Some(external) = path.strip_prefix('@') {
        for source in sources.iter().filter(|source| !source.primary) {
            for alias in &source.aliases {
                ensure!(
                    external != alias,
                    "external citation has no source-relative path"
                );
                if let Some(relative) = external
                    .strip_prefix(alias.as_str())
                    .and_then(|tail| tail.strip_prefix('/'))
                {
                    relative_path(relative, false)?;
                    return Ok((source, relative));
                }
            }
        }
    }
    relative_path(path, false)?;
    let primary = sources
        .iter()
        .find(|source| source.primary)
        .context("native primary source projection is unavailable")?;
    Ok((primary, path))
}

fn qualify(
    member: &ResolvedMember,
    sources: &[MemberSource],
    path: &str,
    publication: &RevisionEnvelope,
) -> Result<QualifiedOrigin> {
    let (source, relative) = source_for_path(sources, path)?;
    // U106(d): an open member is read whole and attributes no Project; its
    // evidence must still lie inside its own scope atoms.
    let project_ids = if member.memberships.is_empty() {
        airk_core::index_catalog::relative_path(relative, false)?;
        ensure!(
            member
                .scope
                .iter()
                .any(|atom| atom.matches_canonical(source.source_root_id, relative)),
            "member returned evidence outside its selected source scope"
        );
        Vec::new()
    } else {
        member.projects_for(source.source_root_id, relative)?
    };
    ensure!(
        !project_ids.is_empty() || member.memberships.is_empty(),
        "member returned evidence outside its selected source scope"
    );
    Ok(QualifiedOrigin {
        index_id: member.index_id,
        source_root_id: source.source_root_id,
        project_ids,
        source_relative_path: relative.into(),
        publication: publication.clone(),
    })
}

/// One canonical evidence list per native result. Auxiliary file/provenance
/// tables and reference summaries remain complete inside the member result.
fn records(body: &Value) -> Result<Vec<(String, &Value, &str)>> {
    if let Some(evidence) = body.get("evidence").filter(|value| value.is_object()) {
        return Ok(records(evidence)?
            .into_iter()
            .map(|(pointer, record, path)| (format!("/evidence{pointer}"), record, path))
            .collect());
    }
    if body.get("representation").and_then(Value::as_str) == Some("minimal") {
        return minimal_records(body, "");
    }
    for key in [
        "hits",
        "items",
        "facts",
        "outline",
        "relationships",
        "paths",
        "files",
    ] {
        if let Some(rows) = body
            .get(key)
            .and_then(Value::as_array)
            .filter(|rows| !rows.is_empty())
        {
            let mut records = Vec::with_capacity(rows.len());
            for (offset, row) in rows.iter().enumerate() {
                let path = row
                    .pointer("/citation/path")
                    .or_else(|| row.get("path"))
                    .or_else(|| row.as_str().map(|_| row))
                    .and_then(Value::as_str)
                    .context("native evidence record has no source path")?;
                records.push((format!("/{key}/{offset}"), row, path));
            }
            return Ok(records);
        }
    }
    if let Some(minimal) = body.get("minimal") {
        return minimal_records(minimal, "/minimal");
    }
    // Conditional FileInfo has a complete native source locator at the root,
    // rather than a hit array. Qualify that exact metadata record so a follow-up
    // can prove the same source-root identity without inventing a text range.
    if let Some(path) = body.get("path").and_then(Value::as_str)
        && body.get("content_hash").and_then(Value::as_str).is_some()
        && body.get("source").is_some_and(Value::is_object)
    {
        return Ok(vec![(String::new(), body, path)]);
    }
    Ok(Vec::new())
}

fn minimal_records<'a>(
    minimal: &'a Value,
    prefix: &str,
) -> Result<Vec<(String, &'a Value, &'a str)>> {
    let Some(items) = minimal.get("items").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let files = minimal
        .get("files")
        .and_then(Value::as_array)
        .context("minimal facts have no file table")?;
    let mut records = Vec::with_capacity(items.len());
    for (offset, row) in items.iter().enumerate() {
        let file = row
            .get("file")
            .map(Value::as_u64)
            .unwrap_or(Some(0))
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| files.get(index))
            .context("minimal fact has an invalid file binding")?;
        let path = file
            .get("path")
            .and_then(Value::as_str)
            .context("minimal file has no path")?;
        records.push((format!("{prefix}/items/{offset}"), row, path));
    }
    Ok(records)
}

/// Compose exactly one outcome per selected index in canonical order. The
/// caller owns serial admission, deadlines/cancellation and final live authority
/// fencing. This helper performs no source reads, retries or hidden fan-out.
pub fn compose(
    request: &CompositionRequest,
    inputs: Vec<MemberInput>,
) -> Result<CombinedReadResult> {
    compose_inner(request, inputs, false)
}

/// The actual capabilities route may share exactly equal native field groups.
/// Every other read keeps its ordinary complete inline member representation.
pub fn compose_capabilities(
    request: &CompositionRequest,
    inputs: Vec<MemberInput>,
) -> Result<CombinedReadResult> {
    compose_inner(request, inputs, true)
}

fn compose_inner(
    request: &CompositionRequest,
    inputs: Vec<MemberInput>,
    capabilities: bool,
) -> Result<CombinedReadResult> {
    request.selection.validate()?;
    let allocations = request.budgets.allocate(&request.selection)?;
    ensure!(
        request.observed_to_unix_ms >= request.observed_from_unix_ms,
        "invalid combined observation interval"
    );
    ensure!(
        inputs.len() <= request.selection.members.len(),
        "too many combined member results"
    );
    let mut inputs_by_id = BTreeMap::new();
    for mut input in inputs {
        ensure!(
            request.selection.member(input.index_id).is_some(),
            "result belongs to an unselected index"
        );
        ensure!(
            input.result.is_object(),
            "native member result must be an object"
        );
        canonicalize_envelope(&mut input.result)?;
        ensure!(
            !inputs_by_id.contains_key(&input.index_id),
            "duplicate member execution"
        );
        validate_sources(&input.sources)?;
        encoded_size(&input.result, airk_core::MAX_RESPONSE_BYTES)?;
        inputs_by_id.insert(input.index_id, input);
    }
    let mut members = Vec::with_capacity(allocations.len());
    let mut publications = Vec::with_capacity(allocations.len());
    let mut ranking = Vec::new();
    let mut partial = false;
    let mut truncated = false;
    let mut failed = Vec::new();
    for (offset, allocation) in allocations.into_iter().enumerate() {
        let member = &request.selection.members[offset];
        let Some(input) = inputs_by_id.remove(&allocation.index_id) else {
            partial = true;
            truncated = true;
            failed.push(allocation.index_id);
            publications.push(MemberPublication {
                index_id: allocation.index_id,
                publication: None,
            });
            members.push(ComposedMember {
                index_id: allocation.index_id,
                index_name: request.index_names.get(&allocation.index_id).cloned(),
                is_error: false,
                allocation,
                outcome: MemberOutcome::Unqueried,
                result: Value::Null,
                cutoff: Some(
                    if allocation.admits_read() {
                        "member_not_queried"
                    } else {
                        "shared_budget_exhausted"
                    }
                    .into(),
                ),
                elapsed_ms: None,
            });
            continue;
        };
        ensure!(
            input.exhausted || allocation.admits_read(),
            "member executed with no shared budget allocation"
        );
        let (body, body_pointer) = body(&input.result);
        ensure!(body.is_object(), "native member body must be an object");
        let member_failed = input.is_error
            || input
                .result
                .get("isError")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            || body.get("error").is_some_and(|error| !error.is_null());
        let publication = publication(body)?;
        publications.push(MemberPublication {
            index_id: allocation.index_id,
            publication: publication.clone(),
        });
        truncated |= body
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        truncated |= body
            .pointer("/evidence/truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        truncated |= body
            .pointer("/page/more")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if member_failed {
            partial = true;
            failed.push(allocation.index_id);
        } else if input.exhausted {
            ensure!(
                publication.is_some(),
                "exhausted member has no native publication observation"
            );
            ensure!(
                records(body)?.is_empty(),
                "exhausted member unexpectedly contains query evidence"
            );
        } else {
            let records = records(body)?;
            ensure!(
                records.len() <= allocation.limit,
                "member result exceeded its count allocation"
            );
            let mut text_bytes = 0usize;
            for (local_rank, (pointer, record, path)) in records.into_iter().enumerate() {
                text_bytes = text_bytes
                    .checked_add(
                        record
                            .get("text")
                            .and_then(Value::as_str)
                            .map_or(0, str::len),
                    )
                    .context("combined text byte accounting overflow")?;
                let publication = publication
                    .as_ref()
                    .context("native evidence has no member publication")?;
                ranking.push(RankedReference {
                    result_pointer: format!("/members/{offset}/result{body_pointer}{pointer}"),
                    index_id: allocation.index_id,
                    index_name: request.index_names.get(&allocation.index_id).cloned(),
                    local_rank,
                    rank_contribution: rank_contribution(local_rank),
                    origin: RankedOrigin::from_full(
                        qualify(member, &input.sources, path, publication)?,
                        offset,
                        &publications,
                    )?,
                });
            }
            ensure!(
                text_bytes <= allocation.max_bytes,
                "member result exceeded its text allocation"
            );
        }
        members.push(ComposedMember {
            index_id: allocation.index_id,
            index_name: request.index_names.get(&allocation.index_id).cloned(),
            is_error: member_failed,
            allocation,
            outcome: if member_failed {
                MemberOutcome::Failed
            } else if input.exhausted {
                MemberOutcome::Exhausted
            } else {
                MemberOutcome::Complete
            },
            result: input.result,
            cutoff: (input.exhausted && !member_failed).then(|| "continuation_exhausted".into()),
            elapsed_ms: if input.exhausted {
                None
            } else {
                input.elapsed_ms
            },
        });
    }
    // All candidates have a single origin/index contribution. Independent
    // indexes with identical paths or bytes are deliberately not deduplicated.
    ranking.sort_by(|left, right| {
        right
            .rank_contribution
            .total_cmp(&left.rank_contribution)
            .then_with(|| left.index_id.cmp(&right.index_id))
            .then_with(|| left.origin.source_root_id.cmp(&right.origin.source_root_id))
            .then_with(|| {
                left.origin
                    .source_relative_path
                    .cmp(&right.origin.source_relative_path)
            })
            .then_with(|| left.local_rank.cmp(&right.local_rank))
    });
    let mut result = CombinedReadResult {
        schema: COMBINED_READ_SCHEMA.into(),
        selection: request.selection.observation()?,
        publication_vector: PublicationVector {
            authority: request.authority,
            observed_from_unix_ms: request.observed_from_unix_ms,
            observed_to_unix_ms: request.observed_to_unix_ms,
            members: publications,
        },
        rank_policy: RANK_FUSION_POLICY.into(),
        members,
        ranking,
        shared_values: None,
        partial,
        truncated,
        read_retry: None,
        error: (partial && request.selection.member_availability == MemberAvailability::Require)
            .then(|| CombinedReadFailure {
                code: "member_unavailable".into(),
                retryable: false,
                failed_members: failed,
            }),
    };
    if capabilities {
        let mut bodies = result
            .members
            .iter_mut()
            .filter(|member| member.outcome == MemberOutcome::Complete)
            .map(|member| &mut member.result)
            .collect::<Vec<_>>();
        result.shared_values =
            share_capability_fields(&mut bodies)?.map(|table| SharedNativeValues::Capabilities {
                values: table.values,
            });
    } else {
        for (offset, member) in result.members.iter_mut().enumerate() {
            if matches!(
                member.outcome,
                MemberOutcome::Complete | MemberOutcome::Exhausted
            ) {
                factor_member_publications(
                    &mut member.result,
                    member.index_id,
                    offset,
                    &result.publication_vector,
                )?;
            }
        }
        let mut bodies = result
            .members
            .iter_mut()
            .filter(|member| {
                matches!(
                    member.outcome,
                    MemberOutcome::Complete | MemberOutcome::Exhausted
                )
            })
            .map(|member| &mut member.result)
            .collect::<Vec<_>>();
        result.shared_values =
            share_native_headers(&mut bodies)?.map(|table| SharedNativeValues::Headers {
                values: table.values,
            });
    }
    result.publication_vector.validate()?;
    result.read_retry =
        airk_core::combined_reads::read_retry::proposed_retry(&serde_json::to_value(&result)?);
    encoded_size(&result, airk_core::MAX_RESPONSE_BYTES).map_err(|_| CombinedReplyBudget)?;
    Ok(result)
}

#[cfg(test)]
mod tests;
