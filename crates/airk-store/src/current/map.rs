// SPDX-License-Identifier: Apache-2.0
//! Bounded authenticated key/bucket map for the current catalog and mappings.
//! Publication calls `put` only inside the current owner's SQL transaction.
use super::CurrentScope;
use airk_core::identity::{Sha256Digest, framed_digest};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::BTreeMap;

const HEIGHT: u8 = 16;
pub const MAX_BUCKET_BYTES: usize = 512 * 1024;
pub const MAX_VALUE_BYTES: usize = 256 * 1024;
pub const MAX_BUCKET_ENTRIES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapDomain {
    Artifact,
    Vector,
    Occurrence,
    FactName,
    EdgeSource,
    EdgeTarget,
}

#[derive(Clone, Debug)]
pub(crate) struct MapChange {
    key: Sha256Digest,
    value: Option<Vec<u8>>,
}

impl MapChange {
    pub(crate) fn new(
        scope: &CurrentScope,
        domain: MapDomain,
        identity: &[u8],
        value: Option<Vec<u8>>,
    ) -> Result<Self> {
        if let Some(value) = &value {
            ensure!(
                value.len() <= MAX_VALUE_BYTES,
                "map change value limit exceeded"
            );
        }
        Ok(Self {
            key: key(scope, domain, identity)?,
            value,
        })
    }

    pub(crate) fn key(&self) -> Sha256Digest {
        self.key
    }
    pub(crate) fn value(&self) -> Option<&[u8]> {
        self.value.as_deref()
    }
}
impl MapDomain {
    fn label(self) -> &'static [u8] {
        match self {
            Self::Artifact => b"artifact",
            Self::Vector => b"vector",
            Self::Occurrence => b"occurrence",
            Self::FactName => b"fact-name",
            Self::EdgeSource => b"edge-source",
            Self::EdgeTarget => b"edge-target",
        }
    }
}

pub fn key(scope: &CurrentScope, domain: MapDomain, identity: &[u8]) -> Result<Sha256Digest> {
    scope.validate()?;
    ensure!(
        !identity.is_empty() && identity.len() <= 8192,
        "invalid current map identity"
    );
    Ok(framed_digest(
        b"current-keyed-map-key/1",
        &[scope_digest(scope).as_bytes(), domain.label(), identity],
    ))
}

