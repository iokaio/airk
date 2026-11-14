// SPDX-License-Identifier: Apache-2.0
//! Process-lifetime ownership of a service runtime namespace; never delete the lock inode.
#[cfg(unix)]
use crate::store_fs::open_existing_ownership_file;
use crate::store_fs::{
    OwnershipFile, acquire_existing_file, acquire_existing_file_for, acquire_file,
    acquire_file_for, acquire_shared_file,
};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::atomic::Ordering,
};

pub mod data_scope;

pub mod arbiter;
pub mod arbiter_assets;
pub mod collection;
pub mod embedded_model;
#[cfg(test)]
mod inference_admission_tests;
pub mod model_files;
pub mod registry;
#[cfg(unix)]
#[path = "service_shared_helper_lease.rs"]
mod shared_helper_lease;
#[cfg(unix)]
pub use shared_helper_lease::{SharedHelperLease, shared_helper_pin_name};

pub struct ProjectLease {
    derived: bool,
    _file: OwnershipFile,
    root: PathBuf,
    binding: airk_core::service::ProjectBinding,
    epoch: Option<u64>,
}

impl ProjectLease {
    /// The Unix guardian retains this same open-file-description ownership pin.
    /// A raw File closes on drop without explicitly unlocking the shared lease.
    #[cfg(unix)]
    pub fn duplicate_native_pin(&self) -> Result<File> {
        let pin = self._file.0.try_clone()?;
        // flock belongs to the shared open-file description. Explicit unlock
        // would release the guardian's pin too; after pinning, only last close
        // may release this project lease.
        self._file.1.store(true, Ordering::Relaxed);
        Ok(pin)
    }
    pub fn acquire(root: &Path, binding: airk_core::service::ProjectBinding) -> Result<Self> {
        let root = root.canonicalize()?;
        let file = acquire_file(&root, &format!("worker-1.4-{binding}.lock"))?;
        Ok(Self {
            _file: file,
            root,
            binding,
            epoch: None,
            derived: false,
        })
    }
    pub(crate) fn duplicate_split_pin(&self) -> Result<File> {
        let pin = self._file.0.try_clone()?;
        self._file.1.store(true, Ordering::Relaxed);
        Ok(pin)
    }
    pub fn identity(&self) -> (airk_core::service::ProjectBinding, Option<u64>) {
        (self.binding, self.epoch)
    }
    /// Compare exact native custody without resolving a new path or implying an admitted epoch.
    pub(crate) fn matches_registration(
        &self,
        control: &Path,
        binding: airk_core::service::ProjectBinding,
    ) -> bool {
        self.binding == binding && self.root == control
    }
}

pub struct ServiceLease {
    _file: OwnershipFile,
    root: PathBuf,
    inference: std::sync::Mutex<InferenceAdmission>,
}
#[derive(Default)]
struct InferenceAdmission {
    factory_claimed: bool,
    ever_admitted: bool,
    admitted_owners: u64,
    sealed: bool,
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
    /// Exactly one native factory belongs to this actual lease object. Arc
    /// clones share the claim; a second factory can never mint another proof.
    pub fn claim_inference_factory(&self) -> Result<()> {
        let mut state = self
            .inference
            .lock()
            .map_err(|_| anyhow::anyhow!("native inference admission unavailable"))?;
        ensure!(
            !state.factory_claimed && !state.sealed,
            "native inference factory already claimed or sealed"
        );
        state.factory_claimed = true;
        Ok(())
    }
    /// The safe Kernel adapter records every compute scope before entering FFI.
    /// This monotonic fact is never reset by idle, failure, or model release.
    pub fn admit_inference_owner(&self) -> Result<()> {
        let mut state = self
            .inference
            .lock()
            .map_err(|_| anyhow::anyhow!("native inference admission unavailable"))?;
        ensure!(!state.sealed, "native inference admission sealed");
        state.admitted_owners = state
            .admitted_owners
            .checked_add(1)
            .context("native inference admission count overflow")?;
        state.ever_admitted = true;
        Ok(())
    }
    /// Close future compute scopes and report the monotonic admission count.
    /// This is not a completion witness: callers must separately prove every
    /// admitted owner actually joined its native computation and deliveries.
    pub fn seal_inference_admission(&self) -> Result<u64> {
        let mut state = self
            .inference
            .lock()
            .map_err(|_| anyhow::anyhow!("native inference admission unavailable"))?;
        ensure!(state.factory_claimed, "native inference factory is missing");
        state.sealed = true;
        Ok(state.admitted_owners)
    }
    pub fn seal_unused_inference(&self) -> Result<()> {
        let mut state = self
            .inference
            .lock()
            .map_err(|_| anyhow::anyhow!("native inference admission unavailable"))?;
        ensure!(
            state.factory_claimed && !state.ever_admitted,
            "native inference owner has been admitted or factory is missing"
        );
        state.sealed = true;
        Ok(())
    }
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
        Ok(Self {
            _file: file,
            root,
            inference: std::sync::Mutex::new(InferenceAdmission::default()),
        })
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

#[cfg(test)]
mod existing_owner_tests {
    use super::*;
    #[test]
    fn existing_owner_never_creates_or_repairs_missing_or_malformed_leaf() -> Result<()> {
        let root = tempfile::tempdir()?;
        assert!(acquire_existing_file(root.path(), "manifest.lock").is_err());
        assert_eq!(fs::read_dir(root.path())?.count(), 0);
        fs::write(root.path().join("manifest.lock"), b"legacy")?;
        assert!(acquire_existing_file(root.path(), "manifest.lock").is_err());
        assert_eq!(fs::read(root.path().join("manifest.lock"))?, b"legacy");
        fs::write(root.path().join("manifest.lock"), b"")?;
        let held = acquire_existing_file(root.path(), "manifest.lock")?;
        assert!(acquire_existing_file(root.path(), "manifest.lock").is_err());
        drop(held);
        drop(acquire_existing_file(root.path(), "manifest.lock")?);
        assert_eq!(fs::read_dir(root.path())?.count(), 1);
        assert_eq!(fs::metadata(root.path().join("manifest.lock"))?.len(), 0);
        Ok(())
    }
}
