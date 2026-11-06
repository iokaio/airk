// SPDX-License-Identifier: Apache-2.0
//! Actual matcher-input ledger. Only a completed scanner may seal it; JSON
//! baseline bytes become trusted only through an authenticated Current resource.
use airk_core::{
    identity::Sha256Digest,
    source_control::{
        ControlInputState, MAX_CONTROL_INPUT_BYTES, MAX_CONTROL_INPUTS, SourceControlBaseline,
    },
};
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path, sync::Mutex};

/// A positively observed known-state inequality, never a classifier for I/O.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlDriftKind {
    Inputs,
    Context,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlDrift {
    pub kind: ControlDriftKind,
}
impl std::fmt::Display for ControlDrift {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str(match self.kind {
            ControlDriftKind::Inputs => "control_inputs_changed",
            ControlDriftKind::Context => "control_context_changed",
        })
    }
}
impl std::error::Error for ControlDrift {}
fn changed(kind: ControlDriftKind) -> anyhow::Error {
    ControlDrift { kind }.into()
}
/// Preserve uncertainty. Only actual NotFound is an absence observation.
fn require_absence(observed: std::io::Result<std::fs::Metadata>) -> Result<()> {
    match observed {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
        Ok(_) => Err(changed(ControlDriftKind::Inputs)),
    }
}

