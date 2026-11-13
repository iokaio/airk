// SPDX-License-Identifier: Apache-2.0
//! Imported legacy HTTP backend tests. Moved from
//! `catalog_host_context::legacy_binding_tests` (split step 2b): they build
//! the backend, which sits above the catalog.
use airk_catalog_control::catalog_host_context::legacy_binding_tests::{
    imported_fixture, insert_imported_fixture,
};
use airk_core::identity::{Incarnation, Sha256Digest};
use airk_core::service::ProjectRegistration;
use anyhow::Result;

#[tokio::test]
async fn imported_http_union_preserves_all_bindings_and_requires_explicit_project() -> Result<()> {
    use airk_core::{
        grant_provenance::GrantProvenance,
        service_http::{HttpSelection, NativeImportedHttpUnion},
    };
    use airk_installer::runtime_install::http_legacy::{LegacyMigrationResolution, resolve};
    use airk_mcp::read_attempt::ReadAttemptContext;
    use std::time::{Duration, Instant};
    let (owner, mut registry, first, grant) = imported_fixture(false)?;
    let control = owner.path().join("runtime");
    let second = ProjectRegistration::new(
        owner
            .path()
            .join("second-source")
            .to_string_lossy()
            .into_owned(),
        owner
            .path()
            .join("second-data")
            .to_string_lossy()
            .into_owned(),
        "second".into(),
    )?;
    let GrantProvenance::ImportedLegacySnapshot { upgrade_id, .. } = grant.provenance else {
        panic!("import fixture");
    };
    let sql = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
    let tx = sql.unchecked_transaction()?;
    let second_grant =
        insert_imported_fixture(&tx, registry.incarnation()?, upgrade_id, &second, false, 30)?;
    tx.commit()?;
    let mut selections = Vec::new();
    for (offset, registration) in [first.clone(), second.clone()].into_iter().enumerate() {
        let LegacyMigrationResolution::Eligible { selection } = resolve(
            &mut registry,
            registration,
            Sha256Digest::from_bytes([90 + offset as u8; 32]),
        )?
        else {
            panic!("exact imported fixture");
        };
        selections.push(*selection);
    }
    let union = NativeImportedHttpUnion {
        namespace: registry.incarnation()?,
        selections,
        no_default_project: true,
    };
    union.validate()?;
    let mut duplicate = union.clone();
    duplicate.selections[1] = duplicate.selections[0].clone();
    assert!(duplicate.validate().is_err());
    let mut crossed = union.clone();
    crossed.selections[1].namespace = Incarnation::from_bytes([99; 16]);
    assert!(crossed.validate().is_err());
    let mut narrowed = union.clone();
    narrowed.no_default_project = false;
    assert!(narrowed.validate().is_err());
    let selection = HttpSelection::ImportedLegacyUnion { selection: union };
    let context =
        || ReadAttemptContext::new(Instant::now() + Duration::from_secs(10), Default::default());
    let factory =
        crate::http_backend::factory(control.clone(), selection.clone(), context()).await?;
    let backend = factory()?;
    let projects = backend.call("projects".into(), Default::default()).await?;
    let projects: serde_json::Value =
        serde_json::from_str(projects["content"][0]["text"].as_str().unwrap())?;
    assert!(projects["default_project"].is_null());
    let bindings = projects["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["project"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let first_id = first.binding.to_string();
    let second_id = second.binding.to_string();
    assert_eq!(
        bindings,
        std::collections::BTreeSet::from([first_id.as_str(), second_id.as_str()])
    );
    let ambiguous = backend
        .call(
            "airk_search".into(),
            serde_json::json!({"query":"original"})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await?;
    assert_eq!(ambiguous["isError"], true);
    assert!(ambiguous.to_string().contains("PROJECT_REQUIRED"));
    let foreign = backend
        .call(
            "airk_search".into(),
            serde_json::json!({"query":"original","project":"ff".repeat(32)})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await?;
    assert_eq!(foreign["isError"], true);
    sql.execute(
        "UPDATE index_access_grants SET revoked=1,revision=2 WHERE id=?1",
        [second_grant.grant.id.to_string()],
    )?;
    assert!(
        crate::http_backend::validate(&control, &selection, &context())
            .await
            .is_err(),
        "one revoked member refuses the entire original group, not a narrower first-root credential"
    );
    backend.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn imported_http_factory_is_lazy_and_revalidates_revocation() -> Result<()> {
    use airk_core::service_http::HttpSelection;
    use airk_installer::runtime_install::http_legacy::{LegacyMigrationResolution, resolve};
    use airk_mcp::read_attempt::ReadAttemptContext;
    use std::time::{Duration, Instant};
    let (owner, mut registry, registration, _) = imported_fixture(false)?;
    let LegacyMigrationResolution::Eligible { selection } = resolve(
        &mut registry,
        registration,
        Sha256Digest::from_bytes([93; 32]),
    )?
    else {
        panic!("expected imported scope");
    };
    let selection = HttpSelection::ImportedLegacy { selection };
    let control = owner.path().join("runtime");
    let context =
        ReadAttemptContext::new(Instant::now() + Duration::from_secs(10), Default::default());
    let factory = crate::http_backend::factory(control.clone(), selection.clone(), context).await?;
    let first = factory()?;
    let second = factory()?;
    assert!(!std::sync::Arc::ptr_eq(&first, &second));
    assert!(!first.catalog().is_empty());
    first.close();
    first.shutdown().await?;
    // Factory/catalog creation needs no live service or worker and changes no grant.
    let sql = rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?;
    sql.execute("UPDATE index_access_grants SET revoked=1,revision=2", [])?;
    let context =
        ReadAttemptContext::new(Instant::now() + Duration::from_secs(10), Default::default());
    assert!(
        crate::http_backend::validate(&control, &selection, &context)
            .await
            .is_err()
    );
    let context = ReadAttemptContext::new(Instant::now(), Default::default());
    assert!(
        crate::http_backend::validate(&control, &selection, &context)
            .await
            .is_err()
    );
    assert_eq!(
        sql.query_row("SELECT count(*) FROM index_access_grants", [], |r| r
            .get::<_, i64>(0))?,
        1
    );
    second.shutdown().await?;
    Ok(())
}
