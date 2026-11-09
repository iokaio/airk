// SPDX-License-Identifier: Apache-2.0
//! Bounded full deletion of an inactive, ownership-matched service collection.
use super::*;
use crate::service::ProjectLease;
use crate::service::data_scope::CleanBreakNamespaceOwner;
use airk_core::service::ProjectRegistration;

const CLEAN_BREAK_RETIREMENT: &str = "clean-break-retirement.json";
const MAX_RETIRED_METADATA: usize = 64;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CleanBreakRetirement {
    schema: u32,
    registration: ProjectRegistration,
    original_owner: Owner,
    files: Vec<(String, Sha256Digest)>,
}

fn retirement_file_digest(path: &Path) -> Result<Sha256Digest> {
    use sha2::{Digest, Sha256};
    let metadata = plain(path)?;
    ensure!(metadata.is_file(), "invalid clean-break retirement file");
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 8192];
    let mut read = 0u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        read += count as u64;
        ensure!(read <= metadata.len(), "clean-break retirement file grew");
        hash.update(&buffer[..count]);
    }
    ensure!(
        read == metadata.len(),
        "clean-break retirement file changed"
    );
    Ok(Sha256Digest::from_bytes(hash.finalize().into()))
}

fn validate_retired_metadata_file(
    directory: &Path,
    path: &Path,
    registration: &ProjectRegistration,
) -> Result<()> {
    if path
        .file_name()
        .is_some_and(|name| name == "source-policy.json")
    {
        crate::current::policy_capsule::PolicyCapsule::load(directory)?
            .context("retained policy vanished")?
            .validate_registration(registration)
    } else {
        super::split::validate_retained_capsule_file(path)
    }
}

impl CleanBreakRetirement {
    fn validate(&self, registration: &ProjectRegistration) -> Result<()> {
        registration.validate()?;
        ensure!(
            self.schema == 1
                && self.registration == *registration
                && self.original_owner.version == 140
                && self.original_owner.root == registration.root
                && self.original_owner.data_root == registration.data_root
                && self.original_owner.collection == registration.collection
                && self.files.len() <= MAX_RETIRED_METADATA
                && self.files.iter().enumerate().all(|(index, (name, _))| {
                    (name == "source-policy.json"
                        || name.starts_with("split-state-") && name.ends_with(".json"))
                        && !name.contains(['/', '\\'])
                        && self.files[..index]
                            .iter()
                            .all(|(previous, _)| previous != name)
                }),
            "clean-break retirement owner or metadata scope changed"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_METADATA,
            "clean-break retirement metadata exceeds bound"
        );
        Ok(())
    }
}

pub(super) fn owned_clean_break_scratch(path: &Path, binding: Sha256Digest) -> bool {
    json::<CleanBreakRetirement>(path).is_ok_and(|retirement| {
        retirement.validate(&retirement.registration).is_ok()
            && owner_digest(&retirement.original_owner).is_ok_and(|owner| owner == binding)
    })
}

fn finish_clean_break_metadata(
    directory: &Path,
    registration: &ProjectRegistration,
    retirement: &CleanBreakRetirement,
) -> Result<bool> {
    retirement.validate(registration)?;
    // Validate the entire remaining namespace before unlinking any capsule. A
    // missing captured file means a previous turn already removed exactly it.
    for (count, entry) in fs::read_dir(directory)?.enumerate() {
        ensure!(
            count < MAX_RETIRED_METADATA + 2,
            "clean-break retirement inventory exceeds bound"
        );
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("invalid clean-break retirement filename")?;
        ensure!(
            plain(&path)?.is_file(),
            "foreign clean-break retirement directory"
        );
        if name == CLEAN_BREAK_RETIREMENT {
            continue;
        }
        if name == "service-owner.json" {
            let owner: Owner = json(&path)?;
            ensure!(
                serde_json::to_vec(&owner)? == serde_json::to_vec(&retirement.original_owner)?,
                "clean-break collection owner changed"
            );
            continue;
        }
        let (_, expected) = retirement
            .files
            .iter()
            .find(|(file, _)| file == name)
            .context("unknown clean-break retirement file is preserved")?;
        ensure!(
            retirement_file_digest(&path)? == *expected,
            "clean-break retained capsule changed"
        );
        validate_retired_metadata_file(directory, &path, registration)?;
    }
    for (name, _) in &retirement.files {
        let path = directory.join(name);
        if path.try_exists()? {
            fs::remove_file(path)?;
        }
    }
    let owner = directory.join("service-owner.json");
    if owner.try_exists()? {
        fs::remove_file(owner)?;
    }
    fs::remove_file(directory.join(CLEAN_BREAK_RETIREMENT))?;
    fs::remove_dir(directory)?;
    Ok(true)
}

