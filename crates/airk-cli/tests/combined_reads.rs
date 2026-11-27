// SPDX-License-Identifier: Apache-2.0
//! Actual native readers and CLI/MCP transports over an isolated authority-1
//! component-format fixture. This is not Genesis or installer qualification.
mod common;
use common::{ctl::Ctl, http, read_retry};
#[path = "combined_reads/single_member_retry.rs"]
mod single_member_retry;

#[path = "combined_reads/permanent_trim.rs"]
mod permanent_trim;

#[path = "combined_reads/changes_verification.rs"]
mod changes_verification;

#[cfg(feature = "local-inference")]
#[path = "combined_reads/retention.rs"]
mod retention;
#[path = "combined_reads/split.rs"]
mod split;

use airk_core::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{IndexId, QueryProjectId, SourceRootId},
    service::{ControlMethod, ControlResponse, ProjectRegistration},
};
use airk_store::service::{ProjectLease, registry::ServiceRegistry};
use anyhow::{Context, Result, ensure};
use common::pipe;
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn binary() -> PathBuf {
    std::env::var_os("AIRK_CATALOG_CANDIDATE_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_airk").into())
}

fn fixture_candidate(base: &Path, inference: bool) -> Result<PathBuf> {
    let candidate = binary();
    // A supplied immutable image retains its existing build provenance.
    if std::env::var_os("AIRK_CATALOG_CANDIDATE_BINARY").is_some() || !inference {
        return Ok(candidate);
    }
    #[cfg(feature = "local-inference")]
    {
        // This disposable component fixture seals only the current main image.
        // It is not an installed-payload or deployment qualification fixture.
        let payload = base.join("runtime");
        let main = payload.join(airk_cli::install_files::main_name());
        fs::create_dir_all(main.parent().context("owned main parent missing")?)?;
        fs::copy(&candidate, &main)?;
        let receipt = airk_cli::runtime_pair::RuntimePair {
            schema: 2,
            version: env!("CARGO_PKG_VERSION").into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            airk_sha256: airk_cli::runtime_pair::digest_file(&main)?,
            embed_sha256: String::new(),
            split: None,
        };
        fs::write(
            payload.join("airk-runtime.json"),
            serde_json::to_vec(&receipt)?,
        )?;
        Ok(main)
    }
    #[cfg(not(feature = "local-inference"))]
    {
        let _ = base;
        anyhow::bail!("owned inference fixture requires local-inference feature")
    }
}

struct OwnedProcess {
    #[cfg(unix)]
    child: airk_process::GroupChild,
    #[cfg(not(unix))]
    child: std::process::Child,
    #[cfg(not(unix))]
    job: airk_process::ChildJob,
}
impl OwnedProcess {
    fn start_with_process_limit(mut command: Command, process_limit: u32) -> Result<Self> {
        #[cfg(unix)]
        let _ = process_limit;
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        #[cfg(unix)]
        {
            Ok(Self {
                child: airk_process::GroupChild::spawn(&mut command)?,
            })
        }
        #[cfg(not(unix))]
        {
            let mut child = command.spawn()?;
            let job = match airk_process::ChildJob::attach_with_process_limit(
                &mut child,
                process_limit,
            ) {
                Ok(job) => job,
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
            };
            Ok(Self { child, job })
        }
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        #[cfg(unix)]
        self.child.drain();
        #[cfg(not(unix))]
        {
            self.job.drain();
            let _ = self.child.wait();
        }
    }
}

/// The membership atoms a fixture session reads: its Projects and, when
/// named, an explicit subset of their Indexes (else every Index those
/// Projects hold). Reads under it go through a window selection (W1/W4) over
/// exactly these atoms, granted at the service's current catalog revision.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SessionScope {
    project_ids: Vec<QueryProjectId>,
    index_ids: Option<Vec<IndexId>>,
}

/// A catalog session always read through its combined owner: its selectors
/// were the session's defaults. A window read of one Index that names no
/// selector is the native singleton, so a session whose window selects one
/// Index names its own Projects (and its explicit Index subset, when it has
/// one) on every read that names neither (the window's explicit-selector
/// combined path).
fn session_defaults(
    scope: &SessionScope,
    window: &pipe::Window,
) -> Option<serde_json::Map<String, Value>> {
    if window.selection.index_ids.len() != 1 {
        return None;
    }
    let mut defaults = serde_json::Map::new();
    defaults.insert("project_ids".into(), json!(scope.project_ids));
    if let Some(indexes) = &scope.index_ids {
        defaults.insert("index_ids".into(), json!(indexes));
    }
    Some(defaults)
}

fn with_session_defaults(
    arguments: &mut serde_json::Map<String, Value>,
    defaults: &serde_json::Map<String, Value>,
) {
    if !arguments.contains_key("project_ids") && !arguments.contains_key("index_ids") {
        for (key, value) in defaults {
            arguments.insert(key.clone(), value.clone());
        }
    }
}

/// One read under an earlier session's window after a restrictive catalog
/// change: the refused envelope, or `None` when the service refused the
/// window read before any result (`scoped_read_unavailable`). Any other
/// transport failure is an error.
async fn old_authority_reply(
    backend: &dyn airk_mcp::remote::RemoteTools,
    tool: &str,
    arguments: &Value,
) -> Result<Option<Value>> {
    match backend
        .call(
            tool.into(),
            arguments
                .as_object()
                .context("object arguments required")?
                .clone(),
        )
        .await
    {
        Ok(reply) => Ok(Some(reply)),
        Err(error) => {
            let text = format!("{error:#}");
            ensure!(
                text.contains(r#""status":"failure","code":"scoped_read_unavailable""#),
                "old window read failed unexpectedly instead of a scoped refusal: {text}"
            );
            Ok(None)
        }
    }
}

/// A copy without the selection-dependent scope observation fields
/// (selection authority spec §2.1, §3.6): each window grant reports its own
/// selection source and scope fingerprint, so a member read through a
/// one-Index window and the same member inside a wider window's combined
/// read differ only there. Every other field stays literal.
fn without_window_scope(mut value: Value) -> Value {
    fn walk(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                if let Some(observation) = fields
                    .get_mut("scope_observation")
                    .and_then(Value::as_object_mut)
                {
                    for key in ["fingerprint", "selection"] {
                        observation.remove(key);
                    }
                }
                fields.values_mut().for_each(walk);
            }
            Value::Array(values) => values.iter_mut().for_each(walk),
            _ => {}
        }
    }
    walk(&mut value);
    value
}

/// Pipe reads of one fixture session (see `session_defaults`).
struct SessionReads {
    inner: std::sync::Arc<pipe::PipeTools>,
    defaults: Option<serde_json::Map<String, Value>>,
}

impl SessionReads {
    fn arguments(
        &self,
        mut arguments: serde_json::Map<String, Value>,
    ) -> serde_json::Map<String, Value> {
        if let Some(defaults) = &self.defaults {
            with_session_defaults(&mut arguments, defaults);
        }
        arguments
    }
}

impl airk_mcp::remote::RemoteTools for SessionReads {
    fn call(
        &self,
        tool: String,
        arguments: serde_json::Map<String, Value>,
    ) -> airk_mcp::remote::RemoteFuture<'_> {
        self.inner.call(tool, self.arguments(arguments))
    }
}

struct Fixture {
    owner: tempfile::TempDir,
    candidate: PathBuf,
    control: PathBuf,
    session: SessionScope,
    /// The CLI's session-routed window, reused while it lives and its scope
    /// is unchanged (see `Fixture::session_window`).
    cli_window: std::sync::Mutex<Option<(SessionScope, pipe::Window)>>,
    registrations: Vec<ProjectRegistration>,
    indexes: Vec<IndexId>,
    projects: Vec<QueryProjectId>,
}
/// Isolated fresh installation: the empty-registry Genesis has no predecessor.
fn settle_genesis(control: &Path, installer: Sha256Digest) -> Result<()> {
    use airk_store::service::{
        ServiceLease,
        registry::catalog::genesis::{
            CatalogGenesis,
            activation::{
                AuthorityV1CandidateObservation, AuthorityV1NativeQualification,
                AuthorityV1ReadinessProducer,
            },
        },
    };
    struct FreshInstallation;
    impl AuthorityV1ReadinessProducer for FreshInstallation {
        fn required_predecessor_identities(&self) -> Result<Vec<Sha256Digest>> {
            Ok(Vec::new())
        }
        fn qualify(
            &self,
            expected: &AuthorityV1CandidateObservation,
        ) -> Result<AuthorityV1NativeQualification> {
            Ok(AuthorityV1NativeQualification {
                candidate: expected.clone(),
                predecessor_refusals: Vec::new(),
            })
        }
    }
    let mut genesis = CatalogGenesis::begin(
        ServiceLease::acquire(control)?,
        installer,
        Incarnation::from_bytes([111; 16]),
    )?;
    genesis.backup()?;
    genesis.migrate()?;
    genesis.promote_authority_v1()?;
    genesis
        .record_authority_v1_readiness(Incarnation::from_bytes([112; 16]), &FreshInstallation)?;
    genesis.settle_authority_v1()?;
    Ok(())
}

impl Fixture {
    fn new(dedicated_is_indexed: bool) -> Result<Self> {
        Self::with_indexes(dedicated_is_indexed, 3)
    }

    fn with_indexes(dedicated_is_indexed: bool, count: usize) -> Result<Self> {
        Self::with_runtime(dedicated_is_indexed, count, false)
    }

    #[cfg(feature = "local-inference")]
    fn with_inference() -> Result<Self> {
        Self::with_runtime(true, 3, true)
    }

    fn with_runtime(dedicated_is_indexed: bool, count: usize, helper: bool) -> Result<Self> {
        Self::with_runtime_layout(dedicated_is_indexed, count, helper, None)
    }
    fn with_runtime_layout(
        dedicated_is_indexed: bool,
        count: usize,
        helper: bool,
        removed_last_parent: Option<bool>,
    ) -> Result<Self> {
        Self::with_runtime_topology(
            dedicated_is_indexed,
            count,
            helper,
            removed_last_parent,
            false,
            false,
        )
    }
    fn with_shared_runtime() -> Result<Self> {
        Self::with_runtime_topology(true, 3, false, None, true, false)
    }
    fn with_runtime_topology(
        dedicated_is_indexed: bool,
        count: usize,
        helper: bool,
        removed_last_parent: Option<bool>,
        shared: bool,
        same_source_aliases: bool,
    ) -> Result<Self> {
        ensure!(
            (3..=32).contains(&count),
            "fixture index count outside bound"
        );
        let owner = match std::env::var_os("AIRK_PRIVATE_CENSUS_BASE") {
            Some(base) => tempfile::Builder::new()
                .prefix("airk-combined-")
                .tempdir_in(base)?,
            None => tempfile::tempdir()?,
        };
        let base = owner.path().canonicalize()?;
        let candidate = fixture_candidate(&base, helper)?;
        let control = base.join("control");
        airk_cli::local_ipc::prepare_root(&control)?;
        let namespace = Incarnation::from_bytes([110; 16]);
        drop(ServiceRegistry::open(&control, namespace)?);
        // Empty authority-1 catalog through the real owned Genesis adapter.
        // Projects, indexes and memberships below are physical-format fixtures.
        settle_genesis(
            &control,
            airk_cli::runtime_pair::engine_identity_at(&candidate)?,
        )?;

        let projects = (0..3)
            .map(|n| QueryProjectId::from_identity(Incarnation::from_bytes([20 + n; 16])))
            .collect::<Vec<_>>();
        let indexes = (0..count)
            .map(|n| IndexId::from_identity(Incarnation::from_bytes([40 + n as u8; 16])))
            .collect::<Vec<_>>();
        let sources = (0..count)
            .map(|n| SourceRootId::from_identity(Incarnation::from_bytes([50 + n as u8; 16])))
            .collect::<Vec<_>>();
        let mut registrations = Vec::new();
        for n in 0..count {
            let nested = n + 1 == count && removed_last_parent.is_some();
            let parent = if nested {
                let parent = base.join("removed-parent");
                fs::create_dir(&parent)?;
                parent
            } else {
                base.clone()
            };
            let source = parent.join(format!("source{n}"));
            let data = if shared {
                base.join("shared-data")
            } else if nested && removed_last_parent == Some(true) {
                parent.join(format!("data{n}"))
            } else {
                base.join(format!("data{n}"))
            };
            fs::create_dir(&source)?;
            if shared {
                fs::create_dir_all(&data)?;
            } else {
                fs::create_dir(&data)?;
            }
            for directory in ["api", "impl", "private", "api-other"] {
                fs::create_dir(source.join(directory))?;
                fs::write(
                    source.join(directory).join("policy.md"),
                    format!("# boundaryneedle {n} {directory}\nOriginal isolated source.\n"),
                )?;
            }
            airk_store::service::data_scope::bind(&data, &control, namespace)?;
            let collection = format!("combined{n}");
            let kernel = airk_kernel::Kernel::open_service_collection(
                &source,
                Some(data.clone()),
                &collection,
            )?;
            kernel.configure_vectors(false)?;
            if n != 1 || dedicated_is_indexed {
                kernel.index()?;
            }
            registrations.push(airk_kernel::service_projects::registration(
                &source,
                &data,
                &collection,
            )?);
        }
        let sql = Connection::open(control.join("service-1.4.sqlite3"))?;
        let tx = sql.unchecked_transaction()?;
        for (n, registration) in registrations.iter().enumerate() {
            tx.execute(
                "INSERT INTO projects(binding,payload,epoch,paused,registered) VALUES(?1,?2,1,1,1)",
                params![
                    registration.binding.to_string(),
                    serde_json::to_vec(registration)?
                ],
            )?;
            tx.execute("INSERT INTO index_definitions VALUES(?1,?2,?2,?3,?4,'collections-1.4',?5,?5,1,1,0,NULL,0,'complete')",params![indexes[n].to_string(),registration.binding.to_string(),registration.root,registration.data_root,registration.collection])?;
            tx.execute(
                "INSERT INTO index_sources VALUES(?1,?2,?3,1,1)",
                params![
                    sources[n].to_string(),
                    indexes[n].to_string(),
                    registration.root
                ],
            )?;
        }
        for (n, project) in projects.iter().enumerate() {
            tx.execute(
                "INSERT INTO query_projects VALUES(?1,?2,1,1,1)",
                params![project.to_string(), format!("Fictional Project {n}")],
            )?;
        }
        let mut memberships = vec![(0, 0, "api"), (1, 0, "impl"), (0, 1, "api"), (2, 2, "")];
        memberships.extend((3..count).map(|index| (0, index, "api")));
        for (project, index, prefix) in memberships {
            let prefix = if same_source_aliases && project == 1 && index == 0 {
                "api"
            } else {
                prefix
            };
            tx.execute(
                "INSERT INTO project_index_memberships VALUES(?1,?2,1)",
                params![projects[project].to_string(), indexes[index].to_string()],
            )?;
            tx.execute(
                "INSERT INTO project_index_scopes VALUES(?1,?2,?3,'subtree',?4)",
                params![
                    projects[project].to_string(),
                    indexes[index].to_string(),
                    sources[index].to_string(),
                    prefix
                ],
            )?;
        }
        tx.commit()?;
        drop(sql);
        let registry = ServiceRegistry::open_saved(&control)?;
        registry.catalog_state()?.context("catalog missing")?;
        let session = SessionScope {
            project_ids: projects[..2].to_vec(),
            index_ids: None,
        };
        Ok(Self {
            owner,
            candidate,
            control,
            session,
            cli_window: std::sync::Mutex::new(None),
            registrations,
            indexes,
            projects,
        })
    }

    async fn start(&self) -> Result<OwnedProcess> {
        self.start_with_env(&[]).await
    }

    /// `start` with extra environment for the service process only (the
    /// test-support Answer job gate).
    async fn start_with_env(&self, env: &[(&str, String)]) -> Result<OwnedProcess> {
        let log = self.owner.path().join("service.log");
        let mut command = Command::new(&self.candidate);
        for (key, value) in env {
            command.env(key, value);
        }
        // The private foreground service role (split Phase 4; was `airk serve`).
        command
            .args(["__airk_role", "service-foreground", "--runtime-dir"])
            .arg(&self.control)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(fs::File::create(&log)?));
        // This enclosing fixture job contains the service, source watcher,
        // in-process inference provider and all 32 admitted resident workers. Each native
        // owner retains its own unchanged admission and descendant limits.
        let mut service = OwnedProcess::start_with_process_limit(command, 32 + 3)?;
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let response =
                airk_cli::service_runtime::request(&self.control, ControlMethod::RequestEpoch)
                    .await;
            if matches!(response, Ok(ControlResponse::RequestEpoch { .. })) {
                if self.registrations.len() == 3 {
                    // Cover the startup inference reservation, query working
                    // space, and three resident readers through the native contract.
                    const THREE_READER_MEMORY_BYTES: u64 = 10 << 30;
                    let response = airk_cli::service_runtime::request(
                        &self.control,
                        ControlMethod::ConfigureResources {
                            configuration: airk_core::service_resources::ResourceConfiguration {
                                memory_bytes: Some(THREE_READER_MEMORY_BYTES),
                                ..Default::default()
                            },
                        },
                    )
                    .await?;
                    let ControlResponse::Resources {
                        observation,
                        configuration_error,
                        ..
                    } = response
                    else {
                        anyhow::bail!(
                            "fixture resource configuration refused: {}",
                            serde_json::to_string(&response)?
                        );
                    };
                    ensure!(
                        !configuration_error
                            && observation.configuration.memory_bytes == Some(THREE_READER_MEMORY_BYTES)
                            && observation.resident_limit >= 3,
                        "fixture lacks explicit three-reader capacity: {}",
                        serde_json::to_string(&observation)?
                    );
                    fs::write(
                        self.owner.path().join("resources.json"),
                        serde_json::to_vec(&observation)?,
                    )?;
                }
                return Ok(service);
            }
            ensure!(
                service.child.try_wait()?.is_none(),
                "service exited: {}",
                fs::read_to_string(&log)?
            );
            ensure!(
                Instant::now() < deadline,
                "service startup failed: {}; {}",
                match &response {
                    Ok(response) => serde_json::to_string(response)?,
                    Err(error) => format!("{error:#}"),
                },
                fs::read_to_string(&log)?
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// The Indexes a window over `scope` selects: its explicit subset, else
    /// every Index its Projects hold in the current catalog.
    async fn scope_indexes(&self, scope: &SessionScope) -> Result<Vec<IndexId>> {
        match &scope.index_ids {
            Some(ids) => Ok(ids.clone()),
            None => self.member_indexes(&scope.project_ids).await,
        }
    }

    /// A fresh window over `scope` (W1/W4 at the current catalog revision).
    async fn window_for(&self, scope: &SessionScope) -> Result<pipe::Window> {
        let index_ids = self.scope_indexes(scope).await?;
        pipe::open_window(
            &self.control,
            "combined fixture session",
            index_ids,
            scope.project_ids.clone(),
        )
        .await
    }

    /// The Indexes `projects` hold in the service's current catalog, in
    /// fixture order, read through the canonical catalog pages.
    async fn member_indexes(&self, projects: &[QueryProjectId]) -> Result<Vec<IndexId>> {
        use airk_core::catalog_management::{CatalogItem, CatalogKind, CatalogPageRequest};
        let mut held = std::collections::BTreeSet::new();
        for project in projects {
            let mut after = None;
            loop {
                let request = CatalogPageRequest {
                    kind: CatalogKind::Memberships,
                    index_id: None,
                    project_id: Some(*project),
                    after: after.take(),
                    expected_catalog_revision: None,
                    limit: 32,
                    max_bytes: 48 * 1024,
                };
                request.validate()?;
                let response = airk_cli::service_runtime::request(
                    &self.control,
                    ControlMethod::CatalogPage { request },
                )
                .await?;
                let ControlResponse::CatalogPage { page, .. } = response else {
                    anyhow::bail!(
                        "membership page refused: {}",
                        serde_json::to_string(&response)?
                    );
                };
                for item in page.items {
                    let CatalogItem::Membership(membership) = item else {
                        anyhow::bail!("membership page returned another kind");
                    };
                    ensure!(
                        membership.project_id == *project,
                        "membership page crossed its Project"
                    );
                    held.insert(membership.index_id);
                }
                match page.continuation {
                    Some(next) => after = Some(next),
                    None => break,
                }
            }
        }
        let ordered: Vec<IndexId> = self
            .indexes
            .iter()
            .filter(|id| held.contains(id))
            .copied()
            .collect();
        ensure!(
            ordered.len() == held.len(),
            "catalog membership names an Index outside the fixture"
        );
        Ok(ordered)
    }

    /// The in-process reader of `scope`: pipe reads under a window over its
    /// atoms. The fixture's current session shares the CLI's window, so the
    /// native and CLI reads of one session report one selection observation,
    /// as the catalog session's single grant did.
    async fn resolve(
        &self,
        scope: &SessionScope,
    ) -> Result<std::sync::Arc<dyn airk_mcp::remote::RemoteTools>> {
        let window = if *scope == self.session {
            self.session_window().await?
        } else {
            self.window_for(scope).await?
        };
        Ok(std::sync::Arc::new(SessionReads {
            defaults: session_defaults(scope, &window),
            inner: pipe::PipeTools::window(&self.control, window),
        }))
    }

    /// The window the CLI session route reads through: the cached one while
    /// the service still knows it and the session and the Indexes it selects
    /// are unchanged (renewed and granted again at the current revision; a
    /// refused grant keeps the previous one), else a new one.
    async fn session_window(&self) -> Result<pipe::Window> {
        let cached = self.cli_window.lock().unwrap().clone();
        let indexes = self.scope_indexes(&self.session).await?;
        if let Some((scope, mut window)) = cached
            && scope == self.session
            && window.selection.index_ids == indexes
            && pipe::renew(&self.control, &window).await.is_ok()
        {
            if let Err(error) = pipe::regrant(&self.control, &mut window).await {
                eprintln!("fixture CLI window keeps its previous grant: {error:#}");
            }
            *self.cli_window.lock().unwrap() = Some((scope, window.clone()));
            return Ok(window);
        }
        let window = self.window_for(&self.session).await?;
        *self.cli_window.lock().unwrap() = Some((self.session.clone(), window.clone()));
        Ok(window)
    }

    async fn cli(&self, tool: &str, arguments: &Value) -> Result<Value> {
        let window = self.session_window().await?;
        let mut arguments = arguments.clone();
        if let (Some(defaults), Some(fields)) = (
            session_defaults(&self.session, &window),
            arguments.as_object_mut(),
        ) {
            with_session_defaults(fields, &defaults);
        }
        let arguments = &arguments;
        let mut command = Ctl::new();
        command
            .window(window)
            .arg("--service-runtime-dir")
            .arg(&self.control)
            .args(["tools", "call", tool, "--args"])
            .arg(arguments.to_string());
        let output =
            tokio::task::spawn_blocking(move || read_retry::output(&mut command)).await??;
        ensure!(
            output.status.success() || output.status.code() == Some(1),
            "client process failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).with_context(|| {
            format!(
                "CLI did not return an envelope: {}",
                String::from_utf8_lossy(&output.stderr)
            )
        })
    }

    fn assert_sources_unchanged(&self) -> Result<()> {
        for (n, registration) in self.registrations.iter().enumerate() {
            for directory in ["api", "impl", "private", "api-other"] {
                let path = Path::new(&registration.root)
                    .join(directory)
                    .join("policy.md");
                assert_eq!(
                    fs::read_to_string(path)?,
                    format!("# boundaryneedle {n} {directory}\nOriginal isolated source.\n")
                );
            }
        }
        Ok(())
    }

