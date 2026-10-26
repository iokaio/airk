// SPDX-License-Identifier: Apache-2.0
//! Explicit offline 1.3 derived-file retirement; no old database is opened.
use super::{CurrentScope, database, gate};
use crate::{FileStore, quota::Quota, service::registry::ServiceRegistry};
use airk_core::identity::{Incarnation, Sha256Digest, framed_digest};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const MAX_FILES: usize = 65_536;
const MAX_METADATA: u64 = 32 * 1024 * 1024;
const MAX_BYTES: u64 = 1024 * 1024 * 1024 * 1024;
const TURN: usize = 4096;
const MAX_RECEIPTS: usize = 16;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub root: PathBuf,
    pub data_dir: PathBuf,
    pub collection: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Witness {
    bytes: u64,
    modified_ns: String,
    identity: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scope {
    namespace: Incarnation,
    epoch: u64,
    root: PathBuf,
    data_dir: PathBuf,
    directory: PathBuf,
    directory_identity: String,
    owner: CurrentScope,
    files: BTreeMap<String, Witness>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preview {
    pub approval: Sha256Digest,
    pub root: PathBuf,
    pub data_dir: PathBuf,
    pub collection: String,
    pub files: usize,
    pub bytes: u64,
    pub sources_preserved: bool,
    pub settings_preserved: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    version: u32,
    scope: Scope,
    approval: Sha256Digest,
    complete: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    value: Intent,
    sha256: Sha256Digest,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub approval: Sha256Digest,
    pub complete: bool,
    pub remaining_files: usize,
    pub sources_preserved: bool,
    pub settings_preserved: bool,
}
fn digest(value: &impl Serialize) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"legacy-derived-retirement/1.4",
        &[&serde_json::to_vec(value)?],
    ))
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
fn plain(path: &Path) -> Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "linked legacy retirement entry"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "redirected legacy retirement entry"
        );
    }
    ensure!(
        metadata.is_file() || metadata.is_dir(),
        "unsupported legacy retirement entry"
    );
    if metadata.is_file() {
        crate::managed_collection::reject_file_aliases(path, &metadata)?;
    }
    Ok(metadata)
}
pub(crate) fn identity(path: &Path, metadata: &fs::Metadata) -> Result<String> {
    crate::managed_collection::file_identity(path, metadata)
}
pub(crate) fn witness(path: &Path) -> Result<Witness> {
    let metadata = plain(path)?;
    ensure!(metadata.is_file(), "legacy retirement file changed type");
    Ok(Witness {
        bytes: metadata.len(),
        modified_ns: metadata
            .modified()?
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
            .to_string(),
        identity: identity(path, &metadata)?,
    })
}
fn derived_root(name: &str) -> bool {
    matches!(
        name,
        "current.sqlite3"
            | "current.sqlite3-wal"
            | "current.sqlite3-shm"
            | "current.sqlite3-journal"
            | "current-owner.json"
            | "current-bootstrap.json"
            | "current-runtime-bootstrap.json"
            | "objects"
            | "current-metadata"
            | "current-resources"
            | "coverage.json"
    )
}
fn inventory(directory: &Path) -> Result<BTreeMap<String, Witness>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![directory.to_path_buf()];
    let mut visited = 0usize;
    let mut bytes = 0u64;
    while let Some(parent) = pending.pop() {
        ensure!(
            plain(&parent)?.is_dir(),
            "legacy derived directory changed type"
        );
        for entry in fs::read_dir(&parent)? {
            visited += 1;
            ensure!(
                visited <= MAX_FILES,
                "legacy retirement inventory budget exceeded"
            );
            let path = entry?.path();
            let relative = path.strip_prefix(directory)?;
            let parts = relative
                .iter()
                .map(|part| part.to_str().context("non-UTF-8 legacy entry"))
                .collect::<Result<Vec<_>>>()?;
            // Root files outside the derived allowlist are preserved, including all settings/usage.
            if parts.len() == 1 && !derived_root(parts[0]) {
                continue;
            }
            let metadata = plain(&path)?;
            let digest_name = |name: &str| {
                name.len() == 64
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            };
            let known = match parts.as_slice() {
                ["objects" | "current-metadata" | "current-resources"] => metadata.is_dir(),
                [name] => derived_root(name) && metadata.is_file(),
                ["objects", kind] => {
                    metadata.is_dir()
                        && matches!(
                            *kind,
                            "artifacts" | "sources" | "models" | "embedding-packs"
                        )
                }
                ["objects", _, name] => metadata.is_file() && digest_name(name),
                ["current-metadata" | "current-resources", name] => {
                    metadata.is_file() && name.strip_suffix(".json").is_some_and(digest_name)
                }
                _ => false,
            };
            ensure!(
                known,
                "unknown legacy derived entry; preserve it for review"
            );
            if metadata.is_dir() {
                pending.push(path);
            } else {
                let proof = witness(&path)?;
                bytes = bytes
                    .checked_add(proof.bytes)
                    .context("legacy retirement size overflow")?;
                ensure!(bytes <= MAX_BYTES, "legacy retirement byte budget exceeded");
                result.insert(parts.join("/"), proof);
            }
        }
    }
    Ok(result)
}
fn paths(control: &Path, target: &Target) -> Result<(PathBuf, PathBuf, PathBuf)> {
    crate::validate_collection(&target.collection)?;
    ensure!(
        control.is_absolute() && target.root.is_absolute() && target.data_dir.is_absolute(),
        "legacy cleanup requires absolute paths"
    );
    for path in [control, &target.root, &target.data_dir] {
        ensure!(
            path.to_str()
                .is_some_and(|value| value.len() <= 4096 && !value.chars().any(char::is_control)),
            "legacy cleanup path exceeds budget"
        );
        ensure!(
            plain(path)?.is_dir(),
            "legacy cleanup directory unavailable"
        );
    }
    let control = control.canonicalize()?;
    let root = target.root.canonicalize()?;
    let data = target.data_dir.canonicalize()?;
    for (a, b) in [(&root, &data), (&control, &data), (&control, &root)] {
        #[cfg(windows)]
        let (a, b) = (
            PathBuf::from(a.to_string_lossy().to_lowercase()),
            PathBuf::from(b.to_string_lossy().to_lowercase()),
        );
        ensure!(
            !a.as_path().starts_with(b.as_path()) && !b.as_path().starts_with(a.as_path()),
            "legacy cleanup overlaps source or control state"
        );
    }
    let directory = data.join("collections-1.2").join(&target.collection);
    crate::managed_collection::validate_ancestors(&directory)?;
    ensure!(plain(&directory)?.is_dir(), "legacy collection unavailable");
    Ok((control, root, directory.canonicalize()?))
}
fn capture(control: &Path, target: &Target, directory: &Path, root: PathBuf) -> Result<Scope> {
    let (namespace, epoch) = ServiceRegistry::open_saved(control)?.saved_request_epoch()?;
    let owner = database::legacy_retirement_scope(directory)?;
    ensure!(
        Path::new(&owner.root) == root && owner.collection == target.collection,
        "foreign legacy collection owner"
    );
    let data_dir = target.data_dir.canonicalize()?;
    let directory_identity = identity(directory, &plain(directory)?)?;
    Ok(Scope {
        namespace,
        epoch,
        root,
        data_dir,
        directory: directory.into(),
        directory_identity,
        owner,
        files: inventory(directory)?,
    })
}
fn preview_of(scope: &Scope) -> Result<Preview> {
    Ok(Preview {
        approval: digest(scope)?,
        root: scope.root.clone(),
        data_dir: scope.data_dir.clone(),
        collection: scope.owner.collection.clone(),
        files: scope.files.len(),
        bytes: scope.files.values().map(|file| file.bytes).sum(),
        sources_preserved: true,
        settings_preserved: true,
    })
}
/// No Kernel, SQLite connection, old command execution or index conversion.
pub fn preview(control: &Path, target: &Target) -> Result<Preview> {
    let (control, root, directory) = paths(control, target)?;
    let store = FileStore::new(&directory);
    let _worker = crate::scheduler::Queue::new(&target.data_dir)
        .worker_lock(&target.collection)?
        .context("legacy worker is active")?;
    let writer = store.lock()?;
    let _gate = gate::Gate::exclusive(&store, &writer)?;
    preview_of(&capture(&control, target, &directory, root)?)
}
fn read(path: &Path) -> Result<Intent> {
    ensure!(
        plain(path)?.len() <= MAX_METADATA,
        "legacy cleanup intent exceeds budget"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_METADATA + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_METADATA,
        "legacy cleanup intent grew beyond budget"
    );
    let envelope: Envelope = serde_json::from_slice(&bytes)?;
    ensure!(
        envelope.value.version == 1
            && envelope.sha256 == digest(&envelope.value)?
            && serde_json::to_vec(&envelope)? == bytes
            && digest(&envelope.value.scope)? == envelope.value.approval,
        "changed legacy cleanup intent"
    );
    Ok(envelope.value)
}
fn save(path: &Path, intent: Intent) -> Result<()> {
    let prefix = format!(".airk-legacy-derived-{}-", intent.approval);
    let parent = path.parent().context("legacy journal parent missing")?;
    if exists(path)? {
        ensure!(
            plain(path)?.is_file(),
            "changed legacy cleanup receipt type"
        );
    }
    let mut alternate = intent.clone();
    alternate.complete = !alternate.complete;
    let alternate = serde_json::to_vec(&Envelope {
        sha256: digest(&alternate)?,
        value: alternate,
    })?;
    let bytes = serde_json::to_vec(&Envelope {
        sha256: digest(&intent)?,
        value: intent,
    })?;
    ensure!(
        bytes.len() as u64 <= MAX_METADATA,
        "legacy cleanup intent exceeds budget"
    );
    // Reserved scratch must match this exact canonical scope, including a
    // possible truncated write. Unknown bytes and other approvals are preserved.
    for (count, entry) in fs::read_dir(parent)?.enumerate() {
        ensure!(
            count < MAX_RECEIPTS * 2,
            "legacy cleanup scratch budget exceeded"
        );
        let entry = entry?;
        if !entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(&prefix))
        {
            continue;
        }
        ensure!(
            plain(&entry.path())?.len() <= MAX_METADATA,
            "oversized legacy cleanup scratch"
        );
        let partial = fs::read(entry.path())?;
        ensure!(
            bytes.starts_with(&partial) || alternate.starts_with(&partial),
            "foreign legacy cleanup scratch preserved"
        );
        fs::remove_file(entry.path())?;
    }
    let mut file = tempfile::Builder::new()
        .prefix(&prefix)
        .tempfile_in(parent)?;
    file.write_all(&bytes)?;
    crate::durability::persist_synced(file, path)
}
/// Remove at most 4096 captured files per explicitly requested turn.
pub fn remove(control: &Path, target: &Target, approval: Sha256Digest) -> Result<Report> {
    let (control, root, directory) = paths(control, target)?;
    let registry = ServiceRegistry::open_saved(&control)?;
    let namespace = registry.incarnation()?;
    let _journal = crate::service::acquire_file(&control, "legacy-derived-retirement.lock")?;
    let journals = control.join("legacy-derived-retirement-1.4");
    if !journals.try_exists()? {
        fs::create_dir(&journals)?;
    }
    ensure!(
        plain(&journals)?.is_dir(),
        "redirected legacy cleanup journal"
    );
    let path = journals.join(format!("{approval}.json"));
    let saved = if exists(&path)? {
        Some(read(&path)?)
    } else {
        None
    };
    if let Some(intent) = &saved {
        ensure!(
            intent.approval == approval
                && intent.scope.namespace == namespace
                && intent.scope.root == root
                && intent.scope.directory == directory
                && intent.scope.owner.collection == target.collection,
            "foreign legacy cleanup request"
        );
        if intent.complete {
            return Ok(report(approval, true, 0));
        }
    }
    let _worker = crate::scheduler::Queue::new(&target.data_dir)
        .worker_lock(&target.collection)?
        .context("legacy worker is active")?;
    let store = FileStore::new(&directory);
    let writer = store.lock()?;
    let _gate = gate::Gate::exclusive(&store, &writer)?;
    super::release_lexical_cache();
    let mut intent =
        if let Some(intent) = saved {
            intent
        } else {
            let scope = capture(&control, target, &directory, root)?;
            ensure!(digest(&scope)? == approval, "stale legacy cleanup approval");
            let mut count = 0;
            for (visited, entry) in fs::read_dir(&journals)?.enumerate() {
                ensure!(
                    visited < MAX_RECEIPTS * 2,
                    "legacy cleanup scratch budget exceeded"
                );
                let entry = entry?;
                if entry.file_name().to_str().is_some_and(|name| {
                    name.starts_with(&format!(".airk-legacy-derived-{approval}-"))
                }) {
                    continue;
                }
                count += 1;
                ensure!(
                    count < MAX_RECEIPTS,
                    "legacy cleanup receipt capacity exceeded"
                );
                let saved = read(&entry.path())?;
                ensure!(
                    saved.scope.namespace == namespace
                        && entry.file_name().to_str() == Some(&format!("{}.json", saved.approval)),
                    "unknown legacy cleanup journal entry"
                );
            }
            let intent = Intent {
                version: 1,
                scope,
                approval,
                complete: false,
            };
            save(&path, intent)?;
            read(&path)?
        };
    ensure!(
        identity(&directory, &plain(&directory)?)? == intent.scope.directory_identity,
        "legacy collection was replaced"
    );
    let live = inventory(&directory)?;
    for (name, proof) in &live {
        ensure!(
            intent.scope.files.get(name) == Some(proof),
            "legacy derived state changed; preserve it for review"
        );
    }
    let quota = Quota::for_store(&store)?;
    let grants = quota.current_grants(&writer)?;
    ensure!(
        grants
            .iter()
            .all(|grant| grant.scope().incarnation == intent.scope.owner.incarnation),
        "foreign legacy quota allowance"
    );
    // Keep the ownership header until all captured payload and allowances are retired.
    let payload = live
        .keys()
        .filter(|name| name.as_str() != database::OWNER_FILE)
        .collect::<Vec<_>>();
    for name in payload.iter().take(TURN) {
        let path = directory.join(name);
        ensure!(
            witness(&path)? == live[*name],
            "legacy file changed before removal"
        );
        fs::remove_file(path)?;
    }
    crate::maintenance::sync_parent(&directory)?;
    if payload.len() > TURN {
        return Ok(report(approval, false, live.len() - TURN));
    }
    for grant in grants {
        grant.release(&writer)?;
    }
    let owner_path = directory.join(database::OWNER_FILE);
    if let Some(proof) = live.get(database::OWNER_FILE) {
        ensure!(
            witness(&owner_path)? == *proof,
            "legacy owner changed before removal"
        );
        fs::remove_file(owner_path)?;
    }
    crate::maintenance::sync_parent(&directory)?;
    intent.complete = true;
    save(&path, intent)?;
    Ok(report(approval, true, 0))
}
fn report(approval: Sha256Digest, complete: bool, remaining_files: usize) -> Report {
    Report {
        approval,
        complete,
        remaining_files,
        sources_preserved: true,
        settings_preserved: true,
    }
}

