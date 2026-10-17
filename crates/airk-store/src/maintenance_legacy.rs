// SPDX-License-Identifier: Apache-2.0
//! Legacy searchable representations retain their original revision namespace.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LegacyRevision {
    pub revision: String,
    pub bytes: Option<u64>,
    pub reasons: Vec<ProtectionReason>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Component {
    hash: String,
    bytes: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    version: u32,
    revision: String,
    collection: String,
    authorized_head: RevisionToken,
    components: BTreeMap<String, Component>,
}

pub(super) fn paths(store: &FileStore) -> Result<Vec<std::path::PathBuf>> {
    let base = store.directory().join("maintenance/legacy-intents");
    if !base.exists() {
        return Ok(vec![]);
    }
    plain(base.parent().unwrap())?;
    plain(&base)?;
    let mut paths = vec![];
    for entry in fs::read_dir(base)? {
        let entry = entry?;
        ensure!(
            paths.len() < MAX_INTENTS && entry.file_type()?.is_file(),
            "invalid legacy retirement intent set"
        );
        ensure!(
            entry
                .file_name()
                .to_str()
                .and_then(|s| s.strip_suffix(".json"))
                .is_some_and(crate::blobs::valid_digest),
            "invalid legacy retirement name"
        );
        paths.push(entry.path());
    }
    paths.sort();
    Ok(paths)
}

pub(super) fn revisions(store: &FileStore) -> Result<Vec<String>> {
    paths(store)?
        .iter()
        .map(|path| {
            let intent: Intent = read_json(path)?.context("missing legacy intent")?;
            validate(store, path, &intent)?;
            Ok(intent.revision)
        })
        .collect()
}

fn validate(store: &FileStore, path: &Path, intent: &Intent) -> Result<()> {
    let active = current_receipt(store)?;
    ensure!(
        intent.version == 1
            && crate::blobs::valid_digest(&intent.revision)
            && intent.collection == collection(store)?
            && path
                == store
                    .directory()
                    .join("maintenance/legacy-intents")
                    .join(format!("{}.json", intent.revision))
            && intent.authorized_head.incarnation == active.token.incarnation
            && intent.authorized_head.generation <= active.token.generation
            && !intent.components.is_empty()
            && intent.components.len() <= 64,
        "foreign or malformed legacy retirement intent"
    );
    crate::sqlite::validate_retained_token(store.directory(), &intent.authorized_head)?;
    for (name, component) in &intent.components {
        ensure!(
            !name.is_empty()
                && !name.contains('\\')
                && name
                    .split('/')
                    .all(|p| !p.is_empty() && p != "." && p != "..")
                && Path::new(name)
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_)))
                && crate::blobs::valid_digest(&component.hash),
            "unsafe legacy component identity"
        );
    }
    Ok(())
}

pub(super) fn snapshot(
    store: &FileStore,
    directory: &Path,
    work: &mut WorkBudget,
) -> Result<Option<airk_core::Snapshot>> {
    plain(directory)?;
    let path = directory.join("manifest.json");
    plain(&path)?;
    work.metadata(path.metadata()?.len())?;
    let mut manifest_bytes = Vec::new();
    File::open(&path)?
        .take(131_073)
        .read_to_end(&mut manifest_bytes)?;
    ensure!(
        manifest_bytes.len() <= 131_072,
        "legacy inspection manifest exceeds bound"
    );
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
    if matches!(manifest["schema"].as_u64(), Some(2 | 3)) {
        return Ok(None);
    }
    let revision = directory
        .file_name()
        .and_then(|s| s.to_str())
        .context("legacy revision missing")?;
    ensure!(
        manifest["schema"].as_u64() == Some(1)
            && manifest["revision"].as_str() == Some(revision)
            && manifest["collection"].as_str() == Some(collection(store)?.as_str()),
        "foreign legacy generation"
    );
    let engine = manifest["engine"]
        .as_str()
        .context("legacy engine missing")?;
    ensure!(
        engine.starts_with("tantivy-")
            && (engine.ends_with("/generation-3") || engine.ends_with("/generation-2")),
        "unsupported legacy generation engine"
    );
    let path = directory.join("inventory.json");
    plain(&path)?;
    let length = path.metadata()?.len();
    work.metadata(length)?;
    ensure!(
        length <= crate::blobs::MAX_CATALOG_BYTES,
        "legacy catalog exceeds bound"
    );
    let mut bytes = vec![];
    File::open(path)?.take(length + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 == length, "legacy catalog changed");
    let mut hash = Sha256::new();
    hash.update(engine.as_bytes());
    hash.update(&bytes);
    ensure!(
        format!("{:x}", hash.finalize()) == revision,
        "legacy catalog revision mismatch"
    );
    let snapshot: airk_core::Snapshot = serde_json::from_slice(&bytes)?;
    ensure!(
        snapshot.collection == collection(store)?
            && snapshot.schema_version == airk_core::SCHEMA_VERSION,
        "legacy catalog scope mismatch"
    );
    Ok(Some(snapshot))
}

