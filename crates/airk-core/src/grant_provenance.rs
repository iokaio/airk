// SPDX-License-Identifier: Apache-2.0
//! Persisted grant provenance and ID partitions. Syntax is never host authority.
use crate::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{GrantId, ReviewedGrant},
    service::ProjectBinding,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_IMPORTED_COMPAT_GRANTS: usize = 1024;
pub const MAX_REVIEWED_GRANTS: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantProvenanceKind {
    UserReviewedProjectGrant,
    ImportedLegacySnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GrantProvenance {
    UserReviewedProjectGrant,
    ImportedLegacySnapshot {
        upgrade_id: Incarnation,
        plan_digest: Sha256Digest,
        legacy_binding: ProjectBinding,
        configuration_digest: Sha256Digest,
    },
}
impl GrantProvenance {
    pub const fn kind(&self) -> GrantProvenanceKind {
        match self {
            Self::UserReviewedProjectGrant => GrantProvenanceKind::UserReviewedProjectGrant,
            Self::ImportedLegacySnapshot { .. } => GrantProvenanceKind::ImportedLegacySnapshot,
        }
    }
}

/// Administrative read wrapper only. User CreateGrant inputs remain ReviewedGrant
/// and cannot assert this provenance. Native Store validation establishes lineage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedGrant {
    pub grant: ReviewedGrant,
    pub provenance: GrantProvenance,
}
impl PersistedGrant {
    pub fn validate(&self) -> Result<()> {
        self.grant.validate()?;
        let imported = self.provenance.kind() == GrantProvenanceKind::ImportedLegacySnapshot;
        ensure!(
            is_imported_grant_id(self.grant.id) == imported,
            "grant provenance and ID partition disagree"
        );
        if imported {
            ensure!(
                self.grant.memberships.len() == 1
                    && self.grant.memberships[0].policy_epoch == 0
                    && self.grant.memberships[0].policy_digest.is_none(),
                "imported compatibility grant must retain one empty-policy index"
            );
        }
        Ok(())
    }
}

pub const fn is_imported_grant_id(id: GrantId) -> bool {
    id.identity().as_bytes()[0] & 0x80 != 0
}
fn partitioned_id(digest: Sha256Digest, imported: bool) -> GrantId {
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    bytes[0] = (bytes[0] & 0x7f) | if imported { 0x80 } else { 0 };
    GrantId::from_identity(Incarnation::from_bytes(bytes))
}
pub fn native_reviewed_grant_id(digest: Sha256Digest) -> GrantId {
    partitioned_id(digest, false)
}
pub fn native_imported_grant_id(
    namespace: Incarnation,
    host: Sha256Digest,
    provenance: &GrantProvenance,
) -> Result<GrantId> {
    ensure!(
        matches!(provenance, GrantProvenance::ImportedLegacySnapshot { .. }),
        "imported ID requires native import lineage"
    );
    Ok(partitioned_id(
        framed_digest(
            b"airk/imported-legacy-grant/1",
            &[&serde_json::to_vec(&(namespace, host, provenance))?],
        ),
        true,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partitions_and_import_lineage_are_deterministic_and_disjoint() -> Result<()> {
        let namespace = Incarnation::from_bytes([1; 16]);
        let host = Sha256Digest::from_bytes([2; 32]);
        let provenance = GrantProvenance::ImportedLegacySnapshot {
            upgrade_id: namespace,
            plan_digest: host,
            legacy_binding: host,
            configuration_digest: host,
        };
        let imported = native_imported_grant_id(namespace, host, &provenance)?;
        assert!(is_imported_grant_id(imported));
        assert_eq!(
            imported,
            native_imported_grant_id(namespace, host, &provenance)?
        );
        assert!(!is_imported_grant_id(native_reviewed_grant_id(
            Sha256Digest::from_bytes([255; 32])
        )));
        assert_ne!(
            imported,
            native_imported_grant_id(namespace, Sha256Digest::from_bytes([3; 32]), &provenance)?
        );
        assert!(
            native_imported_grant_id(namespace, host, &GrantProvenance::UserReviewedProjectGrant)
                .is_err()
        );
        Ok(())
    }
    #[test]
    fn provenance_is_not_flattened_or_accepted_in_reviewed_input() -> Result<()> {
        let id = native_reviewed_grant_id(Sha256Digest::from_bytes([1; 32]));
        let grant = ReviewedGrant {
            id,
            host: Sha256Digest::from_bytes([2; 32]),
            revision: crate::index_catalog::CatalogRevision::INITIAL,
            memberships: Vec::new(),
            revoked: false,
        };
        let persisted = PersistedGrant {
            grant: grant.clone(),
            provenance: GrantProvenance::UserReviewedProjectGrant,
        };
        persisted.validate()?;
        let value = serde_json::to_value(&persisted)?;
        assert_eq!(value["grant"], serde_json::to_value(&grant)?);
        assert!(value.get("id").is_none());
        assert!(serde_json::from_value::<ReviewedGrant>(value).is_err());
        let mut forged = serde_json::to_value(grant)?;
        forged["provenance"] = serde_json::to_value(persisted.provenance)?;
        assert!(serde_json::from_value::<ReviewedGrant>(forged).is_err());
        Ok(())
    }

    #[test]
    fn reviewed_create_grant_rejects_reserved_imported_identity() -> Result<()> {
        use crate::{
            catalog_management::{CatalogOperation, CatalogRequest},
            index_catalog::CatalogRevision,
            service::RequestKey,
        };
        let caller = Incarnation::from_bytes([1; 16]);
        let grant = ReviewedGrant {
            id: native_reviewed_grant_id(Sha256Digest::from_bytes([2; 32])),
            host: Sha256Digest::from_bytes([3; 32]),
            revision: CatalogRevision::INITIAL,
            memberships: Vec::new(),
            revoked: false,
        };
        let mut request = CatalogRequest {
            caller,
            key: RequestKey {
                namespace: caller,
                epoch: 1,
                nonce: caller,
            },
            expected_catalog_revision: CatalogRevision::INITIAL,
            operation: CatalogOperation::CreateGrant { grant },
        };
        request.validate()?;
        let CatalogOperation::CreateGrant { grant } = &mut request.operation else {
            unreachable!()
        };
        grant.id = GrantId::from_identity(Incarnation::from_bytes([255; 16]));
        assert!(request.validate().is_err());
        Ok(())
    }
}
