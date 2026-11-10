// SPDX-License-Identifier: Apache-2.0
//! Native, descriptor-anchored construction of unpublished setup metadata.
//! Every mutation is relative to an actual retained directory object. Logical
//! paths label native ownership; they are never used as mutation operands.
use super::metadata::Metadata;
use super::{IndexSetupReceipt, NativeIndexPreparation, SetupPreparationReview, SetupStage};
use crate::service::{ProjectLease, data_scope, registry::ServiceRegistry};
use crate::store_fs::{SetupDirectory, SetupFileMode, file_identity};
use airk_core::identity::{Incarnation, Sha256Digest, framed_digest};
use anyhow::{Context, Result, ensure};
use std::path::PathBuf;
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::{Read, Write},
    path::Path,
};
#[path = "service_catalog_setup_storage_split.rs"]
mod split;
#[cfg(test)]
pub(in crate::service::registry::catalog) use split::prepare_split_test_storage;
pub use split::{PreparedSplitStorage, prepare_split_storage};

#[cfg(windows)]
#[path = "service_catalog_setup_storage_windows.rs"]
mod windows;
#[cfg(windows)]
#[path = "service_catalog_setup_storage_windows_root.rs"]
mod windows_root;
#[cfg(windows)]
#[path = "service_catalog_setup_storage_windows_stage_prefix.rs"]
mod windows_stage_prefix;

/// Native OS pins captured during review. They are never serializable input.
pub struct NativeSetupPins {
    parent: File,
    data: Option<File>,
    sources: Vec<File>,
    #[cfg(windows)]
    ancestors: Vec<File>,
}
impl NativeSetupPins {
    pub fn from_native_review(parent: File, data: Option<File>, sources: Vec<File>) -> Self {
        Self {
            parent,
            data,
            sources,
            #[cfg(windows)]
            ancestors: Vec::new(),
        }
    }
    #[cfg(windows)]
    pub fn from_native_review_with_ancestors(
        parent: File,
        data: Option<File>,
        sources: Vec<File>,
        ancestors: Vec<File>,
    ) -> Self {
        Self {
            parent,
            data,
            sources,
            ancestors,
        }
    }
}

