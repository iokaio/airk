// SPDX-License-Identifier: Apache-2.0
//! Migration of existing imported authority; never creates or expands a grant.
use airk_core::{
    catalog_host::NativeLegacyBindingEligibility, identity::Sha256Digest,
    service::ProjectRegistration, service_http::NativeImportedHttpSelection,
};
use airk_store::service::registry::ServiceRegistry;
use anyhow::{Result, ensure};
use serde::Serialize;
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum LegacyMigrationResolution {
    Eligible {
        selection: Box<NativeImportedHttpSelection>,
    },
    ReviewRequired {
        registration: ProjectRegistration,
        configuration_receipt: Sha256Digest,
    },
}

/// Caller supplies the exact registration recovered from its owned old config.
/// Native lookup is by binding, with all root/data/collection bytes compared.
pub(crate) fn resolve(
    registry: &mut ServiceRegistry,
    registration: ProjectRegistration,
    configuration_receipt: Sha256Digest,
) -> Result<LegacyMigrationResolution> {
    resolve_claim(registry, registration, configuration_receipt, None)
}

pub(crate) fn resolve_claim(
    registry: &mut ServiceRegistry,
    registration: ProjectRegistration,
    configuration_receipt: Sha256Digest,
    origin_sha256: Option<Sha256Digest>,
) -> Result<LegacyMigrationResolution> {
    registration.validate()?;
    // A standalone import retains its original data-root association separately
    // from the new privately owned service storage. Resolve only the exact
    // saved original binding/configuration/native host; absence or ambiguity
    // never selects another index by folder or creates authority.
    let registration = if registry
        .catalog_state()?
        .is_some_and(|state| state.activated && state.authority_contract == 1)
    {
        let host = crate::catalog_host_context::current_native_host(registry.incarnation()?)?;
        let adopted = registry.adopted_legacy_registration_claim(
            &registration,
            configuration_receipt,
            host,
            origin_sha256,
        )?;
        if origin_sha256.is_some() && adopted.is_none() {
            return Ok(LegacyMigrationResolution::ReviewRequired {
                registration,
                configuration_receipt,
            });
        }
        adopted.unwrap_or(registration)
    } else {
        if origin_sha256.is_some() {
            return Ok(LegacyMigrationResolution::ReviewRequired {
                registration,
                configuration_receipt,
            });
        }
        registration
    };
    let observed =
        crate::catalog_host_context::legacy_binding(registry, registration.binding, None)?;
    if let NativeLegacyBindingEligibility::ImportedSnapshot {
        registration: saved,
        grant,
    } = observed.eligibility
        && saved == registration
        && let Some(revision) = observed.catalog_revision
    {
        let selection = NativeImportedHttpSelection {
            namespace: observed.namespace,
            expected_catalog_revision: revision,
            registration,
            grant: *grant,
            configuration_receipt,
        };
        validate_registry(registry, &selection)?;
        return Ok(LegacyMigrationResolution::Eligible {
            selection: Box::new(selection),
        });
    }
    Ok(LegacyMigrationResolution::ReviewRequired {
        registration,
        configuration_receipt,
    })
}

/// Revalidate saved authority and construct its exact fingerprint. Catalog clock
/// may advance; neither frozen grant, provenance nor registration may change.
pub(crate) fn validate_registry(
    registry: &mut ServiceRegistry,
    selection: &NativeImportedHttpSelection,
) -> Result<Sha256Digest> {
    selection.validate()?;
    let observed = crate::catalog_host_context::legacy_binding(
        registry,
        selection.registration.binding,
        None,
    )?;
    ensure!(
        observed.namespace == selection.namespace,
        airk_core::scoped_reads::ScopeReadRefused
    );
    let revision = observed
        .catalog_revision
        .ok_or(airk_core::scoped_reads::ScopeReadRefused)?;
    ensure!(
        revision >= selection.expected_catalog_revision,
        airk_core::scoped_reads::ScopeReadRefused
    );
    let NativeLegacyBindingEligibility::ImportedSnapshot {
        registration,
        grant,
    } = observed.eligibility
    else {
        anyhow::bail!(airk_core::scoped_reads::ScopeReadRefused);
    };
    ensure!(
        registration == selection.registration && *grant == selection.grant,
        airk_core::scoped_reads::ScopeReadRefused
    );
    let host = crate::catalog_host_context::current_native_host(selection.namespace)?;
    ensure!(
        host == selection.grant.grant.host,
        airk_core::scoped_reads::ScopeReadRefused
    );
    crate::catalog_authority::build_legacy_catalog_read_authority(
        registry,
        selection.namespace,
        host,
        revision,
        registration.binding,
    )?
    .fingerprint()
}

pub(crate) fn validate(
    control: &Path,
    selection: &NativeImportedHttpSelection,
) -> Result<Sha256Digest> {
    let mut registry =
        ServiceRegistry::open_saved_before(control, Some(Instant::now() + Duration::from_secs(2)))?;
    validate_registry(&mut registry, selection)
}

#[cfg(test)]
mod tests;
