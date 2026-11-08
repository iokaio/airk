// SPDX-License-Identifier: Apache-2.0
//! Explicit native filesystem preparation after an exact catalog reservation.
//! No evidence read, indexing job, watcher or model setup is performed here.
use airk_core::{
    catalog_setup::{IndexSetupReceipt, IndexSetupRequest, SetupStage, SetupStorageAction},
    identity::{Incarnation, Sha256Digest, framed_digest},
};
use airk_store::service::{collection::Collection, data_scope, registry::ServiceRegistry};
#[cfg(test)]
use airk_store::{WriterLock, roots::RootConfig, service::collection::Selection};
use anyhow::{Context, Result, ensure};
#[cfg(test)]
use std::ffi::OsStr;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    path::{Path, PathBuf},
};

pub(crate) struct NativeSetupReview {
    control: PathBuf,
    request: IndexSetupRequest,
    parent: DirectoryPin,
    existing_data: Option<DirectoryPin>,
    sources: Vec<DirectoryPin>,
    storage_action: SetupStorageAction,
    metadata_plan: Sha256Digest,
    legacy_adoption: Option<Sha256Digest>,
    saved_adoption: Option<airk_store::service::registry::catalog::setup::SavedAdoptionSources>,
}
impl NativeSetupReview {
    pub(crate) fn storage_action(&self) -> SetupStorageAction {
        self.storage_action
    }
    pub(crate) fn review_digest(&self) -> Result<Sha256Digest> {
        self.parent.check()?;
        let data = self
            .existing_data
            .as_ref()
            .map(|pin| {
                pin.check()?;
                pin.identity()
            })
            .transpose()?;
        let sources = self
            .sources
            .iter()
            .map(|pin| {
                pin.check()?;
                pin.identity()
            })
            .collect::<Result<Vec<_>>>()?;
        let digest = framed_digest(
            b"airk/catalog-native-setup-review/1",
            &[&serde_json::to_vec(&(
                self.request.identity()?,
                self.storage_action,
                self.metadata_plan,
                self.parent.identity()?,
                data,
                sources,
            ))?],
        );
        Ok(match self.legacy_adoption {
            None => digest,
            Some(plan) => framed_digest(
                b"airk/catalog-native-legacy-adoption-review/1",
                &[digest.as_bytes(), plan.as_bytes()],
            ),
        })
    }
}

/// Pins actual native owners until the preparation receipt has committed.
/// The opaque Store proof is never returned to a frontend or extracted alone.
pub(crate) enum PreparedIndexSetup {
    Anchored(airk_store::service::registry::catalog::setup::storage::PreparedStorage),
}
impl PreparedIndexSetup {
    pub(crate) fn record(self, registry: &mut ServiceRegistry) -> Result<IndexSetupReceipt> {
        match self {
            Self::Anchored(storage) => storage.record(registry),
        }
    }
    pub(crate) fn record_legacy_adoption(
        self,
        registry: &mut ServiceRegistry,
        plan: &airk_core::catalog_legacy_adoption::LegacyAdoptionPlan,
    ) -> Result<IndexSetupReceipt> {
        match self {
            Self::Anchored(storage) => storage.record_legacy_adoption(registry, plan),
        }
    }
}

