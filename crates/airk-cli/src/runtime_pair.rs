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
            && metadata.len() <= 512 * 1024 * 1024,
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
    let mut remaining = 512 * 1024 * 1024u64;
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

pub fn verify_helper(main: &Path) -> Result<PathBuf> {
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
            && pair.version == env!("CARGO_PKG_VERSION")
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