/// Bounded metadata-only recovery inventory; never opens a legacy collection.
pub fn receipts(control: &Path) -> Result<serde_json::Value> {
    let control = control.canonicalize()?;
    let namespace = ServiceRegistry::open_saved(&control)?.incarnation()?;
    let directory = control.join("legacy-derived-retirement-1.4");
    if !directory.try_exists()? {
        return Ok(serde_json::json!({"items":[]}));
    }
    ensure!(
        plain(&directory)?.is_dir(),
        "redirected legacy cleanup journal"
    );
    let mut items = Vec::new();
    for entry in fs::read_dir(&directory)? {
        ensure!(
            items.len() < MAX_RECEIPTS,
            "legacy cleanup receipt capacity exceeded"
        );
        let entry = entry?;
        let intent = read(&entry.path())?;
        ensure!(
            intent.scope.namespace == namespace
                && entry.file_name().to_str() == Some(&format!("{}.json", intent.approval)),
            "foreign legacy cleanup receipt"
        );
        items.push(
            serde_json::json!({"preview":preview_of(&intent.scope)?,"complete":intent.complete}),
        );
    }
    items.sort_by_key(|item| {
        item["preview"]["approval"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    });
    Ok(serde_json::json!({"items":items}))
}
pub fn forget(control: &Path, approval: Sha256Digest) -> Result<()> {
    let control = control.canonicalize()?;
    let namespace = ServiceRegistry::open_saved(&control)?.incarnation()?;
    let _journal = crate::service::acquire_file(&control, "legacy-derived-retirement.lock")?;
    let directory = control.join("legacy-derived-retirement-1.4");
    ensure!(
        plain(&directory)?.is_dir(),
        "redirected legacy cleanup journal"
    );
    let path = directory.join(format!("{approval}.json"));
    let intent = read(&path)?;
    ensure!(
        intent.scope.namespace == namespace && intent.approval == approval && intent.complete,
        "only the matching completed legacy cleanup receipt can be forgotten"
    );
    fs::remove_file(path)?;
    crate::maintenance::sync_parent(&directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Result<(tempfile::TempDir, PathBuf, Target, PathBuf)> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let root = base.join("source");
        let data_dir = base.join("data");
        let control = base.join("control");
        let directory = data_dir.join("collections-1.2/legacy");
        for path in [&root, &control, &directory] {
            fs::create_dir_all(path)?;
        }
        fs::write(root.join("original.txt"), b"original source")?;
        fs::write(directory.join("scan-limits.json"), b"saved user settings")?;
        fs::write(directory.join("unknown.txt"), b"unrelated user file")?;
        fs::write(directory.join("usage.json"), b"saved usage history")?;
        fs::write(
            directory.join("current.sqlite3"),
            b"not a database: must never be queried",
        )?;
        let scope = CurrentScope {
            root: root.to_str().unwrap().into(),
            collection: "legacy".into(),
            incarnation: Incarnation::from_bytes([21; 16]),
        };
        database::create_owner(&directory, &scope, 65_536)?;
        ServiceRegistry::open(&control, Incarnation::from_bytes([22; 16]))?;
        Ok((
            owner,
            control,
            Target {
                root,
                data_dir,
                collection: "legacy".into(),
            },
            directory,
        ))
    }
    #[test]
    fn offline_retirement_never_queries_old_index_and_preserves_settings_sources_and_later_work()
    -> Result<()> {
        let (_owner, control, target, directory) = fixture()?;
        let preview = preview(&control, &target)?;
        assert_eq!(preview.files, 2);
        assert!(!control.join("legacy-derived-retirement-1.4").exists());
        let result = remove(&control, &target, preview.approval)?;
        assert!(result.complete);
        assert!(!directory.join("current.sqlite3").exists());
        assert!(!directory.join(database::OWNER_FILE).exists());
        assert_eq!(
            fs::read(target.root.join("original.txt"))?,
            b"original source"
        );
        for (name, bytes) in [
            ("scan-limits.json", b"saved user settings".as_slice()),
            ("unknown.txt", b"unrelated user file"),
            ("usage.json", b"saved usage history"),
        ] {
            assert_eq!(fs::read(directory.join(name))?, bytes);
        }
        // Completion replay must not remove a later intentionally created old index.
        fs::write(directory.join("current.sqlite3"), b"later derived index")?;
        assert!(remove(&control, &target, preview.approval)?.complete);
        assert_eq!(
            fs::read(directory.join("current.sqlite3"))?,
            b"later derived index"
        );
        Ok(())
    }
    #[test]
    fn stale_scope_active_owner_aliases_and_unknown_derived_files_refuse_before_deletion()
    -> Result<()> {
        let (_owner, control, target, directory) = fixture()?;
        let mut overlapping = target.clone();
        overlapping.data_dir = target.root.clone();
        #[cfg(windows)]
        {
            overlapping.data_dir = target.root.to_string_lossy().to_uppercase().into();
        }
        assert!(
            format!("{:#}", preview(&control, &overlapping).unwrap_err())
                .contains("overlaps source")
        );
        let approval = preview(&control, &target)?.approval;
        fs::write(directory.join("current.sqlite3"), b"changed index bytes")?;
        assert!(remove(&control, &target, approval).is_err());
        assert_eq!(
            fs::read(directory.join("current.sqlite3"))?,
            b"changed index bytes"
        );
        let queue = crate::scheduler::Queue::new(&target.data_dir);
        let worker = queue.worker_lock(&target.collection)?.unwrap();
        assert!(preview(&control, &target).is_err());
        drop(worker);
        let store = FileStore::new(&directory);
        let reader = gate::Gate::shared(&store)?;
        assert!(preview(&control, &target).is_err());
        drop(reader);
        fs::hard_link(directory.join("current.sqlite3"), directory.join("alias"))?;
        assert!(preview(&control, &target).is_err());
        fs::remove_file(directory.join("alias"))?;
        fs::create_dir_all(directory.join("objects/sources"))?;
        fs::write(
            directory.join("objects/sources/user-file.txt"),
            b"unknown user content",
        )?;
        assert!(preview(&control, &target).is_err());
        assert!(directory.join(database::OWNER_FILE).exists());
        Ok(())
    }
    #[test]
    fn partial_intent_write_and_partial_payload_removal_resume() -> Result<()> {
        let (_owner, control, target, directory) = fixture()?;
        let approval = preview(&control, &target)?.approval;
        let scope = capture(&control, &target, &directory, target.root.clone())?;
        let journals = control.join("legacy-derived-retirement-1.4");
        fs::create_dir(&journals)?;
        let path = journals.join(format!("{approval}.json"));
        let intent = Intent {
            version: 1,
            scope,
            approval,
            complete: false,
        };
        let encoded = serde_json::to_vec(&Envelope {
            sha256: digest(&intent)?,
            value: intent.clone(),
        })?;
        let scratch = journals.join(format!(".airk-legacy-derived-{approval}-interrupted"));
        fs::write(&scratch, &encoded[..100])?;
        assert!(remove(&control, &target, approval)?.complete);
        assert!(!scratch.exists());
        assert_eq!(receipts(&control)?["items"][0]["complete"], true);
        forget(&control, approval)?;
        assert!(receipts(&control)?["items"].as_array().unwrap().is_empty());
        assert!(remove(&control, &target, approval).is_err());
        // An accepted request can finish after death following payload/owner unlink.
        save(&path, intent)?;
        assert!(remove(&control, &target, approval)?.complete);
        Ok(())
    }
    #[test]
    fn bounded_turns_resume_exact_inventory_and_refuse_changed_survivors() -> Result<()> {
        let (_owner, control, target, directory) = fixture()?;
        let objects = directory.join("objects/sources");
        fs::create_dir_all(&objects)?;
        for n in 0..TURN + 1 {
            fs::write(objects.join(format!("{n:064x}")), b"captured object")?;
        }
        let approval = preview(&control, &target)?.approval;
        let result = remove(&control, &target, approval)?;
        assert!(!result.complete && result.remaining_files > 0);
        assert!(directory.join(database::OWNER_FILE).exists());
        let survivor = fs::read_dir(&objects)?.next().unwrap()?.path();
        fs::write(&survivor, b"new object after interruption")?;
        assert!(remove(&control, &target, approval).is_err());
        assert_eq!(fs::read(survivor)?, b"new object after interruption");
        assert!(directory.join(database::OWNER_FILE).exists());
        Ok(())
    }
    #[test]
    fn foreign_allowance_refuses_before_any_payload_is_removed() -> Result<()> {
        let (_owner, control, target, directory) = fixture()?;
        let store = FileStore::new(&directory);
        let writer = store.lock()?;
        let quota = Quota::for_store(&store)?;
        let grant = quota.reserve_current(
            &writer,
            crate::quota::CurrentGrantScope {
                collection: target.collection.clone(),
                incarnation: Incarnation::from_bytes([99; 16]),
                transaction: Sha256Digest::from_bytes([98; 32]),
            },
            65_536,
        )?;
        drop(writer);
        let approval = preview(&control, &target)?.approval;
        assert!(remove(&control, &target, approval).is_err());
        assert_eq!(
            fs::read(directory.join("current.sqlite3"))?,
            b"not a database: must never be queried"
        );
        let writer = store.lock()?;
        grant.release(&writer)?;
        drop(writer);
        assert!(remove(&control, &target, approval)?.complete);
        Ok(())
    }
    #[test]
    fn interrupted_owner_removal_and_corrupt_completion_do_not_authorize_new_effects() -> Result<()>
    {
        let (_owner, control, target, directory) = fixture()?;
        let approval = preview(&control, &target)?.approval;
        // Persist the exact accepted intent, simulating process death before the first effect.
        let scope = capture(&control, &target, &directory, target.root.clone())?;
        let journals = control.join("legacy-derived-retirement-1.4");
        fs::create_dir(&journals)?;
        let path = journals.join(format!("{approval}.json"));
        save(
            &path,
            Intent {
                version: 1,
                scope,
                approval,
                complete: false,
            },
        )?;
        assert!(remove(&control, &target, approval)?.complete);
        let original = fs::read(&path)?;
        let mut corrupt: serde_json::Value = serde_json::from_slice(&original)?;
        corrupt["value"]["complete"] = false.into();
        fs::write(&path, serde_json::to_vec(&corrupt)?)?;
        fs::write(directory.join("current.sqlite3"), b"later safe bytes")?;
        assert!(remove(&control, &target, approval).is_err());
        assert_eq!(
            fs::read(directory.join("current.sqlite3"))?,
            b"later safe bytes"
        );
        Ok(())
    }
}
