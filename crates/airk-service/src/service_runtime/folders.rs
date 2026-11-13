// SPDX-License-Identifier: Apache-2.0
//! M24 folder browsing (`airk_core::folder_browse`): the folders in one
//! folder, or the starting list, for AIRK Admin's Browse dialog and
//! `airkctl folders`. Read-only: folders only, never file names, and a link,
//! junction or other reparse point is listed but never entered.
//!
//! Decision logic (`page`, `Marks::mark`, `crumbs`, `key`) is pure; the file
//! system walk and the Windows drive list are thin.
use airk_core::{
    catalog_management::{CatalogItem, CatalogKind, CatalogPageRequest, MAX_PAGE_BYTES},
    folder_browse::{
        FOLDER_SCHEMA, FolderCrumb, FolderEntry, FolderIndexMark, FolderKind, FolderListing,
        FolderStart, FolderUse, MAX_FOLDER_ENTRIES, MAX_FOLDER_INDEX_MARKS,
        MAX_FOLDER_LISTING_BYTES, MAX_FOLDER_SCAN, validate_request,
    },
    index_catalog::MAX_SELECTION,
    install_record::Locations,
    storage_admin::StorageLocationKind,
};
use anyhow::{Result, bail};
use std::path::{Component, Path, PathBuf};

/// Leading failure codes (`storage::failure_code` reads them).
pub(super) const NOT_FOUND: &str = "folder_not_found";
const NOT_A_FOLDER: &str = "folder_not_a_folder";
const LINK: &str = "folder_link_not_followed";
const NOT_READABLE: &str = "folder_not_readable";
const NOT_ABSOLUTE: &str = "folder_path_not_absolute";
/// Catalog pages read for index marks before `marks_complete: false`.
const MAX_MARK_PAGES: usize = 8;

/// One folder as the walk found it, before paging.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Found {
    pub name: String,
    pub kind: FolderKind,
    pub hidden: bool,
}

/// The installation's locations and the indexes' roots, as compared keys.
#[derive(Default)]
pub(super) struct Marks {
    /// The installation's data location as recorded, offered in the starting list.
    data: Option<PathBuf>,
    locations: Vec<(StorageLocationKind, String)>,
    indexes: Vec<(String, String, FolderUse)>,
    complete: bool,
}

/// How paths compare: separators unified, the verbatim prefix and a trailing
/// separator dropped, and case folded on Windows.
pub(super) fn key(path: &str) -> String {
    let unified = path.replace('\\', "/");
    let trimmed = unified.strip_prefix("//?/").unwrap_or(&unified);
    let trimmed = if trimmed.len() > 1 && !trimmed.ends_with(":/") {
        trimmed.trim_end_matches('/')
    } else {
        trimmed
    };
    if cfg!(windows) {
        trimmed.to_lowercase()
    } else {
        trimmed.to_owned()
    }
}

/// A path as shown: the verbatim `\\?\` prefix removed.
pub(super) fn shown(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
}

impl Marks {
    pub(super) fn new(
        locations: Option<&Locations>,
        indexes: Vec<(String, String, String)>,
        complete: bool,
    ) -> Self {
        let data = locations.map(|locations| PathBuf::from(&locations.data));
        let locations = locations
            .map(|locations| {
                [
                    (StorageLocationKind::Program, &locations.program),
                    (StorageLocationKind::Data, &locations.data),
                    (StorageLocationKind::State, &locations.state),
                    (StorageLocationKind::Cache, &locations.cache),
                    (StorageLocationKind::Logs, &locations.logs),
                ]
                .into_iter()
                .map(|(kind, path)| (kind, key(path)))
                .collect()
            })
            .unwrap_or_default();
        let indexes = indexes
            .into_iter()
            .flat_map(|(name, root, data)| {
                [
                    (name.clone(), key(&root), FolderUse::IndexedRoot),
                    (name, key(&data), FolderUse::IndexData),
                ]
            })
            .collect();
        Self {
            data,
            locations,
            indexes,
            complete,
        }
    }

    /// The installation location and index marks of one folder.
    pub(super) fn mark(&self, path: &str) -> (Option<StorageLocationKind>, Vec<FolderIndexMark>) {
        let folder = key(path);
        let location = self
            .locations
            .iter()
            .find(|(_, location)| *location == folder)
            .map(|(kind, _)| *kind);
        let indexes = self
            .indexes
            .iter()
            .filter(|(_, path, _)| *path == folder)
            .take(MAX_FOLDER_INDEX_MARKS)
            .map(|(name, _, use_)| FolderIndexMark {
                index: name.clone(),
                use_: *use_,
            })
            .collect();
        (location, indexes)
    }
}

