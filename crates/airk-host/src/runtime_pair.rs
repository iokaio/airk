// SPDX-License-Identifier: Apache-2.0
//! Digest-bound executable pair in one immutable, trusted native runtime directory.
//! The installer owns authenticity verification; this adapter checks installed bytes.
use airk_core::runtime_metadata::{SPLIT_RUNTIME_SCHEMA, SplitRuntimeMetadata};
use anyhow::{Context, Result, ensure};
use serde::ser::SerializeStruct;
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

#[cfg(test)]
mod predecessor_tests;

#[cfg(test)]
mod split_tests;

/// Bind a maintenance preview to executable behavior and the paired helper's
/// expected identity. Missing/broken inference remains a separate availability
/// observation; this function never loads a model or starts a helper.
/// Hash this running image once. Health and brokerage use the same startup
/// identity, so a cold broker does not rehash a large executable on its thread.
pub fn current_executable_sha256() -> Result<String> {
    static DIGEST: std::sync::OnceLock<std::result::Result<String, String>> =
        std::sync::OnceLock::new();
    DIGEST
        .get_or_init(|| {
            digest_file(&std::env::current_exe().map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .cloned()
        .map_err(|_| anyhow::anyhow!("running executable identity is unavailable"))
}

pub fn engine_identity() -> Result<airk_core::identity::Sha256Digest> {
    static ID: std::sync::OnceLock<std::result::Result<airk_core::identity::Sha256Digest, String>> =
        std::sync::OnceLock::new();
    let result = ID.get_or_init(|| {
        (|| -> Result<_> {
            let main = std::env::current_exe()?.canonicalize()?;
            let digest = current_executable_sha256()?;
            let manifest = payload_root(&main)?.join("airk-runtime.json");
            engine_identity_from_receipt(&manifest, digest)
        })()
        .map_err(|error| error.to_string())
    });
    result
        .as_ref()
        .copied()
        .map_err(|_| anyhow::anyhow!("verified runtime identity is unavailable for maintenance"))
}

/// Installer observation using the same identity as the running candidate.
/// Authenticity and paired-helper admission remain installer responsibilities.
/// This does not start either executable or load a model.
pub fn engine_identity_at(main: &Path) -> Result<airk_core::identity::Sha256Digest> {
    engine_identity_at_version(main, env!("CARGO_PKG_VERSION"))
}

/// Observe an installer-verified image, including a predecessor or rollback
/// target. The version must come from its verified Approval, not its own receipt.
pub fn engine_identity_at_version(
    main: &Path,
    approved_version: &str,
) -> Result<airk_core::identity::Sha256Digest> {
    let main = main.canonicalize()?;
    engine_identity_from_receipt_version(
        &payload_root(&main)?.join("airk-runtime.json"),
        digest_file(&main)?,
        approved_version,
    )
}

fn engine_identity_from_receipt(
    manifest: &Path,
    digest: String,
) -> Result<airk_core::identity::Sha256Digest> {
    engine_identity_from_receipt_version(manifest, digest, env!("CARGO_PKG_VERSION"))
}

fn engine_identity_from_receipt_version(
    manifest: &Path,
    digest: String,
    approved_version: &str,
) -> Result<airk_core::identity::Sha256Digest> {
    let bytes = match fs::symlink_metadata(manifest) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 4096,
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
            ensure!(pair.airk_sha256 == digest, "engine pair identity mismatch");
            pair.validate_target(approved_version)?;
            pair.verify_split_cli(manifest.parent().context("runtime directory missing")?)?;
            serde_json::to_vec(&pair)?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => digest.into_bytes(),
        Err(error) => return Err(error.into()),
    };
    Ok(airk_core::identity::framed_digest(
        b"airk-maintenance-engine/1.4",
        &[&bytes],
    ))
}

pub struct RuntimePair {
    pub schema: u32,
    pub version: String,
    pub os: String,
    pub arch: String,
    pub airk_sha256: String,
    pub embed_sha256: String,
    /// Full C7 identity, never projected down to the predecessor fields.
    pub split: Option<SplitRuntimeMetadata>,
}

impl Serialize for RuntimePair {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        if self.split.is_some() || self.schema == SPLIT_RUNTIME_SCHEMA {
            return self
                .split_metadata()
                .map_err(serde::ser::Error::custom)?
                .serialize(serializer);
        }
        // Preserve the original schema-one/two declaration order and helper omission.
        let mut record = serializer.serialize_struct(
            "RuntimePair",
            if self.embed_sha256.is_empty() { 5 } else { 6 },
        )?;
        record.serialize_field("schema", &self.schema)?;
        record.serialize_field("version", &self.version)?;
        record.serialize_field("os", &self.os)?;
        record.serialize_field("arch", &self.arch)?;
        record.serialize_field("airk_sha256", &self.airk_sha256)?;
        if !self.embed_sha256.is_empty() {
            record.serialize_field("embed_sha256", &self.embed_sha256)?;
        }
        record.end()
    }
}

impl<'de> Deserialize<'de> for RuntimePair {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct LegacyWire {
            schema: u32,
            version: String,
            os: String,
            arch: String,
            airk_sha256: String,
            #[serde(default, deserialize_with = "present_digest")]
            embed_sha256: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Legacy(LegacyWire),
            Split(SplitRuntimeMetadata),
        }
        fn present_digest<'de, D: serde::Deserializer<'de>>(
            d: D,
        ) -> std::result::Result<Option<String>, D::Error> {
            String::deserialize(d).map(Some)
        }
        let wire = match Wire::deserialize(deserializer)? {
            Wire::Legacy(wire) => wire,
            Wire::Split(split) => {
                return Ok(Self {
                    schema: split.schema,
                    version: split.version.clone(),
                    os: split.os.clone(),
                    arch: split.arch.clone(),
                    airk_sha256: split.airk_sha256.to_string(),
                    embed_sha256: String::new(),
                    split: Some(split),
                });
            }
        };
        let embed_sha256 = match (wire.schema, wire.embed_sha256) {
            (1, Some(digest)) => {
                digest
                    .parse::<airk_core::identity::Sha256Digest>()
                    .map_err(serde::de::Error::custom)?;
                digest
            }
            (2, None) => String::new(),
            _ => {
                return Err(serde::de::Error::custom(
                    "invalid runtime executable contract",
                ));
            }
        };
        wire.airk_sha256
            .parse::<airk_core::identity::Sha256Digest>()
            .map_err(serde::de::Error::custom)?;
        Ok(Self {
            schema: wire.schema,
            version: wire.version,
            os: wire.os,
            arch: wire.arch,
            airk_sha256: wire.airk_sha256,
            embed_sha256,
            split: None,
        })
    }
}
impl RuntimePair {
    pub fn is_single_executable(&self) -> bool {
        matches!(self.schema, 2 | SPLIT_RUNTIME_SCHEMA)
    }
    pub fn validate_target(&self, expected_version: &str) -> Result<()> {
        if self.schema == SPLIT_RUNTIME_SCHEMA || self.split.is_some() {
            return self.split_metadata()?.validate_target(
                expected_version,
                std::env::consts::OS,
                std::env::consts::ARCH,
            );
        }
        ensure!(
            matches!(self.schema, 1 | 2)
                && self.version == expected_version
                && self.os == std::env::consts::OS
                && self.arch == std::env::consts::ARCH,
            "incompatible runtime executable contract"
        );
        self.airk_sha256
            .parse::<airk_core::identity::Sha256Digest>()?;
        if self.schema == 1 {
            self.embed_sha256
                .parse::<airk_core::identity::Sha256Digest>()?;
        } else {
            ensure!(
                self.embed_sha256.is_empty(),
                "single executable cannot name a helper"
            );
        }
        Ok(())
    }

