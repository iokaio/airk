// SPDX-License-Identifier: Apache-2.0
//! A reader is one SQLite read transaction: a stable snapshot of one
//! publication, unaffected by later commits until it is dropped.
use super::{
    DATABASE_FILE, FileStat, Handle, Selector, decode_payload, match_all, schema, validate_path,
    vectors::VectorMatrix,
    writer::{VectorSetup, vector_setup},
};
use airk_core::identity::Sha256Digest;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};

pub const MAX_PAGE: usize = 4096;
pub const MAX_LEXICAL_HITS: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorState {
    NotSelected,
    Pending,
    Done,
    Failed,
}

impl VectorState {
    fn from_column(value: i64) -> Result<Self> {
        Ok(match value {
            0 => Self::NotSelected,
            1 => Self::Pending,
            2 => Self::Done,
            3 => Self::Failed,
            _ => anyhow::bail!("unknown stored vector state"),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRow {
    pub path: String,
    pub source_hash: Sha256Digest,
    pub language: String,
    pub record: Vec<u8>,
    pub metadata: Vec<u8>,
    pub payload: Sha256Digest,
    pub stat: Option<FileStat>,
    pub vector_state: VectorState,
    pub changed_sequence: u64,
}

/// Which files a path lookup lists.
#[derive(Clone, Debug)]
pub enum PathLookup {
    Files,
    Language(String),
    Selector(Selector),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathPage {
    pub paths: Vec<String>,
    /// The last path returned, when more may follow.
    pub next_after: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LexicalQuery {
    /// Terms in the stored form; every one must occur in a hit.
    pub terms: Vec<String>,
    pub path_prefix: Option<String>,
    /// Only these paths, when given.
    pub paths: Option<Vec<String>>,
    pub limit: usize,
    /// BM25 order when true; stable-identity order when false.
    pub ranked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LexicalHit {
    pub path: String,
    pub ordinal: u32,
    pub stable_id: Sha256Digest,
    pub citation: Vec<u8>,
    /// SQLite's BM25 value: lower is a better match. Zero when unranked.
    pub rank: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitRow {
    pub path: String,
    pub source_hash: Sha256Digest,
    pub payload: Sha256Digest,
    pub ordinal: u32,
    pub embedding_key: Sha256Digest,
    pub members: Vec<u8>,
}

pub struct IndexReader {
    directory: PathBuf,
    connection: Connection,
    sequence: u64,
}

fn digest(bytes: Vec<u8>) -> Result<Sha256Digest> {
    Ok(Sha256Digest::from_bytes(bytes.try_into().map_err(
        |_| anyhow::anyhow!("stored digest has the wrong length"),
    )?))
}

fn after_key(after: Option<&str>) -> &str {
    after.unwrap_or("")
}

impl IndexReader {
    pub fn open(directory: &Path) -> Result<Self> {
        let connection = schema::open_reader(&directory.join(DATABASE_FILE))?;
        // The first read inside BEGIN fixes the snapshot for this reader.
        connection.execute_batch("BEGIN DEFERRED")?;
        let sequence = u64::try_from(
            schema::meta_integer(&connection, "sequence")?.context("index sequence missing")?,
        )?;
        Ok(Self {
            directory: directory.to_path_buf(),
            connection,
            sequence,
        })
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// A caller meta value written through `Batch::meta`.
    pub fn meta(&self, key: &str) -> Result<Option<Vec<u8>>> {
        schema::meta_blob(&self.connection, key)
    }

    pub fn resource(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .connection
            .query_row("SELECT body FROM resources WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn file(&self, path: &str) -> Result<Option<FileRow>> {
        validate_path(path)?;
        self.connection
            .query_row(
                "SELECT path,source_hash,language,record,metadata,payload,stat_size,stat_modified_ns,stat_file_id,vector_state,changed_sequence FROM files WHERE path=?1",
                [path],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, Vec<u8>>(4)?,
                        row.get::<_, Vec<u8>>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, Option<i64>>(7)?,
                        row.get::<_, Option<Vec<u8>>>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, i64>(10)?,
                    ))
                },
            )
            .optional()?
            .map(|(path, hash, language, record, metadata, payload, size, modified, id, state, changed)| {
                Ok(FileRow {
                    path,
                    source_hash: digest(hash)?,
                    language,
                    record,
                    metadata,
                    payload: digest(payload)?,
                    stat: match (size, modified, id) {
                        (Some(size), Some(modified_ns), Some(file_id)) => Some(FileStat {
                            size: u64::try_from(size)?,
                            modified_ns,
                            file_id,
                        }),
                        _ => None,
                    },
                    vector_state: VectorState::from_column(state)?,
                    changed_sequence: u64::try_from(changed)?,
                })
            })
            .transpose()
    }

    /// The raw bytes of a stored payload, checked against their digest.
    pub fn payload(&self, digest: &Sha256Digest) -> Result<Vec<u8>> {
        let (raw_bytes, encoding, body): (i64, u8, Vec<u8>) = self
            .connection
            .query_row(
                "SELECT raw_bytes,encoding,body FROM payloads WHERE digest=?1",
                [digest.as_bytes().as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?
            .context("index payload missing")?;
        let raw = decode_payload(encoding, u64::try_from(raw_bytes)?, &body)?;
        use sha2::{Digest, Sha256};
        ensure!(
            Sha256::digest(&raw).as_slice() == digest.as_bytes(),
            "index payload does not match its digest; rebuild the index"
        );
        Ok(raw)
    }

    pub fn file_count(&self) -> Result<u64> {
        Ok(u64::try_from(self.connection.query_row(
            "SELECT count(*) FROM files",
            [],
            |row| row.get::<_, i64>(0),
        )?)?)
    }

    pub fn vector_state_count(&self, state: VectorState) -> Result<u64> {
        let code = match state {
            VectorState::NotSelected => 0,
            VectorState::Pending => 1,
            VectorState::Done => 2,
            VectorState::Failed => 3,
        };
        Ok(u64::try_from(self.connection.query_row(
            "SELECT count(*) FROM files WHERE vector_state=?1",
            [code],
            |row| row.get::<_, i64>(0),
        )?)?)
    }

    /// Paths in byte order after `after`, under `prefix`.
    pub fn lookup(
        &self,
        lookup: &PathLookup,
        prefix: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<PathPage> {
        ensure!(
            (1..=MAX_PAGE).contains(&limit),
            "path page limit out of range"
        );
        // A page starting at the prefix itself must include a path equal to it.
        let include_prefix = !prefix.is_empty() && after.is_none_or(|after| after < prefix);
        let after = after_key(after).max(prefix);
        let fetch = i64::try_from(limit + 1)?;
        let mut paths = match lookup {
            PathLookup::Files => self.collect_paths(
                "SELECT path FROM files WHERE path>?1 AND substr(path,1,length(?2))=?2 ORDER BY path LIMIT ?3",
                params![after, prefix, fetch],
            )?,
            PathLookup::Language(language) => self.collect_paths(
                "SELECT path FROM files WHERE language=?4 AND path>?1 AND substr(path,1,length(?2))=?2 ORDER BY path LIMIT ?3",
                params![after, prefix, fetch, language],
            )?,
            PathLookup::Selector(selector) => self.collect_paths(
                "SELECT f.path FROM selectors s JOIN files f ON f.id=s.file_id WHERE s.kind=?4 AND s.value=?5 AND f.path>?1 AND substr(f.path,1,length(?2))=?2 ORDER BY f.path LIMIT ?3",
                params![after, prefix, fetch, selector.kind as u8, selector.value],
            )?,
        };
        // `path > after` skips a path equal to the prefix; add it back once.
        if include_prefix && let Some(exact) = self.exact_match(lookup, prefix)? {
            paths.insert(0, exact);
            paths.truncate(limit + 1);
        }
        let next_after = (paths.len() > limit).then(|| {
            paths.truncate(limit);
            paths.last().cloned().expect("limit is positive")
        });
        Ok(PathPage { paths, next_after })
    }

    fn exact_match(&self, lookup: &PathLookup, path: &str) -> Result<Option<String>> {
        let found = match lookup {
            PathLookup::Files => self
                .connection
                .query_row("SELECT path FROM files WHERE path=?1", [path], |row| row.get(0))
                .optional()?,
            PathLookup::Language(language) => self
                .connection
                .query_row(
                    "SELECT path FROM files WHERE path=?1 AND language=?2",
                    params![path, language],
                    |row| row.get(0),
                )
                .optional()?,
            PathLookup::Selector(selector) => self
                .connection
                .query_row(
                    "SELECT f.path FROM selectors s JOIN files f ON f.id=s.file_id WHERE f.path=?1 AND s.kind=?2 AND s.value=?3",
                    params![path, selector.kind as u8, selector.value],
                    |row| row.get(0),
                )
                .optional()?,
        };
        Ok(found)
    }

    fn collect_paths(&self, sql: &str, parameters: impl rusqlite::Params) -> Result<Vec<String>> {
        let mut statement = self.connection.prepare_cached(sql)?;
        let rows = statement.query_map(parameters, |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Citation units that contain every term. Returns the hits and whether
    /// more matched than `limit`.
    pub fn lexical(&self, query: &LexicalQuery) -> Result<(Vec<LexicalHit>, bool)> {
        ensure!(
            (1..=MAX_LEXICAL_HITS).contains(&query.limit),
            "lexical limit out of range"
        );
        let expression = match_all(&query.terms)?;
        let paths = query
            .paths
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let order = if query.ranked {
            "bm25(fts), c.stable_id"
        } else {
            "c.stable_id"
        };
        let rank = if query.ranked { "bm25(fts)" } else { "0.0" };
        let sql = format!(
            "SELECT f.path, c.ordinal, c.stable_id, c.citation, {rank} FROM fts JOIN chunks c ON c.id=fts.rowid JOIN files f ON f.id=c.file_id \
             WHERE fts MATCH ?1 AND (?2 IS NULL OR substr(f.path,1,length(?2))=?2) \
             AND (?3 IS NULL OR f.path IN (SELECT value FROM json_each(?3))) ORDER BY {order} LIMIT ?4"
        );
        let mut statement = self.connection.prepare_cached(&sql)?;
        let mut rows = statement.query(params![
            expression,
            query.path_prefix,
            paths,
            i64::try_from(query.limit + 1)?
        ])?;
        let mut hits = Vec::new();
        let mut more = false;
        while let Some(row) = rows.next()? {
            if hits.len() == query.limit {
                more = true;
                break;
            }
            hits.push(LexicalHit {
                path: row.get(0)?,
                ordinal: u32::try_from(row.get::<_, i64>(1)?)?,
                stable_id: digest(row.get(2)?)?,
                citation: row.get(3)?,
                rank: row.get(4)?,
            });
        }
        Ok((hits, more))
    }

    /// The citation stored for one chunk.
    pub fn chunk(&self, path: &str, ordinal: u32) -> Result<Option<(Sha256Digest, Vec<u8>)>> {
        self.connection
            .query_row(
                "SELECT c.stable_id,c.citation FROM chunks c JOIN files f ON f.id=c.file_id WHERE f.path=?1 AND c.ordinal=?2",
                params![path, i64::from(ordinal)],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?
            .map(|(id, citation)| Ok((digest(id)?, citation)))
            .transpose()
    }

    /// The file a stable handle currently names.
    pub fn handle(&self, handle: &Handle) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row(
                "SELECT f.path FROM handles h JOIN files f ON f.id=h.file_id WHERE h.kind=?1 AND h.value=?2",
                params![handle.kind as u8, handle.value],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Paths waiting for vectors, in byte order after `after`.
    pub fn pending_vectors(&self, after: Option<&str>, limit: usize) -> Result<Vec<String>> {
        ensure!((1..=MAX_PAGE).contains(&limit), "page limit out of range");
        self.collect_paths(
            "SELECT path FROM files WHERE vector_state=1 AND path>?1 ORDER BY path LIMIT ?2",
            params![after_key(after), i64::try_from(limit)?],
        )
    }

    fn unit_rows(&self, sql: &str, parameters: impl rusqlite::Params) -> Result<Vec<UnitRow>> {
        let mut statement = self.connection.prepare_cached(sql)?;
        let mut rows = statement.query(parameters)?;
        let mut units = Vec::new();
        while let Some(row) = rows.next()? {
            units.push(UnitRow {
                path: row.get(0)?,
                source_hash: digest(row.get(1)?)?,
                payload: digest(row.get(2)?)?,
                ordinal: u32::try_from(row.get::<_, i64>(3)?)?,
                embedding_key: digest(row.get(4)?)?,
                members: row.get(5)?,
            });
        }
        Ok(units)
    }

    pub fn units(&self, path: &str) -> Result<Vec<UnitRow>> {
        self.unit_rows(
            "SELECT f.path,f.source_hash,f.payload,u.ordinal,u.embedding_key,u.members FROM units u JOIN files f ON f.id=u.file_id WHERE f.path=?1 ORDER BY u.ordinal",
            [path],
        )
    }

    /// Every unit whose vector is stored in `row`, ordered by path and ordinal.
    pub fn units_for_row(&self, row: u64) -> Result<Vec<UnitRow>> {
        self.unit_rows(
            "SELECT f.path,f.source_hash,f.payload,u.ordinal,u.embedding_key,u.members FROM vector_rows v JOIN units u ON u.embedding_key=v.embedding_key JOIN files f ON f.id=u.file_id WHERE v.row=?1 ORDER BY f.path,u.ordinal",
            [i64::try_from(row)?],
        )
    }

    /// The rows holding vectors of units in these files, for a filtered search.
    pub fn rows_for_paths(&self, paths: &[String]) -> Result<Vec<u64>> {
        let paths = serde_json::to_string(paths)?;
        let mut statement = self.connection.prepare_cached(
            "SELECT DISTINCT v.row FROM files f JOIN units u ON u.file_id=f.id JOIN vector_rows v ON v.embedding_key=u.embedding_key WHERE f.path IN (SELECT value FROM json_each(?1)) ORDER BY v.row",
        )?;
        let rows = statement.query_map([paths], |row| row.get::<_, i64>(0))?;
        rows.map(|row| Ok(u64::try_from(row?)?)).collect()
    }

    /// The embedding pipeline, generation and committed row count.
    pub fn vector_setup(&self) -> Result<Option<(VectorSetup, u64, u64)>> {
        vector_setup(&self.connection)
    }

    /// The committed rows of this snapshot's vector generation.
    pub fn load_vectors(&self) -> Result<Option<VectorMatrix>> {
        let Some((setup, generation, rows)) = self.vector_setup()? else {
            return Ok(None);
        };
        VectorMatrix::load(&self.directory, generation, &setup, rows).map(Some)
    }

    /// Change records published after `sequence`, oldest first.
    pub fn changes_since(&self, sequence: u64, limit: usize) -> Result<Vec<(u64, Vec<u8>)>> {
        ensure!((1..=MAX_PAGE).contains(&limit), "page limit out of range");
        let mut statement = self.connection.prepare_cached(
            "SELECT sequence,body FROM changes WHERE sequence>?1 ORDER BY sequence,ordinal LIMIT ?2",
        )?;
        let rows = statement.query_map(
            params![i64::try_from(sequence)?, i64::try_from(limit)?],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )?;
        rows.map(|row| {
            let (sequence, body) = row?;
            Ok((u64::try_from(sequence)?, body))
        })
        .collect()
    }

    /// The oldest sequence whose change record is still kept.
    pub fn oldest_change(&self) -> Result<Option<u64>> {
        Ok(self
            .connection
            .query_row("SELECT min(sequence) FROM changes", [], |row| {
                row.get::<_, Option<i64>>(0)
            })?
            .map(u64::try_from)
            .transpose()?)
    }
}
