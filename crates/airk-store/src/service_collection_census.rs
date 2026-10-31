// SPDX-License-Identifier: Apache-2.0
//! Bounded census evidence only: no accelerated authority without mutation producers.
use super::*;
#[cfg(not(windows))]
use std::collections::BTreeSet;
#[cfg(target_os = "linux")]
use std::fs::File;

#[derive(Debug, PartialEq, Eq)]
pub enum CensusOutcome {
    Pending {
        objects: usize,
    },
    /// Complete observed privacy census, not byte integrity or read authorization.
    Observed {
        objects: usize,
        mutation_epoch: u64,
    },
    Ineligible(CensusUnknown),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CensusUnknown {
    UnsupportedPlatform,
    ObjectEvidence,
    DuplicateIdentity,
    MountBoundary,
    InventoryBound,
    NamespaceChanged,
}

/// Ending every iterator proves coverage only under the retained real service
/// lease and exclusive writer borrow. No ordinary query uses this capability.
pub struct Census<'a> {
    owner: &'a IntegrityOwner<'a>,
    fence: (Incarnation, u64),
    scope: crate::current::CurrentScope,
    token: CurrentToken,
    root: PathBuf,
    #[cfg(any(target_os = "linux", windows))]
    root_identity: [u64; 3],
    writer: &'a mut crate::WriterLock,
    observed_generation: u64,
    #[cfg(not(windows))]
    seen: BTreeSet<[u64; 3]>,
    #[cfg(target_os = "linux")]
    frames: Vec<Frame>,
    #[cfg(target_os = "linux")]
    _root_pin: Option<File>,
    #[cfg(windows)]
    windows: Option<crate::service_census_windows::WindowsWalk>,
    terminal: Option<CensusUnknown>,
    complete: bool,
    observed_objects: usize,
}
/// Borrowed live service-owned privacy evidence; never serialized or cloned.
/// Retains the real writer/lease and must not survive their owner lifecycle.
pub struct OwnerPrivateCensus<'a>(Census<'a>);
impl OwnerPrivateCensus<'_> {
    pub fn check(&self) -> Result<()> {
        self.0.check_fence()?;
        ensure!(
            self.0.complete && self.0.terminal.is_none(),
            "incomplete private census"
        );
        if let Some(receipt) = self.0.owner.load(self.0.fence.0)? {
            ensure!(
                receipt.known_damage.len() < MAX_DAMAGE,
                "integrity damage capacity requires strict fallback"
            );
        }
        Ok(())
    }
    /// Save a dated observation, never a restart-reusable authority receipt.
    pub fn record(&self) -> Result<IntegrityObservation> {
        self.check()?;
        let selected = self.0.owner.collection.select()?;
        let read = selected.store.current_read(&self.0.scope)?;
        ensure!(
            read.token()? == self.0.token,
            "census observation publication changed"
        );
        let observed_unix_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis()
            .try_into()?;
        self.0.owner.publish(
            &read,
            None,
            PrivacyEligibility::OwnerCensus {
                version: 1,
                objects: self.0.observed_objects,
                observed_unix_ms,
            },
        )
    }
}
#[cfg(target_os = "linux")]
struct Frame {
    file: File,
    entries: fs::ReadDir,
    count: usize,
}

