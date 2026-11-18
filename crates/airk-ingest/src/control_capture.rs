// SPDX-License-Identifier: Apache-2.0
//! Control-only native capture. Generic source/admin readers never receive this capability.
use anyhow::{Result, ensure};
use std::path::{Path, PathBuf};
#[cfg(any(test, target_os = "macos"))]
mod decision;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(crate) struct Owner {
    #[cfg(target_os = "macos")]
    anchor: std::sync::Mutex<Option<std::sync::Arc<macos::SystemEtc>>>,
    #[cfg(target_os = "macos")]
    failed: std::sync::atomic::AtomicBool,
}
impl Owner {
    pub(crate) fn path(&self, path: &Path) -> Result<ControlPath> {
        ensure!(
            path.is_absolute()
                && path.as_os_str().len() <= 32768
                && path.components().count() <= 256
                && path.components().all(|part| !matches!(
                    part,
                    std::path::Component::CurDir | std::path::Component::ParentDir
                )),
            "invalid control locator"
        );
        #[cfg(target_os = "macos")]
        let anchor = {
            use std::sync::atomic::Ordering;
            ensure!(
                !self.failed.load(Ordering::Acquire),
                "control anchor previously refused"
            );
            let result = (|| -> Result<_> {
                if !path.starts_with("/etc") {
                    return Ok(None);
                }
                let mut captured = self
                    .anchor
                    .lock()
                    .map_err(|_| anyhow::anyhow!("control anchor unavailable"))?;
                if captured.is_none() {
                    *captured = macos::SystemEtc::capture()?.map(std::sync::Arc::new);
                }
                if let Some(anchor) = captured.as_ref() {
                    anchor.recheck()?;
                }
                Ok(captured.clone())
            })();
            if result.is_err() {
                self.failed.store(true, Ordering::Release);
            }
            result?
        };
        Ok(ControlPath {
            original: path.to_path_buf(),
            #[cfg(target_os = "macos")]
            anchor,
        })
    }
    pub(crate) fn recheck(&self) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            use std::sync::atomic::Ordering;
            ensure!(
                !self.failed.load(Ordering::Acquire),
                "control anchor previously refused"
            );
            let result = self
                .anchor
                .lock()
                .map_err(|_| anyhow::anyhow!("control anchor unavailable"))?
                .as_ref()
                .map_or(Ok(()), |anchor| anchor.recheck());
            if result.is_err() {
                self.failed.store(true, Ordering::Release);
            }
            result?;
        }
        Ok(())
    }
}
pub(crate) struct ControlPath {
    original: PathBuf,
    #[cfg(target_os = "macos")]
    anchor: Option<std::sync::Arc<macos::SystemEtc>>,
}
pub(crate) struct Metadata {
    pub(crate) file: bool,
    pub(crate) directory: bool,
    pub(crate) symlink: bool,
    pub(crate) bytes: u64,
    #[cfg(windows)]
    pub(crate) attributes: u32,
}
impl From<std::fs::Metadata> for Metadata {
    fn from(value: std::fs::Metadata) -> Self {
        #[cfg(windows)]
        use std::os::windows::fs::MetadataExt;
        Self {
            file: value.is_file(),
            directory: value.is_dir(),
            symlink: value.file_type().is_symlink(),
            bytes: value.len(),
            #[cfg(windows)]
            attributes: value.file_attributes(),
        }
    }
}
impl ControlPath {
    pub(crate) fn physical(&self) -> PathBuf {
        #[cfg(target_os = "macos")]
        if self.anchor.is_some() {
            return Path::new("/private/etc").join(self.relative());
        }
        self.original.clone()
    }
    #[cfg(target_os = "macos")]
    fn relative(&self) -> &Path {
        self.original
            .strip_prefix("/etc")
            .expect("fixed control anchor prefix")
    }
    pub(crate) fn qualify(&self, directory: bool) -> Result<()> {
        #[cfg(target_os = "macos")]
        if let Some(anchor) = &self.anchor {
            return anchor.qualify(self.relative(), directory);
        }
        let path = &self.original;
        ensure!(path.is_absolute(), "control locator is not absolute");
        let anchor = path
            .ancestors()
            .last()
            .ok_or_else(|| anyhow::anyhow!("control locator has no root"))?;
        let relative = path
            .strip_prefix(anchor)?
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("non-UTF8 control locator"))?
            .replace('\\', "/");
        crate::native_name_boundary::qualify(anchor, &relative, directory)
    }
    pub(crate) fn metadata(&self) -> std::io::Result<Metadata> {
        #[cfg(target_os = "macos")]
        if let Some(anchor) = &self.anchor {
            return anchor.metadata(self.relative());
        }
        std::fs::symlink_metadata(&self.original).map(Into::into)
    }
    pub(crate) fn missing(&self) -> Result<bool> {
        #[cfg(target_os = "macos")]
        if let Some(anchor) = &self.anchor {
            anchor.qualify(self.relative(), false)?;
            let result = match anchor.metadata(self.relative()) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
                Err(error) => return Err(error.into()),
                Ok(_) => false,
            };
            anchor.recheck()?;
            return Ok(result);
        }
        crate::source_read::missing_protected_path(&self.original)
    }
    pub(crate) fn read(&self, limit: usize, remaining: usize) -> Result<Vec<u8>> {
        #[cfg(target_os = "macos")]
        if let Some(anchor) = &self.anchor {
            return anchor.read(self.relative(), &self.original, limit, remaining);
        }
        crate::source_read::read(
            self.original
                .parent()
                .ok_or_else(|| anyhow::anyhow!("control locator has no parent"))?,
            &self.original,
            limit,
            remaining,
        )
    }
}