fn scope_digest(scope: &CurrentScope) -> Sha256Digest {
    framed_digest(
        b"current-keyed-map-scope/1",
        &[
            scope.root.as_bytes(),
            scope.collection.as_bytes(),
            scope.incarnation.as_bytes(),
        ],
    )
}
fn prefix(key: Sha256Digest) -> u16 {
    u16::from_be_bytes(key.as_bytes()[..2].try_into().expect("two digest bytes"))
}
fn empty(scope: &CurrentScope, height: u8) -> Sha256Digest {
    framed_digest(
        b"current-keyed-map-empty/1",
        &[scope_digest(scope).as_bytes(), &[height]],
    )
}
pub fn empty_root(scope: &CurrentScope) -> Result<Sha256Digest> {
    scope.validate()?;
    Ok(empty(scope, HEIGHT))
}
fn leaf(scope: &CurrentScope, prefix: u16, bytes: &[u8]) -> Sha256Digest {
    framed_digest(
        b"current-keyed-map-bucket/1",
        &[scope_digest(scope).as_bytes(), &prefix.to_be_bytes(), bytes],
    )
}
fn branch(
    scope: &CurrentScope,
    height: u8,
    index: u16,
    left: Sha256Digest,
    right: Sha256Digest,
) -> Result<Sha256Digest> {
    ensure!(
        (1..=HEIGHT).contains(&height) && u32::from(index) < (1u32 << (HEIGHT - height)),
        "invalid map branch coordinate"
    );
    if left == empty(scope, height - 1) && right == empty(scope, height - 1) {
        Ok(empty(scope, height))
    } else {
        Ok(framed_digest(
            b"current-keyed-map-branch/1",
            &[
                scope_digest(scope).as_bytes(),
                &[height],
                &index.to_be_bytes(),
                left.as_bytes(),
                right.as_bytes(),
            ],
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    key: Sha256Digest,
    value: Vec<u8>,
}
fn decode(prefix: u16, bytes: &[u8]) -> Result<Vec<Entry>> {
    ensure!(
        bytes.len() <= MAX_BUCKET_BYTES && bytes.len() >= 2,
        "invalid map bucket size"
    );
    let count = usize::from(u16::from_be_bytes(bytes[..2].try_into()?));
    ensure!(
        (1..=MAX_BUCKET_ENTRIES).contains(&count),
        "invalid map bucket entry count"
    );
    let mut entries = Vec::with_capacity(count);
    let mut offset = 2;
    for _ in 0..count {
        ensure!(
            bytes.len().saturating_sub(offset) >= 36,
            "truncated map bucket entry"
        );
        let key = Sha256Digest::from_bytes(bytes[offset..offset + 32].try_into()?);
        offset += 32;
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into()?) as usize;
        offset += 4;
        ensure!(
            length <= MAX_VALUE_BYTES && bytes.len().saturating_sub(offset) >= length,
            "oversized or truncated map value"
        );
        ensure!(
            self::prefix(key) == prefix
                && entries.last().is_none_or(|entry: &Entry| entry.key < key),
            "unordered or foreign map bucket key"
        );
        entries.push(Entry {
            key,
            value: bytes[offset..offset + length].to_vec(),
        });
        offset += length;
    }
    ensure!(
        offset == bytes.len(),
        "noncanonical map bucket trailing bytes"
    );
    Ok(entries)
}
fn encode(prefix: u16, entries: &[Entry]) -> Result<Vec<u8>> {
    ensure!(
        (1..=MAX_BUCKET_ENTRIES).contains(&entries.len()),
        "map bucket entry limit exceeded"
    );
    let mut bytes = Vec::with_capacity(
        2 + entries
            .iter()
            .map(|entry| 36 + entry.value.len())
            .sum::<usize>(),
    );
    bytes.extend_from_slice(&u16::try_from(entries.len())?.to_be_bytes());
    for entry in entries {
        ensure!(
            entry.value.len() <= MAX_VALUE_BYTES,
            "map value limit exceeded"
        );
        bytes.extend_from_slice(entry.key.as_bytes());
        bytes.extend_from_slice(&u32::try_from(entry.value.len())?.to_be_bytes());
        bytes.extend_from_slice(&entry.value);
    }
    ensure!(
        bytes.len() <= MAX_BUCKET_BYTES,
        "map bucket byte limit exceeded"
    );
    ensure!(
        decode(prefix, &bytes)? == entries,
        "noncanonical map bucket"
    );
    Ok(bytes)
}

pub(crate) fn create(connection: &Connection) -> Result<()> {
    connection.execute_batch("CREATE TABLE current_map_buckets(prefix INTEGER PRIMARY KEY CHECK(prefix BETWEEN 0 AND 65535),content BLOB NOT NULL CHECK(length(content) BETWEEN 2 AND 524288));
        CREATE TABLE current_map_nodes(height INTEGER NOT NULL CHECK(height BETWEEN 0 AND 15),idx INTEGER NOT NULL CHECK(idx BETWEEN 0 AND 65535),digest BLOB NOT NULL CHECK(length(digest)=32),PRIMARY KEY(height,idx)) WITHOUT ROWID;")?;
    Ok(())
}
fn bucket(
    connection: &Connection,
    scope: &CurrentScope,
    prefix: u16,
) -> Result<(Vec<Entry>, Sha256Digest)> {
    let row: Option<Option<Vec<u8>>> = connection.query_row(
        "SELECT CASE WHEN typeof(content)='blob' AND length(content)<=524288 THEN content END FROM current_map_buckets WHERE prefix=?1",
        [i64::from(prefix)],
        |row| row.get(0),
    ).optional()?;
    let bytes = match row {
        Some(None) => anyhow::bail!("invalid current map bucket bytes"),
        Some(Some(bytes)) => Some(bytes),
        None => None,
    };
    match bytes {
        Some(bytes) => {
            let entries = decode(prefix, &bytes)?;
            Ok((entries, leaf(scope, prefix, &bytes)))
        }
        None => Ok((Vec::new(), empty(scope, 0))),
    }
}
fn node(
    connection: &Connection,
    scope: &CurrentScope,
    height: u8,
    index: u16,
) -> Result<Sha256Digest> {
    ensure!(
        height <= HEIGHT && u32::from(index) < (1u32 << (HEIGHT - height)),
        "invalid map node coordinate"
    );
    let row: Option<Option<Vec<u8>>> = connection.query_row(
        "SELECT CASE WHEN typeof(digest)='blob' AND length(digest)=32 THEN digest END FROM current_map_nodes WHERE height=?1 AND idx=?2",
        params![i64::from(height), i64::from(index)],
        |row| row.get(0),
    ).optional()?;
    let value = match row {
        Some(None) => anyhow::bail!("invalid current map node digest"),
        Some(Some(bytes)) => Some(bytes),
        None => None,
    };
    Ok(value
        .map(|bytes| Sha256Digest::from_bytes(bytes.try_into().expect("length checked")))
        .unwrap_or_else(|| empty(scope, height)))
}
fn proof(
    connection: &Connection,
    scope: &CurrentScope,
    root: Sha256Digest,
    key: Sha256Digest,
) -> Result<Vec<Entry>> {
    scope.validate()?;
    let prefix = prefix(key);
    let (entries, mut digest) = bucket(connection, scope, prefix)?;
    let mut index = prefix;
    for height in 0..HEIGHT {
        let sibling = node(connection, scope, height, index ^ 1)?;
        let (left, right) = if index & 1 == 0 {
            (digest, sibling)
        } else {
            (sibling, digest)
        };
        index >>= 1;
        digest = branch(scope, height + 1, index, left, right)?;
    }
    ensure!(
        digest == root,
        "current map membership or absence proof mismatch"
    );
    Ok(entries)
}
pub(crate) fn get(
    connection: &Connection,
    scope: &CurrentScope,
    root: Sha256Digest,
    key: Sha256Digest,
) -> Result<Option<Vec<u8>>> {
    let entries = proof(connection, scope, root, key)?;
    Ok(entries
        .binary_search_by_key(&key, |entry| entry.key)
        .ok()
        .map(|position| entries[position].value.clone()))
}
fn put_node(
    connection: &Connection,
    scope: &CurrentScope,
    height: u8,
    index: u16,
    digest: Sha256Digest,
) -> Result<()> {
    if height == HEIGHT {
        // The root has one owner: CurrentHead. Keeping a duplicate SQL row
        // would create an unused physical value with no proof-checking role.
        return Ok(());
    }
    let values = params![
        i64::from(height),
        i64::from(index),
        digest.as_bytes().as_slice()
    ];
    if digest == empty(scope, height) {
        connection.execute(
            "DELETE FROM current_map_nodes WHERE height=?1 AND idx=?2",
            params![i64::from(height), i64::from(index)],
        )?;
    } else if connection.execute(
        "UPDATE OR ROLLBACK current_map_nodes SET digest=?3 WHERE height=?1 AND idx=?2",
        values,
    )? == 0
    {
        connection.execute(
            "INSERT OR ROLLBACK INTO current_map_nodes(height,idx,digest) VALUES(?1,?2,?3)",
            values,
        )?;
    }
    Ok(())
}
/// The caller owns an exclusive current gate and a SQL transaction covering the
/// eventual head change. This function never commits a partially changed root.
pub(crate) fn put(
    connection: &Connection,
    scope: &CurrentScope,
    root: Sha256Digest,
    key: Sha256Digest,
    value: Option<&[u8]>,
) -> Result<Sha256Digest> {
    if let Some(value) = value {
        ensure!(value.len() <= MAX_VALUE_BYTES, "map value limit exceeded");
    }
    let prefix = prefix(key);
    let mut entries = proof(connection, scope, root, key)?;
    match entries.binary_search_by_key(&key, |entry| entry.key) {
        Ok(position) if let Some(value) = value => entries[position].value = value.to_vec(),
        Ok(position) => {
            entries.remove(position);
        }
        Err(position) if let Some(value) = value => entries.insert(
            position,
            Entry {
                key,
                value: value.to_vec(),
            },
        ),
        Err(_) => return Ok(root),
    }
    let mut digest = if entries.is_empty() {
        connection.execute(
            "DELETE FROM current_map_buckets WHERE prefix=?1",
            [i64::from(prefix)],
        )?;
        empty(scope, 0)
    } else {
        let bytes = encode(prefix, &entries)?;
        if connection.execute(
            "UPDATE OR ROLLBACK current_map_buckets SET content=?2 WHERE prefix=?1",
            params![i64::from(prefix), &bytes],
        )? == 0
        {
            connection.execute(
                "INSERT OR ROLLBACK INTO current_map_buckets(prefix,content) VALUES(?1,?2)",
                params![i64::from(prefix), &bytes],
            )?;
        }
        leaf(scope, prefix, &bytes)
    };
    let mut index = prefix;
    put_node(connection, scope, 0, index, digest)?;
    for height in 0..HEIGHT {
        let sibling = node(connection, scope, height, index ^ 1)?;
        let (left, right) = if index & 1 == 0 {
            (digest, sibling)
        } else {
            (sibling, digest)
        };
        index >>= 1;
        digest = branch(scope, height + 1, index, left, right)?;
        put_node(connection, scope, height + 1, index, digest)?;
    }
    Ok(digest)
}

/// Predicts the exact post-commit map root while the committed SQL tree stays
/// readable. The final SQL transaction must replay the same ordered mutations
/// with `put` and require equality with `root()` before publishing the head.
pub(crate) struct MapPlan<'a> {
    connection: &'a Connection,
    scope: &'a CurrentScope,
    root: Sha256Digest,
    buckets: BTreeMap<u16, Vec<Entry>>,
    nodes: BTreeMap<(u8, u16), Sha256Digest>,
    bucket_bytes: usize,
    operations: usize,
    poisoned: bool,
}

impl<'a> MapPlan<'a> {
    pub fn new(
        connection: &'a Connection,
        scope: &'a CurrentScope,
        root: Sha256Digest,
    ) -> Result<Self> {
        scope.validate()?;
        Ok(Self {
            connection,
            scope,
            root,
            buckets: BTreeMap::new(),
            nodes: BTreeMap::new(),
            bucket_bytes: 0,
            operations: 0,
            poisoned: false,
        })
    }

    pub fn root(&self) -> Result<Sha256Digest> {
        ensure!(!self.poisoned, "current map plan is poisoned");
        Ok(self.root)
    }

    /// Read through the planned afterimage, proving the bucket against the
    /// planned root. Used to derive removals of old per-artifact identities
    /// before their replacement is staged in the same transaction.
    pub fn get(&self, key: Sha256Digest) -> Result<Option<Vec<u8>>> {
        ensure!(!self.poisoned, "current map plan is poisoned");
        let entries = self.proven(key)?;
        Ok(entries
            .binary_search_by_key(&key, |entry| entry.key)
            .ok()
            .map(|position| entries[position].value.clone()))
    }

    fn node(&self, height: u8, index: u16) -> Result<Sha256Digest> {
        match self.nodes.get(&(height, index)) {
            Some(digest) => Ok(*digest),
            None => node(self.connection, self.scope, height, index),
        }
    }

    fn bucket(&self, prefix: u16) -> Result<(Vec<Entry>, Sha256Digest)> {
        match self.buckets.get(&prefix) {
            Some(entries) if entries.is_empty() => Ok((Vec::new(), empty(self.scope, 0))),
            Some(entries) => {
                let bytes = encode(prefix, entries)?;
                Ok((entries.clone(), leaf(self.scope, prefix, &bytes)))
            }
            None => bucket(self.connection, self.scope, prefix),
        }
    }

    fn proven(&self, key: Sha256Digest) -> Result<Vec<Entry>> {
        let prefix = prefix(key);
        let (entries, mut digest) = self.bucket(prefix)?;
        let mut index = prefix;
        for height in 0..HEIGHT {
            let sibling = self.node(height, index ^ 1)?;
            let (left, right) = if index & 1 == 0 {
                (digest, sibling)
            } else {
                (sibling, digest)
            };
            index >>= 1;
            digest = branch(self.scope, height + 1, index, left, right)?;
        }
        ensure!(digest == self.root, "planned current map proof mismatch");
        Ok(entries)
    }

    pub fn put(&mut self, key: Sha256Digest, value: Option<&[u8]>) -> Result<()> {
        ensure!(!self.poisoned, "current map plan is poisoned");
        let result = self.put_inner(key, value);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }

    fn put_inner(&mut self, key: Sha256Digest, value: Option<&[u8]>) -> Result<()> {
        ensure!(
            self.operations < 65_536,
            "current map plan operation bound exceeded"
        );
        if let Some(value) = value {
            ensure!(
                value.len() <= MAX_VALUE_BYTES,
                "map plan value limit exceeded"
            );
        }
        let prefix = prefix(key);
        let mut entries = self.proven(key)?;
        match entries.binary_search_by_key(&key, |entry| entry.key) {
            Ok(position) if let Some(value) = value => entries[position].value = value.to_vec(),
            Ok(position) => {
                entries.remove(position);
            }
            Err(position) if let Some(value) = value => entries.insert(
                position,
                Entry {
                    key,
                    value: value.to_vec(),
                },
            ),
            Err(_) => return Ok(()),
        }
        let size = if entries.is_empty() {
            0
        } else {
            encode(prefix, &entries)?.len()
        };
        let old_size = match self.buckets.get(&prefix) {
            Some(old) if !old.is_empty() => encode(prefix, old)?.len(),
            _ => 0,
        };
        let next_bytes = self
            .bucket_bytes
            .checked_sub(old_size)
            .and_then(|bytes| bytes.checked_add(size))
            .context("current map plan byte overflow")?;
        ensure!(
            next_bytes <= 64 * 1024 * 1024,
            "current map plan byte bound exceeded"
        );
        let mut digest = if entries.is_empty() {
            empty(self.scope, 0)
        } else {
            leaf(self.scope, prefix, &encode(prefix, &entries)?)
        };
        self.buckets.insert(prefix, entries);
        self.nodes.insert((0, prefix), digest);
        let mut index = prefix;
        for height in 0..HEIGHT {
            let sibling = self.node(height, index ^ 1)?;
            let (left, right) = if index & 1 == 0 {
                (digest, sibling)
            } else {
                (sibling, digest)
            };
            index >>= 1;
            digest = branch(self.scope, height + 1, index, left, right)?;
            if height + 1 < HEIGHT {
                self.nodes.insert((height + 1, index), digest);
            }
        }
        ensure!(
            self.nodes.len() <= 1_048_576,
            "current map plan node bound exceeded"
        );
        self.bucket_bytes = next_bytes;
        self.root = digest;
        self.operations += 1;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MapVisitLimits {
    pub nodes: usize,
    pub entries: usize,
    pub bytes: usize,
}
impl Default for MapVisitLimits {
    fn default() -> Self {
        Self {
            nodes: 1_000_000,
            entries: 100_000,
            bytes: 64 * 1024 * 1024,
        }
    }
}
impl MapVisitLimits {
    fn validate(self) -> Result<()> {
        ensure!(
            (1..=2_000_000).contains(&self.nodes)
                && (1..=1_000_000).contains(&self.entries)
                && (1..=256 * 1024 * 1024).contains(&self.bytes),
            "invalid current map visit limits"
        );
        Ok(())
    }
}
/// Traverses the authenticated tree, so omission of an entire occupied bucket
/// cannot silently shorten an enumeration. The caller owns the SQL read lease.
pub(crate) fn visit_all(
    connection: &Connection,
    scope: &CurrentScope,
    root: Sha256Digest,
    limits: MapVisitLimits,
    visitor: &mut dyn FnMut(Sha256Digest, &[u8]) -> Result<()>,
) -> Result<()> {
    scope.validate()?;
    limits.validate()?;
    let mut stack = vec![(HEIGHT, 0u16, root)];
    let mut nodes = 0usize;
    let mut entries = 0usize;
    let mut bytes = 0usize;
    while let Some((height, index, expected)) = stack.pop() {
        nodes += 1;
        ensure!(
            nodes <= limits.nodes,
            "current map visit node budget exceeded"
        );
        if expected == empty(scope, height) {
            continue;
        }
        if height == 0 {
            let (contents, digest) = bucket(connection, scope, index)?;
            ensure!(
                digest == expected && !contents.is_empty(),
                "current map bucket proof mismatch"
            );
            for entry in contents {
                entries += 1;
                bytes = bytes
                    .checked_add(entry.value.len())
                    .context("map visit byte overflow")?;
                ensure!(
                    entries <= limits.entries && bytes <= limits.bytes,
                    "current map visit entry/byte budget exceeded"
                );
                visitor(entry.key, &entry.value)?;
            }
        } else {
            let child_height = height - 1;
            let left_index = index.checked_mul(2).context("map coordinate overflow")?;
            let right_index = left_index + 1;
            let left = node(connection, scope, child_height, left_index)?;
            let right = node(connection, scope, child_height, right_index)?;
            ensure!(
                branch(scope, height, index, left, right)? == expected,
                "current map branch proof mismatch"
            );
            stack.push((child_height, right_index, right));
            stack.push((child_height, left_index, left));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
