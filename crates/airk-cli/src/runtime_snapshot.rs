// SPDX-License-Identifier: Apache-2.0
//! Saved bootstrap/project metadata remains usable without executing a damaged payload.
use super::*;
use airk_core::service::ProjectBinding;
use airk_store::service::registry::ServiceRegistry;

pub(super) struct SavedRuntime {
    pub selection: Selection,
    pub executable: PathBuf,
    pub approval: Approval,
    pub executable_sha256: String,
}
pub(super) fn identity(root: &Path) -> Result<SavedRuntime> {
    ensure!(root.is_dir(), "runtime installation is missing");
    if let Some(saved) = payload_repair::saved_identity(root)? {
        return Ok(saved);
    }
    let selection = selection_record(root)?;
    if let Some(policy) = managed::read(root)? {
        ensure!(
            selection.active == policy.approval.manifest_sha256,
            "managed selection differs from its saved approval"
        );
        return Ok(SavedRuntime {
            selection,
            executable: policy.saved_executable(),
            approval: policy.approval.clone(),
            executable_sha256: policy.saved_executable_sha256().into(),
        });
    }
    let (approval, executable_sha256) = stop_identity(root)?;
    let executable = payload_path(
        root,
        &format!("runtimes/{}/{}", selection.active, main_name()),
    )?;
    Ok(SavedRuntime {
        selection,
        executable,
        approval,
        executable_sha256,
    })
}

