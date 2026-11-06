// SPDX-License-Identifier: Apache-2.0
//! Per-file metadata projection; no source selection/extraction/producer executes.
use crate::Kernel;
use airk_core::{
    extraction::ExtractionProfile,
    profile_families::{
        self, CollectionPolicyObservation, CollectionPolicyRevision, CollectionPolicyUnavailable,
        Family, FileRevision, ProfileFamilyReport, ReplyInclusion, Support,
    },
};
use airk_store::current::{CurrentRead, collection::CollectionConfiguration};
use anyhow::Result;
use std::collections::BTreeMap;
impl Kernel {
    pub(crate) fn current_profile_families(
        &self,
        read: &CurrentRead,
        path: &str,
        hash: &str,
        profile: &ExtractionProfile,
        configuration: &CollectionConfiguration,
    ) -> Result<ProfileFamilyReport> {
        let registry = airk_ingest::adapters::registry::AdapterRegistry::baseline();
        // A permitted per-file report does not grant whole-index settings.
        // Only this typed scope refusal becomes an unavailable observation;
        // I/O/integrity/revocation errors retain their normal failure behavior.
        let (configured, published) = match self.require_index_metadata(read) {
            Ok(()) => {
                let policy = match self.store.extraction_policy_bytes(&self.root_id())? {
                    Some(bytes) => crate::extraction::ExtractionPolicy::parse(&bytes, &registry)?,
                    None => crate::extraction::ExtractionPolicy {
                        retained_profile: None,
                        legacy_fallback_detail: None,
                        version: 3,
                        detail: Some(configuration.index_detail),
                        rules: vec![],
                    },
                };
                (
                    CollectionPolicyObservation::from(CollectionPolicyRevision::from_selection(
                        policy.retained_profile,
                        policy.legacy_detail(),
                        crate::extraction::policy_digest(&policy)?,
                    )),
                    CollectionPolicyObservation::from(CollectionPolicyRevision::from_selection(
                        configuration.retained_profile,
                        configuration.index_detail,
                        configuration.extraction_policy_digest,
                    )),
                )
            }
            Err(error) if error.is::<airk_core::scoped_reads::ScopeReadRefused>() => {
                let unavailable = CollectionPolicyObservation::Unavailable {
                    reason: CollectionPolicyUnavailable::SelectedScope,
                };
                (unavailable, unavailable)
            }
            Err(error) => return Err(error),
        };
        let mut support = BTreeMap::from([(Family::SavedIdentity, Support::Supported)]);
        if let Some(capabilities) = registry.effective_capabilities(path, profile) {
            let observed = |value| {
                if value {
                    Support::Supported
                } else {
                    Support::Unsupported
                }
            };
            support.insert(Family::LexicalCandidates, observed(capabilities.lexical));
            support.insert(Family::Declarations, observed(capabilities.definitions));
            support.insert(Family::Imports, observed(capabilities.imports));
            support.insert(Family::WrittenCalls, observed(capabilities.call_candidates));
        }
        // FileInfo carries metadata/counts, never underlying facts/text/vectors.
        // Final renderer identity projection is unobserved here; keep it unknown.
        let reply = Family::ALL
            .into_iter()
            .filter(|family| *family != Family::SavedIdentity)
            .map(|family| (family, ReplyInclusion::Omitted))
            .collect();
        profile_families::report(
            FileRevision {
                publication: crate::current::runtime::revision(read.token()?),
                path: path.into(),
                source_hash: hash.parse()?,
            },
            profile,
            configured,
            published,
            &support,
            &reply,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        evidence::{EvidenceReadOptions, EvidenceReadStatus},
        extraction::IndexDetail,
    };
    use anyhow::Context;
    #[test]
    fn native_per_file_reports_preserve_applied_identity_after_config_change_and_source_loss()
    -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let base = temporary.path().canonicalize()?;
        let root = base.join("source");
        let data = base.join("data");
        std::fs::create_dir(&root)?;
        for (path, bytes) in [
            ("code.rs", "pub fn retained() {}\n"),
            (
                "Cargo.toml",
                "[package]\nname=\"fixture\"\nversion=\"0.1.0\"\n",
            ),
            (
                "table.md",
                "| key | value |\n| --- | --- |\n| one | two |\n",
            ),
            (
                "requirements.md",
                "# Requirements\nREQ-1: retained metadata must preserve identity.\n",
            ),
        ] {
            std::fs::write(root.join(path), bytes)?;
        }
        let kernel = Kernel::open_service_collection(&root, Some(data), "families")?;
        kernel.configure_vectors(false)?;
        kernel.configure_index_detail(IndexDetail::AgentLean)?;
        kernel.index_service_sources()?;
        let pinned = kernel.current_reader()?;
        let expected = pinned
            .artifact("Cargo.toml")?
            .context("manifest absent")?
            .retained
            .context("manifest retained metadata absent")?
            .profile
            .context("manifest profile absent")?
            .identity()?;
        let publication = crate::current::runtime::revision(pinned.token()?);
        drop(pinned);
        let legacy = kernel.file_info_conditional("Cargo.toml", &Default::default())?;
        assert!(
            serde_json::to_value(legacy.content.as_ref().unwrap())?
                .get("profile_families")
                .is_none()
        );
        let configured = kernel.configure_index_detail(IndexDetail::Detailed)?;
        let quota = kernel.collection_quota()?;
        let before = quota.status()?;
        let mut profiles = std::collections::BTreeSet::new();
        for path in ["code.rs", "Cargo.toml", "table.md", "requirements.md"] {
            let info = kernel.file_info_with_profile_families(path, true)?;
            let report = info.profile_families.context("opt-in report absent")?;
            assert_eq!(report.file.publication, publication);
            assert_eq!(
                report.applied_file.legacy_detail,
                Some(IndexDetail::AgentLean)
            );
            assert_eq!(
                report
                    .configured_collection_policy
                    .known()
                    .unwrap()
                    .policy_digest,
                configured.digest
            );
            assert_eq!(
                report.configured_collection_policy.known().unwrap().detail,
                Some(IndexDetail::Detailed)
            );
            assert_eq!(report.pending_collection_reindex, Some(true));
            assert_eq!(report.families.len(), 18);
            assert!(!report.live_source_checked);
            profiles.insert(report.applied_file.effective_profile_digest);
        }
        assert!(
            profiles.len() >= 2,
            "mixed adapters must retain independent file profiles"
        );
        let changed = kernel.file_info_conditional_with_profile_families(
            "Cargo.toml",
            &EvidenceReadOptions {
                if_evidence: legacy.read.validator,
                ..Default::default()
            },
            true,
        )?;
        assert_eq!(changed.read.status, EvidenceReadStatus::Content);
        assert_eq!(
            changed
                .content
                .as_ref()
                .unwrap()
                .profile_families
                .as_ref()
                .unwrap()
                .applied_file
                .effective_profile_digest,
            expected
        );
        let unchanged = kernel.file_info_conditional_with_profile_families(
            "Cargo.toml",
            &EvidenceReadOptions {
                if_evidence: changed.read.validator,
                ..Default::default()
            },
            true,
        )?;
        assert_eq!(unchanged.read.status, EvidenceReadStatus::NotModified);
        std::fs::rename(&root, base.join("unavailable-originals"))?;
        let missing = kernel.file_info_conditional_with_profile_families(
            "Cargo.toml",
            &Default::default(),
            true,
        )?;
        assert_eq!(
            serde_json::to_value(missing.content)?,
            serde_json::to_value(changed.content)?
        );
        let after = quota.status()?;
        assert_eq!(before.used_bytes, after.used_bytes);
        assert_eq!(before.reserved_bytes, after.reserved_bytes);
        Ok(())
    }

