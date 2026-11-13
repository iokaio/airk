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
            incarnation: Some(Incarnation::from_bytes([82; 16])),
            settings_digest: Sha256Digest::from_bytes([83; 32]),
            settings_available: true,
            discarded_preferences: Vec::new(),
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

#[test]
fn clean_break_preferences_keep_supported_choices_and_discard_old_engine_settings() -> Result<()> {
    let fixture = fixture()?;
    let store = fixture.collection.selected_store()?;
    fs::write(
        store.directory().join("vector-policy.json"),
        serde_json::to_vec(&serde_json::json!({
            "version":1,"owner_root":fixture.registration.root,
            "policy":{"enabled":false,"model":{"old_engine":true},"selection":{"old_choice":true}}
        }))?,
    )?;
    fs::write(
        store.directory().join("extraction-policy.json"),
        serde_json::to_vec(&serde_json::json!({
            "version":1,"owner_root":fixture.registration.root,
            "policy":{"version":4,"detail":"essential","rules":[{"old_rule":true}],"retained_profile":"private_consumer_v1"}
        }))?,
    )?;
    fs::write(
        store.directory().join("compression-policy.json"),
        b"unsupported old bytes",
    )?;
    let original = fs::read(Path::new(&fixture.registration.root).join("original.txt"))?;
    assert!(
        capture(&fixture.control).is_err(),
        "strict retained-state rebuild must remain strict"
    );
    let roster = capture_clean_break(&fixture.control)?;
    assert_eq!(roster.indexes.len(), 1);
    let saved = &roster.indexes[0];
    assert!(saved.paused && saved.watch && saved.settings_available);
    assert!(!saved.vectors_enabled);
    assert_eq!(saved.index_detail, IndexDetail::Essential);
    assert!(saved.default_model.is_none());
    assert!(saved.saved_files["compression-policy.json"].is_none());
    assert!(
        saved
            .discarded_preferences
            .contains(&"vector-policy.json:model".into())
    );
    assert!(
        saved
            .discarded_preferences
            .contains(&"extraction-policy.json:rules".into())
    );
    roster.check_clean_break(&fixture.control)?;
    let mut too_many = roster.clone();
    too_many.indexes = vec![saved.clone(); 65];
    assert!(
        too_many.validate().is_err(),
        "complete roster must refuse over existing 64-index budget"
    );
    assert_eq!(
        fs::read(Path::new(&fixture.registration.root).join("original.txt"))?,
        original
    );
    assert_eq!(
        fs::read(store.directory().join("compression-policy.json"))?,
        b"unsupported old bytes"
    );
    Ok(())
}

