// SPDX-License-Identifier: Apache-2.0
//! The single writer of one plain index: one SQLite transaction per batch.
use super::{
    DATABASE_FILE, FORMAT, FileChange, FileEntry, FileStat, MAX_METADATA_BYTES, MAX_RECORD_BYTES,
    MAX_RESOURCE_BYTES, ResourceChange, schema, validate_path,
    vectors::{self, VectorFile},
};
use airk_core::identity::Sha256Digest;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::path::{Path, PathBuf};

/// Citation units of one file; the old store allowed the same number.
pub const MAX_CHUNKS_PER_FILE: usize = 8192;
pub const MAX_SELECTORS_PER_FILE: usize = 65_536;
pub const MAX_UNITS_PER_FILE: usize = airk_core::extraction::MAX_EXTRACTION_CHUNKS;
/// The changes log keeps this many publications.
pub const RETAINED_CHANGE_SEQUENCES: i64 = 4096;

const STATE_NOT_SELECTED: i64 = 0;
const STATE_PENDING: i64 = 1;
const STATE_DONE: i64 = 2;
const STATE_FAILED: i64 = 3;

/// One base publication: changed files, stat refreshes, resources and an
/// optional change record, in one transaction.
#[derive(Clone, Debug, Default)]
pub struct Batch {
    pub files: Vec<FileChange>,
    /// Unchanged files whose observed stat moved; no new sequence on their own.
    pub stats: Vec<(String, FileStat)>,
    pub resources: Vec<ResourceChange>,
    /// The caller's change record for this publication, kept in the bounded log.
    pub change: Option<Vec<u8>>,
    /// Caller meta values (for example `scope` or `collection_state`), written
    /// in the same transaction. The engine's own keys are refused.
    pub meta: Vec<(String, Vec<u8>)>,
}

/// Meta keys the engine owns; a batch may not write them.
pub(super) const ENGINE_META_KEYS: [&str; 6] = [
    "format",
    "sequence",
    "vector_dimensions",
    "vector_pipeline",
    "vector_generation",
    "vector_rows",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub sequence: u64,
    pub files_changed: usize,
    pub chunks_added: u64,
    pub payloads_written: usize,
}

/// The embedding pipeline that produced every row of the vector file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VectorSetup {
    pub dimensions: u32,
    pub pipeline: Sha256Digest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitEntry {
    pub ordinal: u32,
    pub embedding_key: Sha256Digest,
    /// The caller's member spans, returned unchanged with a vector hit.
    pub members: Vec<u8>,
}

/// One file's semantic units, planned against the source hash it names.
#[derive(Clone, Debug)]
pub struct FileVectors {
    pub path: String,
    pub source_hash: Sha256Digest,
    pub units: Vec<UnitEntry>,
    /// Planning or inference failed for this file; it stays without units.
    pub failed: bool,
}

#[derive(Clone, Debug)]
pub struct VectorBatch {
    pub setup: VectorSetup,
    pub files: Vec<FileVectors>,
    /// Vectors for keys that may be new. Keys already stored are ignored.
    pub vectors: Vec<(Sha256Digest, Vec<f32>)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VectorCommit {
    pub sequence: u64,
    pub rows_added: u64,
    pub files_done: usize,
    /// Files whose source changed or that were removed after planning.
    pub files_stale: usize,
}

pub struct IndexWriter {
    directory: PathBuf,
    connection: Connection,
}

fn apply_resource(transaction: &Transaction<'_>, resource: &ResourceChange) -> Result<()> {
    ensure!(
        !resource.key.is_empty()
            && resource.key.len() <= 256
            && resource
                .body
                .as_ref()
                .is_none_or(|body| body.len() <= MAX_RESOURCE_BYTES),
        "invalid index resource"
    );
    match &resource.body {
        Some(body) => transaction.execute(
            "INSERT INTO resources(key,body) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET body=excluded.body",
            params![resource.key, body],
        )?,
        None => transaction.execute("DELETE FROM resources WHERE key=?1", [&resource.key])?,
    };
    Ok(())
}

fn digest_bytes(digest: &Sha256Digest) -> &[u8] {
    digest.as_bytes().as_slice()
}

fn digest_from(bytes: Vec<u8>) -> Result<Sha256Digest> {
    Ok(Sha256Digest::from_bytes(bytes.try_into().map_err(
        |_| anyhow::anyhow!("stored digest has the wrong length"),
    )?))
}

fn sequence(connection: &Connection) -> Result<u64> {
    Ok(u64::try_from(
        schema::meta_integer(connection, "sequence")?.context("index sequence missing")?,
    )?)
}

impl IndexWriter {
    /// Create an empty index in an existing, empty-of-index directory.
    pub fn create(directory: &Path) -> Result<Self> {
        let connection = schema::create(&directory.join(DATABASE_FILE))?;
        connection.execute_batch("BEGIN IMMEDIATE")?;
        schema::set_meta(&connection, "format", FORMAT)?;
        schema::set_meta(&connection, "sequence", 0i64)?;
        connection.execute_batch("COMMIT")?;
        Ok(Self {
            directory: directory.to_path_buf(),
            connection,
        })
    }

