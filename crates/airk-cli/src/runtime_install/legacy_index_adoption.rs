// SPDX-License-Identifier: Apache-2.0
//! One native maintenance/withdrawal batch and one adoption per physical backing.
use super::*;
use crate::runtime_legacy_origin::{
    self as origin, ConfigLocator, LegacyOriginObservation, NativeLegacyOrigin,
};
use airk_core::{
    catalog_legacy_adoption::{LegacyAdoptionPlan, source_inventory_digest},
    catalog_legacy_authority::StandaloneConfigurationClaim,
    catalog_setup::{IndexSetupRequest, SetupSource, SetupStage, canonical_setup_sources},
    identity::{Incarnation, Sha256Digest, framed_digest},
    service::{ProjectRegistration, RequestKey},
};
use airk_store::{
    legacy_adoption::{
        CaptureLimits, LegacyAdoptionWritePermit, LegacyInventory, LegacyWriterExclusion,
        VerifiedLegacyArchive, capture_tree, copy_tree,
    },
    service::registry::{ServiceRegistry, catalog::setup::NativeLegacyAdoptionCompletion},
};
use std::time::Instant;

const MAX_JOURNAL: u64 = 16 * 1024 * 1024;
const MAX_CLAIMS: usize = 32;
const MAX_REPLY: usize = 16 * 1024 * 1024;

