// SPDX-License-Identifier: Apache-2.0
//! Native filesystem helpers only. Actual catalog reservation, receipt and
//! transport qualification remains with the integrated management campaign.
use super::*;
use airk_core::{
    catalog_setup::SetupSource,
    index_catalog::CatalogRevision,
    service::{ProjectRegistration, RequestKey},
};

fn fixture() -> Result<(tempfile::TempDir, PathBuf, IndexSetupRequest)> {
    let temp = tempfile::tempdir()?;
    let base = temp.path().canonicalize()?;
    for name in ["control", "source", "support"] {
        fs::create_dir(base.join(name))?;
    }
    fs::write(base.join("source/original.txt"), b"original source bytes")?;
    let request = IndexSetupRequest {
        caller: Incarnation::from_bytes([1; 16]),
        key: RequestKey {
            namespace: Incarnation::from_bytes([2; 16]),
            epoch: 0,
            nonce: Incarnation::from_bytes([3; 16]),
        },
        expected_catalog_revision: CatalogRevision::INITIAL,
        name: "Explicit index".into(),
        registration: airk_kernel::service_projects::planned_registration(
            &base.join("source"),
            &base.join("data"),
            "fixture",
        )?,
        sources: vec![
            SetupSource {
                canonical_path: base.join("source").to_str().unwrap().into(),
                aliases: vec![],
                owner: true,
            },
            SetupSource {
                canonical_path: base.join("support").to_str().unwrap().into(),
                aliases: vec!["support".into()],
                owner: false,
            },
        ],
        setup_watch: false,
    };
    Ok((temp, base.join("control"), request))
}

#[test]
fn filesystem_review_is_inert_and_distinguishes_storage_actions() -> Result<()> {
    let (_temp, control, request) = fixture()?;
    let data = Path::new(&request.registration.data_root);
    let first = review_filesystem(control.clone(), &request, false)?;
    assert_eq!(
        first.storage_action(),
        SetupStorageAction::CreateMissingLeaf
    );
    let first_digest = first.review_digest()?;
    assert!(!data.exists());
    assert!(fs::read_dir(&control)?.next().is_none());
    assert_eq!(
        fs::read(Path::new(&request.registration.root).join("original.txt"))?,
        b"original source bytes"
    );
    first.parent.create_leaf(data.file_name().unwrap())?;
    drop(first);
    let empty = review_filesystem(control.clone(), &request, false)?;
    assert_eq!(
        empty.storage_action(),
        SetupStorageAction::ClaimExistingEmptyStorage
    );
    assert_ne!(empty.review_digest()?, first_digest);
    drop(empty);
    let owner = data_scope::bind(data, &control, request.key.namespace)?;
    let owned = review_filesystem(control, &request, false)?;
    assert_eq!(
        owned.storage_action(),
        SetupStorageAction::ReuseOwnedStorage
    );
    owner.check()?;
    assert!(!data.join("collections-1.4").exists());
    Ok(())
}

#[test]
fn review_digest_binds_native_encoding_defaults_without_hashing_source_contents() -> Result<()> {
    let (_temp, control, mut request) = fixture()?;
    let first = review_filesystem(control.clone(), &request, false)?;
    let digest = first.review_digest()?;
    assert_eq!(
        first.metadata_plan,
        airk_store::service::registry::catalog::setup::validate_setup_metadata_plan(
            &control, &request
        )?
    );
    let source = Path::new(&request.registration.root).join("original.txt");
    fs::write(&source, b"new saved source contents")?;
    let same_scope = review_filesystem(control.clone(), &request, false)?;
    assert_eq!(same_scope.review_digest()?, digest);
    request.sources[1].aliases = vec!["support_new".into()];
    let changed = review_filesystem(control.clone(), &request, false)?;
    assert_ne!(changed.metadata_plan, first.metadata_plan);
    assert_ne!(changed.review_digest()?, digest);
    assert!(!Path::new(&request.registration.data_root).exists());
    assert!(fs::read_dir(control)?.next().is_none());
    Ok(())
}

#[test]
fn missing_parent_and_occupied_unowned_storage_are_refused_without_creation() -> Result<()> {
    let (_temp, control, mut request) = fixture()?;
    let base = control.parent().unwrap();
    let nested = base.join("missing-parent/data");
    request.registration = airk_kernel::service_projects::planned_registration(
        Path::new(&request.registration.root),
        &nested,
        "fixture",
    )?;
    assert!(review_filesystem(control.clone(), &request, false).is_err());
    assert!(!base.join("missing-parent").exists());
    let occupied = base.join("occupied");
    fs::create_dir(&occupied)?;
    fs::write(occupied.join("user.txt"), b"preserve")?;
    request.registration = airk_kernel::service_projects::planned_registration(
        Path::new(&request.registration.root),
        &occupied,
        "fixture",
    )?;
    assert!(review_filesystem(control.clone(), &request, false).is_err());
    assert!(review_filesystem(control, &request, true).is_err());
    assert_eq!(fs::read(occupied.join("user.txt"))?, b"preserve");
    assert_eq!(fs::read_dir(&occupied)?.count(), 1);
    Ok(())
}

