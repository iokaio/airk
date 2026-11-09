// SPDX-License-Identifier: Apache-2.0
//! Actual native readers and CLI/MCP transports over an isolated authority-1
//! component-format fixture. This is not Genesis or installer qualification.
#[path = "common/http.rs"]
mod http;
#[path = "common/read_retry.rs"]
mod read_retry;
#[path = "combined_reads/single_member_retry.rs"]
mod single_member_retry;

#[path = "combined_reads/permanent_trim.rs"]
mod permanent_trim;

#[path = "combined_reads/changes_verification.rs"]
mod changes_verification;

#[cfg(feature = "local-inference")]
#[path = "combined_reads/reference_refresh.rs"]
mod reference_refresh;

#[cfg(feature = "local-inference")]
#[path = "combined_reads/retention.rs"]
mod retention;
#[path = "combined_reads/split.rs"]
mod split;

use airk_core::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{CatalogRevision, GrantId, IndexId, QueryProjectId, SourceRootId},
    service::{
        CombinedConnectRequest, ControlMethod, ControlResponse, ProjectRegistration,
        ScopedConnectRequest,
    },
};
use airk_store::service::{ProjectLease, registry::ServiceRegistry};
use anyhow::{Context, Result, ensure};
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
    fn start(command: Command) -> Result<Self> {
        Self::start_with_process_limit(command, 32)
    }

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

