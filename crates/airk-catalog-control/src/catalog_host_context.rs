// SPDX-License-Identifier: Apache-2.0
//! Native administrative context capture; preferences never grant evidence access.
use crate::catalog_grant_draft::{GrantDraftSelection, NativeGrantDraftContext};
use airk_core::{
    catalog_host::NativeHostContextDescriptor,
    catalog_management::{CatalogEntity, CatalogItem},
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{CatalogRevision, MAX_SELECTION},
    scoped_reads::ScopeReadRefused,
};
use airk_store::service::registry::ServiceRegistry;
use anyhow::{Context, Result, ensure};

fn native_host(namespace: Incarnation, caller: Incarnation) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"airk/native-catalog-host/1",
        &[&serde_json::to_vec(&(namespace, caller))?],
    ))
}

pub fn current_native_host(namespace: Incarnation) -> Result<Sha256Digest> {
    native_host(namespace, crate::local_ipc::caller_identity()?)
}

/// Observe existing compatibility metadata without issuing or repairing access.
pub fn legacy_binding(
    registry: &mut ServiceRegistry,
    project: airk_core::service::ProjectBinding,
    expected_revision: Option<CatalogRevision>,
) -> Result<airk_core::catalog_host::NativeLegacyBindingObservation> {
    use airk_core::catalog_host::{
        NativeLegacyBindingEligibility as Eligibility, NativeLegacyBindingObservation,
    };
    let namespace = registry.incarnation()?;
    let state = registry.catalog_state()?;
    if let Some(state) = &state {
        ensure!(
            state.namespace == namespace
                && state.policy_contract
                    <= airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT
                && state.authority_contract <= 1
                && (state.authority_contract == 1 || state.policy_contract == 0),
            "unsupported legacy eligibility contract"
        );
        ensure!(
            state.authority_contract == 0 || state.activated,
            "legacy eligibility catalog is not activated"
        );
    }
    let revision = state.as_ref().map(|state| state.revision);
    let registration = registry.legacy_binding_registration(project)?;
    let eligibility = if expected_revision.is_some() && expected_revision != revision {
        Eligibility::Unavailable
    } else if let Some(state) = &state {
        if state.authority_contract == 0 {
            registration.map_or(Eligibility::Unavailable, |registration| {
                Eligibility::AuthorityZero { registration }
            })
        } else {
            if let Some(registration) = registration {
                match registry.legacy_compatibility_snapshot(
                    project,
                    current_native_host(namespace)?,
                    state.revision,
                ) {
                    Ok(grant) => {
                        let member = &grant.grant.memberships[0];
                        let detail = registry.catalog_detail(
                            &CatalogEntity::Index {
                                id: member.membership.index_id,
                            },
                            Some(state.revision),
                        )?;
                        match detail.item {
                            Some(CatalogItem::Index(index)) if index.backing == registration => {
                                Eligibility::ImportedSnapshot {
                                    registration,
                                    grant: Box::new(grant),
                                }
                            }
                            _ => Eligibility::Unavailable,
                        }
                    }
                    Err(error) if error.is::<ScopeReadRefused>() => Eligibility::Unavailable,
                    Err(error) => return Err(error),
                }
            } else {
                Eligibility::Unavailable
            }
        }
    } else {
        registration.map_or(Eligibility::Unavailable, |registration| {
            Eligibility::AuthorityZero { registration }
        })
    };
    ensure!(
        registry.incarnation()? == namespace && registry.catalog_state()? == state,
        "legacy eligibility snapshot changed"
    );
    let observation = NativeLegacyBindingObservation {
        schema: 1,
        namespace,
        catalog_revision: revision,
        project,
        eligibility,
    };
    observation.validate()?;
    Ok(observation)
}

fn selection_identity(selection: &GrantDraftSelection) -> Result<Sha256Digest> {
    // Canonical semantic validation remains in the grant builder. Bound this
    // capture before serializing and retain the exact supplied ordering/key.
    ensure!(
        selection.project_ids.len() <= MAX_SELECTION
            && selection
                .index_ids
                .as_ref()
                .is_none_or(|ids| ids.len() <= MAX_SELECTION),
        ScopeReadRefused
    );
    Ok(framed_digest(
        b"airk/native-context-grant-selection/1",
        &[&serde_json::to_vec(&(
            &selection.key,
            selection.expected_catalog_revision,
            &selection.project_ids,
            &selection.index_ids,
        ))?],
    ))
}

pub struct NativeHostContext {
    descriptor: NativeHostContextDescriptor,
    caller: Incarnation,
    selection: Option<Sha256Digest>,
}