pub(super) fn preview(
    store: &FileStore,
    protect_predecessor: bool,
    work: &mut WorkBudget,
) -> Result<(Vec<LegacyRevision>, Vec<LegacyRevision>, bool)> {
    let mut candidates = vec![];
    let mut protected = vec![];
    let mut truncated = false;
    for entry in fs::read_dir(store.directory().join("generations"))? {
        if work.expired() {
            truncated = true;
            break;
        }
        work.entry()?;
        let entry = entry?;
        if !entry
            .file_name()
            .to_str()
            .is_some_and(crate::blobs::valid_digest)
        {
            continue;
        }
        let path = entry.path();
        if snapshot(store, &path, work)?.is_none() {
            continue;
        }
        let lease = crate::retention::exclusive(&path)?;
        let mut reasons = vec![];
        if protect_predecessor {
            reasons.push(ProtectionReason::Predecessor);
        }
        if lease.is_none() {
            reasons.push(ProtectionReason::Reader);
        }
        let entry = LegacyRevision {
            revision: path.file_name().unwrap().to_str().unwrap().into(),
            bytes: if work.planning && !reasons.is_empty() {
                None
            } else {
                tree_bytes(&path, work)?
            },
            reasons,
        };
        if entry.reasons.is_empty() {
            if candidates.len() < work.limit.max_candidates {
                candidates.push(entry);
            } else {
                truncated = true;
            }
            if work.planning {
                break;
            }
        } else if protected.len() < work.limit.max_candidates {
            protected.push(entry);
        } else {
            truncated = true;
        }
    }
    Ok((candidates, protected, truncated))
}

fn components(directory: &Path, work: &mut WorkBudget) -> Result<BTreeMap<String, Component>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(path) = pending.pop() {
        plain(&path)?;
        for entry in fs::read_dir(path)? {
            work.entry()?;
            let entry = entry?;
            ensure!(!entry.file_type()?.is_symlink(), "linked legacy component");
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
                continue;
            }
            ensure!(
                entry.file_type()?.is_file() && result.len() < 64,
                "invalid legacy component set"
            );
            let relative = entry
                .path()
                .strip_prefix(directory)?
                .to_str()
                .context("invalid component path")?
                .replace('\\', "/");
            let mut input = File::open(entry.path())?;
            let bytes = input.metadata()?.len();
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 65536];
            loop {
                if work.expired() {
                    return Err(WorkLimit("time").into());
                }
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
            result.insert(
                relative,
                Component {
                    hash: format!("{:x}", hash.finalize()),
                    bytes,
                },
            );
        }
    }
    Ok(result)
}

fn verify_directories(
    directory: &Path,
    expected: &BTreeMap<String, Component>,
    work: &mut WorkBudget,
) -> Result<()> {
    let mut pending = vec![directory.to_path_buf()];
    while let Some(parent) = pending.pop() {
        plain(&parent)?;
        for entry in fs::read_dir(parent)? {
            work.entry()?;
            if work.expired() {
                return Err(WorkLimit("time").into());
            }
            let entry = entry?;
            ensure!(!entry.file_type()?.is_symlink(), "linked legacy directory");
            if entry.file_type()?.is_dir() {
                let path = entry.path();
                let relative = path
                    .strip_prefix(directory)?
                    .to_str()
                    .context("invalid legacy directory")?
                    .replace('\\', "/");
                ensure!(
                    expected
                        .keys()
                        .any(|name| name.starts_with(&format!("{relative}/"))),
                    "unknown legacy detached directory"
                );
                pending.push(path);
            }
        }
    }
    Ok(())
}

