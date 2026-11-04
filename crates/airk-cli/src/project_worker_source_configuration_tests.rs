// SPDX-License-Identifier: Apache-2.0
//! Actual owned RootConfig effects and process-restart reconciliation; no scans.
use super::*;
use airk_core::{
    index_catalog::SourceRootId,
    service_configuration::{ConfigurationRequest, ProjectConfiguration},
    source_policy::SourceRoot,
};
use std::{collections::BTreeMap, fs};

fn fixture() -> Result<(tempfile::TempDir, Kernel, SourceChangePlan)> {
    let temp = tempfile::tempdir()?;
    let base = temp.path().canonicalize()?;
    for name in ["source", "extra", "other", "control"] {
        fs::create_dir(base.join(name))?;
    }
    fs::write(base.join("source/original.txt"), b"unchanged source bytes")?;
    let registration = airk_core::service::ProjectRegistration::new(
        base.join("source").to_str().unwrap().into(),
        base.join("data").to_str().unwrap().into(),
        "fixture".into(),
    )?;
    let namespace = Incarnation::from_bytes([1; 16]);
    let _bound = airk_store::service::data_scope::bind(
        Path::new(&registration.data_root),
        &base.join("control"),
        namespace,
    )?;
    Collection::initialize(
        Path::new(&registration.data_root),
        &registration.root,
        &registration.collection,
        Incarnation::from_bytes([2; 16]),
    )?;
    let kernel = Kernel::open_service_collection_for_diagnostics(
        Path::new(&registration.root),
        Some(registration.data_root.clone().into()),
        &registration.collection,
    )?;
    let selected = Collection::existing(
        Path::new(&registration.data_root),
        &registration.root,
        &registration.collection,
    )?
    .select()?;
    let incarnation = selected.store.service_incarnation()?.unwrap();
    let settings =
        airk_store::service::collection::settings_digest(&selected.store, &registration.root)?;
    let extra = base.join("extra").to_str().unwrap().to_owned();
    let plan = SourceChangePlan {
        registration: registration.clone(),
        engine_identity: Sha256Digest::from_bytes([3; 32]),
        job: Sha256Digest::from_bytes([4; 32]),
        configuration: ConfigurationRequest {
            prepare: false,
            incarnation,
            settings,
            configuration: ProjectConfiguration::AddRoot {
                alias: "extra".into(),
                path: extra.clone(),
            },
        },
        before: SourceConfigurationSnapshot {
            primary_root: registration.root.clone(),
            aliases: BTreeMap::new(),
        },
        after: SourceConfigurationSnapshot {
            primary_root: registration.root.clone(),
            aliases: BTreeMap::from([("extra".into(), extra.clone())]),
        },
        sources: vec![
            SourceRoot {
                id: SourceRootId::from_identity(Incarnation::from_bytes([5; 16])),
                canonical_path: registration.root,
                aliases: vec![],
                owner: true,
            },
            SourceRoot {
                id: SourceRootId::from_identity(Incarnation::from_bytes([6; 16])),
                canonical_path: extra,
                aliases: vec!["extra".into()],
                owner: false,
            },
        ],
    };
    Ok((temp, kernel, plan))
}

#[test]
fn actual_source_effect_resumes_after_reopen_without_reapplying_or_touching_source_bytes()
-> Result<()> {
    let (_temp, kernel, plan) = fixture()?;
    let (selection, writer, _native) = prepare_plan(&kernel, &plan, plan.engine_identity, true)?;
    let path = selection.store.directory().join("roots.json");
    let bytes = fs::read(&path)?;
    let modified = fs::metadata(&path)?.modified()?;
    assert_eq!(
        RootConfig::load(selection.store.directory(), &plan.registration.root)?.roots,
        plan.after.aliases
    );
    assert!(selection.store.persistent_revision()?.is_none());
    drop(writer);
    drop(selection);
    drop(kernel);
    // Simulate a process loss after the durable native effect: the accepted
    // before-settings guard is now stale, yet desired-state proof can resume.
    let reopened = Kernel::open_service_collection_for_diagnostics(
        Path::new(&plan.registration.root),
        Some(plan.registration.data_root.clone().into()),
        &plan.registration.collection,
    )?;
    let (selection, writer, _native) = prepare_plan(&reopened, &plan, plan.engine_identity, false)?;
    assert_eq!(fs::read(&path)?, bytes);
    assert_eq!(fs::metadata(&path)?.modified()?, modified);
    assert_eq!(
        fs::read(Path::new(&plan.registration.root).join("original.txt"))?,
        b"unchanged source bytes"
    );
    drop(writer);
    drop(selection);
    Ok(())
}

