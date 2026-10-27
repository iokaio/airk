// SPDX-License-Identifier: Apache-2.0
//! Read-only directory notifications in a separately supervised lightweight process.
use crate::{
    service_ipc,
    source_protocol::{self as protocol, Command, Reply},
};
use airk_core::service::PROTOCOL_VERSION;
use airk_kernel::source_watch::{Binding, SourceWatcher};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

static OWNERSHIP: OnceLock<airk_store::service::SourceWatchLease> = OnceLock::new();

struct Pending {
    registration: airk_core::service::ProjectRegistration,
    epoch: u64,
    observed: bool,
    expected: u16,
    digest: airk_core::identity::Sha256Digest,
    roots: Vec<String>,
}

fn reply(reply: &Reply) -> Result<()> {
    Ok(service_ipc::write_frame(
        &mut std::io::stdout().lock(),
        reply,
    )?)
}

pub fn run() -> Result<()> {
    let init: protocol::Init = service_ipc::read_frame(&mut std::io::stdin().lock())?
        .context("watcher initialization missing")?;
    ensure!(
        init.protocol == PROTOCOL_VERSION && init.version == env!("CARGO_PKG_VERSION"),
        "incompatible watcher initialization"
    );
    crate::local_ipc::prepare_root(&init.runtime)?;
    OWNERSHIP
        .set(airk_store::service::SourceWatchLease::acquire(
            &init.runtime,
        )?)
        .map_err(|_| anyhow::anyhow!("watcher ownership already established"))?;
    reply(&Reply::Ready {
        service: init.service,
        worker: init.worker,
    })?;
    let activation: Command = service_ipc::read_frame(&mut std::io::stdin().lock())?
        .context("watcher activation missing")?;
    ensure!(
        matches!(activation,Command::Activate {service,worker} if service==init.service && worker==init.worker),
        "watcher activation identity mismatch"
    );
    // No source traversal or native directory registration precedes activation.
    let mut watcher = SourceWatcher::new();
    reply(&Reply::Activated)?;
    let closing = Arc::new(AtomicBool::new(false));
    let reader_closing = closing.clone();
    let (sender, receiver) = mpsc::sync_channel(8);
    std::thread::Builder::new()
        .name("airk-watch-control".into())
        .spawn(move || {
            let mut input = std::io::stdin().lock();
            while let Ok(Some(command)) = service_ipc::read_frame::<_, Command>(&mut input) {
                if sender.send(command).is_err() {
                    break;
                }
            }
            reader_closing.store(true, Ordering::Release);
        })?;
    let mut pending: Option<Pending> = None;
    let mut epochs: BTreeMap<
        airk_core::service::ProjectBinding,
        (u64, Option<airk_core::identity::Sha256Digest>, bool),
    > = BTreeMap::new();
    let mut last_pulse = Instant::now();
    while !closing.load(Ordering::Acquire) {
        for _ in 0..32 {
            let command = match receiver.try_recv() {
                Ok(command) => command,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
            };
            match command {
                Command::Stop => return Ok(()),
                Command::Activate { .. } => anyhow::bail!("watcher already active"),
                Command::Begin {
                    registration,
                    configuration_epoch,
                    configuration_observed,
                    additional_roots,
                    digest,
                } => {
                    registration.validate()?;
                    ensure!(additional_roots <= 16, "watcher root count exceeds bound");
                    pending = Some(Pending {
                        registration,
                        epoch: configuration_epoch,
                        observed: configuration_observed,
                        expected: additional_roots,
                        digest,
                        roots: Vec::new(),
                    });
                }
                Command::Root {
                    project,
                    configuration_epoch,
                    path,
                } => {
                    let pending = pending.as_mut().context("watcher root has no binding")?;
                    ensure!(
                        pending.registration.binding == project
                            && pending.epoch == configuration_epoch
                            && pending.roots.len() < usize::from(pending.expected)
                            && !path.is_empty()
                            && path.len() <= 4096
                            && !path.chars().any(char::is_control),
                        "invalid watcher root frame"
                    );
                    pending.roots.push(path);
                }
                Command::Commit {
                    project,
                    configuration_epoch,
                } => {
                    let pending = pending.take().context("watcher commit has no binding")?;
                    ensure!(
                        pending.registration.binding == project
                            && pending.epoch == configuration_epoch
                            && pending.roots.len() == usize::from(pending.expected)
                            && protocol::binding_digest(
                                &pending.registration,
                                pending.epoch,
                                pending.observed,
                                &pending.roots
                            )? == pending.digest,
                        "watcher binding digest or identity mismatch"
                    );
                    if epochs
                        .get(&project)
                        .is_some_and(|(epoch, _, _)| *epoch > configuration_epoch)
                    {
                        continue;
                    }
                    ensure!(
                        epochs.contains_key(&project) || epochs.len() < 1024,
                        "watcher project epoch capacity exhausted"
                    );
                    if let Some((epoch, digest, installed)) = epochs.get(&project)
                        && *epoch == configuration_epoch
                    {
                        ensure!(
                            *digest == Some(pending.digest),
                            "watcher configuration changed without a new epoch"
                        );
                        if *installed {
                            continue;
                        }
                    }
                    let digest = pending.digest;
                    let installed = watcher.install(Binding {
                        registration: pending.registration,
                        epoch: pending.epoch,
                        configuration_observed: pending.observed,
                        additional_roots: pending.roots,
                    })?;
                    epochs.insert(project, (configuration_epoch, Some(digest), installed));
                }
                Command::Remove {
                    project,
                    configuration_epoch,
                } => {
                    if epochs
                        .get(&project)
                        .is_none_or(|(epoch, _, _)| *epoch <= configuration_epoch)
                    {
                        ensure!(
                            epochs.contains_key(&project) || epochs.len() < 1024,
                            "watcher project epoch capacity exhausted"
                        );
                        epochs.insert(project, (configuration_epoch, None, false));
                        watcher.remove(project);
                    }
                }
                Command::Acknowledge {
                    project,
                    configuration_epoch,
                    sequence,
                } => watcher.acknowledge(project, configuration_epoch, sequence),
            }
        }
        watcher.poll()?;
        for observation in watcher.observations(16) {
            reply(&Reply::Observation {
                project: observation.project,
                configuration_epoch: observation.epoch,
                native_roots: observation.native_roots,
                backend: observation.backend,
                failure: observation.failure,
                traversal: observation.traversal,
            })?;
        }
        for event in watcher.notices(16)? {
            reply(&Reply::Event {
                project: event.project,
                configuration_epoch: event.epoch,
                sequence: event.sequence,
            })?;
        }
        if last_pulse.elapsed() >= Duration::from_millis(250) {
            let (directories, projects) = watcher.counts();
            reply(&Reply::Pulse {
                watched_directories: directories,
                projects,
                pending_events: watcher.pending_count(),
            })?;
            last_pulse = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}
