// SPDX-License-Identifier: Apache-2.0
//! Disk-provider write accounting without a dependency on collection storage.
use anyhow::Result;
use std::{
    fmt,
    io::{self, BufWriter, Write},
    path::{Component, Path},
    sync::Arc,
};
use tantivy::directory::{
    AntiCallToken, Directory, DirectoryLock, FileHandle, Lock, TerminatingWrite, WatchCallback,
    WatchHandle, WritePtr,
    error::{DeleteError, LockError, OpenReadError, OpenWriteError},
};

pub type Charge = Arc<dyn Fn(u64) -> Result<()> + Send + Sync>;

// Preserve typed admission errors through io/serde/Tantivy wrappers without
// coupling retrieval to the store's quota implementation.
#[derive(Debug)]
struct ChargeError(anyhow::Error);
impl fmt::Display for ChargeError {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, out)
    }
}
impl std::error::Error for ChargeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}

/// The callback must reject over-budget growth atomically across all provider
/// instances and retain the publication reservation for its entire lifetime.
#[derive(Clone)]
pub struct BoundedDirectory {
    inner: Box<dyn Directory>,
    charge: Charge,
}
impl BoundedDirectory {
    pub fn new(directory: impl Directory, charge: Charge) -> Self {
        Self {
            inner: Box::new(directory),
            charge,
        }
    }
}
impl fmt::Debug for BoundedDirectory {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("BoundedDirectory")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}
fn relative(path: &Path) -> io::Result<()> {
    if path.as_os_str().is_empty()
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(io::Error::other("invalid bounded directory path"));
    }
    Ok(())
}
impl Directory for BoundedDirectory {
    fn get_file_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>, OpenReadError> {
        relative(path).map_err(|error| OpenReadError::wrap_io_error(error, path.into()))?;
        self.inner.get_file_handle(path)
    }
    fn delete(&self, path: &Path) -> Result<(), DeleteError> {
        relative(path).map_err(|error| DeleteError::IoError {
            io_error: Arc::new(error),
            filepath: path.into(),
        })?;
        // No refunds: deleted merge/intermediate files still consumed their
        // allocation and may remain open on platforms with deferred deletion.
        self.inner.delete(path)
    }
    fn exists(&self, path: &Path) -> Result<bool, OpenReadError> {
        relative(path).map_err(|error| OpenReadError::wrap_io_error(error, path.into()))?;
        self.inner.exists(path)
    }
    fn open_write(&self, path: &Path) -> Result<WritePtr, OpenWriteError> {
        relative(path).map_err(|error| OpenWriteError::wrap_io_error(error, path.into()))?;
        let writer = BudgetWriter::new(self.inner.open_write(path)?, self.charge.clone());
        // Observe denial before uncharged data can enter the directory's buffer.
        Ok(BufWriter::with_capacity(0, Box::new(writer)))
    }
    fn atomic_read(&self, path: &Path) -> Result<Vec<u8>, OpenReadError> {
        relative(path).map_err(|error| OpenReadError::wrap_io_error(error, path.into()))?;
        self.inner.atomic_read(path)
    }
    fn atomic_write(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        relative(path)?;
        // Atomic replacement creates a full temporary file alongside its old
        // version, so charge the entire payload before opening that temporary.
        (self.charge)(bytes.len() as u64).map_err(|error| io::Error::other(ChargeError(error)))?;
        self.inner.atomic_write(path, bytes)
    }
    fn sync_directory(&self) -> io::Result<()> {
        self.inner.sync_directory()
    }
    fn acquire_lock(&self, lock: &Lock) -> Result<DirectoryLock, LockError> {
        relative(&lock.filepath).map_err(LockError::wrap_io_error)?;
        // Native Tantivy lock files are empty and add no logical payload bytes.
        self.inner.acquire_lock(lock)
    }
    fn watch(&self, callback: WatchCallback) -> tantivy::Result<WatchHandle> {
        self.inner.watch(callback)
    }
}

pub struct BudgetWriter<W> {
    inner: W,
    charge: Charge,
}
impl<W> BudgetWriter<W> {
    pub fn new(inner: W, charge: Charge) -> Self {
        Self { inner, charge }
    }
    pub fn get_ref(&self) -> &W {
        &self.inner
    }
    pub fn into_inner(self) -> W {
        self.inner
    }
}
impl<W: Write> Write for BudgetWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        (self.charge)(bytes.len() as u64).map_err(|error| io::Error::other(ChargeError(error)))?;
        self.inner.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}
