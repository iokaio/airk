// SPDX-License-Identifier: Apache-2.0
//! Saved legacy bytes, never live source extraction or administrative authority.
//! The native installer must retain its origin/exclusion capability while copying.
use crate::service::registry::catalog::setup::storage::{
    SetupDirectory, SetupFileMode, file_identity,
};
use crate::{ExpectedHead, FileStore, PersistentRevision};
use airk_core::{
    Artifact, Snapshot,
    extraction::{ExtractionPayload, ExtractionProfile},
    identity::Sha256Digest,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};
const PARTS: &str = ".airk-legacy-copy-parts";

mod current_archive;
mod historical_artifact;
mod historical_catalog;
pub use current_archive::OriginalCurrentPublication;
pub use historical_catalog::OriginalCatalogPublication;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyScope {
    pub root: String,
    pub collection: String,
    pub roots: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyPublicationIdentity {
    pub header: Option<PersistentRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<OriginalCurrentPublication>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<OriginalCatalogPublication>,
    pub generation: u64,
    pub original_root_sha256: Sha256Digest,
    pub archive_inventory_sha256: Sha256Digest,
}

#[derive(Clone, Copy, Debug)]
pub struct CaptureLimits {
    pub max_files: usize,
    pub max_bytes: u64,
    pub max_file_bytes: u64,
    pub max_depth: usize,
    pub max_snapshot_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedFile {
    pub bytes: u64,
    pub sha256: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyInventory {
    pub files: BTreeMap<String, SavedFile>,
    pub directories: BTreeSet<String>,
    pub bytes: u64,
}
impl LegacyInventory {
    pub fn identity(&self) -> Result<Sha256Digest> {
        Ok(Sha256Digest::from_bytes(
            Sha256::digest(serde_json::to_vec(self)?).into(),
        ))
    }
}

struct PinnedRoot {
    path: PathBuf,
    directories: Vec<SetupDirectory>,
}
impl PinnedRoot {
    fn open(path: &Path) -> Result<Self> {
        ensure!(
            path.is_absolute()
                && path.components().all(|part| !matches!(
                    part,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )),
            "legacy tree root must be absolute and normalized"
        );
        let mut chain = path.ancestors().collect::<Vec<_>>();
        chain.reverse();
        let anchor = chain
            .first()
            .context("legacy root ancestor missing")?
            .canonicalize()?;
        let mut directories = vec![SetupDirectory::open_canonical(&anchor)?];
        for child in chain.iter().skip(1) {
            let name = child
                .file_name()
                .context("legacy ancestor component missing")?;
            let next = directories
                .last()
                .context("legacy ancestor pin missing")?
                .open_dir(name)?;
            directories.push(next);
        }
        Ok(Self {
            path: path.to_owned(),
            directories,
        })
    }
    fn root(&self) -> &SetupDirectory {
        self.directories.last().expect("root pin exists")
    }
    fn revalidate(&self) -> Result<()> {
        let current = Self::open(&self.path)?;
        ensure!(
            current.directories.len() == self.directories.len(),
            "legacy ancestor count changed"
        );
        for (expected, actual) in self.directories.iter().zip(&current.directories) {
            ensure!(
                expected.identity()? == actual.identity()?,
                "legacy tree ancestor identity changed"
            );
        }
        Ok(())
    }
    fn parent(&self, name: &str) -> Result<(SetupDirectory, std::ffi::OsString)> {
        let path = Path::new(name);
        ensure!(
            path.components()
                .all(|part| matches!(part, std::path::Component::Normal(_))),
            "invalid legacy relative entry"
        );
        let mut directory = self.root().try_clone()?;
        for component in path
            .parent()
            .context("legacy relative parent missing")?
            .components()
        {
            directory = directory.open_dir(component.as_os_str())?;
        }
        Ok((
            directory,
            path.file_name()
                .context("legacy relative filename missing")?
                .to_owned(),
        ))
    }
    fn file(&self, name: &str) -> Result<File> {
        let (parent, name) = self.parent(name)?;
        parent.open_file(&name, SetupFileMode::Read)
    }
}
fn hash_owned(mut file: File, bound: u64, until: Instant) -> Result<SavedFile> {
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() <= bound,
        "invalid or oversized legacy file handle"
    );
    let identity = file_identity(&file)?;
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        ensure!(Instant::now() < until, "legacy capture deadline exhausted");
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        count = count
            .checked_add(read as u64)
            .context("legacy capture overflow")?;
        ensure!(
            count <= bound && count <= before.len(),
            "legacy file grew during capture"
        );
        hash.update(&buffer[..read]);
    }
    ensure!(
        count == before.len()
            && file.metadata()?.len() == count
            && file_identity(&file)? == identity,
        "legacy file handle changed during capture"
    );
    Ok(SavedFile {
        bytes: count,
        sha256: Sha256Digest::from_bytes(hash.finalize().into()),
    })
}
/// Descriptor-relative reads never follow a replaced ancestor/leaf into another
/// tree. The installer separately owns predecessor process/scheduler exclusion.
pub fn capture_tree(root: &Path, limits: CaptureLimits, until: Instant) -> Result<LegacyInventory> {
    let pins = PinnedRoot::open(root)?;
    let mut inventory = LegacyInventory {
        files: BTreeMap::new(),
        directories: BTreeSet::new(),
        bytes: 0,
    };
    let mut stack = vec![(pins.root().try_clone()?, String::new(), 0usize)];
    while let Some((directory, prefix, depth)) = stack.pop() {
        ensure!(
            Instant::now() < until && depth <= limits.max_depth,
            "legacy capture deadline/depth exhausted"
        );
        for entry in directory.entries()? {
            let entry = entry?;
            let component = entry.to_str().context("non-UTF8 legacy entry")?;
            ensure!(
                !component.contains(['/', '\\', ':', '\0'])
                    && !matches!(component, "" | "." | ".."),
                "invalid legacy entry name"
            );
            let name = if prefix.is_empty() {
                component.to_owned()
            } else {
                format!("{prefix}/{component}")
            };
            ensure!(
                name.len() <= 32768
                    && inventory.files.len() + inventory.directories.len() < limits.max_files,
                "legacy capture entry limit exceeded"
            );
            if let Ok(child) = directory.open_dir(&entry) {
                inventory.directories.insert(name.clone());
                stack.push((child, name, depth + 1));
            } else {
                let opened = directory.open_file(&entry, SetupFileMode::Read)?;
                let identity = file_identity(&opened)?;
                let record = hash_owned(opened, limits.max_file_bytes, until)?;
                ensure!(
                    file_identity(&directory.open_file(&entry, SetupFileMode::Read)?)? == identity,
                    "legacy leaf identity changed during capture"
                );
                inventory.bytes = inventory
                    .bytes
                    .checked_add(record.bytes)
                    .context("legacy capture overflow")?;
                ensure!(
                    inventory.bytes <= limits.max_bytes,
                    "legacy capture byte budget exceeded"
                );
                inventory.files.insert(name, record);
            }
        }
    }
    pins.revalidate()?;
    Ok(inventory)
}
/// Exact partials are resumed only under the original native exclusion. All
/// writes/publications use retained directory descriptors, not checked paths.
pub fn copy_tree(
    source: &Path,
    destination: &Path,
    expected: &LegacyInventory,
    limits: CaptureLimits,
    until: Instant,
    check_exclusion: &mut dyn FnMut() -> Result<()>,
) -> Result<()> {
    check_exclusion()?;
    let source_pins = PinnedRoot::open(source)?;
    let target_pins = PinnedRoot::open(destination)?;
    let source_root = source.canonicalize()?;
    let target_root = destination.canonicalize()?;
    ensure!(
        !target_root.starts_with(&source_root) && !source_root.starts_with(&target_root),
        "legacy archive overlaps original"
    );
    ensure!(
        capture_tree(source, limits, until)? == *expected,
        "legacy original preimage changed"
    );
    ensure!(
        !expected
            .directories
            .iter()
            .any(|name| name == PARTS || name.starts_with(&format!("{PARTS}/")))
            && !expected.files.contains_key(PARTS),
        "reserved legacy copy-part namespace"
    );
    let copy_limits = CaptureLimits {
        max_files: limits
            .max_files
            .checked_add(2)
            .context("copy count overflow")?,
        max_bytes: limits
            .max_bytes
            .checked_add(limits.max_file_bytes)
            .context("copy byte overflow")?,
        ..limits
    };
    let mut existing = capture_tree(destination, copy_limits, until)?;
    let mut partials = Vec::new();
    for (name, part) in &existing.files {
        if let Some(hash) = name.strip_prefix(&format!("{PARTS}/")) {
            ensure!(!hash.contains('/'), "foreign nested legacy copy part");
            let (original, record) = expected
                .files
                .iter()
                .find(|(_, record)| record.sha256.to_string() == hash)
                .context("foreign legacy copy part preserved")?;
            ensure!(
                part.bytes <= record.bytes,
                "oversized legacy copy part preserved"
            );
            let mut old = source_pins.file(original)?;
            let mut saved = target_pins.file(name)?;
            let mut left = part.bytes;
            let mut a = [0u8; 65536];
            let mut b = [0u8; 65536];
            while left > 0 {
                ensure!(Instant::now() < until, "legacy partial deadline exhausted");
                let count = left.min(a.len() as u64) as usize;
                old.read_exact(&mut a[..count])?;
                saved.read_exact(&mut b[..count])?;
                ensure!(
                    a[..count] == b[..count],
                    "changed legacy copy part preserved"
                );
                left -= count as u64;
            }
            partials.push((name.clone(), saved));
        }
    }
    ensure!(
        partials.len() <= 1,
        "multiple unfinished legacy parts preserved"
    );
    for (name, _) in &partials {
        existing.files.remove(name);
    }
    ensure!(
        existing
            .directories
            .iter()
            .all(|name| name == PARTS || !name.starts_with(&format!("{PARTS}/"))),
        "foreign copy-part directory preserved"
    );
    existing.directories.remove(PARTS);
    ensure!(
        existing.directories.is_subset(&expected.directories)
            && existing
                .files
                .iter()
                .all(|(name, record)| expected.files.get(name) == Some(record)),
        "foreign partial legacy copy preserved"
    );
    for (name, owned) in partials {
        check_exclusion()?;
        target_pins.revalidate()?;
        let (parent, leaf) = target_pins.parent(&name)?;
        parent.unlink_file_if_identity(&leaf, &owned)?;
    }
    let parts = if let Ok(parts) = target_pins.root().open_dir(OsStr::new(PARTS)) {
        parts
    } else {
        target_pins.root().create_dir(OsStr::new(PARTS))?
    };
    for directory in &expected.directories {
        check_exclusion()?;
        let (parent, name) = target_pins.parent(directory)?;
        if parent.open_dir(&name).is_err() {
            parent.create_dir(&name)?;
        }
    }
    for (name, record) in &expected.files {
        check_exclusion()?;
        source_pins.revalidate()?;
        target_pins.revalidate()?;
        let (parent, leaf) = target_pins.parent(name)?;
        if let Ok(file) = parent.open_file(&leaf, SetupFileMode::Read) {
            ensure!(
                hash_owned(file, record.bytes, until)? == *record,
                "changed completed legacy copy"
            );
            continue;
        }
        let mut input = source_pins.file(name)?;
        let part_name = record.sha256.to_string();
        let mut output = parts.open_file(OsStr::new(&part_name), SetupFileMode::CreateNew)?;
        let mut hash = Sha256::new();
        let mut copied = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            ensure!(Instant::now() < until, "legacy copy deadline exhausted");
            let read = input.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            copied = copied
                .checked_add(read as u64)
                .context("legacy copy overflow")?;
            ensure!(copied <= record.bytes, "legacy copy source grew");
            output.write_all(&buffer[..read])?;
            hash.update(&buffer[..read]);
        }
        ensure!(
            copied == record.bytes
                && Sha256Digest::from_bytes(hash.finalize().into()) == record.sha256,
            "legacy copy source changed"
        );
        output.sync_all()?;
        parts.publish_no_replace(OsStr::new(&part_name), &output, &parent, &leaf)?;
    }
    check_exclusion()?;
    source_pins.revalidate()?;
    target_pins.revalidate()?;
    target_pins
        .root()
        .unlink_empty_dir_if_identity(OsStr::new(PARTS), &parts)?;
    drop(parts);
    ensure!(
        capture_tree(source, limits, until)? == *expected
            && capture_tree(destination, limits, until)? == *expected,
        "legacy copy final inventory changed"
    );
    Ok(())
}

