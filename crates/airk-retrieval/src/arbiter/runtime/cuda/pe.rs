// SPDX-License-Identifier: Apache-2.0
//! Bounded PE64 metadata, shared byte/file reader. Never executes dependency probes.
use super::*;
fn u16_at(b: &[u8], at: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(
        b.get(at..at + 2).context("truncated PE u16")?.try_into()?,
    ))
}
fn u32_at(b: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4).context("truncated PE u32")?.try_into()?,
    ))
}
#[cfg(test)]
pub(super) fn imports(bytes: &[u8]) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    inspect(bytes.len() as u64, |at, n| {
        Ok(bytes
            .get(at..at.checked_add(n).context("PE read overflow")?)
            .context("truncated PE data")?
            .to_vec())
    })
}
#[cfg(all(windows, target_arch = "x86_64"))]
pub(super) fn imports_file(
    file: &std::fs::File,
    deadline: Instant,
) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    use std::os::windows::fs::FileExt;
    inspect(file.metadata()?.len(), |at, n| {
        in_time(deadline)?;
        let mut bytes = vec![0u8; n];
        let mut read = 0;
        while read < n {
            let got = file.seek_read(&mut bytes[read..], (at + read) as u64)?;
            ensure!(got > 0, "truncated native PE metadata");
            read += got;
        }
        Ok(bytes)
    })
}
fn inspect(
    length: u64,
    read: impl Fn(usize, usize) -> Result<Vec<u8>>,
) -> Result<(BTreeSet<String>, BTreeSet<String>)> {
    ensure!(length >= 64, "truncated PE DOS header");
    let dos = read(0, 64)?;
    ensure!(dos.starts_with(b"MZ"), "not a PE image");
    let pe = usize::try_from(u32_at(&dos, 0x3c)?)?;
    ensure!(pe <= 1 << 20, "PE header offset exceeds bound");
    let coff = read(pe, 24)?;
    ensure!(
        coff.get(..4) == Some(b"PE\0\0") && u16_at(&coff, 4)? == 0x8664,
        "CUDA DLL must be PE64 AMD64"
    );
    let sections = usize::from(u16_at(&coff, 6)?);
    let optional_size = usize::from(u16_at(&coff, 20)?);
    ensure!(
        (1..=96).contains(&sections) && (240..=4096).contains(&optional_size),
        "unsupported PE64 header size"
    );
    let header = read(pe + 24, optional_size + sections * 40)?;
    ensure!(
        u16_at(&header, 0)? == 0x20b && u32_at(&header, 108)? >= 14,
        "unsupported PE64 metadata"
    );
    let headers = usize::try_from(u32_at(&header, 60)?)?;
    ensure!(headers <= 1 << 20, "PE header region exceeds bound");
    let rva = |value: u32| -> Result<usize> {
        let value = usize::try_from(value)?;
        if value < headers {
            ensure!((value as u64) < length, "PE RVA outside headers");
            return Ok(value);
        }
        for n in 0..sections {
            let row = optional_size + n * 40;
            let start = usize::try_from(u32_at(&header, row + 12)?)?;
            let size = usize::try_from(u32_at(&header, row + 16)?)?;
            let raw = usize::try_from(u32_at(&header, row + 20)?)?;
            if value >= start && value - start < size {
                let at = raw
                    .checked_add(value - start)
                    .context("PE section overflow")?;
                ensure!((at as u64) < length, "PE RVA outside file");
                return Ok(at);
            }
        }
        anyhow::bail!("PE RVA has no raw section")
    };
    let string = |value: u32| -> Result<String> {
        let at = rva(value)?;
        let size = 129usize.min(usize::try_from(length - at as u64)?);
        let bytes = read(at, size)?;
        let end = bytes
            .iter()
            .position(|v| *v == 0)
            .context("PE import name exceeds bound")?;
        let name = std::str::from_utf8(&bytes[..end])?.to_ascii_lowercase();
        ensure!(
            !name.is_empty()
                && name.ends_with(".dll")
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
                && !name.contains(".."),
            "invalid PE import name"
        );
        Ok(name)
    };
    let parse = |index: usize,
                 stride: usize,
                 name_offset: usize,
                 delay: bool|
     -> Result<BTreeSet<String>> {
        let entry = 112 + index * 8;
        let address = u32_at(&header, entry)?;
        let size = usize::try_from(u32_at(&header, entry + 4)?)?;
        if address == 0 {
            ensure!(size == 0, "inconsistent empty PE import directory");
            return Ok(BTreeSet::new());
        }
        ensure!(
            size >= stride && size <= 1 << 20,
            "PE import metadata exceeds bound"
        );
        let start = rva(address)?;
        ensure!(
            (start as u64)
                .checked_add(size as u64)
                .is_some_and(|v| v <= length),
            "PE import directory truncated"
        );
        let data = read(start, size)?;
        let mut found = BTreeSet::new();
        let mut terminated = false;
        for row in data.chunks_exact(stride).take(513) {
            if row.iter().all(|v| *v == 0) {
                terminated = true;
                break;
            }
            ensure!(found.len() < 512, "too many PE imports");
            if delay {
                ensure!(u32_at(row, 0)? == 1, "legacy VA delay imports unsupported");
            }
            ensure!(
                found.insert(string(u32_at(row, name_offset)?)?),
                "duplicate PE import descriptor"
            );
        }
        ensure!(terminated, "unterminated PE import directory");
        Ok(found)
    };
    Ok((parse(1, 20, 12, false)?, parse(13, 32, 4, true)?))
}