/// Read-only review. New custom storage requires an existing canonical parent;
/// only its explicitly selected leaf can be created by `prepare`.
pub(crate) fn review(control: &Path, request: &IndexSetupRequest) -> Result<NativeSetupReview> {
    review_inner(control, request, None)
}
#[path = "catalog_legacy_adoption.rs"]
mod legacy_adoption;
pub(crate) use legacy_adoption::review as review_legacy_adoption;
pub(crate) use legacy_adoption::validate_origin as validate_legacy_adoption_origin;
pub(crate) use legacy_adoption::validate_scope as validate_legacy_adoption_origin_scope;
fn review_inner(
    control: &Path,
    request: &IndexSetupRequest,
    legacy_adoption: Option<Sha256Digest>,
) -> Result<NativeSetupReview> {
    crate::runtime_install::validate_control_root(control)?;
    let control = control.canonicalize()?;
    validate_request(&control, request)?;
    let registry = ServiceRegistry::open_saved(&control)?;
    let state = registry
        .catalog_state()?
        .context("native catalog unavailable")?;
    ensure!(
        state.namespace == request.key.namespace && state.activated,
        "native setup catalog changed"
    );
    let previous = registry.index_setup_receipt(request.caller, &request.key)?;
    if let Some(previous) = &previous {
        ensure!(
            previous.request == *request && previous.legacy_adoption == legacy_adoption,
            "native setup request key conflict"
        );
        ensure!(
            previous.stage != SetupStage::Ready,
            "index setup is already ready; read its saved receipt"
        );
    } else {
        ensure!(
            state.revision == request.expected_catalog_revision,
            "native setup review is stale"
        );
    }
    let mut review = review_filesystem(control, request, previous.is_some())?;
    review.legacy_adoption = legacy_adoption;
    Ok(review)
}

/// Split review pins the same physical inputs, while its distinct native intent
/// owns preparation and keeps all destination evidence sealed.
pub(crate) fn review_split(
    control: &Path,
    request: &airk_core::catalog_split::SplitRequest,
) -> Result<NativeSetupReview> {
    crate::runtime_install::validate_control_root(control)?;
    request.validate()?;
    let context = split_format_request(request)?;
    let mut review = review_filesystem(control.canonicalize()?, &context, false)?;
    let collection = Path::new(&context.registration.data_root)
        .join("collections-1.4")
        .join(&context.registration.collection);
    match fs::symlink_metadata(&collection) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
        Ok(_) => anyhow::bail!("split destination collection must be fresh independent storage"),
    }
    review.metadata_plan = framed_digest(
        b"airk/split-native-setup-review/1",
        &[
            review.metadata_plan.as_bytes(),
            request.identity()?.as_bytes(),
        ],
    );
    Ok(review)
}
fn split_format_request(
    request: &airk_core::catalog_split::SplitRequest,
) -> Result<IndexSetupRequest> {
    Ok(IndexSetupRequest {
        caller: request.caller,
        key: request.key.clone(),
        expected_catalog_revision: request.expected_catalog_revision,
        name: request.destination_name.clone(),
        registration: request.destination_registration.clone(),
        sources: airk_core::catalog_setup::canonical_setup_sources(
            &request
                .source_configuration
                .aliases
                .iter()
                .map(|(alias, path)| airk_core::catalog_setup::SetupSource {
                    canonical_path: path.clone(),
                    aliases: vec![alias.clone()],
                    owner: false,
                })
                .chain(std::iter::once(airk_core::catalog_setup::SetupSource {
                    canonical_path: request.original_registration.root.clone(),
                    aliases: vec![],
                    owner: true,
                }))
                .collect::<Vec<_>>(),
        )?,
        setup_watch: false,
    })
}
pub(crate) fn prepare_split(
    registry: &ServiceRegistry,
    control: &Path,
    state: &airk_store::service::registry::catalog::split::NativeSplitState,
) -> Result<airk_store::service::registry::catalog::setup::storage::PreparedSplitStorage> {
    let context = split_format_request(&state.snapshot().request)?;
    // Exact accepted partial state may now exist; revalidate its owned prefix,
    // preserving the original parent/source directory objects through Store CAS.
    let review = review_filesystem(control.canonicalize()?, &context, true)?;
    review.review_digest()?;
    let pins = split_pins(review);
    airk_store::service::registry::catalog::setup::storage::prepare_split_storage(
        registry, control, state, pins,
    )
}
fn split_pins(
    review: NativeSetupReview,
) -> airk_store::service::registry::catalog::setup::storage::NativeSetupPins {
    #[cfg(unix)]
    {
        airk_store::service::registry::catalog::setup::storage::NativeSetupPins::from_native_review(
            review.parent.file,
            review.existing_data.map(|pin| pin.file),
            review.sources.into_iter().map(|pin| pin.file).collect(),
        )
    }
    #[cfg(windows)]
    {
        let mut ancestors = review.parent._ancestors;
        let data = review.existing_data.map(|pin| {
            ancestors.extend(pin._ancestors);
            pin.file
        });
        let sources = review
            .sources
            .into_iter()
            .map(|pin| {
                ancestors.extend(pin._ancestors);
                pin.file
            })
            .collect();
        airk_store::service::registry::catalog::setup::storage::NativeSetupPins::from_native_review_with_ancestors(review.parent.file,data,sources,ancestors)
    }
}

