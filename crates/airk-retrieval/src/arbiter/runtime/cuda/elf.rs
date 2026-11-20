// SPDX-License-Identifier: Apache-2.0
//! Inspect the same held ELF file used for hashing and module admission.
//! The loader consumes program headers, even when the section table is absent.
use std::io::{self, Read, Seek, SeekFrom};
use std::time::Instant;

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct DynamicImage {
    pub soname: Option<String>,
    pub needed: Vec<String>,
    pub origin_search: bool,
}

fn invalid(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

fn u16_at(bytes: &[u8], i: usize) -> u16 {
    u16::from_le_bytes(bytes[i..i + 2].try_into().expect("fixed ELF field"))
}
fn u32_at(bytes: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(bytes[i..i + 4].try_into().expect("fixed ELF field"))
}
fn u64_at(bytes: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(bytes[i..i + 8].try_into().expect("fixed ELF field"))
}

#[derive(Clone, Copy)]
struct Segment {
    offset: u64,
    address: u64,
    size: u64,
}

struct Reader<'a, R> {
    file: &'a mut R,
    len: u64,
    deadline: Instant,
}

impl<R: Read + Seek> Reader<'_, R> {
    fn read(&mut self, at: u64, into: &mut [u8]) -> io::Result<()> {
        if Instant::now() >= self.deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "ELF read deadline"));
        }
        if at
            .checked_add(into.len() as u64)
            .is_none_or(|n| n > self.len)
        {
            return Err(invalid("ELF extent outside held image"));
        }
        self.file.seek(SeekFrom::Start(at))?;
        self.file.read_exact(into)
    }
    fn string(&mut self, table: u64, table_len: u64, offset: u64) -> io::Result<String> {
        if offset >= table_len {
            return Err(invalid("dynamic string outside ELF string table"));
        }
        let mut bytes = [0_u8; 256];
        let len = (table_len - offset).min(bytes.len() as u64) as usize;
        self.read(table + offset, &mut bytes[..len])?;
        let end = bytes[..len]
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(|| invalid("unterminated or overlong ELF dependency name"))?;
        let value =
            std::str::from_utf8(&bytes[..end]).map_err(|_| invalid("non-UTF-8 ELF loader name"))?;
        Ok(value.to_owned())
    }
}

fn unique<T>(slot: &mut Option<T>, value: T) -> io::Result<()> {
    if slot.replace(value).is_some() {
        return Err(invalid("multiple instances of unique ELF metadata"));
    }
    Ok(())
}

pub(super) fn inspect<R: Read + Seek>(
    file: &mut R,
    len: u64,
    deadline: Instant,
) -> io::Result<DynamicImage> {
    let mut reader = Reader {
        file,
        len,
        deadline,
    };
    let mut header = [0; 64];
    reader.read(0, &mut header)?;
    if &header[..7] != b"\x7fELF\x02\x01\x01"
        || u16_at(&header, 16) != 3
        || u16_at(&header, 18) != 62
        || u32_at(&header, 20) != 1
        || u16_at(&header, 52) != 64
        || u16_at(&header, 54) != 56
    {
        return Err(invalid("expected little-endian x86-64 ELF64 shared object"));
    }
    let count = u16_at(&header, 56);
    if !(1..=1024).contains(&count) {
        return Err(invalid("invalid ELF program header count"));
    }
    let mut loads = Vec::new();
    let mut dynamic = None;
    for i in 0..count {
        let offset = u64_at(&header, 32)
            .checked_add(u64::from(i) * 56)
            .ok_or_else(|| invalid("ELF program header overflow"))?;
        let mut ph = [0; 56];
        reader.read(offset, &mut ph)?;
        let kind = u32_at(&ph, 0);
        if kind != PT_LOAD && kind != PT_DYNAMIC {
            continue;
        }
        let segment = Segment {
            offset: u64_at(&ph, 8),
            address: u64_at(&ph, 16),
            size: u64_at(&ph, 32),
        };
        if segment
            .offset
            .checked_add(segment.size)
            .is_none_or(|end| end > len)
            || segment.address.checked_add(segment.size).is_none()
            || segment.size > u64_at(&ph, 40)
        {
            return Err(invalid("invalid ELF segment extent"));
        }
        if kind == PT_LOAD {
            loads.push(segment);
        } else {
            unique(&mut dynamic, segment)?;
        }
    }
    let dynamic = dynamic.ok_or_else(|| invalid("missing ELF dynamic segment"))?;
    let mapped_offset = |address: u64, length: u64| -> io::Result<u64> {
        let mut offset = None;
        for load in &loads {
            if let Some(delta) = address.checked_sub(load.address)
                && delta
                    .checked_add(length)
                    .is_some_and(|end| end <= load.size)
            {
                unique(&mut offset, load.offset + delta)?;
            }
        }
        offset.ok_or_else(|| invalid("ELF dynamic metadata is not mapped by a file-backed segment"))
    };
    if dynamic.size > 64 * 1024
        || !dynamic.size.is_multiple_of(16)
        || mapped_offset(dynamic.address, dynamic.size)? != dynamic.offset
    {
        return Err(invalid("invalid ELF dynamic extent"));
    }
    let (mut strtab, mut strsz, mut soname, mut search) = (None, None, None, None);
    let mut imports = Vec::new();
    let mut terminated = false;
    for index in 0..dynamic.size / 16 {
        let mut entry = [0; 16];
        reader.read(dynamic.offset + index * 16, &mut entry)?;
        let tag = u64_at(&entry, 0);
        let value = u64_at(&entry, 8);
        match tag {
            0 => {
                terminated = true;
                break;
            }
            1 => imports.push(value), // DT_NEEDED preserves loader order.
            5 => unique(&mut strtab, value)?,
            10 => unique(&mut strsz, value)?,
            14 => unique(&mut soname, value)?,
            15 | 29 => unique(&mut search, value)?, // RPATH, RUNPATH
            // DT_AUDIT, DT_DEPAUDIT, DT_AUXILIARY, DT_FILTER select additional images.
            0x6fff_fefc | 0x6fff_fefb | 0x7fff_fffd | 0x7fff_ffff => {
                return Err(invalid("additional ELF loader dependency mechanism"));
            }
            _ => {}
        }
    }
    if !terminated {
        return Err(invalid("unterminated ELF dynamic segment"));
    }
    let size = strsz.ok_or_else(|| invalid("missing ELF dynamic string table size"))?;
    let table = mapped_offset(
        strtab.ok_or_else(|| invalid("missing ELF dynamic string table"))?,
        size,
    )?;
    let origin_search = if let Some(index) = search {
        if reader.string(table, size, index)? != "$ORIGIN" {
            return Err(invalid(
                "ELF loader search path requires exact sealed origin",
            ));
        }
        true
    } else {
        false
    };
    let mut name = |index| -> io::Result<String> {
        let value = reader.string(table, size, index)?;
        if value.is_empty()
            || value.starts_with('.')
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'_' | b'-'))
        {
            return Err(invalid(
                "ELF dependency requires a direct ASCII library name",
            ));
        }
        Ok(value)
    };
    Ok(DynamicImage {
        soname: soname.map(&mut name).transpose()?,
        needed: imports.into_iter().map(name).collect::<io::Result<_>>()?,
        origin_search,
    })
}

#[cfg(test)]
mod tests;