impl NativeHostContext {
    /// Called inside the authenticated native metadata handler. No wire caller
    /// or host is accepted; local IPC already enforces the same-OS-user boundary.
    pub fn load(
        registry: &mut ServiceRegistry,
        key: Sha256Digest,
        expected_revision: Option<CatalogRevision>,
    ) -> Result<Self> {
        let caller = crate::local_ipc::caller_identity()?;
        let namespace = registry.incarnation()?;
        registry
            .catalog_state()?
            .context("Indexing catalog is unavailable; complete explicit setup or upgrade")?;
        let host = native_host(namespace, caller)?;
        let detail =
            registry.catalog_detail(&CatalogEntity::ContextKey { host, key }, expected_revision)?;
        ensure!(detail.namespace == namespace, ScopeReadRefused);
        let context = match detail.item {
            None => None,
            Some(CatalogItem::Context(context)) => Some(context),
            Some(_) => anyhow::bail!(ScopeReadRefused),
        };
        Self::from_snapshot(
            NativeHostContextDescriptor {
                schema: 1,
                namespace,
                host,
                key,
                catalog_revision: detail.revision,
                context,
            },
            caller,
        )
    }

    fn from_snapshot(descriptor: NativeHostContextDescriptor, caller: Incarnation) -> Result<Self> {
        descriptor.validate()?;
        ensure!(
            descriptor.host == native_host(descriptor.namespace, caller)?,
            ScopeReadRefused
        );
        Ok(Self {
            descriptor,
            caller,
            selection: None,
        })
    }

    pub fn descriptor(&self) -> &NativeHostContextDescriptor {
        &self.descriptor
    }

    /// Capture an explicit administrative review request. Saved context defaults
    /// are preferences, not permission, and cannot manufacture an active grant.
    pub fn for_review(mut self, selection: &GrantDraftSelection) -> Result<Self> {
        ensure!(
            selection.key.namespace == self.descriptor.namespace
                && selection.expected_catalog_revision == self.descriptor.catalog_revision,
            ScopeReadRefused
        );
        self.selection = Some(selection_identity(selection)?);
        Ok(self)
    }
}

impl NativeGrantDraftContext for NativeHostContext {
    fn namespace(&self) -> Incarnation {
        self.descriptor.namespace
    }
    fn host(&self) -> Sha256Digest {
        self.descriptor.host
    }
    fn context_key(&self) -> Sha256Digest {
        self.descriptor.key
    }
    fn caller(&self) -> Incarnation {
        self.caller
    }
    fn authorize_selection(&self, selection: &GrantDraftSelection) -> Result<()> {
        ensure!(
            selection.key.namespace == self.descriptor.namespace
                && selection.expected_catalog_revision == self.descriptor.catalog_revision
                && self.selection == Some(selection_identity(selection)?),
            ScopeReadRefused
        );
        Ok(())
    }
}

#[cfg(any(test, feature = "test-support"))]
pub mod legacy_binding_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{index_catalog::QueryProjectId, service::RequestKey};

    fn fixture() -> Result<(NativeHostContext, GrantDraftSelection)> {
        let namespace = Incarnation::from_bytes([1; 16]);
        let caller = Incarnation::from_bytes([2; 16]);
        let context = NativeHostContext::from_snapshot(
            NativeHostContextDescriptor {
                schema: 1,
                namespace,
                host: native_host(namespace, caller)?,
                key: Sha256Digest::from_bytes([3; 32]),
                catalog_revision: CatalogRevision::INITIAL,
                context: None,
            },
            caller,
        )?;
        let selection = GrantDraftSelection {
            key: RequestKey {
                namespace,
                epoch: 1,
                nonce: Incarnation::from_bytes([4; 16]),
            },
            expected_catalog_revision: CatalogRevision::INITIAL,
            project_ids: vec![QueryProjectId::from_identity(Incarnation::from_bytes(
                [5; 16],
            ))],
            index_ids: None,
        };
        Ok((context, selection))
    }

    #[test]
    fn native_identity_separates_registry_and_actual_caller() -> Result<()> {
        let a = Incarnation::from_bytes([1; 16]);
        let b = Incarnation::from_bytes([2; 16]);
        assert_eq!(native_host(a, a)?, native_host(a, a)?);
        assert_ne!(native_host(a, a)?, native_host(b, a)?);
        assert_ne!(native_host(a, a)?, native_host(a, b)?);
        let (context, _) = fixture()?;
        assert!(NativeHostContext::from_snapshot(context.descriptor, a).is_err());
        Ok(())
    }

    #[test]
    fn review_capture_cannot_be_reused_for_changed_selection_or_key() -> Result<()> {
        let (context, selection) = fixture()?;
        assert!(context.authorize_selection(&selection).is_err());
        let context = context.for_review(&selection)?;
        context.authorize_selection(&selection)?;
        for changed in 0..4 {
            let mut other = selection.clone();
            match changed {
                0 => other.key.nonce = Incarnation::from_bytes([9; 16]),
                1 => other.key.namespace = Incarnation::from_bytes([9; 16]),
                2 => other.expected_catalog_revision = CatalogRevision::INITIAL.next()?,
                _ => other.project_ids.clear(),
            }
            assert!(context.authorize_selection(&other).is_err());
        }
        Ok(())
    }

    #[test]
    fn observing_legacy_context_does_not_initialize_catalog_or_permissions() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
        let namespace = registry.incarnation()?;
        assert!(registry.catalog_state()?.is_none());
        assert!(
            NativeHostContext::load(&mut registry, Sha256Digest::from_bytes([2; 32]), None)
                .is_err()
        );
        assert!(registry.catalog_state()?.is_none());
        assert_eq!(registry.incarnation()?, namespace);
        Ok(())
    }
}
