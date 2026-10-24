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
fn private_paths(connection: &Connection, runtime: &Path) -> Result<Vec<std::path::PathBuf>> {
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
            for path in [&owner.install, &owner.state]
                .into_iter()
                .chain(owner.logs.as_ref())
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
                connection.execute(
                    "INSERT OR IGNORE INTO service_private_paths VALUES(?1)",
                    [path],
                )?;
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let mut statement =
        connection.prepare("SELECT path FROM service_private_paths ORDER BY path LIMIT 17")?;
    let paths = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(
        paths.len() <= 16 && paths.iter().all(|path| path.len() <= 4096),
        "private runtime path inventory exceeds bound"
    );
    let mut paths = paths
        .into_iter()
        .map(std::path::PathBuf::from)
        .collect::<Vec<_>>();
    paths.push(runtime.into());
    Ok(paths)
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
fn contains(parent: &Path, child: &Path) -> Result<bool> {
    let mut child = child.components();
    for component in parent.components() {
        let Some(next) = child.next() else {
            return Ok(false);
        };
        if !component_equal(component.as_os_str(), next.as_os_str())? {
            return Ok(false);
        }
    }
    Ok(true)
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
    proposed.validate()?;
    let sources =
        std::iter::once(proposed.root.as_str()).chain(additional.iter().map(String::as_str));
    let sources = sources.map(Path::new).collect::<Vec<_>>();
    let data = Path::new(&proposed.data_root);
    for protected in private_paths(connection, runtime)? {
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
    Ok(())
}

#[cfg(test)]
#[path = "service_source_isolation_tests.rs"]
mod tests;