#[test]
fn aliases_use_actual_native_rules_and_private_roots_never_become_sources() -> Result<()> {
    let (_temp, control, mut request) = fixture()?;
    request.sources[1].aliases = vec!["UpperCase".into()];
    request.validate()?; // Catalog syntax is deliberately not native FS proof.
    assert!(review_filesystem(control.clone(), &request, false).is_err());
    request.sources[1].aliases = vec!["support".into()];
    request.sources[1].canonical_path = control.to_str().unwrap().into();
    assert!(review_filesystem(control.clone(), &request, false).is_err());
    assert!(!Path::new(&request.registration.data_root).exists());
    assert!(fs::read_dir(&control)?.next().is_none());
    Ok(())
}

#[test]
fn native_nested_topology_and_reserved_namespace_are_refused_before_storage_effects() -> Result<()>
{
    let (_temp, control, mut request) = fixture()?;
    let source = Path::new(&request.registration.root);
    let nested = source.join("nested");
    fs::create_dir(&nested)?;
    request.sources[1].canonical_path = nested.to_str().unwrap().into();
    request.validate()?; // Pure metadata syntax does not establish native topology.
    assert!(review_filesystem(control.clone(), &request, false).is_err());
    assert!(!Path::new(&request.registration.data_root).exists());
    request.sources[1].canonical_path = control
        .parent()
        .unwrap()
        .join("support")
        .to_str()
        .unwrap()
        .into();
    let reserved = source.join("@support");
    fs::write(&reserved, b"original namespace occupant")?;
    assert!(review_filesystem(control.clone(), &request, false).is_err());
    assert_eq!(fs::read(&reserved)?, b"original namespace occupant");
    assert!(fs::read_dir(control)?.next().is_none());
    assert!(!Path::new(&request.registration.data_root).exists());
    Ok(())
}

#[test]
fn actual_owned_collection_configuration_is_repeatable_and_creates_no_publication() -> Result<()> {
    let (_temp, control, request) = fixture()?;
    let data = Path::new(&request.registration.data_root);
    let review = review_filesystem(control.clone(), &request, false)?;
    review.parent.create_leaf(data.file_name().unwrap())?;
    let bound = data_scope::bind(data, &control, request.key.namespace)?;
    let collection = Collection::initialize(
        data,
        &request.registration.root,
        &request.registration.collection,
        Incarnation::from_bytes([4; 16]),
    )?;
    let selection = collection.select()?;
    let writer = selection.store.lock()?;
    let first = prepare_configuration(&selection, &writer, &request)?;
    let before = fs::read(selection.store.directory().join("roots.json"))?;
    assert_eq!(prepare_configuration(&selection, &writer, &request)?, first);
    assert_eq!(
        fs::read(selection.store.directory().join("roots.json"))?,
        before
    );
    assert!(selection.store.persistent_revision()?.is_none());
    assert_eq!(
        RootConfig::load(selection.store.directory(), &request.registration.root)?.roots,
        BTreeMap::from([("support".into(), request.sources[1].canonical_path.clone())])
    );
    bound.check()?;
    assert_eq!(
        fs::read(Path::new(&request.registration.root).join("original.txt"))?,
        b"original source bytes"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn redirected_parent_or_leaf_is_refused_before_any_foreign_write() -> Result<()> {
    let (_temp, control, request) = fixture()?;
    let base = control.parent().unwrap();
    let parent = base.join("custom-parent");
    let outside = base.join("outside");
    fs::create_dir(&parent)?;
    fs::create_dir(&outside)?;
    let pin = DirectoryPin::open(&parent)?;
    fs::rename(&parent, base.join("original-parent"))?;
    std::os::unix::fs::symlink(&outside, &parent)?;
    assert!(pin.create_leaf(OsStr::new("data")).is_err());
    assert!(!outside.join("data").exists());
    let data = Path::new(&request.registration.data_root);
    std::os::unix::fs::symlink(&outside, data)?;
    assert!(review_filesystem(control, &request, false).is_err());
    assert!(fs::read_dir(&outside)?.next().is_none());
    Ok(())
}

#[cfg(windows)]
#[test]
fn pinned_parent_cannot_be_renamed_until_preparation_releases_its_handle() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let base = directory.path().canonicalize()?;
    let parent = base.join("parent");
    fs::create_dir(&parent)?;
    let pin = DirectoryPin::open(&parent)?;
    assert!(fs::rename(&parent, base.join("renamed")).is_err());
    pin.create_leaf(OsStr::new("one-leaf"))?;
    assert!(parent.join("one-leaf").is_dir());
    drop(pin);
    fs::rename(&parent, base.join("renamed"))?;
    Ok(())
}

#[test]
fn explicit_setup_never_accepts_a_forged_registration_binding() -> Result<()> {
    let (_temp, control, mut request) = fixture()?;
    request.registration = ProjectRegistration::new(
        request.registration.root.clone(),
        request.registration.data_root.clone(),
        "other".into(),
    )?;
    request.registration.binding = Sha256Digest::from_bytes([7; 32]);
    assert!(review_filesystem(control, &request, false).is_err());
    assert!(!Path::new(&request.registration.data_root).exists());
    Ok(())
}