// The capability supplies an exact private staging directory from a validated
// reservation record, never a prefix match. Captures permit arbitrary private
// filenames; every descendant must remain plain, single-linked, contained and
// on the owning collection's filesystem. Validate the entire batch before unlink.
fn dispose_clean_break_stage_turn(stage: &Path) -> Result<bool> {
    dispose_clean_break_stage_turn_observed(stage, |path| {
        use crate::service::registry::catalog::setup::storage::{SetupDirectory, file_identity};
        if plain(path)?.is_dir() {
            Ok(SetupDirectory::open_canonical(path)?.identity()?.0)
        } else {
            Ok(file_identity(&File::open(path)?)?.0)
        }
    })
}

fn dispose_clean_break_stage_turn_observed(
    stage: &Path,
    mut filesystem: impl FnMut(&Path) -> Result<u64>,
) -> Result<bool> {
    let parent_filesystem = filesystem(stage.parent().context("private staging parent missing")?)?;
    let root = stage.canonicalize()?;
    let mut pending = vec![(stage.to_path_buf(), 0usize)];
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut visited = 0usize;
    while let Some((directory, depth)) = pending.pop() {
        ensure!(depth <= 32, "private staging depth limit");
        ensure!(
            plain(&directory)?.is_dir(),
            "invalid private staging directory"
        );
        ensure!(
            filesystem(&directory)? == parent_filesystem,
            "private staging directory crosses owning filesystem"
        );
        directories.push(directory.clone());
        for entry in fs::read_dir(directory)? {
            visited += 1;
            ensure!(visited <= 4096, "private staging inventory limit");
            let path = entry?.path();
            let metadata = plain(&path)?;
            ensure!(
                filesystem(&path)? == parent_filesystem,
                "private staging entry crosses owning filesystem"
            );
            ensure!(
                path.canonicalize()?.starts_with(&root),
                "private staging escaped owner"
            );
            if metadata.is_dir() {
                pending.push((path, depth + 1));
            } else {
                ensure!(metadata.is_file(), "unknown private staging object");
                files.push(path);
            }
        }
    }
    let more = files.len() > 64;
    for file in files.into_iter().take(64) {
        fs::remove_file(file)?;
    }
    if more {
        return Ok(false);
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        fs::remove_dir(directory)?;
    }
    Ok(true)
}

impl Collection {
    /// Dispose one old collection after the installer has fenced its original
    /// namespace and selected a fresh successor. The caller retains service,
    /// source and project exclusion. Source paths and sibling collections are
    /// never deletion targets. Unknown/active split authority remains refused.
    /// `true` means this exact collection directory is physically absent.
    pub fn purge_for_clean_break_turn(
        registration: &ProjectRegistration,
        lease: &ProjectLease,
    ) -> Result<bool> {
        Self::purge_for_clean_break_inner(registration, lease, None)
    }

    /// Dispose an exact old collection under complete namespace exclusion,
    /// including accepted or interrupted split work. This does not complete jobs.
    pub fn purge_for_clean_break_namespace_turn(
        registration: &ProjectRegistration,
        owner: &CleanBreakNamespaceOwner<'_>,
    ) -> Result<bool> {
        owner.validate_registration(registration)?;
        Self::purge_for_clean_break_inner(
            registration,
            owner.project_lease(registration)?,
            Some(owner),
        )
    }

