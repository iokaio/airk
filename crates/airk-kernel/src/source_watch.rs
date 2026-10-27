// SPDX-License-Identifier: Apache-2.0
//! Bounded directory notifications. The native adapter isolates filesystem stalls
//! in a supervised watcher process; this engine never opens index or source bytes.
use crate::watcher::{comparison_path, deliver, relevant};
use airk_core::{
    service::{ProjectBinding, ProjectRegistration},
    service_watch::{UnreadableDirectory, WatchBackend, WatchFailure, WatchTraversal},
};
use anyhow::{Result, ensure};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};

const MAX_PROJECTS: usize = 1024;
const MAX_BINDING_BYTES: usize = 8 * 1024 * 1024;
const MAX_DIRECTORIES: usize = airk_core::service_watch::MAX_WATCH_DIRECTORIES as usize;
const MAX_PROJECT_DIRECTORIES: usize =
    airk_core::service_watch::MAX_PROJECT_WATCH_DIRECTORIES as usize;
const MAX_PATH_BYTES: usize = 64 * 1024 * 1024;
const MAX_PROJECT_PATH_BYTES: usize = 16 * 1024 * 1024;
const MAX_SCAN_ENTRIES: usize = 262_144;
const MAX_DEPTH: usize = 64;

pub fn memory_allowance() -> u64 {
    airk_core::service_watch::watcher_memory_bytes(crate::limits::machine::memory_bytes())
}

pub struct Binding {
    pub registration: ProjectRegistration,
    pub epoch: u64,
    pub configuration_observed: bool,
    pub additional_roots: Vec<String>,
}
impl Binding {
    fn roots(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.registration.root.as_str())
            .chain(self.additional_roots.iter().map(String::as_str))
    }
    fn weight(&self) -> Result<usize> {
        Ok(serde_json::to_vec(&(
            &self.registration,
            self.epoch,
            self.configuration_observed,
            &self.additional_roots,
        ))?
        .len())
    }
}
pub struct Observation {
    pub project: ProjectBinding,
    pub epoch: u64,
    pub native_roots: u16,
    pub backend: WatchBackend,
    pub failure: Option<WatchFailure>,
    pub traversal: Option<WatchTraversal>,
}
pub struct Notice {
    pub project: ProjectBinding,
    pub epoch: u64,
    pub sequence: u64,
}
struct Dirty {
    first: Instant,
    last: Instant,
}
struct Entry {
    binding: Binding,
    weight: usize,
    dirty: Option<Dirty>,
    sequence: u64,
    inflight: Option<(u64, Instant)>,
    periodic: Instant,
}
struct RootTask {
    project: ProjectBinding,
    index: usize,
    root: PathBuf,
    data: PathBuf,
}
struct Walk {
    task: RootTask,
    pending: Vec<(PathBuf, usize)>,
    queued_bytes: usize,
    cursor: Option<(PathBuf, usize, fs::ReadDir)>,
}
struct Plan {
    tasks: VecDeque<RootTask>,
    walk: Option<Walk>,
    wanted: BTreeSet<PathBuf>,
    path_bytes: usize,
    usage: BTreeMap<ProjectBinding, ProjectUsage>,
    failures: BTreeMap<ProjectBinding, Vec<Option<WatchFailure>>>,
    publishing: bool,
    published_after: Option<PathBuf>,
}
#[derive(Default)]
struct ProjectUsage {
    directories: usize,
    path_bytes: usize,
    entries: usize,
    traversal: WatchTraversal,
}

