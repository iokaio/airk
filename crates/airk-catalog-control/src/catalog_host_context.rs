// SPDX-License-Identifier: Apache-2.0
//! Native administrative context capture; preferences never grant evidence access.
use airk_core::{
    catalog_host::NativeHostContextDescriptor,
    catalog_management::{CatalogEntity, CatalogItem},
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::CatalogRevision,
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
            // Imported compatibility grants were removed (change-log entry
            // 68 part 3): an authority-1 catalog has no legacy-binding read.
            Eligibility::Unavailable
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

pub struct NativeHostContext {
    descriptor: NativeHostContextDescriptor,
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
        Ok(Self { descriptor })
    }

    pub fn descriptor(&self) -> &NativeHostContextDescriptor {
        &self.descriptor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Result<NativeHostContext> {
        let namespace = Incarnation::from_bytes([1; 16]);
        let caller = Incarnation::from_bytes([2; 16]);
        NativeHostContext::from_snapshot(
            NativeHostContextDescriptor {
                schema: 1,
                namespace,
                host: native_host(namespace, caller)?,
                key: Sha256Digest::from_bytes([3; 32]),
                catalog_revision: CatalogRevision::INITIAL,
                context: None,
            },
            caller,
        )
    }

    #[test]
    fn native_identity_separates_registry_and_actual_caller() -> Result<()> {
        let a = Incarnation::from_bytes([1; 16]);
        let b = Incarnation::from_bytes([2; 16]);
        assert_eq!(native_host(a, a)?, native_host(a, a)?);
        assert_ne!(native_host(a, a)?, native_host(b, a)?);
        assert_ne!(native_host(a, a)?, native_host(a, b)?);
        let context = fixture()?;
        assert!(NativeHostContext::from_snapshot(context.descriptor, a).is_err());
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