/// Pure paging: hidden folders dropped unless asked for, ordered by
/// case-folded name, the names after `after`, at most `MAX_FOLDER_ENTRIES`;
/// the continuation is the last returned name when more remain.
pub(super) fn page(
    mut found: Vec<Found>,
    show_hidden: bool,
    after: Option<&str>,
) -> (Vec<Found>, Option<String>) {
    found.retain(|folder| show_hidden || !folder.hidden);
    let order = |name: &str| (name.to_lowercase(), name.to_owned());
    found.sort_by_key(|folder| order(&folder.name));
    if let Some(after) = after {
        let after = order(after);
        found.retain(|folder| order(&folder.name) > after);
    }
    let more = found.len() > MAX_FOLDER_ENTRIES;
    found.truncate(MAX_FOLDER_ENTRIES);
    let continuation = more
        .then(|| found.last().map(|folder| folder.name.clone()))
        .flatten();
    (found, continuation)
}

/// The steps from the root down to `path` (`D:\` is one step with its prefix).
pub(super) fn crumbs(path: &Path) -> Vec<FolderCrumb> {
    let mut steps: Vec<&Path> = path.ancestors().collect();
    steps.reverse();
    steps
        .into_iter()
        .filter_map(|step| {
            let name = match step.components().next_back()? {
                Component::Normal(name) => name.to_string_lossy().into_owned(),
                Component::Prefix(prefix) => prefix.as_os_str().to_string_lossy().into_owned(),
                Component::RootDir => match step.components().next()? {
                    Component::Prefix(prefix) => prefix.as_os_str().to_string_lossy().into_owned(),
                    _ => "/".to_owned(),
                },
                _ => return None,
            };
            Some(FolderCrumb {
                name,
                path: shown(step),
            })
        })
        // `D:` alone (prefix without root) duplicates `D:\`; keep the rooted step.
        .fold(Vec::<FolderCrumb>::new(), |mut crumbs, crumb| {
            if crumbs.last().is_some_and(|last| last.name == crumb.name) {
                crumbs.pop();
            }
            crumbs.push(crumb);
            crumbs
        })
}

/// The parent to go up to; `None` at a drive or file system root.
pub(super) fn parent(path: &Path) -> Option<String> {
    let parent = path.parent()?;
    (parent != path && parent.has_root()).then(|| shown(parent))
}

/// Encode within `MAX_FOLDER_LISTING_BYTES`: drop entries from the end and
/// continue after the last one kept.
fn fit(mut listing: FolderListing) -> Result<FolderListing> {
    while serde_json::to_vec(&listing)?.len() > MAX_FOLDER_LISTING_BYTES {
        if listing.entries.pop().is_none() {
            bail!("{NOT_READABLE}: the folder path is too long to list");
        }
        listing.continuation = listing.entries.last().map(|entry| entry.name.clone());
    }
    listing.validate()?;
    Ok(listing)
}

/// The listing for `path`, or the starting list without one.
pub(super) fn browse(
    control: &Path,
    path: Option<&str>,
    show_hidden: bool,
    after: Option<&str>,
) -> Result<FolderListing> {
    validate_request(path, after)?;
    let locations = super::storage::served_locations(control).ok();
    let (indexes, complete) = index_roots(control);
    let marks = Marks::new(locations.as_ref(), indexes, complete);
    match path {
        None => starting_list(&marks, &drives(), home()),
        Some(path) => folder(Path::new(path), &marks, show_hidden, after),
    }
}

pub(super) fn starting_list(
    marks: &Marks,
    drives: &[PathBuf],
    home: Option<PathBuf>,
) -> Result<FolderListing> {
    let mut entries = Vec::new();
    for drive in drives {
        entries.push(start_entry(
            drive,
            drive_name(drive),
            FolderStart::Drive,
            marks,
        ));
    }
    if let Some(home) = home {
        entries.push(start_entry(
            &home,
            leaf_name(&home),
            FolderStart::Home,
            marks,
        ));
    }
    // The data location may not exist yet; the setup review creates it.
    if let Some(data) = &marks.data {
        entries.push(start_entry(
            data,
            leaf_name(data),
            FolderStart::AirkLocation,
            marks,
        ));
    }
    fit(FolderListing {
        schema: FOLDER_SCHEMA,
        path: None,
        parent: None,
        breadcrumb: Vec::new(),
        airk_location: None,
        indexes: Vec::new(),
        entries,
        continuation: None,
        truncated: false,
        marks_complete: marks.complete,
    })
}

fn leaf_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| shown(path), |name| name.to_string_lossy().into_owned())
}

fn drive_name(drive: &Path) -> String {
    let shown = shown(drive);
    let name = shown.trim_end_matches(['\\', '/']);
    if name.is_empty() {
        shown
    } else {
        name.to_owned()
    }
}

