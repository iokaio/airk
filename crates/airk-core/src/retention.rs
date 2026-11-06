// SPDX-License-Identifier: Apache-2.0
//! Versioned retained evidence, independent of reply projection and source selection.
use crate::{
    extraction::{ExtractionProfile, IndexDetail},
    identity::{RevisionToken, Sha256Digest},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const PROFILE_CONTRACT: u32 = 3;
pub const ALGORITHM: &str = "airk/file-aware-retention/1";
const KEYS: [&str; 3] = ["retained_profile", "retained_family", "retention_algorithm"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedProfile {
    Lean,
    Rich,
}
impl RetainedProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lean => "lean",
            Self::Rich => "rich",
        }
    }
    pub fn retains_fact(self, family: RetainedFamily, kind: &str) -> bool {
        if self == Self::Rich {
            return true;
        }
        match family {
            RetainedFamily::Manifest => matches!(
                kind,
                "key"
                    | "section"
                    | "package_identity"
                    | "package_dependency"
                    | "package_script"
                    | "workspace_member"
                    | "workspace_configuration"
                    | "project_configuration"
                    | "build_system"
                    | "tool_configuration"
                    | "compiler_option"
                    | "configuration_extends"
                    | "project_reference"
                    | "configuration_path"
            ),
            RetainedFamily::Code | RetainedFamily::Markdown | RetainedFamily::Docx => {
                IndexDetail::AgentLean.retains_fact(kind)
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedFamily {
    Code,
    Manifest,
    Markdown,
    Docx,
}
impl RetainedFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Manifest => "manifest",
            Self::Markdown => "markdown",
            Self::Docx => "docx",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AppliedRetention {
    Legacy {
        detail: IndexDetail,
        index_detail_was_absent: bool,
    },
    Selected {
        profile: RetainedProfile,
        family: RetainedFamily,
        algorithm: String,
    },
}
impl AppliedRetention {
    /// Missing historical index_detail remains Detailed; absence itself is retained.
    pub fn from_profile(profile: &ExtractionProfile) -> Result<Self> {
        if profile.contract_version != PROFILE_CONTRACT {
            ensure!(
                matches!(profile.contract_version, 1 | 2),
                "unsupported legacy profile contract"
            );
            ensure!(
                KEYS.iter().all(|key| !profile.options.contains_key(*key)),
                "retention marker requires profile contract 3"
            );
            return Ok(Self::Legacy {
                detail: IndexDetail::from_profile(profile)?,
                index_detail_was_absent: !profile.options.contains_key("index_detail"),
            });
        }
        ensure!(
            !profile.options.contains_key("index_detail"),
            "new retention conflicts with legacy detail"
        );
        ensure!(
            profile.options.get(KEYS[2]).map(String::as_str) == Some(ALGORITHM),
            "unsupported retention algorithm"
        );
        let selection = match profile.options.get(KEYS[0]).map(String::as_str) {
            Some("lean") => RetainedProfile::Lean,
            Some("rich") => RetainedProfile::Rich,
            _ => anyhow::bail!("unsupported retained profile"),
        };
        let family = match profile.options.get(KEYS[1]).map(String::as_str) {
            Some("code") => RetainedFamily::Code,
            Some("manifest") => RetainedFamily::Manifest,
            Some("markdown") => RetainedFamily::Markdown,
            Some("docx") => RetainedFamily::Docx,
            _ => anyhow::bail!("unsupported retained family"),
        };
        Ok(Self::Selected {
            profile: selection,
            family,
            algorithm: ALGORITHM.into(),
        })
    }
    /// Only a qualified owning adapter may choose family. This pure method confers no admission.
    pub fn bind(
        base: &ExtractionProfile,
        selection: RetainedProfile,
        family: RetainedFamily,
    ) -> Result<ExtractionProfile> {
        base.identity()?;
        ensure!(
            base.contract_version <= 2,
            "expected unmodified producer profile"
        );
        let mut result = base.clone();
        result.contract_version = PROFILE_CONTRACT;
        result.options.remove("index_detail");
        result
            .options
            .insert(KEYS[0].into(), selection.as_str().into());
        result
            .options
            .insert(KEYS[1].into(), family.as_str().into());
        result.options.insert(KEYS[2].into(), ALGORITHM.into());
        result.identity()?;
        Ok(result)
    }
    pub fn tracks_code_continuity(&self) -> bool {
        match self {
            Self::Legacy { detail, .. } => detail.tracks_continuity(),
            Self::Selected { profile, .. } => *profile == RetainedProfile::Rich,
        }
    }
    pub fn tracks_document_continuity(&self) -> bool {
        match self {
            Self::Legacy { detail, .. } => *detail == IndexDetail::Detailed,
            Self::Selected { profile, .. } => *profile == RetainedProfile::Rich,
        }
    }
}
/// Eligibility only: the owning cache must still verify the retained object's hash and availability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReuseEligibility {
    SameEffectiveInput,
    SourceChanged,
    InterpretationChanged,
    LegacyProfileMissing,
}
/// Absence is not Detailed and never grants reuse under a newly invented identity.
/// Existing strict legacy hydration can separately derive demonstrable historical semantics.
pub fn recorded_reuse_eligibility(
    old_source: Sha256Digest,
    new_source: Sha256Digest,
    old: Option<&ExtractionProfile>,
    new: &ExtractionProfile,
) -> Result<ReuseEligibility> {
    if old_source != new_source {
        return Ok(ReuseEligibility::SourceChanged);
    }
    match old {
        Some(old) => reuse_eligibility(old_source, new_source, old, new),
        None => Ok(ReuseEligibility::LegacyProfileMissing),
    }
}
pub fn reuse_eligibility(
    old_source: Sha256Digest,
    new_source: Sha256Digest,
    old: &ExtractionProfile,
    new: &ExtractionProfile,
) -> Result<ReuseEligibility> {
    if old_source != new_source {
        return Ok(ReuseEligibility::SourceChanged);
    }
    Ok(if old.identity()? == new.identity()? {
        ReuseEligibility::SameEffectiveInput
    } else {
        ReuseEligibility::InterpretationChanged
    })
}
/// Prospective explicit review. Native saved-work authority and atomic publication remain owners.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionMigrationReview {
    pub schema: u32,
    pub expected_publication: RevisionToken,
    pub expected_applied_policy: Sha256Digest,
    pub reviewed_configured_policy: Sha256Digest,
    pub desired_policy: Sha256Digest,
    pub target: RetainedProfile,
}
impl RetentionMigrationReview {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1 && serde_json::to_vec(self)?.len() <= 4096,
            "invalid retention migration review"
        );
        Ok(())
    }
    pub fn verify_current(
        &self,
        publication: &RevisionToken,
        applied: Sha256Digest,
        configured: Sha256Digest,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            &self.expected_publication == publication
                && self.expected_applied_policy == applied
                && self.reviewed_configured_policy == configured,
            "retention migration review is stale"
        );
        Ok(())
    }
    pub fn identity(&self) -> Result<Sha256Digest> {
        self.validate()?;
        Ok(crate::identity::framed_digest(
            b"airk/retention-migration-review/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
    pub fn requires_publication(&self) -> bool {
        self.expected_applied_policy != self.desired_policy
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn legacy() -> ExtractionProfile {
        ExtractionProfile {
            contract_version: 2,
            adapter_id: "rust".into(),
            adapter_version: "fixture/1".into(),
            language: "rust".into(),
            dialect: None,
            grammar_identity: None,
            mapping_version: 1,
            chunk_version: 1,
            options: Default::default(),
            configuration_inputs: Default::default(),
        }
    }
    #[test]
    fn legacy_missing_detail_keeps_exact_identity_and_detailed_meaning() -> Result<()> {
        let p = legacy();
        let bytes = serde_json::to_vec(&p)?;
        let id = p.identity()?;
        assert_eq!(
            AppliedRetention::from_profile(&p)?,
            AppliedRetention::Legacy {
                detail: IndexDetail::Detailed,
                index_detail_was_absent: true
            }
        );
        assert_eq!(bytes, serde_json::to_vec(&p)?);
        assert_eq!(id, p.identity()?);
        let mut balanced = p;
        balanced
            .options
            .insert("index_detail".into(), "balanced".into());
        assert!(!AppliedRetention::from_profile(&balanced)?.tracks_document_continuity());
        Ok(())
    }
    #[test]
    fn new_selection_is_not_legacy_and_effective_identity_controls_reuse() -> Result<()> {
        let p = legacy();
        let lean = AppliedRetention::bind(&p, RetainedProfile::Lean, RetainedFamily::Code)?;
        let rich = AppliedRetention::bind(&p, RetainedProfile::Rich, RetainedFamily::Code)?;
        assert!(IndexDetail::from_profile(&lean).is_err());
        let hash = Sha256Digest::from_bytes([1; 32]);
        assert_eq!(
            reuse_eligibility(hash, hash, &lean, &lean)?,
            ReuseEligibility::SameEffectiveInput
        );
        assert_eq!(
            recorded_reuse_eligibility(hash, hash, None, &lean)?,
            ReuseEligibility::LegacyProfileMissing
        );
        assert_eq!(
            reuse_eligibility(hash, hash, &lean, &rich)?,
            ReuseEligibility::InterpretationChanged
        );
        assert_eq!(
            reuse_eligibility(hash, Sha256Digest::from_bytes([2; 32]), &lean, &lean)?,
            ReuseEligibility::SourceChanged
        );
        let mut disguised = lean.clone();
        disguised.contract_version = 2;
        assert!(disguised.identity().is_err());
        assert!(IndexDetail::from_profile(&disguised).is_err());
        let mut unknown = lean;
        unknown
            .options
            .insert("retention_algorithm".into(), "future/99".into());
        assert!(unknown.identity().is_err());
        Ok(())
    }
    #[test]
    fn migration_review_is_explicit_stale_checked_and_noop_does_not_require_republication()
    -> Result<()> {
        let publication: RevisionToken =
            format!("airk-r1:{}:1:{}", "11".repeat(16), "22".repeat(32)).parse()?;
        let applied = Sha256Digest::from_bytes([3; 32]);
        let configured = Sha256Digest::from_bytes([4; 32]);
        let review = RetentionMigrationReview {
            schema: 1,
            expected_publication: publication,
            expected_applied_policy: applied,
            reviewed_configured_policy: configured,
            desired_policy: applied,
            target: RetainedProfile::Lean,
        };
        review.verify_current(&publication, applied, configured)?;
        assert!(!review.requires_publication());
        assert!(
            review
                .verify_current(&publication, configured, configured)
                .is_err()
        );
        assert!(
            review
                .verify_current(&publication, applied, applied)
                .is_err()
        );
        let mut migrate = review;
        migrate.desired_policy = configured;
        assert!(migrate.requires_publication());
        Ok(())
    }
    #[test]
    fn manifest_core_is_new_and_rich_is_a_superset() {
        for kind in [
            "package_identity",
            "package_dependency",
            "package_script",
            "workspace_configuration",
            "project_reference",
        ] {
            assert!(!IndexDetail::AgentLean.retains_fact(kind));
            assert!(RetainedProfile::Lean.retains_fact(RetainedFamily::Manifest, kind));
            assert!(RetainedProfile::Rich.retains_fact(RetainedFamily::Manifest, kind));
        }
        assert!(!RetainedProfile::Lean.retains_fact(RetainedFamily::Markdown, "date"));
        assert!(RetainedProfile::Lean.retains_fact(RetainedFamily::Markdown, "requirement"));
    }
}

/// Configuration acceptance is distinct from applying the new published representation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionMigrationResult {
    pub schema: u32,
    pub review_digest: Sha256Digest,
    pub target: RetainedProfile,
    pub publication_before: RevisionToken,
    pub applied_policy_before: Sha256Digest,
    pub configured_policy: Sha256Digest,
    pub policy_saved: bool,
    pub source_pass_requested: bool,
}
impl RetentionMigrationResult {
    pub fn for_review(review: &RetentionMigrationReview) -> Result<Self> {
        Ok(Self {
            schema: 1,
            review_digest: review.identity()?,
            target: review.target,
            publication_before: review.expected_publication,
            applied_policy_before: review.expected_applied_policy,
            configured_policy: review.desired_policy,
            policy_saved: review.reviewed_configured_policy != review.desired_policy,
            source_pass_requested: review.requires_publication(),
        })
    }
    pub fn validate_for(&self, review: &RetentionMigrationReview) -> Result<()> {
        ensure!(
            *self == Self::for_review(review)?,
            "retention migration result differs from exact saved review"
        );
        Ok(())
    }
}

/// Compact reviewed reference; never carries the private policy body on a public frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplitExtractionPolicyReference {
    pub schema: u32,
    pub policy_body_digest: Sha256Digest,
    pub configured_policy_digest: Sha256Digest,
    pub encoded_bytes: u32,
    pub retained_profile: RetainedProfile,
    pub legacy_fallback_detail: IndexDetail,
    pub required_extraction_contract: u32,
}
impl SplitExtractionPolicyReference {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.required_extraction_contract == PROFILE_CONTRACT
                && (1..=131_072).contains(&self.encoded_bytes),
            "invalid split extraction policy reference"
        );
        Ok(())
    }
    pub fn validate_body(&self, canonical_stored_body: &[u8]) -> Result<()> {
        self.validate()?;
        ensure!(
            canonical_stored_body.len() == self.encoded_bytes as usize
                && crate::identity::framed_digest(
                    b"airk/split-extraction-policy-body/1",
                    &[canonical_stored_body]
                ) == self.policy_body_digest,
            "split extraction policy body differs"
        );
        Ok(())
    }
}