/// An actual saved reservation's prepared storage. Project and writer ownership,
/// descriptor pins and the normal quota allowance survive through the Store CAS.
pub struct PreparedStorage {
    control: PathBuf,
    receipt: IndexSetupReceipt,
    root: SetupDirectory,
    parent: SetupDirectory,
    collection: SetupDirectory,
    slot: SetupDirectory,
    sources: Vec<SetupDirectory>,
    configuration: Sha256Digest,
    split: Option<crate::service::registry::catalog::split::NativeSplitState>,
    split_seal: Option<crate::service::registry::catalog::split::NativeSplitState>,
    _project: ProjectLease,
    _claim: File,
    _writer: File,
    _allowance: Option<crate::quota::setup::SetupMetadataAllowance>,
    #[cfg(windows)]
    _ancestors: Vec<File>,
}
impl PreparedStorage {
    pub fn record(self, registry: &mut ServiceRegistry) -> Result<IndexSetupReceipt> {
        self.record_inner(registry, None)
    }
    pub fn record_legacy_adoption(
        self,
        registry: &mut ServiceRegistry,
        plan: &airk_core::catalog_legacy_adoption::LegacyAdoptionPlan,
    ) -> Result<IndexSetupReceipt> {
        plan.validate()?;
        ensure!(
            self.receipt.request == plan.setup,
            "legacy adoption storage differs from reviewed setup"
        );
        self.record_inner(registry, Some(plan.identity()?))
    }
    fn record_inner(
        self,
        registry: &mut ServiceRegistry,
        legacy_adoption: Option<Sha256Digest>,
    ) -> Result<IndexSetupReceipt> {
        ensure!(
            self.receipt.legacy_adoption == legacy_adoption,
            "native storage preparation changed reserved adoption intent"
        );
        ensure!(
            self.split.is_none(),
            "split preparation cannot complete ordinary index setup"
        );
        ensure!(
            registry.root.canonicalize()? == self.control,
            "foreign setup registry owner"
        );
        self.check_final()?;
        let current = registry
            .index_setup_receipt(self.receipt.request.caller, &self.receipt.request.key)?
            .context("native setup reservation disappeared")?;
        ensure!(current == self.receipt, "native setup reservation changed");
        let state = registry
            .catalog_state()?
            .context("native catalog unavailable")?;
        let review = self
            .receipt
            .preparation
            .as_ref()
            .map(|e| e.review.clone())
            .unwrap_or(SetupPreparationReview {
                expected_catalog_revision: state.revision,
                expected_source_revision: self.receipt.source_revision,
            });
        let request = &self.receipt.request;
        let storage_identity = framed_digest(
            b"airk/catalog-native-setup-storage/1",
            &[
                self.control
                    .to_str()
                    .context("non-UTF-8 control")?
                    .as_bytes(),
                request.key.namespace.as_bytes(),
                request.registration.binding.as_bytes(),
                self.receipt.operation_id.as_bytes(),
            ],
        );
        let mut native = NativeIndexPreparation::from_validated_native_review(
            request.registration.clone(),
            request.sources.clone(),
            storage_identity,
            self.configuration,
            review,
        )?;
        native.legacy_adoption = legacy_adoption;
        registry.record_index_preparation(
            request.caller,
            &request.key,
            self.receipt.request_digest,
            &native,
        )
    }
    fn check_final(&self) -> Result<()> {
        let request = &self.receipt.request;
        let data = Path::new(&request.registration.data_root);
        check_named_pin(
            &self.parent,
            data.parent().context("setup storage parent missing")?,
        )?;
        check_named_pin(&self.root, data)?;
        for (pin, source) in self.sources.iter().zip(&request.sources) {
            check_named_pin(pin, Path::new(&source.canonical_path))?;
        }
        let owner = data_scope::initial_owner_bytes(data, &self.control, request.key.namespace)?;
        let actual = self
            .root
            .read_file(OsStr::new(data_scope::OWNER), owner.len() as u64)?;
        data_scope::bound_from_existing_setup(data, &self.control, request.key.namespace, &actual)?
            .check()?;
        let namespace = self.root.open_dir(OsStr::new("collections-1.4"))?;
        ensure!(
            namespace
                .open_dir(OsStr::new(&request.registration.collection))?
                .identity()?
                == self.collection.identity()?,
            "native setup collection object changed"
        );
        let incarnations = self.collection.open_dir(OsStr::new("incarnations"))?;
        ensure!(
            incarnations
                .open_dir(OsStr::new(&self.receipt.operation_id.to_string()))?
                .identity()?
                == self.slot.identity()?,
            "native setup selected incarnation changed"
        );
        let format = match &self.split_seal {
            Some(state) => Metadata::new_split(&self.control, &self.receipt, state)?,
            None => Metadata::new(&self.control, &self.receipt)?,
        };
        validate_collection(&self.collection, &format, true)
    }
}