pub struct SavedLegacyArtifact {
    pub artifact: Artifact,
    pub raw_source: Option<Vec<u8>>,
    pub profile: Option<ExtractionProfile>,
    pub payload: Option<ExtractionPayload>,
}

/// Evidence capability made from verified private copied bytes, not a caller's
/// Snapshot. It grants no catalog membership or source/installer authority.
pub struct VerifiedLegacyArchive {
    scope: LegacyScope,
    identity: LegacyPublicationIdentity,
    evidence: ArchiveEvidence,
    store: FileStore,
    pins: PinnedRoot,
}
enum ArchiveEvidence {
    Catalog(Box<crate::blobs::Catalog>),
    Historical(Box<historical_catalog::HistoricalCatalog>),
    Current(Box<current_archive::CurrentArchive>),
}
/// A copied immutable catalog has no active predecessor OS locks. Reconstruct
/// only its scratch reader lease, outside the sealed collection inventory.
/// This grants no publication authority: the unchanged canonical catalog reader
/// still validates the pointer, receipt, inventory and generation contents.
fn prepare_validation_reader_lease(pins: &PinnedRoot, header: &PersistentRevision) -> Result<()> {
    ensure!(
        matches!(header.storage_format, 1 | 2) && crate::blobs::valid_digest(&header.revision),
        "invalid saved catalog lease identity"
    );
    crate::validate_collection(&header.collection)?;
    ensure!(
        pins.path.file_name() == Some(OsStr::new(&header.collection)),
        "validation collection differs from saved catalog lease"
    );
    let parent = pins
        .directories
        .get(
            pins.directories
                .len()
                .checked_sub(2)
                .context("validation namespace pin missing")?,
        )
        .context("validation namespace pin missing")?;
    pins.revalidate()?;
    let locks = parent.create_dir(OsStr::new(".reader-locks"))?;
    let collection = locks.create_dir(OsStr::new(&header.collection))?;
    let name = OsStr::new(&header.revision);
    let lease = match collection.open_file(name, SetupFileMode::CreateNew) {
        Ok(file) => file,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::AlreadyExists) =>
        {
            collection.open_file(name, SetupFileMode::ReadWrite)?
        }
        Err(error) => return Err(error),
    };
    ensure!(
        lease.metadata()?.len() == 0,
        "unknown nonempty validation reader lease preserved"
    );
    lease.sync_all()?;
    collection.sync()?;
    pins.revalidate()
}

