// SPDX-License-Identifier: Apache-2.0
//! One registry namespace owns a 1.4 data root, independently of legacy storage.
use airk_core::identity::Incarnation;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub(crate) const OWNER: &str = "service-data-1.4.owner.json";
pub(crate) const PART: &str = "service-data-1.4.owner.part";
pub(crate) const LOCK: &str = "service-data-1.4.lock";
const MAX_BYTES: u64 = 16384;
#[derive(Debug, Clone, Copy)]
pub struct ForeignDataRoot;
impl std::fmt::Display for ForeignDataRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("1.4 data root belongs to another service registry; choose its original service or a fresh data root")
    }
}
impl std::error::Error for ForeignDataRoot {}
#[derive(Debug, Clone, Copy)]
pub struct UnboundDataRoot;
impl std::fmt::Display for UnboundDataRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("unbound existing 1.4 collections require their original development runtime or a fresh data root; no implicit adoption")
    }
}
impl std::error::Error for UnboundDataRoot {}
/// An existing storage root that other local accounts can reach. AIRK never
/// changes permissions of a directory it did not create.
#[derive(Debug, Clone)]
pub struct StorageNotPrivate(pub String);
impl std::fmt::Display for StorageNotPrivate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "storage_not_private: {}; choose a private directory owned by this account or restrict it yourself (for example `chmod 700`); permissions are not changed automatically",
            self.0
        )
    }
}
impl std::error::Error for StorageNotPrivate {}

