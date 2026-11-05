// SPDX-License-Identifier: Apache-2.0
//! Read-only checks of the exact saved sources referenced by one returned projection.
use crate::Kernel;
use airk_core::{
    identity::{Sha256Digest, framed_digest},
    verification::*,
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis().min(u64::MAX as u128) as u64)
}

fn qualifications() -> Vec<VerificationQualification> {
    [
        (
            "extraction_not_certified",
            "Saved-byte equality does not establish extraction or OCR accuracy.",
        ),
        (
            "unsaved_buffers_not_checked",
            "Native checks observe saved disk bytes, not unsaved editor buffers.",
        ),
        (
            "coverage_not_certified",
            "Returned-source checks do not establish complete or relevant search.",
        ),
        (
            "unresolved_written_target",
            "Matching host bytes do not resolve written language targets or calls.",
        ),
    ]
    .into_iter()
    .map(|(kind, reason)| VerificationQualification {
        kind: kind.into(),
        verified: false,
        reason: reason.into(),
    })
    .collect()
}

fn selection_digest(value: &Value) -> Result<Sha256Digest> {
    fn normalize(value: &mut Value) {
        let Some(fields) = value.as_object_mut() else {
            return;
        };
        for key in [
            "request_id",
            "generation",
            "freshness",
            "discovery_readiness",
            "semantic_readiness",
            "discovery_observation",
            "source_verification",
        ] {
            fields.remove(key);
        }
        if let Some(publication) = fields.get_mut("publication")
            && let Some(token) = publication.get("token").cloned()
        {
            *publication = serde_json::json!({"token":token});
        }
        if let Some(read) = fields.get_mut("read").and_then(Value::as_object_mut) {
            for key in ["publication", "current_publication"] {
                if let Some(publication) = read.get_mut(key)
                    && let Some(token) = publication.get("token").cloned()
                {
                    *publication = serde_json::json!({"token":token});
                }
            }
        }
        if let Some(evidence) = fields.get_mut("evidence")
            && evidence.is_object()
        {
            normalize(evidence);
        }
        if let Some(steps) = fields.get_mut("steps").and_then(Value::as_array_mut) {
            for step in steps {
                if let Some(report) = step.as_object_mut() {
                    report.remove("elapsed_ms");
                    if let Some(result) = report.get_mut("result") {
                        normalize(result);
                    }
                }
            }
        }
    }
    let mut selection = value.clone();
    normalize(&mut selection);
    Ok(framed_digest(
        b"saved-source-verification-selection/1",
        &[&serde_json::to_vec(&selection)?],
    ))
}

/// Linear encoded-cost admission, including association tables and headroom for
/// completed outcomes. Never repeatedly serialize an ever-smaller large table.
fn trim_metadata(receipt: &mut VerificationReceipt, limit: usize) -> Result<()> {
    use std::collections::BTreeSet;
    let sources = std::mem::take(&mut receipt.sources);
    let references = std::mem::take(&mut receipt.references);
    let origins = std::mem::take(&mut receipt.origins);
    let mut used = serde_json::to_vec(receipt)?.len() + 1024;
    anyhow::ensure!(used <= limit, VerificationReplyBudget);
    let mut reference_costs = BTreeMap::<Sha256Digest, usize>::new();
    for reference in &references {
        *reference_costs.entry(reference.source_id).or_default() +=
            serde_json::to_vec(reference)?.len() + 1;
    }
    let mut kept = 0;
    let mut kept_origins = BTreeSet::new();
    for source in sources.iter().take(MAX_SOURCES) {
        let origin_cost = if kept_origins.contains(&source.origin_id) {
            0
        } else {
            serde_json::to_vec(
                origins
                    .iter()
                    .find(|origin| origin.id == source.origin_id)
                    .context("verification origin missing")?,
            )?
            .len()
                + 1
        };
        let cost = serde_json::to_vec(source)?.len()
            + 1
            + reference_costs.get(&source.id).copied().unwrap_or(0)
            + 128
            + origin_cost;
        if used + cost > limit {
            break;
        }
        used += cost;
        kept += 1;
        kept_origins.insert(source.origin_id.clone());
    }
    receipt.source_rows_omitted = sources.len() - kept;
    receipt.sources = sources.into_iter().take(kept).collect();
    let ids: BTreeSet<_> = receipt.sources.iter().map(|source| source.id).collect();
    receipt.references = references
        .into_iter()
        .filter(|reference| ids.contains(&reference.source_id))
        .collect();
    receipt.origins = origins
        .into_iter()
        .filter(|origin| kept_origins.contains(&origin.id))
        .collect();
    if receipt.source_rows_omitted > 0 {
        receipt.all_references_accounted = false;
        receipt.limits.hit.push("receipt_bytes".into());
    }
    Ok(())
}

impl Kernel {
    /// Check a final one-member projection through its already installed read
    /// authority. Inputs remain untrusted: only retained current identities and
    /// authorized source locations can become eligible saved-byte checks.
    pub fn verify_scoped_projection(
        &self,
        project: airk_core::service::ProjectBinding,
        value: Value,
        controls: VerificationControls,
    ) -> Result<Value> {
        self.verify_scoped_projection_allocated(project, value, controls, None)
    }

    /// An internal member allocation may be zero but cannot widen the valid
    /// caller policy. Public verification controls retain their positive bounds.
    pub fn verify_scoped_projection_allocated(
        &self,
        project: airk_core::service::ProjectBinding,
        value: Value,
        controls: VerificationControls,
        allocation: Option<VerificationAllocation>,
    ) -> Result<Value> {
        self.verify_scoped_projection_budgeted(project, value, controls, allocation, None)
    }

