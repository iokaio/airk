// SPDX-License-Identifier: Apache-2.0
use super::*;

struct PrivateRoot {
    _owner: tempfile::TempDir,
    root: PathBuf,
}
impl PrivateRoot {
    fn path(&self) -> &Path {
        &self.root
    }
}
fn private_root() -> Result<PrivateRoot> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("private-control");
    crate::local_ipc::prepare_root(&root)?;
    Ok(PrivateRoot {
        _owner: owner,
        root,
    })
}

fn record(time: u64) -> LogRecord {
    let mut record = make_record(
        Incarnation::from_bytes([1; 16]),
        LogKind::ServiceStarted,
        None,
        None,
        1,
    );
    record.observed_unix_ms = time;
    record
}
#[test]
fn full_export_reads_every_retained_file_and_excludes_malformed_payloads() -> Result<()> {
    let root = private_root()?;
    let sink = Sink::open(root.path())?;
    for (n, name) in FILES[..4].iter().rev().enumerate() {
        let mut bytes = Vec::new();
        for i in 0..40 {
            serde_json::to_writer(&mut bytes, &record((n * 40 + i) as u64))?;
            bytes.push(b'\n');
        }
        if n == 3 {
            bytes.extend_from_slice(b"{private-invalid-content}\n");
        }
        fs::write(sink.directory.join(name), bytes)?;
    }
    let full = export(root.path())?;
    assert_eq!(full.records.len(), 160);
    assert_eq!(full.records.first().unwrap().observed_unix_ms, 0);
    assert_eq!(full.records.last().unwrap().observed_unix_ms, 159);
    assert_eq!(full.malformed_records, 1);
    assert!(!full.tail_truncated);
    assert!(!serde_json::to_string(&full)?.contains("private-invalid-content"));
    let recent = recent(root.path(), 32)?;
    assert_eq!(recent.records.len(), 32);
    assert!(recent.tail_truncated);
    Ok(())
}
#[test]
fn cached_reads_create_nothing_and_partial_records_are_preserved_but_never_exported() -> Result<()>
{
    let owner = private_root()?;
    let root = owner.path().join("missing");
    assert!(recent(&root, 32).is_err());
    assert!(!root.exists());
    let mut sink = Sink::open(owner.path())?;
    let current = sink.directory.join(FILES[0]);
    fs::write(&current, b"{untrusted_private_token")?;
    let partial = recent(owner.path(), 32)?;
    assert!(partial.records.is_empty() && partial.tail_truncated);
    sink.append(&record(1))?;
    let page = recent(owner.path(), 32)?;
    assert_eq!(page.records.len(), 1);
    assert_eq!(page.malformed_records, 1);
    assert!(!serde_json::to_string(&page)?.contains("untrusted_private_token"));
    assert!(fs::read(&current)?.starts_with(b"{untrusted_private_token\n"));
    assert!(recent(owner.path(), 0).is_err());
    assert!(recent(owner.path(), 33).is_err());
    Ok(())
}
#[test]
fn fifo_size_retention_purges_to_low_water_and_preserves_the_newest_complete_entries() -> Result<()>
{
    let root = private_root()?;
    let mut sink = Sink::open(root.path())?;
    assert_eq!(
        (HIGH_WATER_BYTES, LOW_WATER_BYTES),
        (200_000_000, 100_000_000)
    );
    let (high, low, segment) = (8192, 4096, 1024);
    let mut purges = 0;
    for n in 0..160 {
        let mut bytes = serde_json::to_vec(&record(n))?;
        let before = preflight(&sink.directory)?;
        let crosses = before + bytes.len() as u64 + 2 > high;
        sink.append_retained(&mut bytes, high, low, segment)?;
        let retained = preflight(&sink.directory)?;
        assert!(retained <= high);
        if crosses {
            assert!(retained <= low);
            purges += 1;
        }
        let page = export(root.path())?;
        assert_eq!(page.records.last().unwrap().observed_unix_ms, n);
        assert!(
            page.records
                .windows(2)
                .all(|pair| pair[1].observed_unix_ms == pair[0].observed_unix_ms + 1)
        );
        assert_eq!(page.malformed_records, 0);
    }
    assert!(purges >= 2);
    assert!(fs::read_dir(&sink.directory)?.count() <= FILES.len() + 1);
    drop(sink);
    let mut restarted = Sink::open(root.path())?;
    restarted.append_retained(&mut serde_json::to_vec(&record(160))?, high, low, segment)?;
    assert_eq!(
        export(root.path())?
            .records
            .last()
            .unwrap()
            .observed_unix_ms,
        160
    );
    Ok(())
}
#[test]
fn foreign_oversized_or_aliased_log_files_are_preserved_and_disable_writing() -> Result<()> {
    let root = private_root()?;
    let sink = Sink::open(root.path())?;
    let directory = sink.directory.clone();
    drop(sink);
    let unknown = directory.join("unknown.txt");
    fs::write(&unknown, b"Preserve unrelated fixture bytes.")?;
    assert!(Sink::open(root.path()).is_err());
    assert!(recent(root.path(), 32).is_err());
    assert_eq!(fs::read(&unknown)?, b"Preserve unrelated fixture bytes.");
    fs::remove_file(unknown)?;
    let current = directory.join(FILES[0]);
    let alias = root.path().join("alias");
    fs::write(&current, b"Retain aliased fixture.")?;
    fs::hard_link(&current, &alias)?;
    assert!(Sink::open(root.path()).is_err());
    assert!(recent(root.path(), 32).is_err());
    assert_eq!(fs::read(&alias)?, b"Retain aliased fixture.");
    fs::remove_file(alias)?;
    let oversized = OpenOptions::new().write(true).open(&current)?;
    oversized.set_len(FILE_BYTES + 1)?;
    drop(oversized);
    assert!(Sink::open(root.path()).is_err());
    assert_eq!(fs::metadata(current)?.len(), FILE_BYTES + 1);
    let before = fs::read(root.path().join(OWNER))?;
    fs::write(root.path().join(OWNER), b"{foreign}")?;
    assert!(Sink::open(root.path()).is_err());
    assert!(!before.is_empty());
    assert_eq!(fs::read(root.path().join(OWNER))?, b"{foreign}");
    Ok(())
}
#[test]
fn finite_queue_accounts_loss_and_flush_finishes_even_with_live_publisher_clones() -> Result<()> {
    let counters = Arc::new(Counters::default());
    let (sender, _receiver) = sync_channel(0);
    let blocked = Log {
        sender: Some(sender),
        counters: counters.clone(),
        instance: Incarnation::from_bytes([1; 16]),
    };
    let start = std::time::Instant::now();
    blocked.record(LogKind::ControlFailed, None, None);
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    assert_eq!(blocked.telemetry().dropped_records, 1);
    let root = private_root()?;
    let (log, guard) = Log::start(root.path().into(), Incarnation::from_bytes([2; 16]));
    // A deterministic loss receipt exercises the same pending counter as a full
    // queue, without depending on filesystem speed or racing a timed producer.
    log.counters.pending_drops.store(7, Ordering::Release);
    log.counters.dropped.store(7, Ordering::Release);
    log.record(LogKind::ServiceStarted, None, None);
    let clone = log.clone();
    log.record(LogKind::ServiceStopped, None, None);
    drop(guard);
    assert_eq!(log.telemetry().write_failures, 0);
    let page = recent(root.path(), 32)?;
    assert_eq!(page.records.len(), 3);
    assert_eq!(page.records[0].kind, LogKind::RecordsDropped);
    assert_eq!(page.records[0].count, 7);
    assert_eq!(page.records[2].kind, LogKind::ServiceStopped);
    clone.record(LogKind::ControlFailed, None, None);
    assert_eq!(clone.telemetry().dropped_records, 8);
    Ok(())
}