    #[test]
    fn authorized_partial_scope_report_skips_private_policy_bytes_and_keeps_file_identity()
    -> Result<()> {
        partial_scope_report(None)
    }
    #[test]
    fn selected_partial_scope_report_keeps_profile3_without_private_policy_authority() -> Result<()>
    {
        partial_scope_report(Some(airk_core::retention::RetainedProfile::Lean))
    }
    fn partial_scope_report(target: Option<airk_core::retention::RetainedProfile>) -> Result<()> {
        use airk_core::{
            grant_provenance::GrantProvenance,
            identity::{Incarnation, Sha256Digest},
            index_catalog::*,
            scoped_reads::{ScopeReadRefused, SourceDescriptor},
        };
        let temporary = tempfile::tempdir()?;
        let base = temporary.path().canonicalize()?;
        let root = base.join("source");
        let data = base.join("data");
        std::fs::create_dir_all(root.join("public"))?;
        std::fs::write(root.join("public/one.rs"), "pub struct Authorized;\n")?;
        std::fs::write(root.join("secret.rs"), "pub struct OutsideGrant;\n")?;
        let kernel = Kernel::open_service_collection(&root, Some(data), "scoped-families")?;
        kernel.configure_vectors(false)?;
        kernel.index_service_sources()?;
        if let Some(target) = target {
            let review = kernel.preview_retained_migration(target)?;
            kernel.configure_retained_migration(&review)?;
            kernel.index_service_sources()?;
        }
        let read = kernel.current_reader()?;
        let token = crate::current::runtime::revision(read.token()?);
        let artifact = read
            .artifact("public/one.rs")?
            .context("authorized artifact absent")?;
        let expected = artifact
            .retained
            .as_ref()
            .and_then(|value| value.profile.as_ref())
            .context("profile absent")?
            .identity()?;
        let hash: Sha256Digest = artifact.content_hash.parse()?;
        drop(read);
        let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
        let index = IndexId::from_identity(Incarnation::from_bytes([2; 16]));
        let source = SourceRootId::from_identity(Incarnation::from_bytes([3; 16]));
        let authority = crate::scoped_reads::ScopedReadAuthority {
            schema: 1,
            backing: airk_core::service::ProjectRegistration::new(
                kernel.root_id(),
                kernel.data_root.to_str().unwrap().into(),
                kernel.collection.clone(),
            )?
            .binding,
            index_id: index,
            project_ids: vec![project],
            provenance: GrantProvenance::UserReviewedProjectGrant,
            grant: ReviewedGrant {
                id: GrantId::from_identity(Incarnation::from_bytes([4; 16])),
                host: Sha256Digest::from_bytes([5; 32]),
                revision: CatalogRevision::INITIAL,
                revoked: false,
                memberships: vec![GrantedMembership {
                    membership: Membership {
                        project_id: project,
                        index_id: index,
                        revision: CatalogRevision::INITIAL,
                        scope: vec![ScopeAtom {
                            source_root_id: source,
                            kind: PathScope::Exact,
                            path: "public/one.rs".into(),
                        }],
                    },
                    source_revision: CatalogRevision::INITIAL,
                    policy_epoch: 0,
                    policy_digest: None,
                }],
            },
            sources: vec![SourceDescriptor {
                source_root_id: source,
                original_root: kernel.root_id(),
            }],
        };
        let scoped = kernel.with_scoped_reads(authority)?;
        // Any attempt to read private collection settings now fails, rather
        // than returning plausible metadata. The permitted per-file path must
        // never enter that branch under a partial Project grant.
        std::fs::write(
            kernel.store.directory().join("extraction-policy.json"),
            b"malformed private policy metadata",
        )?;
        let full_error = kernel
            .file_info_with_profile_families("public/one.rs", true)
            .err()
            .context("real private metadata error was downgraded")?;
        assert!(!full_error.is::<ScopeReadRefused>());
        std::fs::rename(&root, base.join("unavailable-originals"))?;
        let options = EvidenceReadOptions {
            revision: Some(token),
            known_hash: Some(hash),
            ..Default::default()
        };
        let basic = scoped.file_info_conditional("public/one.rs", &options)?;
        let result = scoped.file_info_conditional_with_profile_families(
            "public/one.rs",
            &EvidenceReadOptions {
                if_evidence: basic.read.validator,
                ..options.clone()
            },
            true,
        )?;
        assert_eq!(result.read.status, EvidenceReadStatus::Content);
        let report = result
            .content
            .as_ref()
            .and_then(|value| value.profile_families.as_ref())
            .context("authorized report suppressed")?;
        let unavailable = CollectionPolicyObservation::Unavailable {
            reason: CollectionPolicyUnavailable::SelectedScope,
        };
        assert_eq!(report.configured_collection_policy, unavailable);
        assert_eq!(report.published_collection_policy, unavailable);
        assert_eq!(report.pending_collection_reindex, None);
        assert_eq!(report.applied_file.effective_profile_digest, expected);
        if let Some(target) = target {
            assert!(
                matches!(&report.applied_file.applied_retention,Some(airk_core::retention::AppliedRetention::Selected{profile,..}) if *profile==target)
            );
            assert!(report.applied_file.legacy_detail.is_none());
        }

        assert_eq!(report.file.publication, token);
        assert_eq!(report.file.source_hash, hash);
        assert_eq!(report.file.path, "public/one.rs");
        assert_eq!(report.families.len(), 18);
        assert!(!report.live_source_checked);
        let repeated = scoped.file_info_conditional_with_profile_families(
            "public/one.rs",
            &EvidenceReadOptions {
                if_evidence: result.read.validator,
                ..options
            },
            true,
        )?;
        assert_eq!(repeated.read.status, EvidenceReadStatus::NotModified);
        let error = scoped
            .file_info_with_profile_families("secret.rs", true)
            .err()
            .context("outside-grant file was reported")?;
        assert!(error.is::<ScopeReadRefused>());
        Ok(())
    }
    #[test]
    fn selected_file_report_keeps_actual_identity_and_private_policy_separate_without_source_io()
    -> Result<()> {
        use airk_core::retention::{AppliedRetention, RetainedFamily, RetainedProfile};
        let temporary = tempfile::tempdir()?;
        let base = temporary.path().canonicalize()?;
        let root = base.join("source");
        let data = base.join("data");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("Cargo.toml"),b"[package]\nname=\"profile-families\"\nversion=\"0.1.0\"\n[dependencies]\nserde=\"1\"\n")?;
        let kernel = Kernel::open_service_collection(&root, Some(data), "selected-families")?;
        kernel.configure_vectors(false)?;
        kernel.index_service_sources()?;
        let review = kernel.preview_retained_migration(RetainedProfile::Lean)?;
        kernel.configure_retained_migration(&review)?;
        kernel.index_service_sources()?;
        let actual = kernel
            .file_info_with_profile_families("Cargo.toml", true)?
            .profile_families
            .context("selected report missing")?;
        assert!(matches!(
            &actual.applied_file.applied_retention,
            Some(AppliedRetention::Selected {
                profile: RetainedProfile::Lean,
                family: RetainedFamily::Manifest,
                ..
            })
        ));
        assert!(actual.applied_file.legacy_detail.is_none());
        assert!(actual.applied_file.legacy_detail_was_absent.is_none());
        assert_eq!(
            actual
                .published_collection_policy
                .known()
                .unwrap()
                .retained_profile,
            Some(RetainedProfile::Lean)
        );
        assert_eq!(actual.pending_collection_reindex, Some(false));
        assert_eq!(
            actual
                .families
                .iter()
                .find(|r| r.family == Family::ManifestPackagesDependencies)
                .unwrap()
                .retention,
            profile_families::Retention::Retained
        );
        let rich = kernel.preview_retained_migration(RetainedProfile::Rich)?;
        kernel.configure_retained_migration(&rich)?;
        let changed = kernel.file_info_conditional_with_profile_families(
            "Cargo.toml",
            &Default::default(),
            true,
        )?;
        let report = changed
            .content
            .as_ref()
            .unwrap()
            .profile_families
            .as_ref()
            .unwrap();
        assert_eq!(report.applied_file, actual.applied_file);
        assert_eq!(
            report
                .configured_collection_policy
                .known()
                .unwrap()
                .retained_profile,
            Some(RetainedProfile::Rich)
        );
        assert_eq!(
            report
                .published_collection_policy
                .known()
                .unwrap()
                .retained_profile,
            Some(RetainedProfile::Lean)
        );
        assert_eq!(report.pending_collection_reindex, Some(true));
        std::fs::rename(&root, base.join("unavailable-originals"))?;
        let quota = kernel.collection_quota()?;
        let before = quota.status()?;
        let repeated = kernel.file_info_conditional_with_profile_families(
            "Cargo.toml",
            &EvidenceReadOptions {
                if_evidence: changed.read.validator,
                ..Default::default()
            },
            true,
        )?;
        assert_eq!(repeated.read.status, EvidenceReadStatus::NotModified);
        let missing = kernel.file_info_conditional_with_profile_families(
            "Cargo.toml",
            &Default::default(),
            true,
        )?;
        assert_eq!(
            serde_json::to_value(missing.content)?,
            serde_json::to_value(changed.content)?
        );
        let after = quota.status()?;
        assert_eq!(after.used_bytes, before.used_bytes);
        assert_eq!(after.reserved_bytes, before.reserved_bytes);
        Ok(())
    }
}
