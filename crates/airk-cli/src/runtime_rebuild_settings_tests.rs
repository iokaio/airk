// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_store::service::data_scope;

fn roster_digest(roster: &SavedRoster) -> Result<Sha256Digest> {
    roster.validate()?;
    Ok(airk_core::identity::framed_digest(
        b"airk/rebuild-saved-roster/1",
        &[&serde_json::to_vec(roster)?],
    ))
}

#[test]
fn rebuild_revalidation_allows_only_forward_general_revision_with_identical_saved_roster()
-> Result<()> {
    let registration = ProjectRegistration::new(
        "/saved-source".into(),
        "/saved-state".into(),
        "fixture".into(),
    )?;
    let original = SavedRoster {
        namespace: Incarnation::from_bytes([81; 16]),
        revision: 9,
        indexes: vec![SavedIndex {
            saved_files: BTreeMap::from([
                ("roots.json".into(), None),
                (
                    "extraction-policy.json".into(),
                    Some(serde_json::to_vec(&serde_json::json!({
                        "version":1,"owner_root":registration.root,
                        "policy":{"version":3,"detail":IndexDetail::Balanced,"rules":[]}
                    }))?),
                ),
                (
                    "vector-policy.json".into(),
                    Some(serde_json::to_vec(&serde_json::json!({
                        "version":1,"owner_root":registration.root,"policy":{"enabled":false}
                    }))?),
                ),
                ("compression-policy.json".into(), None),
                ("scan-limits.json".into(), None),
            ]),
            registration,
            paused: true,
            watch: false,
            roots: BTreeMap::new(),
            vectors_enabled: false,
            index_detail: IndexDetail::Balanced,
            incarnation: Incarnation::from_bytes([82; 16]),
            settings_digest: Sha256Digest::from_bytes([83; 32]),
            default_model: None,
        }],
    };
    let original_digest = roster_digest(&original)?;
    let mut observed = original.clone();
    observed.revision = 10;
    original.check_observation(&observed)?;
    assert_eq!(original.revision, 9);
    assert_eq!(roster_digest(&original)?, original_digest);
    assert_ne!(roster_digest(&observed)?, original_digest);
    observed.indexes[0].watch = true;
    assert!(original.check_observation(&observed).is_err());
    observed = original.clone();
    observed.revision = 8;
    assert!(original.check_observation(&observed).is_err());
    observed = original.clone();
    observed.namespace = Incarnation::from_bytes([84; 16]);
    assert!(original.check_observation(&observed).is_err());
    observed = original.clone();
    observed.indexes.clear();
    assert!(original.check_observation(&observed).is_err());
    observed = original.clone();
    observed.revision = 10;
    observed.indexes[0]
        .saved_files
        .get_mut("extraction-policy.json")
        .and_then(Option::as_mut)
        .context("missing fixture policy")?
        .push(b' ');
    observed.validate()?;
    assert!(
        original.check_observation(&observed).is_err(),
        "changed exact settings bytes were ignored"
    );
    Ok(())
}

struct Fixture {
    _owner: tempfile::TempDir,
    control: std::path::PathBuf,
    registration: ProjectRegistration,
    collection: Collection,
}

fn fixture() -> Result<Fixture> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let control = base.join("control");
    let data = base.join("data");
    let source = base.join("source");
    for path in [&control, &data, &source] {
        crate::local_ipc::prepare_root(path)?;
    }
    fs::write(source.join("original.txt"), b"Saved source stays original.")?;
    let namespace = Incarnation::from_bytes([71; 16]);
    let registration = ProjectRegistration::new(
        source.to_str().unwrap().into(),
        data.to_str().unwrap().into(),
        "fixture".into(),
    )?;
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    registry.register(&registration)?;
    registry.pause(registration.binding, true)?;
    registry.configure_watch(registration.binding, true)?;
    data_scope::bind(&data, &control, namespace)?;
    let collection = Collection::initialize(
        &data,
        &registration.root,
        &registration.collection,
        Incarnation::from_bytes([72; 16]),
    )?;
    let store = collection.selected_store()?;
    drop(store.lock()?);
    fs::write(
        store.directory().join("extraction-policy.json"),
        serde_json::to_vec(&serde_json::json!({
            "version":1,"owner_root":registration.root,
            "policy":{"version":3,"detail":IndexDetail::Balanced,"rules":[]}
        }))?,
    )?;
    fs::write(
        store.directory().join("vector-policy.json"),
        serde_json::to_vec(&serde_json::json!({
            "version":1,"owner_root":registration.root,"policy":{"enabled":false}
        }))?,
    )?;
    Ok(Fixture {
        _owner: owner,
        control,
        registration,
        collection,
    })
}