pub struct SourceWatcher {
    entries: BTreeMap<ProjectBinding, Entry>,
    weight: usize,
    root_index: BTreeMap<PathBuf, BTreeSet<ProjectBinding>>,
    watcher: Option<RecommendedWatcher>,
    receiver: Receiver<Vec<PathBuf>>,
    sender: SyncSender<Vec<PathBuf>>,
    overflow: Arc<AtomicBool>,
    topology: Arc<AtomicBool>,
    notice_after: Option<ProjectBinding>,
    watched: BTreeSet<PathBuf>,
    plan: Option<Plan>,
    refresh: Instant,
    observations: BTreeMap<ProjectBinding, Observation>,
    force_rebind: bool,
}
impl Default for SourceWatcher {
    fn default() -> Self {
        Self::new()
    }
}
impl SourceWatcher {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::sync_channel(256);
        let mut engine = Self {
            entries: BTreeMap::new(),
            weight: 0,
            root_index: BTreeMap::new(),
            watcher: None,
            receiver,
            sender,
            overflow: Arc::new(AtomicBool::new(false)),
            topology: Arc::new(AtomicBool::new(false)),
            notice_after: None,
            watched: BTreeSet::new(),
            plan: None,
            refresh: Instant::now(),
            observations: BTreeMap::new(),
            force_rebind: false,
        };
        engine.connect();
        engine
    }
    fn connect(&mut self) {
        let sender = self.sender.clone();
        let overflow = self.overflow.clone();
        let topology = self.topology.clone();
        self.watcher = RecommendedWatcher::new(
            move |event: notify::Result<notify::Event>| {
                if event.as_ref().is_ok_and(|event| {
                    matches!(
                        event.kind,
                        notify::EventKind::Create(_)
                            | notify::EventKind::Remove(_)
                            | notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
                            | notify::EventKind::Any
                            | notify::EventKind::Other
                    )
                }) {
                    topology.store(true, Ordering::Release);
                }
                deliver(&sender, &overflow, event)
            },
            notify::Config::default().with_follow_symlinks(false),
        )
        .ok();
    }
    pub fn install(&mut self, binding: Binding) -> Result<bool> {
        binding.registration.validate()?;
        ensure!(binding.additional_roots.len() <= 16, "too many watch roots");
        let mut roots = BTreeSet::new();
        for root in binding.roots() {
            ensure!(
                !root.is_empty()
                    && root.len() <= 4096
                    && !root.chars().any(char::is_control)
                    && Path::new(root).is_absolute()
                    && roots.insert(root),
                "invalid watch root binding"
            );
        }
        let weight = binding.weight()?;
        let project = binding.registration.binding;
        self.remove(project);
        if self.entries.len() >= MAX_PROJECTS || self.weight + weight > MAX_BINDING_BYTES {
            self.observations.insert(
                project,
                Observation {
                    project,
                    epoch: binding.epoch,
                    native_roots: 0,
                    backend: WatchBackend::PeriodicFallback,
                    failure: Some(WatchFailure::RootLimit),
                    traversal: None,
                },
            );
            return Ok(false);
        }
        let now = Instant::now();
        self.weight += weight;
        self.entries.insert(
            project,
            Entry {
                binding,
                weight,
                dirty: Some(Dirty {
                    first: now.checked_sub(Duration::from_secs(2)).unwrap_or(now),
                    last: now.checked_sub(Duration::from_secs(2)).unwrap_or(now),
                }),
                sequence: 0,
                inflight: None,
                periodic: now,
            },
        );
        self.rebuild_index();
        self.plan = None;
        self.refresh = now.checked_sub(Duration::from_secs(10)).unwrap_or(now);
        Ok(true)
    }
    pub fn remove(&mut self, project: ProjectBinding) {
        if let Some(entry) = self.entries.remove(&project) {
            self.weight = self.weight.saturating_sub(entry.weight);
            self.rebuild_index();
            self.plan = None;
            self.refresh = Instant::now()
                .checked_sub(Duration::from_secs(10))
                .unwrap_or_else(Instant::now);
        }
        self.observations.remove(&project);
    }
    fn rebuild_index(&mut self) {
        self.root_index.clear();
        for (id, entry) in &self.entries {
            for root in entry.binding.roots() {
                self.root_index
                    .entry(comparison_path(Path::new(root)))
                    .or_default()
                    .insert(*id);
            }
        }
    }
    fn dirty(entry: &mut Entry, now: Instant) {
        if let Some(dirty) = entry.dirty.as_mut() {
            dirty.last = now;
        } else {
            entry.dirty = Some(Dirty {
                first: now,
                last: now,
            });
        }
    }
    pub fn acknowledge(&mut self, project: ProjectBinding, epoch: u64, sequence: u64) {
        if let Some(entry) = self.entries.get_mut(&project)
            && entry.binding.epoch == epoch
            && entry
                .inflight
                .is_some_and(|(pending, _)| pending == sequence)
        {
            entry.inflight = None;
        }
    }
    pub fn poll(&mut self) -> Result<()> {
        let now = Instant::now();
        if self.overflow.swap(false, Ordering::AcqRel) {
            for (project, entry) in &mut self.entries {
                Self::dirty(entry, now);
                self.observations.insert(
                    *project,
                    Observation {
                        project: *project,
                        epoch: entry.binding.epoch,
                        native_roots: 0,
                        backend: WatchBackend::PeriodicFallback,
                        failure: Some(WatchFailure::EventOverflow),
                        traversal: None,
                    },
                );
            }
            self.plan = None;
            self.refresh = now.checked_sub(Duration::from_secs(10)).unwrap_or(now);
        }
        if self.topology.swap(false, Ordering::AcqRel) {
            self.refresh = now.checked_sub(Duration::from_secs(10)).unwrap_or(now);
        }
        for _ in 0..16 {
            let Ok(paths) = self.receiver.try_recv() else {
                break;
            };
            for path in paths {
                let normalized = comparison_path(&path);
                let projects = normalized
                    .ancestors()
                    .take(128)
                    .filter_map(|ancestor| self.root_index.get(ancestor))
                    .flatten()
                    .copied()
                    .collect::<BTreeSet<_>>();
                for project in projects {
                    let entry = self.entries.get_mut(&project).unwrap();
                    if entry.binding.roots().any(|root| {
                        relevant(
                            &path,
                            Path::new(root),
                            Path::new(&entry.binding.registration.data_root),
                        )
                    }) {
                        Self::dirty(entry, now);
                        if self.root_index.contains_key(&normalized) {
                            self.force_rebind = true;
                        }
                    }
                }
            }
        }
        for entry in self.entries.values_mut() {
            if now.duration_since(entry.periodic) >= Duration::from_secs(300) {
                entry.periodic = now;
                Self::dirty(entry, now);
            }
        }
        if self.plan.is_none() && now.duration_since(self.refresh) >= Duration::from_secs(10) {
            self.refresh = now;
            if self.watcher.is_none() {
                self.connect();
            }
            if self.watcher.is_some() {
                let mut tasks = VecDeque::new();
                let mut failures = BTreeMap::new();
                for (project, entry) in &self.entries {
                    failures.insert(
                        *project,
                        vec![None; 1 + entry.binding.additional_roots.len()],
                    );
                    for (index, root) in entry.binding.roots().enumerate() {
                        tasks.push_back(RootTask {
                            project: *project,
                            index,
                            root: PathBuf::from(root),
                            data: PathBuf::from(&entry.binding.registration.data_root),
                        });
                    }
                }
                self.plan = Some(Plan {
                    tasks,
                    walk: None,
                    wanted: BTreeSet::new(),
                    path_bytes: 0,
                    usage: BTreeMap::new(),
                    failures,
                    publishing: false,
                    published_after: None,
                });
            } else {
                for (project, entry) in &self.entries {
                    self.observations.insert(
                        *project,
                        Observation {
                            project: *project,
                            epoch: entry.binding.epoch,
                            native_roots: 0,
                            backend: WatchBackend::PeriodicFallback,
                            failure: Some(WatchFailure::BackendUnavailable),
                            traversal: None,
                        },
                    );
                }
            }
        }
        if let Some(plan) = self.plan.as_mut()
            && (plan.publishing || plan.step()?)
        {
            let plan = self.plan.take().unwrap();
            self.plan = self.publish_plan(plan)?;
        }
        Ok(())
    }
    fn publish_plan(&mut self, mut plan: Plan) -> Result<Option<Plan>> {
        plan.publishing = true;
        if self.force_rebind {
            plan.published_after = None;
        }
        if let Some(watcher) = self.watcher.as_mut() {
            let removals = self
                .watched
                .iter()
                .filter(|path| self.force_rebind || !plan.wanted.contains(*path))
                .take(64)
                .cloned()
                .collect::<Vec<_>>();
            for path in &removals {
                let _ = watcher.unwatch(path);
                self.watched.remove(path);
            }
            if !removals.is_empty() {
                return Ok(Some(plan));
            }
            self.force_rebind = false;
            use std::ops::Bound::{Excluded, Unbounded};
            let paths = plan
                .wanted
                .range::<PathBuf, _>((
                    plan.published_after.as_ref().map_or(Unbounded, Excluded),
                    Unbounded,
                ))
                .take(64)
                .cloned()
                .collect::<Vec<_>>();
            let now = Instant::now();
            for path in &paths {
                if self.watched.contains(path) {
                    continue;
                }
                if watcher.watch(path, RecursiveMode::NonRecursive).is_ok() {
                    self.watched.insert(path.clone());
                    let normalized = comparison_path(path);
                    let projects = normalized
                        .ancestors()
                        .take(128)
                        .filter_map(|ancestor| self.root_index.get(ancestor))
                        .flatten()
                        .copied()
                        .collect::<BTreeSet<_>>();
                    for project in projects {
                        Self::dirty(self.entries.get_mut(&project).unwrap(), now);
                    }
                } else {
                    for (project, entry) in &self.entries {
                        for (index, root) in entry.binding.roots().enumerate() {
                            if comparison_path(path).starts_with(comparison_path(Path::new(root))) {
                                plan.failures.get_mut(project).unwrap()[index] =
                                    Some(WatchFailure::BackendUnavailable);
                            }
                        }
                    }
                }
            }
            plan.published_after = paths.last().cloned().or(plan.published_after);
            if paths.len() == 64 {
                return Ok(Some(plan));
            }
        }
        self.force_rebind = false;
        for (project, entry) in &self.entries {
            let failures = &plan.failures[project];
            let native = if entry.binding.configuration_observed {
                failures.iter().filter(|failure| failure.is_none()).count() as u16
            } else {
                0
            };
            let complete = usize::from(native) == failures.len();
            self.observations.insert(
                *project,
                Observation {
                    project: *project,
                    epoch: entry.binding.epoch,
                    native_roots: native,
                    backend: if complete {
                        WatchBackend::Native
                    } else {
                        WatchBackend::PeriodicFallback
                    },
                    failure: failures.iter().flatten().copied().next().or_else(|| {
                        (!entry.binding.configuration_observed)
                            .then_some(WatchFailure::MetadataUnavailable)
                    }),
                    traversal: Some({
                        let mut traversal =
                            plan.usage.remove(project).unwrap_or_default().traversal;
                        traversal.incomplete_roots = failures
                            .iter()
                            .enumerate()
                            .filter_map(|(index, failure)| {
                                failure.is_some().then_some(index as u16)
                            })
                            .collect();
                        traversal
                    }),
                },
            );
        }
        Ok(None)
    }
    pub fn observations(&mut self, limit: usize) -> Vec<Observation> {
        let ids = self
            .observations
            .keys()
            .take(limit.min(32))
            .copied()
            .collect::<Vec<_>>();
        ids.into_iter()
            .filter_map(|id| self.observations.remove(&id))
            .collect()
    }
    pub fn notices(&mut self, limit: usize) -> Result<Vec<Notice>> {
        let now = Instant::now();
        let mut output = Vec::new();
        let mut ids = self
            .entries
            .keys()
            .filter(|id| self.notice_after.is_none_or(|after| **id > after))
            .copied()
            .collect::<Vec<_>>();
        ids.extend(
            self.entries
                .keys()
                .filter(|id| self.notice_after.is_some_and(|after| **id <= after))
                .copied(),
        );
        for project in ids {
            let entry = self.entries.get_mut(&project).unwrap();
            if output.len() >= limit.min(32) {
                break;
            }
            if let Some((sequence, sent)) = entry.inflight.as_mut() {
                if now.duration_since(*sent) >= Duration::from_secs(1) {
                    *sent = now;
                    output.push(Notice {
                        project,
                        epoch: entry.binding.epoch,
                        sequence: *sequence,
                    });
                }
            } else if entry.dirty.as_ref().is_some_and(|dirty| {
                now.duration_since(dirty.last) >= Duration::from_secs(2)
                    || now.duration_since(dirty.first) >= Duration::from_secs(30)
            }) {
                entry.sequence = entry
                    .sequence
                    .checked_add(1)
                    .filter(|sequence| *sequence <= i64::MAX as u64)
                    .ok_or_else(|| anyhow::anyhow!("source watcher event sequence exhausted"))?;
                entry.dirty = None;
                entry.inflight = Some((entry.sequence, now));
                output.push(Notice {
                    project,
                    epoch: entry.binding.epoch,
                    sequence: entry.sequence,
                });
            }
        }
        self.notice_after = output
            .last()
            .map(|notice| notice.project)
            .or(self.notice_after);
        Ok(output)
    }
    pub fn pending_count(&self) -> u16 {
        self.entries
            .values()
            .filter(|entry| entry.dirty.is_some() || entry.inflight.is_some())
            .count() as u16
    }
    pub fn counts(&self) -> (u16, u16) {
        (self.watched.len() as u16, self.entries.len() as u16)
    }
}