    /// Metadata bytes share the outer composition allocation just as source
    /// bytes do. Establish this cap before any saved-source reads or hashing.
    pub fn verify_scoped_projection_budgeted(
        &self,
        project: airk_core::service::ProjectBinding,
        mut value: Value,
        controls: VerificationControls,
        allocation: Option<VerificationAllocation>,
        receipt_bytes_budget: Option<usize>,
    ) -> Result<Value> {
        use airk_core::scoped_reads::ScopeReadRefused;
        anyhow::ensure!(self.scoped_read_fingerprint()?.is_some(), ScopeReadRefused);
        airk_core::index_catalog::encoded_size(
            &(&value, &controls, &allocation, &receipt_bytes_budget),
            airk_core::service::MAX_FRAME_BYTES - 4096,
        )?;
        anyhow::ensure!(
            value.is_object(),
            "verification projection must be an object"
        );
        anyhow::ensure!(
            value.get("source_verification").is_none(),
            "verification projection already carries a response-wide receipt"
        );
        let registration = airk_core::service::ProjectRegistration::new(
            self.root_id(),
            self.data_root.to_string_lossy().into_owned(),
            self.collection.clone(),
        )?;
        anyhow::ensure!(registration.binding == project, ScopeReadRefused);
        let read = self.current_reader()?;
        let publication = crate::current::runtime::envelope(&read)?;
        self.annotate_scoped_read(&mut value)?;
        // This is a dated owner observation, separate from each evidence item's
        // exact member publication. Do not supply a missing evidence token from
        // the owner's current head.
        value["verification_owner_publication"] = serde_json::to_value(publication)?;
        if let Some(requested) = controls.resolve()? {
            let budget = allocation.map_or(Ok(requested), |share| share.apply(requested))?;
            let receipt_budget = receipt_bytes_budget.unwrap_or(MAX_RECEIPT_BYTES);
            anyhow::ensure!(
                (1024..=MAX_RECEIPT_BYTES).contains(&receipt_budget),
                VerificationReplyBudget
            );
            let receipt = self.verify_returned_sources_allocated_before(
                &value,
                budget,
                None,
                receipt_budget_for_reply(&value)?.min(receipt_budget),
                true,
            )?;
            attach_response(&mut value, receipt)?;
        } else {
            anyhow::ensure!(
                allocation.is_none(),
                "off verification cannot carry a member allocation"
            );
            anyhow::ensure!(
                receipt_bytes_budget.is_none(),
                "off verification cannot carry a receipt allocation"
            );
        }
        read.token()?;
        self.annotate_scoped_read(&mut value)?;
        validate_projection(&value)?;
        Ok(value)
    }

    /// Per-request cooperative cancellation; never changes scope or admission.
    pub fn with_verification_cancellation(
        mut self,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        self.verification_cancellation = Some(cancelled);
        self
    }

    fn verification_cancelled(&self) -> bool {
        self.verification_cancellation
            .as_ref()
            .is_some_and(|cancelled| cancelled.load(std::sync::atomic::Ordering::Acquire))
    }
    /// Internal P4 handler. Call only with the final returned evidence projection;
    /// off bypasses this handler entirely. Does not refresh or mutate the index.
    pub fn verify_returned_sources(
        &self,
        value: &Value,
        budget: VerificationBudget,
    ) -> Result<VerificationReceipt> {
        self.verify_returned_sources_before(value, budget, None, receipt_budget_for_reply(value)?)
    }

    pub(crate) fn verify_returned_sources_before(
        &self,
        value: &Value,
        budget: VerificationBudget,
        outer_deadline: Option<Instant>,
        receipt_bytes_budget: usize,
    ) -> Result<VerificationReceipt> {
        self.verify_returned_sources_allocated_before(
            value,
            budget,
            outer_deadline,
            receipt_bytes_budget,
            false,
        )
    }