fn start_entry(path: &Path, name: String, start: FolderStart, marks: &Marks) -> FolderEntry {
    let shown = shown(path);
    let (airk_location, indexes) = marks.mark(&shown);
    FolderEntry {
        name,
        path: shown,
        kind: FolderKind::Folder,
        hidden: false,
        start: Some(start),
        airk_location,
        indexes,
    }
}

pub(super) fn folder(
    path: &Path,
    marks: &Marks,
    show_hidden: bool,
    after: Option<&str>,
) -> Result<FolderListing> {
    if !path.is_absolute() {
        bail!("{NOT_ABSOLUTE}: give a full path, such as D:\\code");
    }
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            bail!("{NOT_FOUND}: {} does not exist yet", shown(path))
        }
        Err(error) => bail!("{NOT_READABLE}: {}: {error}", shown(path)),
    };
    if is_link(&metadata) {
        bail!(
            "{LINK}: {} is a link; open the folder it points to",
            shown(path)
        );
    }
    if !metadata.is_dir() {
        bail!("{NOT_A_FOLDER}: {} is a file", shown(path));
    }
    let (found, truncated) = walk(path)?;
    let (found, continuation) = page(found, show_hidden, after);
    let entries = found
        .into_iter()
        .map(|folder| {
            let child = shown(&path.join(&folder.name));
            let (airk_location, indexes) = marks.mark(&child);
            FolderEntry {
                name: folder.name,
                path: child,
                kind: folder.kind,
                hidden: folder.hidden,
                start: None,
                airk_location,
                indexes,
            }
        })
        .collect();
    let own = shown(path);
    let (airk_location, indexes) = marks.mark(&own);
    fit(FolderListing {
        schema: FOLDER_SCHEMA,
        path: Some(own),
        parent: parent(path),
        breadcrumb: crumbs(path),
        airk_location,
        indexes,
        entries,
        continuation,
        truncated,
        marks_complete: marks.complete,
    })
}

/// The folders and links directly in `path`; files are skipped unread.
fn walk(path: &Path) -> Result<(Vec<Found>, bool)> {
    let entries = std::fs::read_dir(path)
        .map_err(|error| anyhow::anyhow!("{NOT_READABLE}: {}: {error}", shown(path)))?;
    let mut found = Vec::new();
    for (scanned, entry) in entries.enumerate() {
        if scanned == MAX_FOLDER_SCAN {
            return Ok((found, true));
        }
        let Ok(entry) = entry else { continue };
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let kind = if is_link(&metadata) {
            FolderKind::Link
        } else if metadata.is_dir() {
            FolderKind::Folder
        } else {
            continue;
        };
        // A name that is not Unicode cannot be shown or sent back exactly.
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let hidden = is_hidden(&name, &metadata);
        found.push(Found { name, kind, hidden });
    }
    Ok((found, false))
}

/// Not followed: a symbolic link, or on Windows any reparse point (junctions).
fn is_link(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        metadata.file_type().is_symlink()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn is_hidden(name: &str, metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        name.starts_with('.') || metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        name.starts_with('.')
    }
}

fn home() -> Option<PathBuf> {
    let variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_dir())
}

/// Mounted drives without touching them (a disconnected network drive is not
/// probed); `/` elsewhere.
fn drives() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        // SAFETY: GetLogicalDrives takes no arguments and returns a bit mask.
        let mask = unsafe { windows_sys::Win32::Storage::FileSystem::GetLogicalDrives() };
        (0..26u8)
            .filter(|bit| mask & (1 << bit) != 0)
            .map(|bit| PathBuf::from(format!("{}:\\", char::from(b'A' + bit))))
            .collect()
    }
    #[cfg(not(windows))]
    {
        vec![PathBuf::from("/")]
    }
}

/// The catalog's indexes as (name, primary root, data root); `false` when
/// they could not all be read (marks are then partial, never guessed).
fn index_roots(control: &Path) -> (Vec<(String, String, String)>, bool) {
    let Ok(mut registry) = airk_store::service::registry::ServiceRegistry::open_saved(control)
    else {
        return (Vec::new(), false);
    };
    let mut roots = Vec::new();
    let mut after = None;
    for _ in 0..MAX_MARK_PAGES {
        let request = CatalogPageRequest {
            kind: CatalogKind::Indexes,
            index_id: None,
            project_id: None,
            after: after.take(),
            expected_catalog_revision: None,
            limit: MAX_SELECTION,
            max_bytes: MAX_PAGE_BYTES,
        };
        let Ok(page) = registry.catalog_page(&request) else {
            return (roots, false);
        };
        for item in page.items {
            if let CatalogItem::Index(index) = item
                && !index.tombstone
            {
                roots.push((index.name, index.backing.root, index.backing.data_root));
            }
        }
        match page.continuation {
            Some(next) => after = Some(next),
            None => return (roots, true),
        }
    }
    (roots, false)
}

#[cfg(test)]
mod tests;