/// Called only by native management after a real durable reserve. Every selected
/// storage mutation below uses retained directory descriptors, including quotas.
pub fn prepare_index_storage(
    registry: &ServiceRegistry,
    control: &Path,
    receipt: &IndexSetupReceipt,
    pins: NativeSetupPins,
) -> Result<PreparedStorage> {
    prepare_with_hook(registry, control, receipt, pins, || {})
}
fn prepare_with_hook(
    registry: &ServiceRegistry,
    control: &Path,
    receipt: &IndexSetupReceipt,
    pins: NativeSetupPins,
    before_effects: impl FnOnce(),
) -> Result<PreparedStorage> {
    prepare_authorized(registry, control, receipt, pins, None, before_effects)
}
fn prepare_authorized(
    registry: &ServiceRegistry,
    control: &Path,
    receipt: &IndexSetupReceipt,
    pins: NativeSetupPins,
    split: Option<&crate::service::registry::catalog::split::NativeSplitState>,
    before_effects: impl FnOnce(),
) -> Result<PreparedStorage> {
    receipt.validate()?;
    let control = control.canonicalize()?;
    ensure!(
        registry.root.canonicalize()? == control,
        "foreign native setup registry"
    );
    if let Some(state) = split {
        let current = registry.catalog_split_state(state.snapshot().receipt.operation_id)?;
        ensure!(
            current.snapshot() == state.snapshot(),
            "native split setup reservation changed"
        );
        crate::service::registry::catalog::split::require_destination_registration(
            &registry.connection,
            &receipt.request.registration,
            state,
        )?;
    } else {
        let saved = registry
            .index_setup_receipt(receipt.request.caller, &receipt.request.key)?
            .context("native setup requires its saved reservation")?;
        ensure!(
            saved == *receipt && receipt.stage != SetupStage::Ready,
            "native setup reservation changed or is already ready"
        );
    }
    super::no_pending_fence(&registry.connection)?;
    if split.is_none() {
        super::reserved_sources(&registry.connection, receipt)?;
    }
    let request = &receipt.request;
    let project = ProjectLease::acquire(&control, request.registration.binding)?;
    #[cfg(windows)]
    let ancestors = pins.ancestors;
    let parent = SetupDirectory::from_file(pins.parent)?;
    let data_path = Path::new(&request.registration.data_root);
    check_named_pin(&parent, data_path.parent().context("setup parent missing")?)?;
    ensure!(
        pins.sources.len() == request.sources.len(),
        "native setup source pin capacity mismatch"
    );
    let sources = pins
        .sources
        .into_iter()
        .map(SetupDirectory::from_file)
        .collect::<Result<Vec<_>>>()?;
    for (pin, source) in sources.iter().zip(&request.sources) {
        check_named_pin(pin, Path::new(&source.canonical_path))?;
    }
    let split_seal = split
        .map(|state| split::initial_seal_state(registry, state))
        .transpose()?;
    let metadata = match &split_seal {
        Some(state) => Metadata::new_split(&control, receipt, state)?,
        None => Metadata::new(&control, receipt)?,
    };
    parent.require_growth(metadata.encoded_bytes()?)?;
    let stage_name = OsString::from(format!(".airk-setup-{}.part", receipt.operation_id));
    let missing_root = pins.data.is_none();
    let root = if let Some(data) = pins.data {
        let data = SetupDirectory::from_file(data)?;
        check_named_pin(&data, data_path)?;
        data
    } else {
        match parent.open_entry(data_path.file_name().context("setup leaf missing")?) {
            Ok(_) => anyhow::bail!("selected missing storage leaf was replaced after review"),
            Err(error) if absent(&error) => (),
            Err(error) => return Err(error),
        }
        parent.create_dir(&stage_name)?
    };
    validate_claim_shape(&root, &metadata)?;
    // Tests inject the actual parent/data replacement here, after every path
    // precheck. No mutable logical path is used below this point.
    before_effects();
    let claim = open_or_create(&root, OsStr::new(data_scope::LOCK))?;
    ensure!(
        claim.metadata()?.len() == 0,
        "unknown native data claim lock contents"
    );
    claim
        .try_lock()
        .context("native data ownership claim is busy")?;
    match root.read_file(OsStr::new(data_scope::OWNER), metadata.owner.len() as u64) {
        Ok(actual) => ensure!(actual == metadata.owner, data_scope::ForeignDataRoot),
        Err(error) if absent(&error) => {
            let allowance = crate::quota::setup::SetupMetadataAllowance::reserve_at(
                &root,
                data_path,
                "",
                metadata.owner.len() as u64,
                allocation(receipt, b"owner")?,
            )?;
            write_exact(&root, data_scope::OWNER, &metadata.owner)?;
            drop(allowance);
        }
        Err(error) => return Err(error),
    }
    #[cfg(windows)]
    let claim = if missing_root {
        windows_root::publish_owner_bound_root(
            &parent,
            &root,
            &control,
            &stage_name,
            &metadata,
            receipt,
            claim,
        )?
    } else {
        claim
    };
    let namespace = root.create_dir(OsStr::new("collections-1.4"))?;
    let writers = namespace.create_dir(OsStr::new(".writer-locks"))?;
    let writer = open_or_create(&writers, OsStr::new(&request.registration.collection))?;
    ensure!(
        writer.metadata()?.len() == 0,
        "unknown native writer lock contents"
    );
    writer.try_lock().context("index writer locked")?;
    let collection_name = OsStr::new(&request.registration.collection);
    let existing_collection = match namespace.open_dir(collection_name) {
        Ok(collection) => {
            validate_collection(&collection, &metadata, false)?;
            Some(collection)
        }
        Err(error) if absent(&error) => None,
        Err(error) => return Err(error),
    };
    let complete = existing_collection
        .as_ref()
        .is_some_and(|collection| validate_collection(collection, &metadata, true).is_ok());
    let allowance = if complete {
        None
    } else {
        Some(crate::quota::setup::SetupMetadataAllowance::reserve_at(
            &root,
            data_path,
            &request.registration.collection,
            metadata.collection_bytes()?,
            allocation(receipt, b"collection")?,
        )?)
    };
    let collection = match existing_collection {
        Some(collection) => collection,
        None => {
            let staged = root.create_dir(&stage_name)?;
            validate_collection(&staged, &metadata, false)?;
            write_collection(&staged, &metadata)?;
            validate_collection(&staged, &metadata, true)?;
            root.publish_no_replace(&stage_name, &staged.file, &namespace, collection_name)?;
            staged
        }
    };
    if !complete {
        write_collection(&collection, &metadata)?;
    }
    validate_collection(&collection, &metadata, true)?;
    let slot = collection
        .open_dir(OsStr::new("incarnations"))?
        .open_dir(OsStr::new(&receipt.operation_id.to_string()))?;
    #[cfg(unix)]
    if missing_root {
        parent.publish_no_replace(
            &stage_name,
            &root.file,
            &parent,
            data_path.file_name().context("setup leaf missing")?,
        )?;
    }
    let prepared = PreparedStorage {
        control,
        receipt: receipt.clone(),
        root,
        parent,
        collection,
        slot,
        sources,
        configuration: metadata.configuration,
        split: split.cloned(),
        split_seal,
        _project: project,
        _claim: claim,
        _writer: writer,
        _allowance: allowance,
        #[cfg(windows)]
        _ancestors: ancestors,
    };
    prepared.check_final()?;
    Ok(prepared)
}

