// SPDX-License-Identifier: Apache-2.0
//! Real helper control exchanges in an isolated runtime; no installed service.
#![cfg(feature = "embedding-worker")]
use airk_cli::{embedding_pool_protocol as protocol, service_ipc};
use airk_core::identity::{Incarnation, Sha256Digest};
use anyhow::{Context, Result, ensure};
use std::{
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn missing_and_revoked_grants_repair_without_restarting_healthy_pool() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let runtime = owner.path().join("runtime");
    airk_cli::local_ipc::prepare_root(&runtime)?;
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    std::fs::create_dir(&source)?;
    std::fs::create_dir(&data)?;
    let project = airk_kernel::service_projects::registration(&source, &data, "grant-repair")?;
    let service = Incarnation::from_bytes([1; 16]);
    let pool = Incarnation::from_bytes([2; 16]);
    let worker = Incarnation::from_bytes([3; 16]);
    let sibling = Incarnation::from_bytes([4; 16]);
    let mut child = OwnedChild(
        Command::new(env!("CARGO_BIN_EXE_airk-embed"))
            .arg("pool")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?,
    );
    let mut input = child.0.stdin.take().context("input")?;
    let mut output = child.0.stdout.take().context("output")?;
    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        while let Ok(Some(reply)) = service_ipc::read_frame::<_, protocol::Observation>(&mut output)
        {
            if sender.send(reply).is_err() {
                break;
            }
        }
    });
    let receive =
        || -> Result<protocol::Observation> { Ok(receiver.recv_timeout(Duration::from_secs(10))?) };
    service_ipc::write_frame(
        &mut input,
        &protocol::Init {
            runtime,
            service,
            pool,
            inference_threads: 1,
        },
    )?;
    ensure!(matches!(receive()?, protocol::Observation::Ready {pool: actual,..} if actual==pool));
    for (member, token) in [(sibling, 5), (worker, 6)] {
        service_ipc::write_frame(&mut input, &protocol::Control::Renew { worker: member })?;
        ensure!(
            matches!(receive()?, protocol::Observation::GrantMissing {worker:actual} if actual==member)
        );
        service_ipc::write_frame(
            &mut input,
            &protocol::Control::Register {
                grant: protocol::Grant {
                    service,
                    pool,
                    project: project.clone(),
                    worker: member,
                    token: Sha256Digest::from_bytes([token; 32]),
                },
                worker_identity: None,
            },
        )?;
        ensure!(matches!(receive()?, protocol::Observation::Registered));
    }
    service_ipc::write_frame(&mut input, &protocol::Control::Revoke { worker })?;
    ensure!(matches!(receive()?, protocol::Observation::Revoked));
    service_ipc::write_frame(&mut input, &protocol::Control::Renew { worker })?;
    ensure!(
        matches!(receive()?, protocol::Observation::GrantMissing {worker:actual} if actual==worker)
    );
    service_ipc::write_frame(
        &mut input,
        &protocol::Control::Register {
            grant: protocol::Grant {
                service,
                pool,
                project,
                worker,
                token: Sha256Digest::from_bytes([7; 32]),
            },
            worker_identity: None,
        },
    )?;
    ensure!(matches!(receive()?, protocol::Observation::Registered));
    for member in [worker, sibling] {
        service_ipc::write_frame(&mut input, &protocol::Control::Renew { worker: member })?;
        ensure!(matches!(receive()?, protocol::Observation::Renewed));
    }
    service_ipc::write_frame(&mut input, &protocol::Control::Status)?;
    ensure!(matches!(
        receive()?,
        protocol::Observation::Status {
            loads: 0,
            sessions: 0,
            ..
        }
    ));
    ensure!(child.0.try_wait()?.is_none());
    service_ipc::write_frame(&mut input, &protocol::Control::Stop)?;
    drop(input);
    // Closed control confirms the owned helper drained before waiting on its handle.
    ensure!(matches!(
        receiver.recv_timeout(Duration::from_secs(10)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
    ensure!(child.0.wait()?.success());
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("reply reader panicked"))?;
    Ok(())
}
