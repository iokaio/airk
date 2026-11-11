// SPDX-License-Identifier: Apache-2.0
//! Opt-in CI worker stderr tails. Never archive service databases or source trees.
#[cfg(test)]
use anyhow::{Context, Result};
use std::{
    fs::File,
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
    process::Child,
    sync::{Arc, Mutex},
    thread::JoinHandle,
};
#[cfg(test)]
use std::{fs::OpenOptions, io::Read, path::Path};

// Only a child owner has a stderr pipe; in-process owners have none.
#[cfg(test)]
const TAIL_BYTES: usize = 4096;

pub(crate) fn directory() -> Option<PathBuf> {
    (std::env::var_os("AIRK_PROCESS_DIAGNOSTICS").as_deref() == Some(std::ffi::OsStr::new("1")))
        .then(|| std::env::var_os("AIRK_NATIVE_DIAGNOSTIC_DIR").map(PathBuf::from))
        .flatten()
}

struct Tail {
    file: File,
    bytes: Vec<u8>,
    pid: u32,
    dropped: u64,
    exit: Option<String>,
    read_failed: bool,
    stderr_eof: bool,
}

impl Tail {
    fn save(&mut self) -> std::io::Result<()> {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schema": "airk-worker-stderr/1", "pid": self.pid,
            "stderr": String::from_utf8_lossy(&self.bytes), "dropped_bytes": self.dropped,
            "exit": self.exit, "read_failed": self.read_failed, "stderr_eof": self.stderr_eof,
        }))?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&bytes)?;
        self.file.set_len(bytes.len() as u64)?;
        self.file.flush()
    }

    #[cfg(test)]
    fn append(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
        if self.bytes.len() > TAIL_BYTES {
            let excess = self.bytes.len() - TAIL_BYTES;
            self.bytes.drain(..excess);
            self.dropped += excess as u64;
        }
    }
}

pub(crate) struct WorkerStderr {
    tail: Arc<Mutex<Tail>>,
    reader: JoinHandle<()>,
}

impl WorkerStderr {
    #[cfg(test)]
    pub(crate) fn start(child: &mut Child, role: &str, parent: &Path) -> Result<Self> {
        std::fs::create_dir_all(parent)?;
        // Exclusive files under the explicitly supplied CI destination. The
        // opaque worker role prevents PID reuse from overwriting prior evidence.
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(parent.join(format!("worker-{}-{role}.json", child.id())))?;
        let tail = Arc::new(Mutex::new(Tail {
            file,
            bytes: Vec::with_capacity(TAIL_BYTES),
            pid: child.id(),
            dropped: 0,
            exit: None,
            read_failed: false,
            stderr_eof: false,
        }));
        tail.lock()
            .map_err(|_| anyhow::anyhow!("worker diagnostic lock poisoned"))?
            .save()?;
        let mut stderr = child
            .stderr
            .take()
            .context("worker diagnostic stderr missing")?;
        let target = Arc::clone(&tail);
        let reader = std::thread::Builder::new()
            .name("airk-worker-stderr".into())
            .spawn(move || {
                let mut buffer = [0; 4096];
                loop {
                    let read = stderr.read(&mut buffer);
                    let Ok(mut tail) = target.lock() else { return };
                    match read {
                        Ok(0) => {
                            tail.stderr_eof = true;
                            let _ = tail.save();
                            break;
                        }
                        Ok(count) => tail.append(&buffer[..count]),
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(_) => {
                            tail.read_failed = true;
                            let _ = tail.save();
                            break;
                        }
                    }
                    // A diagnostic write failure never stops draining the pipe.
                    let _ = tail.save();
                }
            })?;
        Ok(Self { tail, reader })
    }

    /// Called after the existing native owner has drained the worker/children.
    pub(crate) fn finish(self, child: &mut Child) {
        // An inherited stderr descriptor must not extend native shutdown. The
        // bounded collector can finish independently; EOF remains explicit.
        if self.reader.is_finished() {
            let _ = self.reader.join();
        }
        if let Ok(mut tail) = self.tail.lock() {
            tail.exit = child
                .try_wait()
                .ok()
                .flatten()
                .map(|status| status.to_string());
            let _ = tail.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_bounds_noisy_worker_output_and_records_its_exit() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut child = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "process_diagnostics::tests::stderr_child",
                "--nocapture",
            ])
            .env("AIRK_TEST_STDERR_CHILD", "1")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        let capture = WorkerStderr::start(&mut child, "fixture", directory.path())?;
        let pid = child.id();
        let status = child.wait()?;
        assert_eq!(status.code(), Some(7));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !capture.reader.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(
            capture.reader.is_finished(),
            "stderr collector did not drain the exited fixture"
        );
        capture.finish(&mut child);
        let bytes = std::fs::read(directory.path().join(format!("worker-{pid}-fixture.json")))?;
        assert!(bytes.len() < 8192);
        let report: serde_json::Value = serde_json::from_slice(&bytes)?;
        assert_eq!(report["stderr"].as_str().unwrap().len(), TAIL_BYTES);
        assert!(
            report["stderr"]
                .as_str()
                .unwrap()
                .ends_with("worker failure")
        );
        assert_eq!(report["dropped_bytes"], 64 * 4096 + 14 - TAIL_BYTES);
        assert_eq!(report["exit"], status.to_string());
        assert_eq!(report["pid"], pid);
        assert_eq!(report["read_failed"], false);
        assert_eq!(report["stderr_eof"], true);
        Ok(())
    }

    #[test]
    fn stderr_child() {
        if std::env::var_os("AIRK_TEST_STDERR_CHILD").is_some() {
            let mut stderr = std::io::stderr().lock();
            for _ in 0..64 {
                stderr.write_all(&[b'x'; 4096]).unwrap();
            }
            stderr.write_all(b"worker failure").unwrap();
            stderr.flush().unwrap();
            std::process::exit(7);
        }
    }
}
