// SPDX-License-Identifier: Apache-2.0
//! Approved external payload pins. No code path copies into or deletes from the provider's directory.
use super::*;

const POLICY: &str = "managed-runtime.json";
const DETACH: &str = "managed-detach.json";
const MAX_POLICY: u64 = 32 * 1024;

#[derive(Debug)]
struct SupervisorUnavailable;
impl std::fmt::Display for SupervisorUnavailable {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str("organization supervisor is unavailable or incompatible; AIRK will not start a competing service")
    }
}
impl std::error::Error for SupervisorUnavailable {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum ManagedLifecycle {
    External,
    UserOnDemand,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Managed {
    schema: u32,
    install: PathBuf,
    source: PathBuf,
    pub(super) approval: Approval,
    executable_sha256: String,
    pub(super) lifecycle: ManagedLifecycle,
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn read(root: &Path) -> Result<Option<Managed>> {
    if !exists(&root.join(POLICY))? {
        return Ok(None);
    }
    let policy: Managed = serde_json::from_slice(&read_bounded(&root.join(POLICY), MAX_POLICY)?)?;
    ensure!(
        policy.schema == 1 && policy.install == root.canonicalize()?,
        "managed runtime installation identity mismatch"
    );
    ensure!(
        crate::user_paths::canonical_location(&policy.source)? == policy.source
            && !policy.install.starts_with(&policy.source)
            && !policy.source.starts_with(&policy.install),
        "managed payload location is redirected or overlaps writable installation metadata"
    );
    ensure!(policy.approval.schema == 1, "unsupported managed approval");
    digest(&policy.approval.manifest_sha256)?;
    digest(&policy.executable_sha256)?;
    Ok(Some(policy))
}

pub(super) fn require_unmanaged(root: &Path) -> Result<()> {
    ensure!(
        read(root)?.is_none() && !exists(&root.join(DETACH))?,
        "managed runtime is pinned by policy; user payload installation, selection changes and login/launcher management are disabled"
    );
    Ok(())
}
pub(super) fn is_configured(root: &Path) -> Result<bool> {
    Ok(exists(&root.join(POLICY))? || exists(&root.join(DETACH))?)
}

pub(super) fn detaching(root: &Path) -> Result<bool> {
    exists(&root.join(DETACH))
}

impl Managed {
    pub(super) fn saved_executable(&self) -> PathBuf {
        self.source.join(main_name())
    }
    pub(super) fn saved_executable_sha256(&self) -> &str {
        &self.executable_sha256
    }
    pub(super) fn verify(&self) -> Result<(PathBuf, Approval)> {
        ensure!(
            self.source.canonicalize()? == self.source,
            "managed payload was redirected"
        );
        let (_, inventory) = manifest(&self.source, &self.approval.manifest_sha256)?;
        ensure!(
            inventory.version == self.approval.version
                && inventory.files[main_name()] == self.executable_sha256,
            "managed payload version or executable identity changed"
        );
        verify_topology(&self.source, &inventory, false)?;
        Ok((self.source.join(main_name()), self.approval.clone()))
    }
}

pub(super) fn verify_topology(
    source: &Path,
    inventory: &InstallManifest,
    installed: bool,
) -> Result<()> {
    use std::collections::BTreeSet;
    let mut files = inventory.files.keys().cloned().collect::<BTreeSet<_>>();
    files.insert("airk-install.json".into());
    if installed {
        files.insert("approval.json".into());
    }
    let mut directories = BTreeSet::new();
    for file in &files {
        let mut parent = Path::new(file).parent();
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            directories.insert(path.to_path_buf());
            parent = path.parent();
        }
    }
    let mut pending = vec![source.to_path_buf()];
    let mut count = 0usize;
    while let Some(parent) = pending.pop() {
        for entry in fs::read_dir(&parent)? {
            count += 1;
            ensure!(count <= 4096, "managed payload topology exceeds bound");
            let path = entry?.path();
            let relative = path.strip_prefix(source)?;
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "linked managed payload entry refused"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "redirected managed payload entry refused"
                );
            }
            if metadata.is_dir() {
                ensure!(
                    directories.contains(relative),
                    "unlisted managed payload directory refused"
                );
                pending.push(path);
            } else {
                safe_file(&path)?;
                ensure!(
                    files.contains(
                        &relative
                            .to_str()
                            .context("non-UTF-8 managed payload name")?
                            .replace('\\', "/")
                    ),
                    "unlisted managed payload file refused"
                );
            }
        }
    }
    Ok(())
}

