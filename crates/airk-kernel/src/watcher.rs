// SPDX-License-Identifier: Apache-2.0
//! Native events are hints. Durable intents and periodic inventory remain authoritative.
use crate::Kernel;
use airk_store::scheduler::Queue;
use anyhow::{Context, Result, ensure};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const EVENT_CAPACITY: usize = 256;
const EVENT_PATH_BYTES: usize = 32768;
const REFRESH_INTERVAL: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_secs(2);
const MAX_DELAY: Duration = Duration::from_secs(30);

#[derive(Debug, Serialize)]
pub struct WatchReport {
    pub backend: &'static str,
    pub watched_roots: usize,
    pub pending_collections: usize,
    pub queued_collections: Vec<String>,
    pub recovering_dropped_events: bool,
    pub errors: Vec<String>,
}

#[derive(Clone, PartialEq, Eq)]
struct Binding {
    owner: String,
    roots: Vec<PathBuf>,
}
struct Dirty {
    first: Instant,
    last: Instant,
}

pub struct NativeWatcher {
    kernel: Kernel,
    managed_path: PathBuf,
    watcher: Option<RecommendedWatcher>,
    receiver: Receiver<Vec<PathBuf>>,
    sender: SyncSender<Vec<PathBuf>>,
    overflow: Arc<AtomicBool>,
    watched: BTreeSet<PathBuf>,
    bindings: BTreeMap<String, Binding>,
    dirty: BTreeMap<String, Dirty>,
    refreshed: Option<Instant>,
    backend_error: Option<String>,
}

fn deliver(
    sender: &SyncSender<Vec<PathBuf>>,
    overflow: &AtomicBool,
    result: notify::Result<notify::Event>,
) {
    let Ok(event) = result else {
        overflow.store(true, Ordering::Release);
        return;
    };
    if event.need_rescan() {
        overflow.store(true, Ordering::Release);
    }
    if matches!(event.kind, EventKind::Access(_)) {
        return;
    }
    if event.paths.is_empty()
        || event.paths.len() > 8
        || event
            .paths
            .iter()
            .map(|path| path.as_os_str().len())
            .sum::<usize>()
            > EVENT_PATH_BYTES
    {
        overflow.store(true, Ordering::Release);
        return;
    }
    if sender.try_send(event.paths).is_err() {
        overflow.store(true, Ordering::Release);
    }
}

