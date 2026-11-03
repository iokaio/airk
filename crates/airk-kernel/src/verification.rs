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
        anyhow::ensure!(
            budget.mode != VerificationMode::Off,
            "off verification must bypass source checking"
        );
        VerificationControls {
            verification: Some(budget.mode),
            verification_max_sources: Some(budget.max_sources),
            verification_max_bytes: Some(budget.max_bytes),
            verification_deadline_ms: Some(budget.deadline_ms),
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
                    index_id: None,
                    legacy_project: registration.binding.to_string(),
                    collection: self.collection.clone(),
                    incarnation: publication.incarnation,
                    publication,
                    project_ids: vec![],
                });
                id
            };
            let path = reference.path.as_deref();
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
                } else {
                    (SourceCheckState::BudgetExhausted, "source_count_budget")
                };
                receipt.sources.push(VerifiedSource {
                    id: source_id,
                    origin_id: origin_id.clone(),
                    source_root_id: None,
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
                    profile_digest: reference
                        .profile_digest
                        .or(identity.and_then(|identity| identity.1)),
                    project_ids: vec![],
                    reference_ids: vec![],
                    state: check_state,
                    reason: Some(reason.into()),
                    observed_from_unix_ms: from,
                    observed_to_unix_ms: from,
                    bytes_read: 0,
                });
                position
            };
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
            BTreeMap::<String, airk_ingest::source_scope::SelectedSourcePolicy>::new();
        let mut attempted = 0;
        for source in &mut receipt.sources {
            if source.state != SourceCheckState::BudgetExhausted {
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
                let policy = policies.entry(location.root.clone()).or_insert_with(|| {
                    airk_ingest::source_scope::SelectedSourcePolicy::new(
                        Path::new(&location.root),
                        &self.data_root,
                    )
                });
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
                BTreeMap::<String, airk_ingest::source_scope::SelectedSourcePolicy>::new();
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
                        final_policies
                            .entry(location.root.clone())
                            .or_insert_with(|| {
                                airk_ingest::source_scope::SelectedSourcePolicy::new(
                                    Path::new(&location.root),
                                    &self.data_root,
                                )
                            })
                            .allows(&location.relative_path)
                            .unwrap_or(false)
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