    async fn cli_search(&self) -> Result<Value> {
        let mut command = Ctl::new();
        command
            .window(self.session_window().await?)
            .arg("--service-runtime-dir")
            .arg(&self.control)
            .args([
                "search",
                "boundaryneedle",
                "--mode",
                "lexical",
                "--limit",
                "12",
                "--max-bytes",
                "8192",
                "--verification",
                "off",
            ])
            .env("AIRK_MCP_RESPONSE_MODE", "full");
        let output =
            tokio::task::spawn_blocking(move || read_retry::output(&mut command)).await??;
        ensure!(
            output.status.success(),
            "convenience search failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).context("convenience search omitted its result")
    }

    #[cfg(feature = "local-inference")]
    async fn native_cli(&self, index: usize, arguments: Vec<String>) -> Result<Value> {
        let mut command = Ctl::new();
        command
            .arg("--service-runtime-dir")
            .arg(&self.control)
            .arg("--service-project")
            .arg(self.registrations[index].binding.to_string())
            .args(arguments)
            .env("AIRK_MCP_RESPONSE_MODE", "full");
        // Mutations run once with the actual staged producer's native identity.
        // Transport uncertainty never creates or replays a request key.
        let output = tokio::task::spawn_blocking(move || {
            read_retry::once_until(&command, Instant::now() + Duration::from_secs(120))
        })
        .await??;
        ensure!(
            output.status.success(),
            "owned native administration failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(512)
                .collect::<String>()
        );
        serde_json::from_slice(&output.stdout).context("native administration omitted its receipt")
    }
}

fn body(envelope: &Value) -> Result<Value> {
    body_with_budgets(
        envelope,
        airk_core::MAX_RESPONSE_BYTES,
        3 * airk_core::MAX_RESPONSE_BYTES + 4096,
    )
}

/// Plans own their declared output allocation; ordinary native read bodies
/// continue to use body()'s unchanged 64 KiB contract.
fn parse_body(envelope: &Value, declared_output_bytes: usize) -> Result<Value> {
    ensure!((1024..=524288).contains(&declared_output_bytes));
    let transport_budget = (3 * declared_output_bytes + 4096).min(1024 * 1024 - 2048);
    let value = body_with_budgets(envelope, declared_output_bytes, transport_budget)?;
    ensure!(
        value["plan"]["output"]["max_bytes"] == json!(declared_output_bytes),
        "parse reply changed the declared output allocation"
    );
    Ok(value)
}

/// The parity adapter must use the operation's original allocation. Plans
/// already have a larger native output contract; applying an ordinary read
/// frame here rejects a valid complete native reply before its plan check.
fn tool_body(envelope: &Value, tool: &str, arguments: &Value) -> Result<Value> {
    if !matches!(tool, "parse" | "airk_parse") {
        return body(envelope);
    }
    // Decode through the same native MCP/Core request owners rather than
    // reading an allowance from the response or mirroring its defaults.
    let public: airk_mcp::ParseArgs = serde_json::from_value(arguments.clone())?;
    let request = airk_core::workbench::decode_request(serde_json::to_value(public)?)?;
    let output = request
        .output
        .or_else(|| request.plan.map(|plan| plan.output))
        .unwrap_or_default();
    parse_body(envelope, output.max_bytes)
}

#[test]
fn parity_plan_budget_preserves_native_precedence_and_refuses_ordinary_read_overflow() -> Result<()>
{
    let request = json!({"schema":"airk.parse.request/1","mode":"execute","verification":"off",
        "response_mode":"full", "plan":{"steps":[{"id":"one","tool":"capabilities","args":{}}],
            "output":{"view":"raw","max_bytes":262144}},
        "output":{"view":"raw","max_bytes":131072}});
    let value = json!({"schema":"airk.workbench.result/1","plan":{"output":{"max_bytes":131072}},
        "payload":"x".repeat(125000)});
    let envelope = airk_mcp::unified::success(value.clone());
    let original = envelope.clone();
    // Literal old read framing refuses this complete plan. The native declared
    // 128-KiB canonical allocation and its existing transport wrapper accept it.
    assert!(serde_json::to_vec(&envelope)?.len() > 200704);
    assert!(serde_json::to_vec(&value)?.len() < 131072);
    assert_eq!(tool_body(&envelope, "parse", &request)?, value);
    assert_eq!(tool_body(&envelope, "airk_parse", &request)?, value);
    assert_eq!(envelope, original);
    assert!(tool_body(&envelope, "capabilities", &request).is_err());
    let mut narrower = request.clone();
    narrower["output"]["max_bytes"] = 65536.into();
    let mut narrow_value = value.clone();
    narrow_value["plan"]["output"]["max_bytes"] = 65536.into();
    assert!(
        tool_body(
            &airk_mcp::unified::success(narrow_value),
            "parse",
            &narrower
        )
        .is_err()
    );
    let mut wrong_reply = value.clone();
    wrong_reply["plan"]["output"]["max_bytes"] = 262144.into();
    assert!(tool_body(&airk_mcp::unified::success(wrong_reply), "parse", &request).is_err());
    let default_request =
        json!({"schema":"airk.parse.request/1","mode":"prepare","text":"/capabilities"});
    let mut default_value = value;
    default_value["plan"]["output"]["max_bytes"] = 262144.into();
    assert_eq!(
        tool_body(
            &airk_mcp::unified::success(default_value.clone()),
            "parse",
            &default_request
        )?,
        default_value
    );
    Ok(())
}

fn body_with_budgets(
    envelope: &Value,
    body_budget: usize,
    transport_budget: usize,
) -> Result<Value> {
    // Check the exact received representation before resolving any metadata
    // references. Expanded logical copies have a separate materialization bound.
    let wrapped = envelope.get("structuredContent").is_some()
        || envelope.pointer("/content/0/type").and_then(Value::as_str) == Some("text");
    // Public MCP/CLI replies carry both text and structured copies. Native
    // canonical data retains its owning allocation; their explicit public
    // wrapper is bounded separately by the supplied framing allowance.
    let maximum = if wrapped {
        transport_budget
    } else {
        body_budget
    };
    airk_core::index_catalog::encoded_size(envelope, maximum).with_context(||{
        let bytes=airk_core::index_catalog::encoded_size(envelope,usize::MAX).ok();
        let text_bytes=envelope.pointer("/content/0/text").and_then(Value::as_str).map(str::len);
        format!("received result exceeds {maximum}-byte frame bound: canonical_bytes={bytes:?},content_text_bytes={text_bytes:?},has_structured_content={}; raw reply: {}",
            envelope.get("structuredContent").is_some(),save_replies("raw frame bound",envelope).map(|path|path.display().to_string()).unwrap_or_else(|error|format!("diagnostic unavailable: {error}")))
    })?;
    let value = if let Some(value) = envelope.get("structuredContent") {
        value.clone()
    } else if let Some(text) = envelope["content"][0]["text"].as_str() {
        serde_json::from_str(text)?
    } else {
        envelope.clone()
    };
    airk_core::index_catalog::encoded_size(&value, body_budget)
        .context("unwrapped canonical body exceeds its explicit owning budget")?;
    if value["schema"] == "airk.combined-read/1" || value.get("delivery_observation").is_some() {
        let delivery = value
            .get("delivery_observation")
            .context("native combined delivery proof missing")?;
        ensure!(
            delivery["digest_basis"] == "canonical_body_without_delivery_observation",
            "unexpected delivery digest basis"
        );
        let receipt: airk_core::service::CombinedDeliveryReceipt =
            serde_json::from_value(delivery["receipt"].clone())?;
        ensure!(receipt.schema == 1, "unknown delivery receipt schema");
        let mut digest_body = value.clone();
        let fields = digest_body
            .as_object_mut()
            .context("delivery body must be an object")?;
        fields.remove("delivery_observation");
        // U110 (contracts §2.11): an open read adds `read_scope` (how its scope
        // was chosen) to the delivered result after the native delivery
        // digest is taken. It is outside that proof; every native field
        // stays covered, and `read_scope` itself stays in every comparison.
        if fields
            .get("read_scope")
            .is_some_and(|scope| scope.get("source").is_some_and(Value::is_string))
        {
            fields.remove("read_scope");
        }
        let digest = framed_digest(
            b"airk/combined-delivery-result/1",
            &[&serde_json::to_vec(&digest_body)?],
        );
        ensure!(
            receipt.result_digest == digest,
            "delivery digest does not cover the complete native result"
        );
        if value["schema"] == "airk.combined-read/1" {
            ensure!(
                value["publication_vector"]["authority"] == json!(receipt.scope_identity),
                "delivery proof lost its scoped publication identity"
            );
            let members = value["members"]
                .as_array()
                .context("combined member allocations missing")?;
            if let Some(context) = value.get("context") {
                let summary: airk_core::combined_context::CombinedContext =
                    serde_json::from_value(context.clone())?;
                let indexes = members
                    .iter()
                    .map(|member| serde_json::from_value::<IndexId>(member["index_id"].clone()))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                summary.validate(&indexes)?;
                for member in members {
                    let allocation: airk_core::combined_reads::MemberAllocation =
                        serde_json::from_value(member["allocation"].clone())?;
                    ensure!(
                        allocation.deadline_ms <= summary.deadline_ms(),
                        "context member exceeds its one shared original deadline"
                    );
                }
            } else {
                let deadline = members
                    .iter()
                    .try_fold(0u64, |total, member| -> Result<u64> {
                        let part = member["allocation"]["deadline_ms"]
                            .as_u64()
                            .context("combined deadline allocation missing")?;
                        total
                            .checked_add(part)
                            .context("combined deadline allocation overflow")
                    })?;
                ensure!(
                    deadline <= 120_000,
                    "allocated member deadlines exceeded the 120000 ms request deadline ceiling"
                );
            }
        }
    }
    Ok(value)
}

/// An Answer /3 reply, or its combined wrapper, under the 131,072-byte
/// Answer frame (every other tool keeps the 65,536-byte frame of `body`).
fn answer_body(envelope: &Value) -> Result<Value> {
    let frame = airk_core::arbiter::answer_loop::MAX_ANSWER_RESPONSE_BYTES;
    body_with_budgets(envelope, frame, 3 * frame + 4096)
}

fn answer_member_body(combined: &Value, member: &Value) -> Result<Value> {
    let index: IndexId = serde_json::from_value(member["index_id"].clone())?;
    let value = airk_core::combined_reads::shared_values::materialize_combined_member_result(
        combined, index,
    )
    .with_context(|| format!("materialize native member {index}"))?;
    answer_body(&value).with_context(|| format!("inspect expanded native member {index}"))
}

/// The `airk.combined-context/2` summary after its owner validated it: the
/// attempt is a transport identity, and every byte counter is a function of
/// member reply lengths, which carry timing digits (`LoopUse::of_reply`
/// takes the larger of the read bytes and the reply's own length, and
/// hand-back grows later allowances by it). Every other counter, outcome,
/// termination and delivery binding stays compared, and each member reply
/// is compared in full, including its `work.bytes`.
fn answer_summary_copy(
    value: &mut Value,
    summary: &airk_core::combined_context::CombinedContextSummaryV2,
) {
    let context = &mut value["context"];
    context["attempt"] = json!("<original-context-attempt>");
    for counter in ["known_used", "unknown_held"] {
        context[counter]["bytes"] = json!("<reply-length-bytes>");
    }
    for (position, member) in summary.members.iter().enumerate() {
        let report = &mut context["members"][position];
        if member.allowance.is_some() {
            report["allowance"]["attempt"] = json!("<original-context-attempt>");
            report["allowance"]["resources"]["bytes"] = json!("<reply-length-bytes>");
        }
        if matches!(
            member.outcome,
            airk_core::combined_context::PartitionOutcomeV3::Known { .. }
        ) {
            report["outcome"]["used"]["bytes"] = json!("<reply-length-bytes>");
        }
    }
}

fn member_body(combined: &Value, member: &Value) -> Result<Value> {
    let index: IndexId = serde_json::from_value(member["index_id"].clone())?;
    let value = airk_core::combined_reads::shared_values::materialize_combined_member_result(
        combined, index,
    )
    .with_context(|| format!("materialize native member {index}"))?;
    body(&value).with_context(|| format!("inspect expanded native member {index}"))
}

fn logical_comparison_copy(raw: &Value) -> Result<Value> {
    if raw["schema"] != "airk.combined-read/1" {
        return Ok(raw.clone());
    }
    // Reauthenticate even when a caller already peeled its transport envelope.
    // Never normalize a receipt or a dictionary before checking the actual wire.
    body(raw)?;
    airk_core::verification::validate_projection(raw)?;
    // body() checked the complete RAW delivery digest first. Core now checks
    // every known dictionary, referenced header, typed primitive slot and origin
    // while restoring every member's exact canonical native data in a copy.
    let mut logical = airk_core::combined_reads::materialize_combined_read(raw)
        .context("strictly reconstruct complete combined logical view")?;
    normalize_member_verification_timing(&mut logical)?;
    normalize_member_allocation_timing(&mut logical)?;
    if let Some(dictionary) = logical.get("shared_values") {
        ensure!(
            dictionary["schema"]
                == airk_core::combined_reads::shared_values::SHARED_CAPABILITY_SCHEMA
                || dictionary["schema"]
                    == airk_core::combined_reads::header_templates::HEADER_SCHEMA,
            "comparison cannot omit an unknown wire dictionary"
        );
        // Only this authenticated encoding dictionary is derived-only. All of
        // its logical fields now exist in restored members. Keep original wire
        // dictionaries/proofs in raw replies and failure artifacts; normalize
        // named request/timing observations only after this exact reconstruction.
        logical
            .as_object_mut()
            .context("logical combined read must be an object")?
            .remove("shared_values");
    }
    Ok(logical)
}

/// Only these per-member receipt fields contain an elapsed allowance, not
/// the configured shared verification budget. Every other field remains literal.
fn normalize_member_verification_timing(logical: &mut Value) -> Result<()> {
    if logical["schema"] != "airk.combined-read/1" {
        return Ok(());
    }
    let members = logical["members"]
        .as_array_mut()
        .context("missing comparison members")?;
    ensure!(
        members.len() <= airk_core::index_catalog::MAX_SELECTION,
        "comparison member bound exceeded"
    );
    for member in members {
        let Some(receipt) = member
            .get("result")
            .and_then(|result| result.get("source_verification"))
        else {
            continue;
        };
        if receipt["schema"] != "airk.saved-source-verification/1" {
            continue;
        }
        let receipt: airk_core::verification::VerificationReceipt =
            serde_json::from_value(receipt.clone())
                .context("malformed comparison verification receipt")?;
        let allocation: airk_core::combined_reads::MemberAllocation =
            serde_json::from_value(member["allocation"].clone())
                .context("verification receipt missing its admitted member allocation")?;
        let index: IndexId = serde_json::from_value(member["index_id"].clone())?;
        ensure!(
            allocation.index_id == index,
            "crossed verification member allocation"
        );
        ensure!(
            allocation.verification_deadline_ms <= airk_core::verification::MAX_DEADLINE_MS
                && receipt.limits.deadline_ms <= allocation.verification_deadline_ms
                && receipt.limits.effective_deadline_ms <= receipt.limits.deadline_ms,
            "verification receipt exceeded its admitted deadline"
        );
        ensure!(
            receipt.limits.max_sources <= allocation.verification_max_sources
                && receipt.limits.max_bytes <= allocation.verification_max_bytes,
            "verification receipt exceeded its admitted source/byte budget"
        );
        let limits = &mut member["result"]["source_verification"]["limits"];
        limits["deadline_ms"] = json!(0);
        limits["effective_deadline_ms"] = json!(0);
    }
    Ok(())
}

/// Ordinary combined reads split the request clock remaining after metadata I/O.
/// Context allocations instead declare the exact original context budget.
fn normalize_member_allocation_timing(logical: &mut Value) -> Result<()> {
    if logical["schema"] != "airk.combined-read/1" || logical.get("context").is_some() {
        return Ok(());
    }
    let members = logical["members"]
        .as_array_mut()
        .context("missing allocation members")?;
    ensure!(
        !members.is_empty() && members.len() <= airk_core::index_catalog::MAX_SELECTION,
        "invalid allocation member count"
    );
    let mut total = 0u64;
    let mut deadlines = Vec::with_capacity(members.len());
    for member in members.iter() {
        let allocation: airk_core::combined_reads::MemberAllocation =
            serde_json::from_value(member["allocation"].clone())?;
        let index: IndexId = serde_json::from_value(member["index_id"].clone())?;
        ensure!(
            allocation.index_id == index,
            "crossed member allocation identity"
        );
        total = total
            .checked_add(allocation.deadline_ms)
            .context("allocation clock overflow")?;
        deadlines.push(allocation.deadline_ms);
    }
    ensure!(
        (1..=120_000).contains(&total),
        "remaining member clocks exceed original request deadline or have no admitted clock"
    );
    let count = members.len() as u64;
    for (offset, deadline) in deadlines.into_iter().enumerate() {
        ensure!(
            deadline == total / count + u64::from((offset as u64) < total % count),
            "member clocks do not preserve the canonical shared remainder split"
        );
    }
    for member in members {
        member["allocation"]["deadline_ms"] = json!("<remaining-request-clock>");
    }
    Ok(())
}

#[test]
fn current_combined_remaining_clocks_preserve_all_fixed_caps_and_unknown_fields() -> Result<()> {
    let first = IndexId::from_identity(Incarnation::from_bytes([81; 16]));
    let second = IndexId::from_identity(Incarnation::from_bytes([82; 16]));
    let member = |index| {
        json!({"index_id":index,"allocation":{
        "index_id":index,"limit":4,"max_bytes":4096,"deadline_ms":14942,
        "verification_max_sources":8,"verification_max_bytes":8192,"verification_deadline_ms":2000},
        "result":{"future":{"deadline_ms":17},"allocation":{"deadline_ms":19}}})
    };
    let raw = json!({"schema":"airk.combined-read/1","members":[member(first),member(second)]});
    let unchanged = raw.clone();
    let mut expected = raw.clone();
    normalize_member_allocation_timing(&mut expected)?;
    let mut slower = raw.clone();
    for member in slower["members"].as_array_mut().unwrap() {
        member["allocation"]["deadline_ms"] = json!(14938);
    }
    normalize_member_allocation_timing(&mut slower)?;
    assert_eq!(slower, expected);
    assert_eq!(raw, unchanged);
    for (pointer, value) in [
        ("/members/0/allocation/limit", json!(3)),
        ("/members/0/allocation/max_bytes", json!(4095)),
        ("/members/0/allocation/verification_max_sources", json!(7)),
        ("/members/0/allocation/verification_max_bytes", json!(8191)),
        (
            "/members/0/allocation/verification_deadline_ms",
            json!(1999),
        ),
        ("/members/0/result/future/deadline_ms", json!(18)),
        ("/members/0/result/allocation/deadline_ms", json!(20)),
    ] {
        let mut changed = raw.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        normalize_member_allocation_timing(&mut changed)?;
        assert_ne!(changed, expected, "masked semantic field {pointer}");
    }
    for (a, b) in [
        (json!(0), json!(0)),
        (json!(60001), json!(60000)),
        (json!(14943), json!(14941)),
        (json!(u64::MAX), json!(1)),
        (json!(true), json!(14942)),
    ] {
        let mut invalid = raw.clone();
        invalid["members"][0]["allocation"]["deadline_ms"] = a;
        invalid["members"][1]["allocation"]["deadline_ms"] = b;
        assert!(normalize_member_allocation_timing(&mut invalid).is_err());
    }
    let mut crossed = raw.clone();
    crossed["members"][0]["allocation"]["index_id"] = json!(second);
    assert!(normalize_member_allocation_timing(&mut crossed).is_err());
    let mut context = raw.clone();
    context["context"] = json!({"budget":{"deadline_ms":30000}});
    let exact_context = context.clone();
    normalize_member_allocation_timing(&mut context)?;
    assert_eq!(
        context, exact_context,
        "original context budget was treated as an elapsed clock"
    );
    Ok(())
}

async fn native_read(
    backend: &dyn airk_mcp::remote::RemoteTools,
    tool: &str,
    arguments: &Value,
) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let reply = airk_mcp::remote::call_observed(
            backend,
            tool.into(),
            arguments
                .as_object()
                .context("object arguments required")?
                .clone(),
        )
        .await?;
        let Some(retry) = read_retry::within(&reply, deadline) else {
            return Ok(reply);
        };
        tokio::time::sleep(retry.delay).await;
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct ParityCursor {
    schema: u32,
    tool: String,
    arguments: Sha256Digest,
    scope_identity: Sha256Digest,
    vector: airk_core::combined_reads::PublicationVector,
    children: std::collections::BTreeMap<IndexId, Option<String>>,
    checksum: Sha256Digest,
}

fn cursor_checksum(cursor: &ParityCursor) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"airk/combined-cursor/1",
        &[&serde_json::to_vec(&(
            cursor.schema,
            &cursor.tool,
            cursor.arguments,
            cursor.scope_identity,
            &cursor.vector,
            &cursor.children,
        ))?],
    ))
}

fn stable_cursor(encoded: &str) -> Result<String> {
    ensure!(
        encoded.len() <= 32768,
        "combined cursor exceeded byte bound"
    );
    let mut cursor: ParityCursor = serde_json::from_str(
        encoded
            .strip_prefix("airk-cq1:")
            .context("combined cursor prefix missing")?,
    )?;
    cursor.vector.validate()?;
    ensure!(
        cursor.schema == 1
            && cursor.scope_identity == cursor.vector.authority
            && cursor.children.keys().copied().eq(cursor
                .vector
                .members
                .iter()
                .map(|member| member.index_id))
            && cursor
                .vector
                .members
                .iter()
                .all(|member| member.publication.is_some()),
        "combined cursor shape or authority changed"
    );
    ensure!(
        cursor.checksum == cursor_checksum(&cursor)?,
        "combined cursor checksum is invalid"
    );
    ensure!(
        format!("airk-cq1:{}", serde_json::to_string(&cursor)?) == encoded,
        "combined cursor was not canonical"
    );
    // The checksum includes the original dates. Authenticate the actual wire
    // cursor first, then produce a comparison checksum after removing only
    // observation dates; every scope/publication/filter/position field remains.
    cursor.vector.observed_from_unix_ms = 0;
    cursor.vector.observed_to_unix_ms = 0;
    cursor.checksum = cursor_checksum(&cursor)?;
    Ok(format!("airk-cq1:{}", serde_json::to_string(&cursor)?))
}

/// Only transport identities and already checked dated/elapsed observations
/// differ. Scope, publications, semantic budgets, omissions and retries remain.
fn stable(mut value: Value) -> Result<Value> {
    match &mut value {
        Value::Object(fields) => {
            // Generated instructions embed only their outer observation time.
            // Normalize that documented timing identity, retaining all recipe text.
            if fields.get("schema") == Some(&json!("airk.recommendations/1")) {
                let stamp = fields
                    .get("observed_unix_ms")
                    .map(Value::to_string)
                    .unwrap_or_default();
                if let Some(text) = fields
                    .get_mut("instructions")
                    .and_then(|v| v.get_mut("text"))
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
                {
                    fields["instructions"]["text"] = json!(
                        text.replace(&format!("at {stamp} Unix ms"), "at <observation> Unix ms")
                    );
                }
            }
            // Every complete digest was checked by body() before comparison.
            // Its input includes request IDs and dated observation intervals;
            // preserve the live authority/service/revision fields themselves.
            if let Some(receipt) = fields
                .get_mut("delivery_observation")
                .and_then(|observation| observation.get_mut("receipt"))
                .and_then(Value::as_object_mut)
            {
                receipt.remove("result_digest");
            }
            for key in [
                "request_id",
                "observed_unix_ms",
                "observed_from_unix_ms",
                "observed_to_unix_ms",
                "cache_age_ms",
                "cached",
                "elapsed_ms",
            ] {
                fields.remove(key);
            }
            // Callers already peeled transport envelopes. Evidence text must
            // remain literal, even when the source happens to contain JSON.
            for entry in fields.values_mut() {
                *entry = stable(entry.take())?;
            }
        }
        Value::Array(values) => {
            for entry in values {
                *entry = stable(entry.take())?;
            }
        }
        Value::String(encoded) if encoded.starts_with("airk-cq1:") => {
            *encoded = stable_cursor(encoded)?;
        }
        _ => {}
    }
    Ok(value)
}

fn first_difference(left: &Value, right: &Value, path: &str) -> Option<String> {
    if left == right {
        return None;
    }
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            let keys = a
                .keys()
                .chain(b.keys())
                .collect::<std::collections::BTreeSet<_>>();
            for key in keys {
                let next = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                match (a.get(key), b.get(key)) {
                    (Some(a), Some(b)) => {
                        if let Some(diff) = first_difference(a, b, &next) {
                            return Some(diff);
                        }
                    }
                    _ => return Some(format!("{next}: field present on only one side")),
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                return Some(format!("{path}: array lengths {} vs {}", a.len(), b.len()));
            }
            for (n, (a, b)) in a.iter().zip(b).enumerate() {
                if let Some(diff) = first_difference(a, b, &format!("{path}/{n}")) {
                    return Some(diff);
                }
            }
        }
        _ => {}
    }
    Some(format!(
        "{path}: {} vs {}",
        left.to_string().chars().take(160).collect::<String>(),
        right.to_string().chars().take(160).collect::<String>()
    ))
}

// Workbench owns each step's MCP result envelope. Decode only that explicit
// duplicate, after authenticating every original body; source text stays literal.
fn workbench_comparison_copy(raw: &Value) -> Result<Value> {
    ensure!(raw["schema"] == airk_core::workbench::RESULT_SCHEMA);
    let maximum = usize::try_from(
        raw.pointer("/plan/output/max_bytes")
            .and_then(Value::as_u64)
            .context("Workbench output allocation missing")?,
    )?;
    // Original request precedence/bounds were checked by tool_body(). This
    // second check authenticates the unchanged original receipt before copying.
    parse_body(raw, maximum)?;
    let mut result = raw.clone();
    if let Some(receipt) = result
        .get_mut("delivery_observation")
        .and_then(|observation| observation.get_mut("receipt"))
    {
        receipt["result_digest"] = json!("<authenticated-raw-observation-digest>");
    }
    let steps = raw["steps"].as_array().context("Workbench steps missing")?;
    ensure!(steps.len() <= airk_core::workbench::MAX_STEP_REPORTS);
    for (offset, step) in steps.iter().enumerate() {
        let mut logical = current_observations(step, &["/elapsed_ms"], false)?;
        if let Some(envelope) = step.get("result").filter(|value| !value.is_null()) {
            let tool = step["tool"]
                .as_str()
                .context("Workbench step tool missing")?;
            let native = tool_body(envelope, tool, &step["arguments"])?;
            if native["schema"] == airk_core::workbench::RESULT_SCHEMA {
                // Parse is not an executable plan step. Fail closed if a future
                // nested plan changes that contract rather than ignore its proof.
                anyhow::bail!("unreviewed nested Workbench plan result");
            }
            let status = matches!(tool, "status" | "airk_status");
            logical["result"] = if envelope.get("structuredContent").is_some() {
                current_envelope_copy(envelope, status)?
            } else if envelope.pointer("/content/0/type").and_then(Value::as_str) == Some("text") {
                // Text-only MCP still has one explicit canonical JSON slot.
                // Preserve other blocks and wrapper fields, including evidence.
                let mut copy = envelope.clone();
                copy["content"][0]["text"] = current_body_copy(&native, status)?;
                copy
            } else {
                // A single selected index returns its primitive body directly.
                // Normalize only that native root's typed observations.
                current_body_copy(&native, status)?
            };
        }
        result["steps"][offset] = logical;
    }
    Ok(result)
}

fn parity_comparison_copy(raw: &Value) -> Result<Value> {
    if raw["schema"] == airk_core::workbench::RESULT_SCHEMA {
        workbench_comparison_copy(raw)
    } else {
        stable(logical_comparison_copy(raw)?)
    }
}

// Independent small native-shaped bodies, with real delivery framing. These
// pure regressions do not depend on retained private transport captures.
fn signed_workbench_parity_fixture(mut body: Value) -> Result<Value> {
    body.as_object_mut()
        .context("fixture body")?
        .remove("delivery_observation");
    let mut covered = body.clone();
    if covered
        .pointer("/read_scope/source")
        .is_some_and(Value::is_string)
    {
        covered.as_object_mut().unwrap().remove("read_scope");
    }
    let digest = framed_digest(
        b"airk/combined-delivery-result/1",
        &[&serde_json::to_vec(&covered)?],
    );
    let scope = body
        .pointer("/publication_vector/authority")
        .or_else(|| body.get("scope_identity"))
        .cloned()
        .unwrap_or_else(|| json!(Sha256Digest::from_bytes([11; 32])));
    body["delivery_observation"] = json!({"digest_basis":"canonical_body_without_delivery_observation",
        "receipt":{"schema":1,"service_instance":Incarnation::from_bytes([12;16]),
        "namespace":Incarnation::from_bytes([13;16]),"catalog_revision":2,
        "scope_identity":scope,"result_digest":digest}});
    Ok(body)
}

fn workbench_parity_fixture(child: Option<Value>) -> Result<Value> {
    let indexes = [
        IndexId::from_identity(Incarnation::from_bytes([81; 16])),
        IndexId::from_identity(Incarnation::from_bytes([82; 16])),
    ];
    let member = |offset: usize| {
        json!({"index_id":indexes[offset],"index_name":format!("fixture{offset}"),
        "is_error":false,"outcome":"complete","elapsed_ms":47+offset,
        "allocation":{"index_id":indexes[offset],"limit":4,"max_bytes":4096,
        "deadline_ms":14977-offset,"verification_max_sources":0,"verification_max_bytes":0,
        "verification_deadline_ms":0},"result":{"request_id":Incarnation::from_bytes([21+offset as u8;16]),
        "content_hash":Sha256Digest::from_bytes([31;32]),"source_path":"fixture.rs",
        "future":{"request_id":"source-identity","elapsed_ms":7},
        "hits":[{"text":"{\"request_id\":\"literal source\",\"elapsed_ms\":19}"}]}})
    };
    let child = child.unwrap_or_else(|| {
        json!({"schema":"airk.combined-read/1",
        "publication_vector":{"authority":Sha256Digest::from_bytes([11;32]),
        "observed_from_unix_ms":10,"observed_to_unix_ms":20,
        "members":indexes.map(|index|json!({"index_id":index,"publication":null}))},
        "members":[member(0),member(1)],"ranking":[],
        "read_scope":{"source":"explicit","index_ids":indexes}})
    });
    let child = signed_workbench_parity_fixture(child)?;
    signed_workbench_parity_fixture(json!({"schema":"airk.workbench.result/1",
        "scope_identity":Sha256Digest::from_bytes([11;32]),
        "plan":{"output":{"max_bytes":262144},"budgets":{"seconds":30}},
        "future":{"request_id":"plan-evidence","elapsed_ms":9},
        "steps":[{"id":"one","tool":"capabilities","arguments":{"max_bytes":8192},
        "elapsed_ms":242,"status":"complete","error":null,
        "result":{"isError":false,"structuredContent":child,
        "content":[{"type":"text","text":child.to_string()},
        {"type":"text","text":"{\"request_id\":\"second evidence block\",\"elapsed_ms\":11}"}]}}]}))
}

#[test]
fn workbench_nested_native_observations_compare_without_mutating_originals() -> Result<()> {
    let original = workbench_parity_fixture(None)?;
    let before = original.clone();
    let mut child = original["steps"][0]["result"]["structuredContent"].clone();
    child["publication_vector"]["observed_from_unix_ms"] = json!(30);
    child["publication_vector"]["observed_to_unix_ms"] = json!(40);
    for (offset, member) in child["members"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        member["allocation"]["deadline_ms"] = json!(14976);
        member["elapsed_ms"] = json!(52 + offset);
        member["result"]["request_id"] = json!(Incarnation::from_bytes([41 + offset as u8; 16]));
    }
    let mut later = workbench_parity_fixture(Some(child))?;
    later["steps"][0]["elapsed_ms"] = json!(248);
    later = signed_workbench_parity_fixture(later)?;
    assert_ne!(original, later);
    assert_eq!(
        parity_comparison_copy(&original)?,
        parity_comparison_copy(&later)?
    );
    assert_eq!(
        original, before,
        "comparison changed authenticated original bytes"
    );
    Ok(())
}

