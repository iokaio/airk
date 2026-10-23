// SPDX-License-Identifier: Apache-2.0
//! Native foreground service control loop, independent of project stores and queries.
use crate::local_ipc::{self, Listener, Stream};
use airk_core::service::{
    ControlMethod, ControlRequest, ControlResponse, PROTOCOL_VERSION, ServiceHealth,
};
use airk_kernel::service_broker::{Broker, BrokerLimits, new_instance_id};
use anyhow::{Result, ensure};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::{Notify, Semaphore},
    task::JoinSet,
};

struct State {
    policy: Broker,
    started: Instant,
    user: String,
    executable_sha256: String,
    shutdown: Notify,
    management: crate::service_management::Management,
}

async fn connection(mut stream: Stream, state: Arc<State>) -> Result<()> {
    let Some(request): Option<ControlRequest> =
        local_ipc::read_message(&mut stream, Duration::from_secs(5)).await?
    else {
        return Ok(());
    };
    let shutdown =
        request.protocol == PROTOCOL_VERSION && matches!(request.method, ControlMethod::Shutdown);
    let response = if request.protocol != PROTOCOL_VERSION {
        ControlResponse::Incompatible {
            protocol: PROTOCOL_VERSION,
        }
    } else {
        match request.method {
            ControlMethod::Health => {
                let now = Instant::now();
                let policy = state.policy.status(now);
                ControlResponse::Health {
                    observation: Box::new(ServiceHealth {
                        version: env!("CARGO_PKG_VERSION").into(),
                        protocol: PROTOCOL_VERSION,
                        instance: policy.service,
                        process_id: std::process::id(),
                        executable_sha256: state.executable_sha256.clone(),
                        user: state.user.clone(),
                        uptime_ms: state.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                        policy,
                    }),
                }
            }
            ControlMethod::Shutdown => ControlResponse::Stopping,
            method => state.management.request(method).await,
        }
    };
    local_ipc::write_message(&mut stream, &response, Duration::from_secs(5)).await?;
    if shutdown {
        state.shutdown.notify_one();
    }
    Ok(())
}

pub async fn serve(root: &Path) -> Result<()> {
    local_ipc::prepare_root(root)?;
    let _ownership = airk_store::service::ServiceLease::acquire(root)?;
    _ownership.prepare_control_endpoint()?;
    let address = local_ipc::endpoint(root, "control")?;
    let mut listener = Listener::bind(&address)?;
    let instance = new_instance_id()?;
    let state = Arc::new(State {
        policy: Broker::new(instance, BrokerLimits::default())?,
        started: Instant::now(),
        user: local_ipc::current_user()?,
        executable_sha256: crate::runtime_pair::digest_file(&std::env::current_exe()?)?,
        shutdown: Notify::new(),
        management: crate::service_management::Management::start(root.canonicalize()?, instance)?,
    });
    let admission = Arc::new(Semaphore::new(64));
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            stream = listener.accept() => {
                let stream = match stream {
                    Ok(stream) => stream,
                    Err(error) if error.downcast_ref::<local_ipc::PeerRejected>().is_some() => continue,
                    Err(error) => return Err(error),
                };
                if let Ok(permit) = admission.clone().try_acquire_owned() {
                    let state = state.clone();
                    connections.spawn(async move { let _permit = permit; connection(stream, state).await });
                }
            },
            _ = state.shutdown.notified() => break,
            signal = tokio::signal::ctrl_c() => { signal?; break; },
            _ = connections.join_next(), if !connections.is_empty() => {},
        }
    }
    // All handlers have finite read/write deadlines and hold no project publication state.
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while connections.join_next().await.is_some() {}
    })
    .await;
    connections.shutdown().await;
    Ok(())
}

pub async fn request(root: &Path, method: ControlMethod) -> Result<ControlResponse> {
    let address = local_ipc::endpoint(root, "control")?;
    let mut stream =
        tokio::time::timeout(Duration::from_secs(5), local_ipc::connect(&address)).await??;
    local_ipc::write_message(
        &mut stream,
        &ControlRequest {
            protocol: PROTOCOL_VERSION,
            method,
        },
        Duration::from_secs(5),
    )
    .await?;
    let response = local_ipc::read_message(&mut stream, Duration::from_secs(5))
        .await?
        .ok_or_else(|| anyhow::anyhow!("service disconnected before control response"))?;
    if let ControlResponse::Health { observation } = &response {
        ensure!(
            observation.protocol == PROTOCOL_VERSION
                && observation.version == env!("CARGO_PKG_VERSION")
                && observation.user == local_ipc::current_user()?,
            "incompatible native service identity"
        );
    }
    Ok(response)
}
