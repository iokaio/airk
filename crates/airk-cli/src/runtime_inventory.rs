// SPDX-License-Identifier: Apache-2.0
//! Read-only inventory of AIRK-owned per-user data, and a confirmed purge of
//! selected owned logs/caches. Project indexes are removed only through the
//! project removal contract; source trees are never listed for deletion.
use super::*;
use sha2::{Digest, Sha256};

const MAX_ENTRIES: usize = 100_000;

struct Measure {
    exists: bool,
    bytes: u64,
    files: u64,
    complete: bool,
}

/// Bounded, no-follow size measurement. Links count as entries but are never traversed.
fn measure(path: &Path, budget: &mut usize) -> Result<Measure> {
    let mut result = Measure {
        exists: false,
        bytes: 0,
        files: 0,
        complete: true,
    };
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(result),
        Err(error) => return Err(error.into()),
    };
    result.exists = true;
    let mut pending = vec![(path.to_path_buf(), metadata)];
    while let Some((path, metadata)) = pending.pop() {
        if *budget == 0 {
            result.complete = false;
            break;
        }
        *budget -= 1;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            for entry in fs::read_dir(&path)? {
                let entry = entry?;
                pending.push((entry.path(), fs::symlink_metadata(entry.path())?));
            }
        } else {
            result.files += 1;
            result.bytes = result.bytes.saturating_add(metadata.len());
        }
    }
    Ok(result)
}

fn location(
    id: String,
    kind: &str,
    path: &Path,
    purge: &str,
    consequence: &str,
    budget: &mut usize,
) -> Result<serde_json::Value> {
    let measured = measure(path, budget)?;
    Ok(
        serde_json::json!({"id":id,"kind":kind,"path":path,"exists":measured.exists,"bytes":measured.bytes,
        "files":measured.files,"complete":measured.complete,"purge":purge,"consequence":consequence}),
    )
}

fn projects(root: &Path) -> Result<(Vec<serde_json::Value>, bool)> {
    let control = layout::control(root)?;
    if !control.join("service-1.4.sqlite3").try_exists()? {
        return Ok((Vec::new(), true));
    }
    let mut registry = airk_store::service::registry::ServiceRegistry::open_saved(&control)?;
    let mut rows = Vec::new();
    for inactive in [false, true] {
        let (mut after, mut revision) = (None, None);
        loop {
            let page = if inactive {
                registry.list_inactive(after, revision, 32)?
            } else {
                registry.list(after, revision, 32)?
            };
            for project in &page.projects {
                let registration = &project.registration;
                // Reported, never enforced: AIRK does not change existing permissions.
                let storage_warning = airk_store::service::data_scope::privacy_warning(Path::new(
                    &registration.data_root,
                ))
                .ok()
                .flatten();
                rows.push(serde_json::json!({"binding":registration.binding,"root":registration.root,
                    "data_root":registration.data_root,"collection":registration.collection,
                    "storage_warning":storage_warning,
                    "state":if inactive {"inactive"} else {"registered"},
                    "removal":"Unregister the project in AIRK Management, or submit {\"operation\":\"unregister\"} through `airk configure --stdin`; source files are untouched"}));
            }
            if rows.len() > 4096 {
                return Ok((rows, false));
            }
            match page.after {
                Some(next) => {
                    after = Some(next);
                    revision = Some(page.revision);
                }
                None => break,
            }
        }
    }
    Ok((rows, true))
}

pub(super) fn inventory(root: &Path) -> Result<serde_json::Value> {
    let mut budget = MAX_ENTRIES;
    let mut locations = Vec::new();
    for channel in [
        crate::user_paths::Channel::Stable,
        crate::user_paths::Channel::Preview,
    ] {
        let paths = crate::user_paths::UserPaths::discover(channel)?;
        let name = serde_json::to_value(channel)?
            .as_str()
            .unwrap_or("unknown")
            .to_owned();
        for (kind, path, purge, consequence) in [
            (
                "logs",
                &paths.logs,
                "purgeable",
                "Deletes AIRK service and diagnostic logs; future logs start empty.",
            ),
            (
                "cache",
                &paths.cache,
                "purgeable",
                "Deletes re-creatable AIRK caches; later work may re-create them.",
            ),
            (
                "data",
                &paths.data,
                "requires_project_removal",
                "Holds project indexes; remove each project to delete its index.",
            ),
            (
                "state",
                &paths.state,
                "not_purgeable",
                "Holds the service registry and control state; removed only with every project and the runtime.",
            ),
            (
                "install",
                &paths.install,
                "not_purgeable",
                "Runtime payloads; use `airk runtime uninstall`.",
            ),
            (
                "launcher",
                &paths.launcher,
                "not_purgeable",
                "Agent launcher; retired by `airk runtime uninstall`.",
            ),
            (
                "configuration",
                &paths.configuration,
                "not_purgeable",
                "AIRK configuration; review manually.",
            ),
        ] {
            locations.push(location(
                format!("{name}:{kind}"),
                kind,
                path,
                purge,
                consequence,
                &mut budget,
            )?);
        }
    }
    // An unreadable registry leaves the inventory explicitly incomplete.
    let (projects, projects_complete) = projects(root).unwrap_or_default();
    let complete = projects_complete
        && budget > 0
        && locations
            .iter()
            .all(|location| location["complete"] == serde_json::json!(true));
    Ok(
        serde_json::json!({"protocol":1,"read_only":true,"locations":locations,"projects":projects,"incomplete":!complete,
        "not_covered":["custom index roots outside recorded projects","agent configurations outside recorded projects",
            "exports, backups and copies sent to agent providers","temporary uninstall diagnostics (airk-uninstall-* in the system temporary directory)"]}),
    )
}