    fn verify_returned_sources_allocated_before(
        &self,
        value: &Value,
        budget: VerificationBudget,
        outer_deadline: Option<Instant>,
        receipt_bytes_budget: usize,
        allow_zero: bool,
    ) -> Result<VerificationReceipt> {
        // A Kernel owns one index. Combined reads retain the member owners'
        // receipts under their native envelopes; never resolve another index's
        // path through this Kernel merely because its spelling matches.
        anyhow::ensure!(
            value["schema"] != "airk.combined-read/1"
                && value["structuredContent"]["schema"] != "airk.combined-read/1",
            "combined saved-source verification requires the originating member owners"
        );
        anyhow::ensure!(
            budget.mode != VerificationMode::Off,
            "off verification must bypass source checking"
        );
        VerificationControls {
            verification: Some(budget.mode),
            verification_max_sources: Some(if allow_zero {
                budget.max_sources.max(1)
            } else {
                budget.max_sources
            }),
            verification_max_bytes: Some(if allow_zero {
                budget.max_bytes.max(1)
            } else {
                budget.max_bytes
            }),
            verification_deadline_ms: Some(if allow_zero {
                budget.deadline_ms.max(1)
            } else {
                budget.deadline_ms
            }),
        }
        .resolve()?;
        anyhow::ensure!(
            (1024..=MAX_RECEIPT_BYTES).contains(&receipt_bytes_budget),
            VerificationReplyBudget
        );
        let mut limits: VerificationLimits = budget.into();
        limits.receipt_bytes_budget = receipt_bytes_budget;
        let observed_from_unix_ms = now_ms();
        let began = Instant::now();
        let deadline = began + Duration::from_millis(budget.deadline_ms);
        let deadline = outer_deadline.map_or(deadline, |outer| outer.min(deadline));
        limits.effective_deadline_ms = deadline.saturating_duration_since(began).as_millis() as u64;
        let inventory = ReferenceInventory::collect(value);
        anyhow::ensure!(
            !inventory.contains_combined,
            "combined saved-source verification requires the originating member owners"
        );
        if inventory.complete && inventory.references.is_empty() {
            let receipt = VerificationReceipt {
                schema: "airk.saved-source-verification/1".into(),
                method: "sha256-full-saved-source/1".into(),
                mode: budget.mode,
                status: "not_applicable".into(),
                scope_digest: framed_digest(
                    b"saved-source-verification-empty-scope/1",
                    &[
                        self.root_id().as_bytes(),
                        self.data_root.to_string_lossy().as_bytes(),
                        self.collection.as_bytes(),
                    ],
                ),
                selection_digest: selection_digest(value)?,
                observed_from_unix_ms,
                observed_to_unix_ms: now_ms().max(observed_from_unix_ms),
                publication_fence: "unknown".into(),
                inventory_complete: true,
                all_references_accounted: true,
                source_rows_omitted: 0,
                requirements_satisfied: None,
                counts: VerificationCounts {
                    references: Some(0),
                    requested_sources: Some(0),
                    unchecked_sources: Some(0),
                    ..Default::default()
                },
                limits,
                origins: vec![],
                sources: vec![],
                references: vec![],
                qualifications: qualifications(),
                error: None,
            };
            anyhow::ensure!(
                serde_json::to_vec(&receipt)?.len() <= receipt_bytes_budget,
                VerificationReplyBudget
            );
            return Ok(receipt);
        }
        let read = self.current_reader()?;
        let original = crate::current::runtime::envelope(&read)?.token;
        let state = read
            .collection_state()?
            .context("current configuration missing")?;
        // Loading configuration does not probe unrelated source roots. Each
        // actually selected source must still agree with its published alias.
        let configured =
            airk_store::roots::RootConfig::load(self.store.directory(), &self.root_id())?;
        let registration = airk_core::service::ProjectRegistration::new(
            self.root_id(),
            self.data_root.to_string_lossy().into_owned(),
            self.collection.clone(),
        )?;
        let scope_digest = framed_digest(
            b"saved-source-verification-scope/1",
            &[
                registration.binding.as_bytes(),
                &serde_json::to_vec(&state.configuration.roots)?,
                &serde_json::to_vec(&configured.roots)?,
            ],
        );
        let scope_digest = match self.scoped_read_fingerprint()? {
            Some(authority) => framed_digest(
                b"saved-source-verification-scoped/1",
                &[scope_digest.as_bytes(), authority.as_bytes()],
            ),
            None => scope_digest,
        };
        let selection_digest = selection_digest(value)?;
        let mut receipt = VerificationReceipt {
            schema: "airk.saved-source-verification/1".into(),
            method: "sha256-full-saved-source/1".into(),
            mode: budget.mode,
            status: "unverified".into(),
            scope_digest,
            selection_digest,
            observed_from_unix_ms,
            observed_to_unix_ms: observed_from_unix_ms,
            publication_fence: "current".into(),
            inventory_complete: inventory.complete,
            all_references_accounted: inventory.complete,
            source_rows_omitted: 0,
            requirements_satisfied: Some(false),
            counts: VerificationCounts {
                references: inventory
                    .complete
                    .then_some(inventory.references_lower_bound),
                references_lower_bound: inventory.references_lower_bound,
                ..Default::default()
            },
            limits,
            origins: vec![],
            sources: vec![],
            references: vec![],
            qualifications: qualifications(),
            error: None,
        };
        if !inventory.complete {
            receipt.limits.hit.push("reference_inventory".into());
        }
        let mut origins = BTreeMap::<String, String>::new();
        let mut source_positions = BTreeMap::<Sha256Digest, usize>::new();
        let mut identities =
            BTreeMap::<String, Option<(Sha256Digest, Option<Sha256Digest>)>>::new();
        let scoped_member = self
            .scope_observation()
            .and_then(|observation| observation.members.first());
        for reference in &inventory.references {
            let publication = reference.publication.unwrap_or(original);
            let publication_text = publication.to_string();
            let origin_id = if let Some(id) = origins.get(&publication_text) {
                id.clone()
            } else {
                if origins.len() == 32 {
                    receipt.inventory_complete = false;
                    receipt.all_references_accounted = false;
                    break;
                }
                let id = format!("origin-{}", origins.len());
                origins.insert(publication_text.clone(), id.clone());
                receipt.origins.push(VerificationOrigin {
                    id: id.clone(),
                    index_id: scoped_member.map(|member| member.index_id.to_string()),
                    legacy_project: registration.binding.to_string(),
                    collection: self.collection.clone(),
                    incarnation: publication.incarnation,
                    publication,
                    project_ids: scoped_member
                        .map(|member| {
                            member
                                .memberships
                                .iter()
                                .map(|membership| membership.project_id.to_string())
                                .collect()
                        })
                        .unwrap_or_default(),
                });
                id
            };
            let path = reference.path.as_deref();
            if self.read_scope.is_some() {
                read.require_path(path.ok_or(airk_core::scoped_reads::ScopeReadRefused)?)?;
            }
            let current_reference =
                reference.publication == Some(original) && !reference.historical;
            // Only bounded returned identities are looked up, from the same head;
            // no whole inventory, coverage scan or extraction object is loaded.
            if current_reference
                && let Some(path) = path
                && !identities.contains_key(path)
                && identities.len() < MAX_SOURCES
            {
                let identity = read
                    .artifact(path)?
                    .map(|artifact| -> Result<_> {
                        let hash: Sha256Digest = artifact.content_hash.parse()?;
                        let profile = artifact
                            .retained
                            .as_ref()
                            .and_then(|retained| retained.profile.as_ref())
                            .map(|profile| profile.identity())
                            .transpose()?;
                        Ok((hash, profile))
                    })
                    .transpose()?;
                identities.insert(path.into(), identity);
            }
            let identity = path
                .and_then(|path| identities.get(path))
                .and_then(|identity| *identity);
            let expected = reference
                .expected_hash
                .or(identity.map(|identity| identity.0));
            let location = if current_reference {
                path.and_then(|path| {
                    airk_store::roots::source_location(
                        &self.root_id(),
                        &state.configuration.roots,
                        path,
                    )
                    .ok()
                })
            } else {
                None
            };
            let location_root = location
                .as_ref()
                .map(|location| location.root.as_str())
                .or(reference.original_root.as_deref())
                .unwrap_or("");
            let location_relative = location
                .as_ref()
                .map(|location| location.relative_path.as_str())
                .or(reference.source_relative_path.as_deref())
                .or(path)
                .unwrap_or("");
            let attribution = location
                .as_ref()
                .map(|location| {
                    self.scoped_source_attribution(&location.root, &location.relative_path)
                })
                .transpose()?
                .flatten();
            let source_id = framed_digest(
                b"saved-source-verification-subject/1",
                &[
                    registration.binding.as_bytes(),
                    publication_text.as_bytes(),
                    location_root.as_bytes(),
                    location_relative.as_bytes(),
                    &[u8::from(reference.historical)],
                    if path.is_none() {
                        reference.pointer.as_bytes()
                    } else {
                        b""
                    },
                    &expected
                        .map(|hash| hash.to_string())
                        .unwrap_or_default()
                        .into_bytes(),
                ],
            );
            let position = if let Some(position) = source_positions.get(&source_id) {
                *position
            } else {
                let position = receipt.sources.len();
                source_positions.insert(source_id, position);
                let from = now_ms().max(observed_from_unix_ms);
                let (check_state, reason) = if !current_reference {
                    (
                        SourceCheckState::PublicationExpired,
                        "reference_not_current",
                    )
                } else if path.is_none() || identity.is_none() || expected.is_none() {
                    (SourceCheckState::Unsupported, "source_identity_unavailable")
                } else if location.as_ref().is_none_or(|location| {
                    reference
                        .original_root
                        .as_ref()
                        .is_some_and(|root| root != &location.root)
                        || reference
                            .source_relative_path
                            .as_ref()
                            .is_some_and(|path| path != &location.relative_path)
                }) {
                    (SourceCheckState::Unsupported, "reference_root_mismatch")
                } else if expected != identity.map(|identity| identity.0) {
                    (SourceCheckState::Unsupported, "reference_identity_mismatch")
                } else if reference.profile_digest.is_some()
                    && reference.profile_digest != identity.and_then(|identity| identity.1)
                {
                    (SourceCheckState::Unsupported, "reference_profile_mismatch")
                } else {
                    (SourceCheckState::BudgetExhausted, "source_count_budget")
                };
                receipt.sources.push(VerifiedSource {
                    id: source_id,
                    origin_id: origin_id.clone(),
                    source_root_id: attribution
                        .as_ref()
                        .map(|(_, root_id, _)| root_id.to_string()),
                    original_root: if !current_reference {
                        reference.original_root.clone()
                    } else {
                        path.and_then(|path| {
                            airk_store::roots::source_location(
                                &self.root_id(),
                                &state.configuration.roots,
                                path,
                            )
                            .ok()
                        })
                        .map(|location| location.root)
                    },
                    source_relative_path: if !current_reference {
                        reference.source_relative_path.clone()
                    } else {
                        path.and_then(|path| {
                            airk_store::roots::source_location(
                                &self.root_id(),
                                &state.configuration.roots,
                                path,
                            )
                            .ok()
                        })
                        .map(|location| location.relative_path)
                    },
                    path: path.map(str::to_owned),
                    expected_hash: expected,
                    observed_hash: None,
                    profile_digest: identity
                        .and_then(|identity| identity.1)
                        .or(reference.profile_digest),
                    project_ids: attribution
                        .as_ref()
                        .map(|(_, _, projects)| projects.iter().map(ToString::to_string).collect())
                        .unwrap_or_default(),
                    reference_ids: vec![],
                    state: check_state,
                    reason: Some(reason.into()),
                    observed_from_unix_ms: from,
                    observed_to_unix_ms: from,
                    bytes_read: 0,
                });
                position
            };
            // Deduplicated references may disagree about a claimed extraction
            // profile. A preceding valid association cannot certify a later
            // forged profile claim for the same saved-byte subject.
            if current_reference
                && reference.profile_digest.is_some()
                && reference.profile_digest != identity.and_then(|identity| identity.1)
            {
                receipt.sources[position].state = SourceCheckState::Unsupported;
                receipt.sources[position].reason = Some("reference_profile_mismatch".into());
            }
            let id = format!("ref-{}", receipt.references.len());
            receipt.sources[position].reference_ids.push(id.clone());
            receipt.references.push(VerifiedReference {
                id,
                source_id,
                result_pointer: reference.pointer.clone(),
                kind: reference.kind.into(),
            });
        }
        let known_sources = receipt.sources.len();
        let identified_sources = receipt
            .sources
            .iter()
            .filter(|source| source.path.is_some() && source.expected_hash.is_some())
            .count();
        receipt.counts.requested_sources = (receipt.inventory_complete
            && identified_sources == known_sources)
            .then_some(known_sources);
        if !receipt.inventory_complete {
            receipt.counts.references = None;
        }
        receipt.counts.requested_sources_lower_bound = identified_sources;
        // Reserve receipt space before any source hash. Retain a clearly labeled
        // prefix if the complete associations cannot fit; never certify it.
        trim_metadata(&mut receipt, receipt_bytes_budget)?;
        let mut policies =
            BTreeMap::<String, Result<airk_ingest::source_scope::SelectedSourcePolicy>>::new();
        let mut attempted = 0;
        let allocation_exhausted =
            budget.max_sources == 0 || budget.max_bytes == 0 || budget.deadline_ms == 0;
        if allocation_exhausted && !receipt.sources.is_empty() {
            receipt.limits.hit.push("member_allocation".into());
        }
        for source in &mut receipt.sources {
            if source.state != SourceCheckState::BudgetExhausted {
                continue;
            }
            if allocation_exhausted {
                source.reason = Some("member_allocation".into());
                continue;
            }
            if attempted == budget.max_sources {
                if !receipt.limits.hit.iter().any(|hit| hit == "source_count") {
                    receipt.limits.hit.push("source_count".into());
                }
                continue;
            }
            attempted += 1;
            let path = source
                .path
                .as_deref()
                .context("eligible source path missing")?;
            let expected = source
                .expected_hash
                .context("eligible source hash missing")?;
            source.observed_from_unix_ms = now_ms().max(observed_from_unix_ms);
            let check = (|| -> Result<SourceCheck> {
                if self.verification_cancelled() {
                    return Ok(SourceCheck::unchecked(
                        SourceCheckState::BudgetExhausted,
                        0,
                        "cancellation",
                    ));
                }
                if Instant::now() >= deadline {
                    return Ok(SourceCheck::unchecked(
                        SourceCheckState::BudgetExhausted,
                        0,
                        "deadline",
                    ));
                }
                let location = airk_store::roots::source_location(
                    &self.root_id(),
                    &state.configuration.roots,
                    path,
                )?;
                if location.alias != "primary"
                    && configured.roots.get(&location.alias) != Some(&location.root)
                {
                    return Ok(SourceCheck::unchecked(
                        SourceCheckState::OutOfScope,
                        0,
                        "source_alias_changed",
                    ));
                }
                if self
                    .require_source_policy_source(&location.root, &location.relative_path)
                    .is_err()
                {
                    return Ok(SourceCheck::unchecked(
                        SourceCheckState::OutOfScope,
                        0,
                        "permanent_source_policy",
                    ));
                }
                let policy = policies
                    .entry(location.root.clone())
                    .or_insert_with(|| self.selected_source_policy(&location.root));
                let policy = policy
                    .as_mut()
                    .map_err(|_| anyhow::anyhow!("source ignore controls unavailable"))?;
                if !policy.allows(&location.relative_path)? {
                    return Ok(SourceCheck::unchecked(
                        SourceCheckState::OutOfScope,
                        0,
                        "current_source_policy",
                    ));
                }
                Ok(airk_ingest::verify_saved_source(
                    Path::new(&location.root),
                    Path::new(&location.relative_path),
                    expected,
                    budget.max_bytes.saturating_sub(receipt.counts.bytes_read),
                    deadline,
                    &|| self.verification_cancelled(),
                ))
            })()
            .unwrap_or_else(|_| {
                SourceCheck::unchecked(
                    SourceCheckState::Unsupported,
                    0,
                    "source_policy_or_locator_unavailable",
                )
            });
            source.state = check.state;
            source.observed_hash = check.observed_hash;
            source.reason = check.reason;
            source.bytes_read = check.bytes_read;
            source.observed_to_unix_ms = now_ms().max(source.observed_from_unix_ms);
            receipt.counts.bytes_read += check.bytes_read;
            if source.state == SourceCheckState::BudgetExhausted {
                let hit = if self.verification_cancelled() {
                    "cancellation"
                } else if Instant::now() >= deadline {
                    "deadline"
                } else {
                    "source_bytes"
                };
                if !receipt.limits.hit.iter().any(|value| value == hit) {
                    receipt.limits.hit.push(hit.into());
                }
            }
        }
        // A changed head/authority or alias map cannot renew an old check. No
        // new hash is substituted into the original returned evidence.
        // Unknown retained authority/integrity is a delivery refusal, not a
        // report-mode route around the native read fence.
        read.token()?;
        let current = crate::current::runtime::envelope(self.current_reader()?.as_ref())?.token;
        let roots_current =
            airk_store::roots::RootConfig::load(self.store.directory(), &self.root_id())?.roots
                == configured.roots;
        if current != original || !roots_current {
            receipt.publication_fence = "expired".into();
            for source in &mut receipt.sources {
                source.state = SourceCheckState::PublicationExpired;
                source.reason = Some("publication_or_scope_changed".into());
            }
        } else {
            // Ignore metadata may change without an index publication. Recheck
            // selected paths against a fresh policy before delivering matches.
            let mut final_policies =
                BTreeMap::<String, Result<airk_ingest::source_scope::SelectedSourcePolicy>>::new();
            for source in &mut receipt.sources {
                if !matches!(
                    source.state,
                    SourceCheckState::Matched | SourceCheckState::Mismatch
                ) {
                    continue;
                }
                if Instant::now() >= deadline || self.verification_cancelled() {
                    source.state = SourceCheckState::BudgetExhausted;
                    let hit = if self.verification_cancelled() {
                        "cancellation"
                    } else {
                        "deadline"
                    };
                    source.reason = Some(hit.into());
                    if !receipt.limits.hit.iter().any(|value| value == hit) {
                        receipt.limits.hit.push(hit.into());
                    }
                    continue;
                }
                let allowed = source
                    .path
                    .as_deref()
                    .and_then(|path| {
                        airk_store::roots::source_location(
                            &self.root_id(),
                            &state.configuration.roots,
                            path,
                        )
                        .ok()
                    })
                    .is_some_and(|location| {
                        if self
                            .require_source_policy_source(&location.root, &location.relative_path)
                            .is_err()
                        {
                            return false;
                        }
                        final_policies
                            .entry(location.root.clone())
                            .or_insert_with(|| self.selected_source_policy(&location.root))
                            .as_mut()
                            .is_ok_and(|policy| {
                                policy.allows(&location.relative_path).unwrap_or(false)
                            })
                    });
                if !allowed {
                    source.state = SourceCheckState::OutOfScope;
                    source.reason = Some("source_policy_changed".into());
                }
            }
        }
        read.token()?;
        receipt.counts.matched_sources = receipt
            .sources
            .iter()
            .filter(|source| source.state == SourceCheckState::Matched)
            .count();
        receipt.counts.mismatched_sources = receipt
            .sources
            .iter()
            .filter(|source| source.state == SourceCheckState::Mismatch)
            .count();
        receipt.counts.checked_sources =
            receipt.counts.matched_sources + receipt.counts.mismatched_sources;
        receipt.counts.unchecked_sources = receipt
            .counts
            .requested_sources
            .map(|count| count - receipt.counts.checked_sources);
        let complete = receipt.inventory_complete
            && receipt.all_references_accounted
            && receipt.source_rows_omitted == 0;
        if complete && known_sources == 0 {
            receipt.status = "not_applicable".into();
            receipt.requirements_satisfied = None;
        } else {
            let satisfied = complete
                && receipt.publication_fence == "current"
                && known_sources > 0
                && receipt.counts.matched_sources == known_sources;
            receipt.requirements_satisfied = Some(satisfied);
            receipt.status = if satisfied {
                "verified"
            } else if budget.mode == VerificationMode::Require {
                receipt.error = Some(VerificationFailure {
                    code: "verification_required_unmet".into(),
                    retryable: false,
                });
                "requirements_unmet"
            } else if receipt.counts.matched_sources > 0 {
                "partial"
            } else {
                "unverified"
            }
            .into();
        }
        receipt.observed_to_unix_ms = now_ms().max(
            receipt
                .sources
                .iter()
                .map(|source| source.observed_to_unix_ms)
                .max()
                .unwrap_or(observed_from_unix_ms),
        );
        anyhow::ensure!(
            serde_json::to_vec(&receipt)?.len() <= receipt_bytes_budget,
            "verification receipt exceeds reserved metadata budget"
        );
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::evidence::EvidenceReadOptions;
    use serde_json::json;

    fn scoped_fixture(kernel: &Kernel) -> Result<Kernel> {
        use airk_core::{
            grant_provenance::GrantProvenance, identity::Incarnation, index_catalog::*,
            scoped_reads::SourceDescriptor,
        };
        let index = IndexId::from_identity(Incarnation::from_bytes([1; 16]));
        let root = SourceRootId::from_identity(Incarnation::from_bytes([2; 16]));
        let projects =
            [3, 4].map(|byte| QueryProjectId::from_identity(Incarnation::from_bytes([byte; 16])));
        let backing = airk_core::service::ProjectRegistration::new(
            kernel.root_id(),
            kernel.data_root.to_string_lossy().into_owned(),
            kernel.collection.clone(),
        )?
        .binding;
        kernel.with_scoped_reads(crate::scoped_reads::ScopedReadAuthority {
            schema: 1,
            backing,
            index_id: index,
            project_ids: projects.to_vec(),
            provenance: GrantProvenance::UserReviewedProjectGrant,
            sources: vec![SourceDescriptor {
                source_root_id: root,
                original_root: kernel.root_id(),
            }],
            grant: ReviewedGrant {
                id: GrantId::from_identity(Incarnation::from_bytes([5; 16])),
                host: Sha256Digest::from_bytes([6; 32]),
                revision: CatalogRevision::INITIAL,
                revoked: false,
                memberships: projects
                    .into_iter()
                    .zip(["shared/a.rs", "shared/b.rs"])
                    .map(|(project_id, path)| GrantedMembership {
                        membership: Membership {
                            project_id,
                            index_id: index,
                            revision: CatalogRevision::INITIAL,
                            scope: vec![ScopeAtom {
                                source_root_id: root,
                                kind: PathScope::Exact,
                                path: path.into(),
                            }],
                        },
                        source_revision: CatalogRevision::INITIAL,
                        policy_epoch: 0,
                        policy_digest: None,
                    })
                    .collect(),
            },
        })
    }

    #[test]
    fn scoped_member_receipts_fill_precise_source_associations_and_zero_shares_do_no_hashing()
    -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir_all(root.join("shared"))?;
        for (path, content) in [
            ("shared/a.rs", "pub struct Alpha;\n"),
            ("shared/b.rs", "pub struct Beta;\n"),
            ("secret.rs", "pub struct Hidden;\n"),
        ] {
            std::fs::write(root.join(path), content)?;
        }
        let kernel = Kernel::open(&root, Some(temp.path().join("data")))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let scoped = scoped_fixture(&kernel)?;
        let a = serde_json::to_value(
            kernel.file_info_conditional("shared/a.rs", &EvidenceReadOptions::default())?,
        )?;
        let b = serde_json::to_value(
            kernel.file_info_conditional("shared/b.rs", &EvidenceReadOptions::default())?,
        )?;
        let value = json!({"steps":[{"result":a.clone()},{"result":b}]});
        let receipt = scoped.verify_returned_sources(&value, budget(VerificationMode::Report))?;
        assert_eq!(receipt.status, "verified");
        assert_eq!(receipt.origins.len(), 1);
        assert!(receipt.origins[0].index_id.is_some());
        assert_eq!(receipt.origins[0].project_ids.len(), 2);
        assert_eq!(receipt.sources.len(), 2);
        assert!(
            receipt
                .sources
                .iter()
                .all(|source| source.source_root_id.is_some() && source.project_ids.len() == 1)
        );
        assert_ne!(
            receipt.sources[0].project_ids,
            receipt.sources[1].project_ids
        );
        assert_eq!(
            receipt.sources[0].source_root_id,
            receipt.sources[1].source_root_id
        );
        let unscoped = kernel.verify_returned_sources(&value, budget(VerificationMode::Report))?;
        assert_ne!(receipt.scope_digest, unscoped.scope_digest);
        let mut forged = a.clone();
        forged["profile_id"] = json!("f".repeat(64));
        let rejected =
            scoped.verify_returned_sources(&forged, budget(VerificationMode::Require))?;
        assert_eq!(rejected.status, "requirements_unmet");
        assert_eq!(rejected.counts.bytes_read, 0);
        assert_eq!(rejected.sources[0].state, SourceCheckState::Unsupported);
        let registration = airk_core::service::ProjectRegistration::new(
            kernel.root_id(),
            kernel.data_root.to_string_lossy().into_owned(),
            kernel.collection.clone(),
        )?;
        std::fs::remove_file(root.join("shared/a.rs"))?;
        let result = scoped.verify_scoped_projection_allocated(
            registration.binding,
            a.clone(),
            VerificationControls {
                verification: Some(VerificationMode::Require),
                ..Default::default()
            },
            Some(VerificationAllocation {
                max_sources: 0,
                max_bytes: 0,
                deadline_ms: 0,
            }),
        )?;
        assert_eq!(
            result["source_verification"]["status"],
            "requirements_unmet"
        );
        assert_eq!(result["source_verification"]["counts"]["bytes_read"], 0);
        assert_eq!(
            result["source_verification"]["counts"]["checked_sources"],
            0
        );
        assert_eq!(
            result["source_verification"]["sources"][0]["state"],
            "budget_exhausted"
        );
        assert_eq!(result["source_verification"]["limits"]["max_sources"], 0);
        assert_eq!(result["error"]["code"], "verification_required_unmet");
        assert!(
            kernel
                .verify_scoped_projection(registration.binding, a, VerificationControls::default())
                .is_err()
        );
        let unauthorized = serde_json::to_value(
            kernel.file_info_conditional("secret.rs", &EvidenceReadOptions::default())?,
        )?;
        assert!(
            scoped
                .verify_scoped_projection(
                    registration.binding,
                    unauthorized,
                    VerificationControls::default()
                )
                .is_err()
        );
        let oversized = json!({"unused":"x".repeat(airk_core::service::MAX_FRAME_BYTES)});
        assert!(
            scoped
                .verify_scoped_projection(
                    registration.binding,
                    oversized,
                    VerificationControls::default()
                )
                .is_err()
        );
        Ok(())
    }