    fn split_metadata(&self) -> Result<&SplitRuntimeMetadata> {
        let split = self
            .split
            .as_ref()
            .context("full split runtime metadata is missing")?;
        split.validate()?;
        ensure!(
            self.schema == SPLIT_RUNTIME_SCHEMA
                && self.schema == split.schema
                && self.version == split.version
                && self.os == split.os
                && self.arch == split.arch
                && self.airk_sha256 == split.airk_sha256.to_string()
                && self.embed_sha256.is_empty(),
            "inconsistent split runtime metadata view"
        );
        Ok(split)
    }

    fn verify_split_cli(&self, root: &Path) -> Result<()> {
        if self.schema == SPLIT_RUNTIME_SCHEMA || self.split.is_some() {
            let split = self.split_metadata()?;
            ensure!(
                digest_file(&root.join(split.cli_relative_path()?))?
                    == split.airkctl_sha256.to_string(),
                "split runtime CLI identity mismatch"
            );
        }
        Ok(())
    }
}

pub fn single_executable(main: &Path) -> Result<bool> {
    let manifest = payload_root(main)?.join("airk-runtime.json");
    if !manifest.try_exists()? {
        return Ok(true);
    } // uninstalled development image
    let metadata = fs::symlink_metadata(&manifest)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 4096,
        "invalid runtime executable receipt"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "redirected runtime executable receipt"
        );
    }
    let mut bytes = Vec::new();
    fs::File::open(&manifest)?
        .take(4097)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 4096,
        "runtime executable receipt exceeds limit"
    );
    let pair: RuntimePair = serde_json::from_slice(&bytes)?;
    Ok(pair.is_single_executable())
}