impl VerifiedLegacyArchive {
    /// Validation may create SQLite/advisory metadata. `validation` MUST be an
    /// independently copied working tree, never the immutable archive/original.
    pub fn open_validation_copy(
        validation: &Path,
        scope: LegacyScope,
        archive_inventory: &LegacyInventory,
        limits: CaptureLimits,
        until: Instant,
    ) -> Result<Self> {
        let archive =
            Self::open_discovered_validation_copy(validation, archive_inventory, limits, until)?;
        ensure!(
            archive.scope == scope,
            "legacy saved scope differs from captured native declaration"
        );
        Ok(archive)
    }

    /// Discover labels only from authenticated saved evidence on an independently
    /// inventoried working copy. The installer must still validate original
    /// configuration scope before granting authority or publishing a destination.
    pub fn open_discovered_validation_copy(
        validation: &Path,
        archive_inventory: &LegacyInventory,
        limits: CaptureLimits,
        until: Instant,
    ) -> Result<Self> {
        let pins = PinnedRoot::open(validation)?;
        ensure!(
            capture_tree(validation, limits, until)? == *archive_inventory,
            "legacy validation copy differs from sealed archive inventory"
        );
        let archive_inventory_sha256 = archive_inventory.identity()?;
        pins.revalidate()?;
        let store = FileStore::new(validation);
        if current_archive::present(validation)? {
            let current = current_archive::open(&store, limits, until)?;
            let snapshot = &current.snapshot;
            let scope = LegacyScope {
                root: snapshot.root.clone(),
                collection: snapshot.collection.clone(),
                roots: snapshot.roots.clone(),
            };
            let identity = LegacyPublicationIdentity {
                header: None,
                generation: current.original.token.sequence,
                original_root_sha256: current.original.token.digest,
                archive_inventory_sha256,
                current: Some(current.original.clone()),
                catalog: None,
            };
            pins.revalidate()?;
            return Ok(Self {
                scope,
                identity,
                evidence: ArchiveEvidence::Current(Box::new(current)),
                store,
                pins,
            });
        }
        let header = store
            .persistent_revision()
            .context("validate archived catalog publication header")?;
        let head = store
            .expected_head()
            .context("read archived catalog publication head")?;
        if let Some(header) = &header
            && header.storage_format != 0
        {
            if let ExpectedHead::V2(token) = &head {
                ensure!(
                    header.token.as_ref() == Some(token),
                    "archived catalog head/header changed"
                );
            }
            prepare_validation_reader_lease(&pins, header)
                .context("prepare scratch catalog generation reader lease")?;
        }
        let original_root_sha256 = match head {
            ExpectedHead::Legacy(hash) => hash,
            ExpectedHead::V2(token) => token.revision,
            ExpectedHead::Empty => anyhow::bail!("legacy collection has no saved publication"),
        };
        let evidence = if let Some(header) = &header
            && historical_catalog::present(&store, header)?
        {
            ArchiveEvidence::Historical(Box::new(
                historical_catalog::open(&store, header, archive_inventory, limits, until)
                    .context("open authenticated historical catalog")?,
            ))
        } else {
            ArchiveEvidence::Catalog(Box::new(
                store
                    .catalog_bounded(limits.max_snapshot_bytes)
                    .context("open archived catalog with its scratch generation lease")?
                    .context("legacy saved catalog unavailable")?,
            ))
        };
        let snapshot = match &evidence {
            ArchiveEvidence::Catalog(catalog) => &catalog.snapshot,
            ArchiveEvidence::Historical(catalog) => &catalog.snapshot,
            ArchiveEvidence::Current(_) => unreachable!("Current handled before catalog"),
        };
        let scope = LegacyScope {
            root: snapshot.root.clone(),
            collection: snapshot.collection.clone(),
            roots: snapshot.roots.clone(),
        };
        crate::validate_collection(&scope.collection)?;
        ensure!(
            serde_json::to_vec(&store.persistent_revision()?)? == serde_json::to_vec(&header)?,
            "archived catalog header changed while opening"
        );
        let identity = LegacyPublicationIdentity {
            header,
            current: None,
            catalog: match &evidence {
                ArchiveEvidence::Historical(catalog) => Some(catalog.original.clone()),
                _ => None,
            },
            generation: snapshot.generation,
            original_root_sha256,
            archive_inventory_sha256,
        };
        pins.revalidate()?;
        Ok(Self {
            scope,
            identity,
            evidence,
            store,
            pins,
        })
    }
    pub fn scope(&self) -> &LegacyScope {
        &self.scope
    }
    pub fn identity(&self) -> &LegacyPublicationIdentity {
        &self.identity
    }
    pub fn snapshot(&self) -> &Snapshot {
        match &self.evidence {
            ArchiveEvidence::Catalog(catalog) => &catalog.snapshot,
            ArchiveEvidence::Historical(catalog) => &catalog.snapshot,
            ArchiveEvidence::Current(current) => &current.snapshot,
        }
    }
    pub fn artifact(&self, path: &str, maximum: u64) -> Result<SavedLegacyArtifact> {
        self.pins.revalidate()?;
        if let ArchiveEvidence::Current(current) = &self.evidence {
            current.revalidate()?;
        }
        let entry = self
            .snapshot()
            .artifacts
            .get(path)
            .context("artifact is outside saved legacy inventory")?;
        if matches!(&self.evidence, ArchiveEvidence::Historical(_)) {
            return historical_artifact::load(&self.store, &self.pins, entry, maximum);
        }
        let profile = entry
            .retained
            .as_ref()
            .and_then(|reference| reference.profile.clone());
        let payload = profile
            .as_ref()
            .map(|_| self.store.load_extraction_payload(entry))
            .transpose()?;
        let raw_source = if entry
            .retained
            .as_ref()
            .is_some_and(|reference| reference.raw_source)
        {
            Some(self.store.read_retained_source_bounded(entry, maximum)?)
        } else {
            None
        };
        let mut artifact = self.store.load_retained_bounded(entry, maximum)?;
        artifact.retained = None;
        self.pins.revalidate()?;
        Ok(SavedLegacyArtifact {
            artifact,
            raw_source,
            profile,
            payload,
        })
    }
}