/// The caller supplies trusted machine/user policy, never workspace executable settings.
#[allow(clippy::too_many_arguments)]
pub(super) async fn configure(
    source: &Path,
    root: &Path,
    approved: &str,
    basis: VerificationBasis,
    lifecycle: ManagedLifecycle,
    requested_state: Option<&Path>,
    channel: RuntimeChannel,
) -> Result<serde_json::Value> {
    let source = source.canonicalize()?;
    let install = crate::user_paths::canonical_location(root)?;
    let state = layout::planned_state(root, requested_state, channel)?;
    ensure!(
        !install.starts_with(&source)
            && !source.starts_with(&install)
            && !state.starts_with(&source)
            && !source.starts_with(&state),
        "managed payload must be separate from writable metadata and state"
    );
    let _lock = install_lock(root)?;
    ensure!(
        !exists(&root.join(DETACH))? && !exists(&root.join("uninstall.json"))?,
        "finish pending runtime removal before configuring a managed payload"
    );
    let (_, inventory) = manifest(&source, approved)?;
    verify_topology(&source, &inventory, false)?;
    self_test(&source.join(main_name()))?;
    let policy = Managed {
        schema: 1,
        install,
        source,
        approval: Approval {
            schema: 1,
            manifest_sha256: approved.into(),
            version: inventory.version,
            verification_basis: basis,
        },
        executable_sha256: inventory.files[main_name()].clone(),
        lifecycle,
    };
    layout::initialize(root, requested_state, channel)?;
    ensure!(
        layout::control(root)?.parent() == Some(state.as_path()),
        "state layout changed during managed approval"
    );
    if let Some(prior) = read(root)? {
        ensure!(
            prior == policy,
            "managed runtime policy is already pinned; an explicit transition is required"
        );
    } else {
        ensure!(
            !exists(&root.join("selected.json"))?
                && !exists(&root.join("startup-owner.json"))?
                && !exists(&root.join("launcher.json"))?
                && !exists(&launcher_path(root))?
                && staging::pending(root)?.is_empty(),
            "managed pin requires a fresh reviewed installation; existing runtime work is preserved"
        );
        let runtimes = root.join("runtimes");
        ensure!(
            !exists(&runtimes)? || fs::read_dir(&runtimes)?.next().transpose()?.is_none(),
            "staged user runtimes require explicit removal before managed configuration"
        );
        ensure!(
            !exists(&layout::control(root)?)?,
            "managed pin cannot adopt an existing service ledger implicitly"
        );
        ensure!(
            serde_json::to_vec(&policy)?.len() as u64 <= MAX_POLICY,
            "managed runtime policy exceeds bound"
        );
        replace_json(root, POLICY, &policy)?;
    }
    match ensure(root).await {
        Ok(observation) => Ok(
            serde_json::json!({"managed":true,"selected":true,"policy":policy,"health":observation}),
        ),
        Err(error) if error.downcast_ref::<SupervisorUnavailable>().is_some() => Ok(
            serde_json::json!({"managed":true,"selected":false,"awaiting_external_supervisor":true,"policy":policy,"control_dir":layout::control(root)?,"health_error":format!("{error:#}").chars().take(512).collect::<String>()}),
        ),
        Err(error) => Err(error),
    }
}

/// Caller holds the installation lock. Public selection commits only after exact native health.
pub(super) async fn ensure(root: &Path) -> Result<ServiceHealth> {
    ensure!(!exists(&root.join(DETACH))?, "managed detach is pending");
    let policy = read(root)?.context("managed runtime policy missing")?;
    let (main, approval) = policy.verify()?;
    if exists(&root.join("selected.json"))? {
        let selection: Selection =
            serde_json::from_slice(&read_bounded(&root.join("selected.json"), 4096)?)?;
        ensure!(
            selection.schema == 1
                && selection.active == approval.manifest_sha256
                && selection.previous.is_none(),
            "managed selection differs from its approved pin"
        );
    }
    let observation = match policy.lifecycle {
        ManagedLifecycle::External => health(root, &approval, &policy.executable_sha256)
            .await
            .context(SupervisorUnavailable)?,
        ManagedLifecycle::UserOnDemand => start(root, &approval, &main, true).await?,
    };
    if !exists(&root.join("selected.json"))? {
        replace_json(
            root,
            "selected.json",
            &Selection {
                schema: 1,
                active: approval.manifest_sha256,
                previous: None,
            },
        )?;
    }
    Ok(observation)
}

pub(super) async fn detach(root: &Path) -> Result<serde_json::Value> {
    let intent_path = root.join(DETACH);
    let policy: Managed = if exists(&intent_path)? {
        serde_json::from_slice(&read_bounded(&intent_path, MAX_POLICY)?)?
    } else {
        let policy = read(root)?.context("managed policy missing")?;
        replace_json(root, DETACH, &policy)?;
        policy
    };
    ensure!(
        policy.schema == 1 && policy.install == root.canonicalize()?,
        "managed detach ownership mismatch"
    );
    digest(&policy.approval.manifest_sha256)?;
    digest(&policy.executable_sha256)?;
    if let Some(current) = read(root)? {
        ensure!(
            current == policy,
            "managed policy changed during detach; preserving it"
        );
    }
    let _quiescence = if policy.lifecycle == ManagedLifecycle::UserOnDemand {
        // Missing provider files must not prevent authenticated shutdown of the
        // exact still-running approved process, whose identity was saved at pinning.
        drain_identity(root, &policy.approval, &policy.executable_sha256).await?;
        Some(quiescent(root)?)
    } else {
        None
    };
    let selection_path = root.join("selected.json");
    if exists(&selection_path)? {
        let selection: Selection = serde_json::from_slice(&read_bounded(&selection_path, 4096)?)?;
        ensure!(
            selection.schema == 1
                && selection.active == policy.approval.manifest_sha256
                && selection.previous.is_none(),
            "changed managed selection preserved"
        );
        fs::remove_file(selection_path)?;
    }
    if exists(&root.join(POLICY))? {
        fs::remove_file(root.join(POLICY))?;
    }
    fs::remove_file(intent_path)?;
    sync_directory(root)?;
    Ok(
        serde_json::json!({"detached":true,"managed_payload_preserved":true,"service_state_preserved":true,"external_supervisor_continues":policy.lifecycle==ManagedLifecycle::External}),
    )
}

#[cfg(test)]
#[path = "runtime_managed_tests.rs"]
mod tests;
