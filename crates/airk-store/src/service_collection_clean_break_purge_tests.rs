// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn clean_break_stage_refuses_foreign_root_filesystem_before_unlink() -> Result<()> {
    stage_filesystem_refusal(false)
}

#[test]
fn clean_break_stage_refuses_foreign_descendant_filesystem_before_unlink() -> Result<()> {
    stage_filesystem_refusal(true)
}

fn stage_filesystem_refusal(descendant: bool) -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let base = temporary.path().canonicalize()?;
    let stage = base.join("exact-owned-stage");
    let child = stage.join("captured");
    fs::create_dir_all(&child)?;
    let payload = child.join("source-fragment");
    fs::write(&payload, b"private capture bytes")?;
    let refused = if descendant { &child } else { &stage };
    let result = dispose_clean_break_stage_turn_observed(&stage, |path| {
        Ok(if path == refused { 22 } else { 11 })
    });
    assert!(
        result.is_err(),
        "a different filesystem must refuse before deleting private captures"
    );
    assert_eq!(fs::read(payload)?, b"private capture bytes");
    Ok(())
}
use crate::service::{data_scope, registry::ServiceRegistry};
use airk_core::{
    index_catalog::{CatalogRevision, IndexId, SourceRootId},
    source_policy::{POLICY_SCHEMA, SourcePolicy, SourceRoot},
};

struct Fixture {
    _temporary: tempfile::TempDir,
    source: PathBuf,
    data: PathBuf,
    registry: ServiceRegistry,
    _bound: data_scope::BoundDataRoot,
    registrations: Vec<ProjectRegistration>,
    collections: Vec<Collection>,
    leases: Vec<ProjectLease>,
}

fn fixture() -> Result<Fixture> {
    let temporary = match std::env::var_os("AIRK_PRIVATE_CENSUS_BASE") {
        Some(base) => tempfile::Builder::new()
            .prefix("airk-clean-break-")
            .tempdir_in(base)?,
        None => tempfile::tempdir()?,
    };
    let base = temporary.path().canonicalize()?;
    let source = base.join("sources");
    let data = base.join("data");
    let control = base.join("control");
    for path in [&source, &data, &control] {
        fs::create_dir(path)?;
    }
    fs::write(source.join("original.txt"), b"original source bytes")?;
    let namespace = Incarnation::from_bytes([82; 16]);
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    let bound = data_scope::bind(&data, &control, namespace)?;
    let mut registrations = Vec::new();
    let mut collections = Vec::new();
    let mut leases = Vec::new();
    for (position, name) in ["first", "second"].into_iter().enumerate() {
        let selected_source = source.join(name);
        fs::create_dir(&selected_source)?;
        fs::write(
            selected_source.join("original.txt"),
            b"original source bytes",
        )?;
        let registration = ProjectRegistration::new(
            selected_source.to_str().unwrap().into(),
            data.to_str().unwrap().into(),
            name.into(),
        )?;
        registry.register(&registration)?;
        let collection = Collection::initialize(
            &data,
            &registration.root,
            name,
            Incarnation::from_bytes([83 + position as u8; 16]),
        )?;
        let mut lease = ProjectLease::acquire(&control, registration.binding)?;
        registry.admit_worker(&mut lease)?;
        registrations.push(registration);
        collections.push(collection);
        leases.push(lease);
    }
    Ok(Fixture {
        _temporary: temporary,
        source,
        data,
        registry,
        _bound: bound,
        registrations,
        collections,
        leases,
    })
}

fn retain_policy(fixture: &Fixture) -> Result<()> {
    let registration = &fixture.registrations[0];
    let collection = &fixture.collections[0];
    let capsule = crate::current::policy_capsule::PolicyCapsule {
        schema: 1,
        policy: SourcePolicy {
            schema: POLICY_SCHEMA.into(),
            index_id: IndexId::from_identity(Incarnation::from_bytes([85; 16])),
            owner_binding: registration.binding,
            namespace: "collections-1.4".into(),
            collection: registration.collection.clone(),
            epoch: CatalogRevision::INITIAL,
            roots: vec![SourceRoot {
                id: SourceRootId::from_identity(Incarnation::from_bytes([86; 16])),
                canonical_path: registration.root.clone(),
                aliases: vec![],
                owner: true,
            }],
            positive_includes: None,
            denies: vec![],
        },
        operation_id: Incarnation::from_bytes([87; 16]),
        application: None,
        planned_removal: None,
    };
    collection.retain_source_policy(&capsule.policy)?;
    capsule.save(&collection.directory)
}

