// SPDX-License-Identifier: Apache-2.0
//! Bind glibc handles and resolved symbols to the kernel's file-backed VMAs.
use super::*;
use held::Held;
use std::{
    ffi::{CStr, CString, c_void},
    fs::File,
    io::{BufRead, BufReader},
    os::{fd::AsRawFd, unix::fs::MetadataExt},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct ImageId(u64, u64, u64);
impl ImageId {
    pub(super) fn file(file: &File) -> Result<Self> {
        let m = file.metadata()?;
        Ok(Self(
            libc::major(m.dev()).into(),
            libc::minor(m.dev()).into(),
            m.ino(),
        ))
    }
}

struct Map {
    start: usize,
    end: usize,
    image: ImageId,
    executable: bool,
}
fn map_row(line: &str) -> Result<Map> {
    let mut fields = line.split_whitespace();
    let (start, end) = fields
        .next()
        .and_then(|s| s.split_once('-'))
        .context("Linux VMA extent")?;
    let perms = fields.next().context("Linux VMA access")?;
    fields.next().context("Linux VMA offset")?;
    let (major, minor) = fields
        .next()
        .and_then(|s| s.split_once(':'))
        .context("Linux VMA device")?;
    let ino = fields.next().context("Linux VMA inode")?.parse()?;
    Ok(Map {
        start: usize::from_str_radix(start, 16)?,
        end: usize::from_str_radix(end, 16)?,
        image: ImageId(
            u64::from_str_radix(major, 16)?,
            u64::from_str_radix(minor, 16)?,
            ino,
        ),
        executable: perms.as_bytes().get(2) == Some(&b'x'),
    })
}

pub(super) fn identity_at(address: usize) -> Result<ImageId> {
    for line in BufReader::new(File::open("/proc/self/maps")?).lines() {
        let row = map_row(&line?)?;
        if (row.start..row.end).contains(&address) {
            ensure!(
                row.image.2 != 0,
                "symbol or loader metadata is not file-backed"
            );
            return Ok(row.image);
        }
    }
    anyhow::bail!("address is absent from current Linux process image")
}

pub(super) fn executable_images() -> Result<BTreeSet<ImageId>> {
    let mut images = BTreeSet::new();
    for line in BufReader::new(File::open("/proc/self/maps")?).lines() {
        let row = map_row(&line?)?;
        if row.executable && row.image.2 != 0 {
            images.insert(row.image);
        }
    }
    Ok(images)
}

// Public prefix of glibc's struct link_map from <link.h>. l_ld points into
// the loaded image and remains valid while an owning dlopen handle is held.
#[repr(C)]
struct LinkMap {
    address: usize,
    name: *const libc::c_char,
    dynamic: *const c_void,
}

pub(super) struct Handle(usize);

impl Handle {
    fn opened(raw: *mut c_void) -> Result<Self> {
        ensure!(
            !raw.is_null(),
            "loading authenticated Linux CUDA image failed"
        );
        Ok(Self(raw as usize))
    }

    pub(super) fn no_load(name: &str) -> Result<Option<Self>> {
        let name = CString::new(name)?;
        // RTLD_NOLOAD never loads an absent dependency.
        let handle = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_NOLOAD) };
        Ok((!handle.is_null()).then_some(Self(handle as usize)))
    }

    pub(super) fn map(held: &Held) -> Result<Self> {
        let path = CString::new(format!("/proc/self/fd/{}", held.image_file().as_raw_fd()))?;
        // All DT_NEEDED names have already resolved to authenticated handles.
        Self::opened(unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) })
    }

    pub(super) fn main_image() -> Result<Self> {
        Self::opened(unsafe { libc::dlopen(std::ptr::null(), libc::RTLD_NOW | libc::RTLD_LOCAL) })
    }

    pub(super) fn id(&self) -> Result<ImageId> {
        let mut link: *mut LinkMap = std::ptr::null_mut();
        // SAFETY: live dlopen handle, writable output for RTLD_DI_LINKMAP.
        let status = unsafe {
            libc::dlinfo(
                self.0 as *mut _,
                libc::RTLD_DI_LINKMAP,
                (&raw mut link).cast(),
            )
        };
        ensure!(
            status == 0 && !link.is_null(),
            "Linux loader did not identify held module"
        );
        identity_at(unsafe { (*link).dynamic } as usize)
    }

    /// The caller supplies the exact ABI of the named symbol. Library dependency
    /// lookup may return another image's symbol; check the kernel mapping first.
    pub(super) unsafe fn symbol<T: Copy>(&self, name: &CStr, image: ImageId) -> Result<T> {
        let pointer = unsafe { libc::dlsym(self.0 as *mut _, name.as_ptr()) };
        ensure!(
            !pointer.is_null()
                && std::mem::size_of::<T>() == std::mem::size_of_val(&pointer)
                && identity_at(pointer as usize)? == image,
            "Linux CUDA symbol missing or from a different image"
        );
        Ok(unsafe { std::mem::transmute_copy(&pointer) })
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe { libc::dlclose(self.0 as *mut _) };
        }
    }
}
