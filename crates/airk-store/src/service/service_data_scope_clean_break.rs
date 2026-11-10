// SPDX-License-Identifier: Apache-2.0
//! Ownership-bound storage disposal. Storage privacy is not IPC credential custody.
use super::*;
use crate::quota::{CleanBreakInventory, clean_break_inventory};
use crate::service::registry::{
    MAX_PROJECTS, ServiceRegistry, catalog::setup::storage::write_exact,
};
use crate::service::{EmbeddingLease, ProjectLease, ServiceLease, SourceWatchLease};
use crate::store_fs::{SetupDirectory, SetupFileMode, file_identity};
use airk_core::{
    identity::{Sha256Digest, framed_digest},
    service::ProjectRegistration,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
};

const MAX_FILES: usize = 4096;
const MAX_RECEIPT: u64 = 512 * 1024;
const FILES_PER_TURN: usize = 64;

fn missing(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}
fn saved_roster(
    control: &Path,
    namespace: Incarnation,
    registrations: &[ProjectRegistration],
) -> Result<ServiceRegistry> {
    ensure!(
        registrations.len() <= MAX_PROJECTS,
        "clean-break storage roster limit"
    );
    let registry = ServiceRegistry::open_saved(control)?;
    ensure!(
        registry.incarnation()? == namespace,
        "clean-break storage namespace changed"
    );
    let bindings = registry.bindings()?;
    ensure!(
        bindings.len() == registrations.len(),
        "incomplete clean-break storage registration roster"
    );
    let mut observed = BTreeSet::new();
    for registration in registrations {
        registration.validate()?;
        ensure!(
            observed.insert(registration.binding)
                && registry
                    .retained_project(registration.binding)?
                    .registration
                    == *registration,
            "clean-break storage registration changed"
        );
    }
    Ok(registry)
}
fn roster(
    data: &Path,
    control: &Path,
    namespace: Incarnation,
    registrations: &[ProjectRegistration],
) -> Result<()> {
    ensure!(
        canonical_location(data)? == data,
        "clean-break storage location is not canonical"
    );
    let registry = saved_roster(control, namespace, registrations)?;
    for registration in registrations {
        for source in crate::service::collection::clean_break_retained_source_roots(
            &registry,
            registration,
            namespace,
            registrations,
        )? {
            for source in [
                PathBuf::from(&source),
                canonical_location(Path::new(&source))?,
            ] {
                ensure!(
                    !source.starts_with(data) && !data.starts_with(&source),
                    "clean-break storage intersects original source"
                );
            }
        }
    }
    let selected = registrations
        .iter()
        .filter(|row| Path::new(&row.data_root) == data)
        .collect::<Vec<_>>();
    ensure!(
        !selected.is_empty(),
        "clean-break storage has no saved registration"
    );
    let mut names = BTreeSet::new();
    ensure!(
        selected
            .iter()
            .all(|row| names.insert(row.collection.as_str())),
        "duplicate clean-break storage collection"
    );
    Ok(())
}

struct NamespaceRoot {
    directory: SetupDirectory,
    staging: BTreeMap<airk_core::service::ProjectBinding, BTreeSet<PathBuf>>,
}

