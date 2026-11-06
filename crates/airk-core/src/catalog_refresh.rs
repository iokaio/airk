// SPDX-License-Identifier: Apache-2.0
//! Inert refresh intent. Native review, current grants and durable admission are
//! independently required; copied evidence never grants mutation authority.
use crate::{
    identity::{Incarnation, RevisionToken, Sha256Digest, framed_digest},
    index_catalog::{IndexId, SourceRootId, encoded_size, relative_path, unique_bounded},
    service::{CombinedConnectRequest, RequestKey},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_REFRESH_MEMBERS: usize = 8;
pub const MAX_REFRESH_SOURCES: usize = 64;
// Leave configuration envelope space below MAX_CONFIGURATION_BYTES=16384.
// Admission still measures the complete containing ConfigurationWork/Request.
pub const MAX_REFRESH_SELECTION_BYTES: usize = 12 << 10;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshSource {
    pub source_root_id: SourceRootId,
    pub source_relative_path: String,
    /// Reviewed original locator; matched to canonical native descriptor before admission.
    pub canonical_root: String,
    pub indexed_hash: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshMember {
    pub index_id: IndexId,
    pub original_publication: RevisionToken,
    pub sources: Vec<RefreshSource>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshMode {
    SelectedOnly,
    /// Explicit broader intent only. The native preview must independently
    /// disclose its exact scope/cost and prove the current grant covers it.
    FullReconcileReviewed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshFallbackReason {
    NoCurrentIndex,
    PendingRecovery,
    ConfigurationChanged,
    ExtractorChanged,
    IncompleteScan,
    DirectorySelection,
    ControlInput,
    ControlInputsChanged,
    ControlContextChanged,
    DependencyInput,
    UnqualifiedControlProvenance,
}

/// The actual worker must produce this typed refusal before starting broader
/// discovery/capture/publication if a selected-only review has become unsafe.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshReviewRequired {
    pub reasons: Vec<RefreshFallbackReason>,
}
impl RefreshReviewRequired {
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.reasons.is_empty(), "empty refresh review reason");
        unique_bounded(&self.reasons, 11)
    }
}
impl std::fmt::Display for RefreshReviewRequired {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("refresh_scope_changed_review_required")
    }
}
impl std::error::Error for RefreshReviewRequired {}