#[test]
fn workbench_nested_native_comparison_retains_semantics_and_refuses_drift() -> Result<()> {
    let original = workbench_parity_fixture(None)?;
    let expected = parity_comparison_copy(&original)?;
    let child = original["steps"][0]["result"]["structuredContent"].clone();
    for (pointer, value) in [
        ("/members/0/allocation/max_bytes", json!(4095)),
        ("/members/0/allocation/verification_max_sources", json!(1)),
        (
            "/members/0/result/content_hash",
            json!(Sha256Digest::from_bytes([32; 32])),
        ),
        ("/members/0/result/source_path", json!("other.rs")),
        (
            "/members/0/result/future/request_id",
            json!("different-source-identity"),
        ),
        ("/members/0/result/future/elapsed_ms", json!(8)),
        (
            "/members/0/result/hits/0/text",
            json!("{\"request_id\":\"changed source\",\"elapsed_ms\":19}"),
        ),
        ("/read_scope/source", json!("different-scope-source")),
        (
            "/publication_vector/authority",
            json!(Sha256Digest::from_bytes([14; 32])),
        ),
    ] {
        let mut changed = child.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert_ne!(
            parity_comparison_copy(&workbench_parity_fixture(Some(changed))?)?,
            expected,
            "masked semantic field {pointer}"
        );
    }
    for (pointer, value) in [
        ("/plan/budgets/seconds", json!(29)),
        ("/plan/output/max_bytes", json!(131072)),
        ("/scope_identity", json!(Sha256Digest::from_bytes([16; 32]))),
        ("/future/elapsed_ms", json!(10)),
        ("/steps/0/arguments/max_bytes", json!(8191)),
        ("/future/request_id", json!("different-plan-evidence")),
        (
            "/steps/0/result/content/1/text",
            json!("{\"request_id\":\"different second evidence block\",\"elapsed_ms\":11}"),
        ),
        (
            "/steps/0/result/structuredContent/delivery_observation/receipt/service_instance",
            json!(Incarnation::from_bytes([15; 16])),
        ),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        if pointer.contains("structuredContent") {
            changed["steps"][0]["result"]["content"][0]["text"] =
                json!(changed["steps"][0]["result"]["structuredContent"].to_string());
        }
        let changed = signed_workbench_parity_fixture(changed)?;
        assert_ne!(
            parity_comparison_copy(&changed)?,
            expected,
            "masked semantic field {pointer}"
        );
    }
    for pointer in [
        "/delivery_observation/receipt/result_digest",
        "/steps/0/result/structuredContent/delivery_observation/receipt/result_digest",
        "/steps/0/result/content/0/text",
    ] {
        let mut invalid = original.clone();
        *invalid.pointer_mut(pointer).unwrap() = json!("invalid-original-proof");
        if pointer != "/delivery_observation/receipt/result_digest" {
            invalid = signed_workbench_parity_fixture(invalid)?;
        }
        assert!(
            parity_comparison_copy(&invalid).is_err(),
            "accepted {pointer}"
        );
    }
    Ok(())
}

#[test]
fn workbench_step_shape_matrix_covers_single_combined_and_mixed_results() -> Result<()> {
    let template = workbench_parity_fixture(None)?;
    let combined = template["steps"][0]["result"]["structuredContent"].clone();
    let primitive = json!({"request_id":Incarnation::from_bytes([61;16]),
        "content_hash":Sha256Digest::from_bytes([62;32]),"scope_identity":Sha256Digest::from_bytes([63;32]),
        "future":{"request_id":"retained-source-ID","elapsed_ms":19},
        "hits":[{"text":"{\"request_id\":\"literal evidence\",\"elapsed_ms\":23}"}]});
    let wrap = |body: &Value, shape| match shape {
        0 => body.clone(),
        1 => json!({"isError":false,"structuredContent":body,
            "content":[{"type":"text","text":body.to_string()},
            {"type":"text","text":"{\"request_id\":\"second source\"}"}],
            "future":{"request_id":"wrapper-evidence"}}),
        2 => json!({"isError":false,
            "content":[{"type":"text","text":body.to_string()},
            {"type":"text","text":"{\"request_id\":\"second source\"}"}],
            "future":{"request_id":"wrapper-evidence"}}),
        _ => unreachable!(),
    };
    let plan = |result: Value| -> Result<Value> {
        let mut plan = template.clone();
        plan["steps"][0]["result"] = result;
        signed_workbench_parity_fixture(plan)
    };
    let mut original_steps = Vec::new();
    let mut later_steps = Vec::new();
    for (kind, original) in [("single", primitive), ("combined", combined)] {
        let mut later = original.clone();
        if kind == "single" {
            later["request_id"] = json!(Incarnation::from_bytes([64; 16]));
        } else {
            later["publication_vector"]["observed_from_unix_ms"] = json!(30);
            later["publication_vector"]["observed_to_unix_ms"] = json!(40);
            for (offset, member) in later["members"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                member["allocation"]["deadline_ms"] = json!(14976);
                member["elapsed_ms"] = json!(51 + offset);
                member["result"]["request_id"] =
                    json!(Incarnation::from_bytes([65 + offset as u8; 16]));
            }
            later = signed_workbench_parity_fixture(later)?;
        }
        for shape in 0..3 {
            let first_plan = plan(wrap(&original, shape))?;
            let second_plan = plan(wrap(&later, shape))?;
            let expected = parity_comparison_copy(&first_plan)?;
            assert_eq!(
                expected,
                parity_comparison_copy(&second_plan)?,
                "{kind} shape {shape}"
            );
            let mut first_step = first_plan["steps"][0].clone();
            let mut later_step = second_plan["steps"][0].clone();
            first_step["id"] = json!(format!("{kind}-{shape}"));
            later_step["id"] = first_step["id"].clone();
            original_steps.push(first_step);
            later_steps.push(later_step);
            let prefix = if kind == "single" {
                ""
            } else {
                "/members/0/result"
            };
            for (suffix, replacement) in [
                ("/content_hash", json!(Sha256Digest::from_bytes([67; 32]))),
                ("/future/request_id", json!("changed-source-ID")),
                ("/future/elapsed_ms", json!(20)),
                (
                    "/hits/0/text",
                    json!("{\"request_id\":\"different literal evidence\",\"elapsed_ms\":23}"),
                ),
            ] {
                let mut changed = original.clone();
                let pointer = format!("{prefix}{suffix}");
                *changed.pointer_mut(&pointer).unwrap() = replacement;
                if kind == "combined" {
                    changed = signed_workbench_parity_fixture(changed)?;
                }
                assert_ne!(
                    expected,
                    parity_comparison_copy(&plan(wrap(&changed, shape))?)?,
                    "masked {kind} shape {shape} field {pointer}"
                );
            }
            let scoped_fields = if kind == "single" {
                vec![("/scope_identity", json!(Sha256Digest::from_bytes([69; 32])))]
            } else {
                vec![
                    (
                        "/publication_vector/authority",
                        json!(Sha256Digest::from_bytes([69; 32])),
                    ),
                    ("/read_scope/source", json!("different-scope-source")),
                    ("/members/0/allocation/max_bytes", json!(4095)),
                ]
            };
            for (pointer, replacement) in scoped_fields {
                let mut changed = original.clone();
                *changed.pointer_mut(pointer).unwrap() = replacement;
                if kind == "combined" {
                    changed = signed_workbench_parity_fixture(changed)?;
                }
                assert_ne!(
                    expected,
                    parity_comparison_copy(&plan(wrap(&changed, shape))?)?,
                    "masked {kind} shape {shape} scope/budget field {pointer}"
                );
            }
            let mut invalid = original.clone();
            invalid
                .pointer_mut(&format!("{prefix}/request_id"))
                .unwrap()
                .clone_from(&json!("not-a-native-transport-identity"));
            if kind == "combined" {
                invalid = signed_workbench_parity_fixture(invalid)?;
            }
            assert!(
                parity_comparison_copy(&plan(wrap(&invalid, shape))?).is_err(),
                "invalid typed request ID {kind} shape {shape}"
            );
            if kind == "combined" {
                let mut invalid = original.clone();
                invalid["delivery_observation"]["receipt"]["result_digest"] =
                    json!(Sha256Digest::from_bytes([68; 32]));
                assert!(
                    parity_comparison_copy(&plan(wrap(&invalid, shape))?).is_err(),
                    "invalid original combined proof shape {shape}"
                );
            }
            if shape == 1 {
                let mut invalid = wrap(&original, shape);
                invalid["content"][0]["text"] = json!("{\"mismatched\":true}");
                assert!(
                    parity_comparison_copy(&plan(invalid)?).is_err(),
                    "text/structured drift {kind}"
                );
            }
            if shape > 0 {
                let mut changed = wrap(&original, shape);
                changed["content"][1]["text"] = json!("{\"request_id\":\"changed second source\"}");
                assert_ne!(
                    expected,
                    parity_comparison_copy(&plan(changed)?)?,
                    "masked second evidence block {kind} shape {shape}"
                );
            }
        }
    }
    assert_eq!(original_steps.len(), 6);
    let mut first = template.clone();
    first["steps"] = json!(original_steps);
    let mut later = template;
    later["steps"] = json!(later_steps);
    let first = signed_workbench_parity_fixture(first)?;
    let later = signed_workbench_parity_fixture(later)?;
    assert_eq!(
        parity_comparison_copy(&first)?,
        parity_comparison_copy(&later)?,
        "mixed plan did not normalize every supported native result representation"
    );
    Ok(())
}

fn parity_difference(label: &str, left: &Value, right: &Value) -> Result<Option<String>> {
    let a = parity_comparison_copy(left)
        .with_context(|| format!("{label}: left logical reconstruction"))?;
    let b = parity_comparison_copy(right)
        .with_context(|| format!("{label}: right logical reconstruction"))?;
    let Some(difference) = first_difference(&a, &b, "") else {
        return Ok(None);
    };
    let path = save_replies(label, &json!({"left":left,"right":right}))?;
    Ok(Some(format!(
        "{label}: {difference}; complete replies: {}",
        path.display()
    )))
}

fn save_replies(label: &str, replies: &Value) -> Result<PathBuf> {
    let output = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .context("workspace root missing")?
        .join("artifacts/testing/combined-reads");
    fs::create_dir_all(&output)?;
    let path = output.join(format!(
        "{}.json",
        airk_kernel::service_broker::new_instance_id()?
    ));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({"label":label,"replies":replies}))?,
    )?;
    Ok(path)
}

fn assert_parity(label: &str, left: &Value, right: &Value) -> Result<()> {
    if let Some(difference) = parity_difference(label, left, right)? {
        anyhow::bail!("{difference}");
    }
    Ok(())
}

struct McpSession {
    transport: http::HttpMcpSession,
    negotiated_protocol: String,
}
/// The indexes a session scope reaches: its explicit `index_ids`, else every
/// index a selected project has a membership in (the fixture's memberships:
/// project 0 holds index 0, 1 and 3.., project 1 holds index 0, project 2
/// holds index 2), in fixture order.
fn indexes_of(f: &Fixture, selected: &SessionScope) -> Vec<IndexId> {
    if let Some(ids) = &selected.index_ids {
        return ids.clone();
    }
    f.indexes
        .iter()
        .enumerate()
        .filter(|(n, _)| {
            selected.project_ids.iter().any(|project| {
                match f.projects.iter().position(|p| p == project) {
                    Some(0) => *n != 2,
                    Some(1) => *n == 0,
                    Some(2) => *n == 2,
                    _ => false,
                }
            })
        })
        .map(|(_, id)| *id)
        .collect()
}

impl McpSession {
    /// The tokenless local agent session naming, on every read, the indexes
    /// the fixture's session reaches (read whole).
    fn start(f: &Fixture) -> Result<Self> {
        Self::start_selected(f, &f.session)
    }
    fn start_selected(f: &Fixture, selected: &SessionScope) -> Result<Self> {
        Self::start_scope(f, http::Scope::indexes(indexes_of(f, selected)))
    }
    fn start_scope(f: &Fixture, scope: http::Scope) -> Result<Self> {
        let mut session = Self {
            transport: http::HttpMcpSession::start(&f.control, scope)?,
            negotiated_protocol: String::new(),
        };
        let initialized = session.request(1, "initialize", json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"airk-combined-fixture","version":"1"}}))?;
        ensure!(
            initialized.get("error").is_none(),
            "initialize failed: {initialized}"
        );
        session.negotiated_protocol = initialized["result"]["protocolVersion"]
            .as_str()
            .context("actual negotiated MCP protocol")?
            .to_owned();
        session
            .transport
            .send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
        Ok(session)
    }
    fn request(&mut self, id: u64, method: &str, params: Value) -> Result<Value> {
        self.transport.request(id.try_into()?, method, params)
    }
    /// The open read of this session's scope through the pipe (M3
    /// `ToolSelection::Open`): the same service path the MCP read takes,
    /// so the two complete results compare directly. A refusal is an error.
    async fn pipe(&self, f: &Fixture, tool: &str, arguments: &Value) -> Result<Value> {
        pipe_read(f, self.transport.scope.open_selection(), tool, arguments)
            .await?
            .map_err(|refused| anyhow::anyhow!("open pipe read of {tool} refused: {refused}"))
    }
    fn call(&mut self, mut id: u64, name: &str, arguments: Value) -> Result<Value> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let reply =
                self.request(id, "tools/call", json!({"name":name,"arguments":arguments}))?;
            let Some(retry) = read_retry::within(&reply["result"], deadline) else {
                return Ok(reply);
            };
            std::thread::sleep(retry.delay);
            id += 100;
        }
    }
    /// U110: one read through tokenless HTTPS MCP and the same open read
    /// through the pipe, with their complete results compared. A session
    /// read's `index_ids` argument becomes the call's named scope (read
    /// whole); `project_ids`/`member_availability` belong to membership
    /// (window) sessions and are not sent. Returns (MCP JSON-RPC reply, pipe result).
    async fn open_parity(
        &mut self,
        f: &Fixture,
        id: u64,
        tool: &str,
        arguments: &Value,
        label: &str,
    ) -> Result<(Value, Value)> {
        let (stripped, scope) = open_read_arguments(arguments, &self.transport.scope)?;
        let mut named = stripped.clone();
        named["index_ids"] = json!(scope.index_ids);
        let wire = self.call(id, tool.strip_prefix("airk_").unwrap_or(tool), named)?;
        ensure!(wire.get("error").is_none(), "{label}: MCP failed: {wire}");
        let piped = pipe_read(f, scope.open_selection(), tool, &stripped)
            .await?
            .map_err(|refused| anyhow::anyhow!("{label}: open pipe read refused: {refused}"))?;
        ensure!(
            wire["result"]["isError"] == piped["isError"],
            "{label}: MCP and pipe error states differ; MCP reply {}; complete replies: {}",
            wire["result"]
                .to_string()
                .chars()
                .take(2048)
                .collect::<String>(),
            save_replies(label, &json!({"mcp":wire,"pipe":piped}))?.display()
        );
        assert_parity(
            label,
            &tool_body(&piped, tool, &stripped)?,
            &tool_body(&wire["result"], tool, &stripped)?,
        )?;
        Ok((wire, piped))
    }
}

