// SPDX-License-Identifier: Apache-2.0
//! Versioned native-state boundary; binary replacement is not state initialization.
use super::*;
use airk_core::identity::Incarnation;

pub(super) const BASELINE: &str = "1.8.16";
pub(super) const CONTRACT: &str = "airk-state/1";
const MARKER: &str = "airk-state-contract.json";

#[cfg(test)]
mod managed_tests;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Marker {
    schema: u32,
    state_contract: String,
    baseline_version: String,
    install: PathBuf,
    control: PathBuf,
    namespace: Incarnation,
}

fn version(value: &str) -> Result<(u64, u64, u64)> {
    ensure!(
        value.split('.').all(|part| !part.is_empty()
            && part.bytes().all(|byte| byte.is_ascii_digit())
            && (part == "0" || !part.starts_with('0'))),
        "unsupported native version"
    );
    let parts = value
        .split('.')
        .map(str::parse::<u64>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ensure!(parts.len() == 3, "unsupported native version");
    Ok((parts[0], parts[1], parts[2]))
}

pub(super) fn modern(value: &str) -> Result<bool> {
    Ok(version(value)? >= version(BASELINE)?)
}

fn read(root: &Path, control: &Path) -> Result<Option<Marker>> {
    let path = control.join(MARKER);
    if !exists(&path)? {
        return Ok(None);
    }
    safe_file(&path)?;
    let marker: Marker = serde_json::from_slice(&read_bounded(&path, 8192)?)?;
    ensure!(
        marker.schema == 1
            && marker.state_contract == CONTRACT
            && marker.baseline_version == BASELINE,
        "unsupported_state_contract: use a runtime supporting the saved state contract"
    );
    ensure!(
        marker.install == root.canonicalize()?
            && marker.control == control.canonicalize()?
            && airk_store::service::registry::ServiceRegistry::open_saved(control)?
                .incarnation()?
                == marker.namespace,
        "state_contract_identity_changed: installation, control or namespace differs from the initialized state"
    );
    Ok(Some(marker))
}

/// This observation does not initialize or repair a missing marker.
pub(super) fn required(root: &Path) -> Result<bool> {
    // A public selection identifies the approved runtime; it does not change a
    // managed provider into an owned payload copied beneath this installation.
    let (main, approval) = if let Some(policy) = managed::read(root)? {
        require_managed_selection(root, &policy.approval)?;
        (policy.saved_executable(), policy.metadata_approval()?)
    } else if let Some(selected) = activation::selection(root)? {
        let directory = root.join("runtimes").join(&selected.active);
        let (approval, _) = saved_payload_identity(&directory, &selected.active)?;
        approved_inventory_metadata(&directory, &approval)?;
        (directory.join(main_name()), approval)
    } else {
        return Ok(false);
    };
    let control = layout::saved_control(root)?;
    let marker = read(root, &control)?;
    Ok(
        requires_reset_for_version(&approval.version, marker.is_some())?
            || !runtime_pair::single_executable(&main)?,
    )
}

fn require_managed_selection(root: &Path, approval: &Approval) -> Result<()> {
    if let Some(selected) = activation::selection(root)? {
        ensure!(
            selected.schema == 1
                && selected.active == approval.manifest_sha256
                && selected.previous.is_none(),
            "managed selection differs from its approved pin"
        );
    }
    Ok(())
}

fn requires_reset_for_version(predecessor: &str, initialized: bool) -> Result<bool> {
    Ok(!modern(predecessor)? || !initialized)
}

pub(super) fn candidate(root: &Path, identity: &str) -> Result<()> {
    let (main, approved) = verify_runtime(root, identity)?;
    ensure!(
        modern(&approved.version)? && runtime_pair::single_executable(&main)?,
        "clean_break_requires_modern_runtime: baseline is 1.8.16"
    );
    if let Some(selected) = activation::selection(root)? {
        let (_, predecessor) = verify_runtime(root, &selected.active)?;
        ensure!(
            version(&approved.version)? >= version(&predecessor.version)?,
            "clean_break_downgrade_refused: a newer installed runtime cannot be reset by an older candidate"
        );
    }
    let observed = self_test_observation(&main)?;
    ensure!(
        observed
            .get("state_contract")
            .and_then(serde_json::Value::as_str)
            == Some(CONTRACT),
        "candidate_state_contract_unsupported: candidate does not declare airk-state/1"
    );
    Ok(())
}

/// The opaque Genesis owner proves actual fresh namespace admission. Publish
/// before its settled commit so a completed first install cannot omit the marker.
pub(super) fn initialize(
    root: &Path,
    owner: &airk_store::service::registry::catalog::genesis::CatalogGenesis,
) -> Result<()> {
    let receipt = owner.receipt()?;
    ensure!(
        receipt.authority_v1.is_some(),
        "state contract needs native Genesis qualification"
    );
    let control = receipt.root.canonicalize()?;
    let marker = Marker {
        schema: 1,
        state_contract: CONTRACT.into(),
        baseline_version: BASELINE.into(),
        install: root.canonicalize()?,
        control: control.clone(),
        namespace: receipt.namespace,
    };
    let path = control.join(MARKER);
    if exists(&path)? {
        safe_file(&path)?;
        let saved: Marker = serde_json::from_slice(&read_bounded(&path, 8192)?)?;
        ensure!(saved == marker, "fresh state contract changed");
        return Ok(());
    }
    replace_json(&control, MARKER, &marker)
}

pub(super) fn require_initialized(root: &Path, control: &Path) -> Result<()> {
    ensure!(
        read(root, control)?.is_some(),
        "clean_break_required: settled state has no initialized baseline contract"
    );
    Ok(())
}

pub(super) fn require_control(control: &Path) -> Result<()> {
    if let Some(root) = layout::installation_for_control(control)? {
        require_initialized(&root, control)?;
        ensure!(
            !required(&root)?,
            "clean_break_required: installed source reads and enrollment require the initialized 1.8.16 baseline"
        );
    }
    Ok(())
}

/// An installer-owned probe of the fresh namespace before selection commits.
/// Private fields prevent ordinary reads/enrollment from constructing authority.
pub(crate) struct InitializedProbe {
    root: PathBuf,
    control: PathBuf,
    candidate: String,
    version: String,
    operation: Incarnation,
    namespace: Incarnation,
    instance: Incarnation,
}

pub(super) fn initialized_probe(
    root: &Path,
    candidate: &str,
    version: &str,
    instance: Incarnation,
    executable_sha256: &str,
) -> Result<Option<InitializedProbe>> {
    let root = root.canonicalize()?;
    let control = layout::saved_control(&root)?;
    let Some(operation) =
        activation::rebuild::initialized_probe_operation(&root, &control, candidate)?
    else {
        return Ok(None);
    };
    let marker = read(&root, &control)?
        .context("clean-break bootstrap lacks initialized Genesis contract")?;
    let (main, approval) = verify_runtime(&root, candidate)?;
    ensure!(
        approval.version == version
            && modern(version)?
            && runtime_pair::single_executable(&main)?
            && runtime_pair::digest_file(&main)? == executable_sha256,
        "clean-break bootstrap health differs from the exact approved candidate"
    );
    Ok(Some(InitializedProbe {
        root,
        control,
        candidate: candidate.into(),
        version: version.into(),
        operation,
        namespace: marker.namespace,
        instance,
    }))
}

impl InitializedProbe {
    pub(crate) fn request_identity(&self) -> Result<(&Path, &str, Incarnation, Incarnation)> {
        ensure!(
            activation::rebuild::initialized_probe_operation(
                &self.root,
                &self.control,
                &self.candidate
            )? == Some(self.operation)
                && read(&self.root, &self.control)?
                    .is_some_and(|marker| marker.namespace == self.namespace),
            "clean-break bootstrap authority changed before registry probe"
        );
        Ok((&self.control, &self.version, self.instance, self.namespace))
    }
}

pub(super) fn require_service(control: &Path) -> Result<()> {
    let Some(root) = layout::installation_for_control(control)? else {
        return Ok(());
    };
    require_initialized(&root, control)?;
    let current = std::env::current_exe()?.canonicalize()?;
    if let Some(policy) = managed::read(&root)? {
        require_managed_selection(&root, &policy.approval)?;
        let (main, approval, _) = policy.verify_inventory_checked(&|| Ok(()))?;
        ensure!(
            main.canonicalize()? == current
                && modern(&approval.version)?
                && runtime_pair::single_executable(&main)?,
            "installed service is not the approved baseline image"
        );
        return Ok(());
    }
    let selected = activation::selection(&root)?;
    let pending = activation::admitted_service_candidate(&root, control)?;
    let identity = pending
        .as_deref()
        .or_else(|| selected.as_ref().map(|selected| selected.active.as_str()))
        .context("installed service has no admitted selection")?;
    let (main, approval) = verify_runtime(&root, identity)?;
    ensure!(
        main.canonicalize()? == current
            && modern(&approval.version)?
            && runtime_pair::single_executable(&main)?,
        "installed service is not the exact approved baseline image"
    );
    Ok(())
}

fn registrations(control: &Path) -> Result<Vec<airk_core::service::ProjectRegistration>> {
    let mut registry = airk_store::service::registry::ServiceRegistry::open_saved(control)?;
    let revision = registry.list(None, None, 1)?.revision;
    let registrations = registry
        .bindings()?
        .into_iter()
        .map(|binding| {
            registry
                .retained_project(binding)
                .map(|project| project.registration)
        })
        .collect::<Result<Vec<_>>>()?;
    registry.list(None, Some(revision), 1)?;
    Ok(registrations)
}

pub(super) fn report(root: &Path) -> Result<serde_json::Value> {
    let required = required(root)?;
    let control = layout::saved_control(root)?;
    let installed = read(root, &control)?.map(|marker| marker.state_contract);
    let saved_registrations = activation::rebuild::client_registrations(root)?;
    let retained_registrations = saved_registrations.is_some();
    let client_metadata = activation::rebuild::client_metadata(root)?;
    let http_snapshot = activation::rebuild::http_snapshot_report(root)?;
    let registrations = if let Some(saved) = saved_registrations {
        saved
    } else {
        registrations(&control)?
    };
    ensure!(
        serde_json::to_vec(&registrations)?.len() <= 128 * 1024,
        "clean_break_complete_registration_roster_limit: full client scope metadata exceeds 128 KiB; no subset was returned"
    );
    Ok(
        serde_json::json!({"required":required,"baseline_version":BASELINE,"state_contract":CONTRACT,
        "installed_contract":installed,"registrations_complete":true,"registrations":registrations,"registrations_scope":if retained_registrations {"predecessor_reset_roster"} else {"current_registry"},
        "predecessor_client_metadata":client_metadata,"http_ownership_snapshot":http_snapshot}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clean_break_state_contract_cannot_be_bypassed_by_version_or_copied_marker() -> Result<()> {
        assert!(requires_reset_for_version("1.8.15", true)?);
        assert!(requires_reset_for_version("1.8.16", false)?);
        assert!(!requires_reset_for_version("1.8.16", true)?);
        assert!(!requires_reset_for_version("1.9.0", true)?);
        let owner = tempfile::tempdir()?;
        let root = owner.path().join("install");
        local_ipc::prepare_root(&root)?;
        let control = owner.path().join("control");
        local_ipc::prepare_root(&control)?;
        let namespace = Incarnation::from_bytes([88; 16]);
        drop(airk_store::service::registry::ServiceRegistry::open(
            &control, namespace,
        )?);
        assert!(read(&root, &control)?.is_none());
        assert!(
            !control.join(MARKER).exists(),
            "observing missing state must not repair it"
        );
        let mut marker = Marker {
            schema: 1,
            state_contract: CONTRACT.into(),
            baseline_version: BASELINE.into(),
            install: root.canonicalize()?,
            control: control.canonicalize()?,
            namespace,
        };
        replace_json(&control, MARKER, &marker)?;
        assert_eq!(read(&root, &control)?, Some(marker.clone()));
        marker.namespace = Incarnation::from_bytes([89; 16]);
        replace_json(&control, MARKER, &marker)?;
        assert!(read(&root, &control).is_err());
        marker.namespace = namespace;
        marker.state_contract = "airk-state/2".into();
        replace_json(&control, MARKER, &marker)?;
        let before = fs::read(control.join(MARKER))?;
        assert!(read(&root, &control).is_err());
        assert_eq!(fs::read(control.join(MARKER))?, before);
        Ok(())
    }

    #[test]
    fn clean_break_registration_census_includes_inactive_without_mutation() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let control = owner.path().join("control");
        local_ipc::prepare_root(&control)?;
        let namespace = airk_core::identity::Incarnation::from_bytes([87; 16]);
        let mut registry =
            airk_store::service::registry::ServiceRegistry::open(&control, namespace)?;
        let first = airk_core::service::ProjectRegistration::new(
            owner.path().join("first").to_string_lossy().into_owned(),
            owner
                .path()
                .join("data-first")
                .to_string_lossy()
                .into_owned(),
            "first".into(),
        )?;
        let second = airk_core::service::ProjectRegistration::new(
            owner.path().join("second").to_string_lossy().into_owned(),
            owner
                .path()
                .join("data-second")
                .to_string_lossy()
                .into_owned(),
            "second".into(),
        )?;
        registry.register(&first)?;
        registry.register(&second)?;
        // Seed retained inactive metadata; the method under test performs only canonical reads.
        rusqlite::Connection::open(control.join("service-1.4.sqlite3"))?.execute(
            "UPDATE projects SET registered=0 WHERE binding=?1",
            [second.binding.to_string()],
        )?;
        assert!(registry.get(second.binding).is_err());
        let before = registry.saved_request_epoch()?;
        let actual = registrations(&control)?;
        assert_eq!(actual.len(), 2);
        assert!(actual.contains(&first) && actual.contains(&second));
        assert_eq!(registry.saved_request_epoch()?, before);
        Ok(())
    }

    #[test]
    fn clean_break_version_boundary_is_numeric_and_explicit() -> Result<()> {
        for old in ["1.4.0", "1.8.11", "1.8.15"] {
            assert!(!modern(old)?);
        }
        for current in ["1.8.16", "1.8.17", "1.9.0", "1.10.0", "2.0.0"] {
            assert!(modern(current)?);
        }
        assert!(modern("1.8").is_err());
        assert!(modern("1.8.16-pre").is_err());
        for invalid in ["01.8.16", "+1.8.16", "1.08.16"] {
            assert!(modern(invalid).is_err());
        }
        Ok(())
    }
}