impl<'a> IntegrityOwner<'a> {
    /// Call only as bounded service Verify work; no transaction survives a step.
    pub fn begin_census(
        &'a self,
        read: &CurrentRead,
        writer: &'a mut crate::WriterLock,
    ) -> Result<Census<'a>> {
        let fence = self.check()?;
        let selected = self.collection.select()?;
        ensure!(
            selected.store.current_owned_scope()?.as_ref() == Some(&read.head().scope),
            "census selection changed"
        );
        #[cfg(any(target_os = "linux", windows))]
        let token = read.token()?;
        let root = selected.store.directory().to_path_buf();
        crate::maintenance::check_writer(
            &selected.store,
            writer,
            &Quota::open_service(Path::new(&self.collection.owner.data_root))?,
        )?;
        #[cfg(any(target_os = "linux", windows))]
        let observed_generation = writer.census_mutation_epoch();
        #[cfg(target_os = "linux")]
        {
            let file = open_absolute(&root)?;
            let Some(identity) = private_identity(&file, true)? else {
                return Census::ineligible(
                    self,
                    fence,
                    read,
                    root,
                    writer,
                    CensusUnknown::ObjectEvidence,
                );
            };
            let entries = entries(&file)?;
            let root_pin = file.try_clone()?;
            let mut seen = BTreeSet::new();
            seen.insert(identity);
            Ok(Census {
                owner: self,
                fence,
                scope: read.head().scope.clone(),
                token,
                root,
                root_identity: identity,
                writer,
                observed_generation,
                seen,
                frames: vec![Frame {
                    file,
                    entries,
                    count: 0,
                }],
                _root_pin: Some(root_pin),
                terminal: None,
                complete: false,
                observed_objects: 1,
            })
        }
        #[cfg(windows)]
        {
            let walk = crate::service_census_windows::WindowsWalk::begin(&root)?;
            let root_identity = walk.root_identity();
            Ok(Census {
                owner: self,
                fence,
                scope: read.head().scope.clone(),
                token,
                root,
                root_identity,
                writer,
                observed_generation,
                windows: Some(walk),
                terminal: None,
                complete: false,
                observed_objects: 1,
            })
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            Census::ineligible(
                self,
                fence,
                read,
                root,
                writer,
                CensusUnknown::UnsupportedPlatform,
            )
        }
    }
}
impl<'a> Census<'a> {
    #[cfg(not(windows))]
    fn ineligible(
        owner: &'a IntegrityOwner<'a>,
        fence: (Incarnation, u64),
        read: &CurrentRead,
        root: PathBuf,
        writer: &'a mut crate::WriterLock,
        reason: CensusUnknown,
    ) -> Result<Self> {
        let observed_generation = writer.census_mutation_epoch();
        Ok(Self {
            owner,
            fence,
            scope: read.head().scope.clone(),
            token: read.token()?,
            root,
            #[cfg(target_os = "linux")]
            root_identity: [0; 3],
            writer,
            observed_generation,
            #[cfg(not(windows))]
            seen: BTreeSet::new(),
            #[cfg(target_os = "linux")]
            frames: Vec::new(),
            #[cfg(target_os = "linux")]
            _root_pin: None,
            #[cfg(windows)]
            windows: None,
            terminal: Some(reason),
            complete: false,
            observed_objects: 0,
        })
    }
    fn check_fence(&self) -> Result<()> {
        ensure!(
            self.owner.check()? == self.fence
                && self.writer.census_mutation_epoch() == self.observed_generation,
            "census ownership generation changed"
        );
        let selected = self.owner.collection.select()?;
        ensure!(
            selected.store.directory() == self.root
                && selected.store.current_owned_scope()?.as_ref() == Some(&self.scope)
                && selected.store.current_read(&self.scope)?.token()? == self.token,
            "census selection changed"
        );
        #[cfg(target_os = "linux")]
        ensure!(
            private_identity(&open_absolute(&self.root)?, true)? == Some(self.root_identity),
            "census root namespace changed"
        );
        #[cfg(windows)]
        {
            let walk = self
                .windows
                .as_ref()
                .context("Windows census root absent")?;
            ensure!(
                walk.root_identity() == self.root_identity,
                "Windows census root identity changed"
            );
            walk.check_current()?;
        }
        Ok(())
    }
    pub fn into_owner_private(self) -> Result<OwnerPrivateCensus<'a>> {
        let proof = OwnerPrivateCensus(self);
        proof.check()?;
        Ok(proof)
    }
    /// Fixed maximum64 visited entries per turn. FS calls themselves are not preemptible.
    pub fn step(&mut self, maximum: usize) -> Result<CensusOutcome> {
        ensure!((1..=64).contains(&maximum), "invalid census step budget");
        if let Some(reason) = &self.terminal {
            return Ok(CensusOutcome::Ineligible(reason.clone()));
        }
        if self.complete {
            self.check_fence()?;
            return Ok(CensusOutcome::Observed {
                objects: self.observed_objects,
                mutation_epoch: self.observed_generation,
            });
        }
        if self.owner.check()? != self.fence {
            return self.stop(CensusUnknown::NamespaceChanged);
        }
        if self.writer.census_mutation_epoch() != self.observed_generation {
            return self.stop(CensusUnknown::NamespaceChanged);
        }
        let selected = self.owner.collection.select()?;
        if selected.store.directory() != self.root
            || selected.store.current_owned_scope()?.as_ref() != Some(&self.scope)
            || selected.store.current_read(&self.scope)?.token()? != self.token
        {
            return self.stop(CensusUnknown::NamespaceChanged);
        }
        drop(selected);
        #[cfg(target_os = "linux")]
        {
            let root = open_absolute(&self.root)?;
            if private_identity(&root, true)? != Some(self.root_identity) {
                return self.stop(CensusUnknown::NamespaceChanged);
            }
            for _ in 0..maximum {
                let Some(frame) = self.frames.last_mut() else {
                    self.check_fence()?;
                    self.complete = true;
                    self.observed_objects = self.seen.len();
                    return Ok(CensusOutcome::Observed {
                        objects: self.seen.len(),
                        mutation_epoch: self.observed_generation,
                    });
                };
                let Some(entry) = frame.entries.next() else {
                    self.frames.pop();
                    continue;
                };
                let entry = entry?;
                frame.count += 1;
                if frame.count > 4096 || self.seen.len() >= 100_000 {
                    return self.stop(CensusUnknown::InventoryBound);
                }
                let file = open_child(&frame.file, &entry.file_name())?;
                let metadata = file.metadata()?;
                if !metadata.is_dir() && !metadata.is_file() {
                    return self.stop(CensusUnknown::ObjectEvidence);
                }
                let Some(identity) = private_identity(&file, metadata.is_dir())? else {
                    return self.stop(CensusUnknown::ObjectEvidence);
                };
                if identity[0] != self.root_identity[0] {
                    return self.stop(CensusUnknown::MountBoundary);
                }
                if !self.seen.insert(identity) {
                    return self.stop(CensusUnknown::DuplicateIdentity);
                }
                if metadata.is_dir() {
                    if self.frames.len() >= 64 {
                        return self.stop(CensusUnknown::InventoryBound);
                    }
                    let entries = entries(&file)?;
                    self.frames.push(Frame {
                        file,
                        entries,
                        count: 0,
                    });
                }
            }
        }
        #[cfg(windows)]
        {
            use crate::service_census_windows::Outcome;
            let owner = self.owner;
            let writer = &*self.writer;
            let expected = self.fence;
            let epoch = self.observed_generation;
            let root = &self.root;
            let scope = &self.scope;
            let token = self.token;
            let outcome = self
                .windows
                .as_mut()
                .context("Windows census cursor absent")?
                .step(maximum, || {
                    ensure!(
                        owner.check()? == expected && writer.census_mutation_epoch() == epoch,
                        "Windows census worker/writer changed"
                    );
                    let selected = owner.collection.select()?;
                    ensure!(
                        selected.store.directory() == root
                            && selected.store.current_owned_scope()?.as_ref() == Some(scope)
                            && selected.store.current_read(scope)?.token()? == token,
                        "Windows census selection changed"
                    );
                    Ok(())
                })?;
            Ok(match outcome {
                Outcome::Pending { objects } => {
                    self.observed_objects = objects;
                    CensusOutcome::Pending { objects }
                }
                Outcome::Observed { objects } => {
                    self.complete = true;
                    self.observed_objects = objects;
                    CensusOutcome::Observed {
                        objects,
                        mutation_epoch: self.observed_generation,
                    }
                }
            })
        }
        #[cfg(not(windows))]
        {
            Ok(CensusOutcome::Pending {
                objects: self.seen.len(),
            })
        }
    }
    fn stop(&mut self, reason: CensusUnknown) -> Result<CensusOutcome> {
        self.terminal = Some(reason.clone());
        #[cfg(target_os = "linux")]
        self.frames.clear();
        #[cfg(windows)]
        {
            self.windows = None;
        }
        Ok(CensusOutcome::Ineligible(reason))
    }
}
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn test_invalidate(census: &Census<'_>) -> Result<()> {
    census.writer.invalidate_census_before_mutation()
}