#[cfg(test)]
mod tests;

pub mod publication;
pub use publication::{
    AdoptionResource, LegacyCompatibility, VerifiedLegacyPublication, verify_publication,
};

/// Actual saved native preparation, never a deserialized authority claim.
/// The reservation commits the prior-reader fence before the first Current write.
pub struct LegacyAdoptionWritePermit {
    control: PathBuf,
    receipt: airk_core::catalog_setup::IndexSetupReceipt,
    plan_digest: Sha256Digest,
    plan: airk_core::catalog_legacy_adoption::LegacyAdoptionPlan,
    control_pin: PinnedRoot,
    data_pin: PinnedRoot,
}
impl LegacyAdoptionWritePermit {
    pub fn from_prepared(
        control: &Path,
        receipt: &airk_core::catalog_setup::IndexSetupReceipt,
        plan: &airk_core::catalog_legacy_adoption::LegacyAdoptionPlan,
    ) -> Result<Self> {
        receipt.validate()?;
        plan.validate()?;
        ensure!(
            receipt.request == plan.setup,
            "legacy permit differs from exact adoption plan"
        );
        let plan_digest = plan.identity()?;
        let control = control.canonicalize()?;
        let permit = Self {
            control_pin: PinnedRoot::open(&control)?,
            data_pin: PinnedRoot::open(Path::new(&receipt.request.registration.data_root))?,
            control,
            receipt: receipt.clone(),
            plan_digest,
            plan: plan.clone(),
        };
        permit.revalidate()?;
        Ok(permit)
    }
    fn revalidate(&self) -> Result<()> {
        use airk_core::catalog_setup::SetupStage;
        self.control_pin.revalidate()?;
        self.data_pin.revalidate()?;
        ensure!(
            self.receipt.legacy_adoption == Some(self.plan_digest)
                && self
                    .receipt
                    .preparation
                    .as_ref()
                    .is_some_and(|p| p.legacy_adoption == Some(self.plan_digest))
                && matches!(self.receipt.stage, SetupStage::Prepared | SetupStage::Ready),
            "legacy adoption requires actual native prepared intent"
        );
        let registry = crate::service::registry::ServiceRegistry::open_saved(&self.control)?;
        let saved = registry
            .index_setup_receipt(self.receipt.request.caller, &self.receipt.request.key)?
            .context("native legacy adoption reservation disappeared")?;
        ensure!(
            saved == self.receipt,
            "native legacy adoption preparation changed"
        );
        ensure!(
            crate::service::registry::runtime_contract::required_capabilities(&self.control)?
                .minimum_registry_schema
                >= 13,
            "legacy saved evidence requires the prior-runtime rollback fence"
        );
        ensure!(
            crate::service::data_scope::has_saved_binding(
                Path::new(&self.receipt.request.registration.data_root),
                &self.control,
                self.receipt.request.key.namespace
            )?,
            "legacy adoption target lacks actual native ownership"
        );
        Ok(())
    }
    pub fn prepared_registration(&self) -> Result<&airk_core::service::ProjectRegistration> {
        self.revalidate()?;
        Ok(&self.receipt.request.registration)
    }
    pub fn require_archive(&self, archive: &VerifiedLegacyArchive) -> Result<()> {
        self.revalidate()?;
        ensure!(
            archive.identity().archive_inventory_sha256 == self.plan.archive_inventory_sha256
                && archive.identity().original_root_sha256 == self.plan.original_publication_sha256
                && archive.scope().root == self.plan.original_registration.root
                && archive.scope().collection == self.plan.original_registration.collection
                && airk_core::catalog_legacy_adoption::source_inventory_digest(
                    &archive.scope().root,
                    &archive.scope().collection,
                    &archive.scope().roots
                )? == self.plan.source_inventory_sha256,
            "legacy archive differs from native reviewed adoption plan"
        );
        Ok(())
    }
    pub fn require_store(&self, store: &FileStore) -> Result<()> {
        self.revalidate()?;
        let managed = store.managed_path()?;
        ensure!(
            managed.namespace == "collections-1.4"
                && managed.root.canonicalize()?
                    == Path::new(&self.receipt.request.registration.data_root).canonicalize()?
                && managed.collection == self.receipt.request.registration.collection
                && store.service_incarnation()? == Some(self.receipt.operation_id),
            "legacy write permit belongs to another prepared service collection"
        );
        Ok(())
    }
}

