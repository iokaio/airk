// SPDX-License-Identifier: Apache-2.0
//! Approved external payload pins. No code path copies into or deletes from the provider's directory.
use super::*;

const POLICY: &str = "managed-runtime.json";
const DETACH: &str = "managed-detach.json";
const MAX_POLICY: u64 = 32 * 1024;
#[path = "runtime_managed_catalog.rs"]
pub(super) mod catalog;

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

pub(super) fn saved_policy_for_lifecycle(root: &Path) -> Result<Option<Managed>> {
    catalog::saved_policy(root)
}

pub(super) fn require_unmanaged(root: &Path) -> Result<()> {
    ensure!(
        read(root)?.is_none()
            && !exists(&root.join(DETACH))?
            && layout::managed_origin(root)?.is_none(),
        "managed runtime is pinned by policy; user payload installation, selection changes and login/launcher management are disabled"
    );
    Ok(())
}
pub(super) fn is_configured(root: &Path) -> Result<bool> {
    Ok(exists(&root.join(POLICY))?
        || exists(&root.join(DETACH))?
        || layout::managed_origin(root)?.is_some())
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
    pub(super) fn metadata_approval(&self) -> Result<Approval> {
        ensure!(
            self.source.canonicalize()? == self.source,
            "managed payload was redirected"
        );
        let inventory = approved_inventory_metadata(&self.source, &self.approval)?;
        ensure!(
            inventory.version == self.approval.version
                && inventory.files[main_name()] == self.executable_sha256,
            "managed payload version or executable identity changed"
        );
        Ok(self.approval.clone())
    }
    #[cfg(test)]
    pub(super) fn verify(&self) -> Result<(PathBuf, Approval)> {
        let (main, approval, _) = self.verify_inventory()?;
        Ok((main, approval))
    }
    pub(super) fn verify_inventory(&self) -> Result<(PathBuf, Approval, InstallManifest)> {
        self.verify_inventory_checked(&|| Ok(()))
    }
    pub(super) fn verify_inventory_checked(
        &self,
        check: &dyn Fn() -> Result<()>,
    ) -> Result<(PathBuf, Approval, InstallManifest)> {
        check()?;
        ensure!(
            self.source.canonicalize()? == self.source,
            "managed payload was redirected"
        );
        let (_, inventory) = manifest_checked(&self.source, &self.approval.manifest_sha256, check)?;
        ensure!(
            inventory.version == self.approval.version
                && inventory.files[main_name()] == self.executable_sha256,
            "managed payload version or executable identity changed"
        );
        verify_topology_checked(&self.source, &inventory, false, check)?;
        Ok((
            self.source.join(main_name()),
            self.approval.clone(),
            inventory,
        ))
    }
}

pub(super) fn verify_topology(
    source: &Path,
    inventory: &InstallManifest,
    installed: bool,
) -> Result<()> {
    verify_topology_checked(source, inventory, installed, &|| Ok(()))
}