fn finish(fixture: &Fixture, position: usize) -> Result<()> {
    for _ in 0..8 {
        if Collection::purge_for_clean_break_turn(
            &fixture.registrations[position],
            &fixture.leases[position],
        )? {
            return Ok(());
        }
    }
    anyhow::bail!("bounded clean-break fixture did not finish")
}

#[test]
fn clean_break_purge_targets_each_exact_sibling_and_preserves_sources() -> Result<()> {
    let fixture = fixture()?;
    let second_owner = fs::read(fixture.collections[1].directory.join("service-owner.json"))?;
    finish(&fixture, 0)?;
    assert!(!fixture.collections[0].directory.exists());
    assert!(fixture.data.is_dir());
    assert_eq!(
        fs::read(fixture.collections[1].directory.join("service-owner.json"))?,
        second_owner
    );
    assert_eq!(
        fixture.registry.bindings()?.len(),
        2,
        "collection cleanup must not rewrite the old registry"
    );
    finish(&fixture, 1)?;
    assert!(!fixture.collections[1].directory.exists());
    assert_eq!(
        fs::read(fixture.source.join("original.txt"))?,
        b"original source bytes"
    );
    for registration in &fixture.registrations {
        assert_eq!(
            fs::read(Path::new(&registration.root).join("original.txt"))?,
            b"original source bytes"
        );
    }
    assert!(Collection::purge_for_clean_break_turn(
        &fixture.registrations[0],
        &fixture.leases[0]
    )?);
    Ok(())
}

#[test]
fn clean_break_purge_rejects_unknown_and_crossed_authority_before_deletion() -> Result<()> {
    let fixture = fixture()?;
    let directory = &fixture.collections[0].directory;
    let owner = fs::read(directory.join("service-owner.json"))?;
    fs::write(directory.join("user-notes.txt"), b"unrecognized data")?;
    assert!(
        Collection::purge_for_clean_break_turn(&fixture.registrations[0], &fixture.leases[0])
            .is_err()
    );
    assert_eq!(fs::read(directory.join("service-owner.json"))?, owner);
    assert_eq!(
        fs::read(directory.join("user-notes.txt"))?,
        b"unrecognized data"
    );
    assert!(directory.join("incarnations").is_dir());
    assert!(
        Collection::purge_for_clean_break_turn(&fixture.registrations[0], &fixture.leases[1])
            .is_err()
    );
    assert_eq!(
        fs::read(fixture.source.join("original.txt"))?,
        b"original source bytes"
    );
    Ok(())
}

#[test]
fn clean_break_purge_removes_valid_retained_policy_instead_of_reporting_false_completion()
-> Result<()> {
    let fixture = fixture()?;
    retain_policy(&fixture)?;
    finish(&fixture, 0)?;
    assert!(
        !fixture.collections[0].directory.exists(),
        "retained policy is old index state and must not survive a completed reset"
    );
    assert!(fixture.collections[1].directory.exists());
    assert_eq!(
        fs::read(fixture.source.join("original.txt"))?,
        b"original source bytes"
    );
    Ok(())
}

#[test]
fn clean_break_metadata_retirement_resumes_after_captured_owner_or_capsule_was_removed()
-> Result<()> {
    let fixture = fixture()?;
    retain_policy(&fixture)?;
    let registration = &fixture.registrations[0];
    let directory = &fixture.collections[0].directory;
    for _ in 0..8 {
        if Collection::purge_removed_turn(registration, &fixture.leases[0])? {
            break;
        }
    }
    let original_owner: Owner = json(&directory.join("service-owner.json"))?;
    let retirement = CleanBreakRetirement {
        schema: 1,
        registration: registration.clone(),
        original_owner,
        files: vec![(
            "source-policy.json".into(),
            retirement_file_digest(&directory.join("source-policy.json"))?,
        )],
    };
    write(&fixture.collections[0], CLEAN_BREAK_RETIREMENT, &retirement)?;
    fs::remove_file(directory.join("source-policy.json"))?;
    fs::remove_file(directory.join("service-owner.json"))?;
    assert!(Collection::purge_for_clean_break_turn(
        registration,
        &fixture.leases[0]
    )?);
    assert!(!directory.exists());
    assert!(fixture.collections[1].directory.exists());
    assert_eq!(
        fs::read(fixture.source.join("original.txt"))?,
        b"original source bytes"
    );
    Ok(())
}