/// Holds the exact existing advisory used by official standalone index writers.
/// Acquisition neither creates a lock nor removes/rewrites a historical marker.
/// Scheduler/process exclusion and original config authority remain caller-owned.
pub struct LegacyWriterExclusion {
    parent: PinnedRoot,
    collection: SetupDirectory,
    locks: SetupDirectory,
    name: std::ffi::OsString,
    advisory: File,
}
impl LegacyWriterExclusion {
    pub fn acquire_existing(collection: &Path) -> Result<Self> {
        let parent = PinnedRoot::open(
            collection
                .parent()
                .context("legacy collection parent missing")?,
        )?;
        let name = collection
            .file_name()
            .context("legacy collection name missing")?
            .to_owned();
        let collection = parent.root().open_dir(&name)?;
        let locks = parent.root().open_dir(OsStr::new(".writer-locks"))?;
        let advisory = locks.open_file(&name, SetupFileMode::ReadWrite)?;
        advisory
            .try_lock()
            .context("legacy saved index writer remains active")?;
        let held = Self {
            parent,
            collection,
            locks,
            name,
            advisory,
        };
        held.revalidate()?;
        Ok(held)
    }
    pub fn revalidate(&self) -> Result<()> {
        self.parent.revalidate()?;
        ensure!(
            self.parent.root().open_dir(&self.name)?.identity()? == self.collection.identity()?,
            "legacy saved collection directory identity changed"
        );
        let locks = self.parent.root().open_dir(OsStr::new(".writer-locks"))?;
        ensure!(
            locks.identity()? == self.locks.identity()?
                && file_identity(&locks.open_file(&self.name, SetupFileMode::ReadWrite)?)?
                    == file_identity(&self.advisory)?,
            "legacy saved writer lease identity changed"
        );
        Ok(())
    }
    /// Local physical-object identity, never a scope, grant or publication.
    /// Grouping callers must also bind the namespace and authenticated saved
    /// publication/source inventory while retaining this native exclusion.
    pub fn collection_identity(&self) -> Result<Sha256Digest> {
        self.revalidate()?;
        let (volume, file) = self.collection.identity()?;
        Ok(airk_core::identity::framed_digest(
            b"airk/legacy-collection-file-identity/1",
            &[
                std::env::consts::OS.as_bytes(),
                &volume.to_be_bytes(),
                &file.to_be_bytes(),
            ],
        ))
    }
}
