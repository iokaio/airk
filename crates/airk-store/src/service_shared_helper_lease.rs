// SPDX-License-Identifier: Apache-2.0
//! Exact shared-helper native pin. A pin observation alone is never a provider
//! completion proof; the native adapter combines it with the durable role/phase.
#![cfg(unix)]
use super::*;
use registry::SharedHelperIdentity;
use std::os::unix::fs::MetadataExt;

pub fn shared_helper_pin_name(owner: SharedHelperIdentity) -> String {
    format!("shared-helper-1.9-{}-{}.lock", owner.service, owner.helper)
}

/// Terminal provider-row GC intentionally retains these zero-byte lock files.
/// Names are never reused. The 128-row provider bound does not imply that all
/// control-directory entries are bounded; exact retired-inode cleanup awaits
/// separate unlink/race qualification and must preserve unknown replacements.
pub struct SharedHelperLease {
    file: OwnershipFile,
}
impl SharedHelperLease {
    pub fn acquire(root: &Path, owner: SharedHelperIdentity) -> Result<Self> {
        let file = acquire_file(&root.canonicalize()?, &shared_helper_pin_name(owner))?;
        valid(&file.0)?;
        Ok(Self { file })
    }
    pub fn identity(&self) -> Result<(u64, u64)> {
        let metadata = self.file.0.metadata()?;
        Ok((metadata.dev(), metadata.ino()))
    }
    /// Descriptor transfer only; this does not acquire a producer pin or prove
    /// release. The receiver compares its saved inode with its inherited pin.
    pub fn open_pin(root: &Path, owner: SharedHelperIdentity) -> Result<File> {
        let file = open_existing_ownership_file(root, &shared_helper_pin_name(owner))?;
        valid(&file)?;
        Ok(file)
    }
    pub fn duplicate_native_pin(&self) -> Result<File> {
        let file = self.file.0.try_clone()?;
        // Last close, not an explicit unlock by the service, releases a pin
        // already inherited by a native guardian or guarded helper child.
        self.file.1.store(true, Ordering::Release);
        Ok(file)
    }
    /// Existing-only observation; missing/replaced/foreign entries are errors.
    pub fn released(
        root: &Path,
        owner: SharedHelperIdentity,
        device: u64,
        inode: u64,
    ) -> Result<bool> {
        let file = open_existing_ownership_file(root, &shared_helper_pin_name(owner))?;
        valid(&file)?;
        let metadata = file.metadata()?;
        ensure!(
            (metadata.dev(), metadata.ino()) == (device, inode),
            "shared helper pin identity changed"
        );
        match file.try_lock() {
            Ok(()) => {
                file.unlock()?;
                Ok(true)
            }
            Err(std::fs::TryLockError::WouldBlock) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

fn valid(file: &File) -> Result<()> {
    let metadata = file.metadata()?;
    // SAFETY: scalar native identity query with no pointer arguments.
    let user = unsafe { libc::geteuid() };
    ensure!(
        metadata.is_file()
            && metadata.len() == 0
            && metadata.nlink() == 1
            && metadata.mode() & 0o077 == 0
            && metadata.uid() == user,
        "shared helper pin is not private native ownership"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owner() -> SharedHelperIdentity {
        SharedHelperIdentity {
            service: airk_core::identity::Incarnation::from_bytes([1; 16]),
            helper: airk_core::identity::Incarnation::from_bytes([2; 16]),
        }
    }
    #[test]
    fn shared_pin_remains_held_until_last_guardian_description_closes() -> Result<()> {
        let root = tempfile::tempdir()?;
        let lease = SharedHelperLease::acquire(root.path(), owner())?;
        let identity = lease.identity()?;
        let inherited = lease.duplicate_native_pin()?;
        drop(lease);
        assert!(!SharedHelperLease::released(
            root.path(),
            owner(),
            identity.0,
            identity.1
        )?);
        drop(inherited);
        assert!(SharedHelperLease::released(
            root.path(),
            owner(),
            identity.0,
            identity.1
        )?);
        Ok(())
    }
    #[test]
    fn shared_pin_observation_never_creates_or_replaces_an_unknown_entry() -> Result<()> {
        let root = tempfile::tempdir()?;
        assert!(SharedHelperLease::released(root.path(), owner(), 1, 2).is_err());
        assert_eq!(fs::read_dir(root.path())?.count(), 0);
        let lease = SharedHelperLease::acquire(root.path(), owner())?;
        let identity = lease.identity()?;
        drop(lease);
        let path = root.path().join(shared_helper_pin_name(owner()));
        fs::rename(&path, root.path().join("saved"))?;
        fs::write(&path, b"foreign")?;
        assert!(SharedHelperLease::released(root.path(), owner(), identity.0, identity.1).is_err());
        assert_eq!(fs::read(path)?, b"foreign");
        Ok(())
    }
}