fn purge_items(root: &Path, select: &[String]) -> Result<Vec<serde_json::Value>> {
    ensure!(
        !select.is_empty() && select.len() <= 4,
        "select 1..4 purgeable locations"
    );
    let report = inventory(root)?;
    let mut items = Vec::new();
    for id in select {
        let location = report["locations"]
            .as_array()
            .context("inventory locations missing")?
            .iter()
            .find(|location| location["id"] == serde_json::json!(id))
            .with_context(|| format!("unknown inventory location: {id}"))?;
        ensure!(
            location["purge"] == serde_json::json!("purgeable"),
            "{id} is not purgeable here: {}",
            location["consequence"]
                .as_str()
                .unwrap_or("review it manually")
        );
        items.push(location.clone());
    }
    Ok(items)
}

fn plan_digest(items: &[serde_json::Value]) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(items)?)))
}

/// Delete regular files and empty directories inside `root`, never the root
/// itself, never following links. Links and other special files are retained.
fn purge_contents(root: &Path, retained: &mut Vec<String>) -> Result<u64> {
    let mut deleted = 0u64;
    let mut stack = vec![(root.to_path_buf(), false)];
    let mut visited = 0usize;
    while let Some((path, expanded)) = stack.pop() {
        visited += 1;
        ensure!(
            visited <= MAX_ENTRIES,
            "purge exceeds its entry budget; run it again"
        );
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            retained.push(format!("link retained: {}", path.display()));
        } else if metadata.is_dir() {
            if expanded {
                if path != root {
                    match fs::remove_dir(&path) {
                        Ok(()) => (),
                        Err(error) => retained.push(format!("{}: {error}", path.display())),
                    }
                }
            } else {
                stack.push((path.clone(), true));
                for entry in fs::read_dir(&path)? {
                    stack.push((entry?.path(), false));
                }
            }
        } else if metadata.is_file() {
            match fs::remove_file(&path) {
                Ok(()) => deleted += 1,
                Err(error) => retained.push(format!("{}: {error}", path.display())),
            }
        } else {
            retained.push(format!("special file retained: {}", path.display()));
        }
    }
    Ok(deleted)
}

/// Without `confirm`, returns the exact plan and its digest. With the matching
/// digest, purges while the service is quiescent; repeating is safe.
pub(super) fn purge(
    root: &Path,
    select: &[String],
    confirm: Option<&str>,
) -> Result<serde_json::Value> {
    let items = purge_items(root, select)?;
    let digest = plan_digest(&items)?;
    let Some(confirm) = confirm else {
        return Ok(
            serde_json::json!({"protocol":1,"plan":items,"plan_digest":digest,
            "confirm":"Repeat with --confirm <plan_digest> to delete exactly these locations' contents."}),
        );
    };
    ensure!(
        confirm == digest,
        "purge plan changed since review; preview it again"
    );
    let _quiet = quiescent(root)
        .context("stop the AIRK service and close AIRK clients before purging logs or caches")?;
    let (mut deleted, mut retained) = (Vec::new(), Vec::new());
    for item in &items {
        let path = PathBuf::from(item["path"].as_str().context("purge path missing")?);
        let count = purge_contents(&path, &mut retained)?;
        deleted.push(serde_json::json!({"id":item["id"],"path":path,"files_deleted":count}));
    }
    Ok(
        serde_json::json!({"protocol":1,"purged":deleted,"retained":retained,"complete":retained.is_empty(),
        "note":"Logical deletion only; backups and copies elsewhere are unaffected."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purge_contents_keeps_the_root_and_never_follows_links() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("logs");
        fs::create_dir_all(root.join("nested"))?;
        fs::write(root.join("service.log"), "fixture")?;
        fs::write(root.join("nested").join("old.log"), "fixture")?;
        let outside = directory.path().join("outside.txt");
        fs::write(&outside, "keep")?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("link"))?;
        let mut retained = Vec::new();
        assert_eq!(purge_contents(&root, &mut retained)?, 2);
        assert!(root.is_dir());
        assert!(!root.join("nested").exists());
        assert_eq!(fs::read_to_string(&outside)?, "keep");
        #[cfg(unix)]
        assert_eq!(retained.len(), 1);
        // Repeating after completion is a no-op.
        assert_eq!(purge_contents(&root, &mut retained)?, 0);
        Ok(())
    }

    #[test]
    fn measurement_is_bounded_and_reports_missing_locations() -> Result<()> {
        let directory = tempfile::tempdir()?;
        for index in 0..5 {
            fs::write(directory.path().join(format!("{index}.log")), "12345")?;
        }
        let mut budget = 3;
        let partial = measure(directory.path(), &mut budget)?;
        assert!(partial.exists && !partial.complete);
        let mut budget = 100;
        let full = measure(directory.path(), &mut budget)?;
        assert!(full.complete);
        assert_eq!((full.files, full.bytes), (5, 25));
        let mut budget = 100;
        assert!(!measure(&directory.path().join("missing"), &mut budget)?.exists);
        Ok(())
    }
}
