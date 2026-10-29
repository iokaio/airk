// SPDX-License-Identifier: Apache-2.0
//! Offline, resumable rename of the installed default index root. Source bytes are never opened.
use super::*;
use crate::service::{EmbeddingLease, ServiceLease, SourceWatchLease, collection, data_scope};
use rusqlite::types::Value as SqlValue;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Write};

const INTENT: &str = "index-root-migration.json";
const RESULT: &str = "index-root-layout.json";
const MAX_PLAN: u64 = 16 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Edit {
    relative: PathBuf,
    before: Vec<u8>,
    after: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Migration {
    schema: u32,
    namespace: Incarnation,
    source: PathBuf,
    destination: PathBuf,
    replacements: BTreeMap<String, String>,
    edits: Vec<Edit>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Completed {
    schema: u32,
    namespace: Incarnation,
    source: PathBuf,
    destination: PathBuf,
}
fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let parent = path.parent().context("migration metadata parent missing")?;
    ensure!(
        parent.canonicalize()? == parent,
        "redirected migration metadata ancestry"
    );
    let metadata = fs::symlink_metadata(path)?;
    crate::managed_collection::reject_file_aliases(path, &metadata)?;
    ensure!(
        metadata.len() <= limit,
        "index migration metadata exceeds allowance"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "index migration metadata grew beyond allowance"
    );
    Ok(bytes)
}
fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("migration metadata parent missing")?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    crate::durability::persist_synced(file, path)
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
pub fn require_settled(control: &Path) -> Result<()> {
    ensure!(
        !exists(&control.join(INTENT))?,
        "index directory migration is pending; resume installation"
    );
    Ok(())
}
pub fn needed(control: &Path, source: &Path) -> Result<bool> {
    let registry = ServiceRegistry::open_saved(control)?;
    Ok(registrations(&registry)?
        .iter()
        .any(|value| Path::new(&value.data_root) == source))
}
/// A completed receipt selects the renamed default, independently of historical install locations.
pub fn selected(control: &Path, source: &Path, destination: &Path) -> Result<bool> {
    require_settled(control)?;
    let path = control.join(RESULT);
    if !exists(&path)? {
        return Ok(false);
    }
    let value: Completed = serde_json::from_slice(&read(&path, 16384)?)?;
    ensure!(
        value.schema == 1
            && value.namespace == retained_namespace(control)?
            && value.source == source
            && value.destination == destination,
        "index directory migration receipt differs from the installed layout"
    );
    ensure!(
        !exists(source)?,
        "historical index directory reappeared after migration"
    );
    data_scope::check(destination, control, value.namespace)?;
    Ok(true)
}

fn replacements_in_json(
    value: &serde_json::Value,
    map: &BTreeMap<String, String>,
    found: &mut BTreeMap<String, String>,
) {
    match value {
        serde_json::Value::String(text) => {
            if let Some(next) = map.get(text) {
                found.insert(text.clone(), next.clone());
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                replacements_in_json(value, map, found);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                replacements_in_json(value, map, found);
            }
        }
        _ => {}
    }
}
// Preserve canonical struct field order and all unrelated historical text.
fn rewrite(bytes: &[u8], map: &BTreeMap<String, String>) -> Result<Vec<u8>> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Ok(bytes.to_vec());
    };
    let mut found = BTreeMap::new();
    replacements_in_json(&value, map, &mut found);
    let mut text = std::str::from_utf8(bytes)?.to_owned();
    for (before, after) in found {
        text = text.replace(
            &serde_json::to_string(&before)?,
            &serde_json::to_string(&after)?,
        );
    }
    Ok(text.into_bytes())
}
fn sql_value(value: SqlValue, map: &BTreeMap<String, String>) -> Result<SqlValue> {
    Ok(match value {
        SqlValue::Text(value) => SqlValue::Text(match map.get(&value) {
            Some(next) => next.clone(),
            None => String::from_utf8(rewrite(value.as_bytes(), map)?)?,
        }),
        SqlValue::Blob(value) => SqlValue::Blob(rewrite(&value, map)?),
        other => other,
    })
}
fn quoted(name: &str) -> Result<String> {
    ensure!(
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'),
        "unexpected service metadata identifier"
    );
    Ok(format!("\"{name}\""))
}
fn update_registry(connection: &mut Connection, map: &BTreeMap<String, String>) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.pragma_update(None, "defer_foreign_keys", true)?;
    let tables = transaction.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")?
        .query_map([], |row| row.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        tables.len() <= 64,
        "service migration table allowance exceeded"
    );
    for table in tables {
        let table = quoted(&table)?;
        let columns = transaction
            .prepare(&format!("PRAGMA table_info({table})"))?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(
            columns.len() <= 32,
            "service migration column allowance exceeded"
        );
        for column in columns {
            let column = quoted(&column)?;
            let values = transaction
                .prepare(&format!("SELECT DISTINCT {column} FROM {table}"))?
                .query_map([], |row| row.get::<_, SqlValue>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for before in values {
                let after = sql_value(before.clone(), map)?;
                if after != before {
                    transaction.execute(
                        &format!("UPDATE {table} SET {column}=?1 WHERE {column}=?2"),
                        params![after, before],
                    )?;
                }
            }
        }
    }
    bump(&transaction)?;
    let invalid = transaction
        .prepare("PRAGMA foreign_key_check")?
        .query([])?
        .next()?
        .is_some();
    ensure!(
        !invalid,
        "index relocation left a crossed service reference"
    );
    transaction.commit()?;
    Ok(())
}