#[derive(Default)]
struct State {
    contexts: BTreeMap<String, Sha256Digest>,
    inputs: BTreeMap<String, ControlInputState>,
    bytes: u64,
    descriptor_bytes: usize,
    failed: bool,
}
/// One actual scan, shared across root/alias matchers and header-input capture.
/// Failure latches for the entire scan. A fresh scan creates a fresh ledger.
pub struct ControlSnapshot {
    state: Mutex<State>,
    expected: Option<SourceControlBaseline>,
}
impl ControlSnapshot {
    pub fn capture() -> Self {
        Self {
            state: Mutex::new(State::default()),
            expected: None,
        }
    }
    pub fn revalidate(expected: SourceControlBaseline) -> Result<Self> {
        expected.validate()?;
        Ok(Self {
            state: Mutex::new(State::default()),
            expected: Some(expected),
        })
    }
    /// Re-read exactly the authenticated dependency set before selected work
    /// and again before publication. No directory enumeration or document read.
    pub fn revalidate_inputs(&self) -> Result<()> {
        let outcome = self.revalidate_inputs_once();
        if outcome.is_err() {
            self.state
                .lock()
                .map_err(|_| anyhow::anyhow!("control snapshot unavailable"))?
                .failed = true;
        }
        outcome
    }
    fn revalidate_inputs_once(&self) -> Result<()> {
        let expected = self
            .expected
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("control revalidation has no authenticated baseline"))?;
        for root in expected.contexts.keys() {
            self.record_environment(Path::new(root))?;
        }
        for (path, value) in &expected.inputs {
            let path = Path::new(path);
            qualify_locator(path, matches!(value, ControlInputState::Directory))?;
            match value {
                ControlInputState::File { .. } => {
                    let bytes = match crate::read_protected_path(path, 64 << 10, false) {
                        Ok(bytes) => bytes,
                        Err(error)
                            if error
                                .downcast_ref::<std::io::Error>()
                                .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound) =>
                        {
                            if crate::source_read::missing_protected_path(path)? {
                                return Err(changed(ControlDriftKind::Inputs));
                            }
                            return Err(error);
                        }
                        Err(error) => return Err(error),
                    };
                    self.file(path, &bytes)?;
                }
                ControlInputState::Absent => {
                    require_absence(std::fs::symlink_metadata(path))?;
                    self.absent_qualified(path)?;
                }
                ControlInputState::Directory => {
                    let metadata = match std::fs::symlink_metadata(path) {
                        Ok(metadata) => metadata,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            if crate::source_read::missing_protected_path(path)? {
                                return Err(changed(ControlDriftKind::Inputs));
                            }
                            return Err(error.into());
                        }
                        Err(error) => return Err(error.into()),
                    };
                    ensure!(
                        metadata.is_dir() && !metadata.file_type().is_symlink(),
                        "control directory changed"
                    );
                    #[cfg(windows)]
                    {
                        use std::os::windows::fs::MetadataExt;
                        ensure!(
                            metadata.file_attributes() & 0x400 == 0,
                            "control directory redirected"
                        );
                    }
                    self.directory_qualified(path)?;
                }
            }
        }
        Ok(())
    }
    /// This is computed by the same owner that actually selects global files.
    /// Hashing these locators does not itself prove their content or authority.
    pub(crate) fn record_environment(&self, root: &Path) -> Result<()> {
        #[allow(deprecated)]
        let home = std::env::home_dir();
        let mut fields = vec![
            Some(root.to_path_buf()),
            Some(std::env::current_dir()?),
            home,
        ];
        for name in ["GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM", "XDG_CONFIG_HOME"] {
            fields.push(std::env::var_os(name).map(std::path::PathBuf::from));
        }
        let fields: Vec<Option<&str>> = fields
            .iter()
            .map(|path| {
                path.as_ref()
                    .map(|path| {
                        path.to_str()
                            .ok_or_else(|| anyhow::anyhow!("non-UTF8 control context"))
                    })
                    .transpose()
            })
            .collect::<Result<_>>()?;
        let digest = airk_core::identity::framed_digest(
            b"airk/protected-control-environment/1",
            &[&serde_json::to_vec(&fields)?],
        );
        self.context(
            root.to_str()
                .ok_or_else(|| anyhow::anyhow!("non-UTF8 root context"))?,
            digest,
        )
    }
    fn record(&self, path: &Path, value: ControlInputState) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("control snapshot unavailable"))?;
        ensure!(
            !state.failed,
            "control snapshot already refused; fresh review required"
        );
        let outcome = (|| -> Result<()> {
            let path = path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("non-UTF8 control locator"))?;
            ensure!(
                path.len() <= 32768 && !path.chars().any(char::is_control),
                "control locator exceeds bound"
            );
            if let Some(expected) = &self.expected {
                ensure!(
                    expected.inputs.get(path) == Some(&value),
                    ControlDrift {
                        kind: ControlDriftKind::Inputs
                    }
                );
            }
            if let Some(prior) = state.inputs.get(path) {
                ensure!(
                    *prior == value,
                    ControlDrift {
                        kind: ControlDriftKind::Inputs
                    }
                );
                return Ok(());
            }
            ensure!(
                state.inputs.len() < MAX_CONTROL_INPUTS,
                "control snapshot entry budget exceeded"
            );
            let captured = match value {
                ControlInputState::File { bytes, .. } => bytes,
                _ => 0,
            };
            let next = state
                .bytes
                .checked_add(captured)
                .ok_or_else(|| anyhow::anyhow!("control snapshot byte overflow"))?;
            ensure!(
                next <= MAX_CONTROL_INPUT_BYTES,
                "control snapshot byte budget exceeded"
            );
            let descriptor = serde_json::to_vec(&(path, &value))?
                .len()
                .checked_add(2)
                .ok_or_else(|| anyhow::anyhow!("control descriptor overflow"))?;
            let descriptor_total = state
                .descriptor_bytes
                .checked_add(descriptor)
                .ok_or_else(|| anyhow::anyhow!("control descriptor overflow"))?;
            ensure!(
                descriptor_total
                    <= airk_core::source_control::MAX_CONTROL_BASELINE_BYTES.saturating_sub(512),
                "control descriptor budget exceeded"
            );
            state.bytes = next;
            state.descriptor_bytes = descriptor_total;
            state.inputs.insert(path.into(), value);
            Ok(())
        })();
        if outcome.is_err() {
            state.failed = true;
        }
        outcome
    }
    /// Call only with bytes returned by the existing protected reader and used
    /// by the actual matcher/header selector. Never re-open them just to record.
    pub(crate) fn file(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        self.record(
            path,
            ControlInputState::File {
                sha256: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
                bytes: bytes.len() as u64,
            },
        )
    }
    /// Caller first qualifies the missing locator with the existing native
    /// no-follow/name-boundary reader. NotFound alone is not this proof.
    pub(crate) fn absent_qualified(&self, path: &Path) -> Result<()> {
        let outcome = (|| -> Result<()> {
            qualify_locator(path, false)?;
            ensure!(
                crate::source_read::missing_protected_path(path)?,
                "control absence changed or unqualified"
            );
            self.record(path, ControlInputState::Absent)
        })();
        if outcome.is_err() {
            self.state
                .lock()
                .map_err(|_| anyhow::anyhow!("control snapshot unavailable"))?
                .failed = true;
        }
        outcome
    }
    /// Used for an actual qualified .git directory marker, not directory
    /// inventory identity; the relevant info/exclude is recorded separately.
    pub(crate) fn directory_qualified(&self, path: &Path) -> Result<()> {
        self.record(path, ControlInputState::Directory)
    }
    pub(crate) fn context(&self, root: &str, digest: Sha256Digest) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("control snapshot unavailable"))?;
        ensure!(!state.failed, "control snapshot already refused");
        let outcome = (|| -> Result<()> {
            ensure!(
                !root.is_empty() && root.len() <= 32768 && !root.chars().any(char::is_control),
                "control context locator exceeds bound"
            );
            if let Some(expected) = &self.expected {
                ensure!(
                    expected.contexts.get(root) == Some(&digest),
                    ControlDrift {
                        kind: ControlDriftKind::Context
                    }
                );
            }
            if let Some(prior) = state.contexts.get(root) {
                ensure!(
                    *prior == digest,
                    ControlDrift {
                        kind: ControlDriftKind::Context
                    }
                );
            } else {
                ensure!(
                    state.contexts.len() < 17,
                    "control context root budget exceeded"
                );
                let descriptor = serde_json::to_vec(&(root, digest))?
                    .len()
                    .checked_add(2)
                    .ok_or_else(|| anyhow::anyhow!("control context descriptor overflow"))?;
                let descriptor_total = state
                    .descriptor_bytes
                    .checked_add(descriptor)
                    .ok_or_else(|| anyhow::anyhow!("control context descriptor overflow"))?;
                ensure!(
                    descriptor_total
                        <= airk_core::source_control::MAX_CONTROL_BASELINE_BYTES
                            .saturating_sub(512),
                    "control descriptor budget exceeded"
                );
                state.descriptor_bytes = descriptor_total;
                state.contexts.insert(root.into(), digest);
            }
            Ok(())
        })();
        if outcome.is_err() {
            state.failed = true;
        }
        outcome
    }
    /// Scanner calls only after successful COMPLETE discovery and extraction.
    /// Ordinary/untracked or failed/progressive partial scans never call this.
    pub fn seal_complete(
        &self,
        configuration: Sha256Digest,
        settings: Sha256Digest,
    ) -> Result<SourceControlBaseline> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("control snapshot unavailable"))?;
        ensure!(!state.failed, "cannot seal refused control snapshot");
        let baseline = SourceControlBaseline {
            schema: 1,
            // Revalidation compares the original controls; only the locked
            // CurrentUpdater may advance the publication after capture.
            publication: self
                .expected
                .as_ref()
                .and_then(|expected| expected.publication),
            configuration,
            settings,
            contexts: state.contexts.clone(),
            inputs: state.inputs.clone(),
        };
        baseline.validate()?;
        if let Some(expected) = &self.expected {
            ensure!(
                baseline == *expected,
                ControlDrift {
                    kind: ControlDriftKind::Inputs
                }
            );
        }
        Ok(baseline)
    }
}

