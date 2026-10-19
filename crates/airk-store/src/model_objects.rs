// SPDX-License-Identifier: Apache-2.0
//! Immutable collection-local model assets. Mutations require the collection writer.
use airk_core::{
    identity::Sha256Digest,
    model_objects::{ModelObjectRef, ModelObjectSet},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default)]
pub struct RetainedModelObjects {
    pub references: Vec<ModelObjectRef>,
    pub new_bytes: u64,
    pub reused_bytes: u64,
}

pub struct ModelObjects {
    collection: PathBuf,
}
impl ModelObjects {
    pub fn new(collection_directory: &Path) -> Self {
        Self {
            collection: collection_directory.into(),
        }
    }

    fn path(&self, digest: Sha256Digest) -> Result<PathBuf> {
        directory_ancestors(&self.collection, true)?;
        let objects = self.collection.join("objects");
        let models = objects.join("models");
        let path = models.join(digest.to_string());
        for component in [&self.collection, &objects, &models, &path] {
            match fs::symlink_metadata(component) {
                Ok(metadata) => ensure!(
                    !metadata.file_type().is_symlink()
                        && if component == &path {
                            metadata.is_file()
                        } else {
                            metadata.is_dir()
                        },
                    "invalid model object path"
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(path)
    }

    /// Verify every unique receipt-authenticated reference, charging the caller's
    /// read budget before reading. History-only ownership intentionally excludes these.
    pub fn verify(
        &self,
        set: &ModelObjectSet,
        maximum_read_bytes: u64,
    ) -> Result<Vec<ModelObjectRef>> {
        let references = unique(set)?;
        let mut remaining = maximum_read_bytes;
        for reference in &references {
            remaining = remaining
                .checked_sub(reference.bytes)
                .context("model object verification read budget exceeded")?;
            verify_file(&self.path(reference.sha256)?, reference)?;
        }
        Ok(references)
    }

    /// Copies fixed user-provisioned assets only; never resolves paths from model
    /// manifest strings. Existing verified objects incur no new allocation charge.
    pub fn retain(
        &self,
        source_directory: &Path,
        set: &ModelObjectSet,
        reservation: &crate::quota::Reservation,
    ) -> Result<RetainedModelObjects> {
        let staging = reservation.staging_directory()?;
        self.retain_at(
            source_directory,
            set,
            &|bytes| reservation.charge(bytes),
            Some(&staging),
        )
    }

    #[cfg(test)]
    fn retain_charged(
        &self,
        source_directory: &Path,
        set: &ModelObjectSet,
        charge: &dyn Fn(u64) -> Result<()>,
    ) -> Result<RetainedModelObjects> {
        self.retain_at(source_directory, set, charge, None)
    }

    fn retain_at(
        &self,
        source_directory: &Path,
        set: &ModelObjectSet,
        charge: &dyn Fn(u64) -> Result<()>,
        staging: Option<&Path>,
    ) -> Result<RetainedModelObjects> {
        let references = unique(set)?;
        let source_meta = fs::symlink_metadata(source_directory)?;
        ensure!(
            source_meta.is_dir() && !source_meta.file_type().is_symlink(),
            "invalid provisioned model directory"
        );
        directory_ancestors(source_directory, false)?;
        let mut result = RetainedModelObjects {
            references,
            ..Default::default()
        };
        for reference in &result.references {
            let target = self.path(reference.sha256)?;
            if target.exists() {
                verify_file(&target, reference)?;
                result.reused_bytes += reference.bytes;
                continue;
            }
            let name = set
                .assets
                .iter()
                .find(|(_, value)| *value == reference)
                .context("model asset missing")?
                .0;
            let source = source_directory.join(name.filename());
            let metadata = fs::symlink_metadata(&source)?;
            ensure!(
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() == reference.bytes,
                "provisioned model asset changed type or length"
            );
            let mut input = File::open(&source)?;
            ensure!(
                input.metadata()?.len() == reference.bytes,
                "provisioned model asset changed before copy"
            );
            // Reserve all temporary bytes before creating or writing the object.
            charge(reference.bytes)?;
            let directory = target.parent().context("model object directory missing")?;
            fs::create_dir_all(directory)?;
            self.path(reference.sha256)?;
            // Public writes use reservation-owned staging so interrupted copies
            // are recoverable without deleting unknown files in the object pool.
            let mut temporary = tempfile::NamedTempFile::new_in(staging.unwrap_or(directory))?;
            let mut hash = Sha256::new();
            let mut bytes = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            while bytes < reference.bytes {
                let length = usize::try_from((reference.bytes - bytes).min(buffer.len() as u64))?;
                let count = input.read(&mut buffer[..length])?;
                ensure!(count > 0, "provisioned model asset shortened during copy");
                temporary.write_all(&buffer[..count])?;
                hash.update(&buffer[..count]);
                bytes += count as u64;
            }
            ensure!(
                input.metadata()?.len() == bytes
                    && Sha256Digest::from_bytes(hash.finalize().into()) == reference.sha256,
                "provisioned model asset changed during copy"
            );
            temporary.as_file().sync_all()?;
            // No overwrite: a conflicting concurrent writer is an explicit error.
            temporary
                .persist_noclobber(&target)
                .map_err(|error| error.error)?;
            #[cfg(unix)]
            File::open(directory)?.sync_all()?;
            result.new_bytes += bytes;
        }
        Ok(result)
    }
}

/// Install verified input assets once, under native quota/lifecycle ownership.
/// Source is a private download directory outside managed state. Existing input
/// models are never replaced; collection publication retains its own copies.
pub fn install_input_model(
    source: &Path,
    set: &ModelObjectSet,
    quota: &crate::quota::Quota,
    collection: &str,
) -> Result<()> {
    set.validate()?;
    directory_ancestors(source, false)?;
    let parent = quota.root().join("models");
    directory_ancestors(&parent, true)?;
    let bytes = set.assets.values().map(|reference| reference.bytes).sum();
    let reservation = quota.reserve(collection, bytes)?;
    fs::create_dir_all(&parent)?;
    directory_ancestors(&parent, false)?;
    let lock_path = parent.join("install.lock");
    if let Ok(metadata) = fs::symlink_metadata(&lock_path) {
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "invalid model install lock"
        );
    }
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)?;
    lock.try_lock()
        .context("another AIRK process is installing model assets; retry indexing")?;
    let destination = parent.join("local");
    ensure!(
        !destination.try_exists()?,
        "local model already exists; preserving existing assets"
    );
    let staging = tempfile::tempdir_in(reservation.staging_directory()?)?;
    for (name, reference) in &set.assets {
        let input_path = source.join(name.filename());
        verify_file(&input_path, reference)?;
        reservation.charge(reference.bytes)?;
        let target = staging.path().join(name.filename());
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        let mut input = File::open(input_path)?.take(reference.bytes);
        ensure!(
            std::io::copy(&mut input, &mut output)? == reference.bytes,
            "model input shortened during install"
        );
        output.sync_all()?;
        verify_file(&target, reference)?;
    }
    #[cfg(unix)]
    File::open(staging.path())?.sync_all()?;
    fs::rename(staging.path(), &destination)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn directory_ancestors(path: &Path, missing_allowed: bool) -> Result<()> {
    for ancestor in path.ancestors().filter(|path| !path.as_os_str().is_empty()) {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "linked or non-directory model ancestor"
            ),
            Err(error) if missing_allowed && error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn unique(set: &ModelObjectSet) -> Result<Vec<ModelObjectRef>> {
    set.validate()?;
    Ok(set
        .assets
        .values()
        .map(|value| (value.sha256, value.clone()))
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect())
}
fn verify_file(path: &Path, reference: &ModelObjectRef) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() == reference.bytes,
        "model object type or length mismatch"
    );
    let mut input = File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    while bytes < reference.bytes {
        let count = input.read(&mut buffer)?;
        ensure!(count > 0, "model object shortened during verification");
        bytes += count as u64;
        ensure!(
            bytes <= reference.bytes,
            "model object grew during verification"
        );
        hash.update(&buffer[..count]);
    }
    ensure!(
        input.metadata()?.len() == bytes
            && Sha256Digest::from_bytes(hash.finalize().into()) == reference.sha256,
        "model object checksum mismatch"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::model_objects::ModelAssetName;
    fn unlinked_tempdir() -> Result<tempfile::TempDir> {
        // The model source and target guards reject macOS's linked /var path.
        Ok(tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?)?)
    }
    fn fixture(source: &Path) -> Result<ModelObjectSet> {
        fs::create_dir_all(source)?;
        let mut assets = BTreeMap::new();
        for (name, bytes) in [
            (ModelAssetName::ModelJson, b"original metadata".as_slice()),
            (
                ModelAssetName::VocabularyJson,
                b"original locally provided weights".as_slice(),
            ),
        ] {
            fs::write(source.join(name.filename()), bytes)?;
            assets.insert(
                name,
                ModelObjectRef {
                    sha256: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
                    bytes: bytes.len() as u64,
                },
            );
        }
        Ok(ModelObjectSet {
            version: 1,
            pipeline_digest: Sha256Digest::from_bytes([1; 32]),
            legacy_identity: Sha256Digest::from_bytes([2; 32]).to_string(),
            assets,
        })
    }
    #[test]
    fn reuse_is_charged_once_and_tampering_is_explicit() -> Result<()> {
        let root = unlinked_tempdir()?;
        let source = root.path().join("source");
        let set = fixture(&source)?;
        let collection = root.path().join("collection");
        fs::create_dir(&collection)?;
        let objects = ModelObjects::new(&collection);
        let first = objects.retain_charged(&source, &set, &|_| Ok(()))?;
        assert!(first.new_bytes > 0);
        let second =
            objects.retain_charged(&source, &set, &|_| anyhow::bail!("unexpected allocation"))?;
        assert_eq!(second.new_bytes, 0);
        assert_eq!(second.reused_bytes, first.new_bytes);
        objects.verify(&set, first.new_bytes)?;
        assert!(objects.verify(&set, first.new_bytes - 1).is_err());
        let reference = &first.references[0];
        fs::write(
            objects.path(reference.sha256)?,
            vec![b'x'; reference.bytes as usize],
        )?;
        assert!(objects.retain_charged(&source, &set, &|_| Ok(())).is_err());
        Ok(())
    }
    #[test]
    fn reservation_failure_and_source_change_publish_no_asset() -> Result<()> {
        let root = unlinked_tempdir()?;
        let source = root.path().join("source");
        let set = fixture(&source)?;
        let objects = ModelObjects::new(&root.path().join("collection"));
        assert!(
            objects
                .retain_charged(&source, &set, &|_| anyhow::bail!("quota exhausted"))
                .is_err()
        );
        assert!(!root.path().join("collection/objects/models").exists());
        let reference = &set.assets[&ModelAssetName::ModelJson];
        fs::write(
            source.join("model.json"),
            vec![b'x'; reference.bytes as usize],
        )?;
        assert!(objects.retain_charged(&source, &set, &|_| Ok(())).is_err());
        assert!(!objects.path(reference.sha256)?.exists());
        Ok(())
    }

    #[test]
    fn metadata_change_reuses_identical_weight_bytes_and_keeps_old_objects() -> Result<()> {
        let root = unlinked_tempdir()?;
        let source = root.path().join("source");
        let mut set = fixture(&source)?;
        let objects = ModelObjects::new(&root.path().join("collection"));
        let first = objects.retain_charged(&source, &set, &|_| Ok(()))?;
        let old_metadata = set.assets[&ModelAssetName::ModelJson].clone();
        let weights = set.assets[&ModelAssetName::VocabularyJson].clone();
        let bytes = b"changed original metadata";
        fs::write(source.join("model.json"), bytes)?;
        set.pipeline_digest = Sha256Digest::from_bytes([9; 32]);
        set.assets.insert(
            ModelAssetName::ModelJson,
            ModelObjectRef {
                sha256: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
                bytes: bytes.len() as u64,
            },
        );
        let second = objects.retain_charged(&source, &set, &|_| Ok(()))?;
        assert_eq!(second.new_bytes, bytes.len() as u64);
        assert_eq!(second.reused_bytes, weights.bytes);
        assert!(objects.path(old_metadata.sha256)?.exists());
        assert_eq!(
            fs::metadata(objects.path(weights.sha256)?)?.len(),
            weights.bytes
        );
        assert!(first.new_bytes > second.reused_bytes);
        Ok(())
    }

    #[test]
    fn source_change_after_open_fails_checksum_before_immutable_install() -> Result<()> {
        let root = unlinked_tempdir()?;
        let source = root.path().join("source");
        let set = fixture(&source)?;
        let objects = ModelObjects::new(&root.path().join("collection"));
        let first = unique(&set)?.remove(0);
        let name = set
            .assets
            .iter()
            .find(|(_, value)| *value == &first)
            .unwrap()
            .0;
        let changed = std::cell::Cell::new(false);
        let result = objects.retain_charged(&source, &set, &|_| {
            if !changed.replace(true) {
                fs::write(
                    source.join(name.filename()),
                    vec![b'x'; first.bytes as usize],
                )?;
            }
            Ok(())
        });
        assert!(result.is_err());
        assert!(!objects.path(first.sha256)?.exists());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn linked_source_parent_and_object_parent_are_rejected() -> Result<()> {
        use std::os::unix::fs::symlink;
        let root = unlinked_tempdir()?;
        let source = root.path().join("real/source");
        let set = fixture(&source)?;
        symlink(root.path().join("real"), root.path().join("linked"))?;
        let objects = ModelObjects::new(&root.path().join("collection"));
        assert!(
            objects
                .retain_charged(&root.path().join("linked/source"), &set, &|_| Ok(()))
                .is_err()
        );
        fs::create_dir(root.path().join("collection"))?;
        fs::create_dir(root.path().join("outside"))?;
        symlink(
            root.path().join("outside"),
            root.path().join("collection/objects"),
        )?;
        assert!(objects.retain_charged(&source, &set, &|_| Ok(())).is_err());
        assert_eq!(fs::read_dir(root.path().join("outside"))?.count(), 0);
        Ok(())
    }
}
