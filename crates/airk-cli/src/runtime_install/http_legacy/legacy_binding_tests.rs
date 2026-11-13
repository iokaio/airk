// SPDX-License-Identifier: Apache-2.0
//! Legacy binding migration tests. Moved from
//! `catalog_host_context::legacy_binding_tests` (split step 2b): they resolve
//! through the installer's `http_legacy`, which sits above the catalog.
use crate::catalog_host_context::legacy_binding;
use crate::catalog_host_context::legacy_binding_tests::imported_fixture;
use airk_core::catalog_host::NativeLegacyBindingEligibility;
use airk_core::identity::{Incarnation, Sha256Digest};
use airk_core::index_catalog::CatalogRevision;
use airk_core::service::ProjectRegistration;
use airk_store::service::registry::ServiceRegistry;
use anyhow::Result;

#[test]
fn http_migration_retains_imported_scope_and_never_mints_reviewed_authority() -> Result<()> {
    use crate::runtime_install::http_legacy::{LegacyMigrationResolution, resolve, validate};
    for foreign in [false, true] {
        let (owner, mut registry, registration, original) = imported_fixture(foreign)?;
        let receipt = Sha256Digest::from_bytes([93; 32]);
        let resolution = resolve(&mut registry, registration.clone(), receipt)?;
        if foreign {
            assert!(matches!(
                resolution,
                LegacyMigrationResolution::ReviewRequired { .. }
            ));
        } else {
            let LegacyMigrationResolution::Eligible { selection } = resolution else {
                panic!("existing exact imported authority must remain eligible");
            };
            assert_eq!(selection.registration, registration);
            assert_eq!(selection.grant, original);
            assert_eq!(selection.configuration_receipt, receipt);
            assert!(
                matches!(
                    crate::runtime_install::http_legacy::resolve_claim(
                        &mut registry,
                        registration.clone(),
                        receipt,
                        Some(Sha256Digest::from_bytes([95; 32]))
                    )?,
                    LegacyMigrationResolution::ReviewRequired { .. }
                ),
                "unknown explicit standalone claim fell back to unrelated existing imported authority"
            );
            let control = owner.path().join("runtime");
            validate(&control, &selection)?;
            let mut crossed = selection.clone();
            crossed.namespace = Incarnation::from_bytes([94; 16]);
            assert!(validate(&control, &crossed).is_err());
            crossed = selection.clone();
            crossed.grant.provenance =
                airk_core::grant_provenance::GrantProvenance::UserReviewedProjectGrant;
            assert!(crossed.validate().is_err());
            let sql = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
            sql.execute("UPDATE index_access_grants SET revoked=1,revision=2", [])?;
            assert!(validate(&control, &selection).is_err());
            assert!(matches!(
                resolve(&mut registry, registration, receipt)?,
                LegacyMigrationResolution::ReviewRequired { .. }
            ));
            assert_eq!(
                sql.query_row("SELECT count(*) FROM index_access_grants", [], |r| r
                    .get::<_, i64>(0))?,
                1
            );
        }
    }
    Ok(())
}

#[test]
fn legacy_eligibility_observes_exact_registration_without_initializing_catalog() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("runtime");
    crate::local_ipc::prepare_root(&root)?;
    let mut registry = ServiceRegistry::open(&root, Incarnation::from_bytes([19; 16]))?;
    // These paths intentionally do not exist: this observation is metadata only.
    let registration = ProjectRegistration::new(
        owner.path().join("source").to_str().unwrap().into(),
        owner.path().join("data").to_str().unwrap().into(),
        "legacy-observation".into(),
    )?;
    registry.register(&registration)?;
    let migration = crate::runtime_install::http_legacy::resolve(
        &mut registry,
        registration.clone(),
        Sha256Digest::from_bytes([93; 32]),
    )?;
    assert!(matches!(
        migration,
        crate::runtime_install::http_legacy::LegacyMigrationResolution::ReviewRequired { .. }
    ));
    let observed = legacy_binding(&mut registry, registration.binding, None)?;
    observed.validate()?;
    assert_eq!(observed.namespace, registry.incarnation()?);
    assert_eq!(observed.catalog_revision, None);
    assert_eq!(
        observed.eligibility,
        NativeLegacyBindingEligibility::AuthorityZero {
            registration: registration.clone()
        }
    );
    let stale = legacy_binding(
        &mut registry,
        registration.binding,
        Some(CatalogRevision::INITIAL),
    )?;
    assert_eq!(
        stale.eligibility,
        NativeLegacyBindingEligibility::Unavailable
    );
    let missing = legacy_binding(&mut registry, Sha256Digest::from_bytes([20; 32]), None)?;
    assert_eq!(
        missing.eligibility,
        NativeLegacyBindingEligibility::Unavailable
    );
    assert!(registry.catalog_state()?.is_none());
    assert!(!owner.path().join("source").exists());
    Ok(())
}