    fn purge_for_clean_break_inner(
        registration: &ProjectRegistration,
        lease: &ProjectLease,
        namespace: Option<&CleanBreakNamespaceOwner<'_>>,
    ) -> Result<bool> {
        registration.validate()?;
        ensure!(
            lease.identity().0 == registration.binding,
            "foreign clean-break removal lease"
        );
        let data = Path::new(&registration.data_root);
        ensure!(
            data.is_absolute() && Path::new(&registration.root).is_absolute(),
            "clean-break removal requires canonical registration locations"
        );
        match fs::symlink_metadata(data) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
            Err(error) => return Err(error.into()),
            Ok(_) => ensure!(
                plain(data)?.is_dir() && data.canonicalize()? == data,
                "clean-break data root was redirected"
            ),
        }
        let directory = data.join("collections-1.4").join(&registration.collection);
        ensure!(
            !Path::new(&registration.root).starts_with(&directory),
            "clean-break collection contains an original source root"
        );
        crate::managed_collection::validate_ancestors(&directory)?;
        if !directory.try_exists()? {
            return Ok(true);
        }
        ensure!(
            plain(&directory)?.is_dir(),
            "invalid clean-break collection directory"
        );
        let marker = directory.join(CLEAN_BREAK_RETIREMENT);
        if marker.try_exists()? {
            let retirement: CleanBreakRetirement = json(&marker)?;
            return finish_clean_break_metadata(&directory, registration, &retirement);
        }
        if !Self::purge_removed_turn_mode(registration, lease, None, None, namespace)? {
            return Ok(false);
        }
        if !directory.try_exists()? {
            return Ok(true);
        }
        let collection = Self::existing(
            Path::new(&registration.data_root),
            &registration.root,
            &registration.collection,
        )?;
        let mut retirement = CleanBreakRetirement {
            schema: 1,
            registration: registration.clone(),
            original_owner: collection.owner.clone(),
            files: Vec::new(),
        };
        for (count, entry) in fs::read_dir(&directory)?.enumerate() {
            ensure!(
                count <= MAX_RETIRED_METADATA,
                "clean-break retained metadata exceeds bound"
            );
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .context("invalid retained capsule filename")?;
            if name == "service-owner.json" {
                continue;
            }
            if name == "source-policy.json" {
                validate_retired_metadata_file(&directory, &path, registration)?;
            } else {
                ensure!(
                    name.starts_with("split-state-"),
                    "unknown clean-break metadata is preserved"
                );
                validate_retired_metadata_file(&directory, &path, registration)?;
            }
            retirement
                .files
                .push((name.to_owned(), retirement_file_digest(&path)?));
        }
        retirement.files.sort_by(|left, right| left.0.cmp(&right.0));
        retirement.validate(registration)?;
        if namespace.is_some() {
            use crate::service::registry::catalog::setup::storage::{SetupDirectory, write_exact};
            // The complete namespace capability already owns exclusive lifecycle
            // and all credit ledgers. A disposal receipt must not reserve new
            // work or recursively acquire those same quota/relocation locks.
            let target = SetupDirectory::open_canonical(&directory)?;
            write_exact(
                &target,
                CLEAN_BREAK_RETIREMENT,
                &serde_json::to_vec(&retirement)?,
            )?;
        } else {
            write(&collection, CLEAN_BREAK_RETIREMENT, &retirement)?;
        }
        finish_clean_break_metadata(&directory, registration, &retirement)
    }

    pub(crate) fn purge_removed_turn(
        registration: &ProjectRegistration,
        lease: &ProjectLease,
    ) -> Result<bool> {
        Self::purge_removed_turn_inner(registration, lease, None, None)
    }
    pub(crate) fn purge_removed_turn_inner(
        registration: &ProjectRegistration,
        lease: &ProjectLease,
        split_access: Option<&SplitBuildPermit>,
        funding: Option<&crate::quota::funding::FundingUse>,
    ) -> Result<bool> {
        Self::purge_removed_turn_mode(registration, lease, split_access, funding, None)
    }

    fn purge_removed_turn_mode(
        registration: &ProjectRegistration,
        lease: &ProjectLease,
        split_access: Option<&SplitBuildPermit>,
        funding: Option<&crate::quota::funding::FundingUse>,
        namespace: Option<&CleanBreakNamespaceOwner<'_>>,
    ) -> Result<bool> {
        ensure!(
            lease.identity().0 == registration.binding,
            "foreign removal lease"
        );
        let directory = Path::new(&registration.data_root)
            .join("collections-1.4")
            .join(&registration.collection);
        crate::managed_collection::validate_ancestors(&directory)?;
        match fs::symlink_metadata(&directory) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
            Err(error) => return Err(error.into()),
            Ok(_) => (),
        }
        ensure!(
            plain(&directory)?.is_dir(),
            "invalid removed collection directory"
        );
        let policy = crate::current::policy_capsule::PolicyCapsule::load(&directory)?;
        if let Some(policy) = &policy {
            policy.validate_registration(registration)?;
        }
        // Owner is deleted last. A crash after that point may leave only an empty directory.
        if !directory.join("service-owner.json").try_exists()? {
            if policy.is_some() {
                for entry in fs::read_dir(&directory)? {
                    ensure!(
                        entry?.file_name() == "source-policy.json",
                        "unowned removal state is preserved"
                    );
                }
                return Ok(true);
            }
            ensure!(
                fs::read_dir(&directory)?.next().is_none(),
                "unowned removal state is preserved"
            );
            fs::remove_dir(&directory)?;
            return Ok(true);
        }
        let collection = Self::existing(
            Path::new(&registration.data_root),
            &registration.root,
            &registration.collection,
        )?;
        let retained_split = collection.owner.required_split.is_some();
        if let Some(pointer) = &collection.owner.required_split {
            if let Some(namespace) = namespace {
                super::split::validate_capsule_for_clean_break(&directory, pointer, namespace)?;
            } else {
                super::split::validate_capsule_for_retirement(&directory, pointer, split_access)?;
            }
        }
        // A dead funded capture's exact stage remains owned until the native
        // payload-disposal checkpoint below. Recognize ONLY the admitted ledger
        // paths; do not classify a prefix as safe or delete staging early.
        let admitted_staging = if let Some(namespace) = namespace {
            namespace.exact_staging_paths(registration)?
        } else if let Some(funding) = funding {
            let permit =
                split_access.context("funded staging disposal requires claimed cleanup access")?;
            let store = FileStore::new(&directory)
                .with_split_access(permit.clone())?
                .with_operation_funding(funding.clone())?;
            Quota::for_store(&store)?.operation_staging_for_disposal(funding)?
        } else {
            std::collections::BTreeSet::new()
        };
        let _usage = if namespace.is_none() {
            Some(crate::usage::deletion_guard(&directory)?)
        } else {
            None
        };
        collection.sweep_metadata_scratch()?;
        let incarnations = directory.join("incarnations");
        // Prove the complete top-level scope before deleting anything.
        let mut top = Vec::new();
        for (count, entry) in fs::read_dir(&directory)?.enumerate() {
            ensure!(count < 2112, "removal metadata budget exceeded");
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .context("invalid removal entry")?;
            if admitted_staging.contains(&path) {
                ensure!(
                    plain(&path)?.is_dir(),
                    "invalid exact funded staging directory"
                );
                continue;
            }
            if name.starts_with("split-state-") && retained_split {
                super::split::validate_retained_capsule_file(&path)?;
                if let Some(namespace) = namespace {
                    super::split::validate_retained_capsule_for_clean_break(&path, namespace)?;
                }
                continue;
            }
            if name == "incarnations" || name == ".current-gates" {
                ensure!(plain(&path)?.is_dir(), "invalid incarnation root");
                continue;
            }
            if name == "source-policy.json" && policy.is_some() {
                ensure!(plain(&path)?.is_file(), "invalid retained source policy");
                continue;
            }
            ensure!(
                matches!(
                    name,
                    "service-owner.json"
                        | "integrity-observation.json"
                        | "integrity-recovery.json"
                        | "integrity-projection.json"
                        | "active.json"
                        | "replacement.json"
                        | "initial-slot.json"
                        | "derived-cleanup.json"
                        | "manifest.lock"
                        | "replacement.lock"
                        | "usage.lock"
                        | "usage-policy.json"
                        | "usage.sqlite3"
                        | "usage.sqlite3-wal"
                        | "usage.sqlite3-shm"
                        | "usage.sqlite3-journal"
                ),
                "unknown removal entry is preserved: {name}"
            );
            ensure!(plain(&path)?.is_file(), "invalid removal metadata");
            top.push(path);
        }
        if incarnations.try_exists()? {
            let mut slots = Vec::new();
            for (count, entry) in fs::read_dir(&incarnations)?.enumerate() {
                ensure!(count < 16, "removal incarnation budget exceeded");
                let path = entry?.path();
                let slot: Incarnation = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .context("invalid removal slot")?
                    .parse()?;
                ensure!(plain(&path)?.is_dir(), "invalid removal slot directory");
                slots.push((slot, path));
            }
            // The owner binds the entire collection. Each slot additionally uses
            // the existing allowlisted inventory, writer lock and request gate.
            let last_slot = slots.len() == 1;
            if let Some((_, path)) = slots.into_iter().next() {
                if namespace.is_some() {
                    if path.join("current-owner.json").try_exists()? {
                        crate::current::discover_owned_scope_for_disposal(
                            &path,
                            &registration.root,
                            &registration.collection,
                        )?;
                    }
                    let (files, mut directories) = retirement_inventory(&path)?;
                    for file in files
                        .iter()
                        .filter(|file| {
                            file.file_name()
                                .is_none_or(|name| name != "current-owner.json")
                        })
                        .take(64)
                    {
                        fs::remove_file(file)?;
                    }
                    if files.len() > 64 {
                        return Ok(false);
                    }
                    let owner = path.join("current-owner.json");
                    if owner.try_exists()? {
                        fs::remove_file(owner)?;
                    }
                    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
                    for directory in directories {
                        fs::remove_dir(directory)?;
                    }
                    // Old quota rows remain immutable until whole-root retirement.
                    return Ok(false);
                }
                let mut store = FileStore::new(&path);
                if let Some(permit) = split_access {
                    store = store.with_split_access(permit.clone())?;
                }
                if let Some(funding) = funding {
                    store = store.with_operation_funding(funding.clone())?;
                }
                let writer = store.lock()?;
                let incarnation = if path.join("current-owner.json").try_exists()? {
                    Some(
                        store
                            .current_scope(&collection.owner.root, &collection.owner.collection)?
                            .context("removal slot ownership missing")?
                            .incarnation,
                    )
                } else {
                    None
                };
                let gate = Gate::exclusive(&store, &writer)?;
                let incarnation = if funding.is_some() {
                    store.service_incarnation()?
                } else {
                    incarnation
                };
                let quota = Quota::for_store(&store)?;
                if let Some(funding) = funding {
                    ensure!(
                        quota
                            .funded_children(funding)?
                            .iter()
                            .all(|child| child.closed
                                || !matches!(
                                    child.kind,
                                    crate::quota::funding::FundingChildKind::Reservation { .. }
                                )),
                        "split cleanup must explicitly adopt/reconcile captured reservations before disposal"
                    );
                }
                // Refuse a crossed/ordinary quota before deleting a journal or
                // any payload needed to recover an active funded child.
                for grant in quota.current_grants(&writer)? {
                    if (funding.is_none() && last_slot)
                        || Some(grant.scope().incarnation) == incarnation
                    {
                        grant.require_release_authority()?;
                    }
                }
                let (files, mut directories) = retirement_inventory(&path)?;
                let ordinary: Vec<_> = files
                    .iter()
                    .filter(|file| {
                        file.file_name()
                            .is_none_or(|name| name != "current-owner.json")
                    })
                    .collect();
                for file in ordinary.iter().take(4096) {
                    fs::remove_file(file)?;
                }
                // The inventory's extra entry is a continuation sentinel. Keep
                // it even when the preserved owner reduces the deletion count.
                if files.len() > 4096 {
                    return Ok(false);
                }
                if funding.is_some() {
                    store.reconcile_disposed_operation_funding(&writer, &quota, &gate)?;
                }
                for grant in quota.current_grants(&writer)? {
                    if (funding.is_none() && last_slot)
                        || Some(grant.scope().incarnation) == incarnation
                    {
                        grant.release(&writer)?;
                    }
                }
                if path.join("current-owner.json").try_exists()? {
                    fs::remove_file(path.join("current-owner.json"))?;
                }
                directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
                for path in directories {
                    fs::remove_dir(&path)
                        .with_context(|| format!("remove owned directory {}", path.display()))?;
                }
                // Bound each turn to one incarnation; the registry guard persists.
                return Ok(false);
            }
            fs::remove_dir(&incarnations)?;
        }
        // Last-incarnation disposal normally removed these through canonical
        // journal/quota reconciliation on the prior turn. Never report complete
        // from a missing incarnation if its actual funded staging still exists.
        for stage in &admitted_staging {
            if namespace.is_some() && stage.try_exists()? {
                if !dispose_clean_break_stage_turn(stage)? {
                    return Ok(false);
                }
                return Ok(false);
            }
            match fs::symlink_metadata(stage) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
                Ok(_) => anyhow::bail!("admitted funded staging awaits native disposal settlement"),
            }
        }
        let gates = directory.join(".current-gates");
        if gates.try_exists()? {
            let mut gate_files = Vec::new();
            for (count, entry) in fs::read_dir(&gates)?.enumerate() {
                ensure!(count < 4096, "removal gate inventory budget exceeded");
                let path = entry?.path();
                let _: Incarnation = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .context("invalid removal gate")?
                    .parse()?;
                ensure!(plain(&path)?.is_file(), "invalid removal gate file");
                gate_files.push(path);
            }
            for path in gate_files {
                let file = fs::OpenOptions::new().read(true).write(true).open(&path)?;
                file.try_lock()?;
                fs::remove_file(path)?;
            }
            fs::remove_dir(gates)?;
        }
        top.sort_by_key(|path| {
            path.file_name()
                .is_some_and(|name| name == "service-owner.json")
        });
        for path in top {
            if (policy.is_some() || retained_split)
                && path
                    .file_name()
                    .is_some_and(|name| name == "service-owner.json")
            {
                continue;
            }
            fs::remove_file(path)?;
        }
        if policy.is_none() && !retained_split {
            fs::remove_dir(&directory)?;
        }
        Ok(true)
    }
}

#[cfg(test)]
#[path = "service_collection_clean_break_purge_tests.rs"]
mod clean_break_tests;
