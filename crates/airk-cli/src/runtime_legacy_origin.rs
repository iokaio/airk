// SPDX-License-Identifier: Apache-2.0
//! Capture an original stdio declaration and an owned old extension helper.
//! No old executable is launched. This is origin evidence, never a grant,
//! distribution approval, process-exclusion proof, or deletion authorization.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
};

// Authentic 1.3.1 images are 281,591,032 bytes on Windows and 270,039,760
// bytes on ARM macOS. Keep a bounded ceiling that includes those producers.
const MAX_CACHE_IMAGE_BYTES: u64 = 512 * 1024 * 1024;

fn require_cache_image_size(bytes: u64) -> Result<()> {
    ensure!(
        bytes <= MAX_CACHE_IMAGE_BYTES,
        "legacy helper image exceeds inspection bound"
    );
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConfigFormat {
    CodexToml,
    McpServersJson,
    VscodeServersJson,
    OpenCodeJson,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConfigLocator {
    pub path: PathBuf,
    pub format: ConfigFormat,
    pub server: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_path: Option<Vec<String>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyAdditionalRoot {
    pub alias: String,
    pub path: PathBuf,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyDeclaration {
    pub role: String,
    pub root: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub collection: Option<String>,
    /// Kept distinct: this selector is not a spelling of --collection.
    pub index_collection: Option<String>,
    pub additional_roots: Vec<LegacyAdditionalRoot>,
    pub no_default_project: bool,
    pub service_install_dir: Option<PathBuf>,
    pub service_runtime_dir: Option<PathBuf>,
    pub service_project: Option<String>,
    pub argv: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StandaloneScope {
    pub root: PathBuf,
    pub data_dir: PathBuf,
    pub collection: String,
    pub namespace: String,
    pub additional_roots: Vec<LegacyAdditionalRoot>,
    pub no_default_project: bool,
    pub data_dir_defaulted: bool,
    pub collection_defaulted: bool,
}
impl StandaloneScope {
    pub(crate) fn collection_path(&self) -> PathBuf {
        self.data_dir.join(&self.namespace).join(&self.collection)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyOriginObservation {
    pub schema: u32,
    pub provenance: String,
    pub user: String,
    pub config_path: PathBuf,
    pub config_sha256: String,
    pub server: String,
    pub container_path: Vec<String>,
    /// Hash of the complete selected object, including policies/environment.
    /// Those bytes are never copied to a diagnostic or receipt as plaintext.
    pub selected_entry_sha256: String,
    pub global_storage: PathBuf,
    pub location_basis: String,
    pub executable: PathBuf,
    pub executable_sha256: String,
    pub executable_identity: String,
    pub cache_version: String,
    pub platform: String,
    pub digest_prefix: String,
    pub artifact_approved: bool,
    pub declaration: LegacyDeclaration,
}
pub(crate) struct NativeLegacyOrigin {
    observation: LegacyOriginObservation,
    config: CapturedFile,
    executable: CapturedFile,
}

/// Cache ownership authorizes only retirement of this generated image. It does
/// not approve running its bytes or grant access to a root/collection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LegacyCacheObservation {
    pub schema: u32,
    pub provenance: String,
    pub user: String,
    pub global_storage: PathBuf,
    pub global_storage_identity: String,
    pub helpers_identity: String,
    pub cache_directory: PathBuf,
    pub cache_directory_identity: String,
    pub executable: PathBuf,
    pub executable_sha256: String,
    pub executable_bytes: u64,
    pub executable_identity: String,
    pub executable_modified_unix_ns: String,
    pub cache_version: String,
    pub platform: String,
    pub digest_prefix: String,
    pub artifact_approved: bool,
}

pub(crate) struct NativeLegacyCacheImage {
    observation: LegacyCacheObservation,
    executable: Option<CapturedFile>,
    directories: Vec<(PathBuf, File, String)>,
}
impl NativeLegacyCacheImage {
    pub(crate) fn withdraw(&mut self, operation: &str) -> Result<()> {
        self.revalidate()?;
        let image = self
            .executable
            .as_mut()
            .context("cannot withdraw an absent legacy image")?;
        let target = withdrawal_path(&self.observation.executable, operation)?;
        ensure!(
            image.path == self.observation.executable,
            "legacy cache already withdrawn"
        );
        move_new(&image.path, &target)?;
        image.path = target;
        self.revalidate()?;
        #[cfg(unix)]
        open_directory(&self.observation.cache_directory)?.sync_all()?;
        Ok(())
    }
    pub(crate) fn restore_withdrawn(&mut self, operation: &str) -> Result<()> {
        self.revalidate()?;
        let image = self
            .executable
            .as_mut()
            .context("cannot restore an absent legacy image")?;
        ensure!(
            image.path == withdrawal_path(&self.observation.executable, operation)?,
            "legacy cache withdrawal binding changed"
        );
        move_new(&image.path, &self.observation.executable)?;
        image.path = self.observation.executable.clone();
        self.revalidate()?;
        #[cfg(unix)]
        open_directory(&self.observation.cache_directory)?.sync_all()?;
        Ok(())
    }
    pub(crate) fn observation(&self) -> &LegacyCacheObservation {
        &self.observation
    }
    pub(crate) fn exists(&self) -> bool {
        self.executable.is_some()
    }
    pub(crate) fn revalidate(&mut self) -> Result<()> {
        ensure!(
            self.observation.user == crate::local_ipc::current_user()?,
            "legacy cache user changed"
        );
        for (path, held, identity) in &self.directories {
            require_owned_directory(held)?;
            ensure!(
                directory_identity(held)? == *identity
                    && directory_identity(&open_directory(path)?)? == *identity,
                "legacy cache directory identity changed"
            );
        }
        if self.directories.len() == 3 {
            ensure!(
                fs::symlink_metadata(&self.observation.cache_directory)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                "a directory appeared after legacy cache removal"
            );
        }
        if let Some(image) = &mut self.executable {
            if image.path != self.observation.executable {
                ensure!(
                    fs::symlink_metadata(&self.observation.executable)
                        .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                    "legacy cache command reappeared during withdrawal"
                );
            }
            image.revalidate()?;
        } else {
            ensure!(
                fs::symlink_metadata(&self.observation.executable)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                "an image appeared after legacy cache removal"
            );
        }
        Ok(())
    }
}

pub(crate) fn inspect_caches_at_global_storage(
    global: &Path,
) -> Result<Vec<LegacyCacheObservation>> {
    inspect_caches_for_replan(global, &[])
}

/// Fresh inventory may skip an empty leaf only when the prior native journal
/// proves its exact directory identity and already absent original image.
pub(crate) fn inspect_caches_for_replan(
    global: &Path,
    previous: &[LegacyCacheObservation],
) -> Result<Vec<LegacyCacheObservation>> {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
    absolute(global)?;
    ensure!(
        editor_user_directory(global).is_some(),
        "invalid editor storage cache root"
    );
    require_owned_directory(&open_directory(global)?)?;
    let helpers = global.join("iokaio.airk/helpers");
    match fs::symlink_metadata(&helpers) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    require_owned_directory(&open_directory(&global.join("iokaio.airk"))?)?;
    require_owned_directory(&open_directory(&helpers)?)?;
    let mut entries = Vec::new();
    let mut bytes = 0u64;
    for (count, entry) in fs::read_dir(&helpers)?.enumerate() {
        ensure!(
            std::time::Instant::now() < until,
            "legacy helper cache inspection deadline exhausted"
        );
        ensure!(count < 128, "legacy helper cache count exceeds bound");
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("invalid legacy cache directory name")?;
        let (version, _, _) = cache_name(name)?;
        require_obsolete_cache_version(&version)?;
        let image = path.join(if cfg!(windows) { "airk.exe" } else { "airk" });
        let length = match fs::symlink_metadata(&image) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let saved = previous
                    .iter()
                    .find(|old| old.executable == image)
                    .context("unknown empty cache leaf cannot enter a replan")?;
                let mut proof = resume_cache(saved)?;
                proof.revalidate()?;
                ensure!(!proof.exists(), "cache image appeared during replan");
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        require_cache_image_size(length)?;
        bytes = bytes
            .checked_add(length)
            .context("legacy helper inventory size overflow")?;
        ensure!(
            bytes <= 2 * 1024 * 1024 * 1024,
            "legacy helper inventory exceeds inspection bound"
        );
        entries.push(path);
    }
    entries.sort();
    entries
        .into_iter()
        .map(|path| {
            ensure!(
                std::time::Instant::now() < until,
                "legacy helper cache inspection deadline exhausted"
            );
            let captured = capture_cache(global, &path, None)?;
            ensure!(
                std::time::Instant::now() < until,
                "legacy helper cache inspection deadline exhausted"
            );
            // Preparation persists observations, not an unbounded collection of
            // open descriptors. Retirement reacquires each exact native identity.
            Ok(captured.observation().clone())
        })
        .collect()
}

/// Only an exact original native journal observation can resume an already
/// absent image. Callers still need independent process/image exclusion proof.
pub(crate) fn resume_cache(observed: &LegacyCacheObservation) -> Result<NativeLegacyCacheImage> {
    capture_cache(
        &observed.global_storage,
        &observed.cache_directory,
        Some(observed),
    )
}

pub(crate) fn resume_withdrawn_cache(
    observed: &LegacyCacheObservation,
    operation: &str,
) -> Result<NativeLegacyCacheImage> {
    let target = withdrawal_path(&observed.executable, operation)?;
    capture_cache_image(
        &observed.global_storage,
        &observed.cache_directory,
        Some(observed),
        Some(&target),
    )
}

/// Cheap intermediate archive guard over previously fully hashed native
/// observations. Full digests are checked again at transaction boundaries.
pub(crate) fn revalidate_withdrawn_cache_identity(
    cache: &LegacyCacheObservation,
    operation: &str,
) -> Result<()> {
    ensure!(
        cache.user == crate::local_ipc::current_user()?
            && cache.schema == 1
            && !cache.artifact_approved,
        "legacy cache identity user/schema changed"
    );
    let directories = [
        cache.global_storage.clone(),
        cache.global_storage.join("iokaio.airk"),
        cache.global_storage.join("iokaio.airk/helpers"),
        cache.cache_directory.clone(),
    ];
    for (index, path) in directories.iter().enumerate() {
        let directory = open_directory(path)?;
        require_owned_directory(&directory)?;
        let expected = match index {
            0 => Some(&cache.global_storage_identity),
            2 => Some(&cache.helpers_identity),
            3 => Some(&cache.cache_directory_identity),
            _ => None,
        };
        if let Some(expected) = expected {
            ensure!(
                directory_identity(&directory)? == *expected,
                "legacy cache directory identity changed during archive copy"
            );
        }
    }
    ensure!(
        fs::symlink_metadata(&cache.executable)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "old command reappeared during archive copy"
    );
    let target = withdrawal_path(&cache.executable, operation)?;
    let held = CapturedFile::open_inner(
        &target,
        MAX_CACHE_IMAGE_BYTES,
        Some(&cache.executable_sha256),
    )?;
    ensure!(
        held.identity == cache.executable_identity
            && held.bytes == cache.executable_bytes
            && modified_stamp(held.modified)? == cache.executable_modified_unix_ns,
        "legacy cache image identity or change metadata changed during archive copy"
    );
    held.revalidate_identity()
}

fn modified_stamp(time: std::time::SystemTime) -> Result<String> {
    Ok(time
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos()
        .to_string())
}

fn capture_cache(
    global: &Path,
    cache: &Path,
    previous: Option<&LegacyCacheObservation>,
) -> Result<NativeLegacyCacheImage> {
    capture_cache_image(global, cache, previous, None)
}
fn capture_cache_image(
    global: &Path,
    cache: &Path,
    previous: Option<&LegacyCacheObservation>,
    relocated: Option<&Path>,
) -> Result<NativeLegacyCacheImage> {
    absolute(global)?;
    ensure!(
        editor_user_directory(global).is_some(),
        "invalid editor storage cache root"
    );
    let helpers = global.join("iokaio.airk/helpers");
    ensure!(
        cache.parent() == Some(helpers.as_path()),
        "foreign legacy cache directory"
    );
    let name = cache
        .file_name()
        .and_then(|name| name.to_str())
        .context("invalid legacy cache directory name")?;
    let (version, platform, prefix) = cache_name(name)?;
    require_obsolete_cache_version(&version)?;
    let image = cache.join(if cfg!(windows) { "airk.exe" } else { "airk" });
    let mut directories = Vec::new();
    for path in [global.to_owned(), global.join("iokaio.airk"), helpers] {
        let file = open_directory(&path)?;
        require_owned_directory(&file)?;
        let identity = directory_identity(&file)?;
        directories.push((path, file, identity));
    }
    let cache_exists = match fs::symlink_metadata(cache) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && previous.is_some() => false,
        Err(error) => return Err(error.into()),
    };
    if cache_exists {
        let file = open_directory(cache)?;
        require_owned_directory(&file)?;
        let identity = directory_identity(&file)?;
        directories.push((cache.to_owned(), file, identity));
        for (count, entry) in fs::read_dir(cache)?.enumerate() {
            ensure!(
                count == 0,
                "unknown additional legacy cache entries preserved"
            );
            ensure!(
                entry?.path() == relocated.unwrap_or(&image),
                "unknown legacy cache entry preserved"
            );
        }
    }
    let captured = match fs::symlink_metadata(relocated.unwrap_or(&image)) {
        Ok(_) => Some(CapturedFile::open(
            relocated.unwrap_or(&image),
            MAX_CACHE_IMAGE_BYTES,
        )?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let observed = if let Some(captured) = &captured {
        ensure!(
            cache_exists,
            "legacy cache directory appeared during inspection"
        );
        ensure!(
            captured.digest.starts_with(&prefix),
            "legacy helper full hash differs from generated cache name"
        );
        LegacyCacheObservation {
            schema: 1,
            provenance: "owned_legacy_helper_cache".into(),
            user: crate::local_ipc::current_user()?,
            global_storage: global.into(),
            global_storage_identity: directories[0].2.clone(),
            helpers_identity: directories[2].2.clone(),
            cache_directory: cache.into(),
            cache_directory_identity: directories[3].2.clone(),
            executable: image,
            executable_sha256: captured.digest.clone(),
            executable_bytes: captured.file.metadata()?.len(),
            executable_identity: captured.identity.clone(),
            executable_modified_unix_ns: modified_stamp(captured.modified)?,
            cache_version: version,
            platform,
            digest_prefix: prefix,
            artifact_approved: false,
        }
    } else {
        let saved =
            previous.context("empty legacy cache has no original native retirement receipt")?;
        ensure!(
            saved.schema == 1
                && saved.provenance == "owned_legacy_helper_cache"
                && !saved.artifact_approved
                && saved.user == crate::local_ipc::current_user()?
                && saved.global_storage == global
                && saved.cache_directory == cache
                && saved.executable == image
                && saved.cache_version == version
                && saved.platform == platform
                && saved.digest_prefix == prefix
                && saved.executable_sha256.len() == 64
                && saved
                    .executable_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                && saved.executable_sha256.starts_with(&prefix)
                && saved.executable_bytes <= MAX_CACHE_IMAGE_BYTES
                && !saved.cache_directory_identity.is_empty()
                && !saved.executable_identity.is_empty()
                && saved.global_storage_identity == directories[0].2
                && saved.helpers_identity == directories[2].2
                && (!cache_exists || saved.cache_directory_identity == directories[3].2),
            "crossed missing-image cache retirement receipt"
        );
        saved.clone()
    };
    if let Some(saved) = previous {
        ensure!(&observed == saved, "legacy cache preimage changed");
    }
    let mut result = NativeLegacyCacheImage {
        observation: observed,
        executable: captured,
        directories,
    };
    result.revalidate()?;
    Ok(result)
}
impl LegacyOriginObservation {
    pub(crate) fn identity(&self) -> Result<airk_core::identity::Sha256Digest> {
        Ok(airk_core::identity::framed_digest(
            b"airk/legacy-origin/1",
            &[&serde_json::to_vec(self)?],
        ))
    }
    pub(crate) fn standalone_scope(&self) -> Result<StandaloneScope> {
        let declaration = &self.declaration;
        ensure!(
            declaration.role == "mcp"
                && declaration.service_install_dir.is_none()
                && declaration.service_runtime_dir.is_none()
                && declaration.service_project.is_none()
                && declaration.index_collection.is_none(),
            "declaration is not an original standalone scope"
        );
        let core = self
            .cache_version
            .split(['-', '+'])
            .next()
            .context("missing legacy version")?;
        let numbers = core
            .split('.')
            .map(str::parse::<u64>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ensure!(
            numbers.len() == 3
                && numbers.as_slice() >= [0, 2, 1].as_slice()
                && numbers.as_slice() < [1, 4, 0].as_slice(),
            "unsupported standalone version family"
        );
        let root = declaration
            .root
            .clone()
            .context("legacy root depends on an uncaptured historical working directory")?;
        absolute(&root)?;
        let data_dir = declaration
            .data_dir
            .clone()
            .unwrap_or_else(|| root.join(".airk"));
        absolute(&data_dir)?;
        let collection = declaration
            .collection
            .clone()
            .unwrap_or_else(|| "default".into());
        ensure!(
            collection.len() <= 255
                && !collection.is_empty()
                && !collection.contains(['/', '\\', ':'])
                && collection != "."
                && collection != "..",
            "invalid legacy collection component"
        );
        Ok(StandaloneScope {
            root,
            data_dir,
            collection,
            namespace: if numbers.as_slice() >= [1, 2, 0].as_slice() {
                "collections-1.2"
            } else {
                "collections"
            }
            .into(),
            additional_roots: declaration.additional_roots.clone(),
            no_default_project: declaration.no_default_project,
            data_dir_defaulted: declaration.data_dir.is_none(),
            collection_defaulted: declaration.collection.is_none(),
        })
    }
}
impl NativeLegacyOrigin {
    pub(crate) fn revalidate_identity(&self) -> Result<()> {
        ensure!(
            crate::local_ipc::current_user()? == self.observation.user,
            "legacy origin user changed"
        );
        self.config.revalidate_identity()?;
        if self.executable.path != self.observation.executable {
            ensure!(
                fs::symlink_metadata(&self.observation.executable)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                "original legacy command reappeared during saved capture"
            );
        }
        self.executable.revalidate_identity()
    }
    pub(crate) fn identity(&self) -> Result<airk_core::identity::Sha256Digest> {
        self.observation.identity()
    }
    /// Source-verified official standalone families: root was launch-CWD based
    /// when omitted, so only an explicit absolute root can migrate unattended.
    pub(crate) fn standalone_scope(&self) -> Result<StandaloneScope> {
        self.observation.standalone_scope()
    }
    pub(crate) fn observation(&self) -> &LegacyOriginObservation {
        &self.observation
    }
    pub(crate) fn executable(&self) -> &Path {
        &self.observation.executable
    }
    /// The caller first durably records the complete original observation and
    /// operation. Only that exact image can be withdrawn; no executable runs.
    #[cfg(test)]
    pub(crate) fn withdraw(&mut self, operation: &str) -> Result<PathBuf> {
        self.revalidate()?;
        let target = withdrawal_path(&self.observation.executable, operation)?;
        ensure!(
            self.executable.path == self.observation.executable,
            "legacy image already withdrawn"
        );
        move_new(&self.observation.executable, &target)?;
        self.executable.path = target.clone();
        self.revalidate()?;
        #[cfg(unix)]
        open_directory(
            target
                .parent()
                .context("legacy withdrawal parent missing")?,
        )?
        .sync_all()?;
        Ok(target)
    }
    #[cfg(test)]
    pub(crate) fn restore_withdrawn(&mut self, operation: &str) -> Result<()> {
        self.revalidate()?;
        let target = withdrawal_path(&self.observation.executable, operation)?;
        ensure!(
            self.executable.path == target,
            "legacy withdrawn image binding changed"
        );
        move_new(&target, &self.observation.executable)?;
        self.executable.path = self.observation.executable.clone();
        self.revalidate()?;
        #[cfg(unix)]
        open_directory(
            target
                .parent()
                .context("legacy withdrawal parent missing")?,
        )?
        .sync_all()?;
        Ok(())
    }
    pub(crate) fn revalidate(&mut self) -> Result<()> {
        ensure!(
            crate::local_ipc::current_user()? == self.observation.user,
            "legacy origin user changed"
        );
        self.config.revalidate()?;
        if self.executable.path != self.observation.executable {
            ensure!(
                fs::symlink_metadata(&self.observation.executable)
                    .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
                "original legacy command reappeared during saved capture"
            );
        }
        self.executable.revalidate()
    }
}

pub(crate) fn withdrawal_path(original: &Path, operation: &str) -> Result<PathBuf> {
    ensure!(
        operation.len() == 32
            && operation
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "invalid legacy withdrawal operation"
    );
    Ok(original
        .parent()
        .context("legacy image parent missing")?
        .join(format!(".airk-withheld-{operation}")))
}

pub(crate) fn resume_withdrawn(
    locator: &ConfigLocator,
    previous: &LegacyOriginObservation,
    operation: &str,
) -> Result<NativeLegacyOrigin> {
    let target = withdrawal_path(&previous.executable, operation)?;
    let mut proof = inspect_in_image(
        locator,
        std::slice::from_ref(&previous.global_storage),
        Some(&target),
    )?;
    ensure!(
        matches!(
            previous.location_basis.as_str(),
            "native_default" | "explicit_owned_editor_storage"
        ),
        "unknown original storage provenance"
    );
    if previous.location_basis == "native_default" {
        ensure!(
            native_cache_roots()?
                .iter()
                .any(|root| same_directory(root, &previous.global_storage)),
            "original native-default storage is no longer a native user editor location"
        );
    }
    proof.observation.location_basis = previous.location_basis.clone();
    ensure!(
        &proof.observation == previous,
        "withdrawn legacy origin differs from original durable observation"
    );
    Ok(proof)
}

/// Checks actual saved configuration bytes without creating HTTP authority.
pub(crate) fn require_migrated_origins(
    origins: &[LegacyOriginObservation],
    images: &[PathBuf],
) -> Result<Vec<serde_json::Value>> {
    let mut receipts = Vec::new();
    for origin in origins {
        let mut file = CapturedFile::open(&origin.config_path, 1024 * 1024)?;
        let bytes = file.bytes(1024 * 1024)?;
        let document = if origin.container_path.first().map(String::as_str) == Some("mcp_servers") {
            let parsed: toml::Value =
                toml::from_str(std::str::from_utf8(&bytes)?).map_err(|_| {
                    anyhow::anyhow!("invalid saved Codex configuration during retirement")
                })?;
            serde_json::to_value(parsed)?
        } else {
            serde_json::from_slice::<UniqueJson>(&bytes)
                .map_err(|_| anyhow::anyhow!("invalid saved MCP configuration during retirement"))?
                .0
        };
        let mut container = Some(&document);
        for key in &origin.container_path {
            container = container.and_then(|value| value.get(key));
        }
        if let Some(container) = container {
            let servers = container
                .as_object()
                .context("saved MCP container changed type")?;
            ensure!(
                servers.len() <= 256,
                "saved MCP server count exceeds native retirement bound"
            );
            for entry in servers.values() {
                let command = entry.get("command").and_then(|value| {
                    value.as_str().or_else(|| {
                        value
                            .as_array()
                            .and_then(|values| values.first())
                            .and_then(|value| value.as_str())
                    })
                });
                if let Some(command) = command {
                    ensure!(
                        !images
                            .iter()
                            .any(|image| same_path_label(Path::new(command), image)),
                        "legacy cache is still referenced by an original MCP configuration"
                    );
                }
            }
            if let Some(selected) = servers.get(&origin.server) {
                let object = selected
                    .as_object()
                    .context("original MCP entry changed type")?;
                ensure!(
                    !object.contains_key("command") && !object.contains_key("args"),
                    "original MCP entry has not completed HTTPS migration"
                );
                let url = object
                    .get("url")
                    .and_then(|value| value.as_str())
                    .context("original MCP entry lacks migrated HTTPS endpoint")?;
                let port = url
                    .strip_prefix("https://127.0.0.1:")
                    .and_then(|value| value.strip_suffix("/mcp"))
                    .context("migrated MCP endpoint is not the canonical local HTTPS transport")?;
                ensure!(
                    !port.is_empty()
                        && port.bytes().all(|byte| byte.is_ascii_digit())
                        && port.parse::<u16>().is_ok_and(|port| port > 0),
                    "invalid migrated local HTTPS port"
                );
            }
        }
        file.revalidate()?;
        receipts.push(serde_json::json!({"path":origin.config_path,"server":origin.server,"container_path":origin.container_path,"original_sha256":origin.config_sha256,"saved_sha256":file.digest,"original_stdio_reference_absent":true}));
    }
    Ok(receipts)
}
fn same_path_label(left: &Path, right: &Path) -> bool {
    match (
        crate::user_paths::canonical_location(left),
        crate::user_paths::canonical_location(right),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn move_new(from: &Path, to: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let from: Vec<_> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<_> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: terminated native paths; zero flags forbid replacing any destination.
        ensure!(
            unsafe {
                windows_sys::Win32::Storage::FileSystem::MoveFileExW(from.as_ptr(), to.as_ptr(), 0)
            } != 0,
            "exclusive legacy withdrawal failed: {}",
            std::io::Error::last_os_error()
        );
    }
    #[cfg(unix)]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let from = CString::new(from.as_os_str().as_bytes())?;
        let to = CString::new(to.as_os_str().as_bytes())?;
        #[cfg(target_os = "linux")]
        // SAFETY: terminated paths; native exclusive rename refuses any destination.
        let result = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(target_os = "macos")]
        // SAFETY: terminated paths and documented exclusive rename flag.
        let result = unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) };
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        bail!("exclusive legacy withdrawal unsupported on this OS");
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        ensure!(
            result == 0,
            "exclusive legacy withdrawal failed: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}

pub(crate) fn native_global_stores() -> Result<Vec<PathBuf>> {
    let base = if cfg!(target_os = "macos") {
        crate::user_paths::home()?.join("Library/Application Support")
    } else if cfg!(windows) {
        // Native configuration_base is <RoamingAppData>/AIRK, whereas
        // VS Code's user data is its sibling, not inside AIRK state.
        crate::user_paths::configuration_base()?
            .parent()
            .context("native roaming directory is absent")?
            .to_owned()
    } else {
        crate::user_paths::configuration_base()?
    };
    Ok(["Code", "Code - Insiders", "VSCodium"]
        .map(|name| base.join(name).join("User/globalStorage"))
        .into())
}
pub(crate) fn inspect(locator: &ConfigLocator) -> Result<NativeLegacyOrigin> {
    let roots = native_global_stores()?;
    let mut result = inspect_in(locator, &roots)?;
    result.observation.location_basis = "native_default".into();
    Ok(result)
}

pub(crate) fn verified_global_storage(global: &Path) -> Result<PathBuf> {
    absolute(global)?;
    ensure!(
        editor_user_directory(global).is_some(),
        "invalid editor global storage root"
    );
    require_owned_directory(&open_directory(global)?)?;
    let canonical = global.canonicalize()?;
    ensure!(
        editor_user_directory(&canonical).is_some(),
        "redirected editor global storage layout"
    );
    Ok(canonical)
}

pub(crate) fn native_cache_roots() -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    for root in native_global_stores()? {
        match fs::symlink_metadata(&root) {
            Ok(_) => roots.push(verified_global_storage(&root)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
        let user = root
            .parent()
            .context("native editor user directory missing")?;
        let profiles = user.join("profiles");
        match fs::symlink_metadata(&profiles) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
            Ok(_) => (),
        }
        require_owned_directory(&open_directory(user)?)?;
        require_owned_directory(&open_directory(&profiles)?)?;
        for (count, entry) in fs::read_dir(&profiles)?.enumerate() {
            ensure!(count < 128, "native editor profile count exceeds bound");
            let path = entry?.path().join("globalStorage");
            match fs::symlink_metadata(&path) {
                Ok(_) => roots.push(verified_global_storage(&path)?),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
    }
    roots.sort();
    roots.dedup();
    ensure!(roots.len() <= 128, "native editor root count exceeds bound");
    Ok(roots)
}

/// Discover a custom profile only from an actual original declaration's image,
/// then perform the full held config/cache proof. A project key is not authority.
pub(crate) fn inspect_declared_cache(locator: &ConfigLocator) -> Result<NativeLegacyOrigin> {
    let mut original = CapturedFile::open(&locator.path, 1024 * 1024)?;
    let (command, _, _) = declaration_from_config_at(
        &original.bytes(1024 * 1024)?,
        locator.format,
        &locator.server,
        locator.container_path.as_deref(),
    )?;
    let command = PathBuf::from(command);
    let global = command
        .ancestors()
        .nth(4)
        .context("legacy declared cache lacks global storage")?;
    let defaults = native_global_stores()?;
    let proof = if defaults.iter().any(|root| {
        same_directory(root, global)
            || editor_user_directory(global).is_some_and(|user| {
                root.parent()
                    .is_some_and(|parent| same_directory(parent, user))
            })
    }) {
        inspect(locator)?
    } else {
        inspect_at_global_storage(locator, global)?
    };
    original.revalidate()?;
    ensure!(
        proof.observation().config_sha256 == original.digest,
        "original declaration changed during cache discovery"
    );
    Ok(proof)
}
/// The frontend supplies its real extension globalStorage context for a
/// custom editor user-data directory. It supplies no owner/hash assertion:
/// native code captures those from existing objects and the original config.
/// This remains origin evidence, never approval of executable bytes.
pub(crate) fn inspect_at_global_storage(
    locator: &ConfigLocator,
    global_storage: &Path,
) -> Result<NativeLegacyOrigin> {
    absolute(global_storage)?;
    ensure!(
        editor_user_directory(global_storage).is_some(),
        "explicit editor storage must retain User/globalStorage or User/profiles/<id>/globalStorage layout"
    );
    let root = open_directory(global_storage)?;
    require_owned_directory(&root)?;
    inspect_in(locator, &[global_storage.to_owned()])
}
fn inspect_in(locator: &ConfigLocator, global_stores: &[PathBuf]) -> Result<NativeLegacyOrigin> {
    inspect_in_image(locator, global_stores, None)
}
fn inspect_in_image(
    locator: &ConfigLocator,
    global_stores: &[PathBuf],
    relocated: Option<&Path>,
) -> Result<NativeLegacyOrigin> {
    bounded(&locator.server, 256)?;
    let mut config = CapturedFile::open(&locator.path, 1024 * 1024)?;
    let bytes = config.bytes(1024 * 1024)?;
    let (command, argv, entry_hash) = if locator.container_path.is_some() {
        declaration_from_config_at(
            &bytes,
            locator.format,
            &locator.server,
            locator.container_path.as_deref(),
        )?
    } else {
        declaration_from_config(&bytes, locator.format, &locator.server)?
    };
    let declaration = normalize_argv(&argv)?;
    let command = PathBuf::from(command);
    absolute(&command)?;
    let cache = command
        .parent()
        .context("legacy helper lacks cache directory")?;
    let helpers = cache
        .parent()
        .context("legacy helper lacks helpers directory")?;
    let extension = helpers
        .parent()
        .context("legacy helper lacks extension directory")?;
    let global = extension
        .parent()
        .context("legacy helper lacks global storage")?;
    ensure!(
        command.file_name().and_then(|v| v.to_str())
            == Some(if cfg!(windows) { "airk.exe" } else { "airk" }),
        "not an AIRK helper image"
    );
    ensure!(
        helpers.file_name().and_then(|v| v.to_str()) == Some("helpers")
            && extension.file_name().and_then(|v| v.to_str()) == Some("iokaio.airk"),
        "not an owned legacy AIRK helper cache"
    );
    ensure!(
        global_stores.iter().any(|p| same_directory(p, global)
            || editor_user_directory(global).is_some_and(|user| p
                .parent()
                .is_some_and(|parent| same_directory(parent, user)))),
        "legacy helper is outside a native user editor global storage location"
    );
    // Capture native ownership of the actual storage and each helper-cache
    // directory. A caller-named path alone cannot establish this provenance.
    for directory in [global, extension, helpers, cache] {
        require_owned_directory(&open_directory(directory)?)?;
    }
    let name = cache
        .file_name()
        .and_then(|v| v.to_str())
        .context("invalid helper cache name")?;
    let (version, platform, prefix) = cache_name(name)?;
    if let Some(relocated) = relocated {
        ensure!(
            relocated.parent() == command.parent()
                && relocated
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(".airk-withheld-")),
            "foreign withdrawn legacy image"
        );
    }
    let executable = CapturedFile::open(relocated.unwrap_or(&command), MAX_CACHE_IMAGE_BYTES)?;
    ensure!(
        executable.digest.starts_with(&prefix),
        "legacy helper digest prefix does not match actual image"
    );
    let observation = LegacyOriginObservation {
        schema: 1,
        provenance: "owned_legacy_helper_cache".into(),
        user: crate::local_ipc::current_user()?,
        config_path: locator.path.clone(),
        config_sha256: config.digest.clone(),
        server: locator.server.clone(),
        container_path: container_path(locator.format, locator.container_path.as_deref())?,
        selected_entry_sha256: entry_hash,
        global_storage: global.to_owned(),
        location_basis: "explicit_owned_editor_storage".into(),
        executable: command,
        executable_sha256: executable.digest.clone(),
        executable_identity: executable.identity.clone(),
        cache_version: version,
        platform,
        digest_prefix: prefix,
        artifact_approved: false,
        declaration,
    };
    let mut result = NativeLegacyOrigin {
        observation,
        config,
        executable,
    };
    result.revalidate()?;
    Ok(result)
}
fn same_directory(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}
fn editor_user_directory(global: &Path) -> Option<&Path> {
    if global.file_name()?.to_str()? != "globalStorage" {
        return None;
    }
    let parent = global.parent()?;
    if parent.file_name()?.to_str()? == "User" {
        return Some(parent);
    }
    let id = parent.file_name()?.to_str()?;
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return None;
    }
    let profiles = parent.parent()?;
    if profiles.file_name()?.to_str()? != "profiles" {
        return None;
    }
    let user = profiles.parent()?;
    (user.file_name()?.to_str()? == "User").then_some(user)
}
fn cache_name(name: &str) -> Result<(String, String, String)> {
    bounded(name, 256)?;
    let (head, prefix) = name
        .rsplit_once('-')
        .context("missing legacy digest prefix")?;
    ensure!(
        prefix.len() == 16
            && prefix
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
        "invalid legacy digest prefix"
    );
    let host = if cfg!(windows) {
        "win32"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };
    for arch in ["x64", "arm64"] {
        let platform = format!("{host}-{arch}");
        if let Some(version) = head.strip_suffix(&format!("-{platform}")) {
            let core = version.split(['-', '+']).next().unwrap_or("");
            let numbers: Vec<_> = core.split('.').collect();
            ensure!(
                numbers.len() == 3
                    && numbers
                        .iter()
                        .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                    && version
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b)),
                "invalid cache version"
            );
            return Ok((version.into(), platform, prefix.into()));
        }
    }
    bail!("legacy cache platform does not match this native OS")
}
fn require_obsolete_cache_version(version: &str) -> Result<()> {
    fn numbers(version: &str) -> Result<Vec<u64>> {
        Ok(version
            .split(['-', '+'])
            .next()
            .context("missing cache version")?
            .split('.')
            .map(str::parse)
            .collect::<std::result::Result<Vec<u64>, _>>()?)
    }
    let version = numbers(version)?;
    let selected = numbers(env!("CARGO_PKG_VERSION"))?;
    ensure!(
        version.len() == 3 && version.as_slice() >= [0, 2, 1].as_slice() && version < selected,
        "current, future or unknown helper cache version preserved for review"
    );
    Ok(())
}
fn bounded(s: &str, max: usize) -> Result<()> {
    ensure!(
        !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control),
        "invalid or oversized legacy declaration text"
    );
    Ok(())
}
fn absolute(path: &Path) -> Result<()> {
    bounded(path.to_str().context("legacy path is not UTF-8")?, 4096)?;
    ensure!(
        path.is_absolute()
            && !path
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::CurDir)),
        "legacy path must be absolute without traversal"
    );
    Ok(())
}
pub(crate) fn normalize_argv(argv: &[String]) -> Result<LegacyDeclaration> {
    ensure!(
        !argv.is_empty()
            && argv.len() <= 128
            && argv.iter().map(String::len).sum::<usize>() <= 65536,
        "legacy argv exceeds limits"
    );
    let mut result = LegacyDeclaration {
        role: String::new(),
        root: None,
        data_dir: None,
        collection: None,
        index_collection: None,
        additional_roots: vec![],
        no_default_project: false,
        service_install_dir: None,
        service_runtime_dir: None,
        service_project: None,
        argv: argv.to_vec(),
    };
    let mut seen = BTreeSet::new();
    let mut aliases = BTreeSet::new();
    let mut i = 0;
    while i < argv.len() {
        let arg = &argv[i];
        bounded(arg, 4096)?;
        if ["mcp", "bridge", "launch"].contains(&arg.as_str()) {
            ensure!(result.role.is_empty(), "multiple legacy command roles");
            result.role = arg.clone();
            i += 1;
            continue;
        }
        if arg == "--no-default-project" {
            ensure!(seen.insert(arg.clone()), "duplicate legacy selector");
            result.no_default_project = true;
            i += 1;
            continue;
        }
        ensure!(
            [
                "--root",
                "--data-dir",
                "--collection",
                "--index-collection",
                "--additional-root",
                "--service-install-dir",
                "--service-runtime-dir",
                "--service-project"
            ]
            .contains(&arg.as_str()),
            "unsupported legacy selector; declaration must remain unchanged"
        );
        ensure!(
            arg == "--additional-root" || seen.insert(arg.clone()),
            "duplicate legacy selector"
        );
        i += 1;
        let value = argv.get(i).context("missing legacy selector value")?;
        bounded(value, 4096)?;
        match arg.as_str() {
            "--root" | "--data-dir" | "--service-install-dir" | "--service-runtime-dir" => {
                let p = PathBuf::from(value);
                absolute(&p)?;
                match arg.as_str() {
                    "--root" => result.root = Some(p),
                    "--data-dir" => result.data_dir = Some(p),
                    "--service-install-dir" => result.service_install_dir = Some(p),
                    _ => result.service_runtime_dir = Some(p),
                }
            }
            "--collection" => result.collection = Some(value.clone()),
            "--index-collection" => result.index_collection = Some(value.clone()),
            "--service-project" => result.service_project = Some(value.clone()),
            "--additional-root" => {
                let (alias, path) = value
                    .split_once('=')
                    .context("legacy additional root must retain alias=absolute-path")?;
                bounded(alias, 128)?;
                ensure!(
                    aliases.insert(alias.to_owned()),
                    "duplicate legacy root alias"
                );
                let path = PathBuf::from(path);
                absolute(&path)?;
                result.additional_roots.push(LegacyAdditionalRoot {
                    alias: alias.into(),
                    path,
                });
            }
            _ => unreachable!(),
        }
        i += 1;
    }
    ensure!(!result.role.is_empty(), "missing legacy MCP command role");
    Ok(result)
}

fn declaration_from_config(
    bytes: &[u8],
    format: ConfigFormat,
    server: &str,
) -> Result<(String, Vec<String>, String)> {
    declaration_from_config_at(bytes, format, server, None)
}
fn container_path(format: ConfigFormat, requested: Option<&[String]>) -> Result<Vec<String>> {
    let family = match format {
        ConfigFormat::CodexToml => "mcp_servers",
        ConfigFormat::McpServersJson => "mcpServers",
        ConfigFormat::VscodeServersJson => "servers",
        ConfigFormat::OpenCodeJson => "mcp",
    };
    let path = requested
        .map(<[String]>::to_vec)
        .unwrap_or_else(|| vec![family.into()]);
    ensure!(
        path.len() <= 3 && !path.is_empty(),
        "invalid original MCP container path"
    );
    for key in &path {
        bounded(key, 4096)?;
    }
    ensure!(
        path.len() == 1 && path[0] == family
            || matches!(format, ConfigFormat::McpServersJson)
                && path.len() == 3
                && path[0] == "projects"
                && path[2] == "mcpServers",
        "unsupported original MCP container path"
    );
    Ok(path)
}
pub(crate) fn matches_locator(
    proof: &LegacyOriginObservation,
    locator: &ConfigLocator,
) -> Result<bool> {
    Ok(proof.config_path == locator.path
        && proof.server == locator.server
        && proof.container_path
            == container_path(locator.format, locator.container_path.as_deref())?)
}
fn declaration_from_config_at(
    bytes: &[u8],
    format: ConfigFormat,
    server: &str,
    requested: Option<&[String]>,
) -> Result<(String, Vec<String>, String)> {
    let path = container_path(format, requested)?;
    let entry = if matches!(format, ConfigFormat::CodexToml) {
        codex_entry(std::str::from_utf8(bytes)?, server)?
    } else {
        // serde's Value parser accepts duplicate keys; explicitly reject those before selection.
        let value: UniqueJson =
            serde_json::from_slice(bytes).context("invalid original MCP configuration")?;
        let mut selected = &value.0;
        for key in &path {
            selected = selected
                .get(key)
                .context("original MCP container is absent")?;
        }
        selected
            .get(server)
            .context("selected original MCP server is absent")?
            .clone()
    };
    let digest = hex(&serde_json::to_vec(&entry)?);
    let object = entry
        .as_object()
        .context("original server is not an object")?;
    ensure!(
        !object.contains_key("url"),
        "HTTP declaration is not a legacy stdio origin"
    );
    if matches!(format, ConfigFormat::OpenCodeJson) {
        ensure!(
            object.get("type").and_then(|v| v.as_str()) == Some("local"),
            "original server is not local stdio"
        );
        let args: Vec<String> = serde_json::from_value(
            object
                .get("command")
                .context("missing original command")?
                .clone(),
        )?;
        ensure!(!args.is_empty(), "empty original command");
        return Ok((args[0].clone(), args[1..].to_vec(), digest));
    }
    if let Some(kind) = object.get("type") {
        ensure!(
            kind.as_str() == Some("stdio"),
            "original server is not stdio"
        );
    }
    let command = object
        .get("command")
        .and_then(|v| v.as_str())
        .context("missing original executable")?
        .to_owned();
    let args: Vec<String> = serde_json::from_value(
        object
            .get("args")
            .context("missing original arguments")?
            .clone(),
    )?;
    Ok((command, args, digest))
}
/// Parse the whole original TOML document structurally. Table-looking text in
/// literal/basic multiline strings is data, never a server declaration.
fn codex_entry(text: &str, server: &str) -> Result<serde_json::Value> {
    let document: toml::Value = toml::from_str(text)
        .map_err(|_| anyhow::anyhow!("invalid original Codex TOML configuration"))?;
    let entry = document
        .get("mcp_servers")
        .and_then(toml::Value::as_table)
        .and_then(|servers| servers.get(server))
        .and_then(toml::Value::as_table)
        .context("original Codex server is absent or not a table")?;
    // Include every selected subtable/policy field in the observation digest;
    // the original complete file hash separately binds unrelated settings.
    Ok(serde_json::to_value(entry)?)
}
struct UniqueJson(serde_json::Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON without duplicate object keys")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Self::Value, M::Error> {
                let mut out = serde_json::Map::new();
                while let Some((k, v)) = map.next_entry::<String, UniqueJson>()? {
                    if out.insert(k, v.0).is_some() {
                        return Err(serde::de::Error::custom(
                            "duplicate original configuration key",
                        ));
                    }
                }
                Ok(UniqueJson(out.into()))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut out = vec![];
                while let Some(v) = seq.next_element::<UniqueJson>()? {
                    out.push(v.0);
                }
                Ok(UniqueJson(out.into()))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                v: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                v: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                v: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(
                    serde_json::Number::from_f64(v)
                        .ok_or_else(|| E::custom("non-finite configuration number"))?
                        .into(),
                ))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::Null))
            }
        }
        d.deserialize_any(Visitor)
    }
}
fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

