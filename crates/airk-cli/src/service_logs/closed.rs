// SPDX-License-Identifier: Apache-2.0
//! Closed-log purge (U100): listing and deleting the `previous-*` files of
//! both rings. It runs on the writer thread between appends and holds each
//! ring's `writer.lock`, so neither this writer nor another appender (for
//! example `record_client_error`) rotates while files are deleted. A deleted
//! `previous-k` only leaves a gap: rotation and retention skip missing files.
//! `current.jsonl`, `writer.lock` and the owner records are never deleted.
use super::*;
use airk_core::storage_admin::{PurgeFile, PurgedFile};

/// How long a requester waits for the writer thread's answer.
pub(super) const REPLY_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

const RINGS: [(&str, &str); 2] = [
    (DIRECTORY, OWNER),
    (
        airk_core::service_logs::RAW_LOG_DIRECTORY,
        airk_core::service_logs::RAW_LOG_OWNER,
    ),
];

pub(super) enum Request {
    List,
    Delete(Vec<PurgeFile>),
}

pub(super) enum Reply {
    Listed(Vec<PurgeFile>),
    Deleted(Vec<PurgedFile>),
}

struct Ring {
    name: &'static str,
    directory: PathBuf,
    _guard: WriteGuard,
}

/// Each existing ring, ownership-checked (never created) and locked.
fn open_rings(root: &Path) -> Result<Vec<Ring>> {
    let parent = crate::service_locations::log_parent(root)?;
    let mut rings = Vec::new();
    for (name, owner) in RINGS {
        if !exists(&parent.join(owner))? && !exists(&parent.join(name))? {
            continue;
        }
        let directory = named_owner(root, false, name, owner)?;
        preflight(&directory)?;
        let lock = open_file(&directory.join("writer.lock"), true)?;
        let guard = lock_writer(&lock)?;
        rings.push(Ring {
            name,
            directory,
            _guard: guard,
        });
    }
    Ok(rings)
}