pub fn require_reviewed_refresh_mode(
    mode: RefreshMode,
    reasons: &[RefreshFallbackReason],
) -> Result<()> {
    unique_bounded(reasons, 11)?;
    if mode == RefreshMode::SelectedOnly && !reasons.is_empty() {
        return Err(RefreshReviewRequired {
            reasons: reasons.to_vec(),
        }
        .into());
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshSelection {
    pub schema: u32,
    pub caller: Incarnation,
    pub key: RequestKey,
    pub original_result_digest: Sha256Digest,
    pub original_evidence_selection_digest: Sha256Digest,
    /// Binds the complete saved canonical query, including verification and
    /// semantic policy. The client retains that query; no source text in IPC.
    pub original_query_digest: Sha256Digest,
    pub selection: CombinedConnectRequest,
    pub mode: RefreshMode,
    /// Explicit refreshed subset; the original result may have other members.
    pub members: Vec<RefreshMember>,
}
impl RefreshSelection {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported refresh selection schema");
        self.selection.validate()?;
        // Receipt keys and catalog selections have independent namespaces.
        // Native receipt admission and scoped catalog resolution authenticate
        // each domain; the complete selection identity retains both.
        ensure!(!self.members.is_empty(), "empty selected refresh");
        let selected = self.selection.index_ids.as_ref().ok_or_else(|| {
            anyhow::anyhow!("refresh requires explicit original member index identities")
        })?;
        unique_bounded(
            &self
                .members
                .iter()
                .map(|member| member.index_id)
                .collect::<Vec<_>>(),
            MAX_REFRESH_MEMBERS,
        )?;
        let mut total = 0usize;
        for member in &self.members {
            ensure!(
                selected.contains(&member.index_id),
                "foreign refresh member"
            );
            ensure!(
                member.original_publication.generation > 0,
                "refresh requires an indexed publication"
            );
            ensure!(
                !member.sources.is_empty(),
                "refresh member has no selected sources"
            );
            total = total
                .checked_add(member.sources.len())
                .ok_or_else(|| anyhow::anyhow!("refresh source count overflow"))?;
            ensure!(
                total <= MAX_REFRESH_SOURCES,
                "refresh source count exceeds review budget"
            );
            let mut paths = BTreeSet::new();
            for source in &member.sources {
                relative_path(&source.source_relative_path, false)?;
                ensure!(
                    source.canonical_root.len() <= 32768
                        && std::path::Path::new(&source.canonical_root).is_absolute()
                        && !source.canonical_root.chars().any(char::is_control),
                    "invalid reviewed source root locator"
                );
                ensure!(
                    !source.source_relative_path.chars().any(char::is_control),
                    "control character in refresh source path"
                );
                ensure!(
                    paths.insert((source.source_root_id, &source.source_relative_path)),
                    "duplicate refresh source binding"
                );
            }
        }
        encoded_size(self, MAX_REFRESH_SELECTION_BYTES)?;
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(framed_digest(
            b"airk/catalog-refresh-selection/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index_catalog::{CatalogRevision, GrantId, QueryProjectId};
    fn selection() -> RefreshSelection {
        let incarnation = Incarnation::from_bytes([1; 16]);
        let digest = Sha256Digest::from_bytes([2; 32]);
        let index = IndexId::from_identity(incarnation);
        RefreshSelection {
            schema: 1,
            caller: incarnation,
            key: RequestKey {
                namespace: incarnation,
                epoch: 1,
                nonce: incarnation,
            },
            original_result_digest: digest,
            original_evidence_selection_digest: digest,
            original_query_digest: digest,
            selection: CombinedConnectRequest {
                schema: 1,
                context_key: digest,
                namespace: incarnation,
                expected_catalog_revision: CatalogRevision::INITIAL,
                grant_id: GrantId::from_identity(incarnation),
                expected_grant_revision: CatalogRevision::INITIAL,
                project_ids: vec![QueryProjectId::from_identity(incarnation)],
                index_ids: Some(vec![index]),
            },
            mode: RefreshMode::SelectedOnly,
            members: vec![RefreshMember {
                index_id: index,
                original_publication: RevisionToken {
                    incarnation,
                    generation: 1,
                    revision: digest,
                },
                sources: vec![RefreshSource {
                    source_root_id: SourceRootId::from_identity(incarnation),
                    source_relative_path: "api/policy.md".into(),
                    canonical_root: std::env::temp_dir().to_string_lossy().into_owned(),
                    indexed_hash: digest,
                }],
            }],
        }
    }
    #[test]
    fn refresh_selection_identity_matches_workbench_cross_language_golden() -> Result<()> {
        let mut selected = selection();
        selected.members[0].sources[0].canonical_root = "//owned/source/é".into();
        selected.members[0].sources[0].source_relative_path = "api/é-政策😀.md".into();
        let canonical = r#"{"schema":1,"caller":"01010101010101010101010101010101","key":{"namespace":"01010101010101010101010101010101","epoch":1,"nonce":"01010101010101010101010101010101"},"original_result_digest":"0202020202020202020202020202020202020202020202020202020202020202","original_evidence_selection_digest":"0202020202020202020202020202020202020202020202020202020202020202","original_query_digest":"0202020202020202020202020202020202020202020202020202020202020202","selection":{"schema":1,"context_key":"0202020202020202020202020202020202020202020202020202020202020202","namespace":"01010101010101010101010101010101","expected_catalog_revision":1,"grant_id":"grt_01010101010101010101010101010101","expected_grant_revision":1,"project_ids":["prj_01010101010101010101010101010101"],"index_ids":["idx_01010101010101010101010101010101"]},"mode":"selected_only","members":[{"index_id":"idx_01010101010101010101010101010101","original_publication":"airk-r1:01010101010101010101010101010101:1:0202020202020202020202020202020202020202020202020202020202020202","sources":[{"source_root_id":"src_01010101010101010101010101010101","source_relative_path":"api/é-政策😀.md","canonical_root":"//owned/source/é","indexed_hash":"0202020202020202020202020202020202020202020202020202020202020202"}]}]}"#;
        assert_eq!(serde_json::to_vec(&selected)?, canonical.as_bytes());
        assert_eq!(
            selected.identity()?.to_string(),
            "4630540e46f495609e14aec592677c465636aa0f32edd24c2b0b8f9f7a220298"
        );
        let mut distinct_namespace = selected.clone();
        distinct_namespace.selection.namespace = Incarnation::from_bytes([3; 16]);
        assert_eq!(
            distinct_namespace.identity()?.to_string(),
            "a3610bdf29b2117a4ac43d8fc6838b7dad405c5b4dc36a56e206f628fb6591d4"
        );
        distinct_namespace.key.epoch = 0;
        assert_eq!(
            distinct_namespace.identity()?.to_string(),
            "be7b6287f8cdacaee862bc8be0746b05502630b49cf2bd523b5cbf2e5ccede35"
        );
        // Input maps may arrive in any order; typed deserialization restores
        // struct order, while unknown request fields never disappear silently.
        let mut wire: serde_json::Value = serde_json::from_str(canonical)?;
        let roundtrip: RefreshSelection = serde_json::from_value(wire.clone())?;
        assert_eq!(roundtrip.identity()?, selected.identity()?);
        wire["future_request_field"] = serde_json::json!(true);
        assert!(serde_json::from_value::<RefreshSelection>(wire).is_err());
        let mut nested: serde_json::Value = serde_json::from_str(canonical)?;
        nested["members"][0]["sources"][0]["future_request_field"] = serde_json::json!(true);
        assert!(serde_json::from_value::<RefreshSelection>(nested).is_err());
        let invalid_surrogate = canonical.replace("api/é-政策😀.md", r"api/\ud800.md");
        assert!(serde_json::from_str::<RefreshSelection>(&invalid_surrogate).is_err());
        selected.members[0].sources[0].source_relative_path = format!("api/{}.md", '\u{85}');
        assert!(selected.identity().is_err());
        Ok(())
    }
    #[test]
    fn refresh_digest_binds_original_query_evidence_grant_member_and_mode() -> Result<()> {
        let original = selection();
        let digest = original.identity()?;
        let mut changed = original.clone();
        changed.original_query_digest = Sha256Digest::from_bytes([3; 32]);
        assert_ne!(digest, changed.identity()?);
        changed = original.clone();
        changed.selection.context_key = Sha256Digest::from_bytes([3; 32]);
        assert_ne!(digest, changed.identity()?);
        changed = original.clone();
        changed.key.namespace = Incarnation::from_bytes([3; 16]);
        assert_ne!(digest, changed.identity()?);
        changed = original.clone();
        changed.key.epoch = 0;
        assert_ne!(digest, changed.identity()?);
        changed = original.clone();
        changed.selection.namespace = Incarnation::from_bytes([3; 16]);
        assert_ne!(digest, changed.identity()?);
        changed = original.clone();
        changed.members[0].sources[0].indexed_hash = Sha256Digest::from_bytes([3; 32]);
        assert_ne!(digest, changed.identity()?);
        changed = original;
        changed.mode = RefreshMode::FullReconcileReviewed;
        assert_ne!(digest, changed.identity()?);
        Ok(())
    }
    #[test]
    fn refresh_rejects_foreign_duplicate_traversal_and_unbounded_selections() {
        let mut changed = selection();
        changed.members[0].index_id = IndexId::from_identity(Incarnation::from_bytes([9; 16]));
        assert!(changed.validate().is_err());
        changed = selection();
        let duplicate = changed.members[0].sources[0].clone();
        changed.members[0].sources.push(duplicate);
        assert!(changed.validate().is_err());
        changed = selection();
        changed.members[0].sources[0].source_relative_path = "../outside".into();
        assert!(changed.validate().is_err());
        changed = selection();
        changed.selection.index_ids = None;
        assert!(changed.validate().is_err());
        changed = selection();
        changed.members[0].sources = (0..65)
            .map(|n| RefreshSource {
                source_root_id: SourceRootId::from_identity(Incarnation::from_bytes([1; 16])),
                source_relative_path: format!("api/{n}.md"),
                canonical_root: std::env::temp_dir().to_string_lossy().into_owned(),
                indexed_hash: Sha256Digest::from_bytes([2; 32]),
            })
            .collect();
        assert!(changed.validate().is_err());
        changed = selection();
        changed.members[0].sources = (0..4)
            .map(|n| RefreshSource {
                source_root_id: SourceRootId::from_identity(Incarnation::from_bytes([1; 16])),
                source_relative_path: format!("{n}{}", "a".repeat(4095)),
                canonical_root: std::env::temp_dir().to_string_lossy().into_owned(),
                indexed_hash: Sha256Digest::from_bytes([2; 32]),
            })
            .collect();
        // Individually valid paths/count; complete serialized request exceeds
        // its byte cap. Admission must refuse rather than trim the selection.
        assert!(changed.validate().is_err());
    }
    #[test]
    fn selected_only_refuses_fallback_without_silently_escalating() -> Result<()> {
        let reasons = [
            RefreshFallbackReason::ConfigurationChanged,
            RefreshFallbackReason::PendingRecovery,
        ];
        let error = require_reviewed_refresh_mode(RefreshMode::SelectedOnly, &reasons).unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<RefreshReviewRequired>()
                .unwrap()
                .reasons,
            reasons
        );
        require_reviewed_refresh_mode(RefreshMode::FullReconcileReviewed, &reasons)?;
        assert!(require_reviewed_refresh_mode(RefreshMode::SelectedOnly, &[]).is_ok());
        Ok(())
    }
}
