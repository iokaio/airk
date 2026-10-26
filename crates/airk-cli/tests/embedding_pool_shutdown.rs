// SPDX-License-Identifier: Apache-2.0
#![cfg(feature = "embedding-worker")]
//! A real helper must exit while its authenticated idle model peer stays open.
use airk_cli::{embedding_pool_protocol as pool, local_ipc, service_ipc};
use airk_retrieval::embedding_worker::{ModelLocation, PROTOCOL, Request, Response};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

struct OwnedHelper(Child);
impl Drop for OwnedHelper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn graceful_pool_stop_closes_an_explicitly_held_idle_model_session() -> Result<()> {
    let owner = tempfile::tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    let model = data.join("models/local");
    let control = owner.path().join("control");
    fs::create_dir(&source)?;
    fs::create_dir_all(&model)?;
    local_ipc::prepare_root(&control)?;
    let vocabulary = br#"{"authenticate":[1.0,0.0],"login":[0.0,1.0]}"#;
    let identity = format!("{:x}", Sha256::digest(vocabulary));
    fs::write(model.join("vocabulary.json"), vocabulary)?;
    fs::write(
        model.join("model.json"),
        serde_json::to_vec(&serde_json::json!({"schema":1,
            "name":"original held-session fixture","license":"Apache-2.0",
            "sha256":identity,"dimensions":2}))?,
    )?;
    let instance = airk_kernel::service_broker::new_instance_id()?;
    let project = airk_kernel::service_projects::registration(&source, &data, "idle")?;
    let grant = pool::Grant {
        service: instance,
        pool: instance,
        worker: airk_kernel::service_broker::new_instance_id()?,
        project,
        token: airk_core::identity::Sha256Digest::from_bytes([71; 32]),
    };
    let mut command = Command::new(env!("CARGO_BIN_EXE_airk-embed"));
    command
        .arg("pool")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut helper = OwnedHelper(command.spawn()?);
    let mut input = helper.0.stdin.take().context("helper input missing")?;
    let mut output = helper.0.stdout.take().context("helper output missing")?;
    let (sender, observations) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        while let Ok(Some(observation)) =
            service_ipc::read_frame::<_, pool::Observation>(&mut output)
        {
            if sender.try_send(observation).is_err() {
                break;
            }
        }
    });
    service_ipc::write_frame(
        &mut input,
        &pool::Init {
            runtime: control.clone(),
            service: instance,
            pool: instance,
            inference_threads: 1,
        },
    )?;
    ensure!(
        matches!(observations.recv_timeout(Duration::from_secs(10))?,
        pool::Observation::Ready { pool, role } if pool == instance && role == pool::role(instance)),
        "helper initialization identity mismatch"
    );
    service_ipc::write_frame(
        &mut input,
        &pool::Control::Register {
            grant: grant.clone(),
        },
    )?;
    ensure!(
        matches!(
            observations.recv_timeout(Duration::from_secs(5))?,
            pool::Observation::Registered
        ),
        "helper refused fixture capability"
    );

    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let stream = executor.block_on(async {
        let address = local_ipc::endpoint(&control, &pool::role(instance))?;
        let mut stream =
            tokio::time::timeout(Duration::from_secs(5), local_ipc::connect(&address)).await??;
        local_ipc::write_message(&mut stream, &grant, Duration::from_secs(5)).await?;
        let ready: pool::Observation = local_ipc::read_message(&mut stream, Duration::from_secs(5))
            .await?
            .context("helper peer readiness missing")?;
        ensure!(
            matches!(ready, pool::Observation::Ready { pool, role }
            if pool == instance && role == pool::role(instance)),
            "helper peer identity mismatch"
        );
        let operation = pool::Operation::Call {
            request: Box::new(Request::Open {
                protocol: PROTOCOL,
                version: env!("CARGO_PKG_VERSION").into(),
                location: ModelLocation::Directory { path: model },
                identity: identity.clone(),
            }),
        };
        let envelope = pool::Envelope {
            project: grant.project.binding,
            worker: grant.worker,
            session: airk_kernel::service_broker::new_instance_id()?,
            request_id: airk_kernel::service_broker::new_instance_id()?,
            input_digest: pool::input_digest(&operation)?,
            operation,
        };
        local_ipc::write_message(&mut stream, &envelope, Duration::from_secs(5)).await?;
        let opened: pool::ResultEnvelope =
            local_ipc::read_message(&mut stream, Duration::from_secs(5))
                .await?
                .context("helper open response missing")?;
        ensure!(
            opened.project == envelope.project
                && opened.worker == envelope.worker
                && opened.session == envelope.session
                && opened.request_id == envelope.request_id
                && opened.input_digest == envelope.input_digest,
            "helper response crossed fixture identity"
        );
        ensure!(
            matches!(opened.response, Response::Opened { protocol, version, identity: actual }
            if protocol == PROTOCOL && version == env!("CARGO_PKG_VERSION") && actual == identity),
            "helper did not open the selected model"
        );
        Ok::<_, anyhow::Error>(stream)
    })?;
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        service_ipc::write_frame(&mut input, &pool::Control::Status)?;
        match observations.recv_timeout(Duration::from_secs(5))? {
            pool::Observation::Status {
                loads: 1,
                sessions: 1,
                ..
            } => break,
            pool::Observation::Status { .. } if Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(10));
            }
            observed => anyhow::bail!(
                "held session unavailable: {}",
                serde_json::to_string(&observed)?
            ),
        }
    }
    // Do not send Close or drop the peer. The helper owns a real idle model
    // session and must wake its five-minute transport read during shutdown.
    let until = Instant::now() + Duration::from_secs(5);
    service_ipc::write_frame(&mut input, &pool::Control::Stop)?;
    let exited = loop {
        if let Some(status) = helper.0.try_wait()? {
            break status;
        }
        ensure!(
            Instant::now() < until,
            "graceful helper exit exceeded the control deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    ensure!(exited.success(), "helper did not exit normally: {exited}");
    drop(stream);
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("helper observation reader failed"))?;
    assert_eq!(
        fs::read(data.join("models/local/vocabulary.json"))?,
        vocabulary
    );
    Ok(())
}