/// Qualify every existing ancestor without following a redirect. Missing
/// suffixes are allowed only as an absence observation, rechecked before commit.
pub(crate) fn qualify_locator(path: &Path, directory: bool) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_changed_capture_latches_and_requires_fresh_snapshot() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join(".airkignore");
        std::fs::write(&path, b"first\n")?;
        let captured = crate::source_read::read(directory.path(), &path, 65536, 65537)?;
        let snapshot = ControlSnapshot::capture();
        snapshot.context("root", Sha256Digest::from_bytes([1; 32]))?;
        snapshot.file(&path, &captured)?;
        let baseline = snapshot.seal_complete(
            Sha256Digest::from_bytes([2; 32]),
            Sha256Digest::from_bytes([3; 32]),
        )?;
        std::fs::write(&path, b"second\n")?;
        let changed = crate::source_read::read(directory.path(), &path, 65536, 65537)?;
        let recheck = ControlSnapshot::revalidate(baseline)?;
        assert!(recheck.file(&path, &changed).is_err());
        std::fs::write(&path, b"first\n")?;
        assert!(
            recheck.file(&path, &captured).is_err(),
            "failure must remain latched after restoration"
        );
        let fresh = ControlSnapshot::capture();
        fresh.context("root", Sha256Digest::from_bytes([1; 32]))?;
        fresh.file(&path, &captured)?;
        assert!(
            fresh
                .seal_complete(
                    Sha256Digest::from_bytes([2; 32]),
                    Sha256Digest::from_bytes([3; 32])
                )
                .is_ok()
        );
        Ok(())
    }
    #[test]
    fn complete_revalidation_requires_absence_context_and_every_control() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let absent = directory.path().join("absent");
        let original = ControlSnapshot::capture();
        original.context("root", Sha256Digest::from_bytes([1; 32]))?;
        original.file(Path::new("present"), b"rules")?;
        original.absent_qualified(&absent)?;
        let baseline = original.seal_complete(
            Sha256Digest::from_bytes([2; 32]),
            Sha256Digest::from_bytes([3; 32]),
        )?;
        let current = ControlSnapshot::revalidate(baseline.clone())?;
        current.context("root", Sha256Digest::from_bytes([1; 32]))?;
        current.file(Path::new("present"), b"rules")?;
        assert!(
            current
                .seal_complete(baseline.configuration, baseline.settings)
                .is_err(),
            "missing absent probe is not complete"
        );
        current.absent_qualified(&absent)?;
        assert_eq!(
            current.seal_complete(baseline.configuration, baseline.settings)?,
            baseline
        );
        assert!(current.file(Path::new("new-control"), b"rules").is_err());
        assert!(
            current
                .seal_complete(baseline.configuration, baseline.settings)
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn known_content_and_context_drift_are_typed_but_permission_and_unknown_io_are_not()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().canonicalize()?;
        let path = root.join(".airkignore");
        std::fs::write(&path, b"one\n")?;
        let bytes = crate::source_read::read(&root, &path, 65536, 65537)?;
        let original = ControlSnapshot::capture();
        original.context(root.to_str().unwrap(), Sha256Digest::from_bytes([1; 32]))?;
        original.file(&path, &bytes)?;
        let baseline = original.seal_complete(
            Sha256Digest::from_bytes([2; 32]),
            Sha256Digest::from_bytes([3; 32]),
        )?;
        let inputs = ControlSnapshot::revalidate(baseline.clone())?;
        let error = inputs.file(&path, b"different\n").unwrap_err();
        assert_eq!(
            error.downcast_ref::<ControlDrift>().map(|drift| drift.kind),
            Some(ControlDriftKind::Inputs)
        );
        let context = ControlSnapshot::revalidate(baseline)?;
        let error = context
            .context(root.to_str().unwrap(), Sha256Digest::from_bytes([4; 32]))
            .unwrap_err();
        assert_eq!(
            error.downcast_ref::<ControlDrift>().map(|drift| drift.kind),
            Some(ControlDriftKind::Context)
        );
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Other,
        ] {
            let error = require_absence(Err(std::io::Error::from(kind))).unwrap_err();
            assert!(error.downcast_ref::<ControlDrift>().is_none());
            assert_eq!(error.downcast_ref::<std::io::Error>().unwrap().kind(), kind);
        }
        let error = require_absence(std::fs::symlink_metadata(&path)).unwrap_err();
        assert_eq!(
            error.downcast_ref::<ControlDrift>().map(|drift| drift.kind),
            Some(ControlDriftKind::Inputs)
        );
        require_absence(Err(std::io::Error::from(std::io::ErrorKind::NotFound)))?;
        Ok(())
    }
}