/// U110: the open-read form of combined-session read arguments: the
/// arguments without session selectors, and the named scope (`index_ids`
/// when given, else `default`).
fn open_read_arguments(arguments: &Value, default: &http::Scope) -> Result<(Value, http::Scope)> {
    let mut stripped = arguments.clone();
    let object = stripped
        .as_object_mut()
        .context("object arguments required")?;
    object.remove("project_ids");
    object.remove("member_availability");
    let scope = match object.remove("index_ids") {
        Some(ids) => http::Scope::indexes(
            ids.as_array()
                .context("index_ids must be an array")?
                .iter()
                .map(|id| id.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .context("index_ids must be strings")?,
        ),
        None => default.clone(),
    };
    Ok((stripped, scope))
}

#[tokio::test]
async fn actual_native_cli_and_mcp_preserve_shared_index_scope_and_complete_results() -> Result<()>
{
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let backend = f.resolve(&f.session).await?;
    let arguments = json!({"query":"boundaryneedle","mode":"lexical","limit":12,"max_bytes":8192,"verification":"off","response_mode":"full"});
    let native = native_read(backend.as_ref(), "airk_search", &arguments).await?;
    ensure!(
        native["isError"] != true,
        "native combined search failed: {native}"
    );
    let result = body(&native)?;
    let members = result["members"]
        .as_array()
        .context("member results absent")?;
    assert_eq!(
        members.len(),
        2,
        "shared index executed more than once: {result}"
    );
    let ranking = result["ranking"]
        .as_array()
        .context("qualified ranking absent")?;
    assert!(!ranking.is_empty(), "no native evidence: {result}");
    let mut allocated_count = 0;
    let mut allocated_bytes = 0;
    let mut returned_count = 0;
    let mut returned_bytes = 0;
    for member in members {
        allocated_count += member["allocation"]["limit"]
            .as_u64()
            .context("count allocation missing")?;
        allocated_bytes += member["allocation"]["max_bytes"]
            .as_u64()
            .context("byte allocation missing")?;
        let member_result = member_body(&result, member)?;
        for hit in member_result["hits"]
            .as_array()
            .with_context(|| format!("native search hits absent for member {member}"))?
        {
            returned_count += 1;
            returned_bytes += hit["text"]
                .as_str()
                .context("native hit text missing")?
                .len();
        }
    }
    assert_eq!(
        allocated_count, 12,
        "count budget multiplied by selected members"
    );
    assert_eq!(
        allocated_bytes, 8192,
        "text budget multiplied by selected members"
    );
    assert!(returned_count <= 12 && returned_bytes <= 8192);
    for ranked in ranking {
        let origin = serde_json::to_value(airk_core::combined_reads::materialize_ranked_origin(
            &result, ranked,
        )?)?;
        let relative = origin["source_relative_path"]
            .as_str()
            .context("source path missing")?;
        let index = &origin["index_id"];
        let expected = if relative.starts_with("api/") {
            &f.projects[0]
        } else {
            assert!(
                relative.starts_with("impl/"),
                "ungranted sibling/private source escaped: {ranked}"
            );
            &f.projects[1]
        };
        assert_eq!(origin["project_ids"], json!([expected]));
        assert!(index == &json!(f.indexes[0]) || index == &json!(f.indexes[1]));
        let pointer = ranked["result_pointer"]
            .as_str()
            .context("rank pointer absent")?;
        assert!(
            result.pointer(pointer).is_some(),
            "dangling qualified result pointer: {ranked}"
        );
    }
    let mut wrong_origin = result.clone();
    let ranked = &wrong_origin["ranking"][0];
    let vector = wrong_origin["publication_vector"]["members"]
        .as_array()
        .context("origin vector absent")?;
    let origin_member = vector
        .iter()
        .position(|entry| entry["index_id"] == ranked["index_id"])
        .context("origin member missing")?;
    let other_member = (origin_member + 1) % vector.len();
    wrong_origin["ranking"][0]["origin"]["publication_ref"]["member"] = json!(other_member);
    assert!(
        body(&wrong_origin).is_err(),
        "raw delivery digest accepted a changed origin reference"
    );
    assert!(
        airk_core::combined_reads::materialize_ranked_origin(
            &wrong_origin,
            &wrong_origin["ranking"][0]
        )
        .is_err(),
        "logical origin lookup accepted another index's publication"
    );
    if let Some(definitions) = result
        .pointer("/shared_values/values")
        .and_then(Value::as_object)
        && let Some(id) = definitions.keys().next()
    {
        let mut corrupt = result.clone();
        corrupt["shared_values"]["values"][id]["unrecognized_fixture_field"] = json!(true);
        assert!(
            body(&corrupt).is_err(),
            "raw delivery digest accepted a changed header definition"
        );
        assert!(
            logical_comparison_copy(&corrupt).is_err(),
            "logical comparison ignored an invalid template/hash before reconstructing fields"
        );
    }
    // Acquiring this native ownership pin proves the excluded member has no
    // live worker; querying selected members cannot inspect or start it.
    let untouched = ProjectLease::acquire(&f.control, f.registrations[2].binding)?;
    assert_eq!(untouched.identity().0, f.registrations[2].binding);
    drop(untouched);

    let cli = f.cli("airk_search", &arguments).await?;
    assert_parity("full CLI result", &body(&native)?, &body(&cli)?)?;
    assert_parity(
        "convenience CLI result",
        &body(&native)?,
        &body(&f.cli_search().await?)?,
    )?;
    // U110: tokenless HTTPS MCP names the session's indexes per call (read
    // whole); it takes the open read path, so it compares with the same open
    // read through the pipe.
    let mut mcp = McpSession::start(&f)?;
    let reply = mcp.call(2, "search", arguments.clone())?;
    ensure!(reply.get("error").is_none(), "MCP failed: {reply}");
    let open = mcp.pipe(&f, "search", &arguments).await?;
    assert_parity(
        "full MCP open result",
        &body(&open)?,
        &body(&reply["result"])?,
    )?;
    let catalog = mcp.request(3, "tools/list", json!({}))?;
    let tools = catalog["result"]["tools"]
        .as_array()
        .context("MCP tools missing")?;
    let search = tools
        .iter()
        .find(|tool| tool["name"] == "search")
        .context("search catalog missing")?;
    let local_catalog = serde_json::to_value(
        airk_mcp::open::local_catalog()
            .into_iter()
            .find(|tool| tool.name == "search")
            .context("local search catalog missing")?,
    )?;
    assert_eq!(
        search["inputSchema"], local_catalog["inputSchema"],
        "input catalog drift through HTTPS"
    );
    let checked = native_read(
        backend.as_ref(),
        "airk_search",
        &json!({
            "query":"boundaryneedle","mode":"lexical","limit":2,"max_bytes":1024,
            "verification":"require","verification_max_sources":4,
            "verification_max_bytes":4096,"verification_deadline_ms":2000,"response_mode":"full"
        }),
    )
    .await?;
    assert_ne!(
        checked["isError"], true,
        "combined source verification failed: {checked}"
    );
    let checked = body(&checked)?;
    airk_core::verification::validate_projection(&checked)?;
    let mut checked_sources = 0;
    let mut checked_bytes = 0;
    for member in checked["members"]
        .as_array()
        .context("checked members absent")?
    {
        let materialized = member_body(&checked, member)?;
        let receipt = &materialized["source_verification"];
        assert_eq!(
            receipt["requirements_satisfied"], true,
            "member check incomplete: {receipt}"
        );
        checked_sources += receipt["counts"]["checked_sources"]
            .as_u64()
            .context("source count absent")?;
        checked_bytes += receipt["counts"]["bytes_read"]
            .as_u64()
            .context("source byte count absent")?;
        for origin in receipt["origins"]
            .as_array()
            .context("verification origins absent")?
        {
            assert_eq!(
                origin["index_id"], member["index_id"],
                "receipt lost its native member identity"
            );
        }
        for source in receipt["sources"]
            .as_array()
            .context("verification sources absent")?
        {
            assert!(
                source["source_root_id"].as_str().is_some(),
                "source identity absent: {source}"
            );
            assert_eq!(
                source["project_ids"]
                    .as_array()
                    .context("source attribution absent")?
                    .len(),
                1,
                "shared membership over-attribution: {source}"
            );
        }
    }
    assert!(checked_sources > 0 && checked_sources <= 4 && checked_bytes <= 4096);
    backend.close();
    backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    Ok(())
}

#[tokio::test]
async fn combined_subset_refuses_ambiguous_followup_and_routes_one_index() -> Result<()> {
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let backend = f.resolve(&f.session).await?;
    // native read: deliberate ambiguity and invalid selectors run exactly once.
    for arguments in [
        json!({"path":"api/policy.md"}),
        json!({"path":"api/policy.md","index_ids":[f.indexes[0],f.indexes[1]]}),
    ] {
        let response = backend
            .call(
                "airk_file_info".into(),
                arguments.as_object().unwrap().clone(),
            )
            .await?;
        assert_eq!(
            response["isError"], true,
            "ambiguous origin was guessed: {response}"
        );
    }
    let narrowed = json!({"query":"boundaryneedle","mode":"lexical","project_ids":[f.projects[0]],"index_ids":[f.indexes[0]],"limit":4,"max_bytes":4096,"verification":"off","response_mode":"full"});
    let response = native_read(backend.as_ref(), "airk_search", &narrowed).await?;
    assert_ne!(
        response["isError"], true,
        "explicit narrow selection failed: {response}"
    );
    let value = body(&response)?;
    assert_eq!(
        value["members"].as_array().context("members absent")?.len(),
        1
    );
    // The one-index window over the same membership atoms is the singleton
    // native read of that member.
    let direct: std::sync::Arc<dyn airk_mcp::remote::RemoteTools> = pipe::PipeTools::window(
        &f.control,
        f.window_for(&SessionScope {
            project_ids: vec![f.projects[0]],
            index_ids: Some(vec![f.indexes[0]]),
        })
        .await?,
    );
    let mut direct_arguments = narrowed.clone();
    direct_arguments
        .as_object_mut()
        .unwrap()
        .remove("project_ids");
    direct_arguments
        .as_object_mut()
        .unwrap()
        .remove("index_ids");
    let direct_result = native_read(direct.as_ref(), "airk_search", &direct_arguments).await?;
    assert_parity(
        "one-index routing",
        &without_window_scope(member_body(&value, &value["members"][0])?),
        &without_window_scope(body(&direct_result)?),
    )?;
    direct.close();
    direct.shutdown().await?;
    let untouched = ProjectLease::acquire(&f.control, f.registrations[1].binding)?;
    drop(untouched);
    backend.close();
    backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    Ok(())
}

#[tokio::test]
async fn all_native_read_families_keep_complete_results_through_cli_and_mcp() -> Result<()> {
    let mut f = Fixture::new(true)?;
    f.session.project_ids = f.projects.clone();
    let rust = "// SPDX-License-Identifier: Apache-2.0\nuse std::fmt;\npub struct BoundarySession;\npub fn boundaryneedle() -> usize { std::mem::size_of::<BoundarySession>() }\n";
    for registration in f.registrations.iter().take(2) {
        fs::write(Path::new(&registration.root).join("api/types.rs"), rust)?;
        airk_kernel::Kernel::open_service_collection(
            &registration.root,
            Some(registration.data_root.clone().into()),
            &registration.collection,
        )?
        .index()?;
    }
    let _service = f.start().await?;
    let backend = f.resolve(&f.session).await?;
    let mut mcp = McpSession::start(&f)?;
    let cases = [
        (
            "airk_symbols",
            json!({"name":"BoundarySession","limit":4,"max_bytes":4096}),
            false,
        ),
        (
            "airk_code_facts",
            json!({"name":"BoundarySession","kind":"definition","limit":4,"max_bytes":4096}),
            false,
        ),
        (
            "airk_document_facts",
            json!({"kind":"section","limit":4,"max_bytes":4096}),
            false,
        ),
        (
            "airk_symbols",
            json!({"language":"klingon","limit":4,"max_bytes":4096}),
            false,
        ),
        (
            "airk_code_facts",
            json!({"kind":"bogus_kind","limit":4,"max_bytes":4096}),
            false,
        ),
        (
            "airk_document_facts",
            json!({"kind":"bogus_kind","limit":4,"max_bytes":4096}),
            false,
        ),
        (
            "airk_references",
            json!({"name":"boundaryneedle","limit":4,"max_bytes":4096}),
            false,
        ),
        (
            "airk_list_files",
            json!({"prefix":"api/","limit":4,"max_bytes":4096}),
            false,
        ),
        ("airk_status", json!({}), false),
        ("airk_diagnostics", json!({}), false),
        (
            "airk_capabilities",
            json!({"adapter":"markdown","rich":false}),
            false,
        ),
        ("airk_file_info", json!({"path":"api/types.rs"}), true),
        (
            "airk_read_file",
            json!({"path":"api/types.rs","start_line":1,"end_line":4,"max_bytes":1024}),
            true,
        ),
        (
            "airk_context",
            json!({"path":"api/types.rs","line":3,"before":1,"after":1,"max_bytes":1024}),
            true,
        ),
        (
            "airk_imports",
            json!({"path":"api/types.rs","limit":4,"max_bytes":4096}),
            true,
        ),
        (
            "airk_outline",
            json!({"path":"api/types.rs","limit":4,"max_bytes":4096}),
            true,
        ),
        (
            "airk_relationships",
            json!({"symbol":"file:api/types.rs","direction":"outgoing","limit":4,"max_bytes":4096}),
            true,
        ),
        (
            "airk_relationships",
            json!({"symbol":"file:api/types.rs","direction":"outgoing","limit":1,"max_bytes":4096}),
            true,
        ),
        (
            "airk_changes",
            json!({"selectors":[{"kind":"path","path":"api/types.rs"}],"limit":4,"max_bytes":4096}),
            true,
        ),
    ];
    let mut observations = Vec::new();
    let mut failures = Vec::new();
    for (offset, (tool, mut arguments, single)) in cases.into_iter().enumerate() {
        arguments["response_mode"] = json!("full");
        if !matches!(
            tool,
            "airk_status" | "airk_diagnostics" | "airk_capabilities"
        ) {
            arguments["verification"] = json!("off");
        }
        let expected_indexes = if matches!(tool, "airk_status" | "airk_diagnostics") {
            arguments["project_ids"] = json!([f.projects[2]]);
            arguments["index_ids"] = json!([f.indexes[2]]);
            vec![f.indexes[2]]
        } else if single {
            arguments["project_ids"] = json!([f.projects[0]]);
            arguments["index_ids"] = json!([f.indexes[0]]);
            vec![f.indexes[0]]
        } else {
            arguments["project_ids"] = json!(f.projects[..2]);
            arguments["index_ids"] = json!(f.indexes[..2]);
            f.indexes[..2].to_vec()
        };
        // Collect every family result before reporting a shared contract or
        // framing failure; no symptom is hidden by abandoning later families.
        let native = native_read(backend.as_ref(), tool, &arguments).await?;
        let cli = f.cli(tool, &arguments).await?;
        // U110: MCP names the same indexes per call and reads them whole on
        // the open path; it compares with the same open read through the pipe.
        let wire = match mcp
            .open_parity(
                &f,
                10 + offset as u64,
                tool,
                &arguments,
                &format!("{tool} MCP open"),
            )
            .await
        {
            Ok((wire, _)) => wire,
            Err(error) => {
                failures.push(format!("{tool}: MCP open read: {error:#}"));
                Value::Null
            }
        };
        let native_body =
            body(&native).with_context(|| format!("{tool}: native observed result"))?;
        let cli_body = body(&cli).with_context(|| format!("{tool}: CLI result"))?;
        let mcp_body = if wire.is_null() {
            Value::Null
        } else {
            body(&wire["result"]).with_context(|| format!("{tool}: MCP result"))?
        };
        // Equivalent error envelopes prove parity but cannot qualify this
        // positive read matrix. Every selected member must actually serve the
        // canonical handler; intentional member failures have separate tests.
        for (route, observed) in [("native", &native_body), ("CLI", &cli_body)] {
            let Some(members) = observed["members"].as_array() else {
                let path = save_replies(tool, &json!({"route":route,"raw_result":observed}))?;
                failures.push(format!(
                    "{tool}: {route} member results absent; complete reply: {}",
                    path.display()
                ));
                continue;
            };
            let actual_ids = members
                .iter()
                .map(|member| member["index_id"].as_str())
                .collect::<Option<std::collections::BTreeSet<_>>>();
            let expected_ids = expected_indexes
                .iter()
                .map(ToString::to_string)
                .collect::<std::collections::BTreeSet<_>>();
            if members.len() != expected_ids.len()
                || actual_ids.as_ref().map(|ids| {
                    ids.iter()
                        .map(|id| (*id).to_string())
                        .collect::<std::collections::BTreeSet<_>>()
                }) != Some(expected_ids)
            {
                let path = save_replies(tool, &json!({"route":route,"raw_result":observed}))?;
                failures.push(format!(
                    "{tool}: {route} selected member identities differ; complete reply: {}",
                    path.display()
                ));
            }
            for member in members {
                if member["outcome"] != "complete" || member["is_error"] != false {
                    let path = save_replies(
                        tool,
                        &json!({"route":route,"member":member,"raw_result":observed}),
                    )?;
                    failures.push(format!(
                        "{tool}: {route} positive member {} did not complete: outcome={}, is_error={}; complete reply: {}",
                        member["index_id"], member["outcome"], member["is_error"], path.display()
                    ));
                }
            }
        }
        if tool == "airk_capabilities" && native["isError"] != true {
            let public_bytes = airk_core::index_catalog::encoded_size(
                &native,
                3 * airk_core::MAX_RESPONSE_BYTES + 4096,
            )?;
            assert!(
                public_bytes > airk_core::MAX_RESPONSE_BYTES,
                "the actual capabilities fixture must exercise its public wrapper above the native canonical limit"
            );
            airk_core::index_catalog::encoded_size(&native_body, airk_core::MAX_RESPONSE_BYTES)?;
            // body() already authenticated the original canonical delivery
            // digest. The larger public wrapper must not trigger truncation or
            // change the complete materialized native capability fields.
            for member in 0..2 {
                let materialized =
                    airk_core::combined_reads::shared_values::materialize_combined_member_result(
                        &native_body,
                        f.indexes[member],
                    )
                    .with_context(|| format!("capabilities: materialize member {member}"))?;
                let direct: std::sync::Arc<dyn airk_mcp::remote::RemoteTools> =
                    pipe::PipeTools::window(
                        &f.control,
                        f.window_for(&SessionScope {
                            project_ids: if member == 0 {
                                f.projects[..2].to_vec()
                            } else {
                                vec![f.projects[0]]
                            },
                            index_ids: Some(vec![f.indexes[member]]),
                        })
                        .await
                        .with_context(|| format!("capabilities: one-index window {member}"))?,
                    );
                let canonical = native_read(
                    direct.as_ref(),
                    tool,
                    &json!({"adapter":"markdown","rich":false,"response_mode":"full"}),
                )
                .await?;
                if let Some(difference) = parity_difference(
                    &format!("materialized capabilities member {member}"),
                    &without_window_scope(materialized),
                    &without_window_scope(body(&canonical).with_context(|| {
                        format!("capabilities member {member}: one-index window observation")
                    })?),
                )? {
                    failures.push(difference);
                }
                direct.close();
                direct.shutdown().await?;
            }
        }
        if native["isError"] == true || cli["isError"] == true || wire["result"]["isError"] == true
        {
            let path = save_replies(
                tool,
                &json!({"native":native_body,"cli":cli_body,"mcp":mcp_body}),
            )?;
            failures.push(format!(
                "{tool}: native/CLI/MCP error receipt; complete replies: {}",
                path.display()
            ));
        }
        if let Some(difference) =
            parity_difference(&format!("{tool} CLI"), &native_body, &cli_body)?
        {
            failures.push(difference);
        }
        if native_body["members"].as_array().map(Vec::len) != Some(expected_indexes.len()) {
            let path = save_replies(tool, &native_body)?;
            failures.push(format!(
                "{tool}: selected member outcomes missing; complete reply: {}",
                path.display()
            ));
        }
        observations.push((tool, native_body));
    }
    // Whole-index observations remain unavailable through partial A/B scopes.
    // Exercise those refusals separately from the full-root positive matrix.
    // (U110: an MCP read names whole indexes, so it has no partial scope.)
    for tool in ["airk_status", "airk_diagnostics"] {
        let arguments = json!({"project_ids":f.projects[..2],"index_ids":[f.indexes[0]],"member_availability":"require","response_mode":"full"});
        let native = native_read(backend.as_ref(), tool, &arguments).await?;
        let cli = f.cli(tool, &arguments).await?;
        for (route, envelope) in [("native", &native), ("CLI", &cli)] {
            let observed = body(envelope)?;
            let members = observed["members"].as_array();
            let refused = members
                .and_then(|members| members.first())
                .map(|member| member_body(&observed, member))
                .transpose()?;
            if envelope["isError"] != true
                || members.map(Vec::len) != Some(1)
                || refused.as_ref().map(|member| &member["error"]["code"])
                    != Some(&json!("scoped_read_unavailable"))
            {
                let path = save_replies(tool, envelope)?;
                failures.push(format!("{tool}: {route} partial scope did not preserve whole-index refusal; complete reply: {}", path.display()));
            }
        }
        assert_parity(
            &format!("{tool} partial CLI"),
            &body(&native)?,
            &body(&cli)?,
        )?;
    }
    for tool in [
        "airk_symbols",
        "airk_code_facts",
        "airk_document_facts",
        "airk_references",
        "airk_list_files",
    ] {
        let result = &observations
            .iter()
            .find(|(name, _)| *name == tool)
            .context("family observation missing")?
            .1;
        if result["ranking"].as_array().is_none_or(Vec::is_empty) {
            let path = save_replies(tool, result)?;
            failures.push(format!(
                "{tool}: original fixture produced no qualified evidence; complete reply: {}",
                path.display()
            ));
        }
    }
    backend.close();
    backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    for registration in f.registrations.iter().take(2) {
        assert_eq!(
            fs::read_to_string(Path::new(&registration.root).join("api/types.rs"))?,
            rust
        );
    }
    ensure!(
        failures.is_empty(),
        "complete native family results failed:\n{}",
        failures.join("\n")
    );
    Ok(())
}

#[tokio::test]
async fn capabilities_plans_keep_complete_native_cli_and_mcp_results_above_read_frame() -> Result<()>
{
    const OUTPUT_BYTES: usize = 262144;
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let backend = f.resolve(&f.session).await?;
    let mut mcp = McpSession::start(&f)?;
    for count in [1usize, 2] {
        let selection = json!({
            "project_ids":f.projects[..2],
            "index_ids":f.indexes[..count],
            "member_availability":"require"
        });
        let mut arguments = selection.clone();
        arguments["response_mode"] = json!("full");
        eprintln!(
            "actual {count}-member capabilities/parse transport iteration: direct native read"
        );
        let direct = native_read(backend.as_ref(), "airk_capabilities", &arguments).await?;
        ensure!(
            direct["isError"] != true,
            "{count}-member direct capabilities failed: {direct}"
        );
        let expected =
            body(&direct).with_context(|| format!("{count}-member direct capability body"))?;
        eprintln!(
            "actual {count}-member direct capabilities with native delivery: {} bytes",
            serde_json::to_vec(&expected)?.len()
        );
        assert_eq!(
            expected["workbench"],
            airk_kernel::workbench::combined::operation_catalog(),
            "the full combined operation schemas must survive composition"
        );
        let request = json!({
            "schema":"airk.parse.request/1", "mode":"execute", "verification":"off",
            "project_ids":selection["project_ids"], "index_ids":selection["index_ids"],
            "member_availability":"require",
            "plan":{"steps":[{"id":"form","tool":"capabilities","args":{}}],
                    "output":{"view":"raw","max_bytes":OUTPUT_BYTES}}
        });
        let native = native_read(backend.as_ref(), "airk_parse", &request).await?;
        let cli = f.cli("airk_parse", &request).await?;
        // U110: MCP names the same indexes per call (read whole) on the open
        // path. Use the actual MCP alias used by client hosts, retaining its
        // JSON-RPC envelope when checking the existing one-MiB transport bound,
        // and compare its complete plan with the same open read through the pipe.
        let (wire, _) = mcp
            .open_parity(
                &f,
                9000 + count as u64,
                "parse",
                &request,
                &format!("{count}-member capabilities plan MCP open"),
            )
            .await?;
        airk_core::index_catalog::encoded_size(&wire, 1024 * 1024)?;
        let envelope = &wire["result"];
        ensure!(
            envelope["isError"] != true,
            "MCP open capabilities plan failed: {envelope}"
        );
        let parsed = parse_body(envelope, OUTPUT_BYTES)
            .with_context(|| format!("MCP: {count}-member open capabilities plan"))?;
        let text: Value = serde_json::from_str(
            envelope["content"][0]["text"]
                .as_str()
                .context("parse text mirror missing")?,
        )?;
        assert_eq!(
            text, parsed,
            "MCP: complete text and structured replies differ"
        );
        assert_eq!(parsed["status"], "complete", "MCP: {parsed}");
        assert_eq!(parsed["executed"], true);
        assert_eq!(parsed["truncated"], false);
        for (route, envelope) in [("native", &native), ("CLI", &cli)] {
            ensure!(
                envelope["isError"] != true,
                "{route} capabilities plan failed: {envelope}"
            );
            let parsed = parse_body(envelope, OUTPUT_BYTES)
                .with_context(|| format!("{route}: {count}-member capabilities plan"))?;
            let text: Value = serde_json::from_str(
                envelope["content"][0]["text"]
                    .as_str()
                    .context("parse text mirror missing")?,
            )?;
            assert_eq!(
                text, parsed,
                "{route}: complete text and structured replies differ"
            );
            let bytes = airk_core::index_catalog::encoded_size(&parsed, OUTPUT_BYTES)?;
            ensure!(
                bytes > airk_core::MAX_RESPONSE_BYTES,
                "{route}: fixture did not exercise a plan larger than a native read frame"
            );
            assert_eq!(parsed["status"], "complete", "{route}: {parsed}");
            assert_eq!(parsed["schema"], airk_core::workbench::RESULT_SCHEMA);
            assert_eq!(parsed["executed"], true);
            assert_eq!(parsed["truncated"], false);
            assert_eq!(parsed["calls"], 1);
            assert_eq!(parsed["requires_input"], json!([]));
            assert_eq!(parsed["completeness"], json!([]));
            let steps = parsed["steps"].as_array().context("parse steps missing")?;
            assert_eq!(steps.len(), 1);
            assert_eq!(steps[0]["status"], "complete");
            assert_eq!(steps[0]["error"], Value::Null);
            let combined = body(&steps[0]["result"])?;
            assert_parity(
                &format!("{route}: {count}-member capabilities plan"),
                &expected,
                &combined,
            )?;
            assert_eq!(combined["truncated"], false);
            assert_eq!(combined["partial"], false);
            assert_eq!(combined["workbench"], expected["workbench"]);
            let members = combined["members"]
                .as_array()
                .context("capability members missing")?;
            assert_eq!(members.len(), count);
            for (position, member) in members.iter().enumerate() {
                assert_eq!(member["outcome"], "complete");
                assert_eq!(member["is_error"], false);
                let actual = member_body(&combined, member)?;
                let expected_member = member_body(&expected, &expected["members"][position])?;
                assert_parity(
                    &format!("{route}: complete capability member {position}"),
                    &expected_member,
                    &actual,
                )?;
            }
        }
    }
    backend.close();
    backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    Ok(())
}

#[tokio::test]
async fn actual_query_scope_discovery_keeps_three_views_and_continuations_in_parity() -> Result<()>
{
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let backend = f.resolve(&f.session).await?;
    let mut mcp = McpSession::start(&f)?;
    let mut id = 1000;
    for (view, mut arguments, expected_total) in [
        (
            "projects",
            json!({"view":"projects","limit":1,"max_bytes":16384}),
            2,
        ),
        (
            "indexes",
            json!({"view":"indexes","project_id":f.projects[0],"limit":1,"max_bytes":16384}),
            2,
        ),
        (
            "sources",
            json!({"view":"sources","project_id":f.projects[0],"index_id":f.indexes[0],"limit":1,"max_bytes":16384}),
            1,
        ),
    ] {
        // U110: MCP names whole indexes per call on the open path; each of
        // its pages (its own continuation) compares with the same open read
        // through the pipe.
        let mut open_arguments = arguments.clone();
        for page in 0..4 {
            let (wire, _) = mcp
                .open_parity(
                    &f,
                    id,
                    "query_scope",
                    &open_arguments,
                    &format!("query_scope {view} MCP open page {page}"),
                )
                .await?;
            id += 1;
            match body(&wire["result"])?["cursor"].as_str() {
                Some(cursor) => open_arguments["cursor"] = json!(cursor),
                None => break,
            }
        }
        let mut items = Vec::new();
        let mut cursors = std::collections::BTreeSet::new();
        loop {
            let native = native_read(backend.as_ref(), "airk_query_scope", &arguments).await?;
            let cli = f.cli("airk_query_scope", &arguments).await?;
            ensure!(
                native["isError"] != true && cli["isError"] != true,
                "{view}: scoped discovery returned an error; complete replies: {}",
                save_replies(view, &json!({"native":native,"cli":cli}))?.display()
            );
            let result = body(&native)?;
            assert_parity(&format!("query_scope {view} CLI"), &result, &body(&cli)?)?;
            assert_eq!(result["schema"], "airk.query-scope/1");
            assert_eq!(result["count"], expected_total);
            assert_eq!(result["count_precision"], "exact_selected_scope");
            assert!(
                !result.to_string().contains(&f.projects[2].to_string()),
                "discovery exposed an unselected Project"
            );
            assert!(
                !result.to_string().contains(&f.indexes[2].to_string()),
                "discovery exposed an unselected index"
            );
            let page = result["items"]
                .as_array()
                .context("discovery items absent")?;
            assert_eq!(page.len(), 1, "discovery page limit was not preserved");
            items.extend(page.iter().cloned());
            match result["cursor"].as_str() {
                Some(cursor) => {
                    assert_eq!(result["truncated"], true);
                    ensure!(
                        cursors.insert(cursor.to_owned()) && cursors.len() <= 2,
                        "discovery cursor did not make bounded progress"
                    );
                    arguments["cursor"] = json!(cursor);
                }
                None => {
                    assert_eq!(result["truncated"], false);
                    break;
                }
            }
        }
        assert_eq!(items.len(), expected_total as usize);
        assert_eq!(
            items
                .iter()
                .map(Value::to_string)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            items.len(),
            "discovery repeated a previous page item"
        );
    }
    // native read: deliberate out-of-session discovery is dispatched once.
    let denied = backend
        .call(
            "airk_query_scope".into(),
            json!({"view":"indexes","project_id":f.projects[2]})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await?;
    assert_eq!(
        denied["isError"], true,
        "discovery widened the frozen connection selection"
    );
    for registration in &f.registrations {
        let untouched = ProjectLease::acquire(&f.control, registration.binding)?;
        drop(untouched);
    }
    backend.close();
    backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    Ok(())
}

#[tokio::test]
async fn recommendations_keep_native_cli_mcp_parity_and_execute_copied_examples() -> Result<()> {
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let backend = f.resolve(&f.session).await?;
    let mut mcp = McpSession::start(&f)?;
    let mut id = 4000;
    let mut subset_observations = Vec::new();
    for selection in [
        None,
        Some(
            json!({"project_ids":f.session.project_ids,"index_ids":[f.indexes[1]],"member_availability":"require"}),
        ),
    ] {
        let mut args =
            json!({"view":"recommendations","situation":"focused_task","max_bytes":65536});
        if let Some(selection) = selection {
            args["selection"] = selection;
        }
        let native = native_read(backend.as_ref(), "airk_query_scope", &args).await?;
        let cli = f.cli("airk_query_scope", &args).await?;
        ensure!(
            native["isError"] != true && cli["isError"] != true,
            "advice failed: {native} {cli}"
        );
        // U110: MCP names whole indexes per call on the open path; its advice
        // compares with the same open read through the pipe.
        mcp.open_parity(&f, id, "query_scope", &args, "recommendations MCP open")
            .await?;
        id += 1;
        let result = body(&native)?;
        assert_eq!(result["schema"], "airk.recommendations/1");
        let instructions = result["instructions"]["text"]
            .as_str()
            .context("scope instructions absent")?;
        ensure!(
            instructions.contains("separate native management review"),
            "missing reviewed management guidance: {instructions}"
        );
        ensure!(
            !instructions.contains("pending P9/P10"),
            "obsolete management guidance: {instructions}"
        );
        assert_parity("recommendations CLI", &result, &body(&cli)?)?;
        let plan = json!({"schema":"airk.parse.request/1","mode":"execute","verification":"off","plan":{"schema":"airk.parse.plan/1","steps":[{"id":"advice","tool":"query_scope","args":args}]}});
        // The catalog session's Workbench plan runs through the combined CLI;
        // MCP's open plan compares with the open pipe.
        mcp.open_parity(
            &f,
            id,
            "parse",
            &plan,
            "recommendations Workbench plan MCP open",
        )
        .await?;
        id += 1;
        let parsed = f.cli("airk_parse", &plan).await?;
        ensure!(
            parsed["isError"] != true,
            "Workbench advisory plan failed: {parsed}"
        );
        let parsed_body = body(&parsed)?;
        assert_parity(
            "recommendations Workbench plan",
            &result,
            &body(&parsed_body["steps"][0]["result"])?,
        )?;
        if let Some(choice) = args.get("selection") {
            let slash = json!({"schema":"airk.parse.request/1","mode":"execute","verification":"off","text":format!("/query_scope recommendations --selection '{choice}' --situation focused_task --bytes 65536")});
            let cli = f.cli("airk_parse", &slash).await?;
            mcp.open_parity(&f, id, "parse", &slash, "recommendations slash MCP open")
                .await?;
            id += 1;
            ensure!(cli["isError"] != true, "CLI slash advice failed: {cli}");
            let plan = body(&cli)?;
            assert_parity(
                "recommendations slash CLI",
                &result,
                &body(&plan["steps"][0]["result"])?,
            )?;
        }
        assert_eq!(result["limits"]["evidence_queries"], 0);
        assert_eq!(
            result["instructions"]["examples"][1]["arguments"]["member_availability"],
            result["selected"]["member_availability"]
        );
        assert!(!result.to_string().contains(&f.indexes[2].to_string()));
        // Metadata-only advice must not acquire project worker leases.
        if id == 4002 {
            for registration in &f.registrations {
                drop(ProjectLease::acquire(&f.control, registration.binding)?);
            }
        }
        for example in result["instructions"]["examples"]
            .as_array()
            .context("scope examples absent")?
        {
            let name = example["name"]
                .as_str()
                .context("scope example tool absent")?;
            // U110: the copied examples name the catalog session's selectors;
            // they execute through the catalog session's CLI transport.
            let reply = f.cli(name, &example["arguments"]).await?;
            ensure!(
                reply["isError"] != true,
                "copied example rejected by actual transport: {reply}"
            );
        }
        // Original fixture judgments predate candidate output: this literal
        // matches permitted policy evidence, including in the one-index subset.
        let mut task = result["instructions"]["examples"][1]["arguments"].clone();
        task["query"] = json!("boundaryneedle");
        let reply = f.cli("search", &task).await?;
        ensure!(
            reply["isError"] != true,
            "recommended task scope failed: {reply}"
        );
        let evidence = body(&reply)?;
        assert_eq!(
            evidence["selection"], result["selected"],
            "advisory and retrieval scopes diverged"
        );
        let members = evidence["members"]
            .as_array()
            .context("selected members absent")?;
        let ranking = evidence["ranking"]
            .as_array()
            .context("selected evidence absent")?;
        ensure!(
            !ranking.is_empty(),
            "useful fixture evidence absent from recommended scope: {evidence}"
        );
        assert_eq!(
            members.len(),
            result["selected"]["index_ids"].as_array().unwrap().len()
        );
        subset_observations.push(json!({"selected":result["selected"],"omitted_index_ids":result["omitted_index_ids"],"unique_members_read":members.len(),"returned_evidence_records":ranking.len(),"canonical_response_bytes":serde_json::to_vec(&evidence)?.len(),"mode":"lexical","verification":"off"}));
    }
    save_replies(
        "P8 useful subset observations",
        &json!({"scope":"Original fictional positive lexical fixture; selected and all available scope, not a general recall/latency/physical-I/O claim","cases":subset_observations}),
    )?;
    // Lifecycle/removal advice uses the same complete canonical body in all
    // transports. It offers a separate review and never prepares a mutation.
    for (situation, action) in [
        ("different_lifecycles", "review_split"),
        ("never_index_path", "removal_pending"),
    ] {
        let args = json!({"view":"recommendations","situation":situation,"max_bytes":65536});
        let native = native_read(backend.as_ref(), "airk_query_scope", &args).await?;
        let cli = f.cli("airk_query_scope", &args).await?;
        ensure!(
            native["isError"] != true && cli["isError"] != true,
            "management advice failed: {native} {cli}"
        );
        mcp.open_parity(
            &f,
            id,
            "query_scope",
            &args,
            "management recommendations MCP open",
        )
        .await?;
        id += 1;
        let result = body(&native)?;
        assert_eq!(result["cards"][0]["action"], action);
        let tradeoff = result["cards"][0]["tradeoff"]
            .as_str()
            .context("management tradeoff absent")?;
        ensure!(
            tradeoff.contains("separate native review")
                && tradeoff.contains("Advice does not prepare or apply"),
            "management review boundaries absent: {tradeoff}"
        );
        assert_parity("management recommendations CLI", &result, &body(&cli)?)?;
        let plan = json!({"schema":"airk.parse.request/1","mode":"execute","verification":"off","plan":{"schema":"airk.parse.plan/1","steps":[{"id":"advice","tool":"query_scope","args":args}]}});
        mcp.open_parity(
            &f,
            id,
            "parse",
            &plan,
            "management recommendations Workbench plan MCP open",
        )
        .await?;
        id += 1;
        let parsed = f.cli("airk_parse", &plan).await?;
        ensure!(
            parsed["isError"] != true,
            "management advisory plan failed: {parsed}"
        );
        let parsed_body = body(&parsed)?;
        assert_parity(
            "management recommendations Workbench plan",
            &result,
            &body(&parsed_body["steps"][0]["result"])?,
        )?;
        assert_eq!(result["limits"]["evidence_queries"], 0);
        assert_eq!(result["limits"]["source_scans"], 0);
        assert_eq!(result["limits"]["worker_probes"], 0);
    }
    for args in [
        json!({"view":"recommendations","selection":{"project_ids":[f.projects[2]],"index_ids":[f.indexes[2]]}}),
        json!({"view":"recommendations","max_bytes":1}),
        json!({"view":"recommendations","cursor":"stale"}),
    ] {
        // native read: deliberate invalid/budget calls must be observed once.
        let reply = backend
            .call("airk_query_scope".into(), args.as_object().unwrap().clone())
            .await?;
        ensure!(
            reply["isError"] == true,
            "invalid advice was accepted: {reply}"
        );
    }
    backend.close();
    backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    Ok(())
}

#[tokio::test]
async fn report_keeps_successful_members_and_require_preserves_complete_failure() -> Result<()> {
    let f = Fixture::new(false)?;
    let _service = f.start().await?;
    let backend = f.resolve(&f.session).await?;
    let arguments = json!({"query":"boundaryneedle","mode":"lexical","project_ids":f.session.project_ids,"limit":8,"max_bytes":4096,"verification":"off","response_mode":"full","member_availability":"report"});
    let report = native_read(backend.as_ref(), "airk_search", &arguments).await?;
    let report_body = body(&report)?;
    assert_ne!(
        report["isError"], true,
        "report discarded a usable member: {report}"
    );
    assert_eq!(
        report_body["members"]
            .as_array()
            .context("members absent")?
            .len(),
        2
    );
    assert!(
        !report_body["ranking"]
            .as_array()
            .context("ranking absent")?
            .is_empty(),
        "usable evidence absent: {report}"
    );
    let mut required = arguments;
    required["member_availability"] = json!("require");
    let require = native_read(backend.as_ref(), "airk_search", &required).await?;
    assert_eq!(
        require["isError"], true,
        "require hid member failure: {require}"
    );
    let require_body = body(&require)?;
    assert_eq!(
        require_body["members"]
            .as_array()
            .context("require dropped member outcomes")?
            .len(),
        2
    );
    backend.close();
    backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    Ok(())
}

#[tokio::test]
async fn one_two_four_and_thirty_two_member_scopes_share_one_count_budget() -> Result<()> {
    let mut f = Fixture::with_indexes(true, 32)?;
    f.session.project_ids = f.projects.clone();
    let _service = f.start().await?;
    let backend = f.resolve(&f.session).await?;
    // Warm the one actually admitted worker through the same supported route.
    // The larger selections then measure budget/scope correctness, not a claim
    // of successful execution or latency for thirty-two dispatched children.
    for count in [1, 2, 4, 32] {
        let arguments = json!({
            "query":"boundaryneedle","mode":"lexical","project_ids":f.projects,
            "index_ids":f.indexes[..count],"limit":1,"max_bytes":1024,
            "verification":"off","response_mode":"full"
        });
        let response = native_read(backend.as_ref(), "airk_search", &arguments).await?;
        assert_ne!(
            response["isError"], true,
            "bounded {count}-member read failed: {response}"
        );
        let value = body(&response)?;
        let members = value["members"]
            .as_array()
            .context("member outcomes absent")?;
        assert_eq!(
            members.len(),
            count,
            "member omitted from explicit subset: {value}"
        );
        assert_eq!(
            members
                .iter()
                .filter(|member| member["outcome"] == "complete")
                .count(),
            1,
            "count budget was multiplied or readable member failed: {value}"
        );
        assert_eq!(
            members
                .iter()
                .filter(|member| member["outcome"] == "unqueried")
                .count(),
            count - 1,
            "zero count allocation was hidden: {value}"
        );
        assert_eq!(
            members
                .iter()
                .map(|member| member["allocation"]["limit"].as_u64().unwrap_or(u64::MAX))
                .sum::<u64>(),
            1
        );
        assert_eq!(
            members
                .iter()
                .map(|member| member["allocation"]["max_bytes"]
                    .as_u64()
                    .unwrap_or(u64::MAX))
                .sum::<u64>(),
            1024
        );
        assert_eq!(
            value["ranking"].as_array().context("ranking absent")?.len(),
            1
        );
        assert_eq!(
            value["publication_vector"]["members"]
                .as_array()
                .context("publication vector absent")?
                .len(),
            count
        );
        assert!(
            value.get("publication").is_none(),
            "combined read invented a scalar publication"
        );
    }
    for registration in f.registrations.iter().skip(1) {
        let untouched = ProjectLease::acquire(&f.control, registration.binding)?;
        drop(untouched);
    }
    backend.close();
    backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    Ok(())
}

#[cfg(feature = "local-inference")]
async fn wait_fixture_job(f: &Fixture, id: Sha256Digest) -> Result<airk_core::service::JobRecord> {
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        let response =
            airk_cli::service_runtime::request(&f.control, ControlMethod::Job { id }).await?;
        let ControlResponse::Job { job } = response else {
            anyhow::bail!(
                "native fixture job observation refused: {}",
                serde_json::to_string(&response)?
            );
        };
        if job.outcome.is_some() {
            return Ok(job);
        }
        ensure!(
            Instant::now() < deadline,
            "native fixture job remained pending: {job:?}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[cfg(feature = "local-inference")]
async fn fixture_job(f: &Fixture, index: usize, kind: airk_core::service::JobKind) -> Result<()> {
    let prepared = f
        .native_cli(
            index,
            vec![
                "service".into(),
                "requests".into(),
                "--runtime-dir".into(),
                f.control
                    .to_str()
                    .context("control path must be UTF-8")?
                    .into(),
                "prepare".into(),
                "--project".into(),
                f.registrations[index].binding.to_string(),
                "--kind".into(),
                kind.code().into(),
            ],
        )
        .await?;
    let nonce = prepared["prepared"]
        .as_str()
        .context("native prepared request key missing")?;
    let response = f
        .native_cli(
            index,
            vec![
                "service".into(),
                "requests".into(),
                "--runtime-dir".into(),
                f.control
                    .to_str()
                    .context("control path must be UTF-8")?
                    .into(),
                "retry".into(),
                nonce.into(),
            ],
        )
        .await?;
    let response: ControlResponse = serde_json::from_value(response)?;
    let ControlResponse::Job { job } = response else {
        anyhow::bail!(
            "native fixture job was not acknowledged: {}",
            serde_json::to_string(&response)?
        );
    };
    let job = wait_fixture_job(f, job.id).await?;
    ensure!(
        matches!(
            job.outcome,
            Some(airk_core::service::JobOutcome::Succeeded { .. })
        ),
        "native fixture job failed: {job:?}"
    );
    Ok(())
}

#[cfg(feature = "local-inference")]
async fn fixture_pause(f: &Fixture, index: usize, paused: bool) -> Result<()> {
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::Pause {
            project: f.registrations[index].binding,
            paused,
        },
    )
    .await?;
    ensure!(
        matches!(&response,ControlResponse::Registered { project } if project.paused==paused),
        "native fixture pause was not acknowledged: {}",
        serde_json::to_string(&response)?
    );
    Ok(())
}

#[cfg(feature = "local-inference")]
async fn fixture_install_model(f: &Fixture, index: usize, model: &Path) -> Result<()> {
    let draft = f.owner.path().join(format!("install-model-{index}.json"));
    fs::write(
        &draft,
        serde_json::to_vec(&json!({"operation":"install_model","directory":model}))?,
    )?;
    let response = f
        .native_cli(
            index,
            vec![
                "configure".into(),
                "--file".into(),
                draft
                    .to_str()
                    .context("model draft path must be UTF-8")?
                    .into(),
            ],
        )
        .await?;
    let response: ControlResponse = serde_json::from_value(response)?;
    let ControlResponse::Job { job } = response else {
        anyhow::bail!(
            "native model configuration not acknowledged: {}",
            serde_json::to_string(&response)?
        );
    };
    let job = wait_fixture_job(f, job.id).await?;
    ensure!(
        matches!(
            job.outcome,
            Some(airk_core::service::JobOutcome::Succeeded { .. })
        ),
        "native model configuration failed: {job:?}"
    );
    Ok(())
}

#[cfg(feature = "local-inference")]
#[tokio::test]
async fn actual_helper_outage_keeps_fresh_selected_source_publications_under_fanout() -> Result<()>
{
    use airk_core::readiness::DefaultSearchPolicy;
    use airk_core::service::JobKind;
    let f = Fixture::with_inference()?;
    // Configure the saved policy before the native owner starts. Provisioning
    // below still goes through actual service configuration and the in-process provider.
    for registration in f.registrations.iter().take(2) {
        let kernel = airk_kernel::Kernel::open_service_collection(
            &registration.root,
            Some(registration.data_root.clone().into()),
            &registration.collection,
        )?;
        kernel.configure_vectors(true)?;
        kernel.set_default_semantic_search_policy(DefaultSearchPolicy::PartialHybrid)?;
    }
    let model = f.owner.path().join("outage-model");
    fs::create_dir(&model)?;
    fs::write(
        model.join("vocabulary.json"),
        br#"{"boundaryneedle":[1.0,0.0],"freshpublicationmarker":[1.0,0.0]}"#,
    )?;
    fs::write(
        model.join("model.json"),
        serde_json::to_vec(&json!({
            "schema":1,"name":"original-combined-outage-fixture","license":"Apache-2.0","dimensions":2,
            "sha256":airk_cli::runtime_pair::digest_file(&model.join("vocabulary.json"))?
        }))?,
    )?;
    let _service = f.start().await?;
    for index in 0..2 {
        let response = airk_cli::service_runtime::request(
            &f.control,
            ControlMethod::ConfigureWatch {
                project: f.registrations[index].binding,
                enabled: false,
            },
        )
        .await?;
        ensure!(
            matches!(response, ControlResponse::Watch { .. }),
            "native fixture watcher disable failed"
        );
        fixture_pause(&f, index, false).await?;
        fixture_install_model(&f, index, &model).await?;
        fixture_job(&f, index, JobKind::Index).await?;
        fixture_job(&f, index, JobKind::Vectors).await?;
        fixture_pause(&f, index, true).await?;
    }
    let backend = f.resolve(&f.session).await?;
    let vector_args = json!({"query":"boundaryneedle","mode":"vector","project_ids":f.session.project_ids,"limit":4,"max_bytes":2048,
        "verification":"off","response_mode":"full","member_availability":"require"});
    let warm = native_read(backend.as_ref(), "airk_search", &vector_args).await?;
    assert_ne!(
        warm["isError"], true,
        "in-process provider never served the selected members: {warm}"
    );
    let warm = body(&warm)?;
    assert!(
        warm["ranking"]
            .as_array()
            .context("warm ranking absent")?
            .len()
            >= 2,
        "native vectors were not published: {warm}"
    );
    for member in warm["members"].as_array().context("warm members absent")? {
        let result = member_body(&warm, member)?;
        assert_eq!(result["execution"]["served_mode"], "vector");
        assert!(
            !result["hits"]
                .as_array()
                .context("warm member hits absent")?
                .is_empty(),
            "one selected member had no native vector evidence before helper stop"
        );
    }
    let helper =
        airk_cli::service_runtime::request(&f.control, ControlMethod::EmbeddingStatus).await?;
    let ControlResponse::EmbeddingStatus {
        available: true,
        worker: Some(worker),
        ..
    } = helper
    else {
        anyhow::bail!(
            "in-process provider was unavailable before deliberate stop: {}",
            serde_json::to_string(&helper)?
        );
    };
    let stopped = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::StopEmbedding {
            worker,
            force: true,
        },
    )
    .await?;
    // Stop is submitted exactly once. In-process FFI cannot be killed: an
    // accepted drain can outlive the acknowledgment budget. Observe its owned
    // retirement to completion without replaying an uncertain mutation.
    ensure!(
        matches!(&stopped,ControlResponse::EmbeddingStopped {worker:actual} if *actual==worker)
            || matches!(&stopped,ControlResponse::Failure{code} if code=="embedding_stop_outcome_unknown"),
        "exact provider stop was refused: {}",
        serde_json::to_string(&stopped)?
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let observation = tokio::time::timeout_at(
            deadline,
            airk_cli::service_runtime::request(&f.control, ControlMethod::EmbeddingStatus),
        )
        .await??;
        match &observation {
            ControlResponse::EmbeddingStatus {
                available: false,
                worker: None,
                process_id: None,
                ..
            } => break,
            ControlResponse::EmbeddingStatus {
                worker: Some(current),
                ..
            } if *current == worker => {}
            ControlResponse::Failure { code } if code == "embedding_observation_busy" => {}
            _ => anyhow::bail!(
                "provider retirement changed ownership: {}",
                serde_json::to_string(&observation)?
            ),
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "exact provider retirement did not settle: {}",
            serde_json::to_string(&observation)?
        );
        tokio::time::sleep_until(
            (tokio::time::Instant::now() + Duration::from_millis(25)).min(deadline),
        )
        .await;
    }
    let default_args = json!({"query":"boundaryneedle","project_ids":f.session.project_ids,"limit":4,"max_bytes":2048,
        "verification":"off","response_mode":"full","member_availability":"require"});
    let before = native_read(backend.as_ref(), "airk_search", &default_args).await?;
    assert_ne!(
        before["isError"], true,
        "saved partial-hybrid outage degraded to a member error: {before}"
    );
    let before = body(&before)?;
    for member in before["members"]
        .as_array()
        .context("outage members absent")?
    {
        let result = member_body(&before, member)?;
        assert_eq!(
            result["execution"],
            json!({"version":1,"requested_mode":"default","served_mode":"lexical",
            "incomplete":true,"incomplete_reason":"helper_unavailable"}),
            "outage receipt lost its native cause"
        );
    }
    let source = Path::new(&f.registrations[0].root).join("api/policy.md");
    let original = fs::read(&source)?;
    let changed = b"# boundaryneedle freshpublicationmarker\nNew captured fixture source under helper outage.\n";
    fs::write(&source, changed)?;
    fixture_pause(&f, 0, false).await?;
    fixture_job(&f, 0, JobKind::Index).await?;
    fixture_pause(&f, 0, true).await?;
    let after = native_read(backend.as_ref(), "airk_search", &default_args).await?;
    assert_ne!(
        after["isError"], true,
        "helper outage hid fresh lexical publication: {after}"
    );
    let after = body(&after)?;
    let before_vector = before["publication_vector"]["members"]
        .as_array()
        .context("before vector absent")?;
    let after_vector = after["publication_vector"]["members"]
        .as_array()
        .context("after vector absent")?;
    assert_ne!(
        before_vector[0]["publication"]["token"], after_vector[0]["publication"]["token"],
        "edited selected index did not publish fresh evidence"
    );
    assert_eq!(
        before_vector[1]["publication"]["token"], after_vector[1]["publication"]["token"],
        "unmodified selected member was republished"
    );
    let mut saw_changed = false;
    for member in after["members"]
        .as_array()
        .context("fresh members absent")?
    {
        let result = member_body(&after, member)?;
        assert_eq!(result["execution"]["requested_mode"], "default");
        assert_eq!(result["execution"]["served_mode"], "lexical");
        assert_eq!(
            result["execution"]["incomplete_reason"],
            "helper_unavailable"
        );
        for hit in result["hits"].as_array().context("fresh hits absent")? {
            if member["index_id"] == json!(f.indexes[0])
                && hit["citation"]["path"] == "api/policy.md"
            {
                assert_eq!(
                    hit["citation"]["content_hash"],
                    airk_cli::runtime_pair::digest_file(&source)?
                );
                assert!(
                    hit["text"]
                        .as_str()
                        .context("fresh text absent")?
                        .contains("freshpublicationmarker")
                );
                saw_changed = true;
            }
        }
    }
    assert!(
        saw_changed,
        "fresh selected source disappeared from the combined ranking: {after}"
    );
    assert_eq!(
        fs::read(&source)?,
        changed,
        "a read changed the intentionally edited fixture source"
    );
    // native read: strict semantic requests execute once after the deliberate
    // helper stop; the typed unavailable result never authorizes query replay.
    let strict = backend
        .call(
            "airk_search".into(),
            vector_args.as_object().unwrap().clone(),
        )
        .await?;
    assert_eq!(
        strict["isError"], true,
        "explicit semantic mode silently degraded: {strict}"
    );
    let strict_body = body(&strict)?;
    for member in strict_body["members"]
        .as_array()
        .context("strict member failures absent")?
    {
        assert_eq!(member["outcome"], "failed");
        assert_eq!(
            member_body(&strict_body, member)?["error"]["code"],
            "helper_unavailable"
        );
    }
    let untouched = ProjectLease::acquire(&f.control, f.registrations[2].binding)?;
    drop(untouched);
    backend.close();
    backend.shutdown().await?;
    drop(_service);
    // Restore only the explicitly modified original fixture after all owners
    // drain. This is fixture cleanup, not an indexed refresh or rollback.
    fs::write(&source, original)?;
    f.assert_sources_unchanged()?;
    Ok(())
}

#[test]
fn comparison_normalizes_only_validated_member_verification_elapsed_windows() -> Result<()> {
    use airk_core::verification::{
        VerificationBudget, VerificationCounts, VerificationLimits, VerificationMode,
        VerificationReceipt,
    };
    let index = IndexId::from_identity(Incarnation::from_bytes([71; 16]));
    let allocation = airk_core::combined_reads::MemberAllocation {
        index_id: index,
        limit: 1,
        max_bytes: 4096,
        deadline_ms: 30_000,
        verification_max_sources: 8,
        verification_max_bytes: 16 << 20,
        verification_deadline_ms: 2000,
    };
    let receipt = VerificationReceipt {
        schema: "airk.saved-source-verification/1".into(),
        method: "sha256-full-saved-source/1".into(),
        mode: VerificationMode::Report,
        status: "verified".into(),
        scope_digest: Sha256Digest::from_bytes([72; 32]),
        selection_digest: Sha256Digest::from_bytes([73; 32]),
        observed_from_unix_ms: 1,
        observed_to_unix_ms: 2,
        publication_fence: "current".into(),
        inventory_complete: true,
        all_references_accounted: true,
        source_rows_omitted: 0,
        requirements_satisfied: Some(true),
        counts: VerificationCounts::default(),
        limits: VerificationLimits::from(VerificationBudget {
            mode: VerificationMode::Report,
            max_sources: 8,
            max_bytes: 16 << 20,
            deadline_ms: 2000,
        }),
        origins: vec![],
        sources: vec![],
        references: vec![],
        qualifications: vec![],
        error: None,
    };
    let raw = json!({"schema":"airk.combined-read/1", "members":[{
    "index_id":index,"allocation":allocation,"result":{
        "source_verification":receipt,
        "citation":{"path":"allowed.rs","sha256":Sha256Digest::from_bytes([74;32])},
        "quota":{"used_bytes":7,"reserved_bytes":11},
        "deadline_ms":17
    }}]});
    let unchanged = raw.clone();
    let mut slower = raw.clone();
    slower["members"][0]["result"]["source_verification"]["limits"]["deadline_ms"] = json!(1999);
    slower["members"][0]["result"]["source_verification"]["limits"]["effective_deadline_ms"] =
        json!(1998);
    let mut expected = raw.clone();
    normalize_member_verification_timing(&mut expected)?;
    normalize_member_verification_timing(&mut slower)?;
    assert_eq!(expected, slower);
    assert_eq!(raw, unchanged, "actual received reply was changed");
    for (pointer, invalid) in [
        (
            "/members/0/result/source_verification/limits/deadline_ms",
            json!(2001),
        ),
        (
            "/members/0/result/source_verification/limits/effective_deadline_ms",
            json!(2001),
        ),
        (
            "/members/0/result/source_verification/limits/max_sources",
            json!(9),
        ),
        (
            "/members/0/result/source_verification/limits/max_bytes",
            json!((16 << 20) + 1),
        ),
        (
            "/members/0/allocation/index_id",
            json!(IndexId::from_identity(Incarnation::from_bytes([75; 16]))),
        ),
        (
            "/members/0/allocation/verification_deadline_ms",
            json!(1999),
        ),
    ] {
        let mut crossed = raw.clone();
        *crossed
            .pointer_mut(pointer)
            .context("negative fixture pointer absent")? = invalid;
        assert!(
            normalize_member_verification_timing(&mut crossed).is_err(),
            "accepted {pointer}"
        );
    }
    // Source identities, outcomes, fixed allowances, quota and unrelated deadline
    // fields are semantic observations; valid timing normalization must retain them.
    for (pointer, changed) in [
        ("/members/0/result/citation/path", json!("crossed.rs")),
        (
            "/members/0/result/citation/sha256",
            json!(Sha256Digest::from_bytes([76; 32])),
        ),
        (
            "/members/0/result/source_verification/status",
            json!("requirements_unmet"),
        ),
        (
            "/members/0/result/source_verification/limits/hit",
            json!(["deadline"]),
        ),
        (
            "/members/0/result/source_verification/limits/max_sources",
            json!(7),
        ),
        (
            "/members/0/result/source_verification/limits/max_bytes",
            json!((16 << 20) - 1),
        ),
        ("/members/0/result/quota/reserved_bytes", json!(12)),
        ("/members/0/result/deadline_ms", json!(18)),
        (
            "/members/0/allocation/verification_deadline_ms",
            json!(2001),
        ),
    ] {
        let mut crossed = raw.clone();
        *crossed
            .pointer_mut(pointer)
            .context("semantic fixture pointer absent")? = changed;
        normalize_member_verification_timing(&mut crossed)?;
        assert_ne!(expected, crossed, "erased {pointer}");
    }
    Ok(())
}

#[path = "combined_reads/profile_families.rs"]
mod profile_families;

#[tokio::test]
async fn installed_single_index_scope_disclosure_matches_native_cli_mcp_and_workbench() -> Result<()>
{
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let selected = SessionScope {
        project_ids: vec![f.projects[0]],
        index_ids: Some(vec![f.indexes[0]]),
    };
    let window = f.window_for(&selected).await?;
    let backend = pipe::PipeTools::window(&f.control, window.clone());
    let mut mcp = McpSession::start_scope(&f, http::Scope::indexes([f.indexes[0]]))?;
    for (id, args) in (2200..).zip([
        json!({"view":"projects"}),
        json!({"view":"indexes","project_id":f.projects[0]}),
        json!({"view":"sources","project_id":f.projects[0],"index_id":f.indexes[0]}),
        json!({"view":"recommendations"}),
    ]) {
        let direct = native_read(backend.as_ref(), "airk_query_scope", &args).await?;
        let canonical = body(&direct)?;
        assert_eq!(canonical["mode"], "frozen_single_index");
        assert_eq!(
            canonical["observed_selection"],
            json!({"project_ids":selected.project_ids,"index_ids":[f.indexes[0]]})
        );
        // U110: MCP names the one index per call on the open path (no grant
        // to disclose); it compares with the same open read through the pipe.
        mcp.open_parity(&f, id, "query_scope", &args, "installed scope MCP open")
            .await?;
        for (tool, arguments) in [
            ("airk_query_scope", args.clone()),
            (
                "airk_parse",
                json!({"schema":"airk.parse.request/1",
            "mode":"execute","verification":"off","plan":{"steps":[{"id":"scope","tool":"query_scope","args":args}]}}),
            ),
        ] {
            let mut command = Ctl::new();
            command
                .window(window.clone())
                .arg("--service-runtime-dir")
                .arg(&f.control)
                .args(["tools", "call", tool, "--args"])
                .arg(arguments.to_string());
            let output =
                tokio::task::spawn_blocking(move || read_retry::output(&mut command)).await??;
            ensure!(
                output.status.success(),
                "installed scope CLI refused: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let envelope: Value = serde_json::from_slice(&output.stdout)?;
            let response = body(&envelope)?;
            let actual = if tool == "airk_parse" {
                response["steps"][0]["result"].clone()
            } else {
                response
            };
            assert_parity("installed scope CLI/Workbench", &canonical, &actual)?;
        }
    }
    f.assert_sources_unchanged()?;
    Ok(())
}

#[tokio::test]
async fn exposed_code_facts_preserve_retained_producer_kind_and_language_labels() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().join("source");
    fs::create_dir(&root)?;
    let source = "pub struct ProducerRecord;\n";
    fs::write(root.join("producer.rs"), source)?;
    let kernel = airk_kernel::Kernel::open(&root, Some(owner.path().join("data")))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let store = airk_store::FileStore::new(
        kernel
            .data_dir()
            .join("collections-1.2")
            .join(kernel.collection()),
    );
    let scope = store
        .current_scope(kernel.root().to_str().unwrap(), kernel.collection())?
        .context("current fixture scope")?;
    let read = store.current_read(&scope)?;
    let prior = read.artifact("producer.rs")?.context("retained fixture")?;
    let configuration = read
        .collection_state()?
        .context("fixture collection configuration")?
        .configuration;
    let mut payload = store.load_extraction_payload(&prior)?;
    let mut profile = prior
        .retained
        .as_ref()
        .and_then(|retained| retained.profile.as_ref())
        .context("fixture extraction profile")?
        .clone();
    drop(read);
    ensure!(
        !payload.facts.is_empty(),
        "fixture native declaration missing"
    );
    for fact in &mut payload.facts {
        fact.kind = "producer_marker".into();
    }
    profile.language = "producer_language".into();
    let artifact = airk_core::binding::bind_payload(
        &payload,
        &profile,
        airk_core::binding::ArtifactBinding {
            path: &prior.path,
            source_hash: prior.content_hash.parse()?,
            source_bytes: prior.bytes,
        },
    )?;
    let quota = airk_store::quota::Quota::for_store(&store)?;
    let reservation = quota.reserve(kernel.collection(), 4 * 1024 * 1024)?;
    let writer = store.lock()?;
    let retained = store.retain_payload_with_budget_owned(
        &writer,
        &artifact,
        &payload,
        &profile,
        Some(source.as_bytes()),
        Some(&reservation),
    )?;
    airk_kernel::current::CurrentUpdater {
        store: &store,
        quota: &quota,
        scope: &scope,
        writer: &writer,
        reservation: &reservation,
        configuration: Some(&configuration),
    }
    .apply(
        &[airk_store::current::ArtifactChange::Upsert(Box::new(
            airk_store::current::CapturedInput::new(retained),
        ))],
        None,
    )?;
    drop(writer);
    drop(reservation);
    let args = json!({"kind":"producer_marker","language":"producer_language","compact":false,
        "verification":"off","response_mode":"full","limit":10,"max_bytes":16384});
    let mut tools = airk_mcp::local::LocalTools::new(kernel.clone()).await?;
    let reply = tools
        .call("airk_code_facts".into(), args.as_object().unwrap().clone())
        .await?;
    let actual: Value =
        serde_json::from_str(&reply.content[0].as_text().context("MCP JSON body")?.text)?;
    assert_ne!(reply.is_error, Some(true), "{actual}");
    assert!(
        actual["items"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "{actual}"
    );
    assert!(
        actual["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["kind"] == "producer_marker")
    );
    assert!(actual.get("filter_observations").is_none());
    tools.close().await?;
    assert_eq!(fs::read_to_string(root.join("producer.rs"))?, source);
    Ok(())
}

// Current-only catalog coverage: docs/split/contracts.md, retired U95 comparison.
const CURRENT_PUBLIC_TOOL_NAMES: [&str; 30] = [
    "answer",
    "capabilities",
    "changes",
    "code_facts",
    "context",
    "diagnostics",
    "document_facts",
    "draft_plan",
    "draft_search",
    "draft_strategy",
    "file_info",
    "get_handoff",
    "get_template",
    "help",
    "imports",
    "list_files",
    "list_templates",
    "outline",
    "parse",
    "projects",
    "proposal_status",
    "query_scope",
    "read_file",
    "references",
    "relationships",
    "search",
    "status",
    "suggest_view",
    "symbols",
    "validate_draft",
];

fn validate_current_catalog(document: &Value, canonical: &Value) -> Result<()> {
    let tools = document["tools"]
        .as_array()
        .context("current tools missing")?;
    let names: Vec<_> = tools.iter().map(|tool| tool["name"].as_str()).collect();
    ensure!(
        names == CURRENT_PUBLIC_TOOL_NAMES.map(Some),
        "current catalog differs from the independent thirty-tool roster"
    );
    ensure!(
        document == canonical,
        "complete current catalog differs from native owner"
    );
    Ok(())
}

fn validate_current_catalog_text(text: &str, current: &Value) -> Result<()> {
    ensure!(
        text == airkctl::tools::list_text(current).context("current catalog text")?,
        "complete current catalog text differs"
    );
    Ok(())
}

#[test]
fn current_catalog_rejects_changed_or_unknown_definitions() -> Result<()> {
    let current = json!({"tools": airk_mcp::combined::public_catalog()});
    validate_current_catalog(&current, &current)?;
    for change in [
        "answer_schema",
        "answer_description",
        "answer_annotations",
        "retained_schema",
        "missing",
        "duplicate",
        "unknown",
    ] {
        let mut changed = current.clone();
        match change {
            "answer_schema" => changed["tools"][0]["inputSchema"] = json!({"type":"string"}),
            "answer_description" => changed["tools"][0]["description"] = json!("tampered"),
            "answer_annotations" => {
                changed["tools"][0]["annotations"] = json!({"readOnlyHint":false})
            }
            "retained_schema" => changed["tools"][1]["inputSchema"] = json!({"type":"string"}),
            "missing" => {
                changed["tools"].as_array_mut().unwrap().remove(0);
            }
            "duplicate" => {
                let answer = changed["tools"][0].clone();
                changed["tools"].as_array_mut().unwrap().insert(0, answer);
            }
            _ => changed["tools"][0]["name"] = json!("future_unknown"),
        }
        ensure!(
            validate_current_catalog(&changed, &current).is_err(),
            "{change}"
        );
    }
    let text = airkctl::tools::list_text(&current).context("catalog text")?;
    validate_current_catalog_text(&text, &current)?;
    ensure!(
        validate_current_catalog_text(&format!("{text}unreviewed_future_tool\n"), &current)
            .is_err()
    );
    Ok(())
}

// Exact owned roots only: never recursively search a result for familiar keys.
fn current_observations(value: &Value, timing: &[&str], request: bool) -> Result<Value> {
    use common::parity::strict::{Normalization, normalize};
    let mut rules = Normalization::default();
    for pointer in timing {
        if value.pointer(pointer).is_some() {
            rules.timing.push((*pointer).into());
        }
    }
    if request && let Some(id) = value.get("request_id") {
        let _: Incarnation =
            serde_json::from_value(id.clone()).context("native request observation")?;
        rules.identities.insert(
            "/request_id".into(),
            id.as_str().context("native request ID")?.into(),
        );
    }
    normalize(value, &rules).map_err(anyhow::Error::msg)
}

// Called only after decoding the owning PublicationVector or VerificationReceipt.
// This local typed interval rule does not widen generic strict parity policy.
fn current_interval_copy(raw: &Value) -> Result<Value> {
    let from = raw["observed_from_unix_ms"]
        .as_u64()
        .context("unsigned observation start")?;
    let to = raw["observed_to_unix_ms"]
        .as_u64()
        .context("unsigned observation end")?;
    ensure!(to >= from, "crossed native observation interval");
    let mut result = raw.clone();
    result["observed_from_unix_ms"] = json!("<native-interval-start>");
    result["observed_to_unix_ms"] = json!("<native-interval-end>");
    Ok(result)
}

fn current_member_copy(raw: &Value, status: bool) -> Result<Value> {
    // The caller establishes /members/N/result in airk.combined-read/1, or
    // the actual requested primitive's MCP body. Future/evidence objects are
    // never recursively interpreted as these owned headers.
    let mut result = current_observations(raw, &[], true)?;
    if let Some(freshness) = raw.get("freshness") {
        let _: airk_core::changes::FreshnessObservation =
            serde_json::from_value(freshness.clone())?;
        result["freshness"] = current_observations(freshness, &["/observed_unix_ms"], false)?;
    }
    if let Some(receipt) = raw.get("source_verification")
        && receipt["schema"] == "airk.saved-source-verification/1"
    {
        let typed: airk_core::verification::VerificationReceipt =
            serde_json::from_value(receipt.clone())?;
        ensure!(
            typed.observed_to_unix_ms >= typed.observed_from_unix_ms,
            "crossed verification observation interval"
        );
        result["source_verification"] = current_interval_copy(receipt)?;
    }
    if status
        && raw.get("observed_unix_ms").is_some()
        && raw.get("cache_age_ms").is_some()
        && raw.get("cached").is_some()
    {
        ensure!(
            raw["cached"].is_boolean()
                && raw["cache_age_ms"].as_u64().is_some_and(|age| age < 5000),
            "invalid native five-second Status cache observation"
        );
        result = current_observations(&result, &["/observed_unix_ms", "/cache_age_ms"], false)?;
        // Only this typed Status observer flag differs between independent
        // completed calls. Nested future.cached remains exact.
        result["cached"] = json!("<status-cache-observation>");
    }
    Ok(result)
}

/// The most a combined member's `allocation.deadline_ms` may declare. Every
/// combined read shares one request clock of at most 120,000 ms. A combined
/// Answer /3 wrapper alone declares, for each member that ran, the deadline of
/// its `airk.combined-context/2` summary (kernel `compose_inner`), which the
/// 600,000 ms Answer ceiling bounds (Core arbiter contract, "Combined Answer
/// /3 allowance": frame and time, for Answer only).
fn member_deadline_ceiling(raw: &Value) -> Result<u64> {
    if raw["context"]["schema"] != airk_core::combined_context::SUMMARY_SCHEMA_V2 {
        return Ok(120_000);
    }
    let summary: airk_core::combined_context::CombinedContextSummaryV2 =
        serde_json::from_value(raw["context"].clone())?;
    ensure!(
        (1..=600_000).contains(&summary.budget.deadline_ms),
        "Answer summary deadline is outside the 600000 ms Answer ceiling"
    );
    Ok(summary.budget.deadline_ms)
}

#[test]
fn current_member_deadline_ceiling_exceeds_120s_only_for_an_answer_wrapper() -> Result<()> {
    assert_eq!(
        member_deadline_ceiling(&json!({"schema":"airk.combined-read/1"}))?,
        120_000
    );
    // An Answer /2 summary keeps the ceiling of every other combined read.
    assert_eq!(
        member_deadline_ceiling(&json!({"schema":"airk.combined-read/1",
            "context":{"schema":"airk.combined-context/1","budget":{"deadline_ms":600_000}}}))?,
        120_000
    );
    let (budget, mut answer) = whole_verification_comparison_fixture(1)?;
    assert_eq!(answer["context"]["schema"], "airk.combined-context/2");
    assert_eq!(member_deadline_ceiling(&answer)?, budget.deadline_ms);
    for (deadline, ceiling) in [
        (120_001u64, Some(120_001u64)),
        (300_000, Some(300_000)),
        (600_000, Some(600_000)),
        (600_001, None),
        (0, None),
    ] {
        answer["context"]["budget"]["deadline_ms"] = json!(deadline);
        assert_eq!(member_deadline_ceiling(&answer).ok(), ceiling, "{deadline}");
    }
    Ok(())
}

fn current_body_copy(raw: &Value, status: bool) -> Result<Value> {
    if raw["schema"] != "airk.combined-read/1" {
        return current_member_copy(raw, status);
    }
    // Authenticate every raw byte before expanding the existing native codec;
    // a combined Answer /3 wrapper takes the Answer frame.
    if raw["context"]["schema"] == airk_core::combined_context::SUMMARY_SCHEMA_V2 {
        answer_body(raw)?;
    } else {
        body(raw)?;
    }
    airk_core::verification::validate_projection(raw)?;
    let mut logical = airk_core::combined_reads::materialize_combined_read(raw)?;
    normalize_member_verification_timing(&mut logical)?;
    if let Some(dictionary) = logical.get("shared_values") {
        ensure!(
            dictionary["schema"]
                == airk_core::combined_reads::shared_values::SHARED_CAPABILITY_SCHEMA
                || dictionary["schema"]
                    == airk_core::combined_reads::header_templates::HEADER_SCHEMA,
            "unknown comparison encoding dictionary"
        );
        logical.as_object_mut().unwrap().remove("shared_values");
    }
    let vector: airk_core::combined_reads::PublicationVector =
        serde_json::from_value(raw["publication_vector"].clone())?;
    vector.validate()?;
    logical["publication_vector"] = current_interval_copy(&raw["publication_vector"])?;
    // Core's logical codec intentionally detaches the wire receipt. Restore it
    // so unknown delivery fields still compare. Only the already-authenticated
    // digest of the independently dated raw body is an observation derivative.
    logical["delivery_observation"] = raw["delivery_observation"].clone();
    logical["delivery_observation"]["receipt"]["result_digest"] =
        json!("<authenticated-raw-observation-digest>");
    let deadline_ceiling = member_deadline_ceiling(raw)?;
    for member in logical["members"]
        .as_array_mut()
        .context("current members")?
    {
        let allocation: airk_core::combined_reads::MemberAllocation =
            serde_json::from_value(member["allocation"].clone())?;
        ensure!(
            allocation.deadline_ms <= deadline_ceiling,
            "invalid original member allowance"
        );
        // Fixed count/byte/verification caps stay literal. The shared elapsed
        // request clock is validated and normalized at its exact owner below.
        *member = current_observations(member, &["/elapsed_ms"], false)?;
        member["result"] = current_member_copy(&member["result"], status)?;
    }
    normalize_member_allocation_timing(&mut logical)?;
    Ok(logical)
}

fn current_envelope_copy(raw: &Value, status: bool) -> Result<Value> {
    let mut result = raw.clone();
    if let Some(structured) = raw.get("structuredContent") {
        let normalized = current_body_copy(structured, status)?;
        // Only the explicit MCP duplicate at /content/0/text is decoded.
        // Other text blocks (including JSON-looking evidence) stay literal.
        let text = raw
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .context("native MCP text duplicate")?;
        let parsed: Value = serde_json::from_str(text)?;
        ensure!(
            parsed == *structured,
            "MCP text and structured native bodies differ"
        );
        result["content"][0]["text"] = normalized.clone();
        result["structuredContent"] = normalized;
    } else if raw["schema"] == "airk.combined-read/1" {
        result = current_body_copy(raw, status)?;
    }
    Ok(result)
}

#[test]
fn current_owned_intervals_preserve_unknown_fields_and_reject_invalid_windows() -> Result<()> {
    let original = json!({"observed_from_unix_ms":10,"observed_to_unix_ms":20,
        "future":{"observed_from_unix_ms":7,"elapsed_ms":9,"request_id":"evidence"}});
    let mut later = original.clone();
    later["observed_from_unix_ms"] = json!(30);
    later["observed_to_unix_ms"] = json!(40);
    assert_eq!(
        current_interval_copy(&original)?,
        current_interval_copy(&later)?
    );
    for pointer in [
        "/future/observed_from_unix_ms",
        "/future/elapsed_ms",
        "/future/request_id",
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = json!("different");
        assert_ne!(
            current_interval_copy(&original)?,
            current_interval_copy(&changed)?,
            "{pointer}"
        );
    }
    for (from, to) in [
        (json!(20), json!(19)),
        (json!(true), json!(20)),
        (json!(1), json!(null)),
        (json!(-1), json!(20)),
    ] {
        let mut invalid = original.clone();
        invalid["observed_from_unix_ms"] = from;
        invalid["observed_to_unix_ms"] = to;
        ensure!(current_interval_copy(&invalid).is_err());
    }
    Ok(())
}

#[test]
fn current_cli_observations_preserve_future_fields_and_stable_identities() -> Result<()> {
    let body = json!({"request_id":"11111111111111111111111111111111",
        "future":{"request_id":"opaque evidence ID","cached":false,"elapsed_ms":7},
        "content_hash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "model_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "publication":"original-publication","revision":"original-revision","allocation":{"deadline_ms":17}});
    let envelope = |value: &Value| {
        json!({"isError":false,"content":[{"type":"text","text":value.to_string()},
        {"type":"text","text":"{\"request_id\":\"source text\",\"elapsed_ms\":19}"}],"structuredContent":value})
    };
    let original = current_envelope_copy(&envelope(&body), false)?;
    for pointer in [
        "/future/request_id",
        "/future/cached",
        "/future/elapsed_ms",
        "/content_hash",
        "/model_sha256",
        "/publication",
        "/revision",
        "/allocation/deadline_ms",
    ] {
        let mut changed = body.clone();
        *changed.pointer_mut(pointer).unwrap() = json!("different");
        assert_ne!(
            current_envelope_copy(&envelope(&changed), false)?,
            original,
            "{pointer}"
        );
    }
    let mut later = body.clone();
    later["request_id"] = json!("22222222222222222222222222222222");
    assert_eq!(current_envelope_copy(&envelope(&later), false)?, original);
    let status = json!({"observed_unix_ms":1,"cache_age_ms":0,"cached":false,"future":{"elapsed_ms":3,"cached":false}});
    let mut cached = status.clone();
    cached["observed_unix_ms"] = json!(2);
    cached["cache_age_ms"] = json!(4);
    cached["cached"] = json!(true);
    assert_eq!(
        current_member_copy(&status, true)?,
        current_member_copy(&cached, true)?
    );
    assert_ne!(
        current_member_copy(&status, false)?,
        current_member_copy(&cached, false)?
    );
    cached["future"]["cached"] = json!(true);
    assert_ne!(
        current_member_copy(&status, true)?,
        current_member_copy(&cached, true)?
    );
    let mut invalid = status.clone();
    invalid["cache_age_ms"] = json!(true);
    ensure!(current_member_copy(&invalid, true).is_err());
    Ok(())
}

fn ctl_outcome(
    code: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
    prefix: &str,
    status: bool,
) -> Result<Value> {
    let text = std::str::from_utf8(stdout)?;
    let stdout = match serde_json::from_str::<Value>(text) {
        Ok(value) => current_envelope_copy(&value, status)?,
        Err(_) => json!(text),
    };
    let stderr = std::str::from_utf8(stderr)?;
    let message = stderr
        .lines()
        .last()
        .unwrap_or_default()
        .strip_prefix(prefix)
        .unwrap_or_default();
    Ok(json!({"exit":code,"stdout":stdout,"message":message}))
}

/// Complete CURRENT CLI results equal the actual service's canonical handlers,
/// with reads routed through the session's window (the `airkctl session`
/// route). The nine command and refusal cases of the combined session
/// survive; its three `--project-ids/--index-ids` narrowing rows went with the
/// removed `--combined-session` selector. The old U95 fixture stays
/// historical and is neither replayed, recaptured nor normalized here.
#[tokio::test]
async fn airkctl_combined_reads_match_current_native_handlers() -> Result<()> {
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let window = f.window_for(&f.session).await?;
    let control = f.control.to_str().context("control path")?.to_owned();
    let lexical = json!({"query":"boundaryneedle","mode":"lexical","limit":4,"max_bytes":4096,"verification":"off"}).to_string();
    let search = [
        "search",
        "boundaryneedle",
        "--mode",
        "lexical",
        "--limit",
        "12",
        "--max-bytes",
        "8192",
        "--verification",
        "off",
    ];
    let lines: Vec<Vec<&str>> = vec![
        vec!["tools", "list"],
        vec!["tools", "list", "--json"],
        vec!["tools", "help", "airk_search"],
        vec!["tools", "call", "airk_search", "--args", &lexical],
        vec![
            "tools",
            "call",
            "help",
            "--args",
            r#"{"topic":"workbench"}"#,
        ],
        search.to_vec(),
        vec!["files"],
        vec!["status"],
        vec!["info", "api/policy.md"],
    ];
    // Prove the complete CURRENT service catalog before any historical replay.
    let current_catalog = match airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::ToolsCatalog {
            detail: airk_core::service::ToolsCatalogDetail::List { schemas: true },
        },
    )
    .await?
    {
        ControlResponse::ToolsCatalog { document } => serde_json::from_str::<Value>(&document)?,
        _ => anyhow::bail!("current catalog request returned a non-catalog control response"),
    };
    let canonical_help = current_catalog["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "search")
        .context("canonical Search definition")?;
    let mut failures = Vec::new();
    assert_eq!(
        lines.len(),
        9,
        "all remaining command/scope cases must survive"
    );
    for (case, line) in lines.iter().enumerate() {
        let full: Vec<&str> = ["--service-runtime-dir", &control]
            .into_iter()
            .chain(line.iter().copied())
            .collect();
        // Explicit independent input rows: shorthand commands must preserve the
        // same arguments and narrowing, not merely return plausible output.
        let expected = if case == 0 {
            json!({"exit":0,"message":"","stdout":airkctl::tools::list_text(&current_catalog)})
        } else if case == 1 {
            json!({"exit":0,"message":"","stdout":current_catalog.clone()})
        } else if case == 2 {
            json!({"exit":0,"message":"","stdout":airkctl::tools::help_text(canonical_help,"airk_search")})
        } else {
            let (tool, arguments) = match case {
                3 => ("airk_search", serde_json::from_str::<Value>(&lexical)?),
                4 => ("help", json!({"topic":"workbench"})),
                5 => (
                    "airk_search",
                    json!({"query":"boundaryneedle","mode":"lexical","path_prefix":null,"limit":12,"max_bytes":8192,"verification":"off"}),
                ),
                6 => (
                    "airk_list_files",
                    json!({"cursor":null,"prefix":"","limit":10,"max_bytes":8192}),
                ),
                7 => ("airk_status", json!({})),
                8 => (
                    "airk_file_info",
                    json!({"path":"api/policy.md","revision":null,"if_evidence":null,"follow_moves":false,"known_hash":null,"artifact_id":null}),
                ),
                _ => unreachable!("independent nine-case input table"),
            };
            // airkctl answers help in process for an unselected call; a
            // session-routed help names no selection.
            let selection = if tool == "help" {
                airk_core::service::ToolSelection::Unselected
            } else {
                window.tool_selection()
            };
            let response = airk_cli::service_runtime::request(
                &f.control,
                ControlMethod::ToolsCall {
                    tool: tool.into(),
                    arguments: arguments.as_object().context("native arguments")?.clone(),
                    selection,
                    response_mode: airk_core::service::ToolResponseMode::Full,
                    usage_client: airk_core::usage::ClientFamily::Cli,
                    read_attempt: None,
                },
            )
            .await?;
            match response {
                ControlResponse::ToolResult { result } => {
                    let refused = result["isError"].as_bool().unwrap_or(false);
                    let mut outcome = ctl_outcome(
                        Some(if refused { 1 } else { 0 }),
                        &serde_json::to_vec(&result)?,
                        &[],
                        "airkctl: ",
                        case == 7,
                    )?;
                    if refused {
                        outcome["message"] = json!("MCP tool failed; see the result above");
                    }
                    outcome
                }
                ControlResponse::ToolCallFailed { message } => {
                    json!({"exit":1,"message":message,"stdout":""})
                }
                _ => anyhow::bail!("unexpected canonical control response for {line:?}"),
            }
        };
        let arguments: Vec<std::ffi::OsString> = std::iter::once("airkctl")
            .chain(full.iter().copied())
            .map(Into::into)
            .collect();
        let mut route = window.route();
        let new = tokio::task::spawn_blocking(move || {
            let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
            let mut io = airkctl::cli::Io {
                read_attempt: None,
                stdin: &mut std::io::empty(),
                stdout: &mut stdout,
                stderr: &mut stderr,
                full_responses: true,
                window: Some(&mut route),
            };
            let code = airkctl::cli::run(arguments, &mut io);
            (code, stdout, stderr)
        })
        .await?;
        let current = if line.as_slice() == ["tools", "list"]
            || line.as_slice() == ["tools", "list", "--json"]
        {
            // Current catalog proof uses raw complete output BEFORE stable(),
            // historical timing masks or the U107/U108 projection can touch it.
            ensure!(
                new.2.is_empty(),
                "current catalog unexpectedly wrote stderr"
            );
            let stdout = if line.len() == 3 {
                serde_json::from_slice::<Value>(&new.1)?
            } else {
                json!(std::str::from_utf8(&new.1)?)
            };
            json!({"exit":new.0,"message":"","stdout":stdout})
        } else {
            ctl_outcome(
                Some(i32::from(new.0)),
                &new.1,
                &new.2,
                "airkctl: ",
                case == 7,
            )?
        };
        if case == 0 {
            validate_current_catalog_text(
                current["stdout"].as_str().context("CLI list text")?,
                &current_catalog,
            )?;
        } else if case == 1 {
            ensure!(
                current["stdout"] == current_catalog,
                "complete current catalog differs from native owner"
            );
        }
        assert!(
            current == expected,
            "current combined {line:?}: first difference {:?}",
            first_difference(&current, &expected, "")
        );
        if current["exit"] != json!(0) {
            failures.push(format!("{} {}", line.join(" "), current["message"]));
        }
    }
    // Current canonical refusals: a listing across the combined session
    // and an ambiguous origin (tool errors).
    assert_eq!(failures.len(), 2, "{failures:#?}");
    assert!(
        failures[0].starts_with("files \"MCP tool failed"),
        "{failures:#?}"
    );
    assert!(
        failures[1].starts_with("info api/policy.md \"MCP tool failed"),
        "{failures:#?}"
    );
    f.assert_sources_unchanged()
}

/// One M3 read on the service pipe, retried only as the shared typed retry
/// classifier allows; a refusal is returned as its failure frame.
async fn pipe_read(
    f: &Fixture,
    selection: airk_core::service::ToolSelection,
    tool: &str,
    arguments: &Value,
) -> Result<std::result::Result<Value, Value>> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let response = airk_cli::service_runtime::request(
            &f.control,
            ControlMethod::ToolsCall {
                tool: tool.into(),
                arguments: arguments
                    .as_object()
                    .context("object arguments required")?
                    .clone(),
                selection: selection.clone(),
                response_mode: airk_core::service::ToolResponseMode::Full,
                usage_client: airk_core::usage::ClientFamily::Unknown,
                read_attempt: None,
            },
        )
        .await?;
        let ControlResponse::ToolResult { result } = response else {
            return Ok(Err(serde_json::to_value(&response)?));
        };
        let Some(retry) = read_retry::within(&result, deadline) else {
            return Ok(Ok(result));
        };
        tokio::time::sleep(retry.delay).await;
    }
}

/// One in-process `airkctl` run on the fixture's control root with full
/// responses: (exit code, stdout, stderr).
async fn airkctl_full(f: &Fixture, line: &[&str]) -> Result<(u8, String, String)> {
    airkctl_routed(f, None, line).await
}

/// As `airkctl_full`, with reads that name no selection routed through
/// `window` as an `airkctl session` routes them.
async fn airkctl_routed(
    f: &Fixture,
    window: Option<&pipe::Window>,
    line: &[&str],
) -> Result<(u8, String, String)> {
    let mut route = window.map(pipe::Window::route);
    let control = f.control.to_str().context("control root is not UTF-8")?;
    let arguments: Vec<std::ffi::OsString> = ["airkctl", "--service-runtime-dir", control]
        .into_iter()
        .chain(line.iter().copied())
        .map(Into::into)
        .collect();
    let (code, stdout, stderr) = tokio::task::spawn_blocking(move || {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let mut io = airkctl::cli::Io {
            read_attempt: None,
            stdin: &mut std::io::empty(),
            stdout: &mut stdout,
            stderr: &mut stderr,
            full_responses: true,
            window: route.as_mut(),
        };
        let code = airkctl::cli::run(arguments, &mut io);
        (code, stdout, stderr)
    })
    .await?;
    Ok((code, String::from_utf8(stdout)?, String::from_utf8(stderr)?))
}

fn open_selection(indexes: &[&str], sets: &[&str]) -> airk_core::service::ToolSelection {
    airk_core::service::ToolSelection::Open {
        index_ids: indexes.iter().map(|name| (*name).to_owned()).collect(),
        index_set_ids: sets.iter().map(|name| (*name).to_owned()).collect(),
        workspace: None,
    }
}

/// The refusal code of a pipe read that did not produce a result.
fn refusal_code(refused: &Value) -> String {
    refused["code"].as_str().unwrap_or_default().to_owned()
}

/// U105 (open-agent-access step 5): `airkctl tools call --index/--set
/// [--connection]` and M3 `ToolSelection::Open` are one service path. The
/// list tool and an open read return the same complete result through both;
/// a set expands to its member indexes read whole. (U110 removed the
/// per-connection defaults and agent choice this test also covered.)
#[tokio::test]
async fn open_reads_keep_cli_and_pipe_results_and_expand_sets() -> Result<()> {
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    // The list tool: the local owner may list; CLI and pipe answer the same.
    let listed = pipe_read(&f, open_selection(&[], &[]), "list_scopes", &json!({}))
        .await?
        .map_err(|refused| anyhow::anyhow!("list refused: {refused}"))?;
    let (code, stdout, stderr) = airkctl_full(&f, &["tools", "call", "list_scopes"]).await?;
    ensure!(code == 0, "airkctl list_scopes failed: {stderr}");
    let from_cli: Value = serde_json::from_str(&stdout)?;
    assert_eq!(from_cli, listed, "list_scopes CLI and pipe differ");
    let list = body(&listed)?;
    let indexes = list["indexes"].as_array().context("indexes absent")?;
    let sets = list["index_sets"].as_array().context("index sets absent")?;
    assert_eq!((indexes.len(), sets.len()), (3, 3), "{list}");
    assert_eq!(indexes[0]["id"], json!(f.indexes[0]));
    assert_eq!(indexes[0]["readiness"], "ready", "{list}");
    assert_eq!(
        indexes[0]["paths"],
        json!([f.registrations[0].root]),
        "{list}"
    );
    let set = sets
        .iter()
        .find(|set| set["id"] == json!(f.projects[0]))
        .context("first set absent")?;
    assert_eq!(set["index_ids"], json!([f.indexes[0], f.indexes[1]]));
    let index_name = indexes[0]["name"]
        .as_str()
        .context("index name")?
        .to_owned();
    let set_name = set["name"].as_str().context("set name")?.to_owned();

    // One index by exact name: the same complete read through CLI and pipe.
    let arguments = json!({"query":"boundaryneedle","mode":"lexical","limit":8,"max_bytes":8192,"verification":"off"});
    let piped = pipe_read(
        &f,
        open_selection(&[&index_name], &[]),
        "airk_search",
        &arguments,
    )
    .await?
    .map_err(|refused| anyhow::anyhow!("open read refused: {refused}"))?;
    ensure!(piped["isError"] != true, "open read failed: {piped}");
    let argument_text = arguments.to_string();
    let (code, stdout, stderr) = airkctl_full(
        &f,
        &[
            "tools",
            "call",
            "airk_search",
            "--index",
            &index_name,
            "--args",
            &argument_text,
        ],
    )
    .await?;
    ensure!(code == 0, "airkctl open read failed: {stderr}");
    let from_cli: Value = serde_json::from_str(&stdout)?;
    let (left, right) = (
        stable(logical_comparison_copy(&body(&from_cli)?)?)?,
        stable(logical_comparison_copy(&body(&piped)?)?)?,
    );
    if let Some(difference) = first_difference(&left, &right, "") {
        anyhow::bail!("open read CLI and pipe differ: {difference}");
    }
    ensure!(!piped.to_string().contains("grant_nonce"), "N3: {piped}");

    // A set expands to its two member indexes, each read whole.
    let expanded = pipe_read(
        &f,
        open_selection(&[], &[&set_name]),
        "airk_search",
        &arguments,
    )
    .await?
    .map_err(|refused| anyhow::anyhow!("set read refused: {refused}"))?;
    ensure!(expanded["isError"] != true, "set read failed: {expanded}");
    let expanded = body(&expanded)?;
    assert_eq!(
        expanded["members"]
            .as_array()
            .context("members absent")?
            .len(),
        2,
        "{expanded}"
    );

    // Refusals before any read: an unknown name, a local call naming nothing.
    let refused = pipe_read(
        &f,
        open_selection(&["no such index"], &[]),
        "airk_search",
        &arguments,
    )
    .await?
    .err()
    .context("refusal expected")?;
    assert_eq!(refusal_code(&refused), "scope_name_unknown", "{refused}");
    // U110: a local call naming no scope and no workspace is the tool error
    // `scope_required`, as over MCP.
    let unscoped = pipe_read(&f, open_selection(&[], &[]), "airk_search", &arguments)
        .await?
        .map_err(|refused| anyhow::anyhow!("unscoped read not a tool error: {refused}"))?;
    assert_eq!(
        unscoped["structuredContent"]["error"]["code"], "scope_required",
        "{unscoped}"
    );
    f.assert_sources_unchanged()
}

fn listed_tools(reply: &Value) -> Result<Vec<String>> {
    Ok(reply["result"]["tools"]
        .as_array()
        .context("tools/list result absent")?
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
        .collect())
}

/// U110: the HTTPS MCP endpoint takes no credential (the fixture enrolls
/// nothing and sends no token); every session is the built-in local agent
/// session. Reads name their scope (`index_ids`/`index_set_ids`) or a
/// `workspace` whose stored default applies; otherwise `scope_required`.
/// Workspace defaults set through the CLI apply to the next MCP call, and
/// MCP, the pipe and `airkctl tools call --workspace` return the same result.
#[tokio::test]
async fn tokenless_https_reads_name_their_scope_or_a_workspace_default() -> Result<()> {
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let mut mcp = McpSession::start_scope(&f, http::Scope::none())?;
    let catalog = mcp.request(2, "tools/list", json!({}))?;
    let names = listed_tools(&catalog)?;
    ensure!(
        names.contains(&"list_scopes".to_owned()) && names.contains(&"search".to_owned()),
        "{catalog}"
    );
    let search = catalog["result"]["tools"]
        .as_array()
        .context("tools")?
        .iter()
        .find(|tool| tool["name"] == "search")
        .context("search tool")?;
    let properties = &search["inputSchema"]["properties"];
    ensure!(
        properties["workspace"].is_object()
            && properties["index_ids"]["description"]
                .as_str()
                .is_some_and(|text| text.contains("A scope is required")),
        "{search}"
    );
    let arguments = json!({"query":"boundaryneedle","mode":"lexical","limit":8,"max_bytes":8192,"verification":"off"});
    let with = |key: &str, value: Value| {
        let mut changed = arguments.clone();
        changed[key] = value;
        changed
    };
    // No scope: refused, and the refusal says how to name one.
    let unscoped = mcp.call(3, "search", arguments.clone())?;
    assert_eq!(
        unscoped["result"]["structuredContent"]["error"]["code"], "scope_required",
        "{unscoped}"
    );
    // A named index: the same complete read through MCP and the pipe.
    let index = f.indexes[0].to_string();
    let named = mcp.call(4, "search", with("index_ids", json!([index])))?;
    ensure!(named["result"]["isError"] != true, "{named}");
    let named_body = body(&named["result"])?;
    assert_eq!(named_body["read_scope"]["source"], "named", "{named_body}");
    let piped = pipe_read(
        &f,
        open_selection(&[index.as_str()], &[]),
        "search",
        &arguments,
    )
    .await?
    .map_err(|refused| anyhow::anyhow!("pipe read refused: {refused}"))?;
    assert_parity(
        "U110 named MCP vs pipe",
        &body(&named["result"])?,
        &body(&piped)?,
    )?;
    // A workspace default set through the CLI applies to a nested workspace.
    let folder = f.registrations[0].root.clone();
    let (code, stdout, stderr) = airkctl_full(
        &f,
        &[
            "scope",
            "default",
            "--workspace",
            &folder,
            "--index",
            &index,
        ],
    )
    .await?;
    ensure!(code == 0, "scope default failed: {stderr}");
    let stored: Value = serde_json::from_str(&stdout)?;
    assert_eq!(stored["schema"], 1, "{stored}");
    assert_eq!(stored["entries"][0]["folder"], json!(folder), "{stored}");
    assert_eq!(
        stored["entries"][0]["index_ids"],
        json!([index]),
        "{stored}"
    );
    let nested = format!("{folder}/nested/deeper");
    let defaulted = mcp.call(5, "search", with("workspace", json!(nested)))?;
    ensure!(defaulted["result"]["isError"] != true, "{defaulted}");
    let defaulted_body = body(&defaulted["result"])?;
    assert_eq!(defaulted_body["read_scope"]["source"], "workspace_default");
    assert_eq!(
        defaulted_body["read_scope"]["workspace_folder"],
        json!(folder)
    );
    let mut left = named_body.clone();
    let mut right = defaulted_body.clone();
    for value in [&mut left, &mut right] {
        value.as_object_mut().context("body")?.remove("read_scope");
    }
    assert_eq!(
        stable(logical_comparison_copy(&left)?)?,
        stable(logical_comparison_copy(&right)?)?,
        "a workspace default reads exactly its stored scope"
    );
    // The CLI's --workspace takes the same path.
    let arguments_text = arguments.to_string();
    let (code, stdout, stderr) = airkctl_full(
        &f,
        &[
            "tools",
            "call",
            "search",
            "--args",
            &arguments_text,
            "--workspace",
            &nested,
        ],
    )
    .await?;
    ensure!(code == 0, "tools call --workspace failed: {stderr}");
    let from_cli: Value = serde_json::from_str(&stdout)?;
    assert_parity(
        "U110 workspace MCP vs CLI",
        &body(&defaulted["result"])?,
        &body(&from_cli)?,
    )?;
    // list_scopes shows the stored defaults and the one for a workspace.
    let listed = mcp.call(6, "list_scopes", json!({"workspace": nested}))?;
    let listed_body = body(&listed["result"])?;
    assert_eq!(
        listed_body["workspace_defaults"][0]["folder"],
        json!(folder)
    );
    assert_eq!(listed_body["workspace_default"]["folder"], json!(folder));
    // Clearing the default: the workspace no longer selects a scope.
    let (code, stdout, stderr) =
        airkctl_full(&f, &["scope", "default", "--workspace", &folder]).await?;
    ensure!(code == 0, "scope default clear failed: {stderr}");
    assert_eq!(
        serde_json::from_str::<Value>(&stdout)?["entries"],
        json!([])
    );
    let cleared = mcp.call(7, "search", with("workspace", json!(nested)))?;
    assert_eq!(
        cleared["result"]["structuredContent"]["error"]["code"], "scope_required",
        "{cleared}"
    );
    Ok(())
}

/// U107/U108 (behaviour): `limit_per_index` gives each selected index exactly
/// that many results under the overall cap, for a window selection and an
/// open read alike; it binds the continuation; an oversized request is refused;
/// each read member reports its elapsed time and index name.
#[tokio::test]
async fn per_index_counts_apply_to_window_and_open_reads_and_bind_cursors() -> Result<()> {
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let window = f.window_for(&f.session).await?.tool_selection();
    let open = open_selection(&[&f.indexes[0].to_string(), &f.indexes[1].to_string()], &[]);
    // A cursor tool: the scoped members hold more than one file each.
    let arguments = json!({"limit_per_index":1,"max_bytes":8192,"verification":"off"});
    for (label, selection) in [("window", window.clone()), ("open", open.clone())] {
        let read = pipe_read(&f, selection.clone(), "airk_list_files", &arguments)
            .await?
            .map_err(|refused| anyhow::anyhow!("{label} read refused: {refused}"))?;
        ensure!(read["isError"] != true, "{label}: {read}");
        let value = body(&read)?;
        let members = value["members"].as_array().context("members absent")?;
        assert_eq!(members.len(), 2, "{label}: {value}");
        for member in members {
            assert_eq!(member["allocation"]["limit"], 1, "{label}: {member}");
            ensure!(member["elapsed_ms"].is_u64(), "{label}: {member}");
            ensure!(member["index_name"].is_string(), "{label}: {member}");
        }
        // The continuation binds limit_per_index.
        let cursor = value["cursor"]
            .as_str()
            .context("cursor absent")?
            .to_owned();
        let mut resumed = arguments.clone();
        resumed["cursor"] = json!(cursor);
        let next = pipe_read(&f, selection.clone(), "airk_list_files", &resumed)
            .await?
            .map_err(|refused| anyhow::anyhow!("{label} resume refused: {refused}"))?;
        ensure!(next["isError"] != true, "{label}: {next}");
        resumed["limit_per_index"] = json!(2);
        let changed = pipe_read(&f, selection.clone(), "airk_list_files", &resumed)
            .await?
            .map_err(|refused| anyhow::anyhow!("{label} changed refused: {refused}"))?;
        assert_eq!(
            body(&changed)?["error"]["code"],
            "combined_cursor_expired",
            "{label}: {changed}"
        );
        // Two indexes x 60 exceeds the per-call maximum of 100.
        let mut oversized = arguments.clone();
        oversized["limit_per_index"] = json!(60);
        let refused = pipe_read(&f, selection, "airk_list_files", &oversized)
            .await?
            .map_err(|refused| anyhow::anyhow!("{label} oversized refused: {refused}"))?;
        assert_eq!(
            body(&refused)?["error"]["code"],
            "combined_limit_per_index_too_large",
            "{label}: {refused}"
        );
    }
    f.assert_sources_unchanged()
}

/// The valid two-part Answer /3 reply of the Core semantic cases, under the
/// cases' default budget.
fn answer_reply_case() -> Result<(airk_core::arbiter::answer_loop::BudgetV3, Value)> {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../fixtures/split/arbiter/answer_loop_cases.json"
    ))?;
    let budget: airk_core::arbiter::answer_loop::BudgetV3 =
        serde_json::from_value(cases["default_budget"].clone())?;
    let reply = cases["response_cases"]
        .as_array()
        .context("Answer reply cases")?
        .iter()
        .find(|case| case["id"] == "two_parts_valid")
        .context("valid two-part Answer reply")?["response"]
        .clone();
    airk_core::arbiter::answer_loop::ContextResponseV3::from_canonical(&reply, &budget)?;
    Ok((budget, reply))
}

