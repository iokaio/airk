// SPDX-License-Identifier: Apache-2.0
//! Atomic metadata-only separation of source roots from all registered derived state.
use airk_core::service::ProjectRegistration;
use anyhow::{Result, ensure};
use rusqlite::Connection;
use std::path::Path;

pub(super) const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS service_private_paths(
    path TEXT PRIMARY KEY CHECK(length(path)<=4096)) WITHOUT ROWID;";

#[derive(serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StateBinding {
    schema: u32,
    install: std::path::PathBuf,
    state: std::path::PathBuf,
    #[serde(default)]
    logs: Option<std::path::PathBuf>,
    channel: String,
    ready: bool,
    #[serde(default)]
    managed_catalog_origin: Option<super::catalog::genesis::ManagedGenesisOrigin>,
}

fn binding(path: &Path) -> Result<StateBinding> {
    use std::io::Read;
    let metadata = std::fs::symlink_metadata(path)?;
    crate::managed_collection::reject_file_aliases(path, &metadata)?;
    ensure!(
        metadata.len() <= 32768,
        "runtime binding exceeds source-isolation allowance"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(32769)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 32768,
        "runtime binding grew beyond source-isolation allowance"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

/// Match both install/state receipts before adopting their fixed directory names.
/// Keep proven paths in the private ledger if a receipt later disappears. A raw
/// development namespace has no installer receipt and protects only its own root.
fn private_paths_mode(
    connection: &Connection,
    runtime: &Path,
    remember: bool,
) -> Result<Vec<std::path::PathBuf>> {
    let mut observed = Vec::new();
    let state = runtime
        .parent()
        .ok_or_else(|| anyhow::anyhow!("runtime parent missing"))?;
    let owner_path = state.join("airk-state-1.4.owner.json");
    match std::fs::symlink_metadata(&owner_path) {
        Ok(_) => {
            let owner = binding(&owner_path)?;
            ensure!(
                owner.schema == 1
                    && owner.ready
                    && matches!(owner.channel.as_str(), "stable" | "preview")
                    && owner.state == state
                    && state.canonicalize()? == state
                    && owner.install.canonicalize()? == owner.install
                    && runtime == state.join("control"),
                "runtime state ownership mismatch"
            );
            ensure!(
                binding(&owner.install.join("runtime-layout.json"))? == owner,
                "install/state source-isolation receipts disagree"
            );
            if let Some(origin) = &owner.managed_catalog_origin {
                origin.validate()?;
                let namespace: String = connection.query_row(
                    "SELECT incarnation FROM service_meta WHERE id=1",
                    [],
                    |row| row.get(0),
                )?;
                ensure!(
                    origin.install == owner.install
                        && origin.state == owner.state
                        && origin.control == runtime
                        && origin.namespace.to_string() == namespace
                        && origin.source.canonicalize()? == origin.source,
                    "managed source-isolation origin differs from the native binding"
                );
            }
            for path in [&owner.install, &owner.state]
                .into_iter()
                .chain(owner.logs.as_ref())
                .chain(
                    owner
                        .managed_catalog_origin
                        .as_ref()
                        .map(|origin| &origin.source),
                )
            {
                ensure!(
                    path.is_absolute()
                        && !path.components().any(|part| matches!(
                            part,
                            std::path::Component::ParentDir | std::path::Component::CurDir
                        )),
                    "invalid private runtime path"
                );
                let path = path
                    .to_str()
                    .ok_or_else(|| anyhow::anyhow!("non-UTF-8 private runtime path"))?;
                ensure!(path.len() <= 4096, "private runtime path exceeds bound");
                observed.push(std::path::PathBuf::from(path));
                if remember {
                    connection.execute(
                        "INSERT OR IGNORE INTO service_private_paths VALUES(?1)",
                        [path],
                    )?;
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let mut paths = saved_private_paths(connection)?;
    paths.extend(observed);
    paths.sort();
    paths.dedup();
    ensure!(
        paths.len() <= 16,
        "private runtime path inventory exceeds bound"
    );
    paths.push(runtime.into());
    Ok(paths)
}

fn validate_private_path(path: &Path) -> Result<()> {
    let text = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF-8 private runtime path"))?;
    ensure!(
        path.is_absolute()
            && text.len() <= 4096
            && !text.chars().any(char::is_control)
            && !path.components().any(|part| matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )),
        "invalid saved private runtime path"
    );
    Ok(())
}

fn saved_private_paths(connection: &Connection) -> Result<Vec<std::path::PathBuf>> {
    let mut statement = connection.prepare(
        "SELECT substr(CAST(path AS BLOB),1,4097) FROM service_private_paths ORDER BY path LIMIT 17",
    )?;
    let rows = statement.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
    let mut paths = Vec::new();
    for row in rows {
        let bytes = row?;
        ensure!(
            bytes.len() <= 4096 && paths.len() < 16,
            "private runtime path inventory exceeds bound"
        );
        let path = std::path::PathBuf::from(String::from_utf8(bytes)?);
        validate_private_path(&path)?;
        paths.push(path);
    }
    Ok(paths)
}

impl super::ServiceRegistry {
    /// Exact saved protection metadata only. This does not open source folders,
    /// infer parent directories or validate an installation recovery request.
    pub fn saved_private_paths(&self) -> Result<Vec<std::path::PathBuf>> {
        saved_private_paths(&self.connection)
    }

    /// Preserve native-owned recovery boundaries under the exact service lease.
    /// Adding protected paths can only restrict future registration; it grants
    /// no source/index authority. Existing sixteen-path admission stays fixed.
    /// All supplied paths commit together or none do.
    pub fn preserve_private_paths(
        &mut self,
        lease: &crate::service::ServiceLease,
        paths: &[std::path::PathBuf],
    ) -> Result<()> {
        ensure!(
            lease.root() == self.root.as_path(),
            "private-path preservation lease differs"
        );
        ensure!(
            super::retained_namespace(&self.root)? == self.incarnation()?,
            "private-path preservation namespace changed"
        );
        let mut unique = std::collections::BTreeSet::new();
        for path in paths {
            validate_private_path(path)?;
            unique.insert(path);
            ensure!(
                unique.len() <= 16,
                "private runtime path inventory exceeds bound"
            );
        }
        let root = self.root.clone();
        let tx = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        private_paths_mode(&tx, &root, false)?;
        for path in unique {
            tx.execute(
                "INSERT OR IGNORE INTO service_private_paths VALUES(?1)",
                [path.to_str().unwrap()],
            )?;
        }
        // Recheck the complete union with actual current native layout paths,
        // including rows preserved by earlier recoveries, before committing.
        private_paths_mode(&tx, &root, false)?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
fn private_paths(connection: &Connection, runtime: &Path) -> Result<Vec<std::path::PathBuf>> {
    private_paths_mode(connection, runtime, true)
}

#[derive(Debug)]
pub struct SourceBoundary;
impl std::fmt::Display for SourceBoundary {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str(
            "source roots must be disjoint from service control and registered project data",
        )
    }
}
impl std::error::Error for SourceBoundary {}

fn component_equal(left: &std::ffi::OsStr, right: &std::ffi::OsStr) -> Result<bool> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
        let left = left.encode_wide().collect::<Vec<_>>();
        let right = right.encode_wide().collect::<Vec<_>>();
        // SAFETY: initialized UTF-16 buffers with explicit bounded lengths. Native
        // ordinal uppercase rules avoid ASCII-only case aliases on Windows.
        let compared = unsafe {
            CompareStringOrdinal(
                left.as_ptr(),
                i32::try_from(left.len())?,
                right.as_ptr(),
                i32::try_from(right.len())?,
                1,
            )
        };
        ensure!(compared != 0, "native path comparison failed");
        Ok(compared == CSTR_EQUAL)
    }
    #[cfg(not(windows))]
    {
        Ok(left == right)
    }
}
pub(super) fn contains(parent: &Path, child: &Path) -> Result<bool> {
    let mut child = child.components();
    for component in parent.components() {
        let Some(next) = child.next() else {
            return Ok(false);
        };
        if !path_component_equal(component, next)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn path_component_equal(
    left: std::path::Component<'_>,
    right: std::path::Component<'_>,
) -> Result<bool> {
    #[cfg(windows)]
    if let (std::path::Component::Prefix(left), std::path::Component::Prefix(right)) = (left, right)
    {
        use std::path::Prefix;
        match (left.kind(), right.kind()) {
            (
                Prefix::Disk(a) | Prefix::VerbatimDisk(a),
                Prefix::Disk(b) | Prefix::VerbatimDisk(b),
            ) => return Ok(a.eq_ignore_ascii_case(&b)),
            (
                Prefix::UNC(a, b) | Prefix::VerbatimUNC(a, b),
                Prefix::UNC(c, d) | Prefix::VerbatimUNC(c, d),
            ) => return Ok(component_equal(a, c)? && component_equal(b, d)?),
            _ => {}
        }
    }
    component_equal(left.as_os_str(), right.as_os_str())
}
#[cfg(all(test, windows))]
#[test]
fn native_windows_prefix_aliases_cannot_evade_source_private_boundaries() -> Result<()> {
    assert!(contains(
        Path::new(r"C:\owned"),
        Path::new(r"\\?\c:\owned\private")
    )?);
    assert!(contains(
        Path::new(r"\\?\C:\owned"),
        Path::new(r"c:\owned\private")
    )?);
    assert!(contains(
        Path::new(r"\\server\share\owned"),
        Path::new(r"\\?\UNC\SERVER\SHARE\owned\private")
    )?);
    assert!(!contains(
        Path::new(r"C:\owned"),
        Path::new(r"\\?\D:\owned\private")
    )?);
    Ok(())
}
fn disjoint(left: &Path, right: &Path) -> Result<()> {
    ensure!(
        !contains(left, right)? && !contains(right, left)?,
        SourceBoundary
    );
    Ok(())
}

/// The caller holds one IMMEDIATE transaction for validation and metadata update.
/// Canonical filesystem identity is the native/kernel adapter's responsibility;
/// this bounded ledger operation never opens a collection or source directory.
pub(super) fn validate(
    connection: &Connection,
    runtime: &Path,
    proposed: &ProjectRegistration,
    additional: &[String],
) -> Result<()> {
    validate_mode(connection, runtime, proposed, additional, true)
}

pub(super) fn preflight(
    connection: &Connection,
    runtime: &Path,
    proposed: &ProjectRegistration,
    additional: &[String],
) -> Result<()> {
    validate_mode(connection, runtime, proposed, additional, false)
}
fn validate_mode(
    connection: &Connection,
    runtime: &Path,
    proposed: &ProjectRegistration,
    additional: &[String],
    remember: bool,
) -> Result<()> {
    proposed.validate()?;
    let sources =
        std::iter::once(proposed.root.as_str()).chain(additional.iter().map(String::as_str));
    let sources = sources.map(Path::new).collect::<Vec<_>>();
    let data = Path::new(&proposed.data_root);
    for protected in private_paths_mode(connection, runtime, remember)? {
        disjoint(data, &protected)?;
        for source in &sources {
            disjoint(source, &protected)?;
        }
    }
    for source in &sources {
        disjoint(source, data)?;
    }
    let mut statement = connection.prepare("SELECT substr(p.payload,1,20481),substr(coalesce(w.roots,x'5b5d'),1,98305)
        FROM projects p LEFT JOIN service_watch w ON w.project=p.binding ORDER BY p.binding LIMIT 1025")?;
    let mut rows = statement.query([])?;
    let mut count = 0;
    let mut root_bytes = 0usize;
    while let Some(row) = rows.next()? {
        count += 1;
        ensure!(
            count <= super::MAX_PROJECTS,
            "source isolation registry bound exceeded"
        );
        let payload: Vec<u8> = row.get(0)?;
        let existing = super::decode(&payload, 0, false)?.registration;
        let bytes: Vec<u8> = row.get(1)?;
        root_bytes = root_bytes.saturating_add(bytes.len());
        ensure!(
            root_bytes <= 8 * 1024 * 1024,
            "source isolation root metadata bound exceeded"
        );
        let roots = super::watch::roots(&bytes)?;
        for source in &sources {
            disjoint(source, Path::new(&existing.data_root))?;
        }
        for source in
            std::iter::once(existing.root.as_str()).chain(roots.iter().map(String::as_str))
        {
            disjoint(data, Path::new(source))?;
        }
    }
    // New catalog reservations protect all their sources even before a native
    // watcher has observed them. Watch reconfiguration cannot erase this fence.
    // The legacy schema keeps its original path and performs no catalog walk.
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version >= 12 {
        let mut statement = connection.prepare("SELECT substr(CAST(data_root AS BLOB),1,4097) FROM index_definitions ORDER BY id LIMIT 1025")?;
        let mut rows = statement.query([])?;
        let mut count = 0;
        while let Some(row) = rows.next()? {
            count += 1;
            ensure!(
                count <= super::MAX_PROJECTS,
                "catalog storage isolation capacity exceeded"
            );
            let path: Vec<u8> = row.get(0)?;
            ensure!(
                !path.is_empty() && path.len() <= 4096,
                "catalog storage isolation path exceeds bound"
            );
            let path = String::from_utf8(path)?;
            for source in &sources {
                disjoint(source, Path::new(&path))?;
            }
        }
        let mut statement = connection.prepare(
            "SELECT substr(CAST(s.canonical_path AS BLOB),1,32769) FROM index_sources s JOIN index_definitions i ON i.id=s.index_id ORDER BY s.index_id,s.id LIMIT 17409",
        )?;
        let mut rows = statement.query([])?;
        let mut count = 0;
        let mut bytes = 0usize;
        while let Some(row) = rows.next()? {
            count += 1;
            ensure!(
                count <= super::MAX_PROJECTS * 17,
                "catalog source isolation capacity exceeded"
            );
            let path: Vec<u8> = row.get(0)?;
            bytes = bytes
                .checked_add(path.len())
                .ok_or_else(|| anyhow::anyhow!("catalog source isolation size overflow"))?;
            ensure!(
                !path.is_empty() && path.len() <= 32768 && bytes <= 8 * 1024 * 1024,
                "catalog source isolation byte budget exceeded"
            );
            let path = String::from_utf8(path)?;
            disjoint(data, Path::new(&path))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod preservation_tests;
