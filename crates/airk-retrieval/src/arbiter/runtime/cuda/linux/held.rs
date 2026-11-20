// SPDX-License-Identifier: Apache-2.0
//! Retain exact source descriptors. Private executable bytes enter a sealed
//! memfd before dlopen, so a writable same-UID file cannot race the ELF loader.
use super::*;
use airk_process::directory::{OpenedDirectory, validate_child_name};
use std::{
    fs::{File, Metadata, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Component, PathBuf},
};

const SYSTEM_ROOTS: [&str; 2] = ["/usr/lib/wsl/lib", "/usr/lib/x86_64-linux-gnu"];

fn trusted_directory(path: &Path, host: bool) -> Result<OpenedDirectory> {
    let uid = unsafe { libc::geteuid() };
    ensure!(
        uid != 0 && path.is_absolute(),
        "Linux CUDA requires a non-root process and absolute input"
    );
    let mut directory = OpenedDirectory::open(Path::new("/"))?;
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(name) => directory = directory.open_child(name)?,
            _ => anyhow::bail!("ambiguous CUDA directory"),
        }
        let meta = directory.as_file().metadata()?;
        ensure!(
            meta.mode() & 0o022 == 0 && (meta.uid() == 0 || !host && meta.uid() == uid),
            "Linux CUDA directory has untrusted owner or write access"
        );
    }
    Ok(directory)
}

#[derive(PartialEq, Eq, Debug)]
struct Stamp {
    dev: u64,
    ino: u64,
    size: u64,
    ctime: i64,
    ctime_ns: i64,
}

impl From<&Metadata> for Stamp {
    fn from(meta: &Metadata) -> Self {
        Self {
            dev: meta.dev(),
            ino: meta.ino(),
            size: meta.len(),
            ctime: meta.ctime(),
            ctime_ns: meta.ctime_nsec(),
        }
    }
}

pub(super) fn private_image() -> Result<File> {
    let flags = libc::MFD_ALLOW_SEALING | libc::MFD_CLOEXEC;
    // GNU/Linux 6.3 added an explicit executable intent and a policy to refuse it.
    // EINVAL denotes the older flags ABI. Permission refusals remain terminal.
    let mut fd = unsafe { libc::memfd_create(c"airk-cuda".as_ptr(), flags | libc::MFD_EXEC) };
    if fd < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINVAL) {
        fd = unsafe { libc::memfd_create(c"airk-cuda".as_ptr(), flags) };
    }
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // Each successful memfd_create returned a fresh owned descriptor.
    let file = unsafe { File::from_raw_fd(fd) };
    ensure!(
        file.metadata()?.mode() & 0o111 != 0,
        "kernel disabled executable Linux CUDA memfd"
    );
    Ok(file)
}

