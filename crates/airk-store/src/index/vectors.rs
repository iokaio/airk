// SPDX-License-Identifier: Apache-2.0
//! The flat vector file and exact search over its committed rows.
//!
//! A file holds a 64-byte header and fixed-width rows of little-endian `f32`.
//! Only the row count committed in the database is meaningful: rows past it
//! are leftovers of an uncommitted batch and are overwritten by the next one.
use super::writer::VectorSetup;
use anyhow::{Context, Result, ensure};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

const MAGIC: &[u8; 8] = b"AIRKVEC1";
const HEADER_BYTES: u64 = 64;
pub const MAX_DIMENSIONS: u32 = 4096;

pub(super) fn file_name(generation: u64) -> String {
    format!("vectors-{generation}.f32")
}

pub(super) fn generation_of(name: &str) -> Option<u64> {
    name.strip_prefix("vectors-")?
        .strip_suffix(".f32")?
        .parse()
        .ok()
}

pub(super) fn validate(vector: &[f32], dimensions: u32) -> Result<()> {
    ensure!(
        vector.len() == dimensions as usize && vector.iter().all(|value| value.is_finite()),
        "vector has the wrong dimensions or a non-finite value"
    );
    Ok(())
}

fn header(setup: &VectorSetup) -> [u8; HEADER_BYTES as usize] {
    let mut bytes = [0u8; HEADER_BYTES as usize];
    bytes[..8].copy_from_slice(MAGIC);
    bytes[8..12].copy_from_slice(&setup.dimensions.to_le_bytes());
    bytes[16..48].copy_from_slice(setup.pipeline.as_bytes());
    bytes
}

fn row_bytes(setup: &VectorSetup) -> u64 {
    u64::from(setup.dimensions) * 4
}

pub(super) fn verify_committed_extent(
    directory: &Path,
    generation: u64,
    setup: &VectorSetup,
    rows: u64,
) -> Result<()> {
    let path = directory.join(file_name(generation));
    let mut file = match File::open(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(crate::current::PrivateIntegrityFailure)
                .context("plain index committed vector file is missing");
        }
        other => other.with_context(|| format!("open vector file {}", path.display()))?,
    };
    let expected_length = rows
        .checked_mul(row_bytes(setup))
        .and_then(|length| length.checked_add(HEADER_BYTES));
    let available = file.metadata()?.len();
    if !(1..=MAX_DIMENSIONS).contains(&setup.dimensions)
        || expected_length.is_none_or(|length| length > available)
    {
        return Err(crate::current::PrivateIntegrityFailure)
            .context("plain index committed vector extent is invalid or truncated");
    }
    VectorFile::check_header(&mut file, setup)?;
    Ok(())
}

/// One generation's file, opened for appending rows.
pub struct VectorFile {
    file: File,
    setup: VectorSetup,
    path: PathBuf,
}

impl VectorFile {
    pub(super) fn create(directory: &Path, generation: u64, setup: &VectorSetup) -> Result<()> {
        ensure!(
            (1..=MAX_DIMENSIONS).contains(&setup.dimensions),
            "unsupported vector dimensions"
        );
        let path = directory.join(file_name(generation));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| format!("create vector file {}", path.display()))?;
        file.write_all(&header(setup))?;
        file.sync_all()?;
        Ok(())
    }

    fn check_header(file: &mut File, setup: &VectorSetup) -> Result<()> {
        let mut bytes = [0u8; HEADER_BYTES as usize];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut bytes)
            .context("vector file header is truncated")?;
        if bytes != header(setup) {
            return Err(crate::current::PrivateIntegrityFailure)
                .context("vector file belongs to another embedding pipeline");
        }
        Ok(())
    }

    pub(super) fn open_append(
        directory: &Path,
        generation: u64,
        setup: &VectorSetup,
    ) -> Result<Self> {
        let path = directory.join(file_name(generation));
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("open vector file {}", path.display()))?;
        Self::check_header(&mut file, setup)?;
        Ok(Self {
            file,
            setup: setup.clone(),
            path,
        })
    }

    /// Write rows from `first` on, replacing any uncommitted leftovers there.
    pub(super) fn write_rows<'a>(
        &mut self,
        first: u64,
        rows: impl Iterator<Item = &'a [f32]>,
    ) -> Result<()> {
        let width = row_bytes(&self.setup);
        self.file
            .seek(SeekFrom::Start(HEADER_BYTES + first * width))?;
        let mut buffer = Vec::with_capacity(width as usize * 64);
        for row in rows {
            validate(row, self.setup.dimensions)?;
            for value in row {
                buffer.extend_from_slice(&value.to_le_bytes());
            }
            if buffer.len() >= (1 << 20) {
                self.file.write_all(&buffer)?;
                buffer.clear();
            }
        }
        self.file.write_all(&buffer)?;
        Ok(())
    }

    pub(super) fn sync(&mut self) -> Result<()> {
        self.file
            .sync_data()
            .with_context(|| format!("sync vector file {}", self.path.display()))
    }
}