impl Plan {
    fn unreadable(&mut self, path: PathBuf, reason: &'static str) {
        let walk = self.walk.as_ref().unwrap();
        let traversal = &mut self.usage.entry(walk.task.project).or_default().traversal;
        let root = walk.task.index as u16;
        if !traversal.incomplete_roots.contains(&root) {
            traversal.incomplete_roots.push(root);
        }
        traversal.push(UnreadableDirectory {
            root,
            path: path.to_string_lossy().into_owned(),
            reason: reason.into(),
        });
        self.fail(WatchFailure::RootUnavailable);
    }
    fn fail(&mut self, failure: WatchFailure) {
        if let Some(walk) = self.walk.as_ref() {
            self.failures.get_mut(&walk.task.project).unwrap()[walk.task.index] = Some(failure);
        }
        self.walk = None;
    }
    fn step(&mut self) -> Result<bool> {
        let until = Instant::now() + Duration::from_millis(50);
        for _ in 0..512 {
            if Instant::now() >= until {
                break;
            }
            if self.walk.is_none() {
                let Some(task) = self.tasks.pop_front() else {
                    return Ok(true);
                };
                let root = task.root.clone();
                let bytes = root.as_os_str().len();
                self.walk = Some(Walk {
                    task,
                    pending: vec![(root, 0)],
                    queued_bytes: bytes,
                    cursor: None,
                });
            }
            let project = self.walk.as_ref().unwrap().task.project;
            if self.usage.entry(project).or_default().entries >= MAX_SCAN_ENTRIES {
                self.fail(WatchFailure::ScanEntryLimit);
                continue;
            }
            let walk = self.walk.as_mut().unwrap();
            if let Some((directory, depth, reader)) = walk.cursor.as_mut() {
                match reader.next() {
                    None => {
                        walk.cursor = None;
                    }
                    Some(Err(error)) => {
                        let path = directory.clone();
                        self.unreadable(path, directory_error(&error));
                    }
                    Some(Ok(entry)) => {
                        self.usage.entry(project).or_default().entries += 1;
                        let path = entry.path();
                        if !relevant(&path, &walk.task.root, &walk.task.data) {
                            continue;
                        }
                        match entry.file_type() {
                            Ok(kind) if kind.is_dir() && !kind.is_symlink() => {
                                let bytes = path.as_os_str().len();
                                let usage = self.usage.entry(project).or_default();
                                let failure = if *depth >= MAX_DEPTH {
                                    Some(WatchFailure::DepthLimit)
                                } else if bytes > 4096 {
                                    Some(WatchFailure::PathLengthLimit)
                                } else if usage.directories + walk.pending.len()
                                    >= MAX_PROJECT_DIRECTORIES
                                {
                                    Some(WatchFailure::ProjectDirectoryLimit)
                                } else if usage.path_bytes + walk.queued_bytes + bytes
                                    > MAX_PROJECT_PATH_BYTES
                                {
                                    Some(WatchFailure::ProjectPathBytesLimit)
                                } else if self.path_bytes + walk.queued_bytes + bytes
                                    > MAX_PATH_BYTES
                                {
                                    Some(WatchFailure::GlobalPathBytesLimit)
                                } else {
                                    None
                                };
                                if let Some(failure) = failure {
                                    self.fail(failure);
                                } else {
                                    walk.pending.push((path, *depth + 1));
                                    walk.queued_bytes += bytes;
                                }
                            }
                            Err(error) => {
                                let path = directory.clone();
                                self.unreadable(path, directory_error(&error));
                            }
                            _ => {}
                        }
                    }
                }
            } else if let Some((path, depth)) = walk.pending.pop() {
                walk.queued_bytes = walk.queued_bytes.saturating_sub(path.as_os_str().len());
                let bytes = path.as_os_str().len();
                let usage = self.usage.entry(project).or_default();
                let failure = if usage.directories >= MAX_PROJECT_DIRECTORIES {
                    Some(WatchFailure::ProjectDirectoryLimit)
                } else if usage.path_bytes + bytes > MAX_PROJECT_PATH_BYTES {
                    Some(WatchFailure::ProjectPathBytesLimit)
                } else if !self.wanted.contains(&path) && self.wanted.len() >= MAX_DIRECTORIES {
                    Some(WatchFailure::GlobalDirectoryLimit)
                } else if !self.wanted.contains(&path) && self.path_bytes + bytes > MAX_PATH_BYTES {
                    Some(WatchFailure::GlobalPathBytesLimit)
                } else {
                    None
                };
                if let Some(failure) = failure {
                    self.fail(failure);
                    continue;
                }
                let safe = (|| -> Result<()> {
                    let metadata = fs::symlink_metadata(&path)?;
                    ensure!(
                        metadata.is_dir() && !metadata.file_type().is_symlink(),
                        "redirected watch directory"
                    );
                    #[cfg(windows)]
                    {
                        use std::os::windows::fs::MetadataExt;
                        ensure!(
                            metadata.file_attributes() & 0x400 == 0,
                            "redirected watch directory"
                        );
                    }
                    ensure!(
                        comparison_path(&path.canonicalize()?) == comparison_path(&path),
                        "aliased watch directory"
                    );
                    Ok(())
                })();
                if let Err(error) = safe {
                    let reason = error
                        .downcast_ref::<std::io::Error>()
                        .map(directory_error)
                        .unwrap_or("redirected");
                    self.unreadable(path, reason);
                    continue;
                }
                match fs::read_dir(&path) {
                    Ok(reader) => {
                        let usage = self.usage.entry(project).or_default();
                        usage.directories += 1;
                        usage.path_bytes += bytes;
                        if self.wanted.insert(path.clone()) {
                            self.path_bytes += path.as_os_str().len();
                        }
                        walk.cursor = Some((path, depth, reader));
                    }
                    Err(error) => self.unreadable(path, directory_error(&error)),
                }
            } else {
                self.walk = None;
            }
        }
        Ok(false)
    }
}

fn directory_error(error: &std::io::Error) -> &'static str {
    match error.kind() {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        _ => "io_error",
    }
}

#[cfg(test)]
#[path = "source_watch_tests.rs"]
mod tests;
