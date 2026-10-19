// SPDX-License-Identifier: Apache-2.0
//! Private catalog construction, quota admission and atomic pointer publication.
use super::*;

pub(crate) fn save(directory: &Path, snapshot: &Snapshot, revision: Option<&str>) -> Result<()> {
    ensure!(
        snapshot.schema_version == SCHEMA_VERSION && snapshot.storage_format <= 1,
        "v2 evidence requires transactional publication finalizer"
    );
    save_inner(directory, snapshot, revision, snapshot, false)
}

pub(crate) fn save_catalog(
    directory: &Path,
    snapshot: &Snapshot,
    revision: &str,
    source: &dyn ArtifactSource,
    reservation: Option<&crate::quota::Reservation>,
    maximum: u64,
) -> Result<()> {
    ensure!(snapshot.storage_format == 1, "compact catalog required");
    if let Some(reservation) = reservation {
        let mut database = reservation
            .open_private_sqlite(directory.join("catalog.sqlite"), maximum)
            .with_context(|| format!("admit private fact catalog with {maximum}-byte quota cap"))?;
        save_connection(database.connection_mut(), snapshot, Some(revision), source, None, None)
            .with_context(|| format!("build private fact catalog with {maximum}-byte quota cap; previous generation retained"))?;
        return database.finish().context("finish private fact catalog");
    }
    save_inner(directory, snapshot, Some(revision), source, true)?;
    std::fs::rename(
        directory.join("index.sqlite3"),
        directory.join("catalog.sqlite"),
    )?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(directory.join("catalog.sqlite"))?
        .sync_all()?;
    Ok(())
}