fn registrations(registry: &ServiceRegistry) -> Result<Vec<ProjectRegistration>> {
    let values = registry
        .connection
        .prepare("SELECT payload FROM projects ORDER BY binding LIMIT 1025")?
        .query_map([], |row| row.get::<_, Vec<u8>>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        values.len() <= MAX_PROJECTS,
        "index migration project allowance exceeded"
    );
    values
        .into_iter()
        .map(|bytes| {
            let value: ProjectRegistration = serde_json::from_slice(&bytes)?;
            value.validate()?;
            Ok(value)
        })
        .collect()
}

fn plan(registry: &ServiceRegistry, source: &Path, destination: &Path) -> Result<Migration> {
    let namespace = registry.incarnation()?;
    let (before, after) =
        data_scope::relocated_owner(source, destination, &registry.root, namespace)?;
    let mut edits = vec![Edit {
        relative: PathBuf::from(data_scope::OWNER),
        before,
        after: after.clone(),
    }];
    if exists(&source.join(data_scope::PART))? {
        edits.push(Edit {
            relative: PathBuf::from(data_scope::PART),
            before: read(&source.join(data_scope::PART), 16384)?,
            after,
        });
    }
    let mut replacements = BTreeMap::from([(
        source
            .to_str()
            .context("non-UTF-8 migration source")?
            .into(),
        destination
            .to_str()
            .context("non-UTF-8 migration destination")?
            .into(),
    )]);
    let bindings = registry.bindings()?;
    for existing in registrations(registry)? {
        if Path::new(&existing.data_root) != source {
            continue;
        }
        let next = ProjectRegistration::new(
            existing.root.clone(),
            destination
                .to_str()
                .context("non-UTF-8 migration destination")?
                .into(),
            existing.collection.clone(),
        )?;
        ensure!(
            !bindings.contains(&next.binding),
            "relocated project identity is already registered"
        );
        source_isolation::validate(&registry.connection, &registry.root, &next, &[])?;
        replacements.insert(existing.binding.to_string(), next.binding.to_string());
    }
    for (relative, before, after) in collection::relocation_owners(source, destination)? {
        edits.push(Edit {
            relative,
            before,
            after,
        });
    }
    Ok(Migration {
        schema: 1,
        namespace,
        source: source.into(),
        destination: destination.into(),
        replacements,
        edits,
    })
}

