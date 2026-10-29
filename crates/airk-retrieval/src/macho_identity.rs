// SPDX-License-Identifier: Apache-2.0
//! Pin loadable Mach-O bytes while allowing a replacement Apple code signature.
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};

pub(crate) fn payload_digest(data: &[u8]) -> Result<String> {
    ensure!(
        (32..=64 << 20).contains(&data.len()) && data[..4] == [0xcf, 0xfa, 0xed, 0xfe],
        "invalid Mach-O library"
    );
    let u32_at = |o| u32::from_le_bytes(data[o..o + 4].try_into().unwrap()) as usize;
    let u64_at = |o| u64::from_le_bytes(data[o..o + 8].try_into().unwrap());
    let (count, size) = (u32_at(16), u32_at(20));
    ensure!(
        (1..=256).contains(&count) && (8 * count..=(data.len() - 32).min(65536)).contains(&size),
        "invalid Mach-O command bounds"
    );
    let mut image = data.to_vec();
    let mut offset = 32;
    let mut signature = None;
    let mut linkedit = false;
    for _ in 0..count {
        ensure!(offset + 8 <= 32 + size, "truncated Mach-O command");
        let (command, length) = (u32_at(offset), u32_at(offset + 4));
        ensure!(
            length >= 8 && length % 8 == 0 && offset + length <= 32 + size,
            "invalid Mach-O command length"
        );
        if command == 0x1d {
            ensure!(
                signature.is_none() && length == 16,
                "invalid Mach-O code signature"
            );
            let (start, bytes) = (u32_at(offset + 8), u32_at(offset + 12));
            ensure!(
                start >= 32 + size && start.checked_add(bytes) == Some(data.len()),
                "invalid Mach-O signature boundary"
            );
            signature = Some(start);
            image[offset + 12..offset + 16].fill(0);
        } else if command == 0x19 {
            ensure!(length >= 72, "truncated Mach-O segment");
            if &data[offset + 8..offset + 24] == b"__LINKEDIT\0\0\0\0\0\0" {
                ensure!(!linkedit, "duplicate LINKEDIT");
                let (fileoff, filesize, vmsize) = (
                    u64_at(offset + 40),
                    u64_at(offset + 48),
                    u64_at(offset + 32),
                );
                ensure!(
                    fileoff.checked_add(filesize) == Some(data.len() as u64)
                        && vmsize >= filesize
                        && vmsize - filesize < 65536,
                    "invalid LINKEDIT bounds"
                );
                linkedit = true;
                image[offset + 32..offset + 40].fill(0);
                image[offset + 48..offset + 56].fill(0);
            }
        }
        offset += length;
    }
    ensure!(offset == 32 + size && linkedit, "missing Mach-O segment");
    let end = signature.ok_or_else(|| anyhow::anyhow!("missing Mach-O signature"))?;
    Ok(format!("{:x}", Sha256::digest(&image[..end])))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_signature_replacement_preserves_identity() -> Result<()> {
        let mut bytes = vec![0u8; 256];
        let put = |b: &mut [u8], o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_le_bytes());
        put(&mut bytes, 0, 0xfeedfacf);
        put(&mut bytes, 16, 2);
        put(&mut bytes, 20, 88);
        put(&mut bytes, 32, 0x19);
        put(&mut bytes, 36, 72);
        bytes[40..56].copy_from_slice(b"__LINKEDIT\0\0\0\0\0\0");
        put(&mut bytes, 64, 4096);
        put(&mut bytes, 72, 128);
        put(&mut bytes, 80, 128);
        put(&mut bytes, 104, 0x1d);
        put(&mut bytes, 108, 16);
        put(&mut bytes, 112, 240);
        put(&mut bytes, 116, 16);
        let original = payload_digest(&bytes)?;
        bytes.resize(272, 7);
        put(&mut bytes, 80, 144);
        put(&mut bytes, 116, 32);
        assert_eq!(payload_digest(&bytes)?, original);
        bytes[150] ^= 1;
        assert_ne!(payload_digest(&bytes)?, original);
        put(&mut bytes, 116, 33);
        assert!(payload_digest(&bytes).is_err());
        for end in 0..120 {
            assert!(payload_digest(&bytes[..end]).is_err());
        }
        Ok(())
    }
}
