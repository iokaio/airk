// SPDX-License-Identifier: Apache-2.0
//! Digest-bound executable pair in one immutable, trusted native runtime directory.
//! The installer owns authenticity verification; this adapter checks installed bytes.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

// Unstripped Linux development executables include DWARF in the image itself.
// Hashing is streamed; the same finite ceiling also bounds a concurrently growing file.
const MAX_EXECUTABLE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Bind a maintenance preview to executable behavior and the paired helper's
/// expected identity. Missing/broken inference remains a separate availability
/// observation; this function never loads a model or starts a helper.
pub fn engine_identity() -> Result<airk_core::identity::Sha256Digest> {
    static ID: std::sync::OnceLock<std::result::Result<airk_core::identity::Sha256Digest, String>> =
        std::sync::OnceLock::new();
    let result = ID.get_or_init(|| {
        (|| -> Result<_> {
            let main = std::env::current_exe()?.canonicalize()?;
            let digest = digest_file(&main)?;
            let manifest = main
                .parent()
                .context("runtime parent missing")?
                .join("airk-runtime.json");
            let bytes = match fs::symlink_metadata(&manifest) {
                Ok(metadata) => {
                    ensure!(
                        metadata.is_file()
                            && !metadata.file_type().is_symlink()
                            && metadata.len() <= 4096,
                        "invalid engine pair receipt"
                    );
                    #[cfg(windows)]
                    {
                        use std::os::windows::fs::MetadataExt;
                        ensure!(
                            metadata.file_attributes() & 0x400 == 0,
                            "redirected engine pair receipt"
                        );
                    }
                    let mut bytes = Vec::new();
                    fs::File::open(manifest)?
                        .take(4097)
                        .read_to_end(&mut bytes)?;
                    ensure!(bytes.len() <= 4096, "oversized engine pair receipt");
                    let pair: RuntimePair = serde_json::from_slice(&bytes)?;
                    ensure!(
                        pair.schema == 1
                            && pair.version == env!("CARGO_PKG_VERSION")
                            && pair.os == std::env::consts::OS
                            && pair.arch == std::env::consts::ARCH
                            && pair.airk_sha256 == digest,
                        "engine pair identity mismatch"
                    );
                    pair.embed_sha256
                        .parse::<airk_core::identity::Sha256Digest>()?;
                    serde_json::to_vec(&pair)?
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => digest.into_bytes(),
                Err(error) => return Err(error.into()),
            };
            Ok(airk_core::identity::framed_digest(
                b"airk-maintenance-engine/1.4",
                &[&bytes],
            ))
        })()
        .map_err(|error| error.to_string())
    });
    result
        .as_ref()
        .copied()
        .map_err(|_| anyhow::anyhow!("verified runtime identity is unavailable for maintenance"))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimePair {
    pub schema: u32,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub airk_sha256: String,
    pub embed_sha256: String,
}

pub fn digest_file(path: &Path) -> Result<String> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= MAX_EXECUTABLE_BYTES,
        "invalid runtime component"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "redirected runtime component"
        );
    }
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut remaining = MAX_EXECUTABLE_BYTES;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        remaining = remaining
            .checked_sub(count as u64)
            .context("runtime component grew beyond limit")?;
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Reject malformed/wrong-target images before handing them to native process creation.
/// Successful header inspection does not establish dependency availability or approval.
pub fn validate_native_image(path: &Path) -> Result<()> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(65536).read_to_end(&mut bytes)?;
    #[cfg(windows)]
    {
        ensure!(
            bytes.len() >= 64 && &bytes[..2] == b"MZ",
            "invalid native PE executable header"
        );
        let offset = u32::from_le_bytes(bytes[60..64].try_into()?) as usize;
        let pe = bytes
            .get(offset..offset.saturating_add(26))
            .context("PE header exceeds inspection bound")?;
        let machine = if cfg!(target_arch = "x86_64") {
            0x8664
        } else {
            0xaa64
        };
        ensure!(
            &pe[..4] == b"PE\0\0"
                && u16::from_le_bytes(pe[4..6].try_into()?) == machine
                && u16::from_le_bytes(pe[24..26].try_into()?) == 0x20b,
            "runtime executable has incompatible PE architecture"
        );
    }
    #[cfg(target_os = "linux")]
    {
        ensure!(
            bytes.len() >= 20 && &bytes[..4] == b"\x7fELF" && bytes[4] == 2 && bytes[5] == 1,
            "invalid native ELF executable header"
        );
        let machine = if cfg!(target_arch = "x86_64") {
            62
        } else {
            183
        };
        ensure!(
            u16::from_le_bytes(bytes[18..20].try_into()?) == machine,
            "runtime executable has incompatible ELF architecture"
        );
    }
    #[cfg(target_os = "macos")]
    {
        ensure!(
            bytes.len() >= 16 && bytes[..4] == [0xcf, 0xfa, 0xed, 0xfe],
            "invalid native Mach-O executable header"
        );
        let cpu = if cfg!(target_arch = "aarch64") {
            0x0100000c
        } else {
            0x01000007
        };
        ensure!(
            u32::from_le_bytes(bytes[4..8].try_into()?) == cpu
                && u32::from_le_bytes(bytes[12..16].try_into()?) == 2,
            "runtime executable has incompatible Mach-O architecture"
        );
    }
    Ok(())
}

