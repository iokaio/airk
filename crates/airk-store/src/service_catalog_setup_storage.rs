// SPDX-License-Identifier: Apache-2.0
//! Native, descriptor-anchored construction of unpublished setup metadata.
//! Every mutation is relative to an actual retained directory object. Logical
//! paths label native ownership; they are never used as mutation operands.
use super::metadata::Metadata;
use super::{IndexSetupReceipt, NativeIndexPreparation, SetupPreparationReview, SetupStage};
use crate::service::{ProjectLease, data_scope, registry::ServiceRegistry};
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

#[cfg(unix)]
use std::{
    ffi::{CStr, CString},
    os::{
        fd::{AsRawFd, FromRawFd, RawFd},
        unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::{MetadataExt, OpenOptionsExt},
        },
    },
};
#[cfg(windows)]
#[path = "service_catalog_setup_storage_windows.rs"]
mod windows;
#[cfg(windows)]
#[path = "service_catalog_setup_storage_windows_root.rs"]
mod windows_root;
#[cfg(windows)]
#[path = "service_catalog_setup_storage_windows_stage_prefix.rs"]
mod windows_stage_prefix;

pub(crate) fn file_identity(file: &File) -> Result<(u64, u64)> {
    #[cfg(unix)]
    {
        let metadata = file.metadata()?;
        Ok((metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        let (volume, id, _) = windows::native_file_identity(file)?;
        Ok((volume, id))
    }
}

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
    saved_adoption: Option<super::SavedAdoptionSources>,
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
        let format = if let Some(saved) = &self.saved_adoption {
            Metadata::saved_adoption(&self.control, &self.receipt, saved.plan())?
        } else {
            match &self.split_seal {
                Some(state) => Metadata::new_split(&self.control, &self.receipt, state)?,
                None => Metadata::new(&self.control, &self.receipt)?,
            }
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
pub fn prepare_saved_adoption_storage(
    registry: &ServiceRegistry,
    control: &Path,
    receipt: &IndexSetupReceipt,
    pins: NativeSetupPins,
    saved: super::SavedAdoptionSources,
) -> Result<PreparedStorage> {
    ensure!(
        saved.plan().setup == receipt.request
            && receipt.legacy_adoption == Some(saved.plan().identity()?),
        "saved adoption storage differs from native archive intent"
    );
    ensure!(
        pins.sources.is_empty(),
        "saved adoption cannot claim live source ownership"
    );
    prepare_authorized(registry, control, receipt, pins, None, Some(saved), || {})
}
fn prepare_with_hook(
    registry: &ServiceRegistry,
    control: &Path,
    receipt: &IndexSetupReceipt,
    pins: NativeSetupPins,
    before_effects: impl FnOnce(),
) -> Result<PreparedStorage> {
    prepare_authorized(registry, control, receipt, pins, None, None, before_effects)
}
fn prepare_authorized(
    registry: &ServiceRegistry,
    control: &Path,
    receipt: &IndexSetupReceipt,
    pins: NativeSetupPins,
    split: Option<&crate::service::registry::catalog::split::NativeSplitState>,
    saved_adoption: Option<super::SavedAdoptionSources>,
    before_effects: impl FnOnce(),
) -> Result<PreparedStorage> {
    receipt.validate()?;
    ensure!(
        split.is_none() || saved_adoption.is_none(),
        "saved adoption cannot reuse split authority"
    );
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
        if saved_adoption.is_some() {
            pins.sources.is_empty()
        } else {
            pins.sources.len() == request.sources.len()
        },
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
    let metadata = if let Some(saved) = &saved_adoption {
        Metadata::saved_adoption(&control, receipt, saved.plan())?
    } else {
        match &split_seal {
            Some(state) => Metadata::new_split(&control, receipt, state)?,
            None => Metadata::new(&control, receipt)?,
        }
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
        saved_adoption,
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

pub(crate) enum SetupFileMode {
    Read,
    ReadWrite,
    CreateNew,
}

pub(crate) struct SetupDirectory {
    file: File,
}
#[cfg(unix)]
impl SetupDirectory {
    pub(crate) fn open_canonical(path: &Path) -> Result<Self> {
        ensure!(
            path.is_absolute() && path.canonicalize()? == path,
            "setup directory must be canonical"
        );
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        Self::from_file(file)
    }
    pub(crate) fn from_file(file: File) -> Result<Self> {
        ensure!(
            file.metadata()?.is_dir(),
            "setup descriptor is not a directory"
        );
        Ok(Self { file })
    }
    pub(crate) fn try_clone(&self) -> Result<Self> {
        Self::from_file(self.file.try_clone()?)
    }
    pub(crate) fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
    pub(crate) fn identity(&self) -> Result<(u64, u64)> {
        let metadata = self.file.metadata()?;
        Ok((metadata.dev(), metadata.ino()))
    }
    pub(crate) fn open_dir(&self, name: &OsStr) -> Result<Self> {
        let name = component(name)?;
        // SAFETY: this directory descriptor and single-component C string live
        // through openat; NOFOLLOW prevents rebinding through a linked child.
        let fd = unsafe {
            libc::openat(
                self.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: successful openat returned a new owned descriptor.
        Self::from_file(unsafe { File::from_raw_fd(fd) })
    }
    pub(crate) fn create_dir(&self, name: &OsStr) -> Result<Self> {
        let encoded = component(name)?;
        // SAFETY: retained parent and terminated single-component name.
        if unsafe { libc::mkdirat(self.as_raw_fd(), encoded.as_ptr(), 0o700) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(error.into());
            }
        }
        let result = self.open_dir(name)?;
        ensure!(
            result.identity()?.0 == self.identity()?.0,
            "setup crosses a filesystem boundary"
        );
        self.sync()?;
        Ok(result)
    }
    pub(crate) fn open_entry(&self, name: &OsStr) -> Result<File> {
        self.open_raw(name, libc::O_RDONLY | libc::O_NONBLOCK)
    }
    pub(crate) fn open_file(&self, name: &OsStr, mode: SetupFileMode) -> Result<File> {
        let flags = match mode {
            SetupFileMode::Read => libc::O_RDONLY,
            SetupFileMode::ReadWrite => libc::O_RDWR,
            SetupFileMode::CreateNew => libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
        };
        let file = self.open_raw(name, flags | libc::O_NONBLOCK)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.nlink() == 1,
            "linked or non-file setup metadata"
        );
        Ok(file)
    }
    fn open_raw(&self, name: &OsStr, flags: i32) -> Result<File> {
        let name = component(name)?;
        // SAFETY: owned directory, bounded terminated name, mode is supplied
        // for O_CREAT and ignored otherwise. No parent path is resolved.
        let fd = unsafe {
            libc::openat(
                self.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: successful openat returned a new owned descriptor.
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    pub(crate) fn read_file(&self, name: &OsStr, maximum: u64) -> Result<Vec<u8>> {
        let file = self.open_file(name, SetupFileMode::Read)?;
        ensure!(
            file.metadata()?.len() <= maximum,
            "setup metadata exceeds bound"
        );
        let mut bytes = Vec::new();
        file.take(maximum.saturating_add(1))
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= maximum,
            "setup metadata grew beyond bound"
        );
        Ok(bytes)
    }
    #[cfg(test)]
    pub(crate) fn create_file(&self, name: &OsStr, bytes: &[u8]) -> Result<File> {
        let mut file = self.open_file(name, SetupFileMode::CreateNew)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(file)
    }
    pub(crate) fn unlink_file_if_identity(&self, name: &OsStr, owned: &File) -> Result<()> {
        let current = self.open_file(name, SetupFileMode::Read)?;
        ensure!(
            file_identity(&current)? == file_identity(owned)?,
            "setup quota reservation identity changed"
        );
        // Unix unlink is a name operation under the same native ledger lock;
        // retain the existing cooperating-owner guarantee without claiming an
        // atomic unlink-by-inode primitive on this platform.
        self.unlink_file(name)
    }
    pub(crate) fn unlink_file(&self, name: &OsStr) -> Result<()> {
        let name = component(name)?;
        // SAFETY: caller owns the native entry and supplies one retained parent.
        if unsafe { libc::unlinkat(self.as_raw_fd(), name.as_ptr(), 0) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        self.sync()
    }
    /// Remove only the exact empty child under the caller's native exclusion.
    /// Unix checks then unlinks a name; it has no atomic unlink-by-inode API.
    pub(crate) fn unlink_empty_dir_if_identity(&self, name: &OsStr, owned: &Self) -> Result<()> {
        let current = self.open_dir(name)?;
        ensure!(
            current.identity()? == owned.identity()?,
            "setup empty-directory identity changed"
        );
        ensure!(
            current.entries()?.next().transpose()?.is_none(),
            "setup deletion directory is not empty"
        );
        let name = component(name)?;
        // SAFETY: retained parent, one child component and empty-directory-only
        // unlink. The caller retains native exclusion across identity and unlink.
        if unsafe { libc::unlinkat(self.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) } != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        self.sync()
    }
    pub(crate) fn publish_no_replace(
        &self,
        name: &OsStr,
        owned: &File,
        destination: &Self,
        target: &OsStr,
    ) -> Result<()> {
        let actual = self.open_entry(name)?;
        ensure!(
            file_identity(&actual)? == file_identity(owned)?,
            "native setup publication source identity changed"
        );
        // Unix rename remains a name operation under native writer/claim
        // ownership. This observation adds no claim of atomic rename-by-inode.
        let name = component(name)?;
        let target = component(target)?;
        #[cfg(target_os = "macos")]
        // SAFETY: both retained descriptors and terminated child names live
        // through the atomic no-replacement rename.
        let result = unsafe {
            libc::renameatx_np(
                self.as_raw_fd(),
                name.as_ptr(),
                destination.as_raw_fd(),
                target.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        #[cfg(target_os = "linux")]
        // SAFETY: same descriptor/name invariants as above.
        let result = unsafe {
            libc::renameat2(
                self.as_raw_fd(),
                name.as_ptr(),
                destination.as_raw_fd(),
                target.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let result =
            { anyhow::bail!("native setup requires an atomic no-replacement directory rename") };
        ensure!(result == 0, std::io::Error::last_os_error());
        destination.sync()?;
        self.sync()
    }
    /// Atomic name replacement under the native owner's writer/manifest lock.
    /// Both leaves are checked as exact regular objects in this held directory;
    /// Unix does not provide a compare-and-replace-by-inode primitive.
    pub(crate) fn replace_file(
        &self,
        name: &OsStr,
        owned: &File,
        target: &OsStr,
        expected: &File,
    ) -> Result<()> {
        self.replace_file_to(name, owned, self, target, expected)
    }
    /// Expected-object check under owning exclusion plus descriptor-relative
    /// rename. Unix does not supply atomic compare-and-replace-by-inode.
    pub(crate) fn replace_file_to(
        &self,
        name: &OsStr,
        owned: &File,
        destination: &Self,
        target: &OsStr,
        expected: &File,
    ) -> Result<()> {
        let source = self.open_file(name, SetupFileMode::Read)?;
        let current = destination.open_file(target, SetupFileMode::Read)?;
        ensure!(
            file_identity(&source)? == file_identity(owned)?
                && file_identity(&current)? == file_identity(expected)?,
            "native metadata replacement identity changed"
        );
        let name = component(name)?;
        let target = component(target)?;
        // SAFETY: two retained directories and validated single-component names.
        ensure!(
            unsafe {
                libc::renameat(
                    self.as_raw_fd(),
                    name.as_ptr(),
                    destination.as_raw_fd(),
                    target.as_ptr(),
                )
            } == 0,
            std::io::Error::last_os_error()
        );
        destination.sync()?;
        self.sync()
    }
    pub(crate) fn sync(&self) -> Result<()> {
        self.file.sync_all().map_err(Into::into)
    }
    pub(crate) fn entries(&self) -> Result<SetupEntries> {
        // Open "." to obtain an independent directory offset. dup alone would
        // share the cursor with later bounded inventories on this same handle.
        let name = c".";
        // SAFETY: retained directory and static terminated name.
        let fd = unsafe {
            libc::openat(
                self.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        // SAFETY: fd is new and owned; fdopendir takes ownership on success.
        let directory = unsafe { libc::fdopendir(fd) };
        if directory.is_null() {
            let error = std::io::Error::last_os_error();
            // SAFETY: fdopendir failed and did not consume fd.
            drop(unsafe { File::from_raw_fd(fd) });
            return Err(error.into());
        }
        Ok(SetupEntries {
            directory,
            done: false,
        })
    }
    pub(crate) fn require_growth(&self, bytes: u64) -> Result<()> {
        if bytes == 0 {
            return Ok(());
        }
        self.capacity(Path::new("native-setup-directory"))?
            .require_growth(bytes)
    }
    pub(crate) fn capacity(&self, logical: &Path) -> Result<crate::capacity::Capacity> {
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: retained descriptor and correctly sized writable output.
        if unsafe { libc::fstatvfs(self.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error())
                .context("native setup capacity inspection failed");
        }
        // SAFETY: successful fstatvfs initialized the output.
        let stat = unsafe { stat.assume_init() };
        let block = u128::from(stat.f_frsize);
        let total =
            u64::try_from(u128::from(stat.f_blocks) * block).context("setup capacity overflow")?;
        let free =
            u64::try_from(u128::from(stat.f_bfree) * block).context("setup capacity overflow")?;
        let available =
            u64::try_from(u128::from(stat.f_bavail) * block).context("setup capacity overflow")?;
        ensure!(
            available <= free && available <= total,
            "filesystem returned inconsistent setup capacity"
        );
        Ok(crate::capacity::Capacity {
            observed_path: logical.into(),
            total_to_caller_bytes: total,
            free_bytes: free,
            available_to_caller_bytes: available,
            safety_headroom_bytes: crate::capacity::SAFETY_HEADROOM_BYTES,
            available_growth_bytes: available
                .saturating_sub(crate::capacity::SAFETY_HEADROOM_BYTES),
            guarantee: "point-in-time OS free space; not reserved against concurrent or external writes",
        })
    }
}

#[cfg(unix)]
pub(crate) struct SetupEntries {
    directory: *mut libc::DIR,
    done: bool,
}
#[cfg(unix)]
impl Iterator for SetupEntries {
    type Item = Result<OsString>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        loop {
            #[cfg(target_os = "macos")]
            // SAFETY: errno storage belongs to the current thread.
            let errno = unsafe { libc::__error() };
            #[cfg(not(target_os = "macos"))]
            // SAFETY: errno storage belongs to the current thread.
            let errno = unsafe { libc::__errno_location() };
            // SAFETY: errno is valid thread-local storage and this iterator is
            // the sole consumer of its owned DIR cursor.
            unsafe {
                *errno = 0;
            }
            let entry = unsafe { libc::readdir(self.directory) };
            if entry.is_null() {
                self.done = true;
                // SAFETY: errno is still current thread's storage.
                return if unsafe { *errno } == 0 {
                    None
                } else {
                    Some(Err(std::io::Error::last_os_error().into()))
                };
            }
            // SAFETY: readdir returned a live, terminated name until next call.
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
            if name == b"." || name == b".." {
                continue;
            }
            return Some(Ok(OsString::from_vec(name.to_vec())));
        }
    }
}
#[cfg(unix)]
impl Drop for SetupEntries {
    fn drop(&mut self) {
        // SAFETY: this iterator exclusively owns the DIR and its descriptor.
        let _ = unsafe { libc::closedir(self.directory) };
    }
}
#[cfg(unix)]
fn component(name: &OsStr) -> Result<CString> {
    let bytes = name.as_bytes();
    ensure!(
        !bytes.is_empty() && bytes != b"." && bytes != b".." && !bytes.contains(&b'/'),
        "native setup requires a single child name"
    );
    CString::new(bytes).context("native setup child name contains NUL")
}

#[cfg(all(test, unix))]
#[path = "service_catalog_setup_storage_tests.rs"]
mod tests;