fn checked_reply(value: serde_json::Value) -> Result<serde_json::Value> {
    ensure!(
        // RuntimeCommand prints one final LF after the pretty JSON value.
        serde_json::to_vec_pretty(&value)?.len() < MAX_REPLY,
        "complete native adoption reply exceeds its 16 MiB transport budget"
    );
    Ok(value)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenClaim {
    locator: ConfigLocator,
    original: LegacyOriginObservation,
    group: Incarnation,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupRecord {
    id: Incarnation,
    collection: PathBuf,
    namespace: String,
    physical: Sha256Digest,
    origins: Vec<Sha256Digest>,
    archive: PathBuf,
    inventory: Option<LegacyInventory>,
    plan: Option<LegacyAdoptionPlan>,
    result: Option<serde_json::Value>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    operation: Incarnation,
    selected: String,
    inspection: String,
    locators: Vec<ConfigLocator>,
    claims: Vec<FrozenClaim>,
    groups: Vec<GroupRecord>,
    result: Option<serde_json::Value>,
    completed: bool,
}
struct Captured {
    locator: ConfigLocator,
    proof: Option<NativeLegacyOrigin>,
    scope: origin::StandaloneScope,
    identity: Sha256Digest,
}
struct Group {
    id: Incarnation,
    collection: PathBuf,
    namespace: String,
    physical: Sha256Digest,
    members: Vec<usize>,
    writer: Option<LegacyWriterExclusion>,
}

fn limits() -> CaptureLimits {
    CaptureLimits {
        max_files: 100_000,
        max_bytes: 8 << 30,
        max_file_bytes: 2 << 30,
        max_depth: 64,
        max_snapshot_bytes: 64 << 20,
    }
}
fn text(path: &Path) -> Result<String> {
    Ok(path
        .to_str()
        .context("legacy adoption path is not UTF-8")?
        .to_owned())
}
fn short(domain: &[u8], parts: &[&[u8]]) -> Incarnation {
    let digest = framed_digest(domain, parts);
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    Incarnation::from_bytes(bytes)
}
fn journal_name(op: Incarnation) -> String {
    format!("legacy-index-adoption-batch-{op}.json")
}
fn same<T: Serialize>(left: &T, right: &T) -> Result<bool> {
    Ok(serde_json::to_vec(left)? == serde_json::to_vec(right)?)
}
fn save(root: &Path, journal: &Journal) -> Result<()> {
    let bytes = serde_json::to_vec(journal)?;
    ensure!(
        bytes.len() as u64 <= MAX_JOURNAL,
        "legacy adoption batch journal exceeds bound"
    );
    let path = root.join(journal_name(journal.operation));
    if path.try_exists()? {
        safe_file(&path)?;
    }
    let mut temporary = tempfile::NamedTempFile::new_in(root)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    sync_directory(root)
}
fn read(root: &Path, op: Incarnation) -> Result<Option<Journal>> {
    let path = root.join(journal_name(op));
    if !path.try_exists()? {
        return Ok(None);
    }
    let journal: Journal = serde_json::from_slice(&read_bounded(&path, MAX_JOURNAL)?)?;
    ensure!(
        journal.schema == 2
            && journal.operation == op
            && journal.locators.len() <= MAX_CLAIMS
            && journal.claims.len() <= MAX_CLAIMS
            && journal.groups.len() <= MAX_CLAIMS,
        "invalid legacy adoption batch journal"
    );
    for group in &journal.groups {
        if let Some(plan) = &group.plan {
            plan.validate()?;
        }
    }
    Ok(Some(journal))
}
fn input(path: &Path) -> Result<Vec<u8>> {
    ensure!(
        path.is_absolute(),
        "original locator input must be absolute"
    );
    crate::http_endpoint::read_private(
        path.parent().context("original locator parent missing")?,
        path.file_name()
            .and_then(|v| v.to_str())
            .context("invalid original locator filename")?,
        1024 * 1024,
    )?
    .context("original locator input missing")
}
fn canonical_locators(mut values: Vec<ConfigLocator>) -> Result<Vec<ConfigLocator>> {
    ensure!(
        !values.is_empty() && values.len() <= MAX_CLAIMS,
        "legacy adoption requires 1..32 original locators"
    );
    let mut encoded = values
        .drain(..)
        .map(|value| Ok((serde_json::to_vec(&value)?, value)))
        .collect::<Result<Vec<_>>>()?;
    encoded.sort_by(|a, b| a.0.cmp(&b.0));
    ensure!(
        encoded.windows(2).all(|v| v[0].0 != v[1].0),
        "duplicate original locator in adoption batch"
    );
    Ok(encoded.into_iter().map(|(_, value)| value).collect())
}
fn disjoint(left: &Path, right: &Path) -> Result<()> {
    ensure!(
        airk_store::service::registry::source_relation(left, right)?.is_none(),
        "legacy adoption private storage overlaps original sources, data or native control"
    );
    Ok(())
}
fn private_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            local_ipc::require_existing_root(path)?;
            ensure!(
                path.canonicalize()? == path,
                "redirected legacy adoption private directory"
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().context("private adoption parent missing")?;
            local_ipc::require_existing_root(parent)?;
            ensure!(
                parent.canonicalize()? == parent,
                "redirected private adoption parent"
            );
            fs::create_dir(path)?;
            local_ipc::prepare_root(path)?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
fn private_root(root: &Path, state: &Path) -> Result<PathBuf> {
    let private = state.join("legacy-adoption");
    let owner = serde_json::json!({"schema":1,"install":root,"state":state});
    if private.try_exists()? {
        private_directory(&private)?;
        let saved: serde_json::Value =
            serde_json::from_slice(&read_bounded(&private.join("owner.json"), 4096)?)?;
        ensure!(
            saved == owner,
            "unknown legacy adoption private directory preserved"
        );
    } else {
        private_directory(&private)?;
        replace_json(&private, "owner.json", &owner)?;
    }
    for name in ["archives", "validation"] {
        private_directory(&private.join(name))?;
    }
    Ok(private)
}
fn adopted_data_location(
    root: &Path,
    state: &Path,
    namespace: Incarnation,
) -> Result<(PathBuf, serde_json::Value)> {
    let identity = framed_digest(
        b"airk/adopted-data-root/1",
        &[
            text(root)?.as_bytes(),
            text(state)?.as_bytes(),
            namespace.to_string().as_bytes(),
        ],
    );
    let path = state
        .parent()
        .context("adopted-data parent missing")?
        .join(format!("airk-adopted-indexes-{identity}"));
    let owner = serde_json::json!({"schema":1,"install":root,"state":state,"namespace":namespace,"identity":identity});
    Ok((path, owner))
}
fn adopted_data_root(
    path: &Path,
    owner: &serde_json::Value,
) -> Result<crate::catalog_index_setup::PrivateDirectoryPin> {
    let create = match fs::symlink_metadata(path) {
        Ok(_) => false,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => return Err(error.into()),
    };
    let pin = crate::catalog_index_setup::pin_private_directory(path, create)?;
    if create {
        replace_json(path, "owner.json", owner)?;
    } else {
        let saved: serde_json::Value =
            serde_json::from_slice(&read_bounded(&path.join("owner.json"), 16384)?)?;
        ensure!(saved == *owner, "unknown adopted-data owner preserved");
    }
    pin.check()?;
    Ok(pin)
}
fn startup(control: &Path, proof: &NativeLegacyOrigin) -> Result<serde_json::Value> {
    let scope = proof.standalone_scope()?;
    let target = crate::legacy_startup::Target {
        root: scope.root,
        data_dir: scope.data_dir,
        collection: scope.collection,
        executable: proof.observation().executable.clone(),
    };
    let before = crate::legacy_startup::review_target(control, &target)?;
    let removal = if before.absence_proven() {
        None
    } else {
        Some(crate::legacy_startup::remove_target(
            control,
            &target,
            before
                .approval()
                .context("original scheduler ownership/absence cannot be proven")?,
        )?)
    };
    let after = crate::legacy_startup::review_target(control, &target)?;
    ensure!(
        after.absence_proven(),
        "original scheduler remains registered or running"
    );
    Ok(serde_json::json!({"before":before,"removal":removal,"after":after}))
}
fn claim(proof: &NativeLegacyOrigin) -> Result<StandaloneConfigurationClaim> {
    crate::runtime_legacy_authority::configuration_claim(proof)
}
fn capture_origins(
    locators: &[ConfigLocator],
    withdrawal: &legacy_caches::CacheWithdrawal,
    operation: Incarnation,
    until: Instant,
) -> Result<Vec<Captured>> {
    let mut captured = Vec::new();
    for locator in locators {
        ensure!(
            Instant::now() < until,
            "legacy batch origin deadline exhausted"
        );
        let original = withdrawal.origin(locator)?;
        let proof = origin::resume_withdrawn(locator, &original, &operation.to_string())
            .context("bounded original capability capture failed; no original proof was dropped")?;
        let scope = proof.standalone_scope()?;
        ensure!(
            !scope.no_default_project,
            "unsupported original standalone default-project flags"
        );
        captured.push(Captured {
            locator: locator.clone(),
            identity: proof.identity()?,
            scope,
            proof: Some(proof),
        });
        ensure!(
            Instant::now() < until,
            "legacy batch origin deadline exhausted"
        );
    }
    captured.sort_by_key(|value| value.identity);
    ensure!(
        captured.windows(2).all(|v| v[0].identity != v[1].identity),
        "duplicate original claim identity"
    );
    Ok(captured)
}
fn ready(control: &Path, record: &GroupRecord) -> Result<bool> {
    let Some(plan) = &record.plan else {
        return Ok(false);
    };
    let digest = plan.identity()?;
    Ok(ServiceRegistry::open_saved(control)?
        .index_setup_receipt(plan.setup.caller, &plan.setup.key)?
        .is_some_and(|saved| {
            saved.stage == SetupStage::Ready
                && saved.request == plan.setup
                && saved.legacy_adoption == Some(digest)
        }))
}
fn partition(
    batch: Incarnation,
    captured: &[Captured],
    saved: Option<&Journal>,
    control: &Path,
) -> Result<Vec<Group>> {
    let mut groups: Vec<Group> = Vec::new();
    let mut paths: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for (member, value) in captured.iter().enumerate() {
        let observation = value
            .proof
            .as_ref()
            .context("captured origin missing")?
            .observation();
        let old = saved
            .and_then(|journal| {
                journal
                    .claims
                    .iter()
                    .find(|claim| claim.original == *observation)
            })
            .and_then(|claim| {
                saved
                    .and_then(|journal| journal.groups.iter().find(|group| group.id == claim.group))
            });
        let reused = if let Some(old) = old {
            ready(control, old)?
        } else {
            false
        };
        let original = value.scope.collection_path();
        let collection = if reused {
            old.context("reused group missing")?.collection.clone()
        } else {
            original.canonicalize()?
        };
        if let Some(&index) = paths.get(&collection) {
            ensure!(
                groups[index].namespace == value.scope.namespace,
                "same collection path has crossed legacy namespace"
            );
            groups[index].members.push(member);
            continue;
        }
        let (physical, writer) = if reused {
            (old.context("reused group missing")?.physical, None)
        } else {
            let writer = LegacyWriterExclusion::acquire_existing(&collection)?;
            let physical = writer.collection_identity()?;
            (physical, Some(writer))
        };
        let id = short(
            b"airk/legacy-adoption-physical-group/1",
            &[
                batch.to_string().as_bytes(),
                physical.as_bytes(),
                value.scope.namespace.as_bytes(),
            ],
        );
        if let Some(index) = groups.iter().position(|group| {
            group.physical == physical && group.namespace == value.scope.namespace
        }) {
            ensure!(
                groups[index].collection == collection,
                "aliased native collection backing requires explicit review"
            );
            groups[index].members.push(member);
            continue;
        }
        paths.insert(collection.clone(), groups.len());
        groups.push(Group {
            id,
            collection,
            namespace: value.scope.namespace.clone(),
            physical,
            members: vec![member],
            writer,
        });
    }
    groups.sort_by_key(|group| group.id);
    Ok(groups)
}
struct ValidationCopy {
    _owner: tempfile::TempDir,
    collection: PathBuf,
}
impl ValidationCopy {
    fn path(&self) -> &Path {
        &self.collection
    }
}
fn copy_private(
    archive: &Path,
    parent: &Path,
    inventory: &LegacyInventory,
    collection: &str,
    until: Instant,
    check: &mut dyn FnMut() -> Result<()>,
) -> Result<ValidationCopy> {
    let target = tempfile::Builder::new()
        .prefix("validation-")
        .tempdir_in(parent)?;
    airk_store::validate_collection(collection)?;
    let namespace = target.path().join("collections-1.2");
    private_directory(&namespace)?;
    let working = namespace.join(collection);
    private_directory(&working)?;
    copy_tree(archive, &working, inventory, limits(), until, check)?;
    Ok(ValidationCopy {
        _owner: target,
        collection: working,
    })
}
fn persist_group(root: &Path, journal: &mut Journal, record: &GroupRecord) -> Result<()> {
    let saved = journal
        .groups
        .iter_mut()
        .find(|group| group.id == record.id)
        .context("frozen physical group disappeared")?;
    *saved = record.clone();
    save(root, journal)
}

struct PreparedGroup {
    record: GroupRecord,
    inventory: LegacyInventory,
    validation_parent: PathBuf,
    scope: airk_store::legacy_adoption::LegacyScope,
    original_publication: Sha256Digest,
    until: Instant,
}
fn prepare_group(
    maintenance: &mut legacy_maintenance::Maintenance,
    withdrawal: &mut legacy_caches::CacheWithdrawal,
    journal: &mut Journal,
    group: &Group,
    captured: &mut [Captured],
    private: &Path,
    until: Instant,
) -> Result<PreparedGroup> {
    let root = maintenance.root().to_owned();
    let control = maintenance.control().to_owned();
    let mut record = journal
        .groups
        .iter()
        .find(|saved| saved.id == group.id)
        .context("frozen native group missing")?
        .clone();
    ensure!(
        record.collection == group.collection
            && record.namespace == group.namespace
            && record.physical == group.physical
            && record.archive == private.join("archives").join(group.id.to_string())
            && record.origins
                == group
                    .members
                    .iter()
                    .map(|&index| captured[index].identity)
                    .collect::<Vec<_>>(),
        "saved native physical group paths or original roster changed"
    );
    let primary = group.members[0];
    let original_scope = captured[primary].scope.clone();
    let native_ready = ready(&control, &record)?;
    ensure!(
        record.result.is_none() || native_ready,
        "saved group outcome lacks actual native Ready receipt"
    );
    let before = if native_ready {
        record
            .inventory
            .clone()
            .context("completed group archive inventory missing")?
    } else {
        capture_tree(&group.collection, limits(), until)?
    };
    if let Some(inventory) = &record.inventory {
        ensure!(
            *inventory == before,
            "original group publication changed after admission"
        );
    } else {
        let (total_bytes, total_entries) = journal
            .groups
            .iter()
            .filter(|other| other.id != group.id)
            .filter_map(|other| other.inventory.as_ref())
            .try_fold(
                (before.bytes, before.files.len() + before.directories.len()),
                |(bytes, entries), inventory| -> Result<_> {
                    Ok((
                        bytes
                            .checked_add(inventory.bytes)
                            .context("legacy batch byte overflow")?,
                        entries
                            .checked_add(inventory.files.len() + inventory.directories.len())
                            .context("legacy batch entry overflow")?,
                    ))
                },
            )?;
        ensure!(
            total_bytes <= limits().max_bytes && total_entries <= limits().max_files,
            "legacy batch aggregate archive budget exceeded"
        );
        record.inventory = Some(before.clone());
        persist_group(&root, journal, &record)?;
    }
    private_directory(&record.archive)?;
    let validation_parent = private.join("validation").join(group.id.to_string());
    private_directory(&validation_parent)?;
    let validation = {
        let mut check = || -> Result<()> {
            ensure!(
                Instant::now() < until,
                "legacy batch capture deadline exhausted"
            );
            withdrawal.revalidate_identity()?;
            for &index in &group.members {
                captured[index]
                    .proof
                    .as_ref()
                    .context("original group proof disappeared")?
                    .revalidate_identity()?;
            }
            if let Some(writer) = &group.writer {
                writer.revalidate()?;
            }
            Ok(())
        };
        if !native_ready {
            copy_tree(
                &group.collection,
                &record.archive,
                &before,
                limits(),
                until,
                &mut check,
            )?;
            check()?;
            ensure!(
                capture_tree(&group.collection, limits(), until)? == before,
                "original saved group changed during archive capture"
            );
        }
        ensure!(
            capture_tree(&record.archive, limits(), until)? == before,
            "immutable group archive differs"
        );
        copy_private(
            &record.archive,
            &validation_parent,
            &before,
            &original_scope.collection,
            until,
            &mut check,
        )?
    };
    let archive = VerifiedLegacyArchive::open_discovered_validation_copy(
        validation.path(),
        &before,
        limits(),
        until,
    )
    .context("discover saved scope from sealed archive copy")?;
    let scope = archive.scope().clone();
    let original_publication = archive.identity().original_root_sha256;
    drop(archive); // Keep at most one bounded snapshot projection in memory.
    drop(validation); // Canonical SQLite readers may create scratch WAL/SHM.
    Ok(PreparedGroup {
        record,
        inventory: before,
        validation_parent,
        scope,
        original_publication,
        until,
    })
}
fn run_group(
    maintenance: &mut legacy_maintenance::Maintenance,
    withdrawal: &mut legacy_caches::CacheWithdrawal,
    journal: &mut Journal,
    group: &Group,
    captured: &mut [Captured],
    indexes: &Path,
    prepared: PreparedGroup,
) -> Result<serde_json::Value> {
    let root = maintenance.root().to_owned();
    let control = maintenance.control().to_owned();
    let PreparedGroup {
        mut record,
        inventory: before,
        validation_parent,
        scope,
        original_publication,
        until,
    } = prepared;
    let primary = group.members[0];
    let original_scope = captured[primary].scope.clone();
    // Each top-level read starts from the sealed preimage; no WAL/SHM
    // created by discovery is relabeled as original archived evidence.
    let validation = copy_private(
        &record.archive,
        &validation_parent,
        &before,
        &original_scope.collection,
        until,
        &mut || {
            ensure!(
                Instant::now() < until,
                "legacy batch capture deadline exhausted"
            );
            withdrawal.revalidate_identity()?;
            for &index in &group.members {
                captured[index]
                    .proof
                    .as_ref()
                    .context("original group proof disappeared")?
                    .revalidate_identity()?;
            }
            if let Some(writer) = &group.writer {
                writer.revalidate()?;
            }
            Ok(())
        },
    )
    .context("copy sealed archive for native conversion")?;
    let archive = VerifiedLegacyArchive::open_validation_copy(
        validation.path(),
        scope,
        &before,
        limits(),
        until,
    )
    .context("open fresh sealed archive copy for conversion")?;
    ensure!(
        archive.identity().original_root_sha256 == original_publication,
        "prepared archive publication changed"
    );
    let labels = archive.scope();
    let source_digest = source_inventory_digest(&labels.root, &labels.collection, &labels.roots)?;
    let operation = short(
        b"airk/legacy-adoption-publication-group/1",
        &[
            journal.operation.to_string().as_bytes(),
            group.physical.as_bytes(),
            group.namespace.as_bytes(),
            archive.identity().original_root_sha256.as_bytes(),
            source_digest.as_bytes(),
        ],
    );
    for source in std::iter::once(&labels.root).chain(labels.roots.values()) {
        disjoint(indexes, Path::new(source))?;
    }
    let destination = indexes.join(operation.to_string());
    for &index in &group.members {
        disjoint(&destination, &captured[index].scope.root)?;
        disjoint(&destination, &captured[index].scope.data_dir)?;
    }
    disjoint(&destination, &control)?;
    let registry = ServiceRegistry::open_saved(&control)?;
    let plan = if let Some(plan) = &record.plan {
        ensure!(
            plan.setup.key.nonce == operation,
            "saved group operation differs from actual publication"
        );
        plan.clone()
    } else {
        ensure!(
            !destination.try_exists()?,
            "unknown native group destination preserved"
        );
        let state = registry
            .catalog_state()?
            .context("native adoption catalog absent")?;
        let (namespace, epoch) = registry.saved_request_epoch()?;
        ensure!(
            state.activated && state.namespace == namespace,
            "native group catalog changed"
        );
        let proof = captured[primary]
            .proof
            .as_ref()
            .context("primary original proof missing")?;
        let original_registration = ProjectRegistration::new(
            labels.root.clone(),
            text(&original_scope.data_dir)?,
            labels.collection.clone(),
        )?;
        let registration = ProjectRegistration::new(
            labels.root.clone(),
            text(&destination)?,
            labels.collection.clone(),
        )?;
        let sources = canonical_setup_sources(
            &std::iter::once(SetupSource {
                canonical_path: labels.root.clone(),
                aliases: vec![],
                owner: true,
            })
            .chain(labels.roots.iter().map(|(alias, path)| SetupSource {
                canonical_path: path.clone(),
                aliases: vec![alias.clone()],
                owner: false,
            }))
            .collect::<Vec<_>>(),
        )?;
        let plan = LegacyAdoptionPlan {
            schema: 1,
            setup: IndexSetupRequest {
                caller: local_ipc::caller_identity()?,
                key: RequestKey {
                    namespace,
                    epoch,
                    nonce: operation,
                },
                expected_catalog_revision: state.revision,
                name: format!("Imported {}", labels.collection),
                registration,
                sources,
                setup_watch: false,
            },
            original_registration,
            origin_sha256: proof.identity()?,
            config_sha256: proof.observation().config_sha256.parse()?,
            executable_sha256: proof.observation().executable_sha256.parse()?,
            quiescence_sha256: maintenance.quiescence_sha256()?,
            archive_inventory_sha256: archive.identity().archive_inventory_sha256,
            original_publication_sha256: archive.identity().original_root_sha256,
            source_inventory_sha256: source_digest,
        };
        plan.validate()?;
        record.plan = Some(plan.clone());
        persist_group(&root, journal, &record)?;
        plan
    };
    ensure!(
        plan.setup.registration.data_root == text(&destination)?
            && plan.original_publication_sha256 == archive.identity().original_root_sha256
            && plan.archive_inventory_sha256 == archive.identity().archive_inventory_sha256
            && plan.source_inventory_sha256 == source_digest,
        "saved native group plan differs from its actual archive or destination"
    );
    withdrawal.revalidate()?;
    for &index in &group.members {
        let proof = captured[index]
            .proof
            .as_mut()
            .context("original proof missing")?;
        crate::catalog_index_setup::validate_legacy_adoption_origin_scope(&plan, proof)?;
    }
    crate::catalog_index_setup::validate_legacy_adoption_origin(
        &plan,
        captured[primary]
            .proof
            .as_mut()
            .context("primary proof missing")?,
    )?;
    maintenance.bind_adoption_plan(&plan)?;
    drop(registry);
    let (receipt, converted, grant, claims) =
        maintenance.with_adoption_registry(&plan, |registry| {
            let previous = registry.index_setup_receipt(plan.setup.caller, &plan.setup.key)?;
            let prepared = if let Some(previous) =
                previous.filter(|receipt| receipt.stage == SetupStage::Ready)
            {
                ensure!(
                    previous.request == plan.setup
                        && previous.legacy_adoption == Some(plan.identity()?),
                    "native group receipt changed"
                );
                previous
            } else {
                let review = crate::catalog_index_setup::review_legacy_adoption(
                    &control,
                    &plan,
                    &archive,
                    captured[primary]
                        .proof
                        .as_mut()
                        .context("primary proof missing")?,
                )?;
                registry.reserve_legacy_adoption(&plan)?;
                crate::catalog_index_setup::prepare(registry, &control, review)?
                    .record_legacy_adoption(registry, &plan)?
            };
            let permit = LegacyAdoptionWritePermit::from_prepared(&control, &prepared, &plan)?;
            let kernel = airk_kernel::Kernel::open_service_collection_for_legacy_adoption(
                &permit, &archive,
            )?;
            let converted = kernel.adopt_saved_legacy(
                &archive,
                &airk_kernel::legacy_adoption::PreparationLimits::default(),
                &permit,
            )?;
            withdrawal.revalidate()?;
            if let Some(writer) = &group.writer {
                writer.revalidate()?;
                ensure!(
                    capture_tree(&group.collection, limits(), until)? == before,
                    "original group changed before completion"
                );
            }
            let native = NativeLegacyAdoptionCompletion::from_verified_publication(
                &plan,
                kernel.verify_legacy_adoption_publication()?,
            )?;
            let receipt = registry.complete_legacy_adoption(&native)?;
            let claims = group
                .members
                .iter()
                .map(|&index| {
                    claim(
                        captured[index]
                            .proof
                            .as_ref()
                            .context("original claim missing")?,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let proofs = group
                .members
                .iter()
                .map(|&index| {
                    captured[index]
                        .proof
                        .take()
                        .context("original capability already consumed")
                })
                .collect::<Result<Vec<_>>>()?;
            let mut authority =
        crate::runtime_legacy_authority::RetainedStandaloneAuthority::from_completed_claims(
            proofs,
            native,
            receipt.clone(),
        )?;
            let grant = authority.import(registry)?;
            // Every original capability remains held through the checked native CAS.
            drop(authority.into_origins());
            drop(kernel);
            drop(permit);
            Ok((receipt, converted, grant, claims))
        })?;
    let mappings=group.members.iter().zip(&claims).map(|(&index,claim)|serde_json::json!({"locator":captured[index].locator,
        "origin_sha256":claim.origin_sha256,"config_sha256":claim.config_sha256,"group_operation_id":operation,
        "original_registration":plan.original_registration,"registration":plan.setup.registration,"claim":claim})).collect::<Vec<_>>();
    let result = serde_json::json!({"schema":1,"active":journal.selected,"status":"ready","operation_id":operation,"batch_operation_id":journal.operation,
        "original_registration":plan.original_registration,"registration":plan.setup.registration,"original_config_sha256":plan.config_sha256,
        "source_availability":"not_observed_saved_only","adoption_receipt":receipt,"conversion":converted,"grant":grant,
        "archive":record.archive,"archive_inventory_sha256":before.identity()?,"claims":mappings});
    record.result = Some(result.clone());
    persist_group(&root, journal, &record)?;
    Ok(result)
}

fn run(
    maintenance: &mut legacy_maintenance::Maintenance,
    expected: &str,
    operation: Incarnation,
    locators: Vec<ConfigLocator>,
    inspection: &str,
) -> Result<serde_json::Value> {
    let until = Instant::now() + Duration::from_secs(1800);
    let root = maintenance.root().to_owned();
    let control = maintenance.control().to_owned();
    ensure!(
        selection_record(&root)?.active == expected,
        "legacy adoption selected runtime changed"
    );
    let mut withdrawal =
        legacy_caches::withhold_locked(&root, expected, inspection, &operation.to_string())?;
    let mut captured = capture_origins(&locators, &withdrawal, operation, until)?;
    let mut startup_observations = Vec::new();
    for value in &captured {
        startup_observations.push(startup(
            &control,
            value.proof.as_ref().context("captured origin missing")?,
        )?);
    }
    withdrawal.revalidate()?;
    let old = read(&root, operation)?;
    let groups = partition(operation, &captured, old.as_ref(), &control)?;
    let (state, _) = layout::retained_parameters(&root)?;
    let private = state.join("legacy-adoption");
    for value in &captured {
        disjoint(&private, &value.scope.root)?;
        disjoint(&private, &value.scope.data_dir)?;
    }
    disjoint(&private, &control)?;
    let namespace = ServiceRegistry::open_saved(&control)?.incarnation()?;
    let (indexes, indexes_owner) = adopted_data_location(&root, &state, namespace)?;
    for protected in layout::retained_private_paths(&root)? {
        disjoint(&indexes, &protected)?;
    }
    for value in &captured {
        disjoint(&indexes, &value.scope.root)?;
        disjoint(&indexes, &value.scope.data_dir)?;
        for additional in &value
            .proof
            .as_ref()
            .context("original proof missing")?
            .observation()
            .declaration
            .additional_roots
        {
            disjoint(&indexes, &additional.path)?;
        }
    }
    // Frozen predecessor plans are evidence, never permission to relocate a
    // partially admitted index silently into a different ownership domain.
    if let Some(saved) = &old {
        for group in &saved.groups {
            if let Some(plan) = &group.plan {
                ensure!(
                    Path::new(&plan.setup.registration.data_root).parent()
                        == Some(indexes.as_path()),
                    "saved adoption destination requires explicit recovery; original plan preserved"
                );
            }
        }
    }
    let private = private_root(&root, &state)?;
    let mut frozen = Vec::new();
    for group in &groups {
        for &index in &group.members {
            frozen.push(FrozenClaim {
                locator: captured[index].locator.clone(),
                original: captured[index]
                    .proof
                    .as_ref()
                    .context("captured origin missing")?
                    .observation()
                    .clone(),
                group: group.id,
            });
        }
    }
    let mut journal = if let Some(saved) = old {
        ensure!(
            saved.selected == expected
                && saved.inspection == inspection
                && same(&saved.locators, &locators)?
                && same(&saved.claims, &frozen)?,
            "legacy batch claim roster changed"
        );
        saved
    } else {
        let records = groups
            .iter()
            .map(|group| GroupRecord {
                id: group.id,
                collection: group.collection.clone(),
                namespace: group.namespace.clone(),
                physical: group.physical,
                origins: group
                    .members
                    .iter()
                    .map(|&index| captured[index].identity)
                    .collect(),
                archive: private.join("archives").join(group.id.to_string()),
                inventory: None,
                plan: None,
                result: None,
            })
            .collect::<Vec<_>>();
        for record in &records {
            ensure!(
                !record.archive.try_exists()?,
                "unknown batch archive operation path preserved"
            );
        }
        let saved = Journal {
            schema: 2,
            operation,
            selected: expected.into(),
            inspection: inspection.into(),
            locators,
            claims: frozen,
            groups: records,
            result: None,
            completed: false,
        };
        save(&root, &saved)?;
        saved
    };
    // Capture once into immutable archives and validate the complete saved
    // source union before creating derived storage. Discovery scratch is
    // discarded; conversion reads a fresh verified copy one group at a time.
    let mut prepared_groups = Vec::new();
    let mut scope_bytes = 0u64;
    let registry = ServiceRegistry::open_saved(&control)?;
    for group in &groups {
        let prepared = prepare_group(
            maintenance,
            &mut withdrawal,
            &mut journal,
            group,
            &mut captured,
            &private,
            until,
        )?;
        let scope = &prepared.scope;
        scope_bytes = scope_bytes
            .checked_add(serde_json::to_vec(scope)?.len() as u64)
            .context("batch saved scope size overflow")?;
        ensure!(
            scope_bytes <= MAX_JOURNAL,
            "batch saved scope metadata exceeds bound"
        );
        let proposed = ProjectRegistration::new(
            scope.root.clone(),
            text(&indexes)?,
            scope.collection.clone(),
        )?;
        let additional = scope.roots.values().cloned().collect::<Vec<_>>();
        registry.preflight_source_isolation(&proposed, &additional)?;
        for source in std::iter::once(&scope.root).chain(scope.roots.values()) {
            disjoint(&indexes, Path::new(source))?;
        }
        prepared_groups.push(prepared);
    }
    drop(registry);
    let indexes_pin = adopted_data_root(&indexes, &indexes_owner)?;
    let mut results = Vec::new();
    for (group, prepared) in groups.iter().zip(prepared_groups) {
        results.push(run_group(
            maintenance,
            &mut withdrawal,
            &mut journal,
            group,
            &mut captured,
            &indexes,
            prepared,
        )?);
    }
    indexes_pin.check()?;
    ensure!(
        captured.iter().all(|value| value.proof.is_none()),
        "unconsumed original claims in native batch"
    );
    for group in &groups {
        if let Some(writer) = &group.writer {
            writer.revalidate()?;
        }
    }
    withdrawal.revalidate()?;
    let held = withdrawal.observation()?;
    let mappings = results
        .iter()
        .flat_map(|group| group["claims"].as_array().into_iter().flatten().cloned())
        .collect::<Vec<_>>();
    let mut result = serde_json::json!({"schema":1,"active":expected,"status":"ready","operation_id":operation,"batch_operation_id":operation,
        "cache_inspection_sha256":inspection,"retained_completed_operation":false,
        "groups":results,"claims":mappings,"maintenance":null,"withdrawal":{"held":held,"restored":null},"original_startup":startup_observations});
    checked_reply(result.clone())?;
    journal.result = Some(result.clone());
    save(&root, &journal)?;
    result["withdrawal"]["restored"] = legacy_caches::restore_locked(withdrawal)?;
    journal.result = Some(result.clone());
    save(&root, &journal)?;
    Ok(result)
}
async fn adopt_locators(
    root: &Path,
    expected: &str,
    operation: &str,
    values: Vec<ConfigLocator>,
    inspection: &str,
) -> Result<serde_json::Value> {
    let operation: Incarnation = operation.parse()?;
    digest(expected)?;
    digest(inspection)?;
    let root = root.canonicalize()?;
    let locators = canonical_locators(values)?;
    {
        let _lock = existing_install_lock(&root)?;
        ensure!(
            selection_record(&root)?.active == expected,
            "legacy adoption selected runtime changed"
        );
        if let Some(saved) = read(&root, operation)? {
            ensure!(
                saved.selected == expected
                    && saved.inspection == inspection
                    && same(&saved.locators, &locators)?,
                "legacy batch request conflicts with saved operation"
            );
            if saved.completed {
                let mut result = saved
                    .result
                    .context("completed native batch result missing")?;
                result["retained_completed_operation"] = true.into();
                return checked_reply(result);
            }
        }
    }
    let mut maintenance = legacy_maintenance::enter(&root, &operation.to_string()).await?;
    let primary = run(&mut maintenance, expected, operation, locators, inspection);
    let mut result = maintenance.finish(primary).await?;
    let restored = legacy_maintenance::result(&root, &operation.to_string())?
        .context("native batch maintenance restoration receipt missing")?;
    let _lock = existing_install_lock(&root)?;
    let mut saved = read(&root, operation)?.context("completed batch journal disappeared")?;
    ensure!(
        saved.selected == expected
            && saved.inspection == inspection
            && saved.result.as_ref() == Some(&result),
        "native batch result changed"
    );
    result["maintenance"] = serde_json::to_value(restored)?;
    checked_reply(result.clone())?;
    saved.result = Some(result.clone());
    saved.completed = true;
    save(&root, &saved)?;
    Ok(result)
}
pub(super) async fn adopt_many(
    root: &Path,
    expected: &str,
    operation: &str,
    locators_file: &Path,
    inspection: &str,
) -> Result<serde_json::Value> {
    let values = serde_json::from_slice(&input(locators_file)?)
        .map_err(|_| anyhow::anyhow!("invalid original locator batch metadata"))?;
    adopt_locators(root, expected, operation, values, inspection).await
}
pub(super) async fn adopt(
    root: &Path,
    expected: &str,
    operation: &str,
    locator_file: &Path,
    inspection: &str,
) -> Result<serde_json::Value> {
    let value = serde_json::from_slice(&input(locator_file)?)
        .map_err(|_| anyhow::anyhow!("invalid original locator metadata"))?;
    let result = adopt_locators(root, expected, operation, vec![value], inspection).await?;
    let mut groups = result["groups"]
        .as_array()
        .context("single native adoption groups missing")?
        .clone();
    ensure!(
        groups.len() == 1,
        "single native adoption returned multiple physical groups"
    );
    let mut group = groups.remove(0);
    group["maintenance"] = result["maintenance"].clone();
    group["withdrawal"] = result["withdrawal"].clone();
    group["retained_completed_operation"] = result["retained_completed_operation"].clone();
    group["cache_inspection_sha256"] = result["cache_inspection_sha256"].clone();
    checked_reply(group)
}

#[cfg(test)]
mod tests;
