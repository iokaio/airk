// SPDX-License-Identifier: Apache-2.0
//! Synthetic source bytes only. No config archives or writer consumers exist.
use airk_core::{Snapshot, identity::Sha256Digest, verification::SourceCheckState};
use airk_ingest::{inventory, source_scope::SelectedSourcePolicy};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const CONTENT: &[u8] = b"pub fn hidden_archive_secret() {}\n";

struct Fixture {
    _temp: tempfile::TempDir,
    base: PathBuf,
    archive: PathBuf,
    data: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self> {
        let temp = tempfile::tempdir()?;
        let base = temp.path().canonicalize()?;
        let archive = base.join(".airk/agent-edits/v1/01010101010101010101010101010101");
        fs::create_dir_all(&archive)?;
        let data = base.join("data");
        fs::create_dir(&data)?;
        fs::write(archive.join("before.rs"), CONTENT)?;
        // Actual fixed archive roles, using synthetic non-sensitive payloads.
        fs::write(archive.join("before"), CONTENT)?;
        fs::write(archive.join("prepared"), CONTENT)?;
        fs::write(archive.join("receipt.json"), b"{\"private_fixture\":true}")?;
        fs::write(base.join("visible.rs"), "pub fn visible() {}\n")?;
        fs::create_dir(base.join(".airk-neighbor"))?;
        fs::write(
            base.join(".airk-neighbor/visible.rs"),
            "pub fn nearby() {}\n",
        )?;
        Ok(Self {
            _temp: temp,
            base,
            archive,
            data,
        })
    }

    fn empty(&self, root: &Path) -> Snapshot {
        Snapshot::empty(root.to_string_lossy().into_owned())
    }

    fn refuse_root(&self, root: &Path) {
        let mut emitted = Vec::new();
        let result = airk_ingest::scan_streaming(
            root,
            &self.data,
            &self.empty(root),
            &BTreeMap::new(),
            |artifact, _bytes| {
                emitted.push(artifact.path.clone());
                Ok(artifact.clone())
            },
        );
        assert!(
            result.is_err(),
            "an explicitly selected reserved root must refuse before scanning"
        );
        assert!(
            emitted.is_empty(),
            "no source payload may escape a reserved root"
        );
    }
}

#[test]
fn retained_archive_ordinary_discovery_omits_private_payloads_and_preserves_nearby_sources()
-> Result<()> {
    let f = Fixture::new()?;
    let (snapshot, _) = airk_ingest::scan(&f.base, &f.data, &f.empty(&f.base))?;
    let paths: Vec<_> = snapshot.artifacts.keys().map(String::as_str).collect();
    assert_eq!(paths, [".airk-neighbor/visible.rs", "visible.rs"]);
    Ok(())
}

#[test]
fn retained_archive_explicit_airk_root_refuses_before_source_payloads() -> Result<()> {
    let f = Fixture::new()?;
    f.refuse_root(&f.base.join(".airk"));
    Ok(())
}

#[test]
fn retained_archive_explicit_version_root_refuses_before_source_payloads() -> Result<()> {
    let f = Fixture::new()?;
    f.refuse_root(&f.base.join(".airk/agent-edits/v1"));
    Ok(())
}

#[test]
fn retained_archive_explicit_transaction_root_refuses_before_source_payloads() -> Result<()> {
    let f = Fixture::new()?;
    f.refuse_root(&f.archive);
    Ok(())
}

#[test]
fn retained_archive_selected_source_policy_cannot_rebase_away_private_ancestry() -> Result<()> {
    let f = Fixture::new()?;
    let mut inside = SelectedSourcePolicy::new(&f.archive, &f.data);
    assert!(
        !inside.allows("before.rs")?,
        "root rebasing cannot erase the private ancestor"
    );
    let mut ordinary = SelectedSourcePolicy::new(&f.base, &f.data);
    assert!(!ordinary.allows(".airk/agent-edits/v1/01010101010101010101010101010101/before.rs")?);
    assert!(ordinary.allows(".airk-neighbor/visible.rs")?);
    Ok(())
}

#[test]
fn retained_archive_direct_selected_source_read_refuses() -> Result<()> {
    let f = Fixture::new()?;
    assert!(airk_ingest::read_selected_source(&f.archive, &f.data, "before.rs").is_err());
    assert!(
        airk_ingest::read_selected_source(
            &f.base,
            &f.data,
            ".airk/agent-edits/v1/01010101010101010101010101010101/before.rs"
        )
        .is_err()
    );
    assert!(!airk_ingest::read_selected_source(&f.base, &f.data, "visible.rs")?.is_empty());
    Ok(())
}

#[test]
fn retained_archive_protected_administrative_capture_refuses_even_when_caller_authorizes()
-> Result<()> {
    let f = Fixture::new()?;
    assert!(
        airk_ingest::capture_input(&f.archive.join("before.rs"), 4096, false, &|_| Ok(())).is_err()
    );
    // Generic explicit administrative input keeps its existing authority outside
    // this archive subtree, including unrelated legacy .airk assets.
    fs::create_dir(f.base.join(".airk/models"))?;
    let legacy = f.base.join(".airk/models/explicit.txt");
    fs::write(&legacy, b"explicit legacy input")?;
    assert_eq!(
        airk_ingest::capture_input(&legacy, 4096, false, &|_| Ok(()))?.1,
        b"explicit legacy input"
    );
    Ok(())
}

