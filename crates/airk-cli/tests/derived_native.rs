// SPDX-License-Identifier: Apache-2.0
//! Explicit deletion uses the native inactive owner and preserves original inputs.
mod common;
use airk_core::{
    service::{ControlMethod as Method, ControlResponse as Response, JobKind},
    service_cleanup::{DerivedCleanupPreview, DerivedCleanupReport},
};
use anyhow::{Result, ensure};
use std::{
    fs,
    time::{Duration, Instant},
};

#[test]
fn inactive_cleanup_runs_without_query_admission_preserves_configuration_and_allows_fresh_registration()
-> Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    fs::write(
        source.path().join("original.rs"),
        b"pub fn icy_cleanup_fixture() {}\n",
    )?;
    let service = common::Service::start(source.path(), data.path(), "cleanup-native");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.job(JobKind::Index);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let drafts = tempfile::tempdir()?;
    let configure = |draft| -> Result<serde_json::Value> {
        if matches!(&draft, airk_cli::service_configuration::Draft::Unregister) {
            service.job(JobKind::Inspect);
        }
        let request = runtime.block_on(airk_cli::service_configuration::prepare(
            &service.control,
            service.project.binding,
            draft,
        ))?;
        let response = if request.configuration.is_derived_cleanup() {
            let file = drafts.path().join("cleanup.json");
            fs::write(&file, serde_json::to_vec(&request.configuration)?)?;
            let prepared = common::native_command()
                .args(["service", "requests", "--runtime-dir"])
                .arg(&service.control)
                .args(["prepare-configuration", "--project"])
                .arg(service.project.binding.to_string())
                .arg("--file")
                .arg(&file)
                .output()?;
            ensure!(
                prepared.status.success(),
                "CLI cleanup preparation failed: {}",
                String::from_utf8_lossy(&prepared.stderr)
            );
            let prepared: serde_json::Value = serde_json::from_slice(&prepared.stdout)?;
            assert_eq!(prepared["submitted"], false);
            assert_eq!(airk_cli::client_requests::list(&service.control)?.len(), 1);
            let submitted = common::native_command()
                .args(["service", "requests", "--runtime-dir"])
                .arg(&service.control)
                .arg("retry")
                .arg(prepared["prepared"].as_str().unwrap())
                .output()?;
            ensure!(
                submitted.status.success(),
                "CLI cleanup submission failed: {}",
                String::from_utf8_lossy(&submitted.stderr)
            );
            let response: Response = serde_json::from_slice(&submitted.stdout)?;
            assert!(airk_cli::client_requests::list(&service.control)?.is_empty());
            response
        } else {
            service.request(Method::ConfigureProject {
                project: service.project.binding,
                caller: airk_kernel::service_broker::new_instance_id()?,
                key: service.key(),
                request: Box::new(request.clone()),
            })
        };
        let Response::Job { job } = response else {
            anyhow::bail!("native configuration refused")
        };
        let completed = service.wait_outcome(job.id);
        ensure!(
            completed.state == airk_core::service::JobState::Succeeded,
            "configuration failed for {}: {:?}",
            serde_json::to_value(&request.configuration)?,
            completed.outcome
        );
        let Response::Configuration {
            result: Some(result),
            ..
        } = service.request(Method::ConfigurationResult { id: job.id })
        else {
            anyhow::bail!("native configuration result missing")
        };
        Ok(result)
    };
    let collection = data.path().join("collections-1.4/cleanup-native");
    let active: serde_json::Value =
        serde_json::from_slice(&fs::read(collection.join("active.json"))?)?;
    let old = collection
        .join("incarnations")
        .join(active["active"].as_str().unwrap());
    let policy = fs::read(old.join("vector-policy.json"))?;
    configure(airk_cli::service_configuration::Draft::Unregister)?;
    assert!(
        matches!(service.request(Method::Connect {project:service.project.binding}),Response::Failure {code} if code=="project_unregistered")
    );
    let Response::Console { page } = service.request(Method::InactiveConsole {
        after: None,
        revision: None,
        limit: 16,
    }) else {
        anyhow::bail!("inactive console unavailable")
    };
    assert_eq!(page.projects.len(), 1);
    let preview: DerivedCleanupPreview = serde_json::from_value(configure(
        airk_cli::service_configuration::Draft::PrepareDerivedCleanup,
    )?)?;
    preview.validate()?;
    assert!(!preview.resuming);
    assert!(old.join("current.airk").is_file());
    let report: DerivedCleanupReport = serde_json::from_value(configure(
        airk_cli::service_configuration::Draft::DeleteDerived {
            preview: preview.guard,
        },
    )?)?;
    report.validate()?;
    assert!(report.complete);
    assert!(report.removed_known_files > 0);
    assert!(!old.try_exists()?);
    assert_eq!(
        fs::read(source.path().join("original.rs"))?,
        b"pub fn icy_cleanup_fixture() {}\n"
    );
    let active: serde_json::Value =
        serde_json::from_slice(&fs::read(collection.join("active.json"))?)?;
    let fresh = collection
        .join("incarnations")
        .join(active["active"].as_str().unwrap());
    assert_eq!(fs::read(fresh.join("vector-policy.json"))?, policy);
    assert!(!fresh.join("current.airk").try_exists()?);
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if matches!(
            service.request(Method::Worker {
                project: service.project.binding
            }),
            Response::Worker { identity: None, .. }
        ) {
            break;
        }
        ensure!(
            Instant::now() < until,
            "private cleanup owner did not drain"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(matches!(
        service.request(Method::Register {
            project: service.project.clone()
        }),
        Response::Registered { .. }
    ));
    service.job(JobKind::Index);
    let after: DerivedCleanupPreview = serde_json::from_value({
        configure(airk_cli::service_configuration::Draft::Unregister)?;
        configure(airk_cli::service_configuration::Draft::PrepareDerivedCleanup)?
    })?;
    assert_ne!(
        after.guard, preview.guard,
        "re-registration must invalidate the old preview"
    );
    Ok(())
}