pub(super) fn verify_topology_checked(
    source: &Path,
    inventory: &InstallManifest,
    installed: bool,
    check: &dyn Fn() -> Result<()>,
) -> Result<()> {
    use std::collections::BTreeSet;
    check()?;
    let mut approved_files = inventory.files.clone();
    if installed {
        crate::process_roles::inventory(&mut approved_files);
    }
    let mut files = approved_files.keys().cloned().collect::<BTreeSet<_>>();
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
        check()?;
        for entry in fs::read_dir(&parent)? {
            check()?;
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
                let name = relative
                    .to_str()
                    .context("non-UTF-8 managed payload name")?
                    .replace('\\', "/");
                if !inventory.files.contains_key(&name)
                    && let Some(expected) = approved_files.get(&name)
                {
                    ensure!(
                        runtime_pair::digest_file_checked(&path, check)? == *expected,
                        "modified process alias refused"
                    );
                }
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
    let lock = install_lock(root)?;
    require_manager_change_settled(root)?;
    let authority = InstallerAuthority { root, _lock: &lock };
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
    let prior = read(root)?;
    let reserved = layout::managed_origin(root)?;
    if let Some(prior) = &prior {
        ensure!(
            prior == &policy,
            "managed runtime policy is already pinned; an explicit transition is required"
        );
    }
    let enabled = airk_store::service::registry::catalog::upgrade::PRODUCTION_ACTIVATION_ENABLED;
    if reserved.is_some() && !enabled {
        layout::require_managed_ready(root)?;
    }
    let reservation = if enabled && (prior.is_none() || reserved.is_some()) {
        // Positive first-pin checks precede the INITIAL layout intent. An
        // existing reservation resumes those facts; absent policy alone cannot.
        if reserved.is_none() {
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
                !exists(&state.join("control"))?,
                "managed pin cannot adopt an existing service ledger implicitly"
            );
        }
        Some(catalog::VerifiedManagedReservation::prepare(
            &authority, &policy, &state,
        )?)
    } else {
        None
    };
    if let Some(reservation) = &reservation {
        layout::initialize_managed(root, requested_state, channel, reservation)?;
    } else {
        layout::initialize(root, requested_state, channel)?;
    }
    ensure!(
        layout::saved_control(root)?.parent() == Some(state.as_path()),
        "state layout changed during managed approval"
    );
    if prior.is_none() {
        if let Some(reservation) = &reservation {
            catalog::admit_initial(&authority, &policy, reservation)?;
        } else {
            // Preserve the disabled/old None path, including its strict refusal
            // to adopt any preexisting control directory.
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
        }
        ensure!(
            serde_json::to_vec(&policy)?.len() as u64 <= MAX_POLICY,
            "managed runtime policy exceeds bound"
        );
        replace_json(root, POLICY, &policy)?;
    }
    match ensure_running_locked(&authority).await {
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
#[cfg(test)]
async fn ensure(root: &Path) -> Result<ServiceHealth> {
    let lock = install_lock(root)?;
    let authority = InstallerAuthority { root, _lock: &lock };
    ensure_checked(&authority, None, || Ok(())).await
}

pub(super) async fn ensure_checked(
    authority: &InstallerAuthority<'_>,
    cancel: Option<&setup::SetupCancellation>,
    after_settlement: impl FnOnce() -> Result<()>,
) -> Result<ServiceHealth> {
    let root = authority.root;
    if let Some(cancel) = cancel {
        cancel.checkpoint()?;
    }
    ensure!(!exists(&root.join(DETACH))?, "managed detach is pending");
    let policy = read(root)?.context("managed runtime policy missing")?;
    let check = || cancel.map_or(Ok(()), setup::SetupCancellation::checkpoint);
    let (main, approval, _) = policy.verify_inventory_checked(&check)?;
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
    catalog::before_start(authority, &policy, cancel)?;
    after_settlement()?;
    let observation = async {
        match policy.lifecycle {
            ManagedLifecycle::External => health(root, &approval, &policy.executable_sha256)
                .await
                .context(SupervisorUnavailable),
            ManagedLifecycle::UserOnDemand => {
                start_with_digest_checked(root, &approval, &main, true, None, cancel).await
            }
        }
    };
    publish_after_observation(root, &approval, cancel, observation).await
}

// Shared actual post-health publication boundary; no selection is published
// from an observation completed after session admission has closed.
async fn publish_after_observation<T>(
    root: &Path,
    approval: &Approval,
    cancel: Option<&setup::SetupCancellation>,
    observation: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    let observation = observation.await?;
    if !exists(&root.join("selected.json"))? {
        let publish = || {
            replace_json(
                root,
                "selected.json",
                &Selection {
                    schema: 1,
                    active: approval.manifest_sha256.clone(),
                    previous: None,
                },
            )
        };
        if let Some(cancel) = cancel {
            cancel.effect(publish)?;
        } else {
            publish()?;
        }
    }
    Ok(observation)
}

#[cfg(test)]
mod setup_tests {
    use super::*;

    #[tokio::test]
    async fn external_health_completed_after_close_cannot_publish_selection() -> Result<()> {
        let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
        let (_, approval) = verify_runtime(&root, &identity)?;
        let cancel = setup::SetupCancellation::default();
        let (release, released) = tokio::sync::oneshot::channel::<()>();
        let mut observation = Box::pin(publish_after_observation(
            &root,
            &approval,
            Some(&cancel),
            async {
                released.await?;
                Ok(())
            },
        ));
        std::future::poll_fn(|cx| {
            assert!(std::future::Future::poll(observation.as_mut(), cx).is_pending());
            std::task::Poll::Ready(())
        })
        .await;
        cancel.close();
        release.send(()).expect("external health waiter retained");
        assert!(observation.await.is_err());
        assert!(!root.join("selected.json").exists());
        assert!(!root.join(format!("startup-{identity}.json")).exists());
        Ok(())
    }

    #[tokio::test]
    async fn accepted_observation_publishes_exact_selection_without_starting() -> Result<()> {
        let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
        let (_, approval) = verify_runtime(&root, &identity)?;
        let cancel = setup::SetupCancellation::default();
        publish_after_observation(&root, &approval, Some(&cancel), async { Ok(()) }).await?;
        assert_eq!(selection_record(&root)?.active, identity);
        assert!(!root.join("bootstrap.json").exists());
        assert!(!root.join(format!("startup-{identity}.json")).exists());
        Ok(())
    }
}

pub(super) async fn detach(root: &Path) -> Result<serde_json::Value> {
    require_manager_change_settled(root)?;
    layout::require_managed_ready(root)
        .context("managed_catalog_detach_requires_supported_transition")?;
    lifecycle::require_settled(root)?;
    let intent_path = root.join(DETACH);
    let policy: Managed = if exists(&intent_path)? {
        serde_json::from_slice(&read_bounded(&intent_path, MAX_POLICY)?)?
    } else {
        // A removed user reference is recoverable only from the exact settled
        // reservation. This never creates policy/selection or starts a service.
        let policy = catalog::saved_policy(root)?.context("managed policy missing")?;
        catalog::require_detach_policy(root, &policy)?;
        replace_json(root, DETACH, &policy)?;
        policy
    };
    catalog::require_detach_policy(root, &policy)?;
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
    if policy.lifecycle == ManagedLifecycle::UserOnDemand {
        crate::http_endpoint::trust::remove_owned(&layout::saved_control(root)?)?;
    }
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