#[test]
fn retained_archive_direct_protected_path_read_refuses() -> Result<()> {
    let f = Fixture::new()?;
    assert!(airk_ingest::read_protected_path(&f.archive.join("before.rs"), 4096, false).is_err());
    assert!(!airk_ingest::read_protected_path(&f.base.join("visible.rs"), 4096, false)?.is_empty());
    Ok(())
}

#[test]
fn retained_archive_saved_source_verification_refuses_without_reading_or_hashing_private_bytes()
-> Result<()> {
    let f = Fixture::new()?;
    let expected = Sha256Digest::from_bytes(Sha256::digest(CONTENT).into());
    let check = airk_ingest::verify_saved_source(
        &f.archive,
        Path::new("before.rs"),
        expected,
        4096,
        Instant::now() + Duration::from_secs(5),
        &|| false,
    );
    assert_eq!(check.state, SourceCheckState::OutOfScope);
    assert_eq!(
        check.bytes_read, 0,
        "private saved-source hashes are not public evidence"
    );
    Ok(())
}

#[test]
fn retained_archive_inventory_alias_cannot_reintroduce_private_roots_or_hashes() -> Result<()> {
    let f = Fixture::new()?;
    let roots = BTreeMap::from([("private".into(), f.archive.to_string_lossy().into_owned())]);
    assert!(
        inventory::scan_roots(
            &f.base,
            &f.data,
            &f.empty(&f.base),
            &roots,
            None,
            inventory::InventoryLimits::default()
        )
        .is_err(),
        "reserved inventory roots must refuse rather than contribute hashes or counts"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn retained_archive_canonical_alias_capture_cannot_hide_private_ancestry() -> Result<()> {
    let f = Fixture::new()?;
    let alias = f.base.join("ordinary-alias");
    std::os::unix::fs::symlink(&f.archive, &alias)?;
    assert!(
        airk_ingest::capture_input(&alias.join("before.rs"), 4096, false, &|_| Ok(())).is_err()
    );
    Ok(())
}

#[test]
fn retained_archive_fixed_artifact_roles_refuse_all_direct_capture_routes() -> Result<()> {
    let f = Fixture::new()?;
    for role in ["before", "prepared", "receipt.json"] {
        let path = f.archive.join(role);
        assert!(
            airk_ingest::capture_input(&path, 4096, false, &|_| Ok(())).is_err(),
            "capture {role}"
        );
        assert!(
            airk_ingest::read_protected_path(&path, 4096, false).is_err(),
            "protected {role}"
        );
        assert!(
            airk_ingest::read_selected_source(&f.archive, &f.data, role).is_err(),
            "selected {role}"
        );
        let expected = Sha256Digest::from_bytes(Sha256::digest(fs::read(&path)?).into());
        let check = airk_ingest::verify_saved_source(
            &f.archive,
            Path::new(role),
            expected,
            4096,
            Instant::now() + Duration::from_secs(5),
            &|| false,
        );
        assert_eq!(check.state, SourceCheckState::OutOfScope, "verify {role}");
        assert_eq!(check.bytes_read, 0);
        assert_eq!(check.observed_hash, None);
    }
    Ok(())
}

#[test]
fn retained_archive_protected_capture_keeps_authorization_before_resolution() -> Result<()> {
    let f = Fixture::new()?;
    let mut observed = false;
    let authorization = std::cell::Cell::new(false);
    let result = airk_ingest::capture_input(&f.archive.join("prepared"), 4096, false, &|_| {
        authorization.set(true);
        anyhow::bail!("independent authority refusal")
    });
    if let Err(error) = result {
        observed = error.to_string() == "independent authority refusal";
    }
    assert!(authorization.get());
    assert!(
        observed,
        "archive fencing never bypasses the caller's authorization boundary"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn retained_archive_windows_native_dot_alias_is_excluded_after_metadata_resolution() -> Result<()> {
    let f = Fixture::new()?;
    let base = f
        .base
        .to_str()
        .unwrap()
        .strip_prefix(r"\\?\")
        .unwrap_or(f.base.to_str().unwrap());
    let alias = Path::new(base).join(".airk./agent-edits/v1/01010101010101010101010101010101");
    assert_eq!(
        alias.canonicalize()?,
        f.archive,
        "observe an actual Win32 name alias before the capture test"
    );
    assert!(airk_ingest::read_protected_path(&alias.join("prepared"), 4096, false).is_err());
    let expected = Sha256Digest::from_bytes(Sha256::digest(CONTENT).into());
    let check = airk_ingest::verify_saved_source(
        &alias,
        Path::new("prepared"),
        expected,
        4096,
        Instant::now() + Duration::from_secs(5),
        &|| false,
    );
    assert_eq!(check.state, SourceCheckState::OutOfScope);
    assert_eq!(check.bytes_read, 0);
    Ok(())
}