fn allocation(receipt: &IndexSetupReceipt, phase: &[u8]) -> Result<Incarnation> {
    framed_digest(
        b"airk/catalog-native-setup-allocation/1",
        &[
            receipt.request.key.namespace.as_bytes(),
            receipt.operation_id.as_bytes(),
            phase,
        ],
    )
    .to_string()[..32]
        .parse()
}
fn absent(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
}
fn open_or_create(directory: &SetupDirectory, name: &OsStr) -> Result<File> {
    match directory.open_file(name, SetupFileMode::ReadWrite) {
        Ok(file) => Ok(file),
        Err(error) if absent(&error) => directory.open_file(name, SetupFileMode::CreateNew),
        Err(error) => Err(error),
    }
}
fn check_named_pin(pin: &SetupDirectory, path: &Path) -> Result<()> {
    ensure!(
        SetupDirectory::open_canonical(path)?.identity()? == pin.identity()?,
        "native setup directory object changed"
    );
    Ok(())
}
pub(crate) fn write_exact(directory: &SetupDirectory, name: &str, bytes: &[u8]) -> Result<()> {
    match directory.read_file(OsStr::new(name), bytes.len() as u64) {
        Ok(actual) => {
            ensure!(
                actual == bytes,
                "native setup metadata differs from its saved operation"
            );
            return Ok(());
        }
        Err(error) if absent(&error) => (),
        Err(error) => return Err(error),
    }
    let temporary = super::native_setup_scratch_name(name, bytes);
    let mut file = match directory.open_file(OsStr::new(&temporary), SetupFileMode::ReadWrite) {
        Ok(file) => file,
        Err(error) if absent(&error) => {
            directory.open_file(OsStr::new(&temporary), SetupFileMode::CreateNew)?
        }
        Err(error) => return Err(error),
    };
    file.try_lock()
        .context("native setup metadata write is busy")?;
    let mut actual = Vec::new();
    (&mut file)
        .take(bytes.len() as u64 + 1)
        .read_to_end(&mut actual)?;
    ensure!(
        bytes.starts_with(&actual),
        "unknown native setup partial preserved"
    );
    use std::io::{Seek, SeekFrom};
    let offset = file.seek(SeekFrom::End(0))?;
    ensure!(
        offset <= bytes.len() as u64,
        "native setup partial exceeds bound"
    );
    file.write_all(&bytes[offset as usize..])?;
    file.sync_all()?;
    #[cfg(test)]
    if name == data_scope::OWNER && std::env::var_os("AIRK_TEST_SETUP_OWNER_CRASH").is_some() {
        // Only the dedicated child fixture sets this environment. OS process
        // exit closes native leases without running Rust cleanup destructors.
        std::process::exit(76);
    }
    directory.publish_no_replace(OsStr::new(&temporary), &file, directory, OsStr::new(name))
}
fn validate_claim_shape(root: &SetupDirectory, metadata: &Metadata) -> Result<()> {
    match root.read_file(OsStr::new(data_scope::OWNER), metadata.owner.len() as u64) {
        Ok(owner) => {
            ensure!(owner == metadata.owner, data_scope::ForeignDataRoot);
            return Ok(());
        }
        Err(error) if absent(&error) => (),
        Err(error) => return Err(error),
    }
    for (number, name) in root.entries()?.enumerate() {
        ensure!(number < 8, "unowned setup storage contains unknown entries");
        let name = name?;
        if name == data_scope::PART {
            ensure!(
                metadata
                    .owner
                    .starts_with(&root.read_file(&name, metadata.owner.len() as u64)?),
                "foreign ownership partial preserved"
            );
        } else if name == data_scope::LOCK {
            ensure!(
                root.open_file(&name, SetupFileMode::Read)?
                    .metadata()?
                    .len()
                    == 0,
                "unknown setup claim lock"
            );
        } else if crate::quota::initial_service_claim_entry(&name) {
            // Existing native partial validation ran during read-only review.
            // The actual claim and quota ledger are checked again by descriptors.
            let entry = root.open_entry(&name)?;
            ensure!(
                entry.metadata()?.is_file() || entry.metadata()?.is_dir(),
                "invalid setup quota prefix"
            );
        } else if name
            .to_str()
            .is_some_and(|name| name.starts_with(".setup-write-"))
        {
            ensure!(
                name == super::native_setup_scratch_name(data_scope::OWNER, &metadata.owner)
                    .as_str(),
                "unknown setup scratch preserved"
            );
            ensure!(
                metadata
                    .owner
                    .starts_with(&root.read_file(&name, metadata.owner.len() as u64)?),
                "unknown setup scratch preserved"
            );
        } else {
            anyhow::bail!("unowned setup storage contains unknown entries");
        }
    }
    Ok(())
}
fn write_collection(directory: &SetupDirectory, metadata: &Metadata) -> Result<()> {
    // The capability's complete immutable capsule precedes its owner pointer.
    if let Some((name, bytes)) = &metadata.split_capsule {
        write_exact(directory, name, bytes)?;
    }
    for (name, bytes) in &metadata.files {
        write_exact(directory, name, bytes)?;
    }
    let incarnations = directory.create_dir(OsStr::new("incarnations"))?;
    let slot = incarnations.create_dir(OsStr::new(&metadata.slot))?;
    write_exact(&slot, "roots.json", &metadata.roots)?;
    slot.sync()?;
    directory.sync()
}
pub(super) fn validate_collection(
    directory: &SetupDirectory,
    metadata: &Metadata,
    complete: bool,
) -> Result<()> {
    for (number, name) in directory.entries()?.enumerate() {
        ensure!(
            number < metadata.collection_files().len() * 2 + 4,
            "unknown initial collection metadata"
        );
        let name = name?;
        if name == "incarnations" {
            let incarnations = directory.open_dir(&name)?;
            for (number, slot) in incarnations.entries()?.enumerate() {
                ensure!(number == 0, "unknown initial collection incarnation");
                let slot = slot?;
                ensure!(
                    slot == metadata.slot.as_str(),
                    "foreign initial collection incarnation"
                );
                let slot = incarnations.open_dir(&slot)?;
                validate_files(&slot, &[("roots.json", metadata.roots.as_slice())])?;
            }
        } else if name == ".current-gates" {
            // A real native Collection::select observes unpublished setup and
            // creates this exact advisory. It is coordination, not index data.
            let gates = directory.open_dir(&name)?;
            ensure!(
                gates.identity()?.0 == directory.identity()?.0,
                "initial gate crosses filesystem boundary"
            );
            for (number, gate) in gates.entries()?.enumerate() {
                ensure!(number == 0, "unknown initial current gate preserved");
                let gate = gate?;
                ensure!(
                    gate == metadata.slot.as_str(),
                    "foreign initial current gate preserved"
                );
                ensure!(
                    gates.read_file(&gate, 0)?.is_empty(),
                    "unknown initial current gate contents"
                );
            }
        } else {
            validate_one_file(directory, &name, &metadata.collection_files())?;
        }
    }
    if complete {
        for (name, bytes) in metadata.collection_files() {
            ensure!(
                directory.read_file(OsStr::new(name), bytes.len() as u64)? == bytes,
                "initial collection metadata changed"
            );
        }
        let slot = directory
            .open_dir(OsStr::new("incarnations"))?
            .open_dir(OsStr::new(&metadata.slot))?;
        ensure!(
            slot.read_file(OsStr::new("roots.json"), metadata.roots.len() as u64)?
                == metadata.roots,
            "initial source metadata changed"
        );
    }
    Ok(())
}
fn validate_files(directory: &SetupDirectory, files: &[(&str, &[u8])]) -> Result<()> {
    for (number, name) in directory.entries()?.enumerate() {
        ensure!(number < files.len() * 2, "unknown initial setup file state");
        validate_one_file(directory, &name?, files)?;
    }
    Ok(())
}
fn validate_one_file(
    directory: &SetupDirectory,
    name: &OsStr,
    files: &[(&str, &[u8])],
) -> Result<()> {
    for (expected, bytes) in files {
        if name == *expected {
            ensure!(
                directory.read_file(name, bytes.len() as u64)? == *bytes,
                "initial metadata changed"
            );
            return Ok(());
        }
        if name == super::native_setup_scratch_name(expected, bytes).as_str() {
            ensure!(
                bytes.starts_with(&directory.read_file(name, bytes.len() as u64)?),
                "unknown setup partial preserved"
            );
            return Ok(());
        }
    }
    anyhow::bail!("unknown initial collection file preserved")
}

#[cfg(all(test, unix))]
#[path = "service_catalog_setup_storage_tests.rs"]
mod tests;