    pub fn open(directory: &Path) -> Result<Self> {
        let connection = schema::open_writer(&directory.join(DATABASE_FILE))?;
        let writer = Self {
            directory: directory.to_path_buf(),
            connection,
        };
        writer.remove_retired_vector_files();
        Ok(writer)
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn sequence(&self) -> Result<u64> {
        sequence(&self.connection)
    }

    /// Publish one batch. Either all of it is visible to the next reader or none.
    pub fn apply(&mut self, batch: Batch) -> Result<Commit> {
        let mut previous: Option<&str> = None;
        for change in &batch.files {
            validate_path(change.path())?;
            ensure!(
                previous.is_none_or(|prior| prior < change.path()),
                "batch paths are unsorted or repeated"
            );
            previous = Some(change.path());
        }
        for resource in &batch.resources {
            ensure!(
                !resource.key.is_empty()
                    && resource.key.len() <= 256
                    && resource
                        .body
                        .as_ref()
                        .is_none_or(|body| body.len() <= MAX_RESOURCE_BYTES),
                "invalid index resource"
            );
        }
        ensure!(
            batch
                .change
                .as_ref()
                .is_none_or(|body| body.len() <= MAX_METADATA_BYTES),
            "index change record exceeds byte limit"
        );
        for (key, value) in &batch.meta {
            ensure!(
                !key.is_empty()
                    && key.len() <= 256
                    && !ENGINE_META_KEYS.contains(&key.as_str())
                    && value.len() <= MAX_METADATA_BYTES,
                "invalid index meta value"
            );
        }
        let publishes = !batch.files.is_empty()
            || !batch.resources.is_empty()
            || batch.change.is_some()
            || !batch.meta.is_empty();
        let transaction = self.connection.transaction()?;
        let next = sequence(&transaction)?
            .checked_add(u64::from(publishes))
            .context("index sequence overflow")?;
        let mut commit = Commit {
            sequence: next,
            files_changed: batch.files.len(),
            chunks_added: 0,
            payloads_written: 0,
        };
        // Remove every changed file first: a moved handle or payload may pass
        // from a later path to an earlier one within the same batch.
        let mut retired = Vec::new();
        for change in &batch.files {
            retired.extend(remove_file(&transaction, change.path())?);
        }
        for change in &batch.files {
            if let FileChange::Upsert(entry) = change {
                let (chunks, written) = insert_file(&transaction, entry, next)?;
                commit.chunks_added += chunks;
                commit.payloads_written += usize::from(written);
            }
        }
        for digest in retired {
            transaction.execute(
                "DELETE FROM payloads WHERE digest=?1 AND NOT EXISTS (SELECT 1 FROM files WHERE payload=?1)",
                [digest_bytes(&digest)],
            )?;
        }
        for (path, stat) in &batch.stats {
            validate_path(path)?;
            transaction.execute(
                "UPDATE files SET stat_size=?2, stat_modified_ns=?3, stat_file_id=?4 WHERE path=?1",
                params![
                    path,
                    i64::try_from(stat.size)?,
                    stat.modified_ns,
                    stat.file_id
                ],
            )?;
        }
        for resource in &batch.resources {
            apply_resource(&transaction, resource)?;
        }
        if let Some(body) = &batch.change {
            transaction.execute(
                "INSERT INTO changes(sequence,ordinal,body) VALUES(?1,0,?2)",
                params![i64::try_from(next)?, body],
            )?;
            transaction.execute(
                "DELETE FROM changes WHERE sequence<=?1",
                [i64::try_from(next)? - RETAINED_CHANGE_SEQUENCES],
            )?;
        }
        for (key, value) in &batch.meta {
            schema::set_meta(&transaction, key, value.as_slice())?;
        }
        if publishes {
            schema::set_meta(&transaction, "sequence", i64::try_from(next)?)?;
        }
        transaction.commit()?;
        Ok(commit)
    }

    /// Select the embedding pipeline. A different pipeline starts an empty
    /// vector generation and returns every file with vectors to pending.
    /// `None` turns vectors off and clears them.
    pub fn configure_vectors(&mut self, setup: Option<&VectorSetup>) -> Result<bool> {
        let current = vector_setup(&self.connection)?;
        if current.as_ref().map(|(setup, _, _)| setup) == setup {
            return Ok(false);
        }
        let generation = current.as_ref().map_or(0, |(_, generation, _)| *generation) + 1;
        if let Some(setup) = setup {
            VectorFile::create(&self.directory, generation, setup)?;
        }
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM units", [])?;
        transaction.execute("DELETE FROM vector_rows", [])?;
        match setup {
            Some(setup) => {
                transaction.execute(
                    "UPDATE files SET vector_state=?1 WHERE vector_state IN (?2,?3)",
                    params![STATE_PENDING, STATE_DONE, STATE_FAILED],
                )?;
                schema::set_meta(
                    &transaction,
                    "vector_dimensions",
                    i64::from(setup.dimensions),
                )?;
                schema::set_meta(
                    &transaction,
                    "vector_pipeline",
                    digest_bytes(&setup.pipeline),
                )?;
                schema::set_meta(
                    &transaction,
                    "vector_generation",
                    i64::try_from(generation)?,
                )?;
                schema::set_meta(&transaction, "vector_rows", 0i64)?;
            }
            None => {
                transaction.execute("UPDATE files SET vector_state=?1", [STATE_NOT_SELECTED])?;
                for key in [
                    "vector_dimensions",
                    "vector_pipeline",
                    "vector_generation",
                    "vector_rows",
                ] {
                    transaction.execute("DELETE FROM meta WHERE key=?1", [key])?;
                }
            }
        }
        let next = sequence(&transaction)? + 1;
        schema::set_meta(&transaction, "sequence", i64::try_from(next)?)?;
        transaction.commit()?;
        self.remove_retired_vector_files();
        Ok(true)
    }

    /// Store one embedded batch. New vectors reach the file and its disk
    /// before the transaction that makes them reachable commits.
    pub fn commit_vectors(&mut self, batch: VectorBatch) -> Result<VectorCommit> {
        self.commit_vectors_with_resources(batch, &[])
    }

    pub(crate) fn commit_vectors_with_resources(
        &mut self,
        batch: VectorBatch,
        resources: &[ResourceChange],
    ) -> Result<VectorCommit> {
        let (setup, generation, committed) =
            vector_setup(&self.connection)?.context("vectors are not configured")?;
        ensure!(
            setup == batch.setup,
            "vector batch was planned for another embedding pipeline"
        );
        let mut fresh = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        {
            let mut known = self
                .connection
                .prepare_cached("SELECT 1 FROM vector_rows WHERE embedding_key=?1")?;
            for (key, vector) in &batch.vectors {
                vectors::validate(vector, setup.dimensions)?;
                if seen.insert(*key) && !known.exists([digest_bytes(key)])? {
                    fresh.push((*key, vector.as_slice()));
                }
            }
        }
        if !fresh.is_empty() {
            let mut file = VectorFile::open_append(&self.directory, generation, &setup)?;
            file.write_rows(committed, fresh.iter().map(|(_, vector)| *vector))?;
            file.sync()?;
        }
        let transaction = self.connection.transaction()?;
        let next = sequence(&transaction)? + 1;
        for (offset, (key, _)) in fresh.iter().enumerate() {
            transaction.execute(
                "INSERT INTO vector_rows(embedding_key,row) VALUES(?1,?2)",
                params![digest_bytes(key), i64::try_from(committed + offset as u64)?],
            )?;
        }
        let mut result = VectorCommit {
            sequence: next,
            rows_added: fresh.len() as u64,
            files_done: 0,
            files_stale: 0,
        };
        for file in &batch.files {
            validate_path(&file.path)?;
            ensure!(
                file.units.len() <= MAX_UNITS_PER_FILE,
                "file has too many semantic units"
            );
            let row: Option<(i64, Vec<u8>)> = transaction
                .query_row(
                    "SELECT id,source_hash FROM files WHERE path=?1 AND vector_state=?2",
                    params![file.path, STATE_PENDING],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((id, hash)) = row.filter(|(_, hash)| hash == digest_bytes(&file.source_hash))
            else {
                result.files_stale += 1;
                continue;
            };
            let _ = hash;
            transaction.execute("DELETE FROM units WHERE file_id=?1", [id])?;
            for unit in &file.units {
                ensure!(
                    unit.members.len() <= MAX_RECORD_BYTES,
                    "semantic unit members exceed byte limit"
                );
                transaction.execute(
                    "INSERT INTO units(file_id,ordinal,embedding_key,members) VALUES(?1,?2,?3,?4)",
                    params![
                        id,
                        i64::from(unit.ordinal),
                        digest_bytes(&unit.embedding_key),
                        unit.members
                    ],
                )?;
            }
            transaction.execute(
                "UPDATE files SET vector_state=?2, changed_sequence=?3 WHERE id=?1",
                params![
                    id,
                    if file.failed {
                        STATE_FAILED
                    } else {
                        STATE_DONE
                    },
                    i64::try_from(next)?
                ],
            )?;
            result.files_done += 1;
        }
        ensure!(
            resources.is_empty() || result.files_stale == 0,
            "semantic work changed before its vector publication"
        );
        for resource in resources {
            apply_resource(&transaction, resource)?;
        }
        schema::set_meta(
            &transaction,
            "vector_rows",
            i64::try_from(committed + fresh.len() as u64)?,
        )?;
        schema::set_meta(&transaction, "sequence", i64::try_from(next)?)?;
        transaction.commit()?;
        Ok(result)
    }

    /// The row a stored key occupies, if any.
    pub fn vector_row(&self, key: &Sha256Digest) -> Result<Option<u64>> {
        Ok(self
            .connection
            .query_row(
                "SELECT row FROM vector_rows WHERE embedding_key=?1",
                [digest_bytes(key)],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .map(u64::try_from)
            .transpose()?)
    }

    /// Run SQLite's own checks: the database and the lexical index.
    pub fn verify(&self) -> Result<()> {
        let result: String = self
            .connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        ensure!(
            result == "ok",
            "index database integrity check failed: {result}"
        );
        self.connection
            .execute(
                "INSERT INTO fts(fts, rank) VALUES('integrity-check', 0)",
                [],
            )
            .context("lexical index integrity check failed")?;
        Ok(())
    }

    /// Old generations stay on disk while a reader may still map them.
    fn remove_retired_vector_files(&self) {
        let Ok(current) = vector_setup(&self.connection) else {
            return;
        };
        let keep = current.map(|(_, generation, _)| generation);
        let Ok(entries) = std::fs::read_dir(&self.directory) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if let Some(generation) = vectors::generation_of(&name.to_string_lossy())
                && Some(generation) != keep
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

pub(super) fn vector_setup(connection: &Connection) -> Result<Option<(VectorSetup, u64, u64)>> {
    let Some(dimensions) = schema::meta_integer(connection, "vector_dimensions")? else {
        return Ok(None);
    };
    let pipeline = digest_from(
        schema::meta_blob(connection, "vector_pipeline")?.context("vector pipeline missing")?,
    )?;
    let generation = schema::meta_integer(connection, "vector_generation")?
        .context("vector generation missing")?;
    let rows =
        schema::meta_integer(connection, "vector_rows")?.context("vector row count missing")?;
    Ok(Some((
        VectorSetup {
            dimensions: u32::try_from(dimensions)?,
            pipeline,
        },
        u64::try_from(generation)?,
        u64::try_from(rows)?,
    )))
}

/// Delete a file's rows; return its old payload digest for reclamation.
fn remove_file(transaction: &Transaction<'_>, path: &str) -> Result<Option<Sha256Digest>> {
    let row: Option<(i64, Vec<u8>)> = transaction
        .query_row(
            "SELECT id,payload FROM files WHERE path=?1",
            [path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((id, payload)) = row else {
        return Ok(None);
    };
    transaction.execute(
        "DELETE FROM fts WHERE rowid IN (SELECT id FROM chunks WHERE file_id=?1)",
        [id],
    )?;
    for table in ["chunks", "selectors", "handles", "units"] {
        transaction.execute(&format!("DELETE FROM {table} WHERE file_id=?1"), [id])?;
    }
    transaction.execute("DELETE FROM files WHERE id=?1", [id])?;
    Ok(Some(digest_from(payload)?))
}

/// Insert one file; return its chunk count and whether its payload was new.
fn insert_file(
    transaction: &Transaction<'_>,
    entry: &FileEntry,
    sequence: u64,
) -> Result<(u64, bool)> {
    ensure!(
        entry.record.len() <= MAX_RECORD_BYTES && entry.metadata.len() <= MAX_METADATA_BYTES,
        "file record or metadata exceeds byte limit"
    );
    ensure!(
        entry.chunks.len() <= MAX_CHUNKS_PER_FILE
            && entry.selectors.len() <= MAX_SELECTORS_PER_FILE,
        "file has too many chunks or selectors"
    );
    let written = transaction.execute(
        "INSERT OR IGNORE INTO payloads(digest,raw_bytes,encoding,body) VALUES(?1,?2,?3,?4)",
        params![
            digest_bytes(&entry.payload.digest),
            i64::try_from(entry.payload.raw_bytes)?,
            entry.payload.encoding,
            entry.payload.body
        ],
    )? == 1;
    let stat = entry.stat.as_ref();
    transaction.execute(
        "INSERT INTO files(path,source_hash,language,record,metadata,payload,stat_size,stat_modified_ns,stat_file_id,vector_state,changed_sequence)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            entry.path,
            digest_bytes(&entry.source_hash),
            entry.language,
            entry.record,
            entry.metadata,
            digest_bytes(&entry.payload.digest),
            stat.map(|stat| i64::try_from(stat.size)).transpose()?,
            stat.map(|stat| stat.modified_ns),
            stat.map(|stat| stat.file_id.as_slice()),
            if entry.vector_pending { STATE_PENDING } else { STATE_NOT_SELECTED },
            i64::try_from(sequence)?
        ],
    )?;
    let id = transaction.last_insert_rowid();
    let mut chunk = transaction.prepare_cached(
        "INSERT INTO chunks(file_id,ordinal,stable_id,citation) VALUES(?1,?2,?3,?4)",
    )?;
    let mut lexical = transaction.prepare_cached("INSERT INTO fts(rowid,terms) VALUES(?1,?2)")?;
    for (ordinal, entry) in entry.chunks.iter().enumerate() {
        chunk.execute(params![
            id,
            i64::try_from(ordinal)?,
            digest_bytes(&entry.stable_id),
            entry.citation
        ])?;
        lexical.execute(params![transaction.last_insert_rowid(), entry.terms])?;
    }
    let mut selector = transaction
        .prepare_cached("INSERT OR IGNORE INTO selectors(kind,value,file_id) VALUES(?1,?2,?3)")?;
    for item in &entry.selectors {
        selector.execute(params![item.kind as u8, item.value, id])?;
    }
    let mut handle =
        transaction.prepare_cached("INSERT INTO handles(kind,value,file_id) VALUES(?1,?2,?3)")?;
    for item in &entry.handles {
        handle
            .execute(params![item.kind as u8, item.value, id])
            .context("index handle is already bound to another file")?;
    }
    Ok((entry.chunks.len() as u64, written))
}