fn review_filesystem(
    control: PathBuf,
    request: &IndexSetupRequest,
    reserved: bool,
) -> Result<NativeSetupReview> {
    validate_request(&control, request)?;
    let metadata_plan =
        airk_store::service::registry::catalog::setup::validate_setup_metadata_plan(
            &control, request,
        )?;
    let data = Path::new(&request.registration.data_root);
    let parent = DirectoryPin::open(
        data.parent()
            .context("custom index storage has no parent")?,
    )?;
    let storage_action = validate_destination(data, &control, request.key.namespace, reserved)?;
    if !reserved {
        let collection = data
            .join("collections-1.4")
            .join(&request.registration.collection);
        match fs::symlink_metadata(&collection) {
            Ok(_) => {
                Collection::existing(
                    data,
                    &request.registration.root,
                    &request.registration.collection,
                )?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    let existing_data = if matches!(storage_action, SetupStorageAction::CreateMissingLeaf) {
        None
    } else {
        Some(DirectoryPin::open(data)?)
    };
    let sources = request
        .sources
        .iter()
        .map(|source| DirectoryPin::open(Path::new(&source.canonical_path)))
        .collect::<Result<Vec<_>>>()?;
    Ok(NativeSetupReview {
        control,
        request: request.clone(),
        parent,
        existing_data,
        sources,
        storage_action,
        metadata_plan,
        legacy_adoption: None,
        saved_adoption: None,
    })
}

pub(crate) fn prepare(
    registry: &ServiceRegistry,
    control: &Path,
    review: NativeSetupReview,
) -> Result<PreparedIndexSetup> {
    ensure!(
        control.canonicalize()? == review.control,
        "native setup control changed after review"
    );
    let receipt = registry
        .index_setup_receipt(review.request.caller, &review.request.key)?
        .context("native setup requires its saved reservation before filesystem preparation")?;
    ensure!(
        receipt.request == review.request && receipt.legacy_adoption == review.legacy_adoption,
        "native setup reservation request changed"
    );
    review.review_digest()?;
    #[cfg(unix)]
    let pins =
        airk_store::service::registry::catalog::setup::storage::NativeSetupPins::from_native_review(
            review.parent.file,
            review.existing_data.map(|pin| pin.file),
            review.sources.into_iter().map(|pin| pin.file).collect(),
        );
    #[cfg(windows)]
    let pins = {
        // Retain every original deny-delete ancestor through Store's CAS. Private
        // staged children use HANDLE-relative writes and never path constructors.
        let mut ancestors = review.parent._ancestors;
        let data = review.existing_data.map(|pin| {
            ancestors.extend(pin._ancestors);
            pin.file
        });
        let sources = review
            .sources
            .into_iter()
            .map(|pin| {
                ancestors.extend(pin._ancestors);
                pin.file
            })
            .collect();
        airk_store::service::registry::catalog::setup::storage::NativeSetupPins::from_native_review_with_ancestors(review.parent.file,data,sources,ancestors)
    };
    let storage = match review.saved_adoption {
        Some(saved) => {
            airk_store::service::registry::catalog::setup::storage::prepare_saved_adoption_storage(
                registry, control, &receipt, pins, saved,
            )?
        }
        None => airk_store::service::registry::catalog::setup::storage::prepare_index_storage(
            registry, control, &receipt, pins,
        )?,
    };
    Ok(PreparedIndexSetup::Anchored(storage))
}

fn validate_request(
    control: &Path,
    request: &IndexSetupRequest,
) -> Result<BTreeMap<String, String>> {
    request.validate()?;
    let registration = &request.registration;
    ensure!(
        airk_kernel::service_projects::planned_registration(
            Path::new(&registration.root),
            Path::new(&registration.data_root),
            &registration.collection,
        )? == *registration,
        "native setup paths must be canonical"
    );
    let data = Path::new(&registration.data_root);
    ensure!(
        !data.starts_with(control) && !control.starts_with(data),
        "index storage overlaps service control"
    );
    let mut accepted = Vec::<PathBuf>::new();
    let mut aliases = BTreeMap::new();
    for source in &request.sources {
        let path = Path::new(&source.canonical_path);
        let metadata = fs::symlink_metadata(path)?;
        plain_directory(&metadata)?;
        ensure!(
            path.canonicalize()? == path,
            "native source root redirected"
        );
        ensure!(
            !path.starts_with(data)
                && !data.starts_with(path)
                && !path.starts_with(control)
                && !control.starts_with(path),
            "native setup source overlaps private storage"
        );
        ensure!(
            !accepted
                .iter()
                .any(|old| old.starts_with(path) || path.starts_with(old)),
            "native setup source roots overlap"
        );
        accepted.push(path.into());
        for alias in &source.aliases {
            airk_store::roots::validate_alias(alias)?;
            ensure!(
                fs::symlink_metadata(Path::new(&registration.root).join(format!("@{alias}")))
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
                "native source alias namespace is occupied or unavailable"
            );
            aliases.insert(alias.clone(), source.canonical_path.clone());
        }
    }
    airk_store::roots::validate_planned_sources(Path::new(&registration.root), data, &aliases)?;
    Ok(aliases)
}

fn validate_destination(
    data: &Path,
    control: &Path,
    namespace: Incarnation,
    reserved: bool,
) -> Result<SetupStorageAction> {
    let metadata = match fs::symlink_metadata(data) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SetupStorageAction::CreateMissingLeaf);
        }
        Err(e) => return Err(e.into()),
    };
    plain_directory(&metadata)?;
    ensure!(
        data.canonicalize()? == data,
        "native setup destination redirected"
    );
    if data_scope::has_saved_binding(data, control, namespace)? {
        return Ok(SetupStorageAction::ReuseOwnedStorage);
    }
    if fs::read_dir(data)?.next().transpose()?.is_none() {
        return Ok(SetupStorageAction::ClaimExistingEmptyStorage);
    }
    ensure!(
        reserved && data_scope::recoverable_setup_binding(data, control, namespace)?,
        "custom index storage is occupied without matching native ownership"
    );
    Ok(SetupStorageAction::ResumeReservedStorageClaim)
}