pub fn verify_helper(main: &Path) -> Result<PathBuf> {
    verify_installed_pair(main, env!("CARGO_PKG_VERSION"))
}

/// A stable launcher can validate a later selected runtime without adopting its version.
pub fn verify_installed_pair(main: &Path, expected_version: &str) -> Result<PathBuf> {
    let parent = main.parent().context("runtime directory missing")?;
    for ancestor in parent.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "redirected runtime directory"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "redirected runtime directory"
            );
        }
    }
    let manifest = parent.join("airk-runtime.json");
    let metadata =
        fs::symlink_metadata(&manifest).context("verified runtime pair manifest is missing")?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 4096,
        "invalid runtime pair manifest"
    );
    let mut bytes = Vec::new();
    fs::File::open(manifest)?
        .take(4097)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 4096, "runtime pair manifest exceeds limit");
    let pair: RuntimePair = serde_json::from_slice(&bytes)?;
    ensure!(
        pair.schema == 1
            && pair.version == expected_version
            && pair.os == std::env::consts::OS
            && pair.arch == std::env::consts::ARCH,
        "incompatible runtime pair"
    );
    let helper = parent.join(if cfg!(windows) {
        "airk-embed.exe"
    } else {
        "airk-embed"
    });
    ensure!(
        digest_file(main)? == pair.airk_sha256 && digest_file(&helper)? == pair.embed_sha256,
        "runtime executable pair digest mismatch"
    );
    Ok(helper)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_rejects_modified_helper_main_and_platform_without_execution() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let root = owner.path().canonicalize()?;
        let main = root.join(if cfg!(windows) { "airk.exe" } else { "airk" });
        let helper = root.join(if cfg!(windows) {
            "airk-embed.exe"
        } else {
            "airk-embed"
        });
        fs::write(&main, b"original main fixture")?;
        fs::write(&helper, b"original helper fixture")?;
        let mut pair = RuntimePair {
            schema: 1,
            version: env!("CARGO_PKG_VERSION").into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            airk_sha256: digest_file(&main)?,
            embed_sha256: digest_file(&helper)?,
        };
        let receipt = root.join("airk-runtime.json");
        fs::write(&receipt, serde_json::to_vec(&pair)?)?;
        assert_eq!(verify_helper(&main)?, helper);
        fs::write(&helper, b"modified helper fixture")?;
        assert!(verify_helper(&main).is_err());
        fs::write(&helper, b"original helper fixture")?;
        fs::write(&main, b"modified main fixture")?;
        assert!(verify_helper(&main).is_err());
        fs::write(&main, b"original main fixture")?;
        pair.arch = "foreign".into();
        fs::write(&receipt, serde_json::to_vec(&pair)?)?;
        assert!(verify_helper(&main).is_err());
        Ok(())
    }
}
