// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::cell::RefCell;

/// Owner-private fixtures under `AIRK_PRIVATE_CENSUS_BASE` when set (the
/// system temporary folder can carry foreign grants on some hosts).
fn fixture() -> Result<(tempfile::TempDir, Locations)> {
    let base = match std::env::var_os("AIRK_PRIVATE_CENSUS_BASE") {
        Some(base) => tempfile::Builder::new()
            .prefix("airk-storage-")
            .tempdir_in(base)?,
        None => tempfile::tempdir()?,
    };
    let at = |name: &str| base.path().join(name).to_str().unwrap().to_owned();
    let locations = Locations {
        program: at("program"),
        data: at("data"),
        state: at("state"),
        cache: at("cache"),
        logs: at("logs"),
    };
    Ok((base, locations))
}

fn write(root: &str, relative: &str, bytes: &[u8]) -> Result<PathBuf> {
    let path = Path::new(root).join(relative);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, bytes)?;
    Ok(path)
}

/// A directory link the walker must not follow: a junction on Windows (no
/// privilege needed), a symbolic link elsewhere.
fn link_directory(target: &Path, link: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        let status = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .status()?;
        ensure!(status.success(), "mklink /J failed");
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}

/// Logs fake: the closed files of a folder, deleted on request.
struct FakeLogs {
    folder: PathBuf,
    files: RefCell<Vec<PurgeFile>>,
}
impl ClosedLogs for FakeLogs {
    fn folder(&self) -> Result<PathBuf> {
        Ok(self.folder.clone())
    }
    fn list(&self) -> Result<Vec<PurgeFile>> {
        Ok(self.files.borrow().clone())
    }
    fn delete(&self, plan: &[PurgeFile]) -> Result<Vec<PurgedFile>> {
        ensure!(*self.files.borrow() == plan, "{PLAN_CHANGED}");
        self.files.borrow_mut().clear();
        Ok(plan
            .iter()
            .map(|file| PurgedFile {
                relative: file.relative.clone(),
                deleted: true,
                retained: None,
            })
            .collect())
    }
}
fn no_logs(locations: &Locations) -> FakeLogs {
    FakeLogs {
        folder: PathBuf::from(&locations.logs),
        files: RefCell::new(Vec::new()),
    }
}
fn code(error: anyhow::Error) -> String {
    failure_code(&error, "storage_unavailable")
}

#[test]
fn inventory_lists_the_five_locations_in_order_with_their_rules() -> Result<()> {
    let (_base, locations) = fixture()?;
    write(&locations.program, "service/airk.exe", b"12345")?;
    write(&locations.program, "bin/airkctl.exe", b"123")?;
    write(&locations.cache, "a/b.bin", b"1")?;
    fs::create_dir_all(&locations.state)?;
    fs::create_dir_all(&locations.logs)?;
    // `data` is missing: empty and complete.
    let inventory = inventory(&locations)?;
    inventory.validate()?;
    let kinds: Vec<_> = inventory.locations.iter().map(|l| l.kind).collect();
    assert_eq!(kinds, StorageLocationKind::ALL);
    let shape: Vec<_> = inventory
        .locations
        .iter()
        .map(|l| (l.path.as_str(), l.bytes, l.files, l.complete, l.purge))
        .collect();
    assert_eq!(
        shape,
        [
            (
                locations.program.as_str(),
                8,
                2,
                true,
                PurgeRule::NotPurgeable
            ),
            (locations.data.as_str(), 0, 0, true, PurgeRule::NotPurgeable),
            (
                locations.state.as_str(),
                0,
                0,
                true,
                PurgeRule::NotPurgeable
            ),
            (
                locations.cache.as_str(),
                1,
                1,
                true,
                PurgeRule::CacheContents
            ),
            (locations.logs.as_str(), 0, 0, true, PurgeRule::ClosedLogs),
        ]
    );
    assert!(!inventory.not_covered.is_empty());
    Ok(())
}

#[test]
fn measurement_stops_at_its_budget_and_never_follows_links() -> Result<()> {
    let (base, locations) = fixture()?;
    for name in ["1", "2", "3", "4", "5"] {
        write(&locations.data, name, b"xx")?;
    }
    let mut budget = 2;
    let partial = measure(Path::new(&locations.data), &mut budget);
    assert_eq!(budget, 0);
    assert!(!partial.complete && partial.files <= 2);
    let outside = base.path().join("outside");
    write(outside.to_str().unwrap(), "big.bin", &[0; 64])?;
    fs::create_dir_all(&locations.cache)?;
    link_directory(&outside, &Path::new(&locations.cache).join("linked"))?;
    let mut budget = MAX_INVENTORY_ENTRIES;
    assert_eq!(
        measure(Path::new(&locations.cache), &mut budget),
        Measured {
            bytes: 0,
            files: 0,
            complete: true
        }
    );
    // A linked location itself is not measured either.
    let mut budget = MAX_INVENTORY_ENTRIES;
    let linked = measure(&Path::new(&locations.cache).join("linked"), &mut budget);
    assert!(!linked.complete && linked.bytes == 0);
    Ok(())
}