#[cfg(test)]
fn prepare_configuration(
    selection: &Selection,
    writer: &WriterLock,
    request: &IndexSetupRequest,
) -> Result<Sha256Digest> {
    let mut aliases = BTreeMap::new();
    for source in &request.sources {
        for alias in &source.aliases {
            aliases.insert(alias.clone(), source.canonical_path.clone());
        }
    }
    airk_store::roots::validate_sources(
        Path::new(&request.registration.root),
        Path::new(&request.registration.data_root),
        &aliases,
    )?;
    let directory = selection.store.directory();
    let config_path = directory.join("roots.json");
    if let Ok(metadata) = fs::symlink_metadata(&config_path) {
        plain_file(&config_path, &metadata)?;
    }
    let mut config = RootConfig::load(directory, &request.registration.root)?;
    ensure!(
        config
            .roots
            .iter()
            .all(|(alias, path)| aliases.get(alias) == Some(path)),
        "saved source configuration differs from the reserved setup"
    );
    if config.roots != aliases || !config_path.try_exists()? {
        config.roots = aliases;
        let quota = airk_store::quota::Quota::for_store(&selection.store)?;
        let reservation = quota.reserve(&request.registration.collection, 96 * 1024)?;
        reservation.charge(96 * 1024)?;
        selection
            .store
            .invalidate_owned_state_before_mutation(writer)?;
        config.save(directory)?;
    }
    let actual = RootConfig::load(directory, &request.registration.root)?;
    Ok(framed_digest(
        b"airk/catalog-source-configuration/1",
        &[
            request.registration.root.as_bytes(),
            &serde_json::to_vec(&actual.roots)?,
        ],
    ))
}