pub(super) fn seal_image(image: &File) -> Result<()> {
    let seals = libc::F_SEAL_WRITE | libc::F_SEAL_GROW | libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;
    // All writes have completed, and no writable mapping was created.
    if unsafe { libc::fcntl(image.as_raw_fd(), libc::F_ADD_SEALS, seals) } < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

pub(super) struct Held {
    source: File,
    image: Option<File>,
    original: PathBuf,
    stamp: Stamp,
    pub(super) sha: Sha256Digest,
}

impl Held {
    pub(super) fn open(path: &Path, host: bool, maximum: u64) -> Result<Self> {
        let parent = trusted_directory(path.parent().context("Linux runtime file parent")?, host)?;
        let child = path.file_name().context("Linux runtime filename")?;
        validate_child_name(child)?;
        let direct = Path::new("/proc/self/fd")
            .join(parent.as_file().as_raw_fd().to_string())
            .join(child);
        let source = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(direct)?;
        let meta = source.metadata()?;
        ensure!(
            meta.is_file()
                && meta.len() > 0
                && meta.len() <= maximum
                && meta.mode() & 0o022 == 0
                && if host {
                    meta.uid() == 0
                } else {
                    meta.uid() == unsafe { libc::geteuid() } && meta.nlink() == 1
                },
            "Linux CUDA file has untrusted owner, writable mode, link or length"
        );
        Ok(Self {
            source,
            image: None,
            original: path.to_path_buf(),
            stamp: Stamp::from(&meta),
            sha: Sha256Digest::from_bytes([0; 32]),
        })
    }

    pub(super) fn metadata_bytes(&mut self, deadline: Instant) -> Result<Vec<u8>> {
        in_time(deadline)?;
        let mut data = vec![0; self.stamp.size.try_into()?];
        self.source.rewind()?;
        self.source.read_exact(&mut data)?;
        self.sha = Sha256Digest::from_bytes(Sha256::digest(&data).into());
        self.recheck(deadline)?;
        Ok(data)
    }

    pub(super) fn elf(
        path: &Path,
        host: bool,
        row: &manifest::linux::ElfFile,
        deadline: Instant,
    ) -> Result<Self> {
        let mut held = Self::open(path, host, row.bytes)?;
        ensure!(
            held.stamp.size == row.bytes,
            "Linux CUDA ELF extent differs from manifest"
        );
        if !host {
            held.image = Some(private_image()?);
        }
        let observed = held.inspect(&row.name, host, deadline)?;
        ensure!(
            observed == *row,
            "Linux CUDA ELF bytes, SHA-256, SONAME or ordered DT_NEEDED differ"
        );
        Ok(held)
    }

    pub(super) fn inspect(
        &mut self,
        name: &str,
        host: bool,
        deadline: Instant,
    ) -> Result<manifest::linux::ElfFile> {
        self.source.rewind()?;
        let mut digest = Sha256::new();
        let mut buffer = vec![0; 1 << 20];
        let mut remaining = self.stamp.size;
        while remaining > 0 {
            in_time(deadline)?;
            let len = remaining.min(buffer.len() as u64) as usize;
            self.source.read_exact(&mut buffer[..len])?;
            digest.update(&buffer[..len]);
            if let Some(image) = &mut self.image {
                image.write_all(&buffer[..len])?;
            }
            remaining -= len as u64;
        }
        self.sha = Sha256Digest::from_bytes(digest.finalize().into());
        if let Some(image) = &self.image {
            seal_image(image)?;
        }
        let file = self.image.as_mut().unwrap_or(&mut self.source);
        file.seek(SeekFrom::Start(0))?;
        let observed = super::super::elf::inspect(file, self.stamp.size, deadline)?;
        ensure!(
            !host || !observed.origin_search,
            "host library changes loader search path"
        );
        self.recheck(deadline)?;
        Ok(manifest::linux::ElfFile {
            name: name.to_owned(),
            bytes: self.stamp.size,
            sha256: self.sha,
            soname: observed.soname,
            needed: observed.needed,
        })
    }

    pub(super) fn recheck(&self, deadline: Instant) -> Result<()> {
        in_time(deadline)?;
        ensure!(
            self.stamp == Stamp::from(&self.source.metadata()?)
                && self.stamp == Stamp::from(&std::fs::symlink_metadata(&self.original)?),
            "Linux runtime source identity changed"
        );
        Ok(())
    }

    pub(super) fn image_file(&self) -> &File {
        self.image.as_ref().unwrap_or(&self.source)
    }

    pub(super) fn system_path(name: &str) -> Result<PathBuf> {
        validate_child_name(name.as_ref())?;
        let mut paths = BTreeSet::new();
        for root in SYSTEM_ROOTS {
            let child = Path::new(root).join(name);
            match child.canonicalize() {
                Ok(canonical) => {
                    trusted_directory(Path::new(root), true)?;
                    ensure!(
                        canonical
                            .parent()
                            .is_some_and(|p| SYSTEM_ROOTS.iter().any(|r| p == Path::new(r))),
                        "host CUDA symlink leaves approved system roots"
                    );
                    paths.insert(canonical);
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        ensure!(
            paths.len() == 1,
            "missing or ambiguous pinned Linux system dependency {name}"
        );
        Ok(paths.pop_first().expect("checked unique system path"))
    }
}
