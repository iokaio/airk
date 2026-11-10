// SPDX-License-Identifier: Apache-2.0
//! Bounded administrative context identity, separate from evidence permission.
use crate::{
    catalog_management::{CatalogOperation, CatalogPreview, CatalogRequest, MAX_PAGE_BYTES},
    identity::{Incarnation, Sha256Digest},
    index_catalog::{
        CatalogRevision, ContextChoices, IndexId, MAX_SELECTION, QueryProjectId, encoded_size,
        unique_bounded,
    },
    scoped_reads::ScopeReadRefused,
    service_records::RequestKey,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

/// Read-only native compatibility eligibility. This metadata is neither a
/// bearer credential nor a read grant; Connect independently revalidates it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeLegacyBindingObservation {
    pub schema: u32,
    pub namespace: Incarnation,
    pub catalog_revision: Option<CatalogRevision>,
    pub project: crate::service_records::ProjectBinding,
    pub eligibility: NativeLegacyBindingEligibility,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum NativeLegacyBindingEligibility {
    AuthorityZero {
        registration: crate::service_records::ProjectRegistration,
    },
    ImportedSnapshot {
        registration: crate::service_records::ProjectRegistration,
        grant: Box<crate::grant_provenance::PersistedGrant>,
    },
    Unavailable,
}
impl NativeLegacyBindingObservation {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported native legacy observation");
        let registration = match &self.eligibility {
            NativeLegacyBindingEligibility::AuthorityZero { registration } => Some(registration),
            NativeLegacyBindingEligibility::ImportedSnapshot {
                registration,
                grant,
            } => {
                grant.validate()?;
                ensure!(
                    self.catalog_revision.is_some() && !grant.grant.revoked,
                    "invalid native imported observation"
                );
                ensure!(
                    grant.provenance.imported_binding() == Some(self.project),
                    "native imported observation binding changed"
                );
                Some(registration)
            }
            NativeLegacyBindingEligibility::Unavailable => None,
        };
        if let Some(registration) = registration {
            registration.validate()?;
            ensure!(
                registration.binding == self.project,
                "native legacy observation association changed"
            );
        }
        encoded_size(self, MAX_PAGE_BYTES)?;
        Ok(())
    }
}

/// Native registry/OS-user identity and an explicitly supplied context key.
/// This is neither a bearer credential nor proof of permission to read evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeHostContextDescriptor {
    pub schema: u32,
    pub namespace: Incarnation,
    pub host: Sha256Digest,
    pub key: Sha256Digest,
    pub catalog_revision: CatalogRevision,
    pub context: Option<ContextChoices>,
}

impl NativeHostContextDescriptor {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema == 1, "unsupported native context descriptor");
        if let Some(context) = &self.context {
            context.validate()?;
            ensure!(
                context.host == self.host
                    && context.key == self.key
                    && context.revision <= self.catalog_revision,
                "native context descriptor association changed"
            );
        }
        encoded_size(self, 16 * 1024)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantDraftSelection {
    pub key: RequestKey,
    pub expected_catalog_revision: CatalogRevision,
    pub project_ids: Vec<QueryProjectId>,
    pub index_ids: Option<Vec<IndexId>>,
}
impl GrantDraftSelection {
    pub fn validate(&self) -> Result<()> {
        unique_bounded(&self.project_ids, MAX_SELECTION)?;
        ensure!(!self.project_ids.is_empty(), ScopeReadRefused);
        if let Some(indexes) = &self.index_ids {
            unique_bounded(indexes, MAX_SELECTION)?;
            ensure!(!indexes.is_empty(), ScopeReadRefused);
        }
        ensure!(self.key.epoch <= i64::MAX as u64, ScopeReadRefused);
        Ok(())
    }
}

/// Journal this complete native request before submitting it to the existing
/// CatalogApply lane. Recovery reuses these exact bytes and preview digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedGrantDraft {
    pub request: CatalogRequest,
    pub preview: CatalogPreview,
    pub preview_digest: Sha256Digest,
}
impl ReviewedGrantDraft {
    pub fn new(request: CatalogRequest, preview: CatalogPreview) -> Result<Self> {
        request.validate()?;
        ensure!(
            matches!(request.operation, CatalogOperation::CreateGrant { .. }),
            ScopeReadRefused
        );
        ensure!(
            preview.catalog_revision == request.expected_catalog_revision
                && preview.request_digest == request.identity()?
                && preview.operation == request.operation,
            ScopeReadRefused
        );
        let preview_digest = preview.identity()?;
        let draft = Self {
            request,
            preview,
            preview_digest,
        };
        encoded_size(&draft, MAX_PAGE_BYTES)?;
        Ok(draft)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index_catalog::{ContextId, ContextKind};

    fn descriptor() -> NativeHostContextDescriptor {
        let host = Sha256Digest::from_bytes([2; 32]);
        let key = Sha256Digest::from_bytes([3; 32]);
        NativeHostContextDescriptor {
            schema: 1,
            namespace: Incarnation::from_bytes([1; 16]),
            host,
            key,
            catalog_revision: CatalogRevision::INITIAL,
            context: Some(ContextChoices {
                id: ContextId::from_identity(Incarnation::from_bytes([4; 16])),
                host,
                key,
                kind: ContextKind::Empty,
                name: "Explicit empty context".into(),
                revision: CatalogRevision::INITIAL,
                project_ids: vec![],
                index_ids: vec![],
            }),
        }
    }

    #[test]
    fn context_descriptor_preserves_empty_choices_and_exact_wire_data() -> Result<()> {
        let value = descriptor();
        value.validate()?;
        let wire = serde_json::to_value(&value)?;
        assert!(wire.get("caller").is_none() && wire.get("token").is_none());
        assert_eq!(
            serde_json::from_value::<NativeHostContextDescriptor>(wire)?,
            value
        );
        let mut absent = value;
        absent.context = None;
        absent.validate()?;
        Ok(())
    }

    #[test]
    fn context_descriptor_refuses_foreign_or_future_associations() -> Result<()> {
        let original = descriptor();
        for changed in 0..4 {
            let mut value = original.clone();
            match changed {
                0 => value.schema = 2,
                1 => value.context.as_mut().unwrap().host = Sha256Digest::from_bytes([8; 32]),
                2 => value.context.as_mut().unwrap().key = Sha256Digest::from_bytes([8; 32]),
                _ => value.context.as_mut().unwrap().revision = CatalogRevision::INITIAL.next()?,
            }
            assert!(value.validate().is_err());
        }
        Ok(())
    }
}