/// Nonserializable custody of the complete predecessor namespace. This is
/// stronger than an ordinary job or single-collection retirement permit: the
/// installation owner has selected fresh, independent replacement storage.
pub struct CleanBreakNamespaceOwner<'a> {
    // Release storage locks before the process/project exclusions. No ordinary
    // FileStore writer, quota mutation or recovered job is admitted by this type.
    _storage: Vec<File>,
    roots: BTreeMap<PathBuf, Option<NamespaceRoot>>,
    registrations: Vec<ProjectRegistration>,
    namespace: Incarnation,
    service: &'a ServiceLease,
    projects: Vec<ProjectLease>,
    _embedding: EmbeddingLease,
    _source: SourceWatchLease,
}
impl<'a> CleanBreakNamespaceOwner<'a> {
    /// The native installer retains its installation lock and actual service
    /// exclusion. Canonical process leases enforce its owner protocol; missing
    /// storage advisories are never created to manufacture storage custody.
    pub fn acquire(
        service: &'a ServiceLease,
        namespace: Incarnation,
        registrations: &[ProjectRegistration],
    ) -> Result<Self> {
        ensure!(
            registrations.len() <= MAX_PROJECTS,
            "clean-break namespace roster limit"
        );
        // This global check also authenticates the valid zero-index case: an
        // empty capability has no storage targets, and only an actually empty
        // saved registry in this exact native namespace can construct it.
        drop(saved_roster(service.root(), namespace, registrations)?);
        let source = SourceWatchLease::acquire(service.root())?;
        let embedding = EmbeddingLease::acquire(service.root())?;
        let projects = registrations
            .iter()
            .map(|row| ProjectLease::acquire(service.root(), row.binding))
            .collect::<Result<Vec<_>>>()?;
        let paths = registrations
            .iter()
            .map(|row| PathBuf::from(&row.data_root))
            .collect::<BTreeSet<_>>();
        let mut roots = BTreeMap::new();
        let mut storage = Vec::new();
        let mut entries = 0usize;
        let mut encoded = 0u64;
        for data in paths {
            roster(&data, service.root(), namespace, registrations)?;
            if !exists(&data)? {
                roots.insert(data, None);
                continue;
            }
            let (_, observed) =
                storage_inventory(&data, service.root(), namespace, registrations, false)?;
            let incoming = observed.files.len()
                + observed.directories.len()
                + observed.staging.values().map(BTreeSet::len).sum::<usize>();
            ensure!(
                entries + incoming <= MAX_FILES
                    && storage.len() + observed.files.len() + roots.len() + 1 + projects.len() + 3
                        <= MAX_FILES,
                "complete clean-break namespace inventory or handle limit"
            );
            let (receipt, held) =
                capture_retirement(&data, service.root(), namespace, registrations, false, true)?;
            let (directory, inventory) =
                storage_inventory(&data, service.root(), namespace, registrations, false)?;
            let final_incoming = inventory.files.len()
                + inventory.directories.len()
                + inventory.staging.values().map(BTreeSet::len).sum::<usize>();
            ensure!(
                incoming == final_incoming
                    && storage.len() + held.len() + roots.len() + 1 + projects.len() + 3
                        <= MAX_FILES,
                "clean-break namespace inventory changed during exclusion"
            );
            let staging_bytes = serde_json::to_vec(&inventory.staging)?.len() as u64;
            encoded = encoded
                .checked_add(serde_json::to_vec(&receipt)?.len() as u64)
                .and_then(|total| total.checked_add(staging_bytes))
                .context("namespace receipt or staging size overflow")?;
            ensure!(
                encoded <= MAX_RECEIPT,
                "complete clean-break namespace metadata limit"
            );
            entries += incoming;
            ensure!(
                directory.identity()? == receipt.identity,
                "clean-break namespace root changed"
            );
            storage.extend(held);
            roots.insert(
                data,
                Some(NamespaceRoot {
                    directory,
                    staging: inventory.staging,
                }),
            );
        }
        Ok(Self {
            _storage: storage,
            roots,
            registrations: registrations.to_vec(),
            namespace,
            service,
            projects,
            _embedding: embedding,
            _source: source,
        })
    }
    pub(crate) fn namespace(&self) -> Incarnation {
        self.namespace
    }
    pub(crate) fn registrations(&self) -> &[ProjectRegistration] {
        &self.registrations
    }
    pub(crate) fn validate_registration(&self, registration: &ProjectRegistration) -> Result<()> {
        ensure!(
            self.registrations().contains(registration),
            "foreign clean-break namespace registration"
        );
        ensure!(
            crate::service::registry::retained_namespace(self.service.root())? == self.namespace,
            "clean-break namespace owner changed"
        );
        let data = Path::new(&registration.data_root);
        match self
            .roots
            .get(data)
            .context("clean-break namespace root not captured")?
        {
            Some(root) => {
                ensure!(
                    SetupDirectory::open_canonical(data)?.identity()?
                        == root.directory.identity()?,
                    "clean-break namespace root object changed"
                );
                ensure!(
                    has_saved_binding(data, self.service.root(), self.namespace)?,
                    "clean-break namespace data binding lost"
                );
            }
            None => ensure!(
                !exists(data)?,
                "previously absent clean-break namespace storage appeared"
            ),
        }
        Ok(())
    }
    pub(crate) fn project_lease(
        &self,
        registration: &ProjectRegistration,
    ) -> Result<&ProjectLease> {
        self.validate_registration(registration)?;
        self.projects
            .iter()
            .find(|lease| lease.matches_registration(self.service.root(), registration.binding))
            .context("clean-break namespace project custody missing")
    }
    pub(crate) fn exact_staging_paths(
        &self,
        registration: &ProjectRegistration,
    ) -> Result<BTreeSet<PathBuf>> {
        self.validate_registration(registration)?;
        Ok(self
            .roots
            .get(Path::new(&registration.data_root))
            .and_then(Option::as_ref)
            .and_then(|root| root.staging.get(&registration.binding))
            .cloned()
            .unwrap_or_default())
    }
}