impl<W: TerminatingWrite> TerminatingWrite for BudgetWriter<W> {
    fn terminate_ref(&mut self, token: AntiCallToken) -> io::Result<()> {
        self.inner.terminate_ref(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tantivy::{
        Index, TantivyDocument,
        directory::MmapDirectory,
        schema::{Schema, TEXT},
    };

    #[test]
    fn serialized_writes_preserve_typed_admission_cause() {
        #[derive(Debug)]
        struct Denied;
        impl fmt::Display for Denied {
            fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
                out.write_str("original admission denial")
            }
        }
        impl std::error::Error for Denied {}
        let charge: Charge = Arc::new(|_| Err(Denied.into()));
        let mut writer = BudgetWriter::new(Vec::new(), charge);
        let error = anyhow::Error::from(
            serde_json::to_writer(&mut writer, &vec!["original fixture"]).unwrap_err(),
        );
        assert!(error.chain().any(|cause| cause.is::<Denied>()));
        assert!(writer.get_ref().is_empty());
    }

    fn budget(maximum: u64) -> (Charge, Arc<AtomicU64>) {
        let used = Arc::new(AtomicU64::new(0));
        let counter = used.clone();
        (
            Arc::new(move |bytes| {
                counter
                    .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |prior| {
                        prior.checked_add(bytes).filter(|next| *next <= maximum)
                    })
                    .map_err(|_| anyhow::anyhow!("fixture disk allocation exhausted"))?;
                Ok(())
            }),
            used,
        )
    }
    #[test]
    fn buffered_and_atomic_writes_fail_before_exceeding_provider_allocation() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (charge, used) = budget(6);
        let directory = BoundedDirectory::new(MmapDirectory::open(root.path())?, charge);
        directory.atomic_write(Path::new("metadata"), b"old")?;
        assert!(
            directory
                .atomic_write(Path::new("metadata"), b"larger")
                .is_err()
        );
        assert_eq!(directory.atomic_read(Path::new("metadata"))?, b"old");
        let mut writer = directory.open_write(Path::new("segment"))?;
        assert!(writer.write_all(b"sixsix").is_err());
        assert_eq!(std::fs::metadata(root.path().join("segment"))?.len(), 0);
        assert_eq!(used.load(Ordering::SeqCst), 3);
        writer.write_all(b"yes")?;
        writer.terminate()?;
        assert_eq!(std::fs::read(root.path().join("segment"))?, b"yes");
        assert!(directory.atomic_write(Path::new("metadata"), b"x").is_err());
        assert_eq!(directory.atomic_read(Path::new("metadata"))?, b"old");
        assert!(
            directory
                .atomic_write(Path::new("../outside"), b"x")
                .is_err()
        );
        Ok(())
    }
    #[test]
    fn real_tantivy_commit_obeys_directory_budget() -> Result<()> {
        let root = tempfile::tempdir()?;
        let (charge, used) = budget(65536);
        let directory = BoundedDirectory::new(MmapDirectory::open(root.path())?, charge);
        let mut schema = Schema::builder();
        let text = schema.add_text_field("text", TEXT);
        let index = Index::create(directory, schema.build(), Default::default())?;
        let mut writer = index.writer_with_num_threads::<TantivyDocument>(1, 15_000_000)?;
        let mut document = TantivyDocument::default();
        document.add_text(text, "bounded lexical fixture");
        writer.add_document(document)?;
        writer.commit()?;
        writer.wait_merging_threads()?;
        let disk_bytes: u64 = std::fs::read_dir(root.path())?
            .map(|entry| {
                entry
                    .and_then(|entry| entry.metadata())
                    .map(|metadata| metadata.len())
            })
            .collect::<io::Result<Vec<_>>>()?
            .into_iter()
            .sum();
        assert!(disk_bytes <= used.load(Ordering::SeqCst));
        assert!(used.load(Ordering::SeqCst) <= 65536);
        let denied = tempfile::tempdir()?;
        let (charge, _) = budget(1);
        let directory = BoundedDirectory::new(MmapDirectory::open(denied.path())?, charge);
        assert!(Index::create(directory, index.schema(), Default::default()).is_err());
        Ok(())
    }
}