/// The closed files of the locked rings, sorted by relative path.
fn closed_files(rings: &[Ring]) -> Result<Vec<PurgeFile>> {
    let mut files = Vec::new();
    for ring in rings {
        for name in &FILES[1..] {
            let path = ring.directory.join(name);
            if exists(&path)? {
                crate::install_files::safe_file(&path)?;
                files.push(PurgeFile {
                    relative: format!("{}/{name}", ring.name),
                    bytes: fs::symlink_metadata(&path)?.len(),
                });
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Pure: the ring and closed file a plan entry names, if it names one.
fn closed_name(relative: &str) -> Option<(&str, &str)> {
    let (ring, name) = relative.split_once('/')?;
    (RINGS.iter().any(|(known, _)| *known == ring) && FILES[1..].contains(&name))
        .then_some((ring, name))
}

pub(super) fn serve(root: &Path, request: Request) -> Result<Reply> {
    let rings = open_rings(root)?;
    let current = closed_files(&rings)?;
    let plan = match request {
        Request::List => return Ok(Reply::Listed(current)),
        Request::Delete(plan) => plan,
    };
    ensure!(
        plan == current,
        "storage_purge_plan_changed: the closed log files changed after the plan"
    );
    let mut results = Vec::with_capacity(plan.len());
    for file in plan {
        let (ring, name) = closed_name(&file.relative)
            .context("purge plan names a file that is not a closed log")?;
        let directory = &rings
            .iter()
            .find(|open| open.name == ring)
            .context("purge plan names a ring that is not open")?
            .directory;
        let outcome = fs::remove_file(directory.join(name));
        results.push(PurgedFile {
            relative: file.relative,
            deleted: outcome.is_ok(),
            retained: outcome.err().map(|error| {
                format!("in_use_or_refused: {error}")
                    .chars()
                    .take(256)
                    .collect()
            }),
        });
    }
    Ok(Reply::Deleted(results))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PrivateRoot {
        _owner: tempfile::TempDir,
        root: PathBuf,
    }
    fn private_root() -> Result<PrivateRoot> {
        let owner = match std::env::var_os("AIRK_PRIVATE_CENSUS_BASE") {
            Some(base) => tempfile::Builder::new()
                .prefix("airk-closed-logs-")
                .tempdir_in(base)?,
            None => tempfile::tempdir()?,
        };
        let root = owner.path().join("private-control");
        crate::local_ipc::prepare_root(&root)?;
        Ok(PrivateRoot {
            _owner: owner,
            root,
        })
    }
    fn names(files: &[PurgeFile]) -> Vec<&str> {
        files.iter().map(|file| file.relative.as_str()).collect()
    }

    #[test]
    fn plan_entries_name_only_closed_ring_files() {
        assert_eq!(
            closed_name("logs-1.4/previous-3.jsonl"),
            Some(("logs-1.4", "previous-3.jsonl"))
        );
        assert!(closed_name("job-errors-1/previous-20.jsonl").is_some());
        for refused in [
            "logs-1.4/current.jsonl",
            "logs-1.4/writer.lock",
            "logs-1.4.owner.json",
            "logs-1.4/previous-21.jsonl",
            "other/previous-1.jsonl",
            "logs-1.4/../previous-1.jsonl",
        ] {
            assert!(closed_name(refused).is_none(), "{refused}");
        }
    }

    #[test]
    fn writer_thread_purges_only_closed_files_and_rotation_continues() -> Result<()> {
        let root = private_root()?;
        let summary = Sink::open(&root.root)?.directory.clone();
        let raw = Sink::named(
            &root.root,
            airk_core::service_logs::RAW_LOG_DIRECTORY,
            airk_core::service_logs::RAW_LOG_OWNER,
            RECORD_BYTES,
        )?
        .directory
        .clone();
        for name in &FILES[..4] {
            fs::write(summary.join(name), b"{\"fixture\":1}\n")?;
        }
        fs::write(raw.join(FILES[0]), b"{}\n")?;
        fs::write(raw.join(FILES[2]), b"{}\n")?;
        let (log, guard) = Log::start(root.root.clone(), Incarnation::from_bytes([7; 16]));
        let listed = log.closed_log_files()?;
        assert_eq!(
            names(&listed),
            [
                "job-errors-1/previous-2.jsonl",
                "logs-1.4/previous-1.jsonl",
                "logs-1.4/previous-2.jsonl",
                "logs-1.4/previous-3.jsonl",
            ]
        );
        // A plan that no longer matches deletes nothing.
        let stale = &listed[1..];
        let error = log.delete_closed_logs(stale).unwrap_err();
        assert!(
            format!("{error:#}").contains("storage_purge_plan_changed"),
            "{error:#}"
        );
        assert_eq!(log.closed_log_files()?, listed);
        let deleted = log.delete_closed_logs(&listed)?;
        assert_eq!(deleted.len(), listed.len());
        assert!(
            deleted
                .iter()
                .all(|file| file.deleted && file.retained.is_none())
        );
        assert!(log.closed_log_files()?.is_empty());
        for kept in [
            summary.join(FILES[0]),
            summary.join("writer.lock"),
            raw.join(FILES[0]),
            raw.join("writer.lock"),
        ] {
            assert!(kept.exists(), "{}", kept.display());
        }
        let parent = crate::service_locations::log_parent(&root.root)?;
        assert!(parent.join(OWNER).exists() && parent.join(RINGS[1].1).exists());
        // The writer still appends after the purge.
        log.record(LogKind::ServiceStarted, None, None);
        drop(guard);
        assert_eq!(log.telemetry().write_failures, 0);
        let page = recent(&root.root, 32)?;
        assert_eq!(
            page.records.last().map(|record| &record.kind),
            Some(&LogKind::ServiceStarted)
        );
        // Rotation over the gap renames `current` into the empty slot.
        let sink = Sink::open(&root.root)?;
        sink.rotate()?;
        assert!(!summary.join(FILES[0]).exists());
        assert!(summary.join(FILES[1]).exists());
        Ok(())
    }

    #[test]
    fn a_stopped_writer_answers_unavailable() -> Result<()> {
        let root = private_root()?;
        let (sender, receiver) = sync_channel(1);
        drop(receiver);
        let log = Log {
            sender: Some(sender),
            counters: Arc::new(Counters::default()),
            instance: Incarnation::from_bytes([1; 16]),
            root: root.root.clone(),
        };
        let error = log.closed_log_files().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("diagnostic_log_writer_unavailable"),
            "{error:#}"
        );
        Ok(())
    }
}