fn plain_directory(metadata: &fs::Metadata) -> Result<()> {
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "native setup requires a plain directory"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "native setup refuses redirected directories"
        );
    }
    Ok(())
}
#[cfg(test)]
fn plain_file(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "native setup refuses linked metadata"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            metadata.nlink() == 1,
            "native setup refuses hard-linked metadata"
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "native setup refuses redirected metadata"
        );
        ensure!(
            file_identity(&File::open(path)?)?.2 == 1,
            "native setup refuses hard-linked metadata"
        );
    }
    #[cfg(not(windows))]
    let _ = path;
    Ok(())
}

/// Native object identity, with no timestamp/content-derived ownership shortcut.
struct DirectoryPin {
    path: PathBuf,
    file: File,
    #[cfg(windows)]
    _ancestors: Vec<File>,
}
impl DirectoryPin {
    fn identity(&self) -> Result<(u64, u64)> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = self.file.metadata()?;
            Ok((metadata.dev(), metadata.ino()))
        }
        #[cfg(windows)]
        {
            let (volume, index, _) = file_identity(&self.file)?;
            Ok((u64::from(volume), index))
        }
    }
    fn open(path: &Path) -> Result<Self> {
        ensure!(
            path.is_absolute() && path.canonicalize()? == path,
            "custom storage parent must already exist at its canonical location"
        );
        let metadata = fs::symlink_metadata(path)?;
        plain_directory(&metadata)?;
        #[cfg(unix)]
        let file = {
            use std::os::unix::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(path)?
        };
        #[cfg(windows)]
        let (file, ancestors) = {
            use std::os::windows::fs::OpenOptionsExt;
            let paths: Vec<_> = path.ancestors().collect();
            ensure!(
                paths.len() <= 128,
                "native setup path component budget exceeded"
            );
            let mut ancestors = Vec::new();
            for ancestor in paths.into_iter().rev() {
                let handle = fs::OpenOptions::new()
                    .read(true)
                    // Attribute-only handles do not participate in Windows
                    // delete sharing. FILE_TRAVERSE activates read/execute
                    // sharing without requesting directory enumeration.
                    .access_mode(0x80 | 0x20)
                    .share_mode(3)
                    .custom_flags(0x02000000 | 0x00200000)
                    .open(ancestor)?;
                plain_directory(&handle.metadata()?)?;
                ancestors.push(handle);
            }
            let file = ancestors.pop().context("directory pin missing")?;
            (file, ancestors)
        };
        let pin = Self {
            path: path.into(),
            file,
            #[cfg(windows)]
            _ancestors: ancestors,
        };
        pin.check()?;
        Ok(pin)
    }
    fn check(&self) -> Result<()> {
        let metadata = fs::symlink_metadata(&self.path)?;
        plain_directory(&metadata)?;
        ensure!(
            self.path.canonicalize()? == self.path,
            "native setup directory redirected"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let pinned = self.file.metadata()?;
            ensure!(
                metadata.dev() == pinned.dev() && metadata.ino() == pinned.ino(),
                "native setup directory object changed"
            );
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let current = fs::OpenOptions::new()
                .read(true)
                .access_mode(0x80 | 0x20)
                .share_mode(3)
                .custom_flags(0x02000000 | 0x00200000)
                .open(&self.path)?;
            let (volume, index, _) = file_identity(&current)?;
            ensure!(
                self.identity()? == (u64::from(volume), index),
                "native setup directory object changed"
            );
        }
        Ok(())
    }
    #[cfg(test)]
    fn create_leaf(&self, name: &OsStr) -> Result<()> {
        self.check()?;
        ensure!(
            Path::new(name).components().count() == 1,
            "invalid custom storage leaf"
        );
        #[cfg(unix)]
        {
            use std::os::{fd::AsRawFd, unix::ffi::OsStrExt};
            let name = std::ffi::CString::new(name.as_bytes())?;
            // SAFETY: the retained directory descriptor and terminated single
            // leaf name are live; mkdirat never resolves a mutable parent path.
            let result = unsafe { libc::mkdirat(self.file.as_raw_fd(), name.as_ptr(), 0o700) };
            if result != 0 {
                return Err(std::io::Error::last_os_error())
                    .context("create explicitly selected index leaf");
            }
        }
        #[cfg(windows)]
        fs::create_dir(self.path.join(name))?;
        self.check()?;
        Ok(())
    }
}