    fn budget(mode: VerificationMode) -> VerificationBudget {
        VerificationBudget {
            mode,
            max_sources: 25,
            max_bytes: 16 * 1024 * 1024,
            deadline_ms: 2000,
        }
    }

    #[test]
    fn native_receipts_deduplicate_conditional_and_content_references_without_publication_work()
    -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir(&root)?;
        let content = "# Saved source\nQueue receipts preserve their original revision.\n";
        std::fs::write(root.join("policy.md"), content)?;
        let kernel = Kernel::open(&root, Some(temp.path().join("data")))?;
        kernel.index()?;
        let value = serde_json::to_value(
            kernel.file_info_conditional("policy.md", &EvidenceReadOptions::default())?,
        )?;
        let receipt = kernel.verify_returned_sources(&value, budget(VerificationMode::Report))?;
        assert_eq!(receipt.status, "verified");
        assert_eq!(receipt.counts.requested_sources, Some(1));
        assert_eq!(receipt.counts.references, Some(2));
        assert_eq!(receipt.counts.bytes_read, content.len() as u64);
        assert_eq!(receipt.sources[0].reference_ids.len(), 2);
        assert!(receipt.sources[0].profile_digest.is_some());
        assert_eq!(
            serde_json::to_value(
                kernel.file_info_conditional("policy.md", &EvidenceReadOptions::default())?
            )?,
            value
        );
        let original = receipt.origins[0].publication;
        std::fs::write(root.join("policy.md"), "changed saved bytes\n")?;
        let changed = kernel.verify_returned_sources(&value, budget(VerificationMode::Require))?;
        assert_eq!(changed.status, "requirements_unmet");
        assert_eq!(changed.sources[0].state, SourceCheckState::Mismatch);
        assert_eq!(
            changed.error.as_ref().unwrap().code,
            "verification_required_unmet"
        );
        assert!(!changed.error.as_ref().unwrap().retryable);
        assert_eq!(changed.origins[0].publication, original);
        assert_eq!(
            crate::current::runtime::envelope(kernel.current_reader()?.as_ref())?.token,
            original
        );
        std::fs::remove_file(root.join("policy.md"))?;
        let missing = kernel.verify_returned_sources(&value, budget(VerificationMode::Report))?;
        assert_eq!(missing.sources[0].state, SourceCheckState::Missing);
        assert_eq!(missing.counts.checked_sources, 0);
        Ok(())
    }

    #[test]
    fn native_budget_policy_unknown_and_metadata_limits_cannot_certify_a_subset() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("a.md"), "# Source A\nSaved bytes A.\n")?;
        std::fs::write(root.join("b.md"), "# Source B\nSaved bytes B.\n")?;
        let kernel = Kernel::open(&root, Some(temp.path().join("data")))?;
        kernel.index()?;
        let publication = serde_json::to_value(crate::current::runtime::envelope(
            kernel.current_reader()?.as_ref(),
        )?)?;
        let value = json!({"publication":publication,"paths":["a.md","b.md"]});
        let mut limited = budget(VerificationMode::Report);
        limited.max_sources = 1;
        let receipt = kernel.verify_returned_sources(&value, limited)?;
        assert_eq!(receipt.status, "partial");
        assert_eq!(receipt.counts.matched_sources, 1);
        assert_eq!(receipt.counts.unchecked_sources, Some(1));
        assert!(receipt.limits.hit.iter().any(|hit| hit == "source_count"));
        limited.max_sources = 2;
        limited.max_bytes = 1;
        let bytes = kernel.verify_returned_sources(&value, limited)?;
        assert_eq!(bytes.status, "unverified");
        assert_eq!(bytes.counts.bytes_read, 0);
        assert_eq!(bytes.counts.checked_sources, 0);
        std::fs::write(root.join(".airkignore"), "a.md\n")?;
        let excluded = kernel.verify_returned_sources(
            &json!({"publication":publication,"paths":["a.md"]}),
            budget(VerificationMode::Report),
        )?;
        assert_eq!(excluded.sources[0].state, SourceCheckState::OutOfScope);
        assert_eq!(excluded.counts.bytes_read, 0);
        let unknown=kernel.verify_returned_sources(&json!({"publication":publication,"hits":[{"citation":{"start_line":1}},{"citation":{"start_line":2}}]}),budget(VerificationMode::Require))?;
        assert_eq!(unknown.status, "requirements_unmet");
        assert_eq!(unknown.counts.requested_sources, None);
        assert_eq!(unknown.counts.requested_sources_lower_bound, 0);
        assert_eq!(unknown.sources.len(), 2);
        let large = kernel.verify_returned_sources(
            &json!({"publication":publication,"paths":vec!["b.md";2000]}),
            budget(VerificationMode::Require),
        )?;
        assert!(!large.all_references_accounted);
        assert_eq!(large.source_rows_omitted, 1);
        assert_eq!(large.counts.bytes_read, 0);
        assert_eq!(large.status, "requirements_unmet");
        assert!(serde_json::to_vec(&large)?.len() <= MAX_RECEIPT_BYTES);
        let empty = kernel.verify_returned_sources(
            &json!({"publication":publication,"hits":[]}),
            budget(VerificationMode::Require),
        )?;
        assert_eq!(empty.status, "not_applicable");
        assert_eq!(empty.requirements_satisfied, None);
        Ok(())
    }

    #[test]
    fn aliases_and_old_publications_do_not_become_current_capture_authority() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        let docs = temp.path().join("docs");
        std::fs::create_dir(&root)?;
        std::fs::create_dir(&docs)?;
        std::fs::write(
            docs.join("requirement.md"),
            "# Requirement\nPreserve alias ownership.\n",
        )?;
        let kernel = Kernel::open(&root, Some(temp.path().join("data")))?;
        kernel.add_root("specs", &docs)?;
        kernel.index()?;
        let value = serde_json::to_value(
            kernel.file_info_conditional("@specs/requirement.md", &Default::default())?,
        )?;
        let matched = kernel.verify_returned_sources(&value, budget(VerificationMode::Report))?;
        assert_eq!(matched.status, "verified");
        assert_eq!(matched.counts.requested_sources, Some(1));
        assert_eq!(
            matched.sources[0].source_relative_path.as_deref(),
            Some("requirement.md")
        );
        kernel.remove_root("specs")?;
        let removed = kernel.verify_returned_sources(&value, budget(VerificationMode::Report))?;
        assert_eq!(removed.sources[0].state, SourceCheckState::OutOfScope);
        assert_eq!(removed.counts.bytes_read, 0);
        kernel.index()?;
        let old = kernel.verify_returned_sources(&value, budget(VerificationMode::Require))?;
        assert_eq!(old.status, "requirements_unmet");
        assert_eq!(old.sources[0].state, SourceCheckState::PublicationExpired);
        assert!(
            old.sources[0].original_root.is_some(),
            "retain the old file-info root, never substitute current alias configuration"
        );
        assert_eq!(old.counts.bytes_read, 0);
        Ok(())
    }

    #[test]
    fn no_reference_and_reply_budget_checks_need_no_index_or_source_reads() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("source");
        std::fs::create_dir(&root)?;
        let kernel = Kernel::open(&root, Some(temp.path().join("data")))?;
        let none = kernel.verify_returned_sources(
            &serde_json::json!({"status":"unavailable"}),
            budget(VerificationMode::Require),
        )?;
        assert_eq!(none.status, "not_applicable");
        assert_eq!(none.counts.bytes_read, 0);
        let oversized = serde_json::json!({"hits":[{"text":"\\\"".repeat(40_000),"citation":{"path":"not-indexed.md","content_hash":"a".repeat(64)}}]});
        assert!(
            kernel
                .verify_returned_sources(&oversized, budget(VerificationMode::Report))
                .unwrap_err()
                .downcast_ref::<VerificationReplyBudget>()
                .is_some()
        );
        Ok(())
    }
}
