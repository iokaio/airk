// SPDX-License-Identifier: Apache-2.0
//! Isolated mechanism experiments, never a runtime switch to WAL or a new store format.
use super::lexical::{self, LexicalManifest};
use airk_core::{Artifact, Chunk, DocumentLocation, identity::framed_digest};
use anyhow::{Result, ensure};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};

fn writer(path: &Path) -> Result<Connection> {
    let connection = Connection::open(path)?;
    connection.busy_timeout(Duration::ZERO)?;
    connection.execute_batch("PRAGMA page_size=4096; PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; PRAGMA synchronous=FULL;")?;
    let mode: String = connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    ensure!(mode == "wal", "spike did not establish WAL");
    Ok(connection)
}

fn main_digest(path: &Path) -> Result<Vec<u8>> {
    Ok(Sha256::digest(fs::read(path)?).to_vec())
}

fn head(connection: &Connection) -> Result<(i64, LexicalManifest, String)> {
    let (sequence, bytes, marker): (i64, Vec<u8>, String) = connection.query_row(
        "SELECT sequence,seal,marker FROM spike_head WHERE id=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let digest = framed_digest(b"current-lexical-manifest/1", &[&bytes]);
    Ok((sequence, LexicalManifest::decode(&bytes, digest)?, marker))
}

#[test]
fn wal_snapshot_seals_cover_committed_overlay_while_main_bytes_and_old_read_remain_unchanged()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("spike.sqlite3");
    let mut writer = writer(&path)?;
    lexical::create(&writer)?;
    writer.execute_batch("CREATE TABLE spike_head(id INTEGER PRIMARY KEY,sequence INTEGER NOT NULL,seal BLOB NOT NULL,marker TEXT NOT NULL)")?;
    let empty = lexical::capture(&writer, framed_digest(b"spike-updates", &[]))?;
    writer.execute(
        "INSERT INTO spike_head VALUES(1,0,?1,'old')",
        [empty.canonical_bytes()?],
    )?;
    writer.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
    let before = main_digest(&path)?;
    let reader = Connection::open(&path)?;
    reader.execute_batch("PRAGMA query_only=ON; BEGIN")?;
    let old = head(&reader)?;
    old.1.verify(&reader)?;
    let text = "Original cobalt harbor publication fixture.";
    let artifact = Artifact {
        path: "memo.md".into(),
        content_hash: format!("{:x}", Sha256::digest(text.as_bytes())),
        bytes: text.len(),
        chunks: vec![Chunk {
            start_line: 1,
            end_line: 1,
            text: text.into(),
            location: DocumentLocation::Lines { start: 1, end: 1 },
        }],
        language: "markdown".into(),
        extraction_method: "original-spike-fixture".into(),
        parse_status: "ok".into(),
        facts: vec![],
        retained: None,
    };
    let transaction = writer.transaction()?;
    lexical::replace(&transaction, &artifact.path, Some(&artifact))?;
    lexical::flush(&transaction)?;
    let next = lexical::capture(
        &transaction,
        framed_digest(b"spike-updates", &[text.as_bytes()]),
    )?;
    transaction.execute(
        "UPDATE spike_head SET sequence=1,seal=?1,marker='new' WHERE id=1",
        [next.canonical_bytes()?],
    )?;
    transaction.commit()?;
    assert_eq!(
        main_digest(&path)?,
        before,
        "main-file bytes alone cannot witness this commit"
    );
    assert_eq!((head(&reader)?.0, head(&reader)?.2), (0, "old".into()));
    old.1.verify(&reader)?;
    let latest = Connection::open(&path)?;
    latest.execute_batch("PRAGMA query_only=ON; BEGIN")?;
    let current = head(&latest)?;
    assert_eq!((current.0, current.2), (1, "new".into()));
    current.1.verify(&latest)?;
    assert_ne!(old.1.root, current.1.root);
    drop(latest);
    // A committed overlay changing evidence without its sealed head must fail verification.
    writer.execute(
        "UPDATE current_lexical_docs SET terms=?1 WHERE path='memo.md'",
        ["forged terms"],
    )?;
    assert_eq!(main_digest(&path)?, before);
    let tampered = Connection::open(&path)?;
    tampered.execute_batch("PRAGMA query_only=ON; BEGIN")?;
    let saved = head(&tampered)?;
    assert!(saved.1.verify(&tampered).is_err());
    // The admitted old SQLite snapshot remains internally consistent throughout.
    old.1.verify(&reader)?;
    assert_eq!(head(&reader)?.0, 0);
    Ok(())
}

#[test]
fn pinned_reader_allows_wal_to_outgrow_main_capacity_and_zero_retained_journal_limit() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let path = root.path().join("pressure.sqlite3");
    let writer = writer(&path)?;
    writer.execute_batch("PRAGMA max_page_count=16; PRAGMA journal_size_limit=0; CREATE TABLE counter(id INTEGER PRIMARY KEY,value TEXT); INSERT INTO counter VALUES(1,'zero'); PRAGMA wal_checkpoint(TRUNCATE)")?;
    let pages: i64 = writer.pragma_query_value(None, "max_page_count", |row| row.get(0))?;
    assert_eq!(pages, 16);
    let reader = Connection::open(&path)?;
    reader.execute_batch("PRAGMA query_only=ON; BEGIN")?;
    let pinned: String =
        reader.query_row("SELECT value FROM counter WHERE id=1", [], |row| row.get(0))?;
    assert_eq!(pinned, "zero");
    for revision in 0..64 {
        writer.execute(
            "UPDATE counter SET value=?1 WHERE id=1",
            params![format!("revision-{revision:08}")],
        )?;
    }
    let wal = path.with_file_name("pressure.sqlite3-wal");
    assert!(fs::metadata(&path)?.len() <= pages as u64 * 4096);
    assert!(
        fs::metadata(&wal)?.len() > pages as u64 * 4096,
        "main page admission is not WAL admission"
    );
    let (busy, log, checkpointed): (i64, i64, i64) =
        writer.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
    assert_eq!(busy, 1);
    assert!(log > checkpointed);
    reader.execute_batch("ROLLBACK")?;
    let busy: i64 = writer.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
    assert_eq!(busy, 0);
    assert_eq!(fs::metadata(wal)?.len(), 0);
    Ok(())
}
