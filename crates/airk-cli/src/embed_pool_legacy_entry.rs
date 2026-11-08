// SPDX-License-Identifier: Apache-2.0
// Historical helper process entry retained for predecessor qualification.
use airk_cli::{local_ipc::Listener, service_ipc};
use std::sync::OnceLock;
use tokio::{sync::Semaphore, task::JoinSet};
static PROCESS_LEASE: OnceLock<airk_store::service::EmbeddingLease> = OnceLock::new();
pub(crate) async fn run_guarded_with_engine(
    bootstrap: Option<airk_cli::embedding_pool_bootstrap::GuardedBootstrap>,
    make_engine: impl FnOnce(&protocol::Init, Arc<AtomicBool>) -> Result<Engine>,
) -> Result<()> {
    let init: protocol::Init = service_ipc::read_frame(&mut std::io::stdin().lock())?
        .context("pool initialization missing")?;
    if let Some(bootstrap) = bootstrap.as_ref() {
        bootstrap.validate_init(&init)?;
    } else {
        ensure!(
            !airk_cli::embedding_pool_bootstrap::shared_mode(&init.runtime)?,
            "activated authority-1 helper requires guarded native startup"
        );
    }
    ensure!(
        (1..=4).contains(&init.inference_threads),
        "invalid pool inference allowance"
    );
    local_ipc::prepare_root(&init.runtime)?;
    ensure!(
        PROCESS_LEASE
            .set(airk_store::service::EmbeddingLease::acquire(&init.runtime)?)
            .is_ok(),
        "embedding process already owns a pool lease"
    );
    let role = protocol::role(init.pool);
    let mut listener = Listener::bind(&local_ipc::endpoint(&init.runtime, &role)?)?;
    let (sender, mut receiver) = mpsc::channel::<Work>(8);
    let closing = Arc::new(AtomicBool::new(false));
    let engine_closing = closing.clone();
    let inference_threads = Arc::new(AtomicUsize::new(usize::from(init.inference_threads)));
    let engine_threads = inference_threads.clone();
    let observation = Arc::new(Mutex::new((0u64, 0u64, 0usize, Instant::now())));
    let engine_observation = observation.clone();
    let model_engine = make_engine(&init, closing.clone())?;
    let engine = std::thread::Builder::new()
        .name("airk-pooled-inference".into())
        .stack_size(8 << 20)
        .spawn(move || {
            // This is the explicit historical provider qualification image,
            // not the modern service role or a general cfg(test) bypass. Its
            // guarded bootstrap and actual EmbeddingLease were acquired above.
            #[cfg(test)]
            let _qualification_scope =
                unsafe { airk_retrieval::native_inference::ServiceComputeScope::enter() }
                    .expect("native qualification compute scope");
            let mut engine = model_engine;
            engine.inference_threads = engine_threads;
            while let Some(work) = receiver.blocking_recv() {
                process_work(&mut engine, work, &engine_closing);
                if let Ok(mut observation) = engine_observation.lock() {
                    *observation = (
                        engine.loads,
                        engine.reuses,
                        engine.sessions.len(),
                        Instant::now(),
                    );
                }
            }
        })?;
    let state = Arc::new(State {
        service: init.service,
        pool: init.pool,
        role: role.clone(),
        grants: Mutex::new(BTreeMap::new()),
        retirement: Mutex::new(retirement::Retirements::default()),
        sender,
        closing: closing.clone(),
        accounting: None,
    });
    service_ipc::write_frame(
        &mut std::io::stdout().lock(),
        &protocol::Observation::Ready {
            pool: init.pool,
            role,
        },
    )?;
    let (control, mut commands) = mpsc::channel::<protocol::Control>(8);
    let input_closing = closing.clone();
    std::thread::Builder::new()
        .name("airk-pool-control".into())
        .spawn(move || {
            while let Ok(Some(command)) = service_ipc::read_frame(&mut std::io::stdin().lock()) {
                if control.blocking_send(command).is_err() {
                    break;
                }
            }
            input_closing.store(true, Ordering::Release);
        })?;
    let slots = Arc::new(Semaphore::new(64));
    let mut connections = JoinSet::new();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    while !closing.load(Ordering::Acquire) {
        tokio::select! {
            command=commands.recv()=>{
                let observation=match command {
                    Some(protocol::Control::ConfigureThreads{threads})=>{
                        if (1..=4).contains(&threads){inference_threads.store(usize::from(threads),Ordering::Release);protocol::Observation::Renewed}else{protocol::Observation::Failed}
                    },
                    Some(protocol::Control::Renew{worker})=>{
                        let mut grants=state.grants.lock().map_err(|_|anyhow::anyhow!("pool authorization unavailable"))?;
                        let authorization=grants.values_mut().find(|auth|auth.grant.worker==worker&&auth.alive.load(Ordering::Acquire)&&Instant::now()<auth.expires);
                        if let Some(authorization)=authorization {authorization.expires=Instant::now()+Duration::from_secs(300);protocol::Observation::Renewed}else{protocol::Observation::GrantMissing{worker}}
                    },
                    Some(protocol::Control::Register{grant,worker_identity})=>{
                        let mut retirement=state.retirement.lock().map_err(|_|anyhow::anyhow!("pool membership unavailable"))?;
                        let mut grants=state.grants.lock().map_err(|_|anyhow::anyhow!("pool authorization unavailable"))?;
                        retirement.register(&mut grants,(state.service,state.pool),grant,worker_identity,Instant::now())
                    },
                    Some(protocol::Control::Revoke{worker})=>{
                        let mut retirement=state.retirement.lock().map_err(|_|anyhow::anyhow!("pool membership unavailable"))?;
                        let mut grants=state.grants.lock().map_err(|_|anyhow::anyhow!("pool authorization unavailable"))?;
                        retirement.revoke(&mut grants,worker);
                        let _=state.sender.try_send(Work::Release{worker,owner:None});
                        protocol::Observation::Revoked
                    },
                    Some(protocol::Control::BeginDrain{ticket})=>drain_control(&state,retirement::Control::Begin,ticket)?,
                    Some(protocol::Control::PollDrain{ticket})=>drain_control(&state,retirement::Control::Poll,ticket)?,
                    Some(protocol::Control::ForgetDrain{ticket})=>drain_control(&state,retirement::Control::Forget,ticket)?,
                    Some(protocol::Control::Status)=>{
                        match observation.lock(){Ok(observation)=>protocol::Observation::Status{loads:observation.0,reuses:observation.1,sessions:observation.2,age_ms:observation.3.elapsed().as_millis().min(u64::MAX as u128) as u64},Err(_)=>protocol::Observation::Failed}
                    },
                    Some(protocol::Control::Stop)|None=>break,
                };
                service_ipc::write_frame(&mut std::io::stdout().lock(),&observation)?;
            },
            stream=listener.accept()=>{
                let stream=match stream{Ok(stream)=>stream,Err(error) if error.downcast_ref::<local_ipc::PeerRejected>().is_some()=>continue,Err(error)=>return Err(error)};
                if let Ok(permit)=slots.clone().try_acquire_owned(){let state=state.clone();connections.spawn(async move{let _permit=permit;connection(stream,state).await});}
            },
            _=connections.join_next(),if !connections.is_empty()=>{},
            _=tick.tick()=>{
                if let Some(bootstrap) = bootstrap.as_ref() { bootstrap.enforce_native_owner()?; }
                state.retirement.lock().map_err(|_|anyhow::anyhow!("pool membership unavailable"))?.retry(&state.sender);
                if state.sender.capacity()==8{let _=state.sender.try_send(Work::Sweep);}
                state.expire_grants(Instant::now())?;
            }
        }
    }
    closing.store(true, Ordering::Release);
    // Idle peers may be waiting on a five-minute read deadline. Closing
    // admission must also wake those transports before joining the engine.
    // Aborting a connection never releases the process lease or its charged
    // inference work: the engine drains and joins below before this owner exits.
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    drop(state);
    engine
        .join()
        .map_err(|_| anyhow::anyhow!("pool execution thread failed"))?;
    Ok(())
}