/// The case reply as one transport observed it. Since change-log entry 54
/// its record ids and the digests that bind them are the same on every
/// transport and run; what differs is the reply's request id, its timing,
/// the observation time of its one `read_status`, and each Focus receipt
/// digest, which binds measured model work.
fn observed_answer_reply(
    header: u8,
    index: Option<IndexId>,
) -> Result<(airk_core::arbiter::answer_loop::BudgetV3, Value)> {
    let (budget, mut reply) = answer_reply_case()?;
    // Focus receipt digests: distinct before stay distinct after.
    let mut custody = std::collections::BTreeMap::new();
    for position in 0..reply["decisions"].as_array().context("decisions")?.len() {
        if let Some(receipt) = reply["decisions"][position].get_mut("receipt") {
            let next = u8::try_from(custody.len())?;
            *receipt = custody
                .entry(receipt.as_str().context("receipt digest")?.to_owned())
                .or_insert_with(|| {
                    json!(Sha256Digest::from_bytes(
                        [header.wrapping_mul(16).wrapping_add(0x80 + next); 32]
                    ))
                })
                .clone();
        }
    }
    reply["request_id"] = json!(Incarnation::from_bytes([header; 16]));
    reply["read_status"] = json!({"freshness":{"state":"idle","requested_watermark":3,
        "completed_watermark":3,"running_watermark":null,"last_success_unix_ms":5,
        "error":null,"live_source_verified":false,"observed_unix_ms":1_000 + u64::from(header)},
        "discovery_readiness":{"state":"ready"}});
    let timing = u64::from(header);
    reply["elapsed_ms"] = json!(1000 + timing);
    reply["work"]["compile_us"] = json!(40 + timing);
    reply["work"]["inference_us"] = json!(60 + timing);
    reply["decision_log"]["timings"]["compile_us"] = json!(30 + timing);
    reply["decision_log"]["timings"]["inference_us"] = json!(50 + timing);
    for part in reply["parts"].as_array_mut().context("parts")? {
        part["deadline_share_ms"] = json!(100_000 + timing);
    }
    if let Some(index) = index {
        reply["scope"]["index_id"] = json!(index);
        for application in reply["applications"]
            .as_array_mut()
            .context("applications")?
        {
            application["scope"]["index_id"] = json!(index);
        }
    }
    airk_core::arbiter::answer_loop::ContextResponseV3::from_canonical(&reply, &budget)?;
    Ok((budget, reply))
}

