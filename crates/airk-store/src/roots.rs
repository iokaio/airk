// SPDX-License-Identifier: Apache-2.0
//! Explicit source aliases. Configuration never changes the roots of a published revision.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::Path,
};

const MAX_ROOTS: usize = 16;
const MAX_CONFIG_BYTES: u64 = 96 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootConfig {
    schema: u32,
    owner_root: String,
    pub roots: BTreeMap<String, String>,
}

impl RootConfig {
    pub fn load(directory: &Path, owner_root: &str) -> Result<Self> {
        let path = directory.join("roots.json");
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self {
                    schema: 1,
                    owner_root: owner_root.into(),
                    roots: BTreeMap::new(),
                });
            }
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "refusing linked or non-file source configuration"
        );
        ensure!(
            metadata.len() <= MAX_CONFIG_BYTES,
            "source configuration exceeds 96 KiB"
        );
        let mut bytes = Vec::new();
        fs::File::open(&path)?
            .take(MAX_CONFIG_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_CONFIG_BYTES,
            "source configuration exceeds 96 KiB"
        );
        let config: Self =
            serde_json::from_slice(&bytes).context("invalid source configuration")?;
        ensure!(
            config.schema == 1 && config.owner_root == owner_root,
            "source configuration belongs to another root or schema"
        );
        validate_map(&config.roots)?;
        Ok(config)
    }

    /// Caller holds the collection writer lock. Source directories are never written.
    pub fn save(&self, directory: &Path) -> Result<()> {
        validate_map(&self.roots)?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() as u64 <= MAX_CONFIG_BYTES,
            "source configuration exceeds 96 KiB"
        );
        let path = directory.join("roots.json");
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "refusing linked source configuration"
            );
        }
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist(&path)
            .context("publish source configuration")?;
        Ok(())
    }

    pub fn add(&mut self, alias: &str, path: &Path, owner: &Path, data: &Path) -> Result<()> {
        validate_alias(alias)?;
        ensure!(
            !self.roots.contains_key(alias),
            "source alias already exists; remove it explicitly before rebinding"
        );
        let canonical = path
            .canonicalize()
            .context("resolve additional source root")?;
        let canonical = canonical
            .to_str()
            .context("non-UTF-8 source root is unsupported")?
            .to_owned();
        let mut roots = self.roots.clone();
        roots.insert(alias.into(), canonical);
        validate_sources(owner, data, &roots)?;
        self.roots = roots;
        Ok(())
    }

    pub fn remove(&mut self, alias: &str) -> Result<()> {
        validate_alias(alias)?;
        ensure!(
            self.roots.remove(alias).is_some(),
            "source alias is not configured"
        );
        Ok(())
    }
}

pub fn validate_alias(alias: &str) -> Result<()> {
    ensure!(
        !alias.is_empty()
            && alias.len() <= 32
            && alias.bytes().all(|byte| byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || byte == b'-'
                || byte == b'_'),
        "source alias must be 1..32 lowercase ASCII letters/digits/hyphens/underscores"
    );
    ensure!(
        alias != "primary",
        "primary is reserved for the collection's original root"
    );
    Ok(())
}

fn validate_map(roots: &BTreeMap<String, String>) -> Result<()> {
    ensure!(
        roots.len() <= MAX_ROOTS,
        "a collection supports at most 16 additional source roots"
    );
    for (alias, root) in roots {
        validate_alias(alias)?;
        ensure!(
            root.len() <= 4096 && Path::new(root).is_absolute(),
            "source root must be an absolute path of at most 4096 bytes"
        );
    }
    Ok(())
}