#[test]
fn clean_break_capture_enumerates_two_catalog_indexes_and_all_raw_roots_without_old_storage()
-> Result<()> {
    use airk_core::catalog_setup::{
        IndexSetupRequest, SetupCompletion, SetupPreparationReview, SetupSource,
    };
    use airk_store::service::{
        ServiceLease,
        registry::catalog::{
            genesis::{
                CatalogGenesis,
                activation::{
                    AuthorityV1CandidateObservation, AuthorityV1NativeQualification,
                    AuthorityV1ReadinessProducer,
                },
            },
            setup::NativeIndexPreparation,
        },
    };
    struct MetadataProducer;
    impl AuthorityV1ReadinessProducer for MetadataProducer {
        fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>> {
            Ok(Vec::new())
        }
        fn qualify(
            &self,
            expected: &AuthorityV1CandidateObservation,
        ) -> Result<AuthorityV1NativeQualification> {
            Ok(AuthorityV1NativeQualification {
                candidate: expected.clone(),
                predecessor_refusals: Vec::new(),
            })
        }
    }
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let control = base.join("control");
    for name in ["control", "source", "first-extra", "second-extra"] {
        crate::local_ipc::prepare_root(&base.join(name))?;
    }
    fs::write(base.join("source/original.txt"), b"Original shared source.")?;
    let mut genesis = CatalogGenesis::begin(
        ServiceLease::acquire(&control)?,
        Sha256Digest::from_bytes([101; 32]),
        Incarnation::from_bytes([102; 16]),
    )?;
    genesis.backup()?;
    let receipt = genesis.migrate()?;
    genesis.promote_authority_v1()?;
    genesis.record_authority_v1_readiness(Incarnation::from_bytes([103; 16]), &MetadataProducer)?;
    genesis.settle_authority_v1()?;
    drop(genesis);
    let mut registry = ServiceRegistry::open(&control, receipt.namespace)?;
    let mut originals = Vec::new();
    for position in 0..2 {
        let registration = ProjectRegistration::new(
            base.join("source").to_str().unwrap().into(),
            base.join(format!("missing-storage-{position}"))
                .to_str()
                .unwrap()
                .into(),
            format!("fixture-{position}"),
        )?;
        let request = IndexSetupRequest {
            caller: Incarnation::from_bytes([104; 16]),
            key: airk_core::service::RequestKey {
                namespace: receipt.namespace,
                epoch: 0,
                nonce: Incarnation::from_bytes([105 + position as u8; 16]),
            },
            expected_catalog_revision: registry.catalog_state()?.unwrap().revision,
            name: format!("Independent saved index {position}"),
            registration: registration.clone(),
            sources: vec![
                SetupSource {
                    canonical_path: registration.root.clone(),
                    aliases: vec![],
                    owner: true,
                },
                SetupSource {
                    canonical_path: base
                        .join(if position == 0 {
                            "first-extra"
                        } else {
                            "second-extra"
                        })
                        .to_str()
                        .unwrap()
                        .into(),
                    aliases: vec![format!("extra-{position}")],
                    owner: false,
                },
            ],
            setup_watch: position == 0,
        };
        let reserved = registry.reserve_index_setup(&request)?;
        // Only the physical native preparation proof is substituted in this
        // metadata unit test. No old publication/storage is created or opened.
        let native = NativeIndexPreparation::from_validated_native_review(
            registration.clone(),
            request.sources.clone(),
            Sha256Digest::from_bytes([107; 32]),
            Sha256Digest::from_bytes([108; 32]),
            SetupPreparationReview {
                expected_catalog_revision: reserved.catalog_revision,
                expected_source_revision: reserved.source_revision,
            },
        )?;
        let prepared = registry.record_index_preparation(
            request.caller,
            &request.key,
            reserved.request_digest,
            &native,
        )?;
        registry.complete_index_setup(
            request.caller,
            &request.key,
            reserved.request_digest,
            &SetupCompletion {
                legacy_adoption: None,
                prepared_digest: prepared.preparation.as_ref().unwrap().digest,
                expected_catalog_revision: prepared.catalog_revision,
                expected_source_revision: prepared.source_revision,
            },
        )?;
        registry.pause(registration.binding, position == 0)?;
        originals.push(registration);
    }
    assert!(
        capture(&control).is_err(),
        "retained-state rebuild must still refuse catalog transfer"
    );
    assert_eq!(preflight_clean_break(&control)?, 2);
    let roster = capture_clean_break(&control)?;
    assert_eq!(roster.indexes.len(), 2);
    for registration in &originals {
        let saved = roster
            .indexes
            .iter()
            .find(|saved| saved.registration.binding == registration.binding)
            .unwrap();
        assert_eq!(&saved.registration, registration);
        assert_eq!(saved.roots.len(), 1);
        assert!(!saved.settings_available);
        assert!(
            !saved.vectors_enabled,
            "unknown prior vector opt-out must not become implicit semantic enablement"
        );
        assert!(saved.incarnation.is_none());
        assert!(
            saved
                .discarded_preferences
                .contains(&"unavailable_saved_preferences".into())
        );
        assert!(!Path::new(&registration.data_root).exists());
    }
    assert_eq!(
        roster.indexes.iter().filter(|saved| saved.paused).count(),
        1
    );
    assert_eq!(roster.indexes.iter().filter(|saved| saved.watch).count(), 1);
    assert_ne!(roster.indexes[0].roots, roster.indexes[1].roots);
    roster.check_clean_break(&control)?;
    assert_eq!(
        fs::read(base.join("source/original.txt"))?,
        b"Original shared source."
    );
    Ok(())
}

/// Every file under `root`, with its SHA-256, for exact preservation checks.
fn tree_digest(root: &Path) -> Result<BTreeMap<std::path::PathBuf, String>> {
    use sha2::Digest as _;
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.insert(
                    path.strip_prefix(root)?.to_path_buf(),
                    format!("{:x}", sha2::Sha256::digest(fs::read(&path)?)),
                );
            }
        }
    }
    Ok(files)
}

#[test]
fn clean_break_refuses_a_predecessor_with_an_unfinished_catalog_upgrade_without_changes()
-> Result<()> {
    use airk_store::service::{ServiceLease, registry::catalog::genesis::CatalogGenesis};
    // S2 decision Q3: a catalog that exists but was never activated (the
    // pending-upgrade state) is refused with a typed code and guidance.
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let control = base.join("control");
    crate::local_ipc::prepare_root(&control)?;
    let mut genesis = CatalogGenesis::begin(
        ServiceLease::acquire(&control)?,
        Sha256Digest::from_bytes([121; 32]),
        Incarnation::from_bytes([122; 16]),
    )?;
    genesis.backup()?;
    genesis.migrate()?;
    drop(genesis);
    let before = tree_digest(&base)?;
    for result in [
        preflight_clean_break(&control).map(|_| ()),
        capture_clean_break(&control).map(|_| ()),
    ] {
        let error = result.expect_err("an unfinished catalog upgrade must refuse the clean break");
        assert!(
            error
                .to_string()
                .starts_with("clean_break_predecessor_catalog_upgrade_pending:"),
            "{error:#}"
        );
        assert!(
            error
                .chain()
                .any(|cause| cause.is::<airk_store::service::registry::catalog::AdmissionPending>()),
            "the original pending-admission cause is retained: {error:#}"
        );
    }
    assert_eq!(
        tree_digest(&base)?,
        before,
        "refusal changes no predecessor file"
    );
    Ok(())
}