#[cfg(target_os = "linux")]
fn private_identity(file: &File, directory: bool) -> Result<Option<[u64; 3]>> {
    use crate::private_storage::{Kind, Observation, inspect};
    Ok(
        match inspect(
            file,
            if directory {
                Kind::Directory
            } else {
                Kind::File
            },
        )? {
            Observation::Private(proof) if proof.recheck()?.is_ok() => Some(proof.identity()),
            _ => None,
        },
    )
}
#[cfg(target_os = "linux")]
fn entries(file: &File) -> Result<fs::ReadDir> {
    use std::os::fd::AsRawFd;
    // procfd names the retained directory descriptor; child access never uses
    // the enumerated path and always goes through openat on that same handle.
    fs::read_dir(format!("/proc/self/fd/{}", file.as_raw_fd())).map_err(Into::into)
}
#[cfg(target_os = "linux")]
fn open_child(parent: &File, name: &std::ffi::OsStr) -> Result<File> {
    use std::os::{
        fd::{AsRawFd, FromRawFd},
        unix::ffi::OsStrExt,
    };
    let name = std::ffi::CString::new(name.as_bytes())?;
    let mut metadata = std::mem::MaybeUninit::<libc::stat>::uninit();
    // Reject special entries before opening; no-follow open still guards a
    // subsequent link replacement. Missing mutation ownership prevents promotion.
    ensure!(
        unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                name.as_ptr(),
                metadata.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } == 0,
        "census child metadata unavailable"
    );
    let mode = unsafe { metadata.assume_init() }.st_mode & libc::S_IFMT;
    ensure!(
        mode == libc::S_IFREG || mode == libc::S_IFDIR,
        "census child is not regular storage"
    );
    // SAFETY: owned parent fd, terminated single-component name, no-follow open.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK,
        )
    };
    ensure!(
        fd >= 0,
        "census child unavailable or redirected: {}",
        std::io::Error::last_os_error()
    );
    Ok(unsafe { File::from_raw_fd(fd) })
}
#[cfg(all(test, target_os = "linux"))]
pub(crate) fn test_open_child(parent: &File, name: &std::ffi::OsStr) -> Result<File> {
    open_child(parent, name)
}
#[cfg(target_os = "linux")]
fn open_absolute(path: &Path) -> Result<File> {
    use std::path::Component;
    ensure!(path.is_absolute(), "census root must be absolute");
    let mut parent = File::open("/")?;
    ensure!(
        safe_ancestor(&parent)?,
        "census namespace ancestry is not positively protected"
    );
    for (count, component) in path.components().enumerate() {
        ensure!(count < 64, "census ancestry bound exceeded");
        match component {
            Component::RootDir => (),
            Component::Normal(name) => {
                parent = open_child(&parent, name)?;
                ensure!(
                    parent.metadata()?.is_dir(),
                    "census ancestor is not directory"
                );
                ensure!(
                    safe_ancestor(&parent)?,
                    "census namespace ancestry is not positively protected"
                );
            }
            _ => anyhow::bail!("census root traversal is not canonical"),
        }
    }
    // This establishes bounded protected ancestry only. Positive census authority
    // also requires per-object platform evidence, complete enumeration, and a
    // live checked owner/writer fence.
    Ok(parent)
}

#[cfg(target_os = "linux")]
fn safe_ancestor(file: &File) -> Result<bool> {
    use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
    let before = file.metadata()?;
    let uid = unsafe { libc::geteuid() };
    if !before.is_dir() || (before.uid() != 0 && before.uid() != uid) || before.mode() & 0o022 != 0
    {
        return Ok(false);
    }
    for name in [c"system.posix_acl_access", c"system.posix_acl_default"] {
        let size =
            unsafe { libc::fgetxattr(file.as_raw_fd(), name.as_ptr(), std::ptr::null_mut(), 0) };
        if size >= 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ENODATA) {
            return Ok(false);
        }
    }
    let after = file.metadata()?;
    Ok((before.dev(), before.ino(), before.uid(), before.mode())
        == (after.dev(), after.ino(), after.uid(), after.mode()))
}
