// SPDX-License-Identifier: Apache-2.0
//! Path-free observations of running native processes; never installer authority.
use airk_core::service::{RuntimeBuild, RuntimeProcess};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Read, path::Path, sync::OnceLock};

static PROCESS: OnceLock<RuntimeProcess> = OnceLock::new();
static PROCESS_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= limit,
        "invalid runtime metadata"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "redirected runtime metadata"
        );
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "runtime metadata exceeds bound"
    );
    Ok(bytes)
}

#[derive(Deserialize)]
struct Inventory {
    schema: u32,
    version: String,
    files: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct Receipt {
    schema: u32,
    source: Source,
    profile: String,
    rustc: String,
}
#[derive(Deserialize)]
struct Source {
    sha256: airk_core::identity::Sha256Digest,
    files: u64,
}

fn build(
    main: &Path,
    executable: &str,
) -> Result<(airk_core::identity::Sha256Digest, RuntimeBuild)> {
    let image = crate::process_roles::main_image(main)?;
    let main = image.as_path();
    let root = crate::runtime_pair::payload_root(main)?;
    let bytes = bounded(&root.join("airk-install.json"), 512 * 1024)?;
    let payload = format!("{:x}", Sha256::digest(&bytes)).parse()?;
    let inventory: Inventory = serde_json::from_slice(&bytes)?;
    let name = main
        .strip_prefix(root)?
        .to_str()
        .context("runtime image name unavailable")?
        .replace('\\', "/");
    ensure!(
        inventory.schema == 1
            && inventory.version == env!("CARGO_PKG_VERSION")
            && inventory
                .files
                .get(&name)
                .is_some_and(|hash| hash == executable),
        "runtime inventory differs from this image"
    );
    let receipt_bytes = bounded(&root.join("airk-build.json"), 16384)?;
    let hash = format!("{:x}", Sha256::digest(&receipt_bytes));
    ensure!(
        inventory.files.get("airk-build.json") == Some(&hash),
        "build receipt differs from runtime inventory"
    );
    let receipt: Receipt = serde_json::from_slice(&receipt_bytes)?;
    ensure!(
        receipt.schema == 1 && receipt.profile.len() <= 32 && receipt.rustc.len() <= 2048,
        "invalid build receipt"
    );
    Ok((
        payload,
        RuntimeBuild {
            source_sha256: receipt.source.sha256,
            source_files: receipt.source.files,
            profile: receipt.profile,
            rustc: receipt.rustc,
        },
    ))
}

pub(crate) fn process() -> RuntimeProcess {
    PROCESS_STARTED.store(true, std::sync::atomic::Ordering::Release);
    PROCESS
        .get_or_init(|| {
            let executable = crate::runtime_pair::current_executable_sha256().ok();
            let receipt = executable
                .as_ref()
                .and_then(|hash| build(&std::env::current_exe().ok()?, hash).ok());
            RuntimeProcess {
                version: env!("CARGO_PKG_VERSION").into(),
                protocol: airk_core::service::PROTOCOL_VERSION,
                process_id: std::process::id(),
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                executable_sha256: executable.and_then(|hash| hash.parse().ok()),
                payload_sha256: receipt.as_ref().map(|(payload, _)| *payload),
                build: receipt.as_ref().map(|(_, build)| build.clone()),
                metadata_error: receipt
                    .is_none()
                    .then(|| "build_receipt_missing_or_unverified".into()),
            }
        })
        .clone()
}

pub(crate) fn observe_process() -> RuntimeProcess {
    if let Some(process) = PROCESS.get() {
        return process.clone();
    }
    if !PROCESS_STARTED.swap(true, std::sync::atomic::Ordering::AcqRel) {
        // Diagnostic reads must not wait for a cold, potentially large development image hash.
        // Do not attach this observation to a caller's short-lived Tokio runtime:
        // runtime shutdown would otherwise wait for hashing after the status reply.
        if std::thread::Builder::new()
            .name("airk-runtime-identity".into())
            .spawn(|| {
                process();
            })
            .is_err()
        {
            PROCESS_STARTED.store(false, std::sync::atomic::Ordering::Release);
        }
    }
    RuntimeProcess {
        version: env!("CARGO_PKG_VERSION").into(),
        protocol: airk_core::service::PROTOCOL_VERSION,
        process_id: std::process::id(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        executable_sha256: None,
        payload_sha256: None,
        build: None,
        metadata_error: Some("process_identity_pending".into()),
    }
}

pub(crate) async fn bundle(
    control: &Path,
    identity: airk_core::service::WorkerIdentity,
    frontend: Option<RuntimeProcess>,
) -> serde_json::Value {
    use airk_core::service::{ControlMethod, ControlResponse};
    let worker = Some(observe_process());
    let health = crate::service_runtime::request_for_version_and_instance(
        control,
        ControlMethod::Health,
        env!("CARGO_PKG_VERSION"),
        identity.service,
    )
    .await;
    let service = match health {
        Ok(ControlResponse::Health { observation }) if observation.instance == identity.service => {
            Some(observation)
        }
        _ => None,
    };
    let frontend =
        frontend.filter(|value| serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= 4096));
    let versions_match = frontend
        .as_ref()
        .zip(worker.as_ref())
        .zip(service.as_ref())
        .map(|((frontend, worker), service)| {
            frontend.version == worker.version && worker.version == service.version
        });
    let payloads_match = frontend
        .as_ref()
        .and_then(|process| process.payload_sha256)
        .zip(worker.as_ref().and_then(|process| process.payload_sha256))
        .zip(
            service
                .as_ref()
                .and_then(|service| service.runtime_sha256.as_deref()),
        )
        .map(|((frontend, worker), service)| frontend == worker && worker.to_string() == service);
    let builds_match = frontend
        .as_ref()
        .and_then(|process| process.build.as_ref())
        .zip(worker.as_ref().and_then(|process| process.build.as_ref()))
        .zip(service.as_ref().and_then(|service| service.build.as_ref()))
        .map(|((frontend, worker), service)| frontend == worker && worker == service);
    let service = service.map(|health| serde_json::json!({"version":health.version,"protocol":health.protocol,
        "responsive":true,"accepting":health.policy.accepting,"uptime_ms":health.uptime_ms,"diagnostics":health.logs,
        "process_id":health.process_id,"instance":health.instance,"executable_sha256":health.executable_sha256,
        "payload_sha256":health.runtime_sha256,"build":health.build,"metadata_error":health.build_error,
        "basis":"instance_bound_service_health"}));
    serde_json::json!({"schema":1,"observed_unix_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|time| time.as_millis() as u64),
        "mcp_frontend":{"basis":"client_reported","process":frontend},
        "index_worker":{"basis":"running_process","identity":identity,"process":worker},
        "service":service,"service_error":service.is_none().then_some("service_health_unavailable"),
        "versions_match":versions_match,"payloads_match":payloads_match,"builds_match":builds_match})
}

pub(crate) async fn work(
    control: &Path,
    identity: airk_core::service::WorkerIdentity,
) -> serde_json::Value {
    use airk_core::service::ControlMethod;
    let request = |method| {
        crate::service_runtime::request_for_version_and_instance(
            control,
            method,
            env!("CARGO_PKG_VERSION"),
            identity.service,
        )
    };
    let (progress, watch, worker) = tokio::join!(
        request(ControlMethod::Progress {
            project: identity.project
        }),
        request(ControlMethod::Watch {
            project: identity.project
        }),
        request(ControlMethod::Worker {
            project: identity.project
        }),
    );
    work_observation(
        Some(identity),
        identity.project,
        progress.ok(),
        watch.ok(),
        worker.ok(),
    )
}

fn work_observation(
    identity: Option<airk_core::service::WorkerIdentity>,
    project_scope: airk_core::service::ProjectBinding,
    progress_reply: Option<airk_core::service::ControlResponse>,
    watch_reply: Option<airk_core::service::ControlResponse>,
    worker_reply: Option<airk_core::service::ControlResponse>,
) -> serde_json::Value {
    use airk_core::service::{ControlResponse, JobState};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|time| time.as_millis() as u64);
    let mut errors = Vec::new();
    let (progress, pending, queued, queue_truncated, paused, pending_paused, progress_sampled) =
        match progress_reply {
            Some(ControlResponse::Progress {
                project,
                observation,
                pending_job,
                queued_jobs,
                queue_truncated,
                project_paused,
                pending_paused,
                observed_unix_ms,
            }) if project == project_scope
                && observation.as_ref().is_none_or(|value| {
                    value.worker.project == project_scope
                        && value.job.project == project_scope
                        && value.progress.job == value.job.id
                })
                && queued_jobs.iter().all(|job| job.project == project_scope)
                && pending_job
                    .as_ref()
                    .is_none_or(|job| job.project == project_scope) =>
            {
                (
                    observation,
                    pending_job,
                    queued_jobs,
                    queue_truncated,
                    project_paused,
                    pending_paused,
                    observed_unix_ms,
                )
            }
            _ => {
                errors.push("progress_unavailable_or_wrong_project");
                (None, None, Vec::new(), None, None, None, None)
            }
        };
    let current = progress.as_ref().map(|value| {
        identity.is_some_and(|identity| {
            value.worker == identity
                && value.current_fencing_generation == identity.fencing_generation
        })
    });
    let active_job = progress
        .as_ref()
        .filter(|value| {
            current == Some(true)
                && value.job.state == JobState::Running
                && identity.is_some_and(|identity| {
                    value.job.fencing_generation == identity.fencing_generation
                })
        })
        .map(|value| &value.job);
    let (worker, waiting, worker_age) = match worker_reply {
        Some(ControlResponse::Worker {
            project,
            identity: sampled,
            restarting,
            automatic_start_blocked,
            age_ms,
            waiting,
        }) if project == project_scope => {
            let matches = identity.is_some() && sampled == identity;
            (
                Some(
                    serde_json::json!({"identity":sampled,"matches_serving_worker":matches,"restarting":restarting,
                "automatic_start_blocked":automatic_start_blocked}),
                ),
                if matches || identity.is_none() {
                    waiting
                } else {
                    None
                },
                Some(age_ms),
            )
        }
        _ => {
            errors.push("worker_sample_unavailable_or_wrong_project");
            (None, None, None)
        }
    };
    let wait_matched = waiting.as_ref().is_some_and(|waiting| {
        active_job.is_some_and(|job| job.id == waiting.job)
            || pending.as_ref().is_some_and(|job| job.id == waiting.job)
            || queued.iter().any(|job| job.id == waiting.job)
    });
    let wait_state = if worker.is_none() {
        "unavailable"
    } else if wait_matched && identity.is_none() {
        "observed_pending_job_unconfirmed_worker"
    } else if wait_matched {
        "observed"
    } else if waiting.is_some() {
        "unmatched"
    } else {
        "not_observed"
    };
    let watch = match watch_reply {
        Some(ControlResponse::Watch {
            project,
            observation,
        }) if project == project_scope => Some(observation),
        _ => {
            errors.push("watch_observation_unavailable_or_wrong_project");
            None
        }
    };
    let mut progress_value = serde_json::to_value(&progress).unwrap_or(serde_json::Value::Null);
    if let Some(counters) = progress_value
        .get_mut("progress")
        .and_then(serde_json::Value::as_object_mut)
    {
        counters.remove("grant");
    }
    serde_json::json!({"schema":1,"basis":"instance_bound_service_metadata","observed_unix_ms":now,
        "serving_worker":identity,"project_paused":paused,"pending_job":pending,"pending_paused":pending_paused,
        "queued_jobs":queued,"queue_truncated":queue_truncated,
        "progress_sampled_unix_ms":progress_sampled,"progress_current_worker":current,
        "progress_observation_age_ms":progress.as_ref().and_then(|value| now.map(|now| now.saturating_sub(value.observed_unix_ms))),
        "active_job":active_job,"progress":progress_value,"worker":worker,"worker_sample_age_ms":worker_age,
        "wait":{"state":wait_state,"observation":waiting.filter(|_| wait_matched)},"watch":watch,"failed_observations":errors,
        "queue_limit":16,"interpretation":"Health is not progress. Progress is cached; compare sample age and serving worker identity. Unknown totals remain unknown. No matching wait sample does not prove no wait. Use airk_help topic=jobs or topic=indexing for interpretation and CLI management guidance."})
}