#[test]
fn answer_reply_comparison_normalizes_only_transport_identities_custody_and_timing() -> Result<()> {
    let (budget, raw) = observed_answer_reply(1, None)?;
    let (_, later) = observed_answer_reply(2, None)?;
    let (left, right) = common::parity::context::pair(&raw, &later, &budget)?;
    assert_eq!(left, right);
    // Evidence, decisions, accounting and provenance stay compared.
    for (pointer, value) in [
        (
            "/records/0/canonical_result/future_native_field/retained",
            json!(false),
        ),
        ("/records/1/arguments/query", json!("changed query")),
        ("/paragraphs/0/text", json!("changed paragraph")),
        ("/decisions/1/chosen/0/symbol", json!("B")),
        ("/decisions/1/roster_digest", json!("ab".repeat(32))),
        ("/parts/0/rounds/0/new_evidence", json!(1)),
        ("/parts/1/sub_needs/0/text", json!("changed need")),
        ("/work/calls", json!(7)),
        ("/work/bytes", json!(4096)),
        ("/candidate_count", json!(1)),
        ("/model_work/focus_pairs", json!(1)),
        ("/applications/0/reason", json!("changed reason")),
        ("/work/policy_us", json!(5)),
    ] {
        let mut changed = later.clone();
        *changed
            .pointer_mut(pointer)
            .with_context(|| format!("tamper pointer {pointer}"))? = value;
        if let Ok((left, right)) = common::parity::context::pair(&raw, &changed, &budget) {
            assert_ne!(left, right, "{pointer}");
        }
    }
    // A citation moved to another record is a different reply.
    let mut changed = later.clone();
    changed["paragraphs"][0]["evidence"][0] = changed["records"][0]["id"].clone();
    if let Ok((left, right)) = common::parity::context::pair(&raw, &changed, &budget) {
        assert_ne!(left, right, "moved citation");
    }
    // Two decisions that saw different source units cannot become equal.
    let mut changed = later.clone();
    changed["decisions"][2]["basis"]["digest"] = changed["decisions"][1]["basis"]["digest"].clone();
    let (left, right) = common::parity::context::pair(&raw, &changed, &budget)?;
    assert_ne!(left, right, "custody digest equality pattern");
    // Unknown fields stay compared; transport identities must be typed.
    let mut wire = later.clone();
    wire["future"] = json!({"request_id":"evidence-owned","elapsed_ms":17});
    let mut changed = wire.clone();
    changed["future"]["request_id"] = json!("different-evidence-owned");
    let (left, right) = common::parity::context::pair(&wire, &changed, &budget)?;
    assert_ne!(left, right, "unknown outer fields cannot disappear");
    // Since entry 54 a projection carries no per-read status and record ids
    // and the digests binding them are compared as sent: any change differs.
    let mut changed = later.clone();
    changed["records"][0]["canonical_result"]["request_id"] = json!("not-an-incarnation");
    let (left, right) = common::parity::context::pair(&raw, &changed, &budget)?;
    assert_ne!(left, right, "a projection gained a per-read field");
    let mut changed = later.clone();
    changed["records"][1]["canonical_result"]["full_digest"] =
        changed["records"][0]["canonical_result"]["full_digest"].clone();
    let (left, right) = common::parity::context::pair(&raw, &changed, &budget)?;
    assert_ne!(left, right, "two different read bodies cannot become one");
    for (field, value) in [
        ("full_digest", json!("ab".repeat(32))),
        ("full_bytes", json!(1)),
    ] {
        let mut changed = later.clone();
        changed["records"][0]["canonical_result"][field] = value;
        let (left, right) = common::parity::context::pair(&raw, &changed, &budget)?;
        assert_ne!(left, right, "{field}");
    }
    for (pointer, value) in [
        ("/decision_log/digest", json!("cd".repeat(32))),
        ("/decisions/2/basis/digest", json!("ef".repeat(32))),
    ] {
        let mut changed = later.clone();
        *changed
            .pointer_mut(pointer)
            .with_context(|| format!("tamper pointer {pointer}"))? = value;
        if let Ok((left, right)) = common::parity::context::pair(&raw, &changed, &budget) {
            assert_ne!(left, right, "{pointer}");
        }
    }
    // A record renamed everywhere it is cited is a different reply.
    let mut renamed = later.clone();
    let old = renamed["records"][0]["id"]
        .as_str()
        .context("id")?
        .to_owned();
    let text = renamed.to_string().replace(&old, &"0f".repeat(32));
    renamed = serde_json::from_str(&text)?;
    if let Ok((left, right)) = common::parity::context::pair(&raw, &renamed, &budget) {
        assert_ne!(left, right, "renamed record id");
    }
    let mut changed = later.clone();
    changed["parts"][0]["deadline_share_ms"] = json!(budget.deadline_ms + 1);
    assert!(common::parity::context::pair(&raw, &changed, &budget).is_err());
    let mut changed = later;
    changed["elapsed_ms"] = json!("later");
    assert!(common::parity::context::pair(&raw, &changed, &budget).is_err());
    let native = json!({"resultType":"complete","content":[]});
    let legacy = json!({"content":[]});
    let (left, right) =
        common::parity::context::negotiated_mcp_pair(&native, &legacy, "2025-11-25")?;
    assert_eq!(left, right);
    assert!(
        common::parity::context::negotiated_mcp_pair(
            &json!({"resultType":"future"}),
            &legacy,
            "2025-11-25"
        )
        .is_err()
    );
    assert_ne!(
        common::parity::context::negotiated_mcp_pair(&native, &legacy, "2026-07-28")?.0,
        legacy
    );
    Ok(())
}