/// Bounded metadata catalog, separate from legacy index_detail and reply projection.
pub fn catalog() -> serde_json::Value {
    serde_json::json!({"schema":1,"profiles":["lean","rich"],"profile_contract":PROFILE_CONTRACT,"policy_version":4,"algorithm":ALGORITHM,"requires_native_owned_collection":true,"existing_publication_required":true,"qualified_current_producers":["rust","project-manifest","markdown","instruction-markdown","docx"],"qualification":"complete exact current producer identity; other families/versions remain explicit legacy fallback","default_changed":false,"reply_independent":true,"semantic_pipeline_changed":false,"ocr":"unchanged explicit operations and provenance; no implicit setup"})
}

#[cfg(test)]
mod label_tests {
    #[test]
    fn legacy_balanced_remains_explicitly_separate_from_new_rich() {
        use crate::extraction::IndexDetail;
        assert_eq!(IndexDetail::Balanced.as_str(), "balanced");
        assert_eq!(
            IndexDetail::Balanced.display_name(),
            "Rich Structure (legacy balanced)"
        );
        assert_eq!(super::RetainedProfile::Rich.as_str(), "rich");
        assert_ne!(
            IndexDetail::Balanced.as_str(),
            super::RetainedProfile::Rich.as_str()
        );
    }
}
