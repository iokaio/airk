// SPDX-License-Identifier: Apache-2.0
//! Metadata-only family observations. These describe policy, never query counts
//! or live-source equality. Filesystem and renderer observations belong to callers.
use crate::{
    extraction::{ExtractionProfile, IndexDetail},
    identity::{RevisionToken, Sha256Digest},
    retention::{AppliedRetention, RetainedFamily, RetainedProfile},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    SavedIdentity,
    SavedText,
    LexicalCandidates,
    Vectors,
    Declarations,
    Imports,
    WrittenCalls,
    FrameworkStructure,
    ManifestPackagesDependencies,
    RequirementsDesignDecisions,
    HeadingsSections,
    StandaloneDatesBounds,
    TablesRows,
    MappedRegions,
    DeclarationContinuity,
    DocumentUnitContinuity,
    ImportedAssertions,
    OcrTextFacts,
}
impl Family {
    pub const ALL: [Self; 18] = [
        Self::SavedIdentity,
        Self::SavedText,
        Self::LexicalCandidates,
        Self::Vectors,
        Self::Declarations,
        Self::Imports,
        Self::WrittenCalls,
        Self::FrameworkStructure,
        Self::ManifestPackagesDependencies,
        Self::RequirementsDesignDecisions,
        Self::HeadingsSections,
        Self::StandaloneDatesBounds,
        Self::TablesRows,
        Self::MappedRegions,
        Self::DeclarationContinuity,
        Self::DocumentUnitContinuity,
        Self::ImportedAssertions,
        Self::OcrTextFacts,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retention {
    Retained,
    Selected,
    NotRetained,
    Unknown,
}

/// Observe the actual renderer contract independently of the requested mode.
/// A selected subset of fields is not an omitted family. Missing observations
/// remain unknown, including the transport's historical absent-mode behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplyInclusion {
    Included,
    Selected,
    Omitted,
    Unknown,
}

/// These are COLLECTION POLICY identities, never effective per-file profiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CollectionPolicyRevision {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<IndexDetail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_profile: Option<RetainedProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_fallback_detail: Option<IndexDetail>,
    pub policy_digest: Sha256Digest,
}
impl CollectionPolicyRevision {
    pub fn from_selection(
        target: Option<RetainedProfile>,
        fallback: IndexDetail,
        policy_digest: Sha256Digest,
    ) -> Self {
        Self {
            detail: target.is_none().then_some(fallback),
            retained_profile: target,
            legacy_fallback_detail: target.map(|_| fallback),
            policy_digest,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionPolicyUnavailable {
    SelectedScope,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum CollectionPolicyObservation {
    Known { revision: CollectionPolicyRevision },
    Unavailable { reason: CollectionPolicyUnavailable },
}
impl From<CollectionPolicyRevision> for CollectionPolicyObservation {
    fn from(revision: CollectionPolicyRevision) -> Self {
        Self::Known { revision }
    }
}
impl CollectionPolicyObservation {
    pub fn known(self) -> Option<CollectionPolicyRevision> {
        match self {
            Self::Known { revision } => Some(revision),
            Self::Unavailable { .. } => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppliedFileProfile {
    pub adapter_id: String,
    pub adapter_version: String,
    pub effective_profile_digest: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_retention: Option<AppliedRetention>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_detail: Option<IndexDetail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_detail_was_absent: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRevision {
    pub publication: RevisionToken,
    pub path: String,
    pub source_hash: Sha256Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyObservation {
    pub family: Family,
    pub support: Support,
    pub retention: Retention,
    pub reply_inclusion: ReplyInclusion,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfiguredFileProfileObservation {
    UnknownSourceInputsMayBeRequired,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileFamilyReport {
    pub schema: String,
    pub basis: String,
    pub file: FileRevision,
    pub applied_file: AppliedFileProfile,
    pub configured_collection_policy: CollectionPolicyObservation,
    pub published_collection_policy: CollectionPolicyObservation,
    pub pending_collection_reindex: Option<bool>,
    pub configured_file_profile: ConfiguredFileProfileObservation,
    pub retention_basis: String,
    pub live_source_checked: bool,
    pub families: Vec<FamilyObservation>,
}
fn legacy_retention(family: Family, detail: IndexDetail) -> Retention {
    use Family::*;
    use IndexDetail::*;
    use Retention::*;
    match family {
        SavedIdentity | SavedText | LexicalCandidates => Retained,
        Vectors | MappedRegions | ImportedAssertions | OcrTextFacts => Unknown,
        DeclarationContinuity => match detail {
            Essential | AgentLean => NotRetained,
            Balanced | Detailed => Selected,
        },
        DocumentUnitContinuity => {
            if detail == Detailed {
                Selected
            } else {
                NotRetained
            }
        }
        _ if detail == Essential => NotRetained,
        ManifestPackagesDependencies | StandaloneDatesBounds if detail == AgentLean => NotRetained,
        FrameworkStructure if detail == AgentLean => Selected,
        _ => Retained,
    }
}

fn retention(family: Family, applied: &AppliedRetention) -> Retention {
    match applied {
        AppliedRetention::Legacy { detail, .. } => legacy_retention(family, *detail),
        AppliedRetention::Selected {
            profile: RetainedProfile::Rich,
            ..
        } => legacy_retention(family, IndexDetail::Detailed),
        AppliedRetention::Selected {
            family: RetainedFamily::Manifest,
            ..
        } => match family {
            Family::SavedIdentity
            | Family::SavedText
            | Family::LexicalCandidates
            | Family::ManifestPackagesDependencies
            | Family::HeadingsSections => Retention::Retained,
            Family::Vectors
            | Family::MappedRegions
            | Family::ImportedAssertions
            | Family::OcrTextFacts => Retention::Unknown,
            _ => Retention::NotRetained,
        },
        AppliedRetention::Selected { .. } => legacy_retention(family, IndexDetail::AgentLean),
    }
}

/// Pure bounded metadata projection. Retention means policy eligibility, not
/// observed presence/count/completeness. No callback or source reader is accepted.
pub fn report(
    file: FileRevision,
    profile: &ExtractionProfile,
    configured: CollectionPolicyObservation,
    published: CollectionPolicyObservation,
    support: &BTreeMap<Family, Support>,
    reply: &BTreeMap<Family, ReplyInclusion>,
) -> Result<ProfileFamilyReport> {
    ensure!(
        !file.path.is_empty() && file.path.len() <= 8192 && !file.path.contains('\0'),
        "invalid family report file identity"
    );
    ensure!(
        [&profile.adapter_id, &profile.adapter_version]
            .into_iter()
            .all(|value| !value.is_empty()
                && value.len() <= 128
                && value.bytes().all(|byte| byte.is_ascii_graphic())),
        "invalid bounded adapter identity"
    );
    let applied = AppliedRetention::from_profile(profile)?;
    let (legacy_detail, legacy_detail_was_absent) = match &applied {
        AppliedRetention::Legacy {
            detail,
            index_detail_was_absent,
        } => (Some(*detail), Some(*index_detail_was_absent)),
        AppliedRetention::Selected { .. } => (None, None),
    };
    let value = ProfileFamilyReport {
        schema: "airk.profile-families/1".into(),
        basis: "metadata_only".into(),
        file,
        applied_file: AppliedFileProfile {
            adapter_id: profile.adapter_id.clone(),
            adapter_version: profile.adapter_version.clone(),
            effective_profile_digest: profile.identity()?,
            applied_retention: matches!(&applied, AppliedRetention::Selected { .. })
                .then(|| applied.clone()),
            legacy_detail,
            legacy_detail_was_absent,
        },
        configured_collection_policy: configured,
        published_collection_policy: published,
        pending_collection_reindex: configured
            .known()
            .zip(published.known())
            .map(|(configured, published)| configured.policy_digest != published.policy_digest),
        configured_file_profile: ConfiguredFileProfileObservation::UnknownSourceInputsMayBeRequired,
        retention_basis: "policy_eligibility_not_record_presence_or_completeness".into(),
        live_source_checked: false,
        families: Family::ALL
            .into_iter()
            .map(|family| FamilyObservation {
                family,
                support: support.get(&family).copied().unwrap_or(Support::Unknown),
                retention: retention(family, &applied),
                reply_inclusion: reply
                    .get(&family)
                    .copied()
                    .unwrap_or(ReplyInclusion::Unknown),
            })
            .collect(),
    };
    ensure!(
        serde_json::to_vec(&value)?.len() <= 32768,
        "complete family report exceeds bound"
    );
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Incarnation;
    fn profile(detail: Option<IndexDetail>) -> ExtractionProfile {
        ExtractionProfile {
            contract_version: crate::extraction::EXTRACTION_CONTRACT_VERSION,
            adapter_id: "fixture".into(),
            adapter_version: "1".into(),
            language: "text".into(),
            dialect: None,
            grammar_identity: None,
            mapping_version: 1,
            chunk_version: 1,
            options: detail
                .map(|detail| BTreeMap::from([("index_detail".into(), detail.as_str().into())]))
                .unwrap_or_default(),
            configuration_inputs: BTreeMap::new(),
        }
    }
    fn file() -> FileRevision {
        FileRevision {
            publication: RevisionToken {
                incarnation: Incarnation::from_bytes([1; 16]),
                generation: 1,
                revision: Sha256Digest::from_bytes([2; 32]),
            },
            path: "manifest.toml".into(),
            source_hash: Sha256Digest::from_bytes([3; 32]),
        }
    }
    fn row(value: &ProfileFamilyReport, family: Family) -> &FamilyObservation {
        value
            .families
            .iter()
            .find(|row| row.family == family)
            .unwrap()
    }
    #[test]
    fn all_legacy_details_keep_support_retention_and_actual_reply_independent() -> Result<()> {
        for detail in [
            IndexDetail::Essential,
            IndexDetail::AgentLean,
            IndexDetail::Balanced,
            IndexDetail::Detailed,
        ] {
            for reply in [
                None,
                Some(ReplyInclusion::Selected),
                Some(ReplyInclusion::Omitted),
            ] {
                let configured = CollectionPolicyRevision {
                    detail: Some(IndexDetail::Detailed),
                    retained_profile: None,
                    legacy_fallback_detail: None,
                    policy_digest: Sha256Digest::from_bytes([4; 32]),
                };
                let published = CollectionPolicyRevision {
                    detail: Some(detail),
                    retained_profile: None,
                    legacy_fallback_detail: None,
                    policy_digest: Sha256Digest::from_bytes([5; 32]),
                };
                let mut replies = reply
                    .map(|reply| BTreeMap::from([(Family::TablesRows, reply)]))
                    .unwrap_or_default();
                replies.insert(
                    Family::ManifestPackagesDependencies,
                    ReplyInclusion::Omitted,
                );
                let value = report(
                    file(),
                    &profile(Some(detail)),
                    configured.into(),
                    published.into(),
                    &BTreeMap::from([
                        (Family::TablesRows, Support::Supported),
                        (Family::ManifestPackagesDependencies, Support::Supported),
                    ]),
                    &replies,
                )?;
                assert_eq!(value.families.len(), 18);
                assert_eq!(
                    value.applied_file.effective_profile_digest,
                    profile(Some(detail)).identity()?
                );
                assert_eq!(
                    value
                        .configured_collection_policy
                        .known()
                        .unwrap()
                        .policy_digest,
                    configured.policy_digest
                );
                assert_eq!(value.pending_collection_reindex, Some(true));
                assert_eq!(
                    row(&value, Family::TablesRows).retention,
                    if detail.retains_fact("table") {
                        Retention::Retained
                    } else {
                        Retention::NotRetained
                    }
                );
                assert_eq!(row(&value, Family::TablesRows).support, Support::Supported);
                assert_eq!(
                    row(&value, Family::TablesRows).reply_inclusion,
                    reply.unwrap_or(ReplyInclusion::Unknown)
                );
                assert_eq!(
                    row(&value, Family::ManifestPackagesDependencies).retention,
                    if detail.retains_fact("package_dependency") {
                        Retention::Retained
                    } else {
                        Retention::NotRetained
                    }
                );
                assert_eq!(
                    row(&value, Family::ManifestPackagesDependencies).support,
                    Support::Supported
                );
                assert_eq!(
                    row(&value, Family::ManifestPackagesDependencies).reply_inclusion,
                    ReplyInclusion::Omitted
                );
                assert_eq!(
                    row(&value, Family::StandaloneDatesBounds).retention,
                    if detail.retains_fact("date") {
                        Retention::Retained
                    } else {
                        Retention::NotRetained
                    }
                );
                assert_eq!(
                    row(&value, Family::DeclarationContinuity).retention,
                    if detail.tracks_continuity() {
                        Retention::Selected
                    } else {
                        Retention::NotRetained
                    }
                );
                assert_eq!(
                    row(&value, Family::DocumentUnitContinuity).retention,
                    if detail == IndexDetail::Detailed {
                        Retention::Selected
                    } else {
                        Retention::NotRetained
                    }
                );
                for family in [
                    Family::Vectors,
                    Family::MappedRegions,
                    Family::ImportedAssertions,
                    Family::OcrTextFacts,
                ] {
                    assert_eq!(row(&value, family).support, Support::Unknown);
                    assert_eq!(row(&value, family).retention, Retention::Unknown);
                }
            }
        }
        Ok(())
    }
    #[test]
    fn missing_historical_detail_is_detailed_without_rewriting_profile_identity() -> Result<()> {
        let profile = profile(None);
        let policy = CollectionPolicyRevision {
            detail: Some(IndexDetail::AgentLean),
            retained_profile: None,
            legacy_fallback_detail: None,
            policy_digest: Sha256Digest::from_bytes([9; 32]),
        };
        let value = report(
            file(),
            &profile,
            policy.into(),
            policy.into(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )?;
        assert_eq!(
            value.applied_file.legacy_detail,
            Some(IndexDetail::Detailed)
        );
        assert_eq!(value.applied_file.legacy_detail_was_absent, Some(true));
        assert_eq!(
            value.applied_file.effective_profile_digest,
            profile.identity()?
        );
        assert_eq!(value.pending_collection_reindex, Some(false));
        Ok(())
    }
    #[test]
    fn unavailable_collection_policy_preserves_file_rows_without_inventing_pending_state()
    -> Result<()> {
        let profile = profile(Some(IndexDetail::AgentLean));
        let unavailable = CollectionPolicyObservation::Unavailable {
            reason: CollectionPolicyUnavailable::SelectedScope,
        };
        let known = CollectionPolicyRevision {
            detail: Some(IndexDetail::Detailed),
            retained_profile: None,
            legacy_fallback_detail: None,
            policy_digest: Sha256Digest::from_bytes([8; 32]),
        };
        let value = report(
            file(),
            &profile,
            unavailable,
            known.into(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )?;
        assert_eq!(value.pending_collection_reindex, None);
        assert_eq!(value.configured_collection_policy, unavailable);
        assert_eq!(
            value.applied_file.effective_profile_digest,
            profile.identity()?
        );
        assert_eq!(value.families.len(), 18);
        Ok(())
    }
    #[test]
    fn selected_retention_reports_effective_file_separately_from_collection_and_reply() -> Result<()>
    {
        for target in [RetainedProfile::Lean, RetainedProfile::Rich] {
            let selected = AppliedRetention::bind(
                &profile(Some(IndexDetail::Detailed)),
                target,
                RetainedFamily::Manifest,
            )?;
            let configured = CollectionPolicyRevision::from_selection(
                Some(RetainedProfile::Rich),
                IndexDetail::AgentLean,
                Sha256Digest::from_bytes([11; 32]),
            );
            let published = CollectionPolicyRevision::from_selection(
                Some(RetainedProfile::Lean),
                IndexDetail::AgentLean,
                Sha256Digest::from_bytes([12; 32]),
            );
            let reply = BTreeMap::from([(
                Family::ManifestPackagesDependencies,
                ReplyInclusion::Omitted,
            )]);
            let value = report(
                file(),
                &selected,
                configured.into(),
                published.into(),
                &BTreeMap::new(),
                &reply,
            )?;
            assert_eq!(
                value.applied_file.effective_profile_digest,
                selected.identity()?
            );
            assert!(
                matches!(&value.applied_file.applied_retention,Some(AppliedRetention::Selected{profile,..}) if *profile==target)
            );
            assert!(value.applied_file.legacy_detail.is_none());
            assert!(value.applied_file.legacy_detail_was_absent.is_none());
            assert_eq!(
                row(&value, Family::ManifestPackagesDependencies).retention,
                Retention::Retained
            );
            assert_eq!(
                row(&value, Family::ManifestPackagesDependencies).reply_inclusion,
                ReplyInclusion::Omitted
            );
            assert_eq!(value.pending_collection_reindex, Some(true));
            assert_eq!(value.families.len(), 18);
            assert!(!value.live_source_checked);
            let json = serde_json::to_value(&value)?;
            assert!(json["applied_file"].get("legacy_detail").is_none());
            assert!(
                json["configured_collection_policy"]["revision"]
                    .get("detail")
                    .is_none()
            );
            assert_eq!(
                json["configured_collection_policy"]["revision"]["legacy_fallback_detail"],
                "agent_lean"
            );
        }
        Ok(())
    }
    #[test]
    fn legacy_report_fields_keep_their_exact_serialized_shape() -> Result<()> {
        let legacy = profile(None);
        let policy = CollectionPolicyRevision::from_selection(
            None,
            IndexDetail::Detailed,
            Sha256Digest::from_bytes([13; 32]),
        );
        let value = report(
            file(),
            &legacy,
            policy.into(),
            policy.into(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )?;
        assert_eq!(
            serde_json::to_value(&value.applied_file)?,
            serde_json::json!({"adapter_id":legacy.adapter_id.clone(),"adapter_version":legacy.adapter_version.clone(),"effective_profile_digest":legacy.identity()?,"legacy_detail":"detailed","legacy_detail_was_absent":true})
        );
        assert_eq!(
            serde_json::to_value(policy)?,
            serde_json::json!({"detail":"detailed","policy_digest":policy.policy_digest})
        );
        Ok(())
    }
}