/// Historical stdio entry points must not resurrect a resident launcher or
/// per-client MCP process after selecting a single-executable installation.
pub fn require_legacy_stdio(main: &Path) -> Result<()> {
    ensure!(
        !single_executable(main)?,
        "stdio_mcp_retired: AIRK 1.8.14 uses the service's localhost HTTPS MCP endpoint. Reconnect through AIRK Explorer or use `airk setup` with an existing catalog/combined selection; migrate older owned configurations with `runtime http-legacy-connection`. Do not launch `airk mcp`, `bridge`, or an AIRK launcher."
    );
    Ok(())
}

pub fn require_legacy_inference_process(main: &Path) -> Result<()> {
    ensure!(
        !single_executable(main)?,
        "inference_helper_retired: AIRK 1.8.14 runs semantic inference inside the service; a shared inference guardian is not supported"
    );
    Ok(())
}

pub fn digest_file(path: &Path) -> Result<String> {
    digest_file_checked(path, &|| Ok(()))
}

pub fn digest_file_checked(path: &Path, check: &dyn Fn() -> Result<()>) -> Result<String> {
    check()?;
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
        check()?;
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
    ensure!(
        !single_executable(main)?,
        "this runtime embeds inference and has no helper executable"
    );
    verify_installed_pair(main, env!("CARGO_PKG_VERSION"))
}

pub fn payload_root(main: &Path) -> Result<&Path> {
    payload_root_for_layout(main, cfg!(target_os = "macos"))
}

fn payload_root_for_layout(main: &Path, app_layout: bool) -> Result<&Path> {
    let parent = main.parent().context("runtime directory missing")?;
    if app_layout && parent.file_name().is_some_and(|name| name == "MacOS") {
        let contents = parent.parent().context("app contents missing")?;
        let app = contents.parent().context("app bundle missing")?;
        ensure!(
            contents.file_name().is_some_and(|name| name == "Contents")
                && app.file_name().is_some_and(|name| name == "AIRK.app"),
            "unrecognized runtime app topology"
        );
        return app.parent().context("app payload directory missing");
    }
    Ok(parent)
}

/// A stable launcher can validate a later selected runtime without adopting its version.
pub fn verify_installed_pair(main: &Path, expected_version: &str) -> Result<PathBuf> {
    let (main, helper, pair) = read_installed_pair(main, expected_version)?;
    ensure!(
        digest_file(&main)? == pair.airk_sha256
            && (pair.is_single_executable() || digest_file(&helper)? == pair.embed_sha256),
        "runtime executable pair digest mismatch"
    );
    pair.verify_split_cli(payload_root(&main)?)?;
    Ok(helper)
}

/// Structural metadata read only: this does NOT verify executable bytes or
/// authorize execution. The installer compares these fields only after its full
/// approved inventory hash pass; standalone callers retain verify_installed_pair.
pub fn read_installed_pair(
    main: &Path,
    expected_version: &str,
) -> Result<(PathBuf, PathBuf, RuntimePair)> {
    let image = crate::process_roles::main_image(main)?;
    let main = image.as_path();
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
    let manifest = payload_root(main)?.join("airk-runtime.json");
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
    pair.validate_target(expected_version)?;
    let helper = if pair.is_single_executable() {
        main.to_path_buf()
    } else {
        parent.join(if cfg!(windows) {
            "airk-embed.exe"
        } else {
            "airk-embed"
        })
    };
    Ok((main.to_path_buf(), helper, pair))
}

#[cfg(test)]
mod single_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_pair_receipt_binds_helper_for_bare_and_bundle_layouts() -> Result<()> {
        for bundle in [false, true] {
            let owner = tempfile::tempdir()?;
            let root = owner.path();
            let main = root.join(if bundle {
                "AIRK.app/Contents/MacOS/airk"
            } else {
                "airk"
            });
            fs::create_dir_all(main.parent().unwrap())?;
            let manifest = payload_root_for_layout(&main, bundle)?.join("airk-runtime.json");
            assert_eq!(manifest, root.join("airk-runtime.json"));
            let mut pair = RuntimePair {
                schema: 1,
                version: env!("CARGO_PKG_VERSION").into(),
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                airk_sha256: "a".repeat(64),
                embed_sha256: "b".repeat(64),
                split: None,
            };
            fs::write(&manifest, serde_json::to_vec(&pair)?)?;
            let before = engine_identity_from_receipt(&manifest, pair.airk_sha256.clone())?;
            pair.embed_sha256 = "c".repeat(64);
            fs::write(&manifest, serde_json::to_vec(&pair)?)?;
            assert_ne!(
                before,
                engine_identity_from_receipt(&manifest, pair.airk_sha256.clone())?
            );
            assert!(engine_identity_from_receipt(&manifest, "d".repeat(64)).is_err());
        }
        assert!(
            payload_root_for_layout(Path::new("unknown.app/Contents/MacOS/airk"), true).is_err()
        );
        Ok(())
    }

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
            split: None,
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