#[test]
fn cache_purge_applies_exactly_the_reviewed_plan() -> Result<()> {
    let (base, locations) = fixture()?;
    write(&locations.cache, "z.bin", b"zz")?;
    write(&locations.cache, "a/b.txt", b"b")?;
    write(&locations.cache, "a/deep/c.bin", b"ccc")?;
    let kept = write(&locations.data, "index.bin", b"data")?;
    let outside = base.path().join("outside");
    let foreign = write(outside.to_str().unwrap(), "foreign.bin", b"foreign")?;
    link_directory(&outside, &Path::new(&locations.cache).join("linked"))?;
    let logs = no_logs(&locations);
    let plan = purge_plan(StorageLocationKind::Cache, &locations, &logs)?;
    let listed: Vec<_> = plan
        .files
        .iter()
        .map(|f| (f.relative.as_str(), f.bytes))
        .collect();
    assert_eq!(listed, [("a/b.txt", 1), ("a/deep/c.bin", 3), ("z.bin", 2)]);
    assert!(!plan.truncated);
    assert_eq!(plan.path, locations.cache);
    let digest = plan.digest()?;

    // A file added after the review changes the plan: nothing is deleted.
    let late = write(&locations.cache, "late.bin", b"l")?;
    let refused = purge(StorageLocationKind::Cache, digest, &locations, &logs).unwrap_err();
    assert_eq!(code(refused), PLAN_CHANGED);
    assert!(Path::new(&locations.cache).join("z.bin").exists());
    fs::remove_file(late)?;

    let receipt = purge(StorageLocationKind::Cache, digest, &locations, &logs)?;
    assert!(receipt.complete);
    assert_eq!(receipt.plan_digest, digest);
    assert!(
        receipt
            .files
            .iter()
            .all(|f| f.deleted && f.retained.is_none())
    );
    let cache = Path::new(&locations.cache);
    assert!(cache.is_dir(), "the cache folder itself stays");
    assert!(!cache.join("a").exists(), "emptied folders are removed");
    assert!(cache.join("linked").exists(), "the link stays");
    assert_eq!(fs::read(foreign)?, b"foreign");
    assert_eq!(fs::read(kept)?, b"data");
    assert!(
        purge_plan(StorageLocationKind::Cache, &locations, &logs)?
            .files
            .is_empty()
    );
    Ok(())
}

#[test]
fn a_changed_file_is_retained_and_reported() -> Result<()> {
    let (_base, locations) = fixture()?;
    let file = write(&locations.cache, "one.bin", b"1")?;
    let planned = PurgeFile {
        relative: "one.bin".into(),
        bytes: 1,
    };
    fs::write(&file, b"grown")?;
    let results = delete_cache(Path::new(&locations.cache), std::slice::from_ref(&planned));
    assert!(!results[0].deleted);
    assert!(
        results[0]
            .retained
            .as_deref()
            .unwrap()
            .starts_with("changed")
    );
    fs::remove_file(&file)?;
    let results = delete_cache(Path::new(&locations.cache), &[planned]);
    assert_eq!(results[0].retained.as_deref(), Some("gone"));
    Ok(())
}

#[test]
fn only_cache_and_closed_logs_are_purgeable() -> Result<()> {
    let (base, mut locations) = fixture()?;
    let logs = no_logs(&locations);
    for kind in [
        StorageLocationKind::Program,
        StorageLocationKind::Data,
        StorageLocationKind::State,
    ] {
        assert_eq!(
            code(purge_plan(kind, &locations, &logs).unwrap_err()),
            NOT_PURGEABLE
        );
    }
    // A cache that contains (or lies inside) another location is refused.
    locations.cache = base
        .path()
        .join("data")
        .join("cache")
        .to_str()
        .unwrap()
        .into();
    assert_eq!(
        code(purge_plan(StorageLocationKind::Cache, &locations, &logs).unwrap_err()),
        OVERLAPS
    );
    locations.cache = base.path().to_str().unwrap().into();
    assert_eq!(
        code(purge_plan(StorageLocationKind::Cache, &locations, &logs).unwrap_err()),
        OVERLAPS
    );
    Ok(())
}

#[test]
fn closed_logs_purge_through_the_writer_and_its_location() -> Result<()> {
    let (_base, locations) = fixture()?;
    fs::create_dir_all(&locations.logs)?;
    // The writer lists sorted (`service_logs::closed`).
    let files = vec![
        PurgeFile {
            relative: "job-errors-1/previous-1.jsonl".into(),
            bytes: 4,
        },
        PurgeFile {
            relative: "logs-1.4/previous-2.jsonl".into(),
            bytes: 9,
        },
    ];
    let logs = FakeLogs {
        folder: PathBuf::from(&locations.logs),
        files: RefCell::new(files),
    };
    let plan = purge_plan(StorageLocationKind::Logs, &locations, &logs)?;
    assert_eq!(
        plan.files
            .iter()
            .map(|f| f.relative.as_str())
            .collect::<Vec<_>>(),
        ["job-errors-1/previous-1.jsonl", "logs-1.4/previous-2.jsonl"]
    );
    let receipt = purge(StorageLocationKind::Logs, plan.digest()?, &locations, &logs)?;
    assert!(receipt.complete && receipt.files.len() == 2);
    assert!(logs.files.borrow().is_empty());

    // A writer whose rings live elsewhere (no adopted installation) is refused.
    let elsewhere = FakeLogs {
        folder: PathBuf::from(&locations.state),
        files: RefCell::new(Vec::new()),
    };
    assert_eq!(
        code(purge_plan(StorageLocationKind::Logs, &locations, &elsewhere).unwrap_err()),
        LOGS_ELSEWHERE
    );
    Ok(())
}

#[test]
fn failure_codes_come_from_the_error_prefix() {
    assert_eq!(
        code(anyhow::anyhow!("{PLAN_CHANGED}: detail")),
        PLAN_CHANGED
    );
    assert_eq!(
        code(anyhow::anyhow!("Some IO error")),
        "storage_unavailable"
    );
}