impl NativeWatcher {
    pub fn new(kernel: &Kernel) -> Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(EVENT_CAPACITY);
        let mut value = Self {
            kernel: kernel.clone(),
            managed_path: canonical_managed_path(kernel.data_dir())?,
            watcher: None,
            receiver,
            sender,
            overflow: Arc::new(AtomicBool::new(false)),
            watched: BTreeSet::new(),
            bindings: BTreeMap::new(),
            dirty: BTreeMap::new(),
            refreshed: None,
            backend_error: None,
        };
        value.connect();
        Ok(value)
    }

    fn connect(&mut self) {
        let sender = self.sender.clone();
        let overflow = self.overflow.clone();
        let data = comparison_path(&self.managed_path);
        match RecommendedWatcher::new(
            move |mut event: notify::Result<notify::Event>| {
                if let Ok(event) = &mut event
                    && !event.need_rescan()
                    && !event.paths.is_empty()
                {
                    event
                        .paths
                        .retain(|path| !comparison_path(path).starts_with(&data));
                    if event.paths.is_empty() {
                        return;
                    }
                }
                deliver(&sender, &overflow, event);
            },
            notify::Config::default().with_follow_symlinks(false),
        ) {
            Ok(watcher) => {
                self.watcher = Some(watcher);
                self.backend_error = None;
            }
            Err(error) => {
                self.backend_error = Some(format!(
                    "native watcher unavailable; periodic reconciliation remains active: {error}"
                ));
            }
        }
    }

    fn dirty(&mut self, collection: &str, at: Instant) {
        self.dirty
            .entry(collection.into())
            .and_modify(|dirty| dirty.last = at)
            .or_insert(Dirty {
                first: at,
                last: at,
            });
    }

    /// Call before each worker tick. No source bytes are read or written here.
    /// Failed queue writes retain the in-memory hint; process loss is covered by
    /// the durable queue plus the existing five-minute reconciliation inventory.
    pub fn poll(&mut self) -> Result<WatchReport> {
        let at = Instant::now();
        let queue = Queue::new(self.kernel.data_dir());
        let jobs = queue.list()?;
        let mut errors = Vec::new();
        if self
            .refreshed
            .is_none_or(|last| at.duration_since(last) >= REFRESH_INTERVAL)
        {
            if self.watcher.is_none() {
                self.connect();
            }
            let mut bindings = BTreeMap::new();
            for job in &jobs {
                let binding = (|| -> Result<Binding> {
                    let kernel = Kernel::open_collection(
                        &job.root,
                        Some(self.kernel.data_dir().into()),
                        &job.collection,
                    )?;
                    ensure!(
                        kernel.root_id() == job.root,
                        "registered workspace root identity changed"
                    );
                    let mut roots = vec![kernel.root().to_path_buf()];
                    roots.extend(kernel.configured_roots()?.values().map(PathBuf::from));
                    Ok(Binding {
                        owner: job.root.clone(),
                        roots,
                    })
                })();
                match binding {
                    Ok(binding) => {
                        if self
                            .bindings
                            .get(&job.collection)
                            .is_some_and(|old| old != &binding)
                        {
                            self.dirty(&job.collection, at);
                        }
                        bindings.insert(job.collection.clone(), binding);
                    }
                    Err(error) => push_error(&mut errors, format!("{}: {error:#}", job.collection)),
                }
            }
            self.bindings = bindings;
            self.dirty
                .retain(|collection, _| self.bindings.contains_key(collection));
            let desired: BTreeSet<_> = self
                .bindings
                .values()
                .flat_map(|binding| binding.roots.iter().cloned())
                .collect();
            if let Some(watcher) = &mut self.watcher {
                for path in self
                    .watched
                    .difference(&desired)
                    .cloned()
                    .collect::<Vec<_>>()
                {
                    if let Err(error) = watcher.unwatch(&path) {
                        push_error(&mut errors, format!("unwatch {}: {error}", path.display()));
                    }
                    self.watched.remove(&path);
                }
                for path in desired
                    .difference(&self.watched)
                    .cloned()
                    .collect::<Vec<_>>()
                {
                    match watcher.watch(&path, RecursiveMode::Recursive) {
                        Ok(()) => {
                            self.watched.insert(path);
                        }
                        Err(error) => push_error(
                            &mut errors,
                            format!(
                                "watch {} failed; periodic reconciliation remains active: {error}",
                                path.display()
                            ),
                        ),
                    }
                }
            }
            self.refreshed = Some(at);
        }
        if let Some(error) = &self.backend_error {
            push_error(&mut errors, error.clone());
        }
        // A callback never blocks the OS thread and never writes to SQLite.
        for _ in 0..EVENT_CAPACITY {
            let Ok(paths) = self.receiver.try_recv() else {
                break;
            };
            let affected: Vec<_> = self
                .bindings
                .iter()
                .filter(|(_, binding)| {
                    paths.iter().any(|path| {
                        binding
                            .roots
                            .iter()
                            .any(|root| relevant(path, root, &self.managed_path))
                    })
                })
                .map(|(collection, _)| collection.clone())
                .collect();
            for collection in affected {
                self.dirty(&collection, at);
            }
        }
        let recovering = self.overflow.swap(false, Ordering::AcqRel);
        if recovering {
            for collection in self.bindings.keys().cloned().collect::<Vec<_>>() {
                self.dirty(&collection, at);
            }
        }
        let mut queued = Vec::new();
        for job in jobs {
            let Some(dirty) = self.dirty.get(&job.collection) else {
                continue;
            };
            if job.paused || job.blocked {
                continue;
            }
            if at.duration_since(dirty.last) < QUIET && at.duration_since(dirty.first) < MAX_DELAY {
                continue;
            }
            let result = (|| -> Result<()> {
                let kernel = Kernel::open_collection(
                    &job.root,
                    Some(self.kernel.data_dir().into()),
                    &job.collection,
                )?;
                ensure!(
                    kernel.root_id() == job.root,
                    "registered workspace root identity changed"
                );
                kernel.configured_roots()?; // Alias targets must retain their canonical binding.
                queue.enqueue(
                    &job.collection,
                    &job.root,
                    false,
                    false,
                    true,
                    u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?,
                )?;
                Ok(())
            })();
            match result {
                Ok(()) => {
                    self.dirty.remove(&job.collection);
                    queued.push(job.collection);
                }
                Err(error) => push_error(&mut errors, format!("{}: {error:#}", job.collection)),
            }
        }
        Ok(WatchReport {
            backend: if self.watcher.is_some() {
                "native"
            } else {
                "periodic_fallback"
            },
            watched_roots: self.watched.len(),
            pending_collections: self.dirty.len(),
            queued_collections: queued,
            recovering_dropped_events: recovering,
            errors,
        })
    }
}