/// Caller holds native installation/process ownership. This adapter also fences service,
/// helper, source, project and data writers before inspecting or changing derived state.
pub fn relocate(lease: &ServiceLease, source: &Path, destination: &Path) -> Result<()> {
    relocate_at(lease, source, destination, |_| Ok(()))
}
fn relocate_at(
    lease: &ServiceLease,
    source: &Path,
    destination: &Path,
    checkpoint: impl Fn(&str) -> Result<()>,
) -> Result<()> {
    let control = lease.root();
    ensure!(
        source.is_absolute()
            && destination.is_absolute()
            && source != destination
            && source.parent() == destination.parent(),
        "index migration requires distinct sibling roots"
    );
    ensure!(
        data_scope::canonical_location(source)? == source
            && data_scope::canonical_location(destination)? == destination,
        "index migration root is redirected"
    );
    let _embedding = EmbeddingLease::acquire(control)?;
    let _source = SourceWatchLease::acquire(control)?;
    let mut registry = ServiceRegistry::open(control, retained_namespace(control)?)?;
    let bindings = registry.bindings()?;
    let _projects = bindings
        .iter()
        .map(|id| ProjectLease::acquire(control, *id))
        .collect::<Result<Vec<_>>>()?;
    let intent_path = control.join(INTENT);
    if !exists(&intent_path)? && selected(control, source, destination)? {
        return Ok(());
    }
    let physical = if exists(source)? { source } else { destination };
    let _data = crate::quota::service_lifecycle_lease(physical, true)?;
    let _legacy_data = crate::quota::lifecycle_lease(physical, true)?;
    let _claim = crate::service::acquire_file(physical, data_scope::LOCK)?;
    let migration = if exists(&intent_path)? {
        let value: Migration = serde_json::from_slice(&read(&intent_path, MAX_PLAN)?)?;
        ensure!(
            value.schema == 1
                && value.namespace == registry.incarnation()?
                && value.source == source
                && value.destination == destination,
            "pending index migration belongs to another layout"
        );
        value
    } else {
        ensure!(
            exists(source)? && !exists(destination)?,
            "index migration destination exists; both directories were preserved"
        );
        plan(&registry, source, destination)?
    };
    let _next_projects = migration
        .replacements
        .iter()
        .filter_map(|(old, new)| {
            old.parse::<ProjectBinding>()
                .ok()
                .and_then(|_| new.parse::<ProjectBinding>().ok())
        })
        .filter(|id| !bindings.contains(id))
        .map(|id| ProjectLease::acquire(control, id))
        .collect::<Result<Vec<_>>>()?;
    if !exists(&intent_path)? {
        let bytes = serde_json::to_vec(&migration)?;
        ensure!(
            bytes.len() as u64 <= MAX_PLAN,
            "index migration plan exceeds allowance"
        );
        let growth = migration
            .edits
            .iter()
            .map(|edit| edit.after.len().saturating_sub(edit.before.len()) as u64)
            .sum::<u64>();
        let temporary = migration
            .edits
            .iter()
            .map(|edit| edit.after.len() as u64)
            .max()
            .unwrap_or(0);
        let peak = growth + temporary + 4096;
        let mut collections = BTreeMap::new();
        for edit in &migration.edits {
            let parts = edit.relative.iter().collect::<Vec<_>>();
            if parts.len() == 3 && parts[0] == "collections-1.4" {
                collections.insert(
                    parts[1]
                        .to_str()
                        .context("non-UTF-8 migrated collection")?
                        .to_owned(),
                    edit.after.len().saturating_sub(edit.before.len()) as u64
                        + edit.after.len() as u64,
                );
            }
        }
        // Lifecycle EX excludes competing allocations; the persisted intent and
        // root barrier retain that exclusion across interruption and handle close.
        crate::quota::Quota::open_service(physical)?.service_rename_capacity(peak, &collections)?;
        let journal = fs::metadata(control.join("service-1.4.sqlite3"))?.len();
        let required = peak + bytes.len() as u64 + journal;
        crate::capacity::require_growth(physical, required)?;
        crate::capacity::require_growth(control, required)?;
        write(&intent_path, &bytes)?;
    }
    checkpoint("planned")?;
    let marker = serde_json::to_vec(
        &serde_json::json!({"version":1,"kind":"installed-index-rename","destination":destination,"namespace":migration.namespace}),
    )?;
    let marker_name = ".relocation-in-progress.json";
    let marker_path = physical.join(marker_name);
    if exists(&marker_path)? {
        ensure!(
            read(&marker_path, 16384)? == marker,
            "another data relocation is pending"
        );
    } else {
        write(&marker_path, &marker)?;
    }
    // Windows cannot rename a directory while descendant ownership files are
    // open. Native process leases remain held, and the durable root marker
    // rejects new allocation/read admission while these file handles close.
    drop((_claim, _legacy_data, _data));
    if exists(source)? {
        ensure!(
            !exists(destination)?,
            "index migration destination appeared; both directories were preserved"
        );
        rename_new(source, destination)?;
        crate::maintenance::sync_parent(source.parent().context("migration parent missing")?)?;
    }
    checkpoint("renamed")?;
    for edit in &migration.edits {
        ensure!(
            !edit.relative.is_absolute()
                && edit
                    .relative
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_))),
            "migration metadata path escaped its data root"
        );
        let path = destination.join(&edit.relative);
        let current = read(&path, 32768)?;
        if current != edit.after {
            ensure!(
                current == edit.before,
                "index migration metadata changed; original intent retained"
            );
            write(&path, &edit.after)?;
        }
        checkpoint("metadata")?;
    }
    update_registry(&mut registry.connection, &migration.replacements)?;
    checkpoint("registry")?;
    data_scope::check(destination, control, migration.namespace)?;
    for registration in registrations(&registry)? {
        registration.validate()?;
        if Path::new(&registration.data_root) == destination
            && exists(
                &destination
                    .join("collections-1.4")
                    .join(&registration.collection)
                    .join("service-owner.json"),
            )?
        {
            collection::Collection::existing(
                destination,
                &registration.root,
                &registration.collection,
            )?
            .validate_metadata()?;
        }
    }
    ensure!(
        read(&destination.join(marker_name), 16384)? == marker,
        "index relocation barrier changed"
    );
    fs::remove_file(destination.join(marker_name))?;
    crate::maintenance::sync_parent(destination)?;
    write(
        &control.join(RESULT),
        &serde_json::to_vec(&Completed {
            schema: 1,
            namespace: migration.namespace,
            source: source.into(),
            destination: destination.into(),
        })?,
    )?;
    fs::remove_file(intent_path)?;
    crate::maintenance::sync_parent(control)
}