struct Fixture {
    owner: tempfile::TempDir,
    candidate: PathBuf,
    control: PathBuf,
    session: CombinedConnectRequest,
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
        )
    }
    fn with_shared_runtime() -> Result<Self> {
        Self::with_runtime_topology(true, 3, false, None, true)
    }
    fn with_runtime_topology(
        dedicated_is_indexed: bool,
        count: usize,
        helper: bool,
        removed_last_parent: Option<bool>,
        shared: bool,
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
        // Projects, indexes and grants below are physical-format fixtures.
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
        let caller = airk_cli::local_ipc::caller_identity()?;
        let host = framed_digest(
            b"airk/native-catalog-host/1",
            &[&serde_json::to_vec(&(namespace, caller))?],
        );
        let grant = GrantId::from_identity(Incarnation::from_bytes([60; 16]));
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
        tx.execute(
            "INSERT INTO index_access_grants(id,host,revision,revoked) VALUES(?1,?2,1,0)",
            params![grant.to_string(), host.to_string()],
        )?;
        let mut memberships = vec![(0, 0, "api"), (1, 0, "impl"), (0, 1, "api"), (2, 2, "")];
        memberships.extend((3..count).map(|index| (0, index, "api")));
        for (project, index, prefix) in memberships {
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
            tx.execute(
                "INSERT INTO index_access_memberships VALUES(?1,?2,?3,1,1,0,NULL)",
                params![
                    grant.to_string(),
                    projects[project].to_string(),
                    indexes[index].to_string()
                ],
            )?;
            tx.execute(
                "INSERT INTO index_access_scopes VALUES(?1,?2,?3,?4,'subtree',?5)",
                params![
                    grant.to_string(),
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
        let state = registry.catalog_state()?.context("catalog missing")?;
        let session = CombinedConnectRequest {
            schema: 1,
            context_key: Sha256Digest::from_bytes([70; 32]),
            namespace,
            expected_catalog_revision: state.revision,
            grant_id: grant,
            expected_grant_revision: CatalogRevision::INITIAL,
            project_ids: projects[..2].to_vec(),
            index_ids: None,
        };
        session.validate()?;
        Ok(Self {
            owner,
            candidate,
            control,
            session,
            registrations,
            indexes,
            projects,
        })
    }

    async fn start(&self) -> Result<OwnedProcess> {
        let log = self.owner.path().join("service.log");
        let mut command = Command::new(&self.candidate);
        command
            .args(["serve", "--runtime-dir"])
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
                    // These positive fixtures retain three tiny readers. An
                    // 8GiB host's balanced default admits only one resident;
                    // request capacity explicitly through the native contract.
                    let response = airk_cli::service_runtime::request(
                        &self.control,
                        ControlMethod::ConfigureResources {
                            configuration: airk_core::service_resources::ResourceConfiguration {
                                memory_bytes: Some(4 << 30),
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
                            && observation.configuration.memory_bytes == Some(4 << 30)
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

    async fn cli(&self, tool: &str, arguments: &Value) -> Result<Value> {
        let mut command = Command::new(&self.candidate);
        command
            .arg("--combined-session")
            .arg(serde_json::to_string(&self.session)?)
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
        let mut command = Command::new(&self.candidate);
        command
            .arg("--combined-session")
            .arg(serde_json::to_string(&self.session)?)
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
        let mut command = Command::new(&self.candidate);
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
        digest_body
            .as_object_mut()
            .context("delivery body must be an object")?
            .remove("delivery_observation");
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
                deadline <= 30_000,
                "allocated member deadlines exceeded the configured 30000 ms request deadline"
            );
        }
    }
    Ok(value)
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
            if let Some(deadline) = fields
                .get_mut("allocation")
                .and_then(Value::as_object_mut)
                .and_then(|allocation| allocation.get_mut("deadline_ms"))
            {
                // body() proved the sum stays within the configured request
                // deadline. This is the allowance remaining after metadata I/O.
                *deadline = json!(0);
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

fn parity_difference(label: &str, left: &Value, right: &Value) -> Result<Option<String>> {
    let a = stable(
        logical_comparison_copy(left)
            .with_context(|| format!("{label}: left logical reconstruction"))?,
    )?;
    let b = stable(
        logical_comparison_copy(right)
            .with_context(|| format!("{label}: right logical reconstruction"))?,
    )?;
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
}
impl McpSession {
    fn start(f: &Fixture) -> Result<Self> {
        Self::start_selected(f, &f.session)
    }
    fn start_selected(f: &Fixture, selected: &CombinedConnectRequest) -> Result<Self> {
        Self::start_selection(
            f,
            airk_core::service_http::HttpSelection::Combined {
                selection: selected.clone(),
            },
        )
    }
    fn start_scoped(f: &Fixture, selected: &ScopedConnectRequest) -> Result<Self> {
        Self::start_selection(
            f,
            airk_core::service_http::HttpSelection::Scoped {
                selection: selected.clone(),
            },
        )
    }
    fn start_selection(
        f: &Fixture,
        selection: airk_core::service_http::HttpSelection,
    ) -> Result<Self> {
        let mut session = Self {
            transport: http::HttpMcpSession::start(&f.control, selection)?,
        };
        let initialized = session.request(1, "initialize", json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"airk-combined-fixture","version":"1"}}))?;
        ensure!(
            initialized.get("error").is_none(),
            "initialize failed: {initialized}"
        );
        session
            .transport
            .send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))?;
        Ok(session)
    }
    fn request(&mut self, id: u64, method: &str, params: Value) -> Result<Value> {
        self.transport.request(id.try_into()?, method, params)
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
}

#[tokio::test]
async fn actual_native_cli_and_mcp_preserve_shared_index_scope_and_complete_results() -> Result<()>
{
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
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
    let mut mcp = McpSession::start(&f)?;
    let reply = mcp.call(2, "search", arguments)?;
    ensure!(reply.get("error").is_none(), "MCP failed: {reply}");
    assert_parity("full MCP result", &body(&native)?, &body(&reply["result"])?)?;
    let catalog = mcp.request(3, "tools/list", json!({}))?;
    let tools = catalog["result"]["tools"]
        .as_array()
        .context("MCP tools missing")?;
    let search = tools
        .iter()
        .find(|tool| tool["name"] == "search")
        .context("search catalog missing")?;
    let native_catalog = serde_json::to_value(
        backend
            .catalog()
            .into_iter()
            .find(|tool| tool.name == "airk_search")
            .context("native search catalog missing")?,
    )?;
    for selector in ["project_ids", "index_ids", "member_availability"] {
        assert_eq!(
            search["inputSchema"]["properties"][selector],
            native_catalog["inputSchema"]["properties"][selector],
            "input catalog drift: {selector}"
        );
    }
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
async fn combined_subset_refuses_ambiguous_followup_and_stale_or_revoked_grants() -> Result<()> {
    let f = Fixture::new(true)?;
    let _service = f.start().await?;
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
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
    let direct = airk_cli::project_client::scoped_tools(
        f.control.clone(),
        f.registrations[0].binding,
        ScopedConnectRequest {
            schema: 1,
            context_key: f.session.context_key,
            namespace: f.session.namespace,
            expected_catalog_revision: f.session.expected_catalog_revision,
            grant_id: f.session.grant_id,
            expected_grant_revision: f.session.expected_grant_revision,
            index_id: f.indexes[0],
            project_ids: vec![f.projects[0]],
        },
    )?;
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
        &member_body(&value, &value["members"][0])?,
        &body(&direct_result)?,
    )?;
    direct.close();
    direct.shutdown().await?;
    let untouched = ProjectLease::acquire(&f.control, f.registrations[1].binding)?;
    drop(untouched);
    let mut stale = f.session.clone();
    stale.expected_grant_revision = stale.expected_grant_revision.next()?;
    assert!(
        airk_cli::combined_frontend::resolve(None, Some(&f.control), &stale)
            .await
            .is_err(),
        "stale grant accepted"
    );
    backend.close();
    backend.shutdown().await?;
    drop(_service);
    // This isolated component-format mutation models a persisted revocation.
    // There are no in-flight reads: native delivery-race qualification belongs
    // to the owning supervisor tests, not this offline metadata alteration.
    let sql = Connection::open(f.control.join("service-1.4.sqlite3"))?;
    sql.execute(
        "UPDATE index_access_grants SET revoked=1,revision=revision+1 WHERE id=?1",
        [f.session.grant_id.to_string()],
    )?;
    assert!(
        airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session)
            .await
            .is_err(),
        "revoked grant accepted"
    );
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
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
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
        let wire = mcp.call(
            10 + offset as u64,
            tool.strip_prefix("airk_").unwrap(),
            arguments,
        )?;
        let native_body =
            body(&native).with_context(|| format!("{tool}: native observed result"))?;
        let cli_body = body(&cli).with_context(|| format!("{tool}: CLI result"))?;
        let mcp_body = body(&wire["result"]).with_context(|| format!("{tool}: MCP result"))?;
        // Equivalent error envelopes prove parity but cannot qualify this
        // positive read matrix. Every selected member must actually serve the
        // canonical handler; intentional member failures have separate tests.
        for (route, observed) in [
            ("native", &native_body),
            ("CLI", &cli_body),
            ("MCP", &mcp_body),
        ] {
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
                let direct = airk_cli::project_client::scoped_tools(
                    f.control.clone(),
                    f.registrations[member].binding,
                    ScopedConnectRequest {
                        schema: 1,
                        context_key: f.session.context_key,
                        namespace: f.session.namespace,
                        expected_catalog_revision: f.session.expected_catalog_revision,
                        grant_id: f.session.grant_id,
                        expected_grant_revision: f.session.expected_grant_revision,
                        index_id: f.indexes[member],
                        project_ids: if member == 0 {
                            f.projects[..2].to_vec()
                        } else {
                            vec![f.projects[0]]
                        },
                    },
                )
                .with_context(|| format!("capabilities: construct scoped member {member}"))?;
                let canonical = native_read(
                    direct.as_ref(),
                    tool,
                    &json!({"adapter":"markdown","rich":false,"response_mode":"full"}),
                )
                .await?;
                if let Some(difference) = parity_difference(
                    &format!("materialized capabilities member {member}"),
                    &materialized,
                    &body(&canonical).with_context(|| {
                        format!("capabilities member {member}: legacy scoped observation")
                    })?,
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
        if let Some(difference) =
            parity_difference(&format!("{tool} MCP"), &native_body, &mcp_body)?
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
    for (offset, tool) in ["airk_status", "airk_diagnostics"].into_iter().enumerate() {
        let arguments = json!({"project_ids":f.projects[..2],"index_ids":[f.indexes[0]],"member_availability":"require","response_mode":"full"});
        let native = native_read(backend.as_ref(), tool, &arguments).await?;
        let cli = f.cli(tool, &arguments).await?;
        let wire = mcp.call(
            100 + offset as u64,
            tool.strip_prefix("airk_").unwrap(),
            arguments,
        )?;
        for (route, envelope) in [("native", &native), ("CLI", &cli), ("MCP", &wire["result"])] {
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
        assert_parity(
            &format!("{tool} partial MCP"),
            &body(&native)?,
            &body(&wire["result"])?,
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
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
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
        // Use the actual MCP alias used by client hosts, retaining its JSON-RPC
        // envelope when checking the existing one-MiB transport bound.
        let wire = mcp.call(9000 + count as u64, "parse", request.clone())?;
        airk_core::index_catalog::encoded_size(&wire, 1024 * 1024)?;
        for (route, envelope) in [("native", &native), ("CLI", &cli), ("MCP", &wire["result"])] {
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
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
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
        let mut items = Vec::new();
        let mut cursors = std::collections::BTreeSet::new();
        loop {
            let native = native_read(backend.as_ref(), "airk_query_scope", &arguments).await?;
            let cli = f.cli("airk_query_scope", &arguments).await?;
            let wire = mcp.call(id, "query_scope", arguments.clone())?;
            id += 1;
            ensure!(
                native["isError"] != true
                    && cli["isError"] != true
                    && wire["result"]["isError"] != true,
                "{view}: scoped discovery returned an error; complete replies: {}",
                save_replies(view, &json!({"native":native,"cli":cli,"mcp":wire}))?.display()
            );
            let result = body(&native)?;
            assert_parity(&format!("query_scope {view} CLI"), &result, &body(&cli)?)?;
            assert_parity(
                &format!("query_scope {view} MCP"),
                &result,
                &body(&wire["result"])?,
            )?;
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
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
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
        let wire = mcp.call(id, "query_scope", args.clone())?;
        id += 1;
        ensure!(
            native["isError"] != true
                && cli["isError"] != true
                && wire["result"]["isError"] != true,
            "advice failed: {native} {cli} {wire}"
        );
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
        assert_parity("recommendations MCP", &result, &body(&wire["result"])?)?;
        let plan = json!({"schema":"airk.parse.request/1","mode":"execute","verification":"off","plan":{"schema":"airk.parse.plan/1","steps":[{"id":"advice","tool":"query_scope","args":args}]}});
        let parsed = mcp.call(id, "parse", plan)?;
        id += 1;
        ensure!(
            parsed["result"]["isError"] != true,
            "Workbench advisory plan failed: {parsed}"
        );
        let parsed_body = body(&parsed["result"])?;
        assert_parity(
            "recommendations Workbench plan",
            &result,
            &body(&parsed_body["steps"][0]["result"])?,
        )?;
        if let Some(choice) = args.get("selection") {
            let slash = json!({"schema":"airk.parse.request/1","mode":"execute","verification":"off","text":format!("/query_scope recommendations --selection '{choice}' --situation focused_task --bytes 65536")});
            let cli = f.cli("airk_parse", &slash).await?;
            let wire = mcp.call(id, "parse", slash)?;
            id += 1;
            for (route, reply) in [("CLI", cli), ("MCP", wire["result"].clone())] {
                ensure!(
                    reply["isError"] != true,
                    "{route} slash advice failed: {reply}"
                );
                let plan = body(&reply)?;
                assert_parity(
                    &format!("recommendations slash {route}"),
                    &result,
                    &body(&plan["steps"][0]["result"])?,
                )?;
            }
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
            let reply = mcp.call(id, name, example["arguments"].clone())?;
            id += 1;
            ensure!(
                reply["result"]["isError"] != true,
                "copied example rejected by actual transport: {reply}"
            );
        }
        // Original fixture judgments predate candidate output: this literal
        // matches permitted policy evidence, including in the one-index subset.
        let mut task = result["instructions"]["examples"][1]["arguments"].clone();
        task["query"] = json!("boundaryneedle");
        let reply = mcp.call(id, "search", task)?;
        id += 1;
        ensure!(
            reply["result"]["isError"] != true,
            "recommended task scope failed: {reply}"
        );
        let evidence = body(&reply["result"])?;
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
        let wire = mcp.call(id, "query_scope", args.clone())?;
        id += 1;
        ensure!(
            native["isError"] != true
                && cli["isError"] != true
                && wire["result"]["isError"] != true,
            "management advice failed: {native} {cli} {wire}"
        );
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
        assert_parity(
            "management recommendations MCP",
            &result,
            &body(&wire["result"])?,
        )?;
        let plan = json!({"schema":"airk.parse.request/1","mode":"execute","verification":"off","plan":{"schema":"airk.parse.plan/1","steps":[{"id":"advice","tool":"query_scope","args":args}]}});
        let parsed = mcp.call(id, "parse", plan)?;
        id += 1;
        ensure!(
            parsed["result"]["isError"] != true,
            "management advisory plan failed: {parsed}"
        );
        let parsed_body = body(&parsed["result"])?;
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
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
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
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
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

#[tokio::test]
#[ignore = "opt-in P7 measured replay: run after builds and competing tests finish"]
async fn owned_combined_benchmark_replays_one_two_four_and_thirty_two_indexes() -> Result<()> {
    let mut f = Fixture::with_indexes(true, 32)?;
    f.session.project_ids = f.projects.clone();
    let _service = f.start().await?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("workspace crates parent missing")?
        .parent()
        .context("workspace root missing")?;
    let output = match std::env::var_os("AIRK_COMBINED_BENCHMARK_OUTPUT") {
        Some(path) => std::path::absolute(PathBuf::from(path))?,
        None => repository.join("artifacts").join(format!(
            "p7-combined-{}",
            airk_kernel::service_broker::new_instance_id()?
        )),
    };
    fs::create_dir_all(output.parent().context("benchmark output has no parent")?)?;
    fs::create_dir(&output).context("benchmark output must be a new directory")?;
    let candidate = f.candidate.canonicalize()?;
    // This component fixture has no installed runtime. Only its native owner
    // submits private enrollment; the Python console surface is read-only.
    let enrollment_deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    let health = tokio::time::timeout_at(
        enrollment_deadline,
        airk_cli::service_runtime::request(&f.control, ControlMethod::Health),
    )
    .await??;
    let ControlResponse::Health {
        observation: health,
    } = health
    else {
        anyhow::bail!("combined benchmark native health unavailable");
    };
    let mut enrolled = Vec::new();
    let health_executable: airk_core::identity::Sha256Digest = health
        .executable_sha256
        .parse()
        .context("combined benchmark native health executable digest invalid")?;
    for count in [1, 2, 4, 32] {
        let mut selection = f.session.clone();
        selection.index_ids = Some(f.indexes[..count].to_vec());
        let id = airk_kernel::service_broker::new_instance_id()?;
        let response = tokio::time::timeout_at(
            enrollment_deadline,
            airk_cli::service_runtime::request(
                &f.control,
                ControlMethod::HttpEnroll {
                    connection_id: id,
                    client: airk_core::usage::ClientFamily::Unknown,
                    selection: Box::new(airk_core::service_http::HttpSelection::Combined {
                        selection: selection.clone(),
                    }),
                    explorer_descriptor: None,
                    reattach_connection_id: None,
                },
            ),
        )
        .await??;
        let ControlResponse::HttpConnection { connection } = response else {
            anyhow::bail!("combined benchmark native enrollment refused");
        };
        connection.validate()?;
        let airk_core::service_http::HttpSelection::Combined {
            selection: observed,
        } = &connection.selection
        else {
            anyhow::bail!("combined benchmark enrollment changed selection kind");
        };
        ensure!(
            observed.expected_catalog_revision >= selection.expected_catalog_revision,
            "combined benchmark enrollment moved catalog backwards"
        );
        selection.expected_catalog_revision = observed.expected_catalog_revision;
        ensure!(
            selection == *observed
                && connection.connection_id == id
                && connection.endpoint.service_instance == health.instance
                && connection.endpoint.user == health.user
                && health
                    .catalog_clock
                    .is_some_and(|clock| connection.endpoint.namespace == clock.namespace)
                && connection.endpoint.executable_sha256 == health_executable,
            "combined benchmark enrollment changed native authority or owner"
        );
        enrolled.push(connection);
    }
    let connections_file = f.control.join("http-1").join(format!(
        "benchmark-connections-{}.json",
        airk_kernel::service_broker::new_instance_id()?
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // Windows inherits the existing native control/http-1 owner-only ACL.
    use std::io::Write;
    options
        .open(&connections_file)?
        .write_all(&serde_json::to_vec(&json!({
            "schema":1,"client":"unknown","connections":enrolled
        }))?)?;
    let configuration = json!({
        "schema":2,"transport":"https","command":candidate,"args":[],
        "control":f.control,"selection":{"kind":"combined","selection":f.session},
        "connections_file":connections_file,
        "launcher_sha256":airk_cli::runtime_pair::digest_file(&candidate)?,
        "expected_version":env!("CARGO_PKG_VERSION"),"route":"isolated-approved-runtime",
        "cwd":repository
    });
    let mut indexes = Vec::new();
    for (n, registration) in f.registrations.iter().enumerate() {
        let mut inventory = Vec::new();
        // Match the existing runner's sorted Path inventory (path components,
        // rather than slash-joined byte ordering).
        for directory in ["api", "api-other", "impl", "private"] {
            let relative = format!("{directory}/policy.md");
            let path = Path::new(&registration.root).join(&relative);
            inventory.push(json!({"path":relative,"bytes":fs::metadata(&path)?.len(),
                "sha256":airk_cli::runtime_pair::digest_file(&path)?}));
        }
        indexes.push(
            json!({"index_id":f.indexes[n],"source_root":registration.root,"inventory":inventory}),
        );
    }
    let cases = [1, 2, 4, 32]
        .into_iter()
        .map(|count| {
            let expected = (0..count).map(|n| json!({
            "index_id":f.indexes[n],"path":"api/policy.md",
            "source_root_id":SourceRootId::from_identity(Incarnation::from_bytes([50+n as u8;16])),
            "project_ids":[if n==2 {f.projects[2]} else {f.projects[0]}]
        })).collect::<Vec<_>>();
            json!({"project_ids":f.projects,"index_ids":f.indexes[..count],
            "arguments":{"query":"boundaryneedle","mode":"lexical","path_prefix":"api/",
                "limit":count,"max_bytes":2048,"verification":"off"},"expected":expected})
        })
        .collect::<Vec<_>>();
    let scenario =
        json!({"schema":"airk.combined-investigation/1","indexes":indexes,"cases":cases});
    let config_path = output.join("configuration.json");
    let scenario_path = output.join("scenario.json");
    fs::write(&config_path, serde_json::to_vec_pretty(&configuration)?)?;
    fs::write(&scenario_path, serde_json::to_vec_pretty(&scenario)?)?;
    let samples = std::env::var("AIRK_COMBINED_BENCHMARK_SAMPLES").unwrap_or_else(|_| "30".into());
    let samples: usize = samples
        .parse()
        .context("benchmark samples must be an integer")?;
    ensure!(
        (1..=1000).contains(&samples),
        "benchmark sample count outside existing runner bound"
    );
    let interpreter = std::env::var_os("PYTHON").unwrap_or_else(|| "python".into());
    let mut runner = Command::new(interpreter);
    runner
        .current_dir(repository)
        .arg(repository.join("scripts/benchmark-mcp-investigation.py"))
        .arg("combined")
        .arg("--configuration")
        .arg(config_path)
        .arg("--scenario")
        .arg(scenario_path)
        .arg("--output")
        .arg(output.join("measurement"))
        .arg("--samples")
        .arg(samples.to_string())
        .args(["--fanouts", "1", "2", "4", "32", "--timeout", "30"])
        .stdin(Stdio::null())
        .stdout(Stdio::from(fs::File::create(
            output.join("runner.stdout.log"),
        )?))
        .stderr(Stdio::from(fs::File::create(
            output.join("runner.stderr.log"),
        )?));
    let mut runner = OwnedProcess::start(runner)?;
    // Explicit finite campaign bound: at most four cases, each with the existing
    // thirty-second request timeout, plus fixture/frontend shutdown allowance.
    let deadline = Instant::now() + Duration::from_secs((samples as u64 * 4 + 4) * 30 + 120);
    let status = loop {
        if let Some(status) = runner.child.try_wait()? {
            break status;
        }
        ensure!(
            Instant::now() < deadline,
            "benchmark runner exceeded its owned campaign deadline; logs: {}",
            output.display()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    drop(runner);
    f.assert_sources_unchanged()?;
    let report_path = output.join("measurement/report.json");
    let report: Value =
        serde_json::from_slice(&fs::read(&report_path).with_context(|| {
            format!("benchmark did not retain its report: {}", output.display())
        })?)?;
    assert_eq!(
        report["source_unchanged"], true,
        "fixture changed during replay: {report}"
    );
    ensure!(
        status.success() && report["passed"] == true,
        "bounded combined campaign retained a failure; inspect {}",
        report_path.display()
    );
    eprintln!("P7 combined measurement: {}", report_path.display());
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
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
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
    let selected = ScopedConnectRequest {
        schema: 1,
        context_key: f.session.context_key,
        namespace: f.session.namespace,
        expected_catalog_revision: f.session.expected_catalog_revision,
        grant_id: f.session.grant_id,
        expected_grant_revision: f.session.expected_grant_revision,
        index_id: f.indexes[0],
        project_ids: vec![f.projects[0]],
    };
    let backend = airk_cli::project_client::scoped_tools(
        f.control.clone(),
        f.registrations[0].binding,
        selected.clone(),
    )?;
    let mut mcp = McpSession::start_scoped(&f, &selected)?;
    for (id, args) in (2200..).zip([
        json!({"view":"projects"}),
        json!({"view":"indexes","project_id":f.projects[0]}),
        json!({"view":"sources","project_id":f.projects[0],"index_id":f.indexes[0]}),
        json!({"view":"recommendations"}),
    ]) {
        let direct = native_read(backend.as_ref(), "airk_query_scope", &args).await?;
        let canonical = body(&direct)?;
        assert_eq!(canonical["grant"]["id"], json!(selected.grant_id));
        assert_eq!(canonical["mode"], "frozen_single_index");
        assert_eq!(
            canonical["observed_selection"],
            json!({"project_ids":selected.project_ids,"index_ids":[selected.index_id]})
        );
        let wire = mcp.call(id, "query_scope", args.clone())?;
        assert_parity("installed scope MCP", &canonical, &body(&wire["result"])?)?;
        for (tool, arguments) in [
            ("airk_query_scope", args.clone()),
            (
                "airk_parse",
                json!({"schema":"airk.parse.request/1",
            "mode":"execute","verification":"off","plan":{"steps":[{"id":"scope","tool":"query_scope","args":args}]}}),
            ),
        ] {
            let mut command = Command::new(&f.candidate);
            command
                .arg("--catalog-session")
                .arg(serde_json::to_string(&selected)?)
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