fn push_error(errors: &mut Vec<String>, error: String) {
    if errors.len() < 16 {
        errors.push(error.chars().take(1000).collect());
    }
}

/// Notifications use absolute canonical watched paths, while a supplied data
/// directory may be relative or use an alias such as macOS /var -> /private/var.
/// Resolve only the existing ancestor: starting a watcher must not create state.
fn canonical_managed_path(path: &Path) -> Result<PathBuf> {
    let mut ancestor = std::path::absolute(path)?;
    let mut missing = Vec::new();
    loop {
        match std::fs::metadata(&ancestor) {
            Ok(metadata) => {
                ensure!(metadata.is_dir(), "managed watcher path is not a directory");
                let mut resolved = ancestor.canonicalize()?;
                for component in missing.into_iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .context("managed watcher path has no existing ancestor")?
                        .to_os_string(),
                );
                ensure!(
                    ancestor.pop(),
                    "managed watcher path has no existing ancestor"
                );
            }
            Err(error) => return Err(error).context("resolve managed watcher path"),
        }
    }
}

fn comparison_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(
            path.to_string_lossy()
                .trim_start_matches(r"\\?\")
                .to_lowercase(),
        )
    }
    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

fn relevant(path: &Path, root: &Path, data: &Path) -> bool {
    let path = comparison_path(path);
    let root = comparison_path(root);
    if path.starts_with(comparison_path(data)) {
        return false;
    }
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    !relative.components().any(|component| {
        let name = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        matches!(
            name.as_str(),
            ".git"
                | ".airk"
                | "node_modules"
                | "target"
                | ".venv"
                | "__pycache__"
                | ".aws"
                | ".ssh"
                | ".env"
        ) || name.starts_with(".env.")
            || name.ends_with(".pem")
            || name.ends_with(".key")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_relative_managed_path_resolves_without_creating_directories() -> Result<()> {
        let unique = tempfile::tempdir()?;
        let relative = PathBuf::from(unique.path().file_name().context("unique fixture name")?)
            .join("missing/managed");
        ensure!(!relative.exists(), "unexpected existing fixture path");
        let expected = std::env::current_dir()?.canonicalize()?.join(&relative);
        assert_eq!(canonical_managed_path(&relative)?, expected);
        assert!(!relative.ancestors().nth(2).unwrap().exists());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn aliased_managed_directory_events_are_excluded_at_callback_admission() -> Result<()> {
        use std::os::unix::fs::symlink;
        let fixture = tempfile::tempdir()?;
        let source = fixture.path().join("source");
        std::fs::create_dir(&source)?;
        std::fs::write(source.join("source.txt"), "unchanged source")?;
        let alias = fixture.path().join("source-alias");
        symlink(&source, &alias)?;
        let managed = alias.join("custom-managed-state/missing");
        let kernel = Kernel::open(&source, Some(managed.clone()))?;
        let mut watcher = NativeWatcher::new(&kernel)?;
        assert!(!managed.parent().unwrap().exists());
        assert_eq!(
            watcher.managed_path,
            source.canonicalize()?.join("custom-managed-state/missing")
        );
        kernel.scheduled_index(false, false)?;
        assert_eq!(watcher.poll()?.watched_roots, 1);
        for ordinal in 0..400 {
            std::fs::write(managed.join(format!("derived-{ordinal}")), "derived state")?;
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
            let report = watcher.poll()?;
            assert!(!report.recovering_dropped_events);
            assert_eq!(report.pending_collections, 0);
            assert!(report.queued_collections.is_empty());
        }
        Ok(())
    }

    #[test]
    fn bounded_callback_marks_overflow_and_ignores_access() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let overflow = AtomicBool::new(false);
        let event = || notify::Event::new(EventKind::Any).add_path(PathBuf::from("fixture"));
        deliver(&sender, &overflow, Ok(event()));
        deliver(&sender, &overflow, Ok(event()));
        assert!(overflow.swap(false, Ordering::AcqRel));
        assert_eq!(receiver.try_iter().count(), 1);
        deliver(
            &sender,
            &overflow,
            Ok(notify::Event::new(EventKind::Access(
                notify::event::AccessKind::Any,
            ))),
        );
        assert!(!overflow.load(Ordering::Acquire));
        assert!(receiver.try_recv().is_err());
        deliver(
            &sender,
            &overflow,
            Ok(event().set_flag(notify::event::Flag::Rescan)),
        );
        assert!(overflow.load(Ordering::Acquire));
    }

    #[test]
    fn native_mutations_aliases_and_overflow_reach_durable_queue() -> Result<()> {
        let root = tempfile::tempdir()?;
        let alias = tempfile::tempdir()?;
        let state = tempfile::tempdir()?;
        std::fs::write(root.path().join("fixture.txt"), "first")?;
        let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
        kernel.add_root("other", alias.path())?;
        kernel.scheduled_index(false, false)?;
        let mut watcher = NativeWatcher::new(&kernel)?;
        assert_eq!(watcher.poll()?.watched_roots, 2);
        let before = kernel.schedule_status()?.unwrap().requested_revision;
        std::fs::write(
            alias.path().join("alias.txt"),
            "event from independent source",
        )?;
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            watcher.poll()?;
            if kernel.schedule_status()?.unwrap().requested_revision > before {
                break;
            }
            ensure!(Instant::now() < deadline, "native mutation was not queued");
            std::thread::sleep(Duration::from_millis(50));
        }
        kernel.schedule_pause(true)?;
        watcher.overflow.store(true, Ordering::Release);
        assert!(watcher.poll()?.recovering_dropped_events);
        for dirty in watcher.dirty.values_mut() {
            dirty.last -= QUIET;
        }
        assert!(watcher.poll()?.queued_collections.is_empty());
        kernel.schedule_pause(false)?;
        assert_eq!(watcher.poll()?.queued_collections, vec!["default"]);
        assert!(!relevant(
            &state.path().join("index.sqlite3"),
            root.path(),
            state.path()
        ));
        assert!(!relevant(
            &root.path().join("target/build"),
            root.path(),
            state.path()
        ));
        Ok(())
    }

    #[test]
    fn managed_state_changes_do_not_fill_the_source_event_channel() -> Result<()> {
        let root = tempfile::tempdir()?;
        let state = root.path().join("custom-managed-state");
        std::fs::write(root.path().join("source.txt"), "unchanged source")?;
        let kernel = Kernel::open(root.path(), Some(state.clone()))?;
        kernel.scheduled_index(false, false)?;
        let mut watcher = NativeWatcher::new(&kernel)?;
        watcher.poll()?;
        for ordinal in 0..400 {
            std::fs::write(
                state.join(format!("temporary-fixture-{ordinal}")),
                "derived state",
            )?;
        }
        std::thread::sleep(Duration::from_millis(250));
        let report = watcher.poll()?;
        assert!(!report.recovering_dropped_events);
        assert_eq!(report.pending_collections, 0);
        assert!(report.queued_collections.is_empty());
        Ok(())
    }
}