/// Retains native parent/child identities while an installer uses its private
/// derived-data container. The shared parent is never chmodded or adopted.
pub(crate) struct PrivateDirectoryPin {
    parent: DirectoryPin,
    child: DirectoryPin,
}
impl PrivateDirectoryPin {
    pub(crate) fn check(&self) -> Result<()> {
        self.parent.check()?;
        self.child.check()?;
        crate::local_ipc::require_existing_root(&self.child.path)
    }
}
pub(crate) fn pin_private_directory(path: &Path, create: bool) -> Result<PrivateDirectoryPin> {
    let parent = DirectoryPin::open(path.parent().context("private data parent missing")?)?;
    #[cfg(unix)]
    let name = path.file_name().context("private data leaf missing")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid has no arguments or failure mode.
        let uid = unsafe { libc::geteuid() };
        for ancestor in parent.path.ancestors() {
            let metadata = fs::symlink_metadata(ancestor)?;
            ensure!(
                metadata.is_dir()
                    && !metadata.file_type().is_symlink()
                    && (metadata.uid() == uid || metadata.uid() == 0)
                    && (metadata.mode() & 0o022 == 0
                        || (metadata.uid() == 0 && metadata.mode() & 0o1000 != 0)),
                "unsafe adopted-data parent"
            );
        }
    }
    if create {
        #[cfg(unix)]
        {
            use std::os::{fd::AsRawFd, unix::ffi::OsStrExt};
            let name = std::ffi::CString::new(name.as_bytes())?;
            // SAFETY: pinned parent and a single terminated native leaf name.
            ensure!(
                unsafe { libc::mkdirat(parent.file.as_raw_fd(), name.as_ptr(), 0o700) } == 0,
                "create private adopted-data root: {}",
                std::io::Error::last_os_error()
            );
        }
        #[cfg(windows)]
        {
            crate::local_ipc::create_root(path)?;
        }
    }
    parent.check()?;
    crate::local_ipc::require_existing_root(path)?;
    let child = DirectoryPin::open(path)?;
    let pin = PrivateDirectoryPin { parent, child };
    pin.check()?;
    Ok(pin)
}

#[cfg(windows)]
fn file_identity(file: &File) -> Result<(u32, u64, u32)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut information = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
    // SAFETY: the retained handle and correctly sized output remain live.
    ensure!(
        unsafe {
            GetFileInformationByHandle(file.as_raw_handle().cast(), information.as_mut_ptr())
        } != 0,
        "native setup file identity inspection failed"
    );
    // SAFETY: the successful native call initialized the full output.
    let information = unsafe { information.assume_init() };
    Ok((
        information.dwVolumeSerialNumber,
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
        information.nNumberOfLinks,
    ))
}

#[cfg(test)]
#[path = "catalog_index_setup_tests.rs"]
mod tests;