fn current_answer_request() -> Result<(airk_core::arbiter::answer_loop::BudgetV3, Value)> {
    let request = airk_core::arbiter::answer_loop::ContextRequestV3 {
        needs: Vec::new(),
        anchors: Vec::new(),
        deliverable: None,
        known: Vec::new(),
        schema: airk_core::arbiter::answer_loop::REQUEST_SCHEMA_V3.into(),
        question: "boundaryneedle".into(),
        budget: Default::default(),
        continuation: None,
    };
    request.validate()?;
    let budget = request.budget.clone();
    let mut arguments = serde_json::to_value(request)?;
    arguments["response_mode"] = json!("full");
    Ok((budget, arguments))
}

fn whole_verification_comparison_fixture(
    header: u8,
) -> Result<(airk_core::arbiter::answer_loop::BudgetV3, Value)> {
    use airk_core::{combined_context::*, verification::*};
    let ids = [
        IndexId::from_identity(Incarnation::from_bytes([1; 16])),
        IndexId::from_identity(Incarnation::from_bytes([2; 16])),
    ];
    let authority = Sha256Digest::from_bytes([8; 32]);
    let attempt = Incarnation::from_bytes([9; 16]);
    let (budget, _) = answer_reply_case()?;
    let allowances = initial_allocations_v3(attempt, authority, &ids, &budget)?;
    let mut members = Vec::new();
    let mut reports = Vec::new();
    let mut proofs = Vec::new();
    let mut publications = Vec::new();
    let mut used = LoopUse::default();
    let mut bindings = Vec::new();
    for (position, index) in ids.iter().copied().enumerate() {
        let (_, raw) = observed_answer_reply(header + u8::try_from(position)?, Some(index))?;
        let allowance = allowances[position]
            .clone()
            .context("admitted member allowance")?;
        let reply = airk_core::arbiter::answer_loop::ContextResponseV3::from_canonical(
            &raw,
            &allowance.clamp(&budget)?,
        )?;
        bindings.extend(reply_delivery_bindings(&reply)?);
        let work = LoopUse::of_reply(&reply)?;
        used = used.checked_add(&work)?;
        reports.push(PartitionReportV3 {
            index_id: index,
            allowance: Some(allowance),
            outcome: PartitionOutcomeV3::Known {
                used: work,
                read_depth: reply.work.rounds,
                peak_fanout: reply.work.fanout,
            },
        });
        publications.push(json!({"index_id":index,"publication":reply.scope.publication}));
        members.push(json!({"index_id":index,"result":raw}));
        let mut own = json!({"steps":[{"result":raw}],"future_owner":{"preserved":[1,2,3]}});
        let receipt = VerificationReceipt {
            schema: "airk.saved-source-verification/1".into(),
            method: "sha256-full-saved-source/1".into(),
            mode: VerificationMode::Report,
            status: "unverified".into(),
            scope_digest: authority,
            selection_digest: airk_kernel::verification::selection_digest(&own)?,
            observed_from_unix_ms: u64::from(header),
            observed_to_unix_ms: u64::from(header) + 1,
            publication_fence: "current".into(),
            inventory_complete: true,
            all_references_accounted: true,
            source_rows_omitted: 1,
            requirements_satisfied: Some(false),
            counts: VerificationCounts {
                references: Some(1),
                references_lower_bound: 1,
                requested_sources: Some(1),
                requested_sources_lower_bound: 1,
                unchecked_sources: Some(1),
                ..Default::default()
            },
            limits: VerificationBudget {
                mode: VerificationMode::Report,
                max_sources: 4,
                max_bytes: 8388608,
                deadline_ms: 900,
            }
            .into(),
            origins: Vec::new(),
            sources: Vec::new(),
            qualifications: Vec::new(),
            error: None,
            references: vec![VerifiedReference {
                id: "retained-reference".into(),
                source_id: authority,
                result_pointer: "/steps/0/result/records/0/canonical_result/future_native_field"
                    .into(),
                kind: "citation".into(),
            }],
        };
        own["source_verification"] = serde_json::to_value(receipt)?;
        proofs.push(json!({"index_id":index,"result":own,"allocation":VerificationAllocation {max_sources:4,max_bytes:8388608,deadline_ms:1000},
            "reference_bindings":[{"member_pointer":"/steps/0/result","result_pointer":format!("/members/{position}/result")}]}));
    }
    let summary = CombinedContextSummaryV2 {
        schema: SUMMARY_SCHEMA_V2.into(),
        attempt,
        selection: authority,
        budget: budget.clone(),
        members: reports,
        known_used: used,
        unknown_held: LoopUse::default(),
        termination: Termination::Complete,
        delivery_bindings: bindings,
    };
    summary.validate(&ids)?;
    let projection = json!({"schema":"airk.combined-read/1","members":members,"context":summary,
        "publication_vector":{"authority":authority,"observed_from_unix_ms":1,"observed_to_unix_ms":2,"members":publications},
        "member_verification":{"schema":"airk.combined-verification/1","scope_identity":authority,"members":proofs,"partial":true,"error":null}});
    airk_core::verification::validate_projection(&projection)?;
    context_delivery_bindings(&projection)?;
    Ok((budget, projection))
}

#[test]
fn whole_answer_verification_comparison_validates_duplicate_replies_and_keeps_native_proof_data()
-> Result<()> {
    let (budget, left) = whole_verification_comparison_fixture(1)?;
    let (_, mut right) = whole_verification_comparison_fixture(3)?;
    let mut missing = left.clone();
    missing
        .as_object_mut()
        .unwrap()
        .remove("member_verification");
    for mode in [
        airk_core::verification::VerificationMode::Report,
        airk_core::verification::VerificationMode::Require,
    ] {
        let controls = airk_core::verification::VerificationControls {
            verification: Some(mode),
            ..Default::default()
        };
        assert!(
            common::parity::context::whole_verification(&missing, &missing, &budget, controls)
                .is_err()
        );
        let mut null = missing.clone();
        null["member_verification"] = Value::Null;
        assert!(
            common::parity::context::whole_verification(&null, &null, &budget, controls).is_err()
        );
    }
    let off = airk_core::verification::VerificationControls::off();
    assert!(
        common::parity::context::whole_verification(&missing, &missing, &budget, off)?.is_none()
    );
    assert!(common::parity::context::whole_verification(&left, &left, &budget, off).is_err());
    for member in right["member_verification"]["members"]
        .as_array_mut()
        .unwrap()
    {
        member["allocation"]["deadline_ms"] = json!(800);
        member["result"]["source_verification"]["limits"]["deadline_ms"] = json!(700);
        member["result"]["source_verification"]["limits"]["effective_deadline_ms"] = json!(600);
    }
    let project = |raw: &Value| -> Result<Value> {
        let summary: airk_core::combined_context::CombinedContextSummaryV2 =
            serde_json::from_value(raw["context"].clone())?;
        let mut normalized = raw.clone();
        for position in 0..2 {
            normalized["members"][position]["result"] = common::parity::context::pair(
                &raw["members"][position]["result"],
                &raw["members"][position]["result"],
                &common::parity::context::member_budget(&summary, position)?,
            )?
            .0;
        }
        normalized["member_verification"] = common::parity::context::whole_verification(
            raw,
            &normalized,
            &budget,
            Default::default(),
        )?
        .unwrap();
        Ok(normalized)
    };
    let a = project(&left)?;
    let b = project(&right)?;
    assert_eq!(a, b);
    assert_eq!(
        left["member_verification"]["members"][0]["result"]["source_verification"]["source_rows_omitted"],
        1
    );
    for pointer in [
        "/member_verification/members/0/result/source_verification/scope_digest",
        "/member_verification/members/0/result/source_verification/counts/bytes_read",
        "/member_verification/members/0/result/future_owner",
    ] {
        let mut changed = right.clone();
        if pointer.ends_with("scope_digest") {
            *changed.pointer_mut(pointer).unwrap() = json!("ff".repeat(32));
        } else if pointer.ends_with("future_owner") {
            *changed.pointer_mut(pointer).unwrap() = json!({"changed":true});
        } else {
            changed["member_verification"]["members"][0]["result"]["source_verification"]["counts"]
                ["bytes_read"] = json!(1);
        }
        if let Ok(value) = project(&changed) {
            assert_ne!(a, value);
        }
    }
    let mut future = right.clone();
    future["member_verification"]["members"][0]["result"]["future_owner"] =
        json!({"legitimate_new_data":true});
    let own = &mut future["member_verification"]["members"][0]["result"];
    own["source_verification"]["selection_digest"] =
        serde_json::to_value(airk_kernel::verification::selection_digest(own)?)?;
    assert_ne!(
        a,
        project(&future)?,
        "validly bound unknown owner data must remain compared"
    );
    let mut changed = right.clone();
    changed["member_verification"]["members"][0]["result"]["steps"][0]["result"]["question"] =
        json!("changed duplicate");
    assert!(project(&changed).is_err());
    let mut changed = right.clone();
    changed["member_verification"]["members"][0]["allocation"]["max_bytes"] = json!(8388609);
    assert!(project(&changed).is_err());
    let mut changed = right;
    changed["member_verification"]["members"][0]["result"]["source_verification"]["limits"]["deadline_ms"] =
        json!(801);
    assert!(project(&changed).is_err());
    Ok(())
}
async fn context_pipe_once(
    f: &Fixture,
    selection: airk_core::service::ToolSelection,
    arguments: &Value,
) -> Result<Value> {
    let response = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::ToolsCall {
            tool: "airk_answer".into(),
            arguments: arguments.as_object().context("Answer arguments")?.clone(),
            selection,
            response_mode: airk_core::service::ToolResponseMode::Full,
            usage_client: airk_core::usage::ClientFamily::Unknown,
            read_attempt: None,
        },
    )
    .await?;
    let ControlResponse::ToolResult { result } = response else {
        anyhow::bail!(
            "finite Answer refusal was not replayed: {}",
            serde_json::to_string(&response)?
        )
    };
    if result["isError"] == true {
        let raw = airk_host::service_logs::export_raw(&f.control)
            .map(|bytes| json!({"bytes":bytes}))
            .unwrap_or_else(|error| json!({"unavailable":error.to_string()}));
        let path = save_replies(
            "finite Answer native diagnostic",
            &json!({"result":result,"raw_service_logs":raw}),
        )?;
        anyhow::bail!(
            "finite Answer error was not replayed: {result}; raw native diagnostic {}",
            path.display()
        );
    }
    Ok(result)
}
fn assert_context_envelopes(
    label: &str,
    left: &Value,
    right: &Value,
    arguments: &Value,
    mcp_protocol: Option<&str>,
) -> Result<()> {
    // Capture the complete originals before any validator/projection executes;
    // write only on failure, while the native fixture is still alive.
    let originals =
        json!({"left":left,"right":right,"arguments":arguments,"right_mcp_protocol":mcp_protocol});
    match compare_context_envelopes(label, left, right, arguments, mcp_protocol) {
        Ok(()) => Ok(()),
        Err(error) => {
            let path = save_replies(label, &originals)?;
            Err(error)
                .with_context(|| format!("full raw Answer pair retained at {}", path.display()))
        }
    }
}
/// U110: the envelope without its top-level `read_scope` (from both the structured
/// copy and an identical text copy), and that scope. A text copy that differs
/// from the structured copy is left for the comparison to report.
fn without_open_scope(envelope: &Value) -> Result<(Option<Value>, Value)> {
    let mut copy = envelope.clone();
    let scope = copy
        .get_mut("structuredContent")
        .and_then(Value::as_object_mut)
        .and_then(|structured| structured.remove("read_scope"));
    if let Some(text) = envelope.pointer("/content/0/text").and_then(Value::as_str)
        && let Ok(mut parsed) = serde_json::from_str::<Value>(text)
        && let Some(fields) = parsed.as_object_mut()
        && let Some(text_scope) = fields.remove("read_scope")
    {
        ensure!(
            scope.as_ref().is_none_or(|scope| *scope == text_scope),
            "text and structured scopes differ"
        );
        copy["content"][0]["text"] = Value::String(parsed.to_string());
        return Ok((Some(text_scope), copy));
    }
    Ok((scope, copy))
}