fn storage_inventory(
    data: &Path,
    control: &Path,
    namespace: Incarnation,
    registrations: &[ProjectRegistration],
    absent_collections: bool,
) -> Result<(SetupDirectory, CleanBreakInventory)> {
    let root = SetupDirectory::open_canonical(data)?;
    ensure!(
        has_saved_binding(data, control, namespace)?,
        "clean-break storage requires saved data ownership"
    );
    let owner_bytes = initial_owner_bytes(data, control, namespace)?;
    ensure!(
        root.read_file(OsStr::new(OWNER), MAX_BYTES)? == owner_bytes,
        "clean-break data owner changed"
    );
    let known = registrations
        .iter()
        .filter(|row| Path::new(&row.data_root) == data)
        .map(|row| row.collection.as_str())
        .collect::<BTreeSet<_>>();
    let mut result = CleanBreakInventory::default();
    for (count, name) in root.entries()?.enumerate() {
        ensure!(count < 7, "unknown clean-break storage entries");
        let name = name?;
        let text = name.to_str().context("invalid clean-break storage name")?;
        match text {
            OWNER => result.file(&[text], MAX_BYTES, false),
            PART => {
                ensure!(
                    owner_bytes.starts_with(&root.read_file(&name, MAX_BYTES)?),
                    "foreign data ownership partial"
                );
                result.file(&[text], MAX_BYTES, false);
            }
            LOCK | ".lifecycle-1.4.lock" | "usage-quota.lock" => {
                ensure!(
                    root.open_file(&name, SetupFileMode::Read)?
                        .metadata()?
                        .len()
                        == 0,
                    "unknown clean-break storage lock"
                );
                result.file(&[text], 0, true);
            }
            ".quota-1.4" => {
                let quota = clean_break_inventory(&root, data, registrations, namespace)?;
                result.files.extend(quota.files);
                result.directories.extend(quota.directories);
                for (binding, paths) in quota.staging {
                    result.staging.entry(binding).or_default().extend(paths);
                }
            }
            "collections-1.4" => {
                let collections = root.open_dir(&name)?;
                ensure!(
                    collections.identity()?.0 == root.identity()?.0,
                    "collections cross storage filesystem"
                );
                result.directory(&[text]);
                for (count, name) in collections.entries()?.enumerate() {
                    ensure!(
                        count < MAX_PROJECTS + 1,
                        "clean-break collection census limit"
                    );
                    let name = name?;
                    let collection = name.to_str().context("invalid collection census name")?;
                    let directory = collections.open_dir(&name)?;
                    ensure!(
                        directory.identity()?.0 == root.identity()?.0,
                        "collection metadata crosses filesystem"
                    );
                    if collection == ".writer-locks" {
                        result.directory(&[text, collection]);
                        for (count, name) in directory.entries()?.enumerate() {
                            ensure!(count < MAX_PROJECTS, "clean-break writer census limit");
                            let name = name?;
                            let writer = name.to_str().context("invalid writer advisory name")?;
                            ensure!(
                                known.contains(writer),
                                "foreign clean-break writer advisory"
                            );
                            ensure!(
                                directory
                                    .open_file(&name, SetupFileMode::Read)?
                                    .metadata()?
                                    .len()
                                    == 0,
                                "nonempty clean-break writer advisory"
                            );
                            result.file(&[text, collection, writer], 0, true);
                        }
                    } else {
                        ensure!(
                            !absent_collections,
                            "logical collection remains before storage retirement"
                        );
                        let registration = registrations
                            .iter()
                            .find(|row| {
                                Path::new(&row.data_root) == data && row.collection == collection
                            })
                            .context(
                                "orphan clean-break collection requires original source review",
                            )?;
                        crate::service::collection::Collection::existing(
                            data,
                            &registration.root,
                            collection,
                        )?;
                    }
                }
            }
            _ => anyhow::bail!("unknown clean-break storage entry is preserved: {text}"),
        }
    }
    Ok((root, result))
}