#[test]
fn removal_can_apply_when_the_removed_source_is_unavailable() -> Result<()> {
    let (_temp, kernel, mut plan) = fixture()?;
    let (selection, writer, _) = prepare_plan(&kernel, &plan, plan.engine_identity, true)?;
    drop(writer);
    plan.configuration.settings = airk_store::service::collection::settings_digest(
        &selection.store,
        &plan.registration.root,
    )?;
    drop(selection);
    plan.before = plan.after.clone();
    plan.after.aliases.clear();
    plan.configuration.configuration = ProjectConfiguration::RemoveRoot {
        alias: "extra".into(),
    };
    plan.sources.retain(|source| source.owner);
    fs::remove_dir(&plan.before.aliases["extra"])?;
    let (selection, writer, _) = prepare_plan(&kernel, &plan, plan.engine_identity, true)?;
    assert!(
        RootConfig::load(selection.store.directory(), &plan.registration.root)?
            .roots
            .is_empty()
    );
    drop(writer);
    Ok(())
}

#[test]
fn stale_settings_engine_and_incarnation_refuse_before_native_effect() -> Result<()> {
    let (_temp, kernel, plan) = fixture()?;
    let mut stale = plan.clone();
    stale.configuration.settings = Sha256Digest::from_bytes([17; 32]);
    assert!(prepare_plan(&kernel, &stale, stale.engine_identity, true).is_err());
    let mut rebound = plan.clone();
    rebound.configuration.incarnation = Incarnation::from_bytes([18; 16]);
    assert!(prepare_plan(&kernel, &rebound, rebound.engine_identity, true).is_err());
    assert!(prepare_plan(&kernel, &plan, Sha256Digest::from_bytes([19; 32]), true).is_err());
    let selection = Collection::existing(
        Path::new(&plan.registration.data_root),
        &plan.registration.root,
        &plan.registration.collection,
    )?
    .select()?;
    assert!(
        RootConfig::load(selection.store.directory(), &plan.registration.root)?
            .roots
            .is_empty()
    );
    assert!(selection.store.persistent_revision()?.is_none());
    Ok(())
}

#[test]
fn recovery_never_applies_before_or_overwrites_unknown_configuration() -> Result<()> {
    let (temp, kernel, plan) = fixture()?;
    assert!(
        prepare_plan(&kernel, &plan, plan.engine_identity, false)
            .err()
            .unwrap()
            .is::<ConfigurationChanged>()
    );
    let selected = Collection::existing(
        Path::new(&plan.registration.data_root),
        &plan.registration.root,
        &plan.registration.collection,
    )?
    .select()?;
    let writer = selected.store.lock()?;
    let mut config = RootConfig::load(selected.store.directory(), &plan.registration.root)?;
    config.roots.insert(
        "other".into(),
        temp.path()
            .canonicalize()?
            .join("other")
            .to_str()
            .unwrap()
            .into(),
    );
    config.save(selected.store.directory())?;
    let bytes = fs::read(selected.store.directory().join("roots.json"))?;
    drop(writer);
    drop(selected);
    for apply in [false, true] {
        assert!(
            prepare_plan(&kernel, &plan, plan.engine_identity, apply)
                .err()
                .unwrap()
                .is::<ConfigurationChanged>()
        );
    }
    let selected = Collection::existing(
        Path::new(&plan.registration.data_root),
        &plan.registration.root,
        &plan.registration.collection,
    )?
    .select()?;
    assert_eq!(
        fs::read(selected.store.directory().join("roots.json"))?,
        bytes
    );
    Ok(())
}