#[test]
fn rebuild_roster_preserves_native_saved_roots_pause_watch_and_vector_opt_out() -> Result<()> {
    let fixture = fixture()?;
    let additional = fixture._owner.path().join("additional");
    fs::create_dir(&additional)?;
    let additional = additional.canonicalize()?;
    let store = fixture.collection.selected_store()?;
    let mut roots =
        airk_store::roots::RootConfig::load(store.directory(), &fixture.registration.root)?;
    roots.add(
        "extra",
        &additional,
        Path::new(&fixture.registration.root),
        Path::new(&fixture.registration.data_root),
    )?;
    let writer = store.lock()?;
    roots.save(store.directory())?;
    drop(writer);
    let source_before = fs::read(Path::new(&fixture.registration.root).join("original.txt"))?;
    let roster = capture(&fixture.control)?;
    assert_eq!(roster.indexes.len(), 1);
    let saved = &roster.indexes[0];
    assert_eq!(saved.registration, fixture.registration);
    assert!(saved.paused);
    assert!(saved.watch);
    assert!(!saved.vectors_enabled);
    assert_eq!(saved.index_detail, IndexDetail::Balanced);
    assert_eq!(
        saved.roots,
        BTreeMap::from([("extra".into(), additional.to_str().unwrap().into())])
    );
    roster.check(&fixture.control)?;
    let round_trip: SavedRoster = serde_json::from_slice(&serde_json::to_vec(&roster)?)?;
    assert_eq!(roster_digest(&round_trip)?, roster_digest(&roster)?);
    assert_eq!(
        source_before,
        fs::read(Path::new(&fixture.registration.root).join("original.txt"))?
    );
    Ok(())
}

#[test]
fn rebuild_roster_revalidation_refuses_changed_saved_controls_and_policy() -> Result<()> {
    let fixture = fixture()?;
    let roster = capture(&fixture.control)?;
    let mut registry = ServiceRegistry::open(&fixture.control, roster.namespace)?;
    registry.configure_watch(fixture.registration.binding, false)?;
    drop(registry);
    assert!(roster.check(&fixture.control).is_err());
    let changed = capture(&fixture.control)?;
    assert!(!changed.indexes[0].watch);
    let store = fixture.collection.selected_store()?;
    fs::write(
        store.directory().join("vector-policy.json"),
        serde_json::to_vec(&serde_json::json!({
            "version":1,"owner_root":fixture.registration.root,"policy":{"enabled":true}
        }))?,
    )?;
    assert!(changed.check(&fixture.control).is_err());
    Ok(())
}

#[test]
fn saved_rebuild_journal_refuses_mismatched_policy_choices_and_foreign_envelopes() -> Result<()> {
    let fixture = fixture()?;
    let original = capture(&fixture.control)?;
    let mut changed = original.clone();
    changed.indexes[0].vectors_enabled = true;
    assert!(changed.validate().is_err());
    let mut changed = original.clone();
    changed.indexes[0].index_detail = IndexDetail::Detailed;
    assert!(changed.validate().is_err());
    let mut changed = original.clone();
    changed.indexes[0]
        .roots
        .insert("invented".into(), fixture.registration.root.clone());
    assert!(changed.validate().is_err());
    let mut changed = original;
    changed.indexes[0].saved_files.insert(
        "vector-policy.json".into(),
        Some(serde_json::to_vec(&serde_json::json!({
            "version":1,"owner_root":"foreign-root","policy":{"enabled":false}
        }))?),
    );
    assert!(changed.validate().is_err());
    Ok(())
}

#[test]
fn rebuild_roster_refuses_unsupported_settings_without_defaulting_or_writing_sources() -> Result<()>
{
    let fixture = fixture()?;
    let store = fixture.collection.selected_store()?;
    let original = fs::read(Path::new(&fixture.registration.root).join("original.txt"))?;
    for path in [
        Path::new(&fixture.registration.data_root).join(".quota-1.4/policy.json"),
        store
            .managed_collection_directory()?
            .join("usage-policy.json"),
        store.directory().join("scan-limits.json"),
        store.directory().join("compression-policy.json"),
    ] {
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, b"original unsupported settings")?;
        assert!(
            capture(&fixture.control).is_err(),
            "accepted {}",
            path.display()
        );
        assert_eq!(fs::read(&path)?, b"original unsupported settings");
        fs::remove_file(path)?;
    }
    // Missing implicit detail cannot be guessed to mean AgentLean.
    fs::remove_file(store.directory().join("extraction-policy.json"))?;
    let error = capture(&fixture.control)
        .err()
        .context("missing inherited detail accepted")?;
    assert!(error.to_string().contains("readable saved applied detail"));
    assert_eq!(
        original,
        fs::read(Path::new(&fixture.registration.root).join("original.txt"))?
    );
    Ok(())
}

#[test]
fn rebuild_choices_preserve_inherited_detail_and_reject_custom_vector_and_extraction_rules()
-> Result<()> {
    let fixture = fixture()?;
    let files = BTreeMap::new();
    let choices = supported_choices(
        &files,
        Some(IndexDetail::Detailed),
        Some(fixture.control.as_path()),
        &fixture.registration.root,
    )?;
    assert_eq!(choices.index_detail, IndexDetail::Detailed);
    assert!(choices.vectors_enabled);
    let mut files = BTreeMap::new();
    files.insert("vector-policy.json".into(), Some(serde_json::to_vec(&serde_json::json!({
        "version":1,"owner_root":fixture.registration.root,"policy":{"enabled":false,"selection":{}}
    }))?));
    assert!(
        supported_choices(
            &files,
            Some(IndexDetail::Balanced),
            Some(fixture.control.as_path()),
            &fixture.registration.root
        )
        .is_err()
    );
    files.clear();
    files.insert("extraction-policy.json".into(), Some(serde_json::to_vec(&serde_json::json!({
        "version":1,"owner_root":fixture.registration.root,"policy":{"version":4,"detail":null,"rules":[],"retained_profile":"private_consumer_v1"}
    }))?));
    assert!(
        supported_choices(
            &files,
            Some(IndexDetail::Balanced),
            Some(fixture.control.as_path()),
            &fixture.registration.root
        )
        .is_err()
    );
    Ok(())
}
