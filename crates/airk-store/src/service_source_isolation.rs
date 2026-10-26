// SPDX-License-Identifier: Apache-2.0
//! Atomic metadata-only separation of source roots from all registered derived state.
use airk_core::service::ProjectRegistration;
use anyhow::{Result, ensure};
use std::path::Path;

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
pub(super) fn private_paths(runtime: &Path) -> Result<Vec<std::path::PathBuf>> {
    let state = runtime
        .parent()
        .ok_or_else(|| anyhow::anyhow!("runtime parent missing"))?;
    let path = state.join("airk-state-1.4.owner.json");
    let mut paths = vec![runtime.to_path_buf()];
    match std::fs::symlink_metadata(&path) {
        Ok(_) => {
            let owner = binding(&path)?;
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
                        ))
                        && path.to_str().is_some_and(|value| value.len() <= 4096),
                    "invalid private runtime path"
                );
                paths.push(path.clone());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
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
        #[cfg(windows)]
        let equal = match (component, next) {
            (std::path::Component::Prefix(left), std::path::Component::Prefix(right)) => {
                use std::path::Prefix;
                match (left.kind(), right.kind()) {
                    (
                        Prefix::Disk(left) | Prefix::VerbatimDisk(left),
                        Prefix::Disk(right) | Prefix::VerbatimDisk(right),
                    ) => left.eq_ignore_ascii_case(&right),
                    (
                        Prefix::UNC(left_server, left_share)
                        | Prefix::VerbatimUNC(left_server, left_share),
                        Prefix::UNC(right_server, right_share)
                        | Prefix::VerbatimUNC(right_server, right_share),
                    ) => {
                        component_equal(left_server, right_server)?
                            && component_equal(left_share, right_share)?
                    }
                    _ => component_equal(left.as_os_str(), right.as_os_str())?,
                }
            }
            _ => component_equal(component.as_os_str(), next.as_os_str())?,
        };
        #[cfg(not(windows))]
        let equal = component_equal(component.as_os_str(), next.as_os_str())?;
        if !equal {
            return Ok(false);
        }
    }
    Ok(true)
}
pub(super) fn disjoint(left: &Path, right: &Path) -> Result<()> {
    ensure!(
        !contains(left, right)? && !contains(right, left)?,
        SourceBoundary
    );
    Ok(())
}

/// Redb registry equivalent of `validate`: registration and all known source
/// roots are checked against retained data/control boundaries before mutation.
pub(super) fn validate_native(
    runtime: &Path,
    proposed: &ProjectRegistration,
    proposed_additional: &[String],
    existing: &[(ProjectRegistration, Vec<String>)],
    protected: &[std::path::PathBuf],
) -> Result<()> {
    proposed.validate()?;
    ensure!(
        proposed_additional.len() <= 16,
        "source watch root limit exceeded"
    );
    let mut unique = std::collections::BTreeSet::new();
    for source in proposed_additional {
        ensure!(
            !source.is_empty()
                && source.len() <= 4096
                && !source.chars().any(char::is_control)
                && unique.insert(source),
            "invalid or duplicate source watch root"
        );
    }
    let sources = std::iter::once(proposed.root.as_str())
        .chain(proposed_additional.iter().map(String::as_str))
        .collect::<Vec<_>>();
    let data = Path::new(&proposed.data_root);
    for private in std::iter::once(runtime).chain(protected.iter().map(std::path::PathBuf::as_path))
    {
        disjoint(data, private)?;
        for source in &sources {
            disjoint(Path::new(source), private)?;
        }
    }
    for source in &sources {
        disjoint(Path::new(source), data)?;
    }
    let mut root_bytes = 0usize;
    for (registration, watched_roots) in existing {
        root_bytes =
            root_bytes.saturating_add(watched_roots.iter().map(String::len).sum::<usize>());
        ensure!(
            root_bytes <= 8 * 1024 * 1024,
            "source isolation root metadata exceeds bound"
        );
        for source in &sources {
            disjoint(Path::new(source), Path::new(&registration.data_root))?;
        }
        for source in std::iter::once(registration.root.as_str())
            .chain(watched_roots.iter().map(String::as_str))
        {
            disjoint(data, Path::new(source))?;
        }
        disjoint(
            Path::new(&proposed.root),
            Path::new(&registration.data_root),
        )?;
        for existing_source in std::iter::once(registration.root.as_str())
            .chain(watched_roots.iter().map(String::as_str))
        {
            disjoint(Path::new(existing_source), data)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "service_source_isolation_tests.rs"]
mod tests;