/// Committed rows held in memory for exact search.
pub struct VectorMatrix {
    dimensions: usize,
    values: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorHit {
    pub row: u64,
    /// Dot product with the query. Rows and queries are unit-length, so this
    /// is the cosine similarity.
    pub score: f32,
}

/// Rows to consider in a filtered search.
pub type LiveRows = [u64];

impl VectorMatrix {
    /// Read the first `rows` rows of a generation's file.
    pub fn load(directory: &Path, generation: u64, setup: &VectorSetup, rows: u64) -> Result<Self> {
        let path = directory.join(file_name(generation));
        let mut file =
            File::open(&path).with_context(|| format!("open vector file {}", path.display()))?;
        VectorFile::check_header(&mut file, setup)?;
        let width = row_bytes(setup);
        let length = rows
            .checked_mul(width)
            .context("vector file size overflow")?;
        ensure!(
            file.metadata()?.len() >= HEADER_BYTES + length,
            "vector file is shorter than its committed rows"
        );
        let mut bytes = vec![0u8; usize::try_from(length)?];
        file.read_exact(&mut bytes)?;
        let (words, _) = bytes.as_chunks::<4>();
        let values = words.iter().map(|word| f32::from_le_bytes(*word)).collect();
        Ok(Self {
            dimensions: setup.dimensions as usize,
            values,
        })
    }

    pub fn rows(&self) -> u64 {
        (self.values.len() / self.dimensions.max(1)) as u64
    }

    pub fn row(&self, row: u64) -> Option<&[f32]> {
        let start = usize::try_from(row).ok()?.checked_mul(self.dimensions)?;
        self.values.get(start..start + self.dimensions)
    }

    /// The `limit` best rows, best first; ties go to the lower row. With
    /// `subset`, only those rows are scored.
    pub fn search(
        &self,
        query: &[f32],
        limit: usize,
        subset: Option<&LiveRows>,
    ) -> Result<Vec<VectorHit>> {
        ensure!(
            query.len() == self.dimensions && query.iter().all(|value| value.is_finite()),
            "query vector has the wrong dimensions or a non-finite value"
        );
        if limit == 0 {
            return Ok(Vec::new());
        }
        let score = |row: u64| -> Option<VectorHit> {
            let values = self.row(row)?;
            Some(VectorHit {
                row,
                score: values.iter().zip(query).map(|(a, b)| a * b).sum(),
            })
        };
        let rows: Vec<u64> = match subset {
            Some(subset) => subset.to_vec(),
            None => (0..self.rows()).collect(),
        };
        let threads = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(16)
            .min(rows.len().div_ceil(4096).max(1));
        let parts: Vec<Vec<VectorHit>> = if threads <= 1 {
            vec![best(rows.iter().filter_map(|row| score(*row)), limit)]
        } else {
            let size = rows.len().div_ceil(threads);
            std::thread::scope(|scope| {
                let handles: Vec<_> = rows
                    .chunks(size)
                    .map(|part| {
                        let score = &score;
                        scope.spawn(move || best(part.iter().filter_map(|row| score(*row)), limit))
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| handle.join().expect("vector search worker panicked"))
                    .collect()
            })
        };
        Ok(best(parts.into_iter().flatten(), limit))
    }
}

fn better(left: &VectorHit, right: &VectorHit) -> std::cmp::Ordering {
    right
        .score
        .total_cmp(&left.score)
        .then(left.row.cmp(&right.row))
}

fn best(hits: impl Iterator<Item = VectorHit>, limit: usize) -> Vec<VectorHit> {
    let mut kept: Vec<VectorHit> = Vec::with_capacity(limit + 1);
    for hit in hits {
        if kept.len() == limit
            && better(&hit, kept.last().expect("limit is positive")) != std::cmp::Ordering::Less
        {
            continue;
        }
        let at =
            kept.partition_point(|existing| better(existing, &hit) == std::cmp::Ordering::Less);
        kept.insert(at, hit);
        kept.truncate(limit);
    }
    kept
}
