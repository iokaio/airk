// SPDX-License-Identifier: Apache-2.0
//! Canonical bounded redb service metadata; collection evidence stays worker-owned.
use super::ProjectLease;
use airk_core::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    service::RegisteredProject,
    service_watch::WatchObservation,
};
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    io::{Read, Write},
    ops::{Deref, DerefMut},
    path::Path,
};
pub const MAX_PROJECTS: usize = 1024;
#[path = "service_source_isolation.rs"]
mod source_isolation;
pub use source_isolation::SourceBoundary;
#[path = "service_provider_drains.rs"]
mod provider_drains;
pub use provider_drains::{MAX_PROVIDER_DRAINS, ProviderBoot, ProviderDrain};
#[path = "service_registry_native.rs"]
mod native;
pub use native::{NativeServiceRegistry, fresh_setup_namespaces, reset_owned_sqlite};
pub struct WatchProject {
    pub project: RegisteredProject,
    pub configuration_epoch: u64,
    pub additional_roots: Vec<String>,
    pub observation: WatchObservation,
}
pub struct ServiceRegistry {
    native: NativeServiceRegistry,
}
impl ServiceRegistry {
    pub fn open(root: &Path, incarnation: Incarnation) -> Result<Self> {
        Ok(Self {
            native: NativeServiceRegistry::open(root, incarnation)?,
        })
    }
    pub fn open_saved(root: &Path) -> Result<Self> {
        Ok(Self {
            native: NativeServiceRegistry::open_saved(root)?,
        })
    }
    pub fn incarnation(&self) -> Result<Incarnation> {
        Ok(self.native.incarnation())
    }
}
impl Deref for ServiceRegistry {
    type Target = NativeServiceRegistry;
    fn deref(&self) -> &Self::Target {
        &self.native
    }
}
impl DerefMut for ServiceRegistry {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.native
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    format: u32,
    root: String,
    incarnation: Incarnation,
    #[serde(default)]
    ready: bool,
    #[serde(default)]
    digest: Option<Sha256Digest>,
}

impl Owner {
    fn checksum(&self) -> Sha256Digest {
        framed_digest(
            b"airk/service-registry-owner/2",
            &[
                self.root.as_bytes(),
                self.incarnation.as_bytes(),
                &[u8::from(self.ready)],
            ],
        )
    }

    fn complete(mut self, root: &Path) -> Result<()> {
        self.format = 2;
        self.ready = true;
        self.digest = Some(self.checksum());
        let mut temporary = tempfile::NamedTempFile::new_in(root)?;
        temporary.write_all(&serde_json::to_vec(&self)?)?;
        temporary.as_file().sync_all()?;
        temporary.persist(root.join("service-1.4.owner.json"))?;
        crate::maintenance::sync_parent(root)
    }
}

fn owner(root: &Path, existed: bool, incarnation: Incarnation) -> Result<Owner> {
    let path = root.join("service-1.4.owner.json");
    let root_text = root
        .to_str()
        .context("service runtime root must be UTF-8")?;
    ensure!(
        root_text.len() <= 4096,
        "service runtime root exceeds budget"
    );
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 16384,
                "invalid service registry owner"
            );
            crate::managed_collection::reject_file_aliases(&path, &metadata)?;
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "redirected service registry owner"
                );
            }
            let mut bytes = Vec::new();
            fs::File::open(path)?.take(16385).read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() <= 16384,
                "service registry owner exceeds budget"
            );
            let owner: Owner = serde_json::from_slice(&bytes)?;
            ensure!(
                owner.root == root_text
                    && match owner.format {
                        1 => !owner.ready && owner.digest.is_none(),
                        2 => owner.digest == Some(owner.checksum()),
                        _ => false,
                    },
                "foreign service registry owner"
            );
            Ok(owner)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ensure!(!existed, "service registry has no ownership receipt");
            let mut owner = Owner {
                format: 2,
                root: root_text.into(),
                incarnation,
                ready: false,
                digest: None,
            };
            owner.digest = Some(owner.checksum());
            let mut temporary = tempfile::NamedTempFile::new_in(root)?;
            temporary.write_all(&serde_json::to_vec(&owner)?)?;
            temporary.as_file().sync_all()?;
            temporary.persist_noclobber(path)?;
            crate::maintenance::sync_parent(root)?;
            Ok(owner)
        }
        Err(error) => Err(error.into()),
    }
}

/// Read an already completed namespace without creating or opening a database.
/// Installed-state receipts use this before any directory/registry recovery.
pub fn retained_namespace(root: &Path) -> Result<Incarnation> {
    let root = root.canonicalize()?;
    let native = root.join("service-1.4.airk");
    let path = if native.try_exists()? {
        native
    } else {
        root.join("service-1.4.sqlite3")
    };
    if !path.try_exists()?
        && let Some((ownership, _)) = fresh_setup_namespaces(&root)?
    {
        return Ok(ownership);
    }
    let metadata =
        fs::symlink_metadata(&path).context("initialized service registry is missing")?;
    ensure!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() > 0
            && metadata.len() <= crate::ledger::MAX_FILE_BYTES,
        "initialized service registry is unavailable or redirected"
    );
    crate::managed_collection::reject_file_aliases(&path, &metadata)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "redirected service registry"
        );
    }
    let receipt = owner(&root, true, Incarnation::from_bytes([0; 16]))?;
    ensure!(
        receipt.format == 2
            && (receipt.ready
                || fresh_setup_namespaces(&root)?
                    .is_some_and(|(ownership, _)| ownership == receipt.incarnation)),
        "service registry initialization is incomplete"
    );
    Ok(receipt.incarnation)
}
