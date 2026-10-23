// SPDX-License-Identifier: Apache-2.0
//! Process-lifetime ownership of a service runtime namespace; never delete the lock inode.
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

#[path = "service_collection.rs"]
pub mod collection;
#[path = "service_model_files.rs"]
pub mod model_files;
#[path = "service_registry.rs"]
pub mod registry;

struct OwnershipFile(File);
impl Drop for OwnershipFile {
    fn drop(&mut self) {
        // Release this owner's lock before close: a concurrent Unix fork may
        // briefly retain the open-file description until its CLOEXEC processing.
        let _ = self.0.unlock();
    }
}

fn acquire_file(root: &Path, name: &str) -> Result<OwnershipFile> {
    let path = root.join(name);
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() == 0,
            "invalid service ownership file"
        );
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW).mode(0o600);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000);
    }
    let file = options.open(&path)?;
    crate::managed_collection::reject_file_aliases(&path, &file.metadata()?)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            file.metadata()?.file_attributes() & 0x400 == 0,
            "redirected service ownership file"
        );
    }
    file.try_lock().with_context(|| {
        format!(
            "another AIRK process owns service resource: {}",
            path.display()
        )
    })?;
    Ok(OwnershipFile(file))
}

pub struct ProjectLease {
    _file: OwnershipFile,
    root: PathBuf,
    binding: airk_core::service::ProjectBinding,
    epoch: Option<u64>,
}

impl ProjectLease {
    pub fn acquire(root: &Path, binding: airk_core::service::ProjectBinding) -> Result<Self> {
        let root = root.canonicalize()?;
        let file = acquire_file(&root, &format!("worker-1.4-{binding}.lock"))?;
        Ok(Self {
            _file: file,
            root,
            binding,
            epoch: None,
        })
    }
    pub fn identity(&self) -> (airk_core::service::ProjectBinding, Option<u64>) {
        (self.binding, self.epoch)
    }
}

pub struct ServiceLease {
    _file: OwnershipFile,
    root: PathBuf,
}

/// Pooled inference can outlive its service/last project briefly while draining.
pub struct EmbeddingLease {
    _file: OwnershipFile,
}
/// Directory notifications live outside project owners and may drain after service exit.
pub struct SourceWatchLease {
    _file: OwnershipFile,
}
impl SourceWatchLease {
    pub fn acquire(root: &Path) -> Result<Self> {
        Ok(Self {
            _file: acquire_file(&root.canonicalize()?, "source-watch-1.4.lock")?,
        })
    }
}

impl EmbeddingLease {
    pub fn acquire(root: &Path) -> Result<Self> {
        Ok(Self {
            _file: acquire_file(&root.canonicalize()?, "embedding-1.4.lock")?,
        })
    }
}

impl ServiceLease {
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Native adapter must first establish an owner-only runtime directory.
    pub fn acquire(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        ensure!(
            fs::symlink_metadata(&root)?.is_dir(),
            "invalid service runtime directory"
        );
        let file = acquire_file(&root, "instance-1.4.lock")?;
        Ok(Self { _file: file, root })
    }

    /// This lease proves exclusion of compliant service owners, including after process exit.
    /// Reclaim only the fixed control endpoint, never a caller-supplied path.
    pub fn prepare_control_endpoint(&self) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileTypeExt;
            let socket = self.root.join("control.sock");
            match fs::symlink_metadata(&socket) {
                Ok(metadata) => {
                    ensure!(
                        metadata.file_type().is_socket(),
                        "unexpected service control entry"
                    );
                    fs::remove_file(socket)?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        #[cfg(windows)]
        {
            let _ = &self.root;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn service_and_manifest_ownership_release_before_inherited_descriptor_close() -> Result<()> {
        let owner = tempfile::tempdir()?;
        for name in ["instance-1.4.lock", "manifest.lock", "replacement.lock"] {
            let lease = acquire_file(owner.path(), name)?;
            // dup retains the same open-file description as fork, without
            // invoking non-async-safe Rust code in a threaded post-fork child.
            let inherited = lease.0.try_clone()?;
            assert!(acquire_file(owner.path(), name).is_err());
            drop(lease);
            let replacement = acquire_file(owner.path(), name)?;
            drop(inherited);
            assert!(acquire_file(owner.path(), name).is_err());
            drop(replacement);
        }
        Ok(())
    }

    #[test]
    fn aliased_service_ownership_files_are_preserved_and_refused() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let path = owner.path().join("manifest.lock");
        fs::write(&path, [])?;
        fs::hard_link(&path, owner.path().join("foreign-alias"))?;
        assert!(acquire_file(owner.path(), "manifest.lock").is_err());
        assert!(path.is_file() && owner.path().join("foreign-alias").is_file());
        Ok(())
    }
    #[test]
    fn ownership_lives_until_handle_release_and_never_removes_foreign_control_files() -> Result<()>
    {
        let owner = tempfile::tempdir()?;
        let lease = ServiceLease::acquire(owner.path())?;
        assert!(ServiceLease::acquire(owner.path()).is_err());
        drop(lease);
        let lease = ServiceLease::acquire(owner.path())?;
        fs::write(owner.path().join("control.sock"), b"original user fixture")?;
        #[cfg(unix)]
        assert!(lease.prepare_control_endpoint().is_err());
        assert_eq!(
            fs::read(owner.path().join("control.sock"))?,
            b"original user fixture"
        );
        drop(lease);
        Ok(())
    }
}