/// Complete metadata-only census. It never creates storage, changes permissions,
/// opens publications or reacquires the caller's original native process leases.
pub fn validate_clean_break_storage(
    data: &Path,
    control: &Path,
    namespace: Incarnation,
    registrations: &[ProjectRegistration],
) -> Result<()> {
    roster(data, control, namespace, registrations)?;
    let saved = SetupDirectory::open_canonical(control)?;
    if let Some(receipt) = saved_retirement(&saved, data, control, namespace, registrations)? {
        if !exists(data)? {
            return Ok(());
        }
        let root = SetupDirectory::open_canonical(data)?;
        ensure!(
            root.identity()? == receipt.identity,
            "storage retirement root object changed"
        );
        remaining(&root, &receipt, false)?;
    } else {
        ensure!(
            exists(data)?,
            "storage absence has no admitted retirement proof"
        );
        // This is only a sizing/ownership observation: no receipt is published
        // and no data/process exclusion is minted by this read path.
        let (prospective, _) =
            capture_retirement(data, control, namespace, registrations, false, false)?;
        prospective_budget(&prospective, data, registrations)?;
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetiredFile {
    path: Vec<String>,
    identity: (u64, u64),
    bytes: u64,
    digest: Sha256Digest,
    lock: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetiredDirectory {
    path: Vec<String>,
    identity: (u64, u64),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Retirement {
    schema: u32,
    scope: Scope,
    registrations: Sha256Digest,
    identity: (u64, u64),
    files: Vec<RetiredFile>,
    directories: Vec<RetiredDirectory>,
}
fn registrations_digest(registrations: &[ProjectRegistration]) -> Result<Sha256Digest> {
    let mut registrations = registrations.to_vec();
    registrations.sort_by_key(|row| row.binding);
    Ok(framed_digest(
        b"airk/clean-break-storage-roster/1",
        &[&serde_json::to_vec(&registrations)?],
    ))
}
fn marker(data: &Path, namespace: Incarnation) -> Result<String> {
    Ok(format!(
        "clean-break-storage-{}.json",
        framed_digest(
            b"airk/clean-break-storage/1",
            &[
                data.to_str().context("storage path UTF-8")?.as_bytes(),
                namespace.as_bytes()
            ]
        )
    ))
}
fn valid_path(path: &[String]) -> bool {
    !path.is_empty()
        && path.len() <= 4
        && path.iter().all(|part| {
            !part.is_empty()
                && part.len() <= 255
                && part != "."
                && part != ".."
                && !part.contains(['/', '\\', ':', '\0'])
        })
}
fn single_path(path: &[String], name: &str) -> bool {
    path.len() == 1 && path[0] == name
}
fn directory(root: &SetupDirectory, path: &[String]) -> Result<SetupDirectory> {
    let mut current = root.try_clone()?;
    for part in path {
        current = current.open_dir(OsStr::new(part))?;
        ensure!(
            current.identity()?.0 == root.identity()?.0,
            "storage retirement crosses filesystem"
        );
    }
    Ok(current)
}
fn hash(file: &File, maximum: u64) -> Result<Sha256Digest> {
    let size = file.metadata()?.len();
    ensure!(size <= maximum, "storage retirement file grew");
    // Windows mandatory byte-range locks reject even an EOF read through a
    // different handle. A zero-byte native advisory has exactly known content;
    // observing that fact must not read through its live owner's lock.
    if size == 0 {
        ensure!(file.metadata()?.len() == 0, "storage retirement file grew");
        return Ok(Sha256Digest::from_bytes(Sha256::digest([]).into()));
    }
    let mut bytes = file.try_clone()?;
    use std::io::{Seek, SeekFrom};
    bytes.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 8192];
    let mut read = 0u64;
    loop {
        let count = bytes.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        ensure!(read <= size, "storage retirement file grew");
        hash.update(&buffer[..count]);
    }
    ensure!(read == size, "storage retirement file changed");
    Ok(Sha256Digest::from_bytes(hash.finalize().into()))
}
fn capture_retirement(
    data: &Path,
    control: &Path,
    namespace: Incarnation,
    registrations: &[ProjectRegistration],
    absent_collections: bool,
    hold_locks: bool,
) -> Result<(Retirement, Vec<File>)> {
    let (root, mut inventory) =
        storage_inventory(data, control, namespace, registrations, absent_collections)?;
    ensure!(
        inventory.files.len() + inventory.directories.len() <= MAX_FILES,
        "storage retirement inventory limit"
    );
    ensure!(
        inventory
            .files
            .iter()
            .any(
                |(path, maximum, lock)| single_path(path, ".lifecycle-1.4.lock")
                    && *maximum == 0
                    && *lock
            ),
        "storage retirement requires existing lifecycle exclusion"
    );
    // The root lifecycle comes first: every canonical writer retains its shared
    // counterpart. Never create a missing advisory to manufacture exclusion.
    inventory
        .files
        .sort_by_key(|(path, _, _)| (!single_path(path, ".lifecycle-1.4.lock"), path.clone()));
    let mut files = Vec::new();
    let mut held = Vec::new();
    for (path, maximum, lock) in inventory.files {
        let parent = directory(&root, &path[..path.len() - 1])?;
        let file = parent.open_file(
            OsStr::new(&path[path.len() - 1]),
            if lock && hold_locks {
                SetupFileMode::ReadWrite
            } else {
                SetupFileMode::Read
            },
        )?;
        if lock && hold_locks {
            file.try_lock()
                .context("clean-break storage still has a live owner")?;
        }
        files.push(RetiredFile {
            path,
            identity: file_identity(&file)?,
            bytes: file.metadata()?.len(),
            digest: hash(&file, maximum)?,
            lock,
        });
        held.push(file);
    }
    let directories = inventory
        .directories
        .into_iter()
        .map(|path| {
            Ok(RetiredDirectory {
                identity: directory(&root, &path)?.identity()?,
                path,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let receipt = Retirement {
        schema: 1,
        scope: expected(data, control, namespace)?,
        registrations: registrations_digest(registrations)?,
        identity: root.identity()?,
        files,
        directories,
    };
    // Revalidate the typed census after all existing exclusion handles are
    // held. A newly published reservation or substituted row cannot become a
    // retirement target merely because its filename appeared in an earlier scan.
    let (_, after) =
        storage_inventory(data, control, namespace, registrations, absent_collections)?;
    let before_files = receipt
        .files
        .iter()
        .map(|row| &row.path)
        .collect::<BTreeSet<_>>();
    let after_files = after
        .files
        .iter()
        .map(|(path, _, _)| path)
        .collect::<BTreeSet<_>>();
    let before_directories = receipt
        .directories
        .iter()
        .map(|row| &row.path)
        .collect::<BTreeSet<_>>();
    let after_directories = after.directories.iter().collect::<BTreeSet<_>>();
    ensure!(
        before_files == after_files && before_directories == after_directories,
        "storage retirement census changed during exclusion"
    );
    ensure!(
        serde_json::to_vec(&receipt)?.len() as u64 <= MAX_RECEIPT,
        "storage retirement receipt limit"
    );
    Ok((receipt, held))
}

fn prospective_budget(
    receipt: &Retirement,
    data: &Path,
    registrations: &[ProjectRegistration],
) -> Result<()> {
    let mut entries = receipt.files.len() + receipt.directories.len();
    let mut bytes = serde_json::to_vec(receipt)?.len() as u64;
    let mut needs_writers = false;
    // Collection disposal may create its canonical persistent advisory. Reserve
    // bounded representation space before admission without inventing an inode
    // or publishing any ownership evidence for a file that does not yet exist.
    for registration in registrations
        .iter()
        .filter(|row| Path::new(&row.data_root) == data)
    {
        let path = vec![
            "collections-1.4".to_owned(),
            ".writer-locks".to_owned(),
            registration.collection.clone(),
        ];
        if !receipt.files.iter().any(|row| row.path == path) {
            entries += 1;
            bytes += serde_json::to_vec(&path)?.len() as u64 + 256;
            needs_writers = true;
        }
    }
    if needs_writers
        && !receipt.directories.iter().any(|row| {
            row.path.len() == 2
                && row.path[0] == "collections-1.4"
                && row.path[1] == ".writer-locks"
        })
    {
        entries += 1;
        bytes += 192;
    }
    ensure!(
        entries <= MAX_FILES && bytes <= MAX_RECEIPT,
        "prospective storage retirement inventory or receipt limit"
    );
    Ok(())
}

fn saved_retirement(
    control_handle: &SetupDirectory,
    data: &Path,
    control: &Path,
    namespace: Incarnation,
    registrations: &[ProjectRegistration],
) -> Result<Option<Retirement>> {
    match control_handle.read_file(OsStr::new(&marker(data, namespace)?), MAX_RECEIPT) {
        Ok(bytes) => {
            let receipt: Retirement = serde_json::from_slice(&bytes)?;
            ensure!(
                serde_json::to_vec(&receipt)? == bytes,
                "storage retirement receipt encoding changed"
            );
            validate_receipt(&receipt, data, control, namespace, registrations)?;
            Ok(Some(receipt))
        }
        Err(error) if missing(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

fn validate_receipt(
    receipt: &Retirement,
    data: &Path,
    control: &Path,
    namespace: Incarnation,
    registrations: &[ProjectRegistration],
) -> Result<()> {
    ensure!(
        receipt.schema == 1
            && receipt.scope == expected(data, control, namespace)?
            && receipt.registrations == registrations_digest(registrations)?
            && receipt.files.len() + receipt.directories.len() <= MAX_FILES,
        "storage retirement receipt scope changed"
    );
    ensure!(
        receipt
            .files
            .iter()
            .any(|row| single_path(&row.path, ".lifecycle-1.4.lock") && row.lock && row.bytes == 0)
            && receipt
                .files
                .iter()
                .any(|row| single_path(&row.path, OWNER)),
        "storage retirement receipt lost its ownership fences"
    );
    let mut names = BTreeSet::new();
    for path in receipt
        .files
        .iter()
        .map(|row| &row.path)
        .chain(receipt.directories.iter().map(|row| &row.path))
    {
        ensure!(
            valid_path(path) && names.insert(path),
            "storage retirement receipt path changed"
        );
    }
    Ok(())
}
struct OpenFile {
    parent: SetupDirectory,
    file: File,
    row: usize,
}
fn remaining(
    root: &SetupDirectory,
    receipt: &Retirement,
    hold_locks: bool,
) -> Result<Vec<OpenFile>> {
    let mut stack = vec![(Vec::<String>::new(), root.try_clone()?)];
    let mut files = Vec::new();
    let mut count = 0;
    while let Some((prefix, parent)) = stack.pop() {
        for name in parent.entries()? {
            count += 1;
            ensure!(
                count <= MAX_FILES,
                "storage retirement remaining inventory limit"
            );
            let name = name?;
            let mut path = prefix.clone();
            path.push(
                name.to_str()
                    .context("invalid storage retirement entry")?
                    .into(),
            );
            if let Some((index, row)) = receipt
                .files
                .iter()
                .enumerate()
                .find(|(_, row)| row.path == path)
            {
                let file = parent.open_file(
                    &name,
                    if row.lock && hold_locks {
                        SetupFileMode::ReadWrite
                    } else {
                        SetupFileMode::Read
                    },
                )?;
                if row.lock && hold_locks {
                    file.try_lock()
                        .context("clean-break storage still has a live owner")?;
                }
                ensure!(
                    file_identity(&file)? == row.identity
                        && file.metadata()?.len() == row.bytes
                        && hash(&file, row.bytes)? == row.digest,
                    "storage retirement file identity or bytes changed"
                );
                files.push(OpenFile {
                    parent: parent.try_clone()?,
                    file,
                    row: index,
                });
            } else {
                let expected = receipt
                    .directories
                    .iter()
                    .find(|row| row.path == path)
                    .context("new storage retirement entry is preserved")?;
                let child = parent.open_dir(&name)?;
                ensure!(
                    child.identity()? == expected.identity
                        && child.identity()?.0 == root.identity()?.0,
                    "storage retirement directory changed"
                );
                stack.push((path, child));
            }
        }
    }
    ensure!(
        files.is_empty()
            || files
                .iter()
                .any(|opened| single_path(&receipt.files[opened.row].path, ".lifecycle-1.4.lock")),
        "storage retirement lost lifecycle exclusion before other files were disposed"
    );
    Ok(files)
}

/// Called only after successor readiness and complete old executable retirement.
/// Borrows the actual old service exclusion, owns its other native exclusions,
/// and returns false for bounded disposal. No directory permissions are changed.
pub fn retire_clean_break_storage_turn(
    data: &Path,
    service: &ServiceLease,
    namespace: Incarnation,
    registrations: &[ProjectRegistration],
) -> Result<bool> {
    retire_with_hook(data, service, namespace, registrations, |_| Ok(()))
}
fn retire_with_hook(
    data: &Path,
    service: &ServiceLease,
    namespace: Incarnation,
    registrations: &[ProjectRegistration],
    mut checkpoint: impl FnMut(bool) -> Result<()>,
) -> Result<bool> {
    let control = service.root();
    roster(data, control, namespace, registrations)?;
    let _source = SourceWatchLease::acquire(control)?;
    let _embedding = EmbeddingLease::acquire(control)?;
    let _projects = registrations
        .iter()
        .map(|row| ProjectLease::acquire(control, row.binding))
        .collect::<Result<Vec<_>>>()?;
    roster(data, control, namespace, registrations)?;
    let control_handle = SetupDirectory::open_canonical(control)?;
    let marker = marker(data, namespace)?;
    let receipt = match saved_retirement(&control_handle, data, control, namespace, registrations)?
    {
        Some(receipt) => {
            if !exists(data)? {
                return Ok(true);
            }
            receipt
        }
        None => {
            ensure!(
                exists(data)?,
                "storage absence has no admitted retirement proof"
            );
            let (receipt, held) =
                capture_retirement(data, control, namespace, registrations, true, true)?;
            write_exact(&control_handle, &marker, &serde_json::to_vec(&receipt)?)?;
            // Initial lock custody ends only after the exact immutable roster is
            // durable. The resume verifier reacquires every remaining advisory.
            drop(held);
            checkpoint(false)?;
            receipt
        }
    };
    validate_receipt(&receipt, data, control, namespace, registrations)?;
    let root = SetupDirectory::open_canonical(data)?;
    ensure!(
        root.identity()? == receipt.identity,
        "storage retirement root object changed"
    );
    let mut files = remaining(&root, &receipt, true)?;
    files.sort_by_key(|opened| {
        let path = &receipt.files[opened.row].path;
        (
            if single_path(path, ".lifecycle-1.4.lock") {
                2
            } else if single_path(path, OWNER) {
                1
            } else {
                0
            },
            path.clone(),
        )
    });
    let pending = files.len() > FILES_PER_TURN;
    for (index, opened) in files.into_iter().enumerate() {
        if index == FILES_PER_TURN {
            break;
        }
        ensure!(
            SetupDirectory::open_canonical(data)?.identity()? == receipt.identity,
            "storage retirement root object changed"
        );
        let path = &receipt.files[opened.row].path;
        opened
            .parent
            .unlink_file_if_identity(OsStr::new(&path[path.len() - 1]), &opened.file)?;
        // Native Windows unlink becomes physical only after this exact target's
        // retained handle closes. Keep every other advisory (lifecycle last)
        // alive while exposing the genuine completed-unlink checkpoint.
        drop(opened);
        checkpoint(true)?;
    }
    if pending {
        return Ok(false);
    }
    let mut directories = receipt.directories.iter().collect::<Vec<_>>();
    directories.sort_by_key(|row| std::cmp::Reverse(row.path.len()));
    for row in directories {
        let parent = match directory(&root, &row.path[..row.path.len() - 1]) {
            Ok(parent) => parent,
            Err(error) if missing(&error) => continue,
            Err(error) => return Err(error),
        };
        let name = OsStr::new(&row.path[row.path.len() - 1]);
        let child = match parent.open_dir(name) {
            Ok(child) => child,
            Err(error) if missing(&error) => continue,
            Err(error) => return Err(error),
        };
        ensure!(
            child.identity()? == row.identity,
            "storage retirement directory object changed"
        );
        parent.unlink_empty_dir_if_identity(name, &child)?;
    }
    let parent = SetupDirectory::open_canonical(data.parent().context("storage parent missing")?)?;
    parent
        .unlink_empty_dir_if_identity(data.file_name().context("storage name missing")?, &root)?;
    drop(root);
    ensure!(
        !exists(data)?,
        "storage retirement is not physically complete"
    );
    Ok(true)
}

#[cfg(test)]
#[path = "service_data_scope_clean_break_tests.rs"]
mod tests;