fn header(snapshot: &Snapshot, revision: &str) -> PersistentRevision {
    PersistentRevision {
        token: None,
        manifest_sha256: None,
        storage_format: snapshot.storage_format,
        roots: snapshot.roots.clone(),
        inventory_sha256: (snapshot.storage_format == 1).then(|| {
            use sha2::Digest;
            format!(
                "{:x}",
                sha2::Sha256::digest(
                    serde_json::to_vec(snapshot).expect("snapshot serialization is infallible")
                )
            )
        }),
        revision: revision.into(),
        root: snapshot.root.clone(),
        collection: snapshot.collection.clone(),
        generation: snapshot.generation,
        files: snapshot.artifacts.len(),
        chunks: snapshot.artifacts.values().map(Artifact::chunk_count).sum(),
        source_bytes: snapshot.artifacts.values().map(|a| a.bytes).sum(),
        extractor_version: snapshot.extractor_version.clone(),
        semantic_producer: snapshot.analysis.as_ref().map(|a| a.producer.clone()),
        imported_facts: snapshot.imported_facts.len(),
        imported_relationships: snapshot.imported_relationships.len(),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn save_catalog_v2(
    directory: &Path,
    inventory: &airk_core::publication::InventoryV2,
    lineage: &airk_core::changes::LineageEnvelope,
    token: &airk_core::identity::RevisionToken,
    source: &dyn ArtifactSource,
    reservation: Option<&crate::quota::Reservation>,
    maximum: u64,
    base_directory: &Path,
    previous: Option<&crate::blobs::Catalog>,
) -> Result<()> {
    use std::io::Write;
    inventory.validate()?;
    lineage.validate()?;
    let bytes = inventory.canonical_bytes()?;
    ensure!(
        token.incarnation == inventory.incarnation
            && token.generation == inventory.generation
            && token.revision
                == airk_core::identity::revision_digest(
                    inventory.retrieval_specification.as_bytes(),
                    &bytes
                ),
        "catalog token mismatch"
    );
    let lineage_bytes = serde_json::to_vec(lineage)?;
    ensure!(
        raw_digest(&lineage_bytes) == inventory.lineage_digest
            && lineage.incarnation == inventory.incarnation
            && lineage.observation.generation == inventory.generation,
        "catalog lineage mismatch"
    );
    ensure!(
        reservation.is_some(),
        "v2 catalog requires a charged reservation"
    );
    let reservation = reservation.context("reservation missing")?;
    let mut lineage_file = reservation.create_file(directory.join("lineage.json"))?;
    lineage_file.write_all(&lineage_bytes)?;
    lineage_file.sync_all()?;
    drop(lineage_file);
    let mut database =
        reservation.open_private_sqlite(directory.join("catalog.sqlite"), maximum)?;
    let reuse = reuse_catalog(base_directory, previous, inventory)?;
    let written = (|| -> Result<()> {
        let snapshot = inventory.projection()?;
        save_connection(
            database.connection_mut(),
            &snapshot,
            Some(&token.revision.to_string()),
            source,
            Some(inventory),
            reuse
                .as_ref()
                .zip(previous.and_then(|c| c.inventory.as_ref())),
        )?;
        let conn = database.connection_mut();
        let tx = conn.transaction()?;
        tx.execute_batch("CREATE TABLE v2_catalog(id INTEGER PRIMARY KEY CHECK(id=1),token TEXT NOT NULL,inventory_digest TEXT NOT NULL,lineage_digest TEXT NOT NULL); CREATE TABLE files(path TEXT PRIMARY KEY,artifact_id TEXT NOT NULL UNIQUE,source_hash TEXT NOT NULL,profile_id TEXT NOT NULL,payload TEXT NOT NULL); CREATE INDEX files_source_hash ON files(source_hash); CREATE INDEX files_profile ON files(profile_id); CREATE TABLE change_events(generation INTEGER NOT NULL,event_id INTEGER NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(generation,event_id)); CREATE TABLE revision_headers(generation INTEGER PRIMARY KEY,token TEXT NOT NULL,payload TEXT NOT NULL); CREATE TABLE path_resolutions(artifact_id TEXT NOT NULL,path TEXT NOT NULL,payload TEXT NOT NULL); CREATE INDEX resolutions_path ON path_resolutions(path); CREATE INDEX resolutions_artifact ON path_resolutions(artifact_id);")?;
        tx.execute_batch("CREATE TABLE continuity_schema(id INTEGER PRIMARY KEY CHECK(id=1),version INTEGER NOT NULL); INSERT INTO continuity_schema VALUES(1,1); CREATE TABLE continuity_records(key TEXT PRIMARY KEY,path TEXT NOT NULL,record TEXT NOT NULL); CREATE INDEX continuity_path ON continuity_records(path);")?;
        tx.execute(
            "INSERT INTO v2_catalog VALUES(1,?1,?2,?3)",
            params![
                token.to_string(),
                raw_digest(&bytes).to_string(),
                inventory.lineage_digest.to_string()
            ],
        )?;
        for (path, file) in &inventory.files {
            let payload = serde_json::to_string(file)?;
            ensure!(
                payload.len() <= MAX_FILE_METADATA_BYTES,
                "file metadata exceeds byte bound"
            );
            tx.execute(
                "INSERT INTO files VALUES(?1,?2,?3,?4,?5)",
                params![
                    path,
                    file.state.artifact_id.to_string(),
                    file.state.source_hash.to_string(),
                    file.state.profile_id.to_string(),
                    payload
                ],
            )?;
            for record in &file.continuity {
                tx.execute(
                    "INSERT INTO continuity_records VALUES(?1,?2,?3)",
                    params![
                        serde_json::to_string(&record.key)?,
                        path,
                        serde_json::to_string(record)?
                    ],
                )?;
            }
        }
        for event in &lineage.events {
            tx.execute(
                "INSERT INTO change_events VALUES(?1,?2,?3)",
                params![
                    i64::try_from(event.generation)?,
                    i64::try_from(event.event_id)?,
                    serde_json::to_string(event)?
                ],
            )?;
        }
        for header in &lineage.ancestry {
            tx.execute(
                "INSERT INTO revision_headers VALUES(?1,?2,?3)",
                params![
                    i64::try_from(header.token.generation)?,
                    header.token.to_string(),
                    serde_json::to_string(header)?
                ],
            )?;
        }
        for resolver in &lineage.resolvers {
            tx.execute(
                "INSERT INTO path_resolutions VALUES(?1,?2,?3)",
                params![
                    resolver.artifact_id.to_string(),
                    resolver.from.path,
                    serde_json::to_string(resolver)?
                ],
            )?;
        }
        tx.pragma_update(None, "user_version", crate::IMMUTABLE_CATALOG_VERSION)?;
        tx.commit()?;
        Ok(())
    })();
    if let Err(error) = written {
        return Err(database.write_error(error));
    }
    // Authenticate again after row reuse; do not publish a copy of changed base bytes.
    if reuse.is_some() {
        crate::blobs::verify_v2_for_query(
            base_directory,
            previous
                .and_then(|c| c.receipt.as_ref())
                .context("reuse receipt missing")?,
        )?;
    }
    database.finish()?;
    Ok(())
}

pub(crate) fn publish_catalog_v2(
    directory: &Path,
    inventory: &airk_core::publication::InventoryV2,
    receipt: &airk_core::identity::PublicationReceipt,
    expected: &crate::ExpectedHead,
    reservation: &crate::quota::Reservation,
) -> Result<()> {
    inventory.validate()?;
    let bytes = inventory.canonical_bytes()?;
    let token = receipt.token;
    ensure!(
        token.incarnation == inventory.incarnation
            && token.generation == inventory.generation
            && token.revision
                == airk_core::identity::revision_digest(
                    inventory.retrieval_specification.as_bytes(),
                    &bytes
                )
            && receipt.inventory_digest == raw_digest(&bytes),
        "publication receipt does not bind inventory"
    );
    let prepared = crate::blobs::verify_v2_generation(directory, receipt)?;
    ensure!(
        prepared.canonical_bytes()? == bytes,
        "prepared inventory differs from publication candidate"
    );
    let lineage_bytes = crate::blobs::read_verified(
        &directory
            .join("generations")
            .join(token.revision.to_string())
            .join("lineage.json"),
        &inventory.lineage_digest.to_string(),
        airk_core::changes::MAX_LINEAGE_BYTES as u64,
    )?;
    let lineage: airk_core::changes::LineageEnvelope = serde_json::from_slice(&lineage_bytes)?;
    lineage.validate()?;
    ensure!(
        lineage.incarnation == token.incarnation
            && lineage.observation.generation == token.generation,
        "prepared lineage identity mismatch"
    );
    let (maximum, allowance) = pointer_budget(directory)?;
    reservation.charge(allowance)?;
    // The first SQLite pointer is prepared privately. Creating an empty root DB
    // would otherwise shadow a valid legacy JSON snapshot after a failed transaction.
    let private_root = if !directory.join("index.sqlite3").exists() {
        Some(
            tempfile::Builder::new()
                .prefix("pointer-")
                .tempdir_in(reservation.staging_directory()?)?,
        )
    } else {
        None
    };
    let pointer_directory = private_root
        .as_ref()
        .map_or(directory, |temporary| temporary.path());
    let mut conn = connect_root(pointer_directory, true)?;
    conn.execute_batch(&format!(
        "PRAGMA max_page_count={maximum}; PRAGMA cache_spill=OFF; PRAGMA temp_store=MEMORY;"
    ))?;
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    ensure!(
        &expected_head_connection(directory, &tx)? == expected,
        "publication parent changed"
    );
    if let crate::ExpectedHead::V2(parent) = expected {
        ensure!(
            parent.incarnation == token.incarnation
                && parent.next_generation()? == token.generation,
            "publication sequence/incarnation mismatch"
        );
        ensure!(
            lineage.observation.parent.as_ref() == Some(parent),
            "prepared lineage parent does not match committed parent"
        );
    } else {
        ensure!(
            lineage.observation.parent.is_none(),
            "first v2 observation cannot invent an earlier v2 parent"
        );
    }
    if !matches!(expected, crate::ExpectedHead::Empty) {
        let has_revision: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='revision')",
            [],
            |row| row.get(0),
        )?;
        let (old_root, old_collection, old_generation) = if has_revision {
            let (root, collection, generation): (String, String, i64) = tx.query_row(
                "SELECT root,collection,generation FROM revision WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            (root, collection, u64::try_from(generation)?)
        } else {
            let snapshot: Snapshot =
                serde_json::from_reader(std::fs::File::open(directory.join("snapshot.json"))?)?;
            (snapshot.root, snapshot.collection, snapshot.generation)
        };
        ensure!(
            old_root == inventory.root
                && old_collection == inventory.collection
                && old_generation.checked_add(1) == Some(token.generation),
            "publication changes collection binding or skips sequence"
        );
    }
    tx.execute_batch("CREATE TABLE IF NOT EXISTS revision(id INTEGER PRIMARY KEY CHECK(id=1),root TEXT NOT NULL,collection TEXT NOT NULL,generation INTEGER NOT NULL,schema_version INTEGER NOT NULL,payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS index_components(id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS publication_receipts(revision TEXT PRIMARY KEY,incarnation TEXT NOT NULL,generation INTEGER NOT NULL,payload TEXT NOT NULL); CREATE INDEX IF NOT EXISTS receipts_scope ON publication_receipts(incarnation,generation);")?;
    let mut header = header(&inventory.projection()?, &token.revision.to_string());
    header.token = Some(token);
    header.inventory_sha256 = Some(receipt.inventory_digest.to_string());
    header.manifest_sha256 = Some(receipt.manifest_digest.to_string());
    let header_payload = serde_json::to_string(&header)?;
    ensure!(
        header_payload.len() <= 131_072,
        "active v2 pointer exceeds 128 KiB"
    );
    tx.execute(
        "INSERT INTO publication_receipts VALUES(?1,?2,?3,?4)",
        params![
            token.revision.to_string(),
            token.incarnation.to_string(),
            i64::try_from(token.generation)?,
            serde_json::to_string(receipt)?
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO revision VALUES(1,?1,?2,?3,4,?4)",
        params![
            inventory.root,
            inventory.collection,
            i64::try_from(inventory.generation)?,
            header_payload
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO index_components VALUES(1,?1)",
        [&header_payload],
    )?;
    history_schema(&tx)?;
    tx.pragma_update(None, "user_version", publication_root_version(&tx)?)?;
    tx.commit()?;
    if let Some(private_root) = private_root {
        let (busy, _, _): (i64, i64, i64) =
            conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
        ensure!(busy == 0, "private pointer checkpoint remained busy");
        conn.close().map_err(|(_, error)| error)?;
        let staged = private_root.path().join("index.sqlite3").canonicalize()?;
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&staged)?
            .sync_all()?;
        let target = directory.canonicalize()?.join("index.sqlite3");
        tempfile::TempPath::try_from_path(staged)?.persist_noclobber(target)?;
    }
    Ok(())
}

pub(crate) fn catalog_page_budget(reserved: u64, physical: u64, pointer: u64) -> Result<u64> {
    let required = pointer
        .checked_add(16_384)
        .context("minimum catalog and pointer allowance overflow")?;
    if reserved < required {
        return Err(crate::quota::QuotaPressure {
            scope: crate::quota::PressureScope::Reservation,
            required_bytes: required,
            available_bytes: reserved,
            protected_bytes: None,
        }
        .into());
    }
    // SQLite's maximum representable page count is a format limit, not an
    // application catalog ceiling. Logical policy and observed capacity govern
    // every smaller admission.
    // This observation precedes lineage writes and the quota charge's fresh
    // capacity probe. Do not admit virtually all free space: concurrent writes
    // on the same filesystem would make even a tiny catalog fail admission.
    // Keep 10% of observed growth available in addition to OS safety headroom.
    // It remains a point-in-time bound, not a physical reservation.
    let physical_admission = physical.saturating_sub(physical / 10);
    let maximum = (reserved.min(physical_admission).saturating_sub(pointer) / 4096)
        .min(u64::from(u32::MAX) - 1)
        * 4096;
    ensure!(
        maximum >= 16_384,
        "private catalog admission exhausted: {reserved} reserved bytes and {physical} physical growth bytes remain, {pointer} needed for active pointer publication"
    );
    Ok(maximum)
}

pub(crate) fn pointer_budget(directory: &Path) -> Result<(u64, u64)> {
    let (page_size, pages) = if directory.join("index.sqlite3").exists() {
        let conn = connect_root(directory, false)?;
        let page_size: u32 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        let pages: u32 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        (u64::from(page_size), u64::from(pages))
    } else {
        (4096, 0)
    };
    let maximum = pages
        .checked_add(128)
        .context("pointer page bound overflow")?
        .max(128);
    // page_count includes pages which may exist only in the WAL. A checkpoint
    // can grow the main file by that entire difference, including during legacy
    // migration. Count physical file lengths separately from logical page count.
    let file_bytes = |name: &str| -> Result<u64> {
        match std::fs::metadata(directory.join(name)) {
            Ok(metadata) => Ok(metadata.len()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
            Err(error) => Err(error.into()),
        }
    };
    let database_growth = maximum
        .checked_mul(page_size)
        .context("pointer database bound overflow")?
        .saturating_sub(file_bytes("index.sqlite3")?);
    // With spill disabled, one transaction appends each dirty page at most once.
    // WAL frames use a 24-byte header; allow 32 for conservative framing. The
    // shared-memory index uses 32 KiB per roughly 4096 frames (8 bytes/frame),
    // plus headers and allocation rounding. Existing WAL frames matter too.
    // Reserve the full SHM allocation: a reader can close and remove sidecars
    // between admission and publication, requiring their recreation.
    let existing_frames = file_bytes("index.sqlite3-wal")? / (page_size + 24);
    let shared_growth = existing_frames
        .checked_add(maximum)
        .and_then(|frames| frames.checked_mul(8))
        .and_then(|bytes| bytes.checked_add(65_536))
        .context("pointer shared-memory bound overflow")?;
    let allowance = maximum
        .checked_mul(page_size + 32)
        .and_then(|value| value.checked_add(database_growth))
        .and_then(|value| value.checked_add(shared_growth))
        .and_then(|value| value.checked_add(32))
        .context("pointer journal bound overflow")?;
    Ok((maximum, allowance))
}

pub(crate) fn publish_catalog(
    directory: &Path,
    snapshot: &Snapshot,
    revision: &str,
    reservation: Option<&crate::quota::Reservation>,
) -> Result<()> {
    ensure!(
        snapshot.storage_format == 1 && crate::blobs::valid_digest(revision),
        "invalid compact catalog publication"
    );
    let target = directory
        .join("generations")
        .join(revision)
        .join("catalog.sqlite");
    ensure!(target.is_file(), "prepared immutable catalog missing");
    let (maximum, allowance) = pointer_budget(directory)?;
    if let Some(reservation) = reservation {
        reservation.charge(allowance).with_context(|| {
            format!("admit active catalog pointer with {allowance}-byte main/WAL/SHM allowance")
        })?;
    }
    publish_pointer(directory, snapshot, revision, maximum)
        .with_context(|| format!("publish active catalog pointer with {maximum}-page cap and {allowance}-byte write allowance"))
}

fn publish_pointer(
    directory: &Path,
    snapshot: &Snapshot,
    revision: &str,
    maximum: u64,
) -> Result<()> {
    let mut conn = connect_root(directory, true)?;
    conn.execute_batch(&format!(
        "PRAGMA max_page_count={maximum}; PRAGMA cache_spill=OFF; PRAGMA temp_store=MEMORY;"
    ))?;
    let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(version <= 2, "unsupported SQLite schema");
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS revision(id INTEGER PRIMARY KEY CHECK(id=1),root TEXT NOT NULL,collection TEXT NOT NULL,generation INTEGER NOT NULL,schema_version INTEGER NOT NULL,payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS index_components(id INTEGER PRIMARY KEY CHECK(id=1),payload TEXT NOT NULL);")?;
    let mut pointer = Snapshot::empty(snapshot.root.clone());
    pointer.collection = snapshot.collection.clone();
    pointer.roots = snapshot.roots.clone();
    pointer.generation = snapshot.generation;
    pointer.storage_format = 1;
    pointer.persistent_indexes = true;
    pointer.embedding_model = snapshot.embedding_model.clone();
    pointer.extractor_version = snapshot.extractor_version.clone();
    let payload = serde_json::to_string(&pointer)?;
    ensure!(
        payload.len() <= 131_072,
        "active pointer metadata exceeds 128 KiB"
    );
    tx.execute(
        "INSERT OR REPLACE INTO revision VALUES(1,?1,?2,?3,?4,?5)",
        params![
            snapshot.root,
            snapshot.collection,
            i64::try_from(snapshot.generation)?,
            snapshot.schema_version,
            payload
        ],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO index_components VALUES(1,?1)",
        [serde_json::to_string(&header(snapshot, revision))?],
    )?;
    tx.execute_batch("PRAGMA user_version=2;")?;
    tx.commit()?;
    Ok(())
}

fn save_inner(
    directory: &Path,
    snapshot: &Snapshot,
    revision: Option<&str>,
    source: &dyn ArtifactSource,
    private: bool,
) -> Result<()> {
    std::fs::create_dir_all(directory)?;
    let mut conn = connect(directory, true)?;
    if private {
        conn.execute_batch(
            "PRAGMA journal_mode=OFF; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-16384; PRAGMA cache_spill=ON;",
        )?;
    }
    save_connection(&mut conn, snapshot, revision, source, None, None)
}

fn save_connection(
    conn: &mut Connection,
    snapshot: &Snapshot,
    revision: Option<&str>,
    source: &dyn ArtifactSource,
    inventory: Option<&airk_core::publication::InventoryV2>,
    reuse: Option<(&Connection, &airk_core::publication::InventoryV2)>,
) -> Result<()> {
    let payload = serde_json::to_string(snapshot)?;
    ensure!(
        payload.len()
            <= if snapshot.storage_format == 1 {
                crate::blobs::MAX_CATALOG_BYTES as usize
            } else {
                536_870_912
            },
        "catalog payload exceeds byte bound; previous generation retained"
    );
    let version: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    ensure!(version <= 2, "unsupported SQLite schema");
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS revision(id INTEGER PRIMARY KEY CHECK(id=1), root TEXT NOT NULL, collection TEXT NOT NULL, generation INTEGER NOT NULL, schema_version INTEGER NOT NULL, payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS index_components(id INTEGER PRIMARY KEY CHECK(id=1), payload TEXT NOT NULL); DELETE FROM index_components; PRAGMA user_version=1;")?;
    let measure_context =
        inventory.is_some_and(|i| i.retrieval_specification.ends_with("/source-context-1"));
    if measure_context {
        tx.execute_batch("CREATE TABLE usage_sources(path TEXT PRIMARY KEY,source_hash TEXT NOT NULL,payload TEXT NOT NULL)")?;
    }
    if inventory.is_some() {
        crate::normalized::create(&tx)?;
    } else {
        tx.execute_batch("CREATE TABLE IF NOT EXISTS facts(path TEXT NOT NULL,name TEXT NOT NULL,folded_name TEXT NOT NULL,qualified_name TEXT NOT NULL,symbol_id TEXT NOT NULL,kind TEXT NOT NULL,language TEXT NOT NULL,is_code INTEGER NOT NULL,is_document INTEGER NOT NULL,line INTEGER NOT NULL,payload TEXT NOT NULL); CREATE TABLE IF NOT EXISTS relationships(source TEXT NOT NULL,target TEXT NOT NULL,kind TEXT NOT NULL,path TEXT NOT NULL,line INTEGER NOT NULL,payload TEXT NOT NULL); DELETE FROM facts; DELETE FROM relationships;")?;
    }
    if let Some(revision) = revision {
        let header = header(snapshot, revision);
        tx.execute(
            "INSERT INTO index_components VALUES(1,?1)",
            [serde_json::to_string(&header)?],
        )?;
    }
    tx.execute(
        "INSERT OR REPLACE INTO revision VALUES(1,?1,?2,?3,?4,?5)",
        params![
            snapshot.root,
            snapshot.collection,
            i64::try_from(snapshot.generation)?,
            snapshot.schema_version,
            payload
        ],
    )?;
    {
        let mut writer = inventory.map(|_| crate::normalized::Writer::new(&tx));
        for path in snapshot.artifacts.keys() {
            if let (Some((base, old)), Some(next), Some(writer)) =
                (reuse, inventory, writer.as_mut())
                && old.files.get(path) == next.files.get(path)
                && old.files.contains_key(path)
            {
                if measure_context {
                    let reused: Option<String> = if table_exists(base, "usage_sources")? {
                        base.query_row("SELECT payload FROM usage_sources WHERE path=?1 AND source_hash=?2 AND length(CAST(payload AS BLOB)) <= 1024",
                            params![path, next.files[path].state.source_hash.to_string()], |row| row.get(0)).optional()?
                    } else {
                        None
                    };
                    let size = match reused {
                        Some(value) => value,
                        None => {
                            serde_json::to_string(&airk_core::usage::SourceContextSize::measure(
                                &source.load_artifact(path)?,
                            )?)?
                        }
                    };
                    tx.execute(
                        "INSERT INTO usage_sources VALUES(?1,?2,?3)",
                        params![path, next.files[path].state.source_hash.to_string(), size],
                    )?;
                }
                writer.copy_native(base, path, &next.files[path].state.source_hash.to_string())?;
                continue;
            }
            let a = source.load_artifact(path)?;
            if measure_context {
                let size = airk_core::usage::SourceContextSize::measure(&a)?;
                tx.execute(
                    "INSERT INTO usage_sources VALUES(?1,?2,?3)",
                    params![path, a.content_hash, serde_json::to_string(&size)?],
                )?;
            }
            let keys = airk_core::continuity::FactKeyIndex::new(
                inventory
                    .and_then(|inventory| inventory.files.get(path))
                    .map_or(&[], |file| file.continuity.as_slice()),
            );
            for (ordinal, f) in a.facts.iter().enumerate() {
                insert_hit(
                    &tx,
                    &mut writer,
                    &FactHit {
                        continuity_key: keys.key(ordinal, f),
                        name: f.name.clone(),
                        kind: f.kind.clone(),
                        syntax_kind: f.syntax_kind.clone(),
                        language: a.language.clone(),
                        document: f.document.clone(),
                        code: f.code.clone(),
                        citation: Citation {
                            path: a.path.clone(),
                            content_hash: a.content_hash.clone(),
                            start_line: f.start_line,
                            end_line: f.end_line,
                            location: f.location.clone(),
                        },
                    },
                    false,
                )?;
            }
        }
        for f in &snapshot.imported_facts {
            let mut imported = f.clone();
            imported.continuity_key = None;
            insert_hit(&tx, &mut writer, &imported, true)?;
        }

        for edge in &snapshot.imported_relationships {
            write_edge(&tx, &mut writer, edge, true)?;
        }
    }
    if inventory.is_some() {
        crate::normalized::indexes(&tx)?;
    } else {
        // Build lookup B-trees once after the private bulk load; existing legacy
        // indexes are retained. All indexes commit with the same evidence rows.
        tx.execute_batch("CREATE INDEX IF NOT EXISTS facts_name ON facts(name,kind); CREATE INDEX IF NOT EXISTS facts_qualified ON facts(qualified_name); CREATE INDEX IF NOT EXISTS facts_symbol ON facts(symbol_id); CREATE INDEX IF NOT EXISTS facts_path ON facts(path,line); CREATE INDEX IF NOT EXISTS edges_source ON relationships(source,kind); CREATE INDEX IF NOT EXISTS edges_target ON relationships(target,kind);")?;
    }
    tx.commit()?;
    Ok(())
}

fn reuse_catalog(
    directory: &Path,
    previous: Option<&crate::blobs::Catalog>,
    next: &airk_core::publication::InventoryV2,
) -> Result<Option<Connection>> {
    let Some(previous) = previous else {
        return Ok(None);
    };
    let (Some(inventory), Some(receipt)) = (&previous.inventory, &previous.receipt) else {
        return Ok(None);
    };
    ensure!(
        receipt_connection(&connect_root(directory, false)?, &receipt.token)?.as_ref()
            == Some(receipt),
        "reuse requires a committed local receipt"
    );
    ensure!(
        raw_digest(&inventory.canonical_bytes()?) == receipt.inventory_digest,
        "reuse inventory changed after authentication"
    );
    if inventory.incarnation != next.incarnation
        || inventory.collection != next.collection
        || inventory.root != next.root
        || inventory.roots != next.roots
        || inventory.extractor_version != next.extractor_version
    {
        return Ok(None);
    }
    crate::blobs::verify_v2_for_query(directory, receipt)?;
    let conn = open_immutable_catalog(
        &directory
            .join("generations")
            .join(receipt.token.revision.to_string())
            .join("catalog.sqlite"),
    )?;
    if !crate::normalized::present(&conn)? {
        return Ok(None);
    }
    validate_v2_catalog(&conn, &receipt.token, &receipt.inventory_digest)?;
    Ok(Some(conn))
}

fn insert_hit(
    tx: &rusqlite::Transaction<'_>,
    writer: &mut Option<crate::normalized::Writer<'_>>,
    hit: &FactHit,
    imported: bool,
) -> Result<()> {
    let code = hit.code.as_ref();
    if let Some(writer) = writer {
        writer.fact(hit, imported)?;
    } else {
        tx.prepare_cached("INSERT INTO facts VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)")?
            .execute(params![
                hit.citation.path,
                hit.name,
                hit.name.to_lowercase(),
                code.map_or("", |c| c.qualified_name.as_str()),
                code.map_or("", |c| c.symbol_id.as_str()),
                hit.kind,
                hit.language,
                code.is_some(),
                code.is_none() && !matches!(hit.kind.as_str(), "definition" | "import"),
                i64::try_from(hit.citation.start_line)?,
                serde_json::to_string(hit)?
            ])?;
    }
    if let Some(c) = code {
        if let Some(owner) = &c.owner_id {
            write_edge(
                tx,
                writer,
                &RelationshipHit {
                    source: owner.clone(),
                    target: c.symbol_id.clone(),
                    kind: "contains".into(),
                    resolution: "syntax_candidate".into(),
                    basis: EvidenceBasis::Syntax,
                    producer: "airk-syntax-containment".into(),
                    producer_version: "native-facts/4".into(),
                    citation: hit.citation.clone(),
                },
                imported,
            )?;
        }
        if let Some(target) = &c.target {
            write_edge(
                tx,
                writer,
                &RelationshipHit {
                    source: c
                        .owner_id
                        .clone()
                        .unwrap_or_else(|| format!("file:{}", hit.citation.path)),
                    target: target.clone(),
                    kind: hit.kind.clone(),
                    resolution: "syntax_candidate".into(),
                    basis: c.basis.clone(),
                    producer: c.producer.clone(),
                    producer_version: c.producer_version.clone(),
                    citation: hit.citation.clone(),
                },
                imported,
            )?;
        }
    }
    Ok(())
}

fn write_edge(
    tx: &rusqlite::Transaction<'_>,
    writer: &mut Option<crate::normalized::Writer<'_>>,
    edge: &RelationshipHit,
    imported: bool,
) -> Result<()> {
    if let Some(writer) = writer {
        writer.edge(edge, imported)
    } else {
        let mut statement =
            tx.prepare_cached("INSERT INTO relationships VALUES(?1,?2,?3,?4,?5,?6)")?;
        insert_edge(&mut statement, edge)
    }
}

fn insert_edge(stmt: &mut rusqlite::Statement<'_>, edge: &RelationshipHit) -> Result<()> {
    stmt.execute(params![
        edge.source,
        edge.target,
        edge.kind,
        edge.citation.path,
        i64::try_from(edge.citation.start_line)?,
        serde_json::to_string(edge)?
    ])?;
    Ok(())
}
