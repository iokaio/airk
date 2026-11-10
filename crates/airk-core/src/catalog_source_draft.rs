// SPDX-License-Identifier: Apache-2.0
//! Public source-mutation selectors and complete native reviews. Selectors carry
//! no claimed filesystem/configuration proof or native executable identity.
use crate::{
    catalog_management::{CatalogOperation, CatalogPreview, CatalogRequest, MAX_PAGE_BYTES},
    identity::Sha256Digest,
    index_catalog::{
        CatalogRevision, DenyId, IndexId, PathScope, ScopeAtom, SourceRootId, encoded_size,
    },
    service_records::RequestKey,
    source_policy::validate_root_path,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceMutationSelection {
    AddRoot {
        alias: String,
        path: String,
    },
    RemoveRoot {
        alias: String,
    },
    TrimPath {
        source_root_id: SourceRootId,
        kind: PathScope,
        path: String,
    },
    UnblockPath {
        rule_id: DenyId,
    },
    /// Explicit user retirement only. Confirmed-missing observations use a
    /// separate native event path and cannot be asserted by this public input.
    RetireIndex,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMutationDraftSelection {
    pub key: RequestKey,
    pub expected_catalog_revision: CatalogRevision,
    pub index_id: IndexId,
    pub change: SourceMutationSelection,
}
impl SourceMutationDraftSelection {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.key.epoch <= i64::MAX as u64,
            "invalid source draft request epoch"
        );
        let alias = match &self.change {
            SourceMutationSelection::AddRoot { alias, path } => {
                validate_root_path(path)?;
                ensure!(
                    path.len() <= 4096 && !path.chars().any(char::is_control),
                    "invalid source draft path"
                );
                Some(alias)
            }
            SourceMutationSelection::RemoveRoot { alias } => Some(alias),
            SourceMutationSelection::TrimPath {
                source_root_id,
                kind,
                path,
            } => {
                ScopeAtom {
                    source_root_id: *source_root_id,
                    kind: *kind,
                    path: path.clone(),
                }
                .validate()?;
                None
            }
            SourceMutationSelection::UnblockPath { .. } | SourceMutationSelection::RetireIndex => {
                None
            }
        };
        if let Some(alias) = alias {
            ensure!(
                !alias.is_empty()
                    && alias.len() <= 64
                    && alias
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
                "invalid source draft alias"
            );
        }
        encoded_size(self, MAX_PAGE_BYTES)?;
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMutationDraft {
    pub request: CatalogRequest,
    pub preview: CatalogPreview,
    pub preview_digest: Sha256Digest,
}
impl SourceMutationDraft {
    pub fn validate(&self) -> Result<()> {
        self.request.validate()?;
        ensure!(
            matches!(
                self.request.operation,
                CatalogOperation::BeginIndexSourceChange { .. }
                    | CatalogOperation::ChangeIndexPolicy { .. }
                    | CatalogOperation::RetireIndex { .. }
            ),
            "source draft requires a root change, policy change or index retirement"
        );
        ensure!(
            self.preview.request_digest == self.request.identity()?
                && self.preview.operation == self.request.operation
                && self.preview.catalog_revision == self.request.expected_catalog_revision
                && self.preview.next_revision == self.request.expected_catalog_revision.next()?
                && self.preview.authority_fence_required
                && self.preview.file_data_changes
                    == matches!(
                        self.request.operation,
                        CatalogOperation::ChangeIndexPolicy { .. }
                    )
                && self.preview.identity()? == self.preview_digest,
            "source draft review identity changed"
        );
        encoded_size(self, MAX_PAGE_BYTES)?;
        Ok(())
    }
}

#[cfg(test)]
mod policy_selection_tests {
    use super::*;
    use crate::identity::Incarnation;

    #[test]
    fn trim_selection_is_root_relative_and_full_root_requires_subtree() -> Result<()> {
        let identity = Incarnation::from_bytes([71; 16]);
        let source = SourceRootId::from_identity(identity);
        let mut selection = SourceMutationDraftSelection {
            key: RequestKey {
                namespace: identity,
                epoch: 1,
                nonce: identity,
            },
            expected_catalog_revision: CatalogRevision::INITIAL,
            index_id: IndexId::from_identity(identity),
            change: SourceMutationSelection::TrimPath {
                source_root_id: source,
                kind: PathScope::Subtree,
                path: String::new(),
            },
        };
        selection.validate()?;
        selection.change = SourceMutationSelection::TrimPath {
            source_root_id: source,
            kind: PathScope::Exact,
            path: String::new(),
        };
        assert!(selection.validate().is_err());
        for path in [
            "../private",
            "private/../other",
            "/private",
            "private\\other",
            "private//other",
        ] {
            selection.change = SourceMutationSelection::TrimPath {
                source_root_id: source,
                kind: PathScope::Subtree,
                path: path.into(),
            };
            assert!(selection.validate().is_err(), "{path}");
        }
        Ok(())
    }
}
