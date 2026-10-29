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
    let mut missing = BTreeMap::new();
    let mut last_pulse = Instant::now();
    while !closing.load(Ordering::Acquire) {
        for _ in 0..32 {
            let command = match receiver.try_recv() {
                Ok(command) => command,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
            };
            match command {
                Command::CleanupRemoved { project } => {
                    watcher.remove(project);
                    missing.remove(&project);
                    epochs.remove(&project);
                    // Full removal never resolves or enumerates the old source.
                    // A busy project lease or filesystem failure retains the durable
                    // guard; the next coordinator sweep resumes the same removal.
                    let mut registry = airk_store::service::registry::ServiceRegistry::open(
                        &init.runtime,
                        init.service,
                    )?;
                    let result = registry.purge_removed_project_turn(project);
                    reply(&Reply::RemovalResult {
                        project,
                        complete: matches!(result, Ok(true)),
                        failed: result.is_err(),
                    })?;
                }
                Command::CheckSource {
                    registration,
                    registration_epoch,
                    configuration_epoch,
                } => {
                    registration.validate()?;
                    let project = registration.binding;
                    let stamp = (registration_epoch, configuration_epoch);
                    if confirmed_absent(std::path::Path::new(&registration.root)) {
                        let now = Instant::now();
                        if let Some((prior, since)) = missing.get(&project)
                            && *prior == stamp
                            && now.duration_since(*since) >= Duration::from_secs(10)
                        {
                            reply(&Reply::MissingSource {
                                project,
                                registration_epoch,
                                configuration_epoch,
                            })?;
                            missing.remove(&project);
                        } else if missing
                            .get(&project)
                            .is_none_or(|(prior, _)| *prior != stamp)
                        {
                            ensure!(
                                missing.contains_key(&project) || missing.len() < 1024,
                                "missing-source probe capacity exhausted"
                            );
                            missing.insert(project, (stamp, now));
                        }
                    } else {
                        missing.remove(&project);
                    }
                }
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

/// A missing leaf under an accessible, canonical parent is positive evidence.
/// An absent drive/ancestor, denied access, redirected parent or replacement file
/// is unavailable instead. No source content is opened and nothing is created.
fn confirmed_absent(root: &std::path::Path) -> bool {
    use std::{fs, io::ErrorKind};
    if !matches!(fs::symlink_metadata(root), Err(error) if error.kind() == ErrorKind::NotFound) {
        return false;
    }
    let Some(parent) = root.parent() else {
        return false;
    };
    parent
        .canonicalize()
        .is_ok_and(|canonical| canonical == parent)
        && fs::read_dir(parent).is_ok()
}

#[cfg(test)]
mod missing_source_tests {
    use super::*;
    #[test]
    fn only_missing_leaf_with_accessible_parent_is_confirmed() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let parent = owner.path().canonicalize()?;
        let root = parent.join("source");
        assert!(confirmed_absent(&root));
        std::fs::create_dir(&root)?;
        assert!(!confirmed_absent(&root));
        std::fs::remove_dir(&root)?;
        assert!(confirmed_absent(&root));
        assert!(!confirmed_absent(&root.join("nested")));
        std::fs::write(&root, b"preserved replacement")?;
        assert!(!confirmed_absent(&root));
        assert_eq!(std::fs::read(&root)?, b"preserved replacement");
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn redirected_parent_does_not_confirm_deletion() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let parent = owner.path().canonicalize()?;
        std::os::unix::fs::symlink(&parent, parent.join("alias"))?;
        assert!(!confirmed_absent(&parent.join("alias/missing")));
        Ok(())
    }
}