pub(super) fn read(
    root: &Path,
    after: Option<ProjectBinding>,
    revision: Option<u64>,
    limit: usize,
) -> Result<serde_json::Value> {
    ensure!((1..=32).contains(&limit), "invalid snapshot page limit");
    let saved = identity(root)?;
    let policy = managed::read(root)?;
    let control = layout::control(root);
    let state_error = control
        .as_ref()
        .err()
        .map(|_| "control_binding_unavailable");
    let mut namespace = None;
    let mut epoch = None;
    let mut lifecycle = None;
    let mut console = None;
    let mut registry_error = None;
    if let Ok(control) = &control {
        if control.join("service-1.4.sqlite3").try_exists()? {
            let observed = (|| -> Result<_> {
                let mut registry = ServiceRegistry::open_saved(control)?;
                let (identity, current_epoch) = registry.saved_request_epoch()?;
                let lifecycle = serde_json::json!({"explicitly_stopped":registry.service_explicitly_stopped()?,"pending":registry.pending_lifecycle()?,
                    "recorded_instance":registry.recorded_service_instance()?,"namespace":identity,"epoch":current_epoch});
                let page = crate::service_management::console_page(
                    &mut registry,
                    after,
                    revision,
                    limit,
                    unix_ms()?,
                );
                Ok((identity, current_epoch, lifecycle, page))
            })();
            match observed {
                Ok((identity, current_epoch, observed, page)) => {
                    namespace = Some(identity);
                    epoch = Some(current_epoch);
                    lifecycle = Some(observed);
                    match page {
                        Ok(page) => console = Some(page),
                        Err(_) => {
                            registry_error = Some("cached_registry_unavailable_or_cursor_expired")
                        }
                    }
                }
                Err(_) => registry_error = Some("cached_registry_unavailable_or_cursor_expired"),
            }
        } else if after.is_some() || revision.is_some() {
            registry_error = Some("cached_registry_unavailable_or_cursor_expired");
        }
    }
    let startup_manager = if policy.is_some() {
        serde_json::json!({"mode":"managed_policy"})
    } else {
        match crate::service_manager::ServiceManager::for_runtime(root)
            .and_then(|manager| manager.status())
        {
            Ok(status) => {
                serde_json::json!({"mode":"user","id":status.id,"platform":status.platform,"installed":status.installed,
                "loaded":status.loaded,"running":status.running,"registered_definition_matches":status.registered_definition_matches,
                "verification_failed":status.registration_verification_error.is_some()})
            }
            Err(_) => serde_json::json!({"mode":"unavailable"}),
        }
    };
    let log_control = layout::saved_control(root).ok();
    let logs = log_control
        .as_ref()
        .and_then(|control| crate::service_logs::recent(control, 32).ok());
    let launcher = policy.is_none().then(|| launcher_path(root));
    let value = serde_json::json!({"schema":1,"metadata_only":true,"payload_verified":false,"observed_unix_ms":unix_ms()?,
        "selection":saved.selection,"executable":saved.executable,"approval":saved.approval,"executable_sha256":saved.executable_sha256,
        "managed_policy":policy,"launcher":launcher,"control_dir":control.ok(),"state_error":state_error,
        "registry_namespace":namespace,"request_epoch":epoch,"lifecycle":lifecycle,"registry_error":registry_error,
        "console":console,"startup_manager":startup_manager,"logs":logs,"log_control_dir":log_control,
        "payload_repair":payload_repair::status(root).unwrap_or_else(|_|serde_json::json!({"phase":"unavailable"}))});
    ensure!(
        serde_json::to_vec_pretty(&value)?.len() < 256 * 1024,
        "bootstrap snapshot exceeds budget"
    );
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_snapshot_survives_missing_payload_and_does_not_open_project_data() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let root = base.join("install");
        local_ipc::prepare_root(&root)?;
        layout::initialize(&root, Some(&base.join("state")), RuntimeChannel::Stable)?;
        let source = base.join("fixture-payload");
        let digest = super::super::tests::fixture(&source)?;
        let slot = root.join("runtimes").join(&digest);
        ensure!(
            super::super::tests::fixture(&slot)? == digest,
            "fixture inventory is not deterministic"
        );
        let approval = Approval {
            schema: 1,
            manifest_sha256: digest.clone(),
            version: env!("CARGO_PKG_VERSION").into(),
            verification_basis: VerificationBasis::TrustedDistribution,
        };
        fs::write(slot.join("approval.json"), serde_json::to_vec(&approval)?)?;
        replace_json(
            &root,
            "selected.json",
            &Selection {
                schema: 1,
                active: digest.clone(),
                previous: None,
            },
        )?;
        let control = layout::control(&root)?;
        local_ipc::prepare_root(&control)?;
        let lease = airk_store::service::ServiceLease::acquire(&control)?;
        layout::record_control(&lease)?;
        let mut registry =
            ServiceRegistry::open(&control, airk_kernel::service_broker::new_instance_id()?)?;
        let source_root = base.join("unavailable-originals");
        let data = base.join("unopened-project-data");
        let project = airk_core::service::ProjectRegistration::new(
            source_root.to_str().unwrap().into(),
            data.to_str().unwrap().into(),
            "snapshot-fixture".into(),
        )?;
        registry.register(&project)?;
        let namespace = registry.incarnation()?;
        registry
            .record_service_instance(&lease, airk_kernel::service_broker::new_instance_id()?)?;
        layout::record_registry(&lease, namespace)?;
        drop(registry);
        fs::remove_file(slot.join(helper_name()))?;
        fs::remove_file(slot.join(main_name()))?;
        assert!(selected(&root).is_err());
        let snapshot = read(&root, None, None, 16)?;
        assert_eq!(snapshot["metadata_only"], true);
        assert_eq!(snapshot["payload_verified"], false);
        assert_eq!(snapshot["selection"]["active"], digest);
        assert_eq!(snapshot["registry_namespace"], namespace.to_string());
        assert_eq!(
            snapshot["console"]["projects"][0]["project"]["registration"]["binding"],
            project.binding.to_string()
        );
        assert!(snapshot["lifecycle"]["recorded_instance"].is_string());
        let expired = read(&root, Some(project.binding), Some(u64::MAX), 16)?;
        assert!(expired["console"].is_null());
        assert!(expired["registry_error"].is_string());
        assert_eq!(expired["registry_namespace"], namespace.to_string());
        assert!(expired["lifecycle"]["recorded_instance"].is_string());
        assert!(!source_root.exists());
        assert!(!data.exists());
        assert!(!slot.join(main_name()).exists());
        assert!(!slot.join(helper_name()).exists());
        let cached = identity(&root)?;
        assert_eq!(cached.approval, approval);
        let expected = cached.executable_sha256;
        fs::write(
            slot.join(main_name()),
            b"Preserve a modified original fixture image.",
        )?;
        assert_eq!(identity(&root)?.executable_sha256, expected);
        assert_eq!(
            fs::read(slot.join(main_name()))?,
            b"Preserve a modified original fixture image."
        );
        assert!(read(&root, None, None, 33).is_err());
        Ok(())
    }
}
