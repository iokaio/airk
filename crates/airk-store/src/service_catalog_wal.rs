// SPDX-License-Identifier: Apache-2.0
//! Conservative read-only recognition of an owned registry WAL before SQLite opens it.
//! Format reference: https://sqlite.org/fileformat.html#walformat
//! SQLite performs snapshot/backup/checkpoint work; this adapter never replays pages.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, path::Path};

const PAGE_BYTES: usize = 4096;
const FRAME_BYTES: usize = 24 + PAGE_BYTES;
const MAX_WAL_BYTES: u64 = 64 * 1024 * 1024;

#[cfg(test)]
#[path = "service_catalog_wal_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalObservation {
    pub file_bytes: u64,
    pub valid_frames: u64,
    pub last_commit_frame: u64,
    pub committed_pages: u32,
    pub uncommitted_frames: u64,
    pub stale_epoch_tail_bytes: u64,
}
fn word(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(bytes.try_into().expect("four-byte field"))
}
fn checksum(bytes: &[u8], big_endian: bool, mut state: [u32; 2]) -> [u32; 2] {
    for pair in bytes.as_chunks::<8>().0 {
        let decode = |bytes: &[u8]| {
            if big_endian {
                u32::from_be_bytes(bytes.try_into().expect("checksum word"))
            } else {
                u32::from_le_bytes(bytes.try_into().expect("checksum word"))
            }
        };
        state[0] = state[0]
            .wrapping_add(decode(&pair[..4]))
            .wrapping_add(state[1]);
        state[1] = state[1]
            .wrapping_add(decode(&pair[4..]))
            .wrapping_add(state[0]);
    }
    state
}

pub(super) fn observe(path: &Path) -> Result<Option<WalObservation>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    crate::managed_collection::reject_file_aliases(path, &metadata)?;
    ensure!(
        metadata.len() <= MAX_WAL_BYTES,
        "registry WAL exceeds upgrade bound"
    );
    let mut result = WalObservation {
        file_bytes: metadata.len(),
        valid_frames: 0,
        last_commit_frame: 0,
        committed_pages: 0,
        uncommitted_frames: 0,
        stale_epoch_tail_bytes: 0,
    };
    if metadata.len() == 0 {
        return Ok(Some(result));
    }
    ensure!(
        metadata.len() >= 32 && (metadata.len() - 32).is_multiple_of(FRAME_BYTES as u64),
        "registry WAL has an incomplete frame; preserve it for explicit recovery"
    );
    let mut file = fs::File::open(path)?;
    let mut header = [0u8; 32];
    file.read_exact(&mut header)?;
    let magic = word(&header[..4]);
    ensure!(
        matches!(magic, 0x377f0682 | 0x377f0683)
            && word(&header[4..8]) == 3007000
            && word(&header[8..12]) == PAGE_BYTES as u32,
        "unrecognized registry WAL format"
    );
    let big_endian = magic == 0x377f0683;
    let mut state = checksum(&header[..24], big_endian, [0, 0]);
    ensure!(
        state == [word(&header[24..28]), word(&header[28..32])],
        "registry WAL header checksum mismatch"
    );
    let frame_count = (metadata.len() - 32) / FRAME_BYTES as u64;
    let mut frame = [0u8; FRAME_BYTES];
    let mut stale_tail = false;
    for number in 1..=frame_count {
        file.read_exact(&mut frame)?;
        if stale_tail {
            ensure!(
                frame[8..16] != header[16..24],
                "current WAL epoch resumes after an invalid frame"
            );
            result.stale_epoch_tail_bytes += FRAME_BYTES as u64;
            continue;
        }
        if frame[8..16] != header[16..24] {
            // SQLite can retain complete old-epoch frames after a reset. They
            // are counted explicitly and never interpreted as committed pages.
            let candidate = checksum(
                &frame[24..],
                big_endian,
                checksum(&frame[..8], big_endian, state),
            );
            ensure!(
                candidate != [word(&frame[16..20]), word(&frame[20..24])],
                "current WAL frame has damaged epoch salts"
            );
            stale_tail = true;
            result.stale_epoch_tail_bytes = FRAME_BYTES as u64;
            continue;
        }
        ensure!(
            (1..=8192).contains(&word(&frame[..4])) && word(&frame[4..8]) <= 8192,
            "registry WAL page exceeds database bound"
        );
        state = checksum(&frame[..8], big_endian, state);
        state = checksum(&frame[24..], big_endian, state);
        ensure!(
            state == [word(&frame[16..20]), word(&frame[20..24])],
            "registry WAL frame checksum mismatch; preserve committed state for recovery"
        );
        result.valid_frames = number;
        let pages = word(&frame[4..8]);
        if pages > 0 {
            result.last_commit_frame = number;
            result.committed_pages = pages;
        }
    }
    result.uncommitted_frames = result.valid_frames - result.last_commit_frame;
    ensure!(
        file.metadata()?.len() == metadata.len(),
        "registry WAL changed during owned validation"
    );
    Ok(Some(result))
}