/// Indexes hold source-derived text, so on Unix an existing root must be
/// shielded from other accounts: the root or one of its ancestors is owned by
/// this user and grants no group/other access. POSIX ACLs and Windows ACLs are
/// not evaluated here. Missing roots are accepted; creation uses mode 0700.
#[cfg(unix)]
pub fn require_private(data: &Path) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    if !exists(data)? {
        return Ok(());
    }
    let canonical = data.canonicalize()?;
    // SAFETY: geteuid takes no pointers and has no failure mode.
    let uid = unsafe { libc::geteuid() };
    for ancestor in canonical.ancestors() {
        let metadata = fs::metadata(ancestor)?;
        if metadata.uid() == uid && metadata.permissions().mode() & 0o077 == 0 {
            return Ok(());
        }
    }
    let metadata = fs::metadata(&canonical)?;
    Err(StorageNotPrivate(format!(
        "{} (owner uid {}, mode {:o}) and its ancestors allow other accounts to read stored evidence",
        canonical.display(),
        metadata.uid(),
        metadata.permissions().mode() & 0o7777
    ))
    .into())
}
/// Windows storage relies on the inherited profile ACL and is not evaluated here.
#[cfg(not(unix))]
pub fn require_private(_data: &Path) -> Result<()> {
    Ok(())
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Scope {
    version: u32,
    data: PathBuf,
    control: PathBuf,
    namespace: Incarnation,
}
#[derive(Clone)]
pub struct BoundDataRoot(Scope);
impl BoundDataRoot {
    /// Recheck the saved authority before every native query or job.
    pub fn check(&self) -> Result<()> {
        ensure!(
            exists(&self.0.data.join(OWNER))?,
            "service data ownership receipt is missing"
        );
        check(&self.0.data, &self.0.control, self.0.namespace)
    }
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}
/// Resolve existing ancestors without creating a directory or accepting traversal.
pub fn canonical_location(path: &Path) -> Result<PathBuf> {
    ensure!(
        path.is_absolute()
            && path.to_str().is_some_and(|value| value.len() <= 4096)
            && !path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
        "service data location must be absolute without traversal"
    );
    let mut prefix = path.to_path_buf();
    let mut suffix = Vec::new();
    while !exists(&prefix)? {
        suffix.push(
            prefix
                .file_name()
                .context("service data ancestor missing")?
                .to_owned(),
        );
        ensure!(prefix.pop(), "service data ancestor missing");
    }
    ensure!(prefix.is_dir(), "service data ancestor is not a directory");
    let mut value = prefix.canonicalize()?;
    for name in suffix.into_iter().rev() {
        value.push(name);
    }
    ensure!(
        value
            .to_str()
            .is_some_and(|v| v.len() <= 4096 && !v.chars().any(char::is_control)),
        "invalid service data location"
    );
    Ok(value)
}
fn expected(data: &Path, control: &Path, namespace: Incarnation) -> Result<Scope> {
    ensure!(
        namespace != Incarnation::from_bytes([0; 16]),
        "empty service registry identity"
    );
    let data = canonical_location(data)?;
    let control = canonical_location(control)?;
    ensure!(
        !data.starts_with(&control) && !control.starts_with(&data),
        "service data and control roots must be disjoint"
    );
    Ok(Scope {
        version: 1,
        data,
        control,
        namespace,
    })
}
fn read(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    crate::managed_collection::reject_file_aliases(path, &metadata)?;
    ensure!(
        metadata.len() <= MAX_BYTES,
        "service data ownership exceeds bound"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "service data ownership grew beyond bound"
    );
    Ok(bytes)
}
/// Metadata-only registration check. Missing roots are neither created nor opened.
pub fn check(data: &Path, control: &Path, namespace: Incarnation) -> Result<()> {
    let path = data.join(OWNER);
    if !exists(&path)? {
        return fresh_namespace(data);
    }
    let expected = expected(data, control, namespace)?;
    let bytes = read(&path)?;
    let actual: Scope = serde_json::from_slice(&bytes)?;
    ensure!(actual == expected, ForeignDataRoot);
    ensure!(
        serde_json::to_vec(&actual)? == bytes,
        "changed service data ownership encoding"
    );
    if exists(&data.join(PART))? {
        ensure!(
            bytes.starts_with(&read(&data.join(PART))?),
            "unknown service data claim partial preserved"
        );
    }
    Ok(())
}

pub(crate) fn relocated_owner(
    data: &Path,
    destination: &Path,
    control: &Path,
    namespace: Incarnation,
) -> Result<(Vec<u8>, Vec<u8>)> {
    check(data, control, namespace)?;
    let before = read(&data.join(OWNER))?;
    let after = serde_json::to_vec(&expected(destination, control, namespace)?)?;
    Ok((before, after))
}
fn fresh_namespace(data: &Path) -> Result<()> {
    let directory = data.join("collections-1.4");
    if exists(&directory)? {
        let metadata = fs::symlink_metadata(&directory)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "unowned 1.4 collection namespace preserved"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "unowned redirected 1.4 collection namespace preserved"
            );
        }
        ensure!(
            fs::read_dir(&directory)?.next().transpose()?.is_none(),
            UnboundDataRoot
        );
    }
    Ok(())
}
/// Run only after native project activation, before opening any collection.
/// Legacy collections/models/settings remain untouched. Scope is immutable.
pub fn bind(data: &Path, control: &Path, namespace: Incarnation) -> Result<BoundDataRoot> {
    let scope = expected(data, control, namespace)?;
    check(&scope.data, &scope.control, namespace)?;
    if exists(&scope.data.join(OWNER))? {
        // Existing projects on shared roots fail visibly before any collection opens.
        require_private(&scope.data)?;
        return Ok(BoundDataRoot(scope));
    }
    fresh_namespace(&scope.data)?;
    require_private(&scope.data)?;
    {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&scope.data)?;
    }
    require_private(&scope.data)?;
    ensure!(
        scope.data.canonicalize()? == scope.data,
        "service data root redirected during claim"
    );
    let _lock = super::acquire_file(&scope.data, LOCK)?;
    check(&scope.data, &scope.control, namespace)?;
    if exists(&scope.data.join(OWNER))? {
        return Ok(BoundDataRoot(scope));
    }
    fresh_namespace(&scope.data)?;
    let bytes = serde_json::to_vec(&scope)?;
    ensure!(
        bytes.len() as u64 <= MAX_BYTES,
        "service data ownership exceeds bound"
    );
    let partial = scope.data.join(PART);
    let present = exists(&partial)?;
    if present {
        ensure!(
            bytes.starts_with(&read(&partial)?),
            "unknown service data claim partial preserved"
        );
    }
    let quota = crate::quota::Quota::open_service(&scope.data)?;
    let reservation = quota.reserve_root(bytes.len() as u64)?;
    reservation.charge(bytes.len() as u64)?;
    let mut options = OpenOptions::new();
    options.write(true);
    if present {
        options.truncate(true);
    } else {
        options.create_new(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&partial)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(partial, scope.data.join(OWNER))?;
    crate::maintenance::sync_parent(&scope.data)?;
    Ok(BoundDataRoot(scope))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Result<(tempfile::TempDir, PathBuf, PathBuf, PathBuf)> {
        let root = tempfile::tempdir()?;
        let base = root.path().canonicalize()?;
        let data = base.join("data");
        let first = base.join("first-control");
        let second = base.join("second-control");
        for path in [&data, &first, &second] {
            fs::create_dir(path)?;
        }
        Ok((root, data, first, second))
    }
    #[test]
    fn scope_survives_restart_and_rejects_other_control_or_recreated_registry() -> Result<()> {
        let (_root, data, first, second) = fixture()?;
        let namespace = Incarnation::from_bytes([1; 16]);
        let proof = bind(&data, &first, namespace)?;
        let original = fs::read(data.join(OWNER))?;
        bind(&data, &first, namespace)?.check()?;
        assert!(
            check(&data, &second, namespace)
                .unwrap_err()
                .is::<ForeignDataRoot>()
        );
        let error = match bind(&data, &first, Incarnation::from_bytes([2; 16])) {
            Ok(_) => anyhow::bail!("foreign namespace claimed data"),
            Err(error) => error,
        };
        assert!(error.is::<ForeignDataRoot>());
        assert_eq!(fs::read(data.join(OWNER))?, original);
        fs::remove_file(data.join(OWNER))?;
        assert!(proof.check().is_err());
        fs::write(data.join(OWNER), &original)?;
        proof.check()?;
        Ok(())
    }
    #[test]
    fn fresh_binding_preserves_legacy_files_and_quota_ownership() -> Result<()> {
        let (_root, data, control, _second) = fixture()?;
        let legacy = data.join("collections-1.2/original");
        fs::create_dir_all(&legacy)?;
        fs::write(legacy.join("user-fixture.txt"), b"legacy original")?;
        let quota = crate::quota::Quota::open(&data)?;
        let before = quota.status()?.used_bytes;
        bind(&data, &control, Incarnation::from_bytes([1; 16]))?.check()?;
        assert_eq!(quota.status()?.used_bytes, before);
        assert_eq!(
            fs::read(legacy.join("user-fixture.txt"))?,
            b"legacy original"
        );
        assert!(!data.join("collections-1.4").exists());
        Ok(())
    }
    #[test]
    fn unbound_populated_state_and_unknown_partials_are_preserved_without_adoption() -> Result<()> {
        let (_root, data, control, _second) = fixture()?;
        let original = data.join("collections-1.4/old-development");
        fs::create_dir_all(&original)?;
        fs::write(original.join("saved.txt"), b"retained original")?;
        let ns = Incarnation::from_bytes([1; 16]);
        assert!(bind(&data, &control, ns).is_err());
        assert!(!data.join(OWNER).exists() && !data.join(LOCK).exists());
        assert_eq!(fs::read(original.join("saved.txt"))?, b"retained original");
        let other = data.parent().unwrap().join("fresh-data");
        fs::create_dir(&other)?;
        fs::write(other.join(PART), b"preserve foreign intent")?;
        assert!(bind(&other, &control, ns).is_err());
        assert_eq!(fs::read(other.join(PART))?, b"preserve foreign intent");
        assert!(!other.join(".quota-1.4").exists());
        Ok(())
    }
    #[test]
    fn matching_interrupted_claim_resumes_and_aliased_or_oversized_receipts_fail() -> Result<()> {
        let (root, data, control, _second) = fixture()?;
        let ns = Incarnation::from_bytes([1; 16]);
        let bytes = serde_json::to_vec(&expected(&data, &control, ns)?)?;
        fs::write(data.join(PART), &bytes[..bytes.len() / 2])?;
        bind(&data, &control, ns)?.check()?;
        assert!(!data.join(PART).exists());
        assert_eq!(fs::read(data.join(OWNER))?, bytes);
        let alias = root.path().join("scope-alias");
        fs::hard_link(data.join(OWNER), &alias)?;
        assert!(check(&data, &control, ns).is_err());
        fs::remove_file(alias)?;
        fs::write(data.join(OWNER), vec![b' '; MAX_BYTES as usize + 1])?;
        assert!(check(&data, &control, ns).is_err());
        Ok(())
    }
    #[test]
    fn registration_check_never_creates_missing_storage() -> Result<()> {
        let (_root, data, control, _second) = fixture()?;
        let missing = data.join("not-created");
        check(&missing, &control, Incarnation::from_bytes([1; 16]))?;
        assert!(!missing.exists());
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn dangling_claim_never_creates_foreign_target() -> Result<()> {
        let (_root, data, control, _second) = fixture()?;
        let foreign = data.parent().unwrap().join("foreign");
        std::os::unix::fs::symlink(&foreign, data.join(PART))?;
        assert!(bind(&data, &control, Incarnation::from_bytes([1; 16])).is_err());
        assert!(!foreign.exists());
        Ok(())
    }
    #[test]
    fn racing_first_claims_elect_one_registry_without_overwriting_ownership() -> Result<()> {
        let (_root, data, first, second) = fixture()?;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let mut claims = Vec::new();
        for (control, namespace) in [
            (first, Incarnation::from_bytes([1; 16])),
            (second, Incarnation::from_bytes([2; 16])),
        ] {
            let data = data.clone();
            let barrier = barrier.clone();
            claims.push(std::thread::spawn(move || {
                barrier.wait();
                bind(&data, &control, namespace)
            }));
        }
        let mut winners = 0;
        for claim in claims {
            if let Ok(proof) = claim.join().unwrap() {
                proof.check()?;
                winners += 1;
            }
        }
        assert_eq!(winners, 1);
        assert!(!data.join("collections-1.4").exists());
        Ok(())
    }
}