fn compare_context_envelopes(
    label: &str,
    left: &Value,
    right: &Value,
    arguments: &Value,
    mcp_protocol: Option<&str>,
) -> Result<()> {
    let parsed: airk_mcp::AnswerArgs = serde_json::from_value(arguments.clone())?;
    parsed.request.validate()?;
    // U110: an open read reports its scope after the native read (outside
    // its digests). Both sides must report the same scope; the comparison of
    // the native packet then proceeds without it.
    let (left_scope, left) = without_open_scope(left)?;
    let (right_scope, right) = without_open_scope(right)?;
    ensure!(
        left_scope == right_scope,
        "{label}: open read scopes differ: {left_scope:?} vs {right_scope:?}"
    );
    let (left, right) = (&left, &right);
    let budget = &parsed.request.budget;
    let verification = parsed.verification.controls();
    let projected = mcp_protocol
        .map(|protocol| common::parity::context::negotiated_mcp_pair(left, right, protocol))
        .transpose()?;
    let (left, right) = projected
        .as_ref()
        .map_or((left, right), |(left, right)| (left, right));
    let left_body = answer_body(left)?;
    let right_body = answer_body(right)?;
    let (a, b) = if left_body["schema"] == "airk.combined-read/1"
        || right_body["schema"] == "airk.combined-read/1"
    {
        ensure!(
            left_body["schema"] == right_body["schema"],
            "Answer scope shape differs"
        );
        let mut a = current_body_copy(&left_body, false)?;
        let mut b = current_body_copy(&right_body, false)?;
        let left_members = a["members"].as_array().context("Answer members")?.clone();
        let right_members = b["members"].as_array().context("Answer members")?.clone();
        ensure!(
            left_members.len() == right_members.len(),
            "Answer member counts differ"
        );
        let indexes = left_members
            .iter()
            .map(|member| serde_json::from_value::<IndexId>(member["index_id"].clone()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut summaries = Vec::new();
        for (value, body) in [(&mut a, &left_body), (&mut b, &right_body)] {
            let summary: airk_core::combined_context::CombinedContextSummaryV2 =
                serde_json::from_value(body["context"].clone())?;
            summary.validate(&indexes)?;
            ensure!(
                summary.budget == *budget,
                "original Answer budget changed in summary"
            );
            answer_summary_copy(value, &summary);
            summaries.push(summary);
        }
        for (position, (lhs, rhs)) in left_members.iter().zip(&right_members).enumerate() {
            ensure!(
                lhs["index_id"] == rhs["index_id"],
                "Answer member order/identity differs"
            );
            let left_reply = answer_member_body(&left_body, &left_body["members"][position])?;
            let right_reply = answer_member_body(&right_body, &right_body["members"][position])?;
            a["members"][position]["result"] = common::parity::context::pair(
                &left_reply,
                &left_reply,
                &common::parity::context::member_budget(&summaries[0], position)?,
            )?
            .0;
            b["members"][position]["result"] = common::parity::context::pair(
                &right_reply,
                &right_reply,
                &common::parity::context::member_budget(&summaries[1], position)?,
            )?
            .0;
        }
        if let Some(proof) =
            common::parity::context::whole_verification(&left_body, &a, budget, verification)?
        {
            a["member_verification"] = proof;
        }
        if let Some(proof) =
            common::parity::context::whole_verification(&right_body, &b, budget, verification)?
        {
            b["member_verification"] = proof;
        }
        (
            common::parity::context::replace_body(left, a)?,
            common::parity::context::replace_body(right, b)?,
        )
    } else {
        common::parity::context::envelopes(left, right, budget)?
    };
    if let Some(difference) = first_difference(&a, &b, "") {
        anyhow::bail!("{label}: {difference}");
    }
    Ok(())
}
#[tokio::test]
async fn current_answer_single_index_preserves_complete_pipe_cli_and_https_mcp_results()
-> Result<()> {
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let window = f
        .window_for(&SessionScope {
            project_ids: vec![f.projects[0]],
            index_ids: Some(vec![f.indexes[0]]),
        })
        .await?;
    let (_budget, arguments) = current_answer_request()?;
    let reference = context_pipe_once(&f, window.tool_selection(), &arguments).await?;
    let text = arguments.to_string();
    let (code, stdout, stderr) = airkctl_routed(
        &f,
        Some(&window),
        &["tools", "call", "airk_answer", "--args", &text],
    )
    .await?;
    ensure!(
        code == 0,
        "finite CLI Answer failed without replay: {stderr}"
    );
    let cli: Value = serde_json::from_str(&stdout)?;
    assert_context_envelopes(
        "current Answer single CLI",
        &reference,
        &cli,
        &arguments,
        None,
    )?;
    // U110: HTTPS MCP names the one index per call (read whole) on the open
    // path; it compares with the same open read through the pipe.
    let open_reference = context_pipe_once(
        &f,
        open_selection(&[&f.indexes[0].to_string()], &[]),
        &arguments,
    )
    .await?;
    let mut mcp = McpSession::start_scope(&f, http::Scope::indexes([f.indexes[0]]))?;
    let reply = mcp.request(
        2,
        "tools/call",
        json!({"name":"answer","arguments":arguments}),
    )?;
    ensure!(
        reply.get("error").is_none(),
        "MCP Answer failed without replay: {reply}"
    );
    assert_context_envelopes(
        "current Answer single HTTPS MCP",
        &open_reference,
        &reply["result"],
        &arguments,
        Some(&mcp.negotiated_protocol),
    )?;
    f.assert_sources_unchanged()
}

/// In a running service an Answer is a job of the service's Answer job table
/// (plan section 12.4.1, L2b): the service gives its table to the project
/// owner, and the owner's Answer handler goes through it. So the same request
/// in the same scope, sent again within one service lifetime, is answered by
/// the reply the table kept and not by a second run. Each call is still its
/// own request. A second run would measure its own elapsed time and stage
/// timings, which is how the two are told apart.
#[tokio::test]
async fn a_repeated_answer_in_one_service_is_the_reply_its_job_table_kept() -> Result<()> {
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    // One index through a W4 window selection: the read authority a job's
    // caller holds since grant removal (change-log entry 70).
    let window = f
        .window_for(&SessionScope {
            project_ids: vec![f.projects[0]],
            index_ids: Some(vec![f.indexes[0]]),
        })
        .await?;
    let (budget, arguments) = current_answer_request()?;
    let body = |result: &Value| -> Result<Value> {
        match result
            .get("structuredContent")
            .filter(|body| body.is_object())
        {
            Some(body) => Ok(body.clone()),
            None => Ok(serde_json::from_str(
                result
                    .pointer("/content/0/text")
                    .and_then(Value::as_str)
                    .context("Answer reply text")?,
            )?),
        }
    };
    let mut replies = Vec::new();
    for _ in 0..3 {
        let result = context_pipe_once(&f, window.tool_selection(), &arguments).await?;
        replies.push(body(&result)?);
    }
    // The typed reply, without what each call's own rendering adds: its own
    // wait (`elapsed_ms`) and its delivery record.
    let typed = |reply: &Value| -> Result<Value> {
        let mut value = serde_json::to_value(
            airk_core::arbiter::answer_loop::ContextResponseV3::from_canonical(reply, &budget)?,
        )?;
        value["elapsed_ms"] = Value::Null;
        Ok(value)
    };
    let first = typed(&replies[0])?;
    ensure!(
        first["ticket"].is_null() && first["parts"].as_array().is_some_and(|parts| !parts.is_empty()),
        "the first call is a complete answer: {first}"
    );
    ensure!(
        replies[0]["delivery"]["reused"] == false,
        "the first call made its job: {}",
        replies[0]
    );
    for later in &replies[1..] {
        ensure!(
            typed(later)? == first,
            "a repeated Answer ran again instead of attaching to its kept job: {} then {later}",
            replies[0]
        );
        ensure!(
            later["delivery"]["reused"] == true
                && later["delivery"]["job"] == replies[0]["delivery"]["job"]
                && later["delivery"]["wait_ms"] == later["elapsed_ms"],
            "a reused reply says so, for its own call: {later}"
        );
        ensure!(
            later["request_id"].is_null() || later["request_id"] != replies[0]["request_id"],
            "each call is its own request: {later}"
        );
    }
    f.assert_sources_unchanged()
}

/// The test side of the service's test-support Answer job gate
/// (`answer_jobs::wiring::test_gate`): each job connects here before its work
/// starts and waits for commands.
struct JobGate {
    listener: tokio::net::TcpListener,
}
struct HeldJob {
    stream: tokio::io::BufReader<tokio::net::TcpStream>,
    job: String,
}
const GATE_WAIT: Duration = Duration::from_secs(120);
impl JobGate {
    async fn bind() -> Result<(Self, String)> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?.to_string();
        Ok((Self { listener }, address))
    }
    /// The next job that starts.
    async fn next(&self) -> Result<HeldJob> {
        use tokio::io::AsyncBufReadExt;
        let (stream, _) = tokio::time::timeout(GATE_WAIT, self.listener.accept())
            .await
            .context("no Answer job started under the deadline")??;
        let mut stream = tokio::io::BufReader::new(stream);
        let mut line = String::new();
        tokio::time::timeout(GATE_WAIT, stream.read_line(&mut line)).await??;
        let job = line
            .trim()
            .strip_prefix("started ")
            .context("the gate's first line names the job")?
            .to_owned();
        Ok(HeldJob { stream, job })
    }
    /// No other job has started (checked without waiting).
    fn none_pending(&self) -> Result<()> {
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        ensure!(
            self.listener.poll_accept(&mut context).is_pending(),
            "a further Answer execution started"
        );
        Ok(())
    }
}
impl HeldJob {
    /// Wait until the maker's session has ended in the service.
    async fn maker_closed(&mut self) -> Result<()> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        self.stream.get_mut().write_all(b"m").await?;
        let mut line = String::new();
        tokio::time::timeout(GATE_WAIT, self.stream.read_line(&mut line))
            .await
            .context("the maker's session did not end under the deadline")??;
        ensure!(line.trim() == "maker-closed", "unexpected gate line {line:?}");
        Ok(())
    }
    async fn release(mut self) -> Result<()> {
        use tokio::io::AsyncWriteExt;
        self.stream.get_mut().write_all(b"r").await?;
        Ok(())
    }
}

/// The decisive L2c test (final plan section 12.4.2; review H1), through the
/// real service, supervisor `query()` path, owner and handler. The submitting
/// client's call returns after a short wait and its connection closes; the
/// supervisor tears its session down; only then is the job let run. Its
/// scoped reads still succeed and it finishes; a reconnecting client
/// recovers the result by the job's handle, and the receipt shows one
/// execution. Repeated for a queued job whose maker is gone before it ever
/// starts, with no client polling it.
#[tokio::test]
async fn answer_jobs_outlive_their_makers_session_and_are_recovered_by_their_handle() -> Result<()>
{
    let (gate, address) = JobGate::bind().await?;
    let f = Fixture::new(true)?;
    let _service = f
        .start_with_env(&[("AIRK_TEST_ANSWER_JOB_GATE", address)])
        .await?;
    // The makers and the recovering callers read one index through the same
    // W4 window selection; the jobs hold that selection's authority, never
    // their makers' sessions (change-log entry 70).
    let window = f
        .window_for(&SessionScope {
            project_ids: vec![f.projects[0]],
            index_ids: Some(vec![f.indexes[0]]),
        })
        .await?;
    let selection = || window.tool_selection();
    let (budget, base) = current_answer_request()?;
    let with = |question: &str, extra: Value| -> Value {
        let mut arguments = base.clone();
        arguments["question"] = json!(question);
        for (key, value) in extra.as_object().into_iter().flatten() {
            arguments[key] = value.clone();
        }
        arguments
    };
    let body = |result: &Value| -> Result<Value> {
        match result
            .get("structuredContent")
            .filter(|body| body.is_object())
        {
            Some(body) => Ok(body.clone()),
            None => Ok(serde_json::from_str(
                result
                    .pointer("/content/0/text")
                    .and_then(Value::as_str)
                    .context("Answer reply text")?,
            )?),
        }
    };
    let complete = |reply: &Value| -> Result<()> {
        let typed =
            airk_core::arbiter::answer_loop::ContextResponseV3::from_canonical(reply, &budget)?;
        ensure!(
            typed.ticket.is_none() && !typed.parts.is_empty() && !typed.records.is_empty(),
            "a complete answer with evidence: {reply}"
        );
        Ok(())
    };

    // 1. A running job whose maker leaves.
    let question = "boundaryneedle";
    let maker = {
        let arguments = with(question, json!({"fresh":true,"wait_ms":3_000}));
        let f = &f;
        let selection = selection();
        async move { context_pipe_once(f, selection, &arguments).await }
    };
    let (held, ticket) = tokio::join!(gate.next(), maker);
    let mut held = held?;
    let ticket = body(&ticket?)?;
    ensure!(
        ticket["ticket"]["id"].as_str() == Some(held.job.as_str())
            && ticket["delivery"]["job"].as_str() == Some(held.job.as_str())
            && ticket["delivery"]["reused"] == false,
        "the maker's short wait ends with its job's ticket: {ticket}"
    );
    // The maker's connection is gone; the supervisor ends its session.
    held.maker_closed().await?;
    let job = held.job.clone();
    held.release().await?;
    // A new connection recovers the result by the handle alone.
    let recovered = body(
        &context_pipe_once(&f, selection(), &with(question, json!({"continuation":job}))).await?,
    )?;
    complete(&recovered)?;
    ensure!(
        recovered["delivery"]["job"].as_str() == Some(job.as_str())
            && recovered["delivery"]["reused"] == true,
        "the recovered result is the job's own, delivered again: {recovered}"
    );
    gate.none_pending()?;

    // 2. A queued job: two held jobs fill the gathering limit of two, a third
    // is queued, its maker leaves, and nothing polls it.
    let mut fillers = Vec::new();
    for filler in ["boundaryneedle filler one", "boundaryneedle filler two"] {
        let arguments = with(filler, json!({"fresh":true,"wait_ms":3_000}));
        let call = context_pipe_once(&f, selection(), &arguments);
        let (held, reply) = tokio::join!(gate.next(), call);
        reply?;
        fillers.push(held?);
    }
    let queued = body(
        &context_pipe_once(
            &f,
            selection(),
            &with("boundaryneedle queued", json!({"fresh":true,"wait_ms":3_000})),
        )
        .await?,
    )?;
    let queued_job = queued["delivery"]["job"]
        .as_str()
        .context("a queued job's handle")?
        .to_owned();
    ensure!(
        queued["ticket"]["id"].as_str() == Some(queued_job.as_str()),
        "the queued maker gets its ticket: {queued}"
    );
    gate.none_pending()?;
    for filler in fillers {
        filler.release().await?;
    }
    let mut started = gate.next().await?;
    ensure!(started.job == queued_job, "the queued job starts next");
    started.maker_closed().await?;
    started.release().await?;
    let recovered = body(
        &context_pipe_once(
            &f,
            selection(),
            &with("boundaryneedle queued", json!({"continuation":queued_job})),
        )
        .await?,
    )?;
    complete(&recovered)?;
    ensure!(
        recovered["delivery"]["job"].as_str() == Some(queued_job.as_str())
            && recovered["delivery"]["reused"] == true,
        "the queued job's result is recovered by its handle: {recovered}"
    );
    gate.none_pending()?;
    f.assert_sources_unchanged()
}

/// A combined Answer is never a job of the service's table. Its job options
/// are bounded as on the single-index path, and an option it cannot honour
/// (`invocation_key`, `wait_ms`, `interactive`) is refused with the typed
/// argument error before any member is queried, never silently ignored.
/// `fresh`, which a combined Answer always is, still answers.
#[tokio::test]
async fn a_combined_answer_refuses_job_options_it_cannot_honour() -> Result<()> {
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let (_budget, base) = current_answer_request()?;
    // Both indexes through one W4 window: a combined read.
    let window = f.window_for(&f.session).await?;
    let combined = || window.tool_selection();
    for extra in [
        json!({"invocation_key":"turn-1"}),
        json!({"wait_ms":3_000}),
        json!({"interactive":true}),
        json!({"invocation_key":"bad key"}),
        json!({"wait_ms":0}),
    ] {
        let mut arguments = base.clone();
        for (key, value) in extra.as_object().into_iter().flatten() {
            arguments[key] = value.clone();
        }
        let response = airk_cli::service_runtime::request(
            &f.control,
            ControlMethod::ToolsCall {
                tool: "airk_answer".into(),
                arguments: arguments.as_object().context("Answer arguments")?.clone(),
                selection: combined(),
                response_mode: airk_core::service::ToolResponseMode::Full,
                usage_client: airk_core::usage::ClientFamily::Unknown,
                read_attempt: None,
            },
        )
        .await?;
        let ControlResponse::ToolResult { result } = response else {
            anyhow::bail!("{extra}: not a tool result: {}", serde_json::to_string(&response)?)
        };
        ensure!(result["isError"] == true, "{extra}: answered: {result}");
        let body = &result["structuredContent"];
        ensure!(
            body["error"]["code"] == "invalid_native_arguments"
                && body["error"]["query_submitted"] == false,
            "{extra}: not the typed argument refusal: {result}"
        );
    }
    let mut fresh = base.clone();
    fresh["fresh"] = json!(true);
    let answered = context_pipe_once(&f, combined(), &fresh).await?;
    ensure!(answered["isError"] != true, "a fresh combined Answer answers: {answered}");
    f.assert_sources_unchanged()
}
#[tokio::test]
async fn current_answer_many_and_open_keep_complete_cli_pipe_and_https_mcp_packets() -> Result<()> {
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let (_budget, arguments) = current_answer_request()?;
    let window = f.window_for(&f.session).await?;
    let reference = context_pipe_once(&f, window.tool_selection(), &arguments).await?;
    let text = arguments.to_string();
    let (code, stdout, stderr) = airkctl_routed(
        &f,
        Some(&window),
        &["tools", "call", "airk_answer", "--args", &text],
    )
    .await?;
    ensure!(
        code == 0,
        "many finite Answer failed without replay: {stderr}"
    );
    assert_context_envelopes(
        "current Answer many CLI",
        &reference,
        &serde_json::from_str(&stdout)?,
        &arguments,
        None,
    )?;
    // U110: the open read of the same two indexes through the pipe, the
    // CLI (`tools call --index`) and tokenless HTTPS MCP naming them per call.
    let first = f.indexes[0].to_string();
    let second = f.indexes[1].to_string();
    let reference =
        context_pipe_once(&f, open_selection(&[&first, &second], &[]), &arguments).await?;
    let (code, stdout, stderr) = airkctl_full(
        &f,
        &[
            "tools",
            "call",
            "airk_answer",
            "--index",
            &first,
            "--index",
            &second,
            "--args",
            &text,
        ],
    )
    .await?;
    ensure!(
        code == 0,
        "Open finite Answer failed without replay: {stderr}"
    );
    assert_context_envelopes(
        "current Answer Open CLI",
        &reference,
        &serde_json::from_str(&stdout)?,
        &arguments,
        None,
    )?;
    let mut mcp = McpSession::start(&f)?;
    let reply = mcp.request(
        2,
        "tools/call",
        json!({"name":"answer","arguments":arguments}),
    )?;
    ensure!(
        reply.get("error").is_none(),
        "Open MCP Answer failed without replay: {reply}"
    );
    assert_context_envelopes(
        "current Answer Open HTTPS MCP",
        &reference,
        &reply["result"],
        &arguments,
        Some(&mcp.negotiated_protocol),
    )?;
    f.assert_sources_unchanged()
}

/// Pure validation of retained real transport outputs; never starts a service.
#[test]
#[ignore = "requires an explicitly retained current Answer transport pair"]
fn retained_current_answer_pair_validates_full_transport_results() -> Result<()> {
    use sha2::{Digest, Sha256};
    let path = std::env::var_os("AIRK_CONTEXT_PARITY_CAPTURE")
        .context("retained Answer input path required")?;
    let bytes = fs::read(path)?;
    let capture: Value = serde_json::from_slice(&bytes)?;
    let arguments: airk_mcp::AnswerArgs = serde_json::from_value(capture["arguments"].clone())?;
    arguments.request.validate()?;
    ensure!(
        arguments.request.schema == airk_core::arbiter::answer_loop::REQUEST_SCHEMA_V3,
        "current /3 request required"
    );
    let protocol = capture.get("right_mcp_protocol").and_then(Value::as_str);
    assert_context_envelopes(
        "retained real Answer transport pair",
        &capture["left"],
        &capture["right"],
        &capture["arguments"],
        protocol,
    )?;
    let owners: [(&str, &[u8]); 10] = [
        (
            "crates/airk-cli/tests/combined_reads.rs",
            include_bytes!("combined_reads.rs").as_slice(),
        ),
        (
            "crates/airk-cli/tests/common/parity/context.rs",
            include_bytes!("common/parity/context.rs").as_slice(),
        ),
        (
            "crates/airk-cli/tests/common/parity/strict.rs",
            include_bytes!("common/parity/strict.rs").as_slice(),
        ),
        (
            "crates/airk-core/src/arbiter/answer_loop.rs",
            include_bytes!("../../airk-core/src/arbiter/answer_loop.rs").as_slice(),
        ),
        (
            "crates/airk-core/src/combined_context.rs",
            include_bytes!("../../airk-core/src/combined_context.rs").as_slice(),
        ),
        (
            "crates/airk-core/src/combined_context/answer.rs",
            include_bytes!("../../airk-core/src/combined_context/answer.rs").as_slice(),
        ),
        (
            "crates/airk-core/src/context_records.rs",
            include_bytes!("../../airk-core/src/context_records.rs").as_slice(),
        ),
        (
            "crates/airk-kernel/src/arbiter.rs",
            include_bytes!("../../airk-kernel/src/arbiter.rs").as_slice(),
        ),
        (
            "crates/airk-mcp/src/lib.rs",
            include_bytes!("../../airk-mcp/src/lib.rs").as_slice(),
        ),
        (
            "fixtures/split/arbiter/answer_loop_cases.json",
            include_bytes!("../../../fixtures/split/arbiter/answer_loop_cases.json").as_slice(),
        ),
    ];
    let compiled_sources: std::collections::BTreeMap<_, _> = owners
        .into_iter()
        .map(|(name, bytes)| (name, format!("{:x}", Sha256::digest(bytes))))
        .collect();
    println!(
        "AIRK_CONTEXT_PARITY_VALIDATED {}",
        json!({"schema":1,"input_sha256":format!("{:x}",Sha256::digest(&bytes)),
        "validator_sha256":airk_cli::runtime_pair::digest_file(&std::env::current_exe()?)?,"compiled_sources":compiled_sources,
        "scope":"validated-normalized-full-answer-v3","full_envelopes_equal":true})
    );
    Ok(())
}

#[tokio::test]
async fn one_index_window_and_open_explicit_source_selectors_use_original_authorized_combined_owner()
-> Result<()> {
    let f = Fixture::with_runtime_topology(true, 3, false, None, false, true)?;
    let _service = f.start().await?;
    let ControlResponse::Health { observation } =
        airk_cli::service_runtime::request(&f.control, ControlMethod::Health).await?
    else {
        anyhow::bail!("health missing");
    };
    let clock = observation.catalog_clock.context("catalog clock")?;
    let ControlResponse::WindowOpened {
        window_id, lease, ..
    } = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::WindowOpen {
            label: "one-index alias source navigation".into(),
            owner_pid: std::process::id(),
        },
    )
    .await?
    else {
        anyhow::bail!("window missing");
    };
    let granted = airk_cli::service_runtime::request(
        &f.control,
        ControlMethod::WindowScopeGrant {
            window_id,
            lease: lease.clone(),
            selection: airk_core::service::WindowScopeSelection {
                namespace: clock.namespace,
                expected_catalog_revision: clock.revision,
                index_ids: vec![f.indexes[0]],
                project_ids: f.projects[..2].to_vec(),
            },
        },
    )
    .await?;
    ensure!(
        matches!(granted, ControlResponse::WindowScopeGranted { .. }),
        "grant refused: {}",
        serde_json::to_string(&granted)?
    );
    async fn once(
        f: &Fixture,
        selection: airk_core::service::ToolSelection,
        tool: &str,
        args: Value,
    ) -> Result<Value> {
        let response = airk_cli::service_runtime::request(
            &f.control,
            ControlMethod::ToolsCall {
                tool: tool.into(),
                arguments: args.as_object().context("arguments")?.clone(),
                selection,
                response_mode: airk_core::service::ToolResponseMode::Full,
                usage_client: airk_core::usage::ClientFamily::Unknown,
                read_attempt: None,
            },
        )
        .await?;
        let ControlResponse::ToolResult { result } = response else {
            anyhow::bail!("read refused: {}", serde_json::to_string(&response)?);
        };
        Ok(result)
    }
    let window = airk_core::service::ToolSelection::Window { window_id, lease };
    let ordinary=once(&f,window.clone(),"airk_search",json!({"query":"boundaryneedle","mode":"lexical","path_prefix":"api/","verification":"report","response_mode":"full"})).await?;
    ensure!(
        ordinary["isError"] != true,
        "ordinary search refused: {ordinary}"
    );
    let original = body(&ordinary)?;
    assert_ne!(
        original["schema"], "airk.combined-read/1",
        "no-selector one-index route must remain the native singleton"
    );
    let hit = original["hits"][0].clone();
    let proof = &original["source_verification"];
    let source = proof["sources"]
        .as_array()
        .context("source proof")?
        .iter()
        .find(|source| source["path"] == hit["citation"]["path"])
        .context("exact source")?;
    let origin = proof["origins"]
        .as_array()
        .context("origins")?
        .iter()
        .find(|origin| origin["id"] == source["origin_id"])
        .context("exact origin")?;
    assert_eq!(
        origin["project_ids"]
            .as_array()
            .context("project aliases")?
            .len(),
        2
    );
    let arguments = json!({"path":hit["citation"]["path"],"project_ids":origin["project_ids"],"index_ids":[f.indexes[0]],"revision":origin["publication"],"known_hash":hit["citation"]["content_hash"],"response_mode":"full"});
    let index = f.indexes[0].to_string();
    for selection in [window, open_selection(&[&index], &[])] {
        let mut arguments = arguments.clone();
        if matches!(selection, airk_core::service::ToolSelection::Open { .. }) {
            let ordinary_open=once(&f,selection.clone(),"airk_search",json!({"query":"boundaryneedle","mode":"lexical","path_prefix":"api/","verification":"report","response_mode":"full"})).await?;
            let open_body = body(&ordinary_open)?;
            let open_origins = open_body["source_verification"]["origins"]
                .as_array()
                .context("Open origins")?;
            assert_eq!(open_origins.len(), 1);
            assert_eq!(
                open_origins[0]["project_ids"],
                json!([]),
                "explicit Open must not invent Project attribution"
            );
            arguments["project_ids"] = open_origins[0]["project_ids"].clone();
        }
        let reply = once(&f, selection.clone(), "airk_file_info", arguments.clone()).await?;
        ensure!(
            reply["isError"] != true,
            "source narrowing refused: {reply}"
        );
        let combined = body(&reply)?;
        assert_eq!(combined["schema"], "airk.combined-read/1");
        let members = combined["members"].as_array().context("members")?;
        assert_eq!(members.len(), 1);
        assert_eq!(members[0]["index_id"], json!(f.indexes[0]));
        assert_eq!(members[0]["outcome"], "complete");
        let member = body(&members[0]["result"])?;
        let info = member
            .get("content")
            .filter(|value| value.is_object())
            .unwrap_or(&member);
        assert_eq!(info["content_hash"], hit["citation"]["content_hash"]);
        for foreign in [
            json!({"index_ids":[f.indexes[2]]}),
            json!({"project_ids":[f.projects[2]]}),
        ] {
            let mut changed = arguments.clone();
            for (key, value) in foreign.as_object().unwrap() {
                changed[key] = value.clone();
            }
            let rejected = once(&f, selection.clone(), "airk_file_info", changed).await?;
            assert_eq!(
                rejected["isError"], true,
                "foreign narrowing must refuse: {rejected}"
            );
        }
    }
    let legacy = once(
        &f,
        airk_core::service::ToolSelection::Project {
            binding: f.registrations[0].binding,
        },
        "airk_file_info",
        arguments,
    )
    .await?;
    assert_eq!(
        legacy["isError"], true,
        "legacy Project must not gain combined scope authority"
    );
    assert_eq!(
        body(&legacy)?["error"]["code"],
        "QUERY_SCOPE_CONFIGURATION_REQUIRED"
    );
    Ok(())
}