struct CapturedFile {
    path: PathBuf,
    file: File,
    identity: String,
    digest: String,
    limit: u64,
    bytes: u64,
    modified: std::time::SystemTime,
    ancestors: Vec<(PathBuf, File, String)>,
}
impl CapturedFile {
    fn open(path: &Path, limit: u64) -> Result<Self> {
        Self::open_inner(path, limit, None)
    }
    fn open_inner(path: &Path, limit: u64, expected_digest: Option<&str>) -> Result<Self> {
        absolute(path)?;
        let mut ancestors = vec![];
        for dir in path.ancestors().skip(1) {
            let m = fs::symlink_metadata(dir)?;
            ensure!(
                m.is_dir() && !m.file_type().is_symlink(),
                "redirected legacy ancestor"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    m.file_attributes() & 0x400 == 0,
                    "redirected legacy ancestor"
                );
            }
            let held = open_directory(dir)?;
            ancestors.push((
                dir.to_owned(),
                held.try_clone()?,
                directory_identity(&held)?,
            ));
        }
        crate::install_files::safe_file(path)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(
                windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
            );
        }
        let file = options.open(path)?;
        let identity = file_identity(&file)?;
        let metadata = file.metadata()?;
        ensure!(
            file.metadata()?.len() <= limit,
            "legacy file exceeds bounded capture"
        );
        let mut out = Self {
            path: path.into(),
            file,
            identity,
            digest: String::new(),
            limit,
            bytes: metadata.len(),
            modified: metadata.modified()?,
            ancestors,
        };
        out.digest = if let Some(expected) = expected_digest {
            expected.into()
        } else {
            out.hash()?
        };
        out.revalidate_identity()?;
        Ok(out)
    }
    fn bytes(&mut self, limit: u64) -> Result<Vec<u8>> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut bytes = vec![];
        (&mut self.file).take(limit + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= limit,
            "legacy file grew beyond capture limit"
        );
        Ok(bytes)
    }
    fn hash(&mut self) -> Result<String> {
        self.file.seek(SeekFrom::Start(0))?;
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut bytes = [0u8; 65536];
        loop {
            let n = self.file.read(&mut bytes)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            ensure!(count <= self.limit, "legacy file grew beyond capture limit");
            hash.update(&bytes[..n]);
        }
        Ok(format!("{:x}", hash.finalize()))
    }
    fn revalidate(&mut self) -> Result<()> {
        self.revalidate_identity()?;
        ensure!(self.hash()? == self.digest, "legacy file bytes changed");
        Ok(())
    }
    fn revalidate_identity(&self) -> Result<()> {
        for (dir, held, identity) in &self.ancestors {
            let m = fs::symlink_metadata(dir)?;
            ensure!(
                m.is_dir()
                    && !m.file_type().is_symlink()
                    && directory_identity(held)? == *identity
                    && directory_identity(&open_directory(dir)?)? == *identity,
                "legacy ancestor identity changed"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    m.file_attributes() & 0x400 == 0,
                    "redirected legacy ancestor"
                );
            }
        }
        crate::install_files::safe_file(&self.path)?;
        let current = File::open(&self.path)?;
        ensure!(
            file_identity(&current)? == self.identity
                && file_identity(&self.file)? == self.identity,
            "legacy file identity changed"
        );
        for file in [&current, &self.file] {
            let metadata = file.metadata()?;
            ensure!(
                metadata.len() == self.bytes && metadata.modified()? == self.modified,
                "legacy file size or change metadata changed"
            );
        }
        Ok(())
    }
}
#[cfg(unix)]
fn directory_identity(file: &File) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    let m = file.metadata()?;
    ensure!(m.is_dir(), "legacy ancestor is not a directory");
    // Root-owned system ancestors and sticky /tmp are not user-owned cache
    // evidence. Their exact opened identities are retained, never mutated.
    ensure!(
        m.mode() & 0o022 == 0 || m.mode() & 0o1000 != 0,
        "legacy ancestor is writable by another user"
    );
    Ok(format!("{}:{}", m.dev(), m.ino()))
}
#[cfg(windows)]
fn directory_identity(file: &File) -> Result<String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: retained directory handle and initialized fixed output buffer.
    ensure!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0,
        "cannot capture legacy ancestor identity"
    );
    ensure!(
        info.dwFileAttributes & 0x10 != 0 && info.dwFileAttributes & 0x400 == 0,
        "redirected legacy ancestor"
    );
    Ok(format!(
        "{}:{}:{}",
        info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
    ))
}
fn open_directory(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        options.custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    }
    Ok(options.open(path)?)
}
#[cfg(unix)]
fn require_owned_directory(file: &File) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let m = file.metadata()?;
    ensure!(
        m.is_dir()
            && m.uid().to_string() == crate::local_ipc::current_user()?
            && m.mode() & 0o022 == 0,
        "editor helper directory is foreign-owned or writable by another user"
    );
    directory_identity(file)?;
    Ok(())
}
#[cfg(windows)]
fn require_owned_directory(file: &File) -> Result<()> {
    // The same native handle owner check as files, with the directory bit
    // independently checked and aliases/reparse points refused.
    directory_identity(file)?;
    file_identity(file)?;
    Ok(())
}
#[cfg(unix)]
fn file_identity(file: &File) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    let m = file.metadata()?;
    ensure!(
        m.is_file()
            && m.nlink() == 1
            && m.uid().to_string() == crate::local_ipc::current_user()?
            && m.mode() & 0o022 == 0,
        "legacy file is aliased, foreign-owned or writable by another user"
    );
    Ok(format!("{}:{}", m.dev(), m.ino()))
}
#[cfg(windows)]
fn file_identity(file: &File) -> Result<String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{ConvertSidToStringSidW, GetSecurityInfo, SE_FILE_OBJECT},
            OWNER_SECURITY_INFORMATION,
        },
        Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
    };
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: live read handle and initialized fixed-size output storage.
    ensure!(
        unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0,
        "cannot capture legacy image identity"
    );
    ensure!(
        info.nNumberOfLinks == 1 && info.dwFileAttributes & 0x400 == 0,
        "aliased or redirected legacy file"
    );
    let (mut security, mut owner) = (std::ptr::null_mut(), std::ptr::null_mut());
    // SAFETY: live handle and writable outputs; successful descriptor must be LocalFree'd.
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut security,
        )
    };
    ensure!(status == 0, "cannot authenticate legacy file owner");
    struct Allocation(*mut std::ffi::c_void);
    impl Drop for Allocation {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    let _security = Allocation(security);
    ensure!(!owner.is_null(), "missing legacy file owner");
    let mut sid = std::ptr::null_mut();
    // SAFETY: owner points inside retained native descriptor; writable SID text output.
    ensure!(
        unsafe { ConvertSidToStringSidW(owner, &mut sid) } != 0,
        "invalid legacy owner SID"
    );
    let _sid = Allocation(sid.cast());
    let mut len = 0;
    unsafe {
        while len < 256 && *sid.add(len) != 0 {
            len += 1;
        }
    }
    ensure!(len < 256, "legacy SID exceeds bound");
    let user = String::from_utf16(unsafe { std::slice::from_raw_parts(sid, len) })?;
    ensure!(
        user == crate::local_ipc::current_user()?,
        "foreign legacy file owner"
    );
    Ok(format!(
        "{}:{}:{}",
        info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
    ))
}

#[cfg(test)]
mod tests;