/// Revalidate every scan: replacing a saved root with a link must not switch source identity.
pub fn validate_sources(owner: &Path, data: &Path, roots: &BTreeMap<String, String>) -> Result<()> {
    validate_map(roots)?;
    let owner = owner.canonicalize()?;
    let data = data.canonicalize()?;
    let mut accepted = vec![owner.clone()];
    for (alias, root) in roots {
        let path = Path::new(root);
        let resolved = path
            .canonicalize()
            .with_context(|| format!("resolve source alias {alias}"))?;
        ensure!(
            resolved == path && resolved.is_dir(),
            "source alias {alias} changed identity or is not a directory"
        );
        ensure!(
            !resolved.starts_with(&data),
            "source alias {alias} is inside index storage"
        );
        ensure!(
            !accepted
                .iter()
                .any(|other| resolved.starts_with(other) || other.starts_with(&resolved)),
            "source alias {alias} overlaps another collection source"
        );
        // Virtual paths are explicit and cannot collide with a primary-root file/directory.
        match fs::symlink_metadata(owner.join(format!("@{alias}"))) {
            Ok(_) => anyhow::bail!("primary source contains the reserved @{alias} namespace"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        accepted.push(resolved);
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct SourceLocation {
    pub alias: String,
    pub root: String,
    pub relative_path: String,
}

/// Resolve only through the pinned revision's alias map, never mutable configuration or live I/O.
pub fn source_location(
    owner: &str,
    roots: &BTreeMap<String, String>,
    path: &str,
) -> Result<SourceLocation> {
    ensure!(
        !path.is_empty() && path.len() <= 4096 && !path.contains('\\') && !path.starts_with('/'),
        "invalid indexed source path"
    );
    ensure!(
        path.split('/')
            .all(|part| !part.is_empty() && part != "." && part != ".."),
        "invalid indexed source path"
    );
    if let Some((prefix, relative)) = path.split_once('/')
        && let Some(alias) = prefix.strip_prefix('@')
        && let Some(root) = roots.get(alias)
    {
        return Ok(SourceLocation {
            alias: alias.into(),
            root: root.clone(),
            relative_path: relative.into(),
        });
    }
    Ok(SourceLocation {
        alias: "primary".into(),
        root: owner.into(),
        relative_path: path.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_are_explicit_owner_bound_and_pinned() -> Result<()> {
        let temp = tempfile::tempdir()?;
        for name in ["primary", "docs", "replacement", "data"] {
            fs::create_dir(temp.path().join(name))?;
        }
        let owner = temp.path().join("primary").canonicalize()?;
        let data = temp.path().join("data").canonicalize()?;
        let mut config = RootConfig::load(&data, owner.to_str().unwrap())?;
        config.add("requirements", &temp.path().join("docs"), &owner, &data)?;
        let pinned = config.roots.clone();
        config.save(&data)?;
        assert_eq!(
            RootConfig::load(&data, owner.to_str().unwrap())?.roots,
            pinned
        );
        assert!(RootConfig::load(&data, "another-owner").is_err());
        assert!(
            config
                .add(
                    "requirements",
                    &temp.path().join("replacement"),
                    &owner,
                    &data
                )
                .is_err()
        );
        config.remove("requirements")?;
        config.add(
            "requirements",
            &temp.path().join("replacement"),
            &owner,
            &data,
        )?;
        let evidence =
            source_location(owner.to_str().unwrap(), &pinned, "@requirements/spec.xlsx")?;
        assert_eq!(evidence.root, pinned["requirements"]);
        assert_eq!(evidence.relative_path, "spec.xlsx");
        assert_ne!(evidence.root, config.roots["requirements"]);
        assert!(
            source_location(owner.to_str().unwrap(), &pinned, "@requirements/../secret").is_err()
        );
        Ok(())
    }

    #[test]
    fn overlapping_sources_and_virtual_path_collisions_fail_closed() -> Result<()> {
        let temp = tempfile::tempdir()?;
        for name in ["primary", "other", "data"] {
            fs::create_dir(temp.path().join(name))?;
        }
        let owner = temp.path().join("primary").canonicalize()?;
        let data = temp.path().join("data").canonicalize()?;
        fs::create_dir(owner.join("nested"))?;
        let mut config = RootConfig::load(&data, owner.to_str().unwrap())?;
        assert!(
            config
                .add("nested", &owner.join("nested"), &owner, &data)
                .is_err()
        );
        assert!(config.add("state", &data, &owner, &data).is_err());
        assert!(
            config
                .add("primary", &temp.path().join("other"), &owner, &data)
                .is_err()
        );
        fs::write(owner.join("@docs"), "original source")?;
        assert!(
            config
                .add("docs", &temp.path().join("other"), &owner, &data)
                .is_err()
        );
        assert_eq!(fs::read_to_string(owner.join("@docs"))?, "original source");
        assert!(config.roots.is_empty());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn replacing_registered_root_with_link_is_rejected() -> Result<()> {
        let temp = tempfile::tempdir()?;
        for name in ["primary", "docs", "replacement", "data"] {
            fs::create_dir(temp.path().join(name))?;
        }
        let owner = temp.path().join("primary").canonicalize()?;
        let data = temp.path().join("data").canonicalize()?;
        let docs = temp.path().join("docs");
        let mut config = RootConfig::load(&data, owner.to_str().unwrap())?;
        config.add("docs", &docs, &owner, &data)?;
        fs::remove_dir(&docs)?;
        std::os::unix::fs::symlink(temp.path().join("replacement"), &docs)?;
        assert!(validate_sources(&owner, &data, &config.roots).is_err());
        Ok(())
    }
}