/// A completed metadata-only status reply when no index query could be submitted.
pub(crate) async fn unavailable(
    control: &Path,
    project: airk_core::service::ProjectBinding,
) -> serde_json::Value {
    use airk_core::service::{ControlMethod, ControlResponse};
    let health = crate::service_runtime::request(control, ControlMethod::Health).await;
    let service = match health {
        Ok(ControlResponse::Health { observation }) => Some(observation),
        _ => None,
    };
    let mut work = if let Some(service) = &service {
        let request = |method| {
            crate::service_runtime::request_for_version_and_instance(
                control,
                method,
                &service.version,
                service.instance,
            )
        };
        let (progress, watch, worker) = tokio::join!(
            request(ControlMethod::Progress { project }),
            request(ControlMethod::Watch { project }),
            request(ControlMethod::Worker { project })
        );
        work_observation(None, project, progress.ok(), watch.ok(), worker.ok())
    } else {
        work_observation(None, project, None, None, None)
    };
    work["serving_worker_unavailable"] = true.into();
    let runtime = serde_json::json!({"schema":1,"mcp_frontend":{"basis":"client_reported","process":observe_process()},
        "index_worker":{"basis":"service_worker_observation_only","identity":work["worker"]["identity"],"process":null},
        "service":service.as_ref().map(|health| serde_json::json!({"version":health.version,"protocol":health.protocol,
            "process_id":health.process_id,"instance":health.instance,"responsive":true,"accepting":health.policy.accepting,
            "uptime_ms":health.uptime_ms,"diagnostics":health.logs,"executable_sha256":health.executable_sha256,
            "payload_sha256":health.runtime_sha256,"build":health.build,"metadata_error":health.build_error,"basis":"instance_bound_service_health"})),
        "service_error":service.is_none().then_some("service_health_unavailable"),"versions_match":null,"payloads_match":null,"builds_match":null});
    let payload = serde_json::json!({"project":project,"index_state":"unavailable","publication":null,"files":null,"chunks":null,
        "index_error":{"code":"index_worker_unavailable","message":"No index query was submitted. Use the runtime/work observations for service and queued-job diagnosis, then retry status."},
        "runtime":runtime,"work":work});
    serde_json::json!({"content":[{"type":"text","text":payload.to_string()}],"isError":false})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_status_keeps_scoped_pending_jobs_and_only_matches_waits_to_current_owners_and_jobs()
    -> Result<()> {
        use airk_core::service::{ControlResponse, JobKind, JobRecord, JobState, WorkerIdentity};
        use airk_core::service_resources::{WaitObservation, WaitReason};
        let identity = WorkerIdentity {
            service: "11".repeat(16).parse()?,
            worker: "22".repeat(16).parse()?,
            project: "33".repeat(32).parse()?,
            fencing_generation: 7,
        };
        let job = JobRecord {
            id: "44".repeat(32).parse()?,
            project: identity.project,
            kind: JobKind::Index,
            state: JobState::Pending,
            fencing_generation: 7,
            attempts: 0,
            outcome: None,
        };
        let progress = || {
            Some(ControlResponse::Progress {
                project: identity.project,
                observation: None,
                pending_job: Some(job.clone()),
                queued_jobs: vec![job.clone()],
                queue_truncated: Some(false),
                project_paused: Some(false),
                pending_paused: Some(false),
                observed_unix_ms: Some(1),
            })
        };
        let worker = |sampled, job| {
            Some(ControlResponse::Worker {
                project: identity.project,
                identity: Some(sampled),
                restarting: false,
                automatic_start_blocked: false,
                age_ms: 5,
                waiting: Some(WaitObservation {
                    job,
                    reason: WaitReason::MemoryBudget,
                    age_ms: 10,
                }),
            })
        };
        let matched = work_observation(
            Some(identity),
            identity.project,
            progress(),
            None,
            worker(identity, job.id),
        );
        assert_eq!(matched["pending_job"]["id"], job.id.to_string());
        assert_eq!(matched["wait"]["observation"]["reason"], "memory_budget");
        assert_eq!(matched["wait"]["state"], "observed");
        assert_eq!(matched["progress_current_worker"], serde_json::Value::Null);
        assert!(
            !matched["failed_observations"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let mut stale = identity;
        stale.fencing_generation += 1;
        assert!(
            work_observation(
                Some(identity),
                identity.project,
                progress(),
                None,
                worker(stale, job.id)
            )["wait"]["observation"]
                .is_null()
        );
        let unrelated = work_observation(
            Some(identity),
            identity.project,
            progress(),
            None,
            worker(identity, "55".repeat(32).parse()?),
        );
        assert_eq!(unrelated["wait"]["state"], "unmatched");
        assert!(unrelated["wait"]["observation"].is_null());
        let mut foreign = job.clone();
        foreign.project = "66".repeat(32).parse()?;
        let rejected = work_observation(
            Some(identity),
            identity.project,
            Some(ControlResponse::Progress {
                project: identity.project,
                observation: None,
                pending_job: Some(foreign),
                queued_jobs: Vec::new(),
                queue_truncated: Some(false),
                project_paused: None,
                pending_paused: None,
                observed_unix_ms: None,
            }),
            None,
            None,
        );
        assert!(rejected["pending_job"].is_null());
        let fallback = work_observation(
            None,
            identity.project,
            progress(),
            None,
            worker(identity, job.id),
        );
        assert!(fallback["serving_worker"].is_null());
        assert_eq!(
            fallback["wait"]["state"],
            "observed_pending_job_unconfirmed_worker"
        );
        assert_eq!(fallback["wait"]["observation"]["reason"], "memory_budget");
        Ok(())
    }

    #[test]
    fn build_metadata_requires_bounded_receipt_and_inventory_bound_to_this_executable() -> Result<()>
    {
        let root = tempfile::tempdir()?;
        let main = root.path().join("airk.exe");
        fs::write(&main, b"original fictional executable")?;
        let executable = crate::runtime_pair::digest_file(&main)?;
        let receipt = serde_json::to_vec(
            &serde_json::json!({"schema":1,"source":{"sha256":"a".repeat(64),"files":42},
            "profile":"debug","rustc":"rustc original fixture"}),
        )?;
        let save = |receipt: &[u8]| -> Result<()> {
            fs::write(root.path().join("airk-build.json"), receipt)?;
            fs::write(
                root.path().join("airk-install.json"),
                serde_json::to_vec(&serde_json::json!({"schema":1,
                "version":env!("CARGO_PKG_VERSION"),"files":{"airk.exe":executable,"airk-build.json":format!("{:x}",Sha256::digest(receipt))}}))?,
            )?;
            Ok(())
        };
        save(&receipt)?;
        let (_, observed) = build(&main, &executable)?;
        assert_eq!(observed.source_files, 42);
        assert_eq!(observed.profile, "debug");
        let inventory_path = root.path().join("airk-install.json");
        let mut inventory: serde_json::Value = serde_json::from_slice(&fs::read(&inventory_path)?)?;
        for index in 0..800 {
            inventory["files"][format!("notices/fictional-{index}/LICENSE")] =
                "a".repeat(64).into();
        }
        let inventory = serde_json::to_vec(&inventory)?;
        assert!(inventory.len() > 65536);
        fs::write(&inventory_path, inventory)?;
        assert_eq!(
            build(&main, &executable)?.1.source_files,
            42,
            "real legal inventories can exceed 64 KiB"
        );
        assert!(build(&main, &"b".repeat(64)).is_err());
        fs::write(root.path().join("airk-build.json"), b"changed receipt")?;
        assert!(build(&main, &executable).is_err());
        save(&vec![b' '; 16385])?;
        assert!(build(&main, &executable).is_err());
        Ok(())
    }
}
