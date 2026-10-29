// SPDX-License-Identifier: Apache-2.0
//! Retiring one registration drains its owner without deleting source or derived state.
mod common;
use airk_core::service::{ControlMethod as Method, ControlResponse as Response, JobKind};
use anyhow::{Context, Result, ensure};
use common::read_retry::ReadRetry;
use serde_json::{Value, json};
use std::{
    fs,
    time::{Duration, Instant},
};

#[test]
fn paused_removal_drains_clients_deletes_project_state_preserves_other_projects_and_fences_old_receipts()
-> Result<()> {
    let a = tempfile::tempdir()?;
    let b = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let original_a = "pub fn polar_unregister_fixture() {}\n";
    let original_b = "pub fn tropical_unregister_fixture() {}\n";
    fs::write(a.path().join("original.rs"), original_a)?;
    fs::write(b.path().join("other.rs"), original_b)?;
    let service = common::Service::start(a.path(), data.path(), "unregister-a");
    // This scenario keeps direct clients for two projects alive together.
    // Small hosts intentionally admit only one resident under default policy.
    let configured = service.request(Method::ConfigureResources {
        configuration: airk_core::service_resources::ResourceConfiguration {
            memory_bytes: Some(4 << 30),
            extraction_threads: Some(1),
            ..Default::default()
        },
    });
    ensure!(
        matches!(&configured, Response::Resources { observation, configuration_error: false, .. }
        if observation.resident_limit >= 2),
        "two-worker fixture admission unavailable: {}",
        serde_json::to_string(&configured)?
    );
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.job(JobKind::Index);
    let other = airk_kernel::service_projects::registration(b.path(), data.path(), "unregister-b")?;
    assert!(matches!(
        service.request(Method::Register {
            project: other.clone()
        }),
        Response::Registered { .. }
    ));
    let enqueue = |project, kind| {
        let Response::Job { job } = service.request(Method::Enqueue {
            project,
            kind,
            caller: airk_kernel::service_broker::new_instance_id().unwrap(),
            key: service.key(),
        }) else {
            panic!("native job refused");
        };
        service.wait(job.id)
    };
    enqueue(other.binding, JobKind::Inspect);
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let request_b = executor.block_on(airk_cli::service_configuration::prepare(
        &service.control,
        other.binding,
        airk_cli::service_configuration::Draft::Vectors { enabled: false },
    ))?;
    let Response::Job { job } = service.request(Method::ConfigureProject {
        project: other.binding,
        caller: airk_kernel::service_broker::new_instance_id()?,
        key: service.key(),
        request: Box::new(request_b),
    }) else {
        anyhow::bail!("other project configuration refused");
    };
    service.wait(job.id);
    enqueue(other.binding, JobKind::Index);
    let read = |project, collection: &str| {
        executor.block_on(async {
            let mut connection =
                airk_cli::project_client::Connection::connect(&service.control, project).await?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            loop {
                let reply = tokio::time::timeout_at(
                    deadline.into(),
                    connection.read_retry(
                        "airk_diagnostics".into(),
                        json!({"response_mode":"full"}).as_object().unwrap().clone(),
                    ),
                )
                .await??;
                ensure!(reply["isError"] != true, "status read rejected");
                let value: Value = serde_json::from_str(
                    reply["content"][0]["text"]
                        .as_str()
                        .context("missing status")?,
                )?;
                ensure!(
                    std::time::Instant::now() < deadline,
                    "status observation deadline exhausted"
                );
                if !common::read_retry::publication_pending(
                    &value,
                    collection,
                    &project.to_string(),
                )? {
                    return Ok::<Value, anyhow::Error>(value);
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
        })
    };
    read(service.project.binding, &service.project.collection)?;
    let unrelated = read(other.binding, &other.collection)?;
    // Keep a real direct client idle across unregister; it must not prevent drain.
    let mut idle = executor.block_on(airk_cli::project_client::Connection::connect(
        &service.control,
        service.project.binding,
    ))?;
    service.job(JobKind::Inspect);
    assert!(
        matches!(service.request(Method::Pause {project:service.project.binding,paused:true}),
        Response::Registered{project} if project.paused)
    );
    let Response::Job { job: queued } = service.request(Method::Enqueue {
        project: service.project.binding,
        caller: airk_kernel::service_broker::new_instance_id()?,
        key: service.key(),
        kind: JobKind::Index,
    }) else {
        anyhow::bail!("paused source request was not accepted");
    };
    let request = executor.block_on(airk_cli::service_configuration::prepare(
        &service.control,
        service.project.binding,
        airk_cli::service_configuration::Draft::Unregister,
    ))?;
    let caller = airk_kernel::service_broker::new_instance_id()?;
    let key = service.key();
    let submit = || {
        service.request(Method::ConfigureProject {
            project: service.project.binding,
            caller,
            key: key.clone(),
            request: Box::new(request.clone()),
        })
    };
    let Response::Job { job } = submit() else {
        anyhow::bail!("unregister refused");
    };
    service.wait(job.id);
    assert_eq!(
        service.wait_outcome(queued.id).state,
        airk_core::service::JobState::Canceled
    );
    let Response::Configuration {
        result: Some(result),
        ..
    } = service.request(Method::ConfigurationResult { id: job.id })
    else {
        anyhow::bail!("unregister receipt missing");
    };
    assert_eq!(result["unregistered"], true);
    assert_eq!(result["derived_state_retained"], true);
    assert_eq!(result["settings_preserved"], false);
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
        ensure!(Instant::now() < until, "unregistered owner did not drain");
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        executor
            .block_on(idle.call("airk_status".into(), json!({}).as_object().unwrap().clone()))
            .is_err()
    );
    assert!(
        executor
            .block_on(airk_cli::project_client::Connection::connect(
                &service.control,
                service.project.binding
            ))
            .is_err()
    );
    assert_eq!(
        read(other.binding, &other.collection)?["publication"],
        unrelated["publication"]
    );
    let deadline = Instant::now() + Duration::from_secs(90);
    while data.path().join("collections-1.4/unregister-a").exists() {
        ensure!(
            Instant::now() < deadline,
            "full removal did not delete index files"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        fs::read_to_string(a.path().join("original.rs"))?,
        original_a
    );
    assert_eq!(fs::read_to_string(b.path().join("other.rs"))?, original_b);
    assert!(matches!(
        service.request(Method::Register {
            project: service.project.clone()
        }),
        Response::Registered { .. }
    ));
    assert!(
        matches!(submit(), Response::Failure { .. }),
        "deleted receipts must not replay a removal against a new registration"
    );
    assert_eq!(
        read(other.binding, &other.collection)?["publication"],
        unrelated["publication"]
    );
    Ok(())
}

#[test]
fn deleted_source_is_removed_automatically_with_its_index_without_affecting_other_projects()
-> Result<()> {
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("deleted-source");
    let other_source = owner.path().join("other-source");
    let data = owner.path().join("data");
    for path in [&source, &other_source, &data] {
        fs::create_dir(path)?;
    }
    fs::write(source.join("memo.txt"), "original copper evidence")?;
    fs::write(other_source.join("memo.txt"), "untouched sibling evidence")?;
    let service = common::Service::start(&source, &data, "missing-root");
    service.configure(airk_cli::service_configuration::Draft::Vectors { enabled: false });
    service.job(JobKind::Index);
    let other = airk_kernel::service_projects::registration(&other_source, &data, "other-root")?;
    assert!(matches!(
        service.request(Method::Register {
            project: other.clone()
        }),
        Response::Registered { .. }
    ));
    // Manual mode and pause must not bypass source-lifetime checks.
    assert!(matches!(
        service.request(Method::Pause {
            project: service.project.binding,
            paused: true
        }),
        Response::Registered { .. }
    ));
    fs::remove_file(source.join("memo.txt"))?;
    fs::remove_dir(&source)?;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let registry =
            airk_store::service::registry::ServiceRegistry::open_saved(&service.control)?;
        if !registry.is_registered(service.project.binding)? {
            assert!(registry.is_registered(other.binding)?);
            assert_eq!(
                registry
                    .retained_project(service.project.binding)?
                    .registration,
                service.project
            );
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "missing source was not automatically retired"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
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
            Instant::now() < deadline,
            "automatically retired worker did not drain"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let deadline = Instant::now() + Duration::from_secs(90);
    while data.join("collections-1.4/missing-root").exists() {
        ensure!(
            Instant::now() < deadline,
            "automatic full removal did not delete index files"
        );
        std::thread::sleep(Duration::from_millis(100));
    }

    assert_eq!(
        fs::read_to_string(other_source.join("memo.txt"))?,
        "untouched sibling evidence"
    );
    assert!(!source.exists());
    Ok(())
}