fn rename_new(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let from = source
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let to = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        // SAFETY: terminated owned paths, no replacement flag.
        ensure!(
            unsafe {
                windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(), 8)
            } != 0,
            "index directory rename failed: {}",
            std::io::Error::last_os_error()
        );
    }
    #[cfg(unix)]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let from = CString::new(source.as_os_str().as_bytes())?;
        let to = CString::new(destination.as_os_str().as_bytes())?;
        #[cfg(target_os = "linux")]
        // SAFETY: terminated owned paths and exclusive rename flag.
        let result = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(target_os = "macos")]
        // SAFETY: terminated owned paths and exclusive rename flag.
        let result = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        anyhow::bail!("exclusive index migration is unsupported on this platform");
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        ensure!(
            result == 0,
            "index directory rename failed: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn interruptions_resume_without_replay_or_a_second_root() -> Result<()> {
        for stop in ["planned", "renamed", "metadata", "registry"] {
            let temporary = tempfile::tempdir()?;
            let base = temporary.path().canonicalize()?;
            let source = base.join("service-1.4");
            let destination = base.join("index-v1");
            let control = base.join("control");
            let workspace = base.join("workspace");
            for path in [&source, &control, &workspace] {
                fs::create_dir(path)?;
            }
            let namespace = Incarnation::from_bytes([5; 16]);
            let mut registry = ServiceRegistry::open(&control, namespace)?;
            let registration = ProjectRegistration::new(
                workspace.to_str().unwrap().into(),
                source.to_str().unwrap().into(),
                "fixture".into(),
            )?;
            registry.register_with_setup(&registration, Some(true))?;
            data_scope::bind(&source, &control, namespace)?;
            collection::Collection::initialize(
                &source,
                &registration.root,
                "fixture",
                Incarnation::from_bytes([6; 16]),
            )?;
            drop(registry);
            let lease = ServiceLease::acquire(&control)?;
            let interrupted = Cell::new(false);
            assert!(
                relocate_at(&lease, &source, &destination, |phase| {
                    if phase == stop && !interrupted.replace(true) {
                        anyhow::bail!("simulated interruption");
                    }
                    Ok(())
                })
                .is_err()
            );
            assert!(interrupted.get());
            assert!(require_settled(&control).is_err());
            relocate(&lease, &source, &destination)?;
            assert!(require_settled(&control).is_ok());
            assert!(!source.exists());
            assert!(selected(&control, &source, &destination)?);
        }
        Ok(())
    }

    #[test]
    fn destination_conflict_and_live_writers_preserve_both_roots() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let base = temporary.path().canonicalize()?;
        let source = base.join("service-1.4");
        let destination = base.join("index-v1");
        let control = base.join("control");
        let workspace = base.join("workspace");
        for path in [&source, &control, &workspace] {
            fs::create_dir(path)?;
        }
        let namespace = Incarnation::from_bytes([8; 16]);
        let mut registry = ServiceRegistry::open(&control, namespace)?;
        let registration = ProjectRegistration::new(
            workspace.to_str().unwrap().into(),
            source.to_str().unwrap().into(),
            "fixture".into(),
        )?;
        registry.register(&registration)?;
        data_scope::bind(&source, &control, namespace)?;
        drop(registry);
        let lease = ServiceLease::acquire(&control)?;
        let worker = ProjectLease::acquire(&control, registration.binding)?;
        assert!(relocate(&lease, &source, &destination).is_err());
        assert!(source.exists());
        assert!(!destination.exists());
        drop(worker);
        fs::create_dir(&destination)?;
        fs::write(destination.join("unrelated"), b"preserve")?;
        assert!(relocate(&lease, &source, &destination).is_err());
        assert!(source.exists());
        assert_eq!(fs::read(destination.join("unrelated"))?, b"preserve");
        assert!(require_settled(&control).is_ok());
        Ok(())
    }
}
