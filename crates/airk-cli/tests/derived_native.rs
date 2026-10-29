// SPDX-License-Identifier: Apache-2.0
//! Removal owns complete derived cleanup, including when original sources are gone.
mod common;
use airk_core::service::{ControlMethod as Method, ControlResponse as Response, JobKind};
use anyhow::Result;
use std::{
    fs,
    time::{Duration, Instant},
};

#[test]
fn removal_deletes_configuration_and_index_and_allows_explicit_fresh_registration() -> Result<()> {
    cleanup_fixture(false)
}
#[test]
fn missing_source_removal_never_recreates_or_opens_original_sources() -> Result<()> {
    cleanup_fixture(true)
}
fn cleanup_fixture(missing_source: bool) -> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    fs::write(
        source.join("original.rs"),
        b"pub fn icy_cleanup_fixture() {}\n",
    )?;
    let service = common::Service::start(&source, &data, "cleanup-native");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.job(JobKind::Index);
    let offline = owner.path().join("offline");
    if missing_source {
        fs::rename(&source, &offline)?;
    }
    // One explicit removal action owns all cleanup; no second preview/approval.
    service.configure(airk_cli::service_configuration::Draft::Unregister);
    let until = Instant::now() + Duration::from_secs(90);
    loop {
        let mut registry =
            airk_store::service::registry::ServiceRegistry::open_saved(&service.control)?;
        if registry.project_was_removed(service.project.binding)? {
            assert!(registry.retained_project(service.project.binding).is_err());
            assert!(registry.list_inactive(None, None, 32)?.projects.is_empty());
            break;
        }
        if Instant::now() >= until {
            let mut probe = airk_store::service::registry::ServiceRegistry::open(
                &service.control,
                registry.incarnation()?,
            )?;
            anyhow::bail!(
                "full removal did not complete; local diagnostic turn: {:?}",
                probe.purge_removed_project_turn(service.project.binding)
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!data.join("collections-1.4/cleanup-native").exists());
    assert!(matches!(service.request(Method::RequestReceipt {
        caller: airk_kernel::service_broker::new_instance_id()?, key: service.key(), project: Some(service.project.binding)
    }), Response::RequestReceipt { job: None, removed_project: Some(project), .. } if project==service.project.binding));
    if missing_source {
        assert!(!source.exists());
        fs::rename(&offline, &source)?;
    }
    assert_eq!(
        fs::read(source.join("original.rs"))?,
        b"pub fn icy_cleanup_fixture() {}\n"
    );
    assert!(matches!(
        service.request(Method::Register {
            project: service.project.clone()
        }),
        Response::Registered { .. }
    ));
    // Re-registration creates fresh settings rather than reviving the deleted opt-out.
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.job(JobKind::Index);
    assert!(data.join("collections-1.4/cleanup-native").is_dir());
    Ok(())
}