pub(super) fn maintain(
    store: &FileStore,
    active: &PublicationReceipt,
    candidates: &[LegacyRevision],
    reservation: &Reservation,
    work: &mut WorkBudget,
    report: &mut MaintenanceReceipt,
) -> Result<()> {
    let mut pending = paths(store)?;
    for candidate in candidates {
        if work.expired() || report.examined >= work.limit.max_candidates {
            report.pending = true;
            break;
        }
        let path = store
            .directory()
            .join("maintenance/legacy-intents")
            .join(format!("{}.json", candidate.revision));
        if pending.contains(&path) {
            continue;
        }
        if candidate
            .bytes
            .is_none_or(|bytes| bytes > work.limit.max_removal_bytes.saturating_sub(work.removed))
        {
            report.pending = true;
            continue;
        }
        let source = store
            .directory()
            .join("generations")
            .join(&candidate.revision);
        let Some(_lease) = crate::retention::exclusive(&source)? else {
            continue;
        };
        ensure!(
            snapshot(store, &source, work)?.is_some(),
            "legacy candidate changed format"
        );
        let intent = Intent {
            version: 1,
            revision: candidate.revision.clone(),
            collection: collection(store)?,
            authorized_head: active.token,
            components: components(&source, work)?,
        };
        verify_directories(&source, &intent.components, work)?;
        ensure!(pending.len() < MAX_INTENTS, "legacy intent bound reached");
        write_json(store, &path, &intent, reservation)?;
        pending.push(path);
    }
    for path in pending {
        if work.expired() || report.examined >= work.limit.max_candidates {
            report.pending = true;
            break;
        }
        work.metadata(path.metadata()?.len())?;
        let intent: Intent = read_json(&path)?.context("legacy retirement intent missing")?;
        validate(store, &path, &intent)?;
        let source = store.directory().join("generations").join(&intent.revision);
        let detached = store
            .directory()
            .join("generations")
            .join(format!("legacy-removing-{}", intent.revision));
        let Some(_lease) = crate::retention::exclusive(&source)? else {
            report.pending = true;
            continue;
        };
        report.examined += 1;
        ensure!(
            !(source.exists() && detached.exists()),
            "conflicting legacy retirement representations"
        );
        if source.exists() {
            ensure!(
                snapshot(store, &source, work)?.is_some(),
                "invalid legacy retirement source"
            );
            let actual = components(&source, work)?;
            verify_directories(&source, &intent.components, work)?;
            ensure!(
                serde_json::to_vec(&actual)? == serde_json::to_vec(&intent.components)?,
                "legacy source changed after authorization"
            );
            fs::rename(&source, &detached)?;
            sync_parent(source.parent().unwrap())?;
        }
        if detached.exists() {
            let actual = components(&detached, work)?;
            verify_directories(&detached, &intent.components, work)?;
            for (name, component) in &actual {
                let expected = intent
                    .components
                    .get(name)
                    .context("unknown detached legacy component")?;
                ensure!(
                    component.hash == expected.hash && component.bytes == expected.bytes,
                    "legacy detached component mismatch"
                );
            }
            let mut stopped = false;
            for (name, component) in actual {
                if work.expired()
                    || component.bytes > work.limit.max_removal_bytes.saturating_sub(work.removed)
                {
                    stopped = true;
                    break;
                }
                fs::remove_file(detached.join(name))?;
                work.removed += component.bytes;
            }
            if stopped {
                report.pending = true;
                continue;
            }
            // Every remaining entry was verified; remove only now-empty directories.
            let mut dirs = vec![detached.clone()];
            let mut index = 0;
            while index < dirs.len() {
                for entry in fs::read_dir(&dirs[index])? {
                    let entry = entry?;
                    ensure!(
                        entry.file_type()?.is_dir() && !entry.file_type()?.is_symlink(),
                        "unexpected legacy remainder"
                    );
                    dirs.push(entry.path());
                }
                index += 1;
            }
            for directory in dirs.into_iter().rev() {
                fs::remove_dir(directory)?;
            }
            sync_parent(detached.parent().unwrap())?;
            report.removed += 1;
        }
        fs::remove_file(&path)?;
        sync_parent(path.parent().unwrap())?;
    }
    report.reclaimed_bytes = work.removed;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_legacy_inventory_and_component_hashes_are_verified() -> Result<()> {
        let root = tempfile::tempdir()?;
        let store = FileStore::new(root.path().join("collections/fixture"));
        let mut original = airk_core::Snapshot::empty("original-source-root".into());
        original.collection = "fixture".into();
        let bytes = serde_json::to_vec(&original)?;
        let engine = "tantivy-0.26.2/diskann-0.59.0/airk-file-1/generation-3";
        let mut hash = Sha256::new();
        hash.update(engine.as_bytes());
        hash.update(&bytes);
        let revision = format!("{:x}", hash.finalize());
        let directory = store.directory().join("generations").join(&revision);
        fs::create_dir_all(directory.join("lexical"))?;
        fs::write(directory.join("inventory.json"), &bytes)?;
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec(
                &serde_json::json!({"schema":1,"revision":revision,"collection":"fixture","engine":engine}),
            )?,
        )?;
        fs::write(
            directory.join("lexical/original.idx"),
            b"original bounded legacy index fixture",
        )?;
        let mut work = WorkBudget::new(MaintenanceBudget::default())?;
        assert_eq!(
            snapshot(&store, &directory, &mut work)?.unwrap().root,
            original.root
        );
        let anchored = components(&directory, &mut work)?;
        assert_eq!(anchored.len(), 3);
        verify_directories(&directory, &anchored, &mut work)?;
        fs::create_dir(directory.join("unknown-empty"))?;
        assert!(verify_directories(&directory, &anchored, &mut work).is_err());
        assert!(directory.join("inventory.json").exists());
        fs::remove_dir(directory.join("unknown-empty"))?;
        fs::write(
            directory.join("lexical/original.idx"),
            b"changed index fixture",
        )?;
        let changed = components(&directory, &mut work)?;
        assert_ne!(
            anchored["lexical/original.idx"].hash,
            changed["lexical/original.idx"].hash
        );
        fs::write(
            directory.join("inventory.json"),
            b"{\"collection\":\"foreign\"}",
        )?;
        assert!(snapshot(&store, &directory, &mut work).is_err());
        Ok(())
    }
}
