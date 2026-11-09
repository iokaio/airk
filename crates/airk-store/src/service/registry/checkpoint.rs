// SPDX-License-Identifier: Apache-2.0
//! Read-only references to committed native state, not a second recovery store.
//! The installer establishes writer exclusion separately before a final capture.
use super::{ServiceRegistry, catalog};
use airk_core::{
    identity::{Incarnation, Sha256Digest, framed_digest},
    service::{JobRecord, JobState},
};
use anyhow::{Result, ensure};
use rusqlite::{Connection, types::ValueRef};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const MAX_SNAPSHOT_BYTES: usize = 448 * 1024;
const MAX_TABLES: usize = 128;
pub(crate) const MAX_ROWS: usize = 32_768;
const MAX_ROW_BYTES: usize = 65_536;

/// Capacity refusal means no complete checkpoint was produced. It is not retry
/// permission and must be surfaced before stopping a predecessor when possible.
#[derive(Debug)]
pub struct CheckpointLimit(pub &'static str);
impl std::fmt::Display for CheckpointLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "checkpoint_complete_roster_limit: {}", self.0)
    }
}
impl std::error::Error for CheckpointLimit {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableCheckpoint {
    pub name: String,
    pub schema_digest: Sha256Digest,
    /// Physical ownership and cached telemetry are not restart-restorable state.
    pub ephemeral_or_observation_only: bool,
    pub rows: Option<usize>,
    /// Digest of every complete, type-tagged row, sorted by its canonical digest.
    pub rows_digest: Option<Sha256Digest>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestCheckpoint {
    pub namespace: Incarnation,
    pub epoch: u64,
    pub nonce: Incarnation,
    pub caller: Incarnation,
    pub job: Sha256Digest,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobCheckpoint {
    pub record: JobRecord,
    pub configuration_digest: Option<Sha256Digest>,
    pub original_engine: Option<Sha256Digest>,
    pub configuration_kind: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnfinishedWork {
    pub owner: String,
    pub id: String,
    pub kind: String,
    pub state: String,
    pub original_engine: Option<Sha256Digest>,
    pub payload_digest: Option<Sha256Digest>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetainedIntentDisposition {
    /// New work is admitted only by its ordinary native owner after commit.
    /// No accepted request or old engine authority is transferred.
    OrdinaryAdmissionAfterCommit,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum PublicationState {
    NotInitialized,
    UnavailableStorage,
    Empty,
    Current {
        head: crate::current::CurrentHead,
        token: crate::current::CurrentToken,
    },
    Legacy {
        revision: crate::PersistentRevision,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationCheckpoint {
    pub binding: airk_core::service::ProjectBinding,
    pub registered: bool,
    pub publication: PublicationState,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogIndexCheckpoint {
    pub id: airk_core::index_catalog::IndexId,
    pub binding: airk_core::service::ProjectBinding,
    pub retired: bool,
    pub source_import_state: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointSnapshot {
    pub schema: u32,
    pub control: PathBuf,
    /// None explicitly identifies an absent, never-initialized registry.
    pub registry_schema: Option<u32>,
    pub namespace: Option<Incarnation>,
    pub catalog_state: Option<catalog::CatalogState>,
    pub project_bindings: Vec<airk_core::service::ProjectBinding>,
    pub publications: Vec<PublicationCheckpoint>,
    pub indexes: Vec<CatalogIndexCheckpoint>,
    pub tables: Vec<TableCheckpoint>,
    /// Schema 1 expands all history; schema 2 expands only pending/running jobs.
    /// Complete history remains authenticated by the table commitments.
    pub jobs: Vec<JobCheckpoint>,
    /// Schema 2 expands only requests attached to the expanded accepted jobs.
    pub requests: Vec<RequestCheckpoint>,
    pub unfinished: Vec<UnfinishedWork>,
    pub retained_intent_contract: u32,
    pub retained_intent_disposition: RetainedIntentDisposition,
    pub retained_intents: Vec<UnfinishedWork>,
    pub complete: bool,
}
impl CheckpointSnapshot {
    pub fn digest(&self) -> Result<Sha256Digest> {
        let bytes = serde_json::to_vec(self)?;
        ensure!(
            bytes.len() <= MAX_SNAPSHOT_BYTES,
            CheckpointLimit("serialized snapshot")
        );
        Ok(framed_digest(b"airk/checkpoint-native-state/1", &[&bytes]))
    }
    /// No engine migration adapter is implicit. Original accepted requests,
    /// configuration and receipts are retained byte-for-byte by their owners.
    pub fn require_compatible(
        &self,
        predecessor_engine: Option<Sha256Digest>,
        candidate_engine: Sha256Digest,
    ) -> Result<()> {
        ensure!(
            matches!(self.schema, 1 | 2) && self.complete,
            "incomplete checkpoint snapshot"
        );
        self.digest()?;
        self.require_work_compatible(predecessor_engine, candidate_engine)
    }

    fn require_work_compatible(
        &self,
        predecessor_engine: Option<Sha256Digest>,
        candidate_engine: Sha256Digest,
    ) -> Result<()> {
        self.require_retained_intents()?;
        ensure!(
            predecessor_engine == Some(candidate_engine) || self.unfinished.is_empty(),
            "checkpoint_job_compatibility_blocked: {} unfinished accepted operations require completion under their original engine or explicit reviewed recovery; no automatic engine adapter is installed",
            self.unfinished.len()
        );
        Ok(())
    }

    fn require_retained_intents(&self) -> Result<()> {
        ensure!(
            self.retained_intent_contract == 1
                && self.retained_intents.iter().all(|intent| {
                    intent.original_engine.is_none()
                        && intent.state == "pending"
                        && intent.payload_digest.is_some()
                        && intent
                            .id
                            .parse::<airk_core::service::ProjectBinding>()
                            .is_ok()
                        && matches!(
                            (intent.owner.as_str(), intent.kind.as_str()),
                            ("service_background", "source_update" | "semantic_update")
                                | ("service_setup_pending", "model_setup")
                                | ("service_watch", "source_reconciliation")
                        )
                }),
            "unsupported checkpoint desired-state intent contract"
        );
        Ok(())
    }
}

/// One stable registry read transaction and pinned metadata-only collection reads;
/// never initializes or migrates a registry/index, reads source files, grants
/// work or claims writer quiescence. Final capture requires external exclusion.
pub fn snapshot(control: &Path) -> Result<CheckpointSnapshot> {
    snapshot_for_schema(control, 2)
}

/// Reobserve immutable predecessor packets and interrupted journals without
/// changing their original encoding or digest. New captures use schema 2.
pub fn snapshot_for_schema(control: &Path, schema: u32) -> Result<CheckpointSnapshot> {
    ensure!(
        matches!(schema, 1 | 2),
        "unsupported checkpoint snapshot schema"
    );
    snapshot_with_publications(control, true, schema)
}

/// Early complete registry-capacity observation while accepted index writers
/// may still run. It returns no partial checkpoint and opens no collection.
/// The 4 KiB/publication reserve is an estimate: the actual full manifest bound
/// must still pass after durable worker acknowledgment and before old-owner stop.
pub fn preflight_capacity(control: &Path) -> Result<()> {
    preflight_snapshot(control).map(|_| ())
}

/// Registry-only live observation. This cannot be serialized or used as a final
/// checkpoint: collection publications have not been inspected or fenced.
pub struct PreflightSnapshot(CheckpointSnapshot);

impl PreflightSnapshot {
    pub fn require_compatible(
        &self,
        predecessor_engine: Option<Sha256Digest>,
        candidate_engine: Sha256Digest,
    ) -> Result<()> {
        self.0
            .require_work_compatible(predecessor_engine, candidate_engine)
    }
}

/// Observe capacity and accepted-work compatibility without opening collections
/// still owned by the running predecessor. Final capture remains mandatory.
pub fn preflight_snapshot(control: &Path) -> Result<PreflightSnapshot> {
    let state = snapshot_with_publications(control, false, 2)?;
    let bytes = serde_json::to_vec(&state)?.len();
    let reserve = state
        .project_bindings
        .len()
        .checked_mul(4096)
        .and_then(|reserve| reserve.checked_add(bytes))
        .ok_or(CheckpointLimit("estimated publication metadata"))?;
    ensure!(
        reserve <= MAX_SNAPSHOT_BYTES,
        CheckpointLimit("estimated publication metadata")
    );
    Ok(PreflightSnapshot(state))
}

/// Retry only an observation against native lease or SQLite read contention.
/// Size limits, incomplete publication phases, corruption and I/O failures do
/// not gain retry permission from text that happens to contain "busy".
pub fn is_read_contention(error: &anyhow::Error) -> bool {
    super::is_read_contention(error)
        || matches!(
            error.downcast_ref::<std::fs::TryLockError>(),
            Some(std::fs::TryLockError::WouldBlock)
        )
}

fn snapshot_with_publications(
    control: &Path,
    publications: bool,
    schema: u32,
) -> Result<CheckpointSnapshot> {
    let control = control.canonicalize()?;
    let mut result = CheckpointSnapshot {
        schema,
        control: control.clone(),
        registry_schema: None,
        namespace: None,
        catalog_state: None,
        project_bindings: vec![],
        publications: vec![],
        indexes: vec![],
        tables: vec![],
        jobs: vec![],
        requests: vec![],
        unfinished: vec![],
        retained_intent_contract: 1,
        retained_intent_disposition: RetainedIntentDisposition::OrdinaryAdmissionAfterCommit,
        retained_intents: vec![],
        complete: publications,
    };
    let database = control.join("service-1.4.sqlite3");
    if !database.try_exists()? {
        for name in [
            "service-1.4.owner.json",
            "service-1.4.sqlite3-wal",
            "service-1.4.sqlite3-shm",
            "service-1.4.sqlite3-journal",
        ] {
            ensure!(
                !control.join(name).try_exists()?,
                "checkpoint registry missing with retained native state"
            );
        }
        result.digest()?;
        return Ok(result);
    }
    let until = Instant::now() + Duration::from_secs(5);
    let registry = ServiceRegistry::open_saved_before(&control, Some(until))?;
    let tx = registry.connection.unchecked_transaction()?;
    let connection: &Connection = &tx;
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    result.registry_schema = Some(version);
    result.namespace = Some(registry.incarnation()?);
    result.catalog_state = registry.catalog_state()?;
    result.project_bindings = registry.bindings()?;
    let mut statement = connection
        .prepare("SELECT name,sql FROM sqlite_schema WHERE type='table' ORDER BY name LIMIT 129")?;
    let tables = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    ensure!(tables.len() <= MAX_TABLES, CheckpointLimit("table count"));
    let mut total_rows = 0;
    for (name, ddl) in tables {
        ensure!(
            Instant::now() < until,
            CheckpointLimit("observation deadline")
        );
        ensure!(
            !name.is_empty()
                && name.len() <= 128
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
            "invalid checkpoint native table name"
        );
        ensure!(ddl.len() <= MAX_ROW_BYTES, CheckpointLimit("table schema"));
        if matches!(
            name.as_str(),
            "service_provider_drains"
                | "service_progress"
                | "service_inspections"
                | "service_events"
                | "sqlite_sequence"
        ) {
            result.tables.push(TableCheckpoint {
                name,
                schema_digest: framed_digest(b"airk/checkpoint-native-ddl/1", &[ddl.as_bytes()]),
                ephemeral_or_observation_only: true,
                rows: None,
                rows_digest: None,
            });
            continue;
        }
        let mut statement =
            connection.prepare(&format!("SELECT * FROM \"{name}\" LIMIT {}", MAX_ROWS + 1))?;
        let columns = statement.column_count();
        ensure!(columns <= 64, CheckpointLimit("table column count"));
        let mut rows = statement.query([])?;
        let mut hashes = Vec::new();
        while let Some(row) = rows.next()? {
            total_rows += 1;
            ensure!(
                total_rows <= MAX_ROWS,
                CheckpointLimit("complete native row count")
            );
            ensure!(
                Instant::now() < until,
                CheckpointLimit("observation deadline")
            );
            let mut bytes = Vec::new();
            for index in 0..columns {
                let (tag, value) = match row.get_ref(index)? {
                    ValueRef::Null => (0, vec![]),
                    ValueRef::Integer(value) => (1, value.to_le_bytes().to_vec()),
                    ValueRef::Real(value) => (2, value.to_bits().to_le_bytes().to_vec()),
                    ValueRef::Text(value) => (3, value.to_vec()),
                    ValueRef::Blob(value) => (4, value.to_vec()),
                };
                ensure!(
                    bytes.len() + value.len() + 9 <= MAX_ROW_BYTES,
                    CheckpointLimit("complete row bytes")
                );
                bytes.push(tag);
                bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
                bytes.extend(value);
            }
            hashes.push(framed_digest(b"airk/checkpoint-native-row/1", &[&bytes]).to_string());
        }
        hashes.sort();
        result.tables.push(TableCheckpoint {
            name,
            schema_digest: framed_digest(b"airk/checkpoint-native-ddl/1", &[ddl.as_bytes()]),
            ephemeral_or_observation_only: false,
            rows: Some(hashes.len()),
            rows_digest: Some(framed_digest(
                b"airk/checkpoint-native-table/1",
                &[&serde_json::to_vec(&hashes)?],
            )),
        });
    }
    capture_jobs(&registry, &mut result)?;
    capture_requests(connection, &mut result)?;
    capture_other_work(connection, &mut result)?;
    capture_indexes(connection, &mut result)?;
    if publications {
        capture_publications(&registry, &mut result, until)?;
    }
    tx.commit()?;
    result.digest()?;
    Ok(result)
}

fn capture_indexes(connection: &Connection, snapshot: &mut CheckpointSnapshot) -> Result<()> {
    if !table(snapshot, "index_definitions") {
        return Ok(());
    }
    let mut statement = connection.prepare(
        "SELECT id,legacy_binding,tombstone,source_import_state FROM index_definitions ORDER BY id",
    )?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        ensure!(
            snapshot.indexes.len() < airk_core::index_catalog::MAX_DEFINITIONS,
            CheckpointLimit("catalog index count")
        );
        let index = CatalogIndexCheckpoint {
            id: row.get::<_, String>(0)?.parse()?,
            binding: row.get::<_, String>(1)?.parse()?,
            retired: row.get(2)?,
            source_import_state: row.get(3)?,
        };
        ensure!(
            snapshot.project_bindings.contains(&index.binding),
            "checkpoint catalog index retained backing is missing"
        );
        snapshot.indexes.push(index);
    }
    Ok(())
}

fn capture_publications(
    registry: &ServiceRegistry,
    snapshot: &mut CheckpointSnapshot,
    until: Instant,
) -> Result<()> {
    for binding in &snapshot.project_bindings {
        ensure!(
            Instant::now() < until,
            CheckpointLimit("publication observation deadline")
        );
        let saved = registry.retained_project(*binding)?;
        let registration = saved.registration;
        let registered = if registry.registration_state {
            registry.connection.query_row(
                "SELECT registered FROM projects WHERE binding=?1",
                [binding.to_string()],
                |row| row.get::<_, bool>(0),
            )?
        } else {
            true
        };
        let data = Path::new(&registration.data_root);
        let logical = data.join("collections-1.4").join(&registration.collection);
        let pending_adoption =
            snapshot.indexes.iter().any(|index| {
                index.binding == *binding
                    && !index.retired
                    && index.source_import_state == "pending_metadata"
            }) && catalog::setup::uninitialized_legacy_adoption(registry, &registration)?;
        let publication = if pending_adoption {
            PublicationState::NotInitialized
        } else if !data.try_exists()? || !logical.try_exists()? {
            let setup_pending = if table(snapshot, "service_setup_pending") {
                registry.project_setup_pending(*binding)?
            } else {
                registry.connection.query_row("SELECT EXISTS(SELECT 1 FROM service_jobs WHERE project=?1 AND kind='model_setup' AND state IN('pending','running'))", [binding.to_string()], |row| row.get::<_, bool>(0))?
            };
            if setup_pending {
                PublicationState::NotInitialized
            } else {
                PublicationState::UnavailableStorage
            }
        } else {
            let collection = crate::service::collection::Collection::existing(
                data,
                &registration.root,
                &registration.collection,
            )?;
            // Both manifest and object gates already exist. No create/open of
            // a writable index and no model/source provisioning is permitted.
            let selected = collection.select_existing_locked()?;
            let store = &selected.store;
            if store.directory().join("index.sqlite3").try_exists()? {
                ensure!(
                    !store.directory().join("current.sqlite3").try_exists()?,
                    "mixed checkpoint publication formats"
                );
                store.ensure_index_ready()?;
                let database = store.directory().join("index.sqlite3");
                crate::managed_collection::reject_file_aliases(
                    &database,
                    &std::fs::symlink_metadata(&database)?,
                )?;
                let revision = crate::sqlite::persistent_revision(store.directory())?
                    .ok_or_else(|| anyhow::anyhow!("legacy checkpoint publication missing"))?;
                ensure!(
                    revision.root == registration.root
                        && revision.collection == registration.collection,
                    "foreign legacy checkpoint publication"
                );
                PublicationState::Legacy { revision }
            } else if let Some(head) =
                store.checkpoint_current_head(&registration.root, &registration.collection)?
            {
                let token = head.token()?;
                PublicationState::Current { head, token }
            } else {
                PublicationState::Empty
            }
        };
        snapshot.publications.push(PublicationCheckpoint {
            binding: *binding,
            registered,
            publication,
        });
    }
    Ok(())
}

fn table(snapshot: &CheckpointSnapshot, name: &str) -> bool {
    snapshot.tables.iter().any(|t| t.name == name)
}
fn capture_jobs(registry: &ServiceRegistry, snapshot: &mut CheckpointSnapshot) -> Result<()> {
    let mut statement = registry
        .connection
        .prepare("SELECT id FROM service_jobs ORDER BY seq")?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(ids.len() <= 1024, CheckpointLimit("job count"));
    for id in ids {
        let record = registry.job(id.parse()?)?;
        let mut job = JobCheckpoint {
            record,
            configuration_digest: None,
            original_engine: None,
            configuration_kind: None,
        };
        if table(snapshot, "service_configuration") {
            use rusqlite::OptionalExtension;
            let payload: Option<Vec<u8>> = registry
                .connection
                .query_row(
                    "SELECT substr(payload,1,16641) FROM service_configuration WHERE job=?1",
                    [&id],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(payload) = payload {
                ensure!(
                    payload.len() <= 16640,
                    CheckpointLimit("configuration bytes")
                );
                let work: airk_core::service_configuration::ConfigurationWork =
                    serde_json::from_slice(&payload)?;
                work.request.validate()?;
                job.original_engine = Some(work.engine);
                job.configuration_kind =
                    serde_json::to_value(&work.request.configuration)?["operation"]
                        .as_str()
                        .map(str::to_owned);
                job.configuration_digest = Some(framed_digest(
                    b"airk/checkpoint-configuration/1",
                    &[&payload],
                ));
            }
        }
        if matches!(job.record.state, JobState::Pending | JobState::Running) {
            snapshot.unfinished.push(UnfinishedWork {
                owner: "service_jobs".into(),
                id,
                kind: job
                    .configuration_kind
                    .clone()
                    .unwrap_or_else(|| format!("{:?}", job.record.kind)),
                state: format!("{:?}", job.record.state),
                original_engine: job.original_engine,
                payload_digest: job.configuration_digest,
            });
        }
        // Parse and validate every retained job/configuration before reducing
        // duplicate expansion. The complete table digest still covers them all.
        if snapshot.schema == 1 || matches!(job.record.state, JobState::Pending | JobState::Running)
        {
            snapshot.jobs.push(job);
        }
    }
    Ok(())
}
fn capture_requests(connection: &Connection, snapshot: &mut CheckpointSnapshot) -> Result<()> {
    let mut statement = connection.prepare("SELECT namespace,epoch,nonce,caller,job FROM service_requests ORDER BY namespace,epoch,nonce,caller")?;
    let mut rows = statement.query([])?;
    let mut count = 0;
    while let Some(row) = rows.next()? {
        count += 1;
        ensure!(count <= 4096, CheckpointLimit("request count"));
        let request = RequestCheckpoint {
            namespace: row.get::<_, String>(0)?.parse()?,
            epoch: u64::try_from(row.get::<_, i64>(1)?)?,
            nonce: row.get::<_, String>(2)?.parse()?,
            caller: row.get::<_, String>(3)?.parse()?,
            job: row.get::<_, String>(4)?.parse()?,
        };
        if snapshot.schema == 1 || snapshot.jobs.iter().any(|job| job.record.id == request.job) {
            snapshot.requests.push(request);
        }
    }
    Ok(())
}
fn capture_other_work(connection: &Connection, snapshot: &mut CheckpointSnapshot) -> Result<()> {
    capture_operation_receipts(connection, snapshot)?;
    // Unknown phases stay blocked. These are observations, never transition or
    // retry authority; the original Store owners retain every receipt/payload.
    for (owner, sql) in [
        (
            "service_plans",
            "SELECT id,'plan',phase,engine,request FROM service_plans WHERE phase NOT IN('complete','failed','canceled') ORDER BY id",
        ),
        (
            "service_lifecycle",
            "SELECT id,'lifecycle',phase,NULL,request FROM service_lifecycle WHERE phase NOT IN('complete','abandoned') ORDER BY id",
        ),
        (
            "service_background",
            "SELECT project,'source_update','pending',NULL,CAST(revision AS BLOB) FROM service_background WHERE source_pending=1 UNION ALL SELECT project,'semantic_update','pending',NULL,CAST(revision AS BLOB) FROM service_background WHERE semantic_pending=1 ORDER BY 1,2",
        ),
        (
            "service_setup_pending",
            "SELECT project,'model_setup','pending',NULL,CAST(project AS BLOB) FROM service_setup_pending ORDER BY project",
        ),
        (
            "service_watch",
            "SELECT project,'source_reconciliation','pending',NULL,roots FROM service_watch WHERE requested>indexed ORDER BY project",
        ),
    ] {
        if !table(snapshot, owner) {
            continue;
        }
        let mut statement = connection.prepare(sql)?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            ensure!(
                snapshot.unfinished.len() + snapshot.retained_intents.len() < 4096,
                CheckpointLimit("unfinished operation count")
            );
            let engine: Option<String> = row.get(3)?;
            let payload: Vec<u8> = row.get(4)?;
            ensure!(
                payload.len() <= MAX_ROW_BYTES,
                CheckpointLimit("operation payload")
            );
            let work = UnfinishedWork {
                owner: owner.into(),
                id: row.get(0)?,
                kind: row.get(1)?,
                state: row.get(2)?,
                original_engine: engine.map(|id| id.parse()).transpose()?,
                payload_digest: Some(framed_digest(b"airk/checkpoint-operation/1", &[&payload])),
            };
            if matches!(
                owner,
                "service_background" | "service_setup_pending" | "service_watch"
            ) {
                ensure!(
                    work.original_engine.is_none(),
                    "assigned work cannot become a retained desired-state intent"
                );
                snapshot.retained_intents.push(work);
            } else {
                snapshot.unfinished.push(work);
            }
        }
    }
    Ok(())
}

mod receipts;
use receipts::capture_operation_receipts;
#[cfg(test)]
pub(crate) use receipts::terminal as terminal_operation_receipt;

#[cfg(test)]
mod history_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::service::{JobKind, ProjectRegistration, RequestKey};

    #[test]
    fn absent_registry_is_explicit_and_does_not_initialize_state() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let state = snapshot(directory.path())?;
        assert!(state.registry_schema.is_none());
        assert!(state.namespace.is_none());
        assert!(state.complete);
        assert_eq!(std::fs::read_dir(directory.path())?.count(), 0);
        Ok(())
    }

    #[test]
    fn live_capacity_preflight_does_not_open_unacknowledged_collection_state() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let control = directory.path().join("control");
        std::fs::create_dir(&control)?;
        let mut registry = ServiceRegistry::open(&control, Incarnation::from_bytes([7; 16]))?;
        let data = directory.path().join("data");
        let project = ProjectRegistration::new(
            directory
                .path()
                .join("missing-source")
                .to_string_lossy()
                .into_owned(),
            data.to_string_lossy().into_owned(),
            "fixture".into(),
        )?;
        registry.register(&project)?;
        // An in-progress worker has not yet installed its complete native owner.
        let logical = data.join("collections-1.4").join("fixture");
        std::fs::create_dir_all(&logical)?;
        preflight_capacity(&control)?;
        assert!(
            snapshot(&control).is_err(),
            "final capture must validate the publication owner"
        );
        assert_eq!(std::fs::read_dir(&logical)?.count(), 0);
        Ok(())
    }

    #[test]
    fn checkpoint_retry_classification_retains_native_error_identity() {
        let held = anyhow::Error::new(std::fs::TryLockError::WouldBlock)
            .context("owned collection is applying");
        assert!(is_read_contention(&held));
        let sqlite = anyhow::Error::new(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None,
        ));
        assert!(is_read_contention(&sqlite));
        assert!(!is_read_contention(&anyhow::anyhow!("busy current writer")));
        assert!(!is_read_contention(&anyhow::Error::new(CheckpointLimit(
            "observation deadline"
        ))));
        assert!(!is_read_contention(
            &std::io::Error::new(std::io::ErrorKind::PermissionDenied, "busy").into()
        ));
    }

    #[test]
    fn checkpoint_current_observer_never_creates_missing_gate_metadata() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let logical = directory.path().join("collections-1.4").join("fixture");
        let slot = logical
            .join("incarnations")
            .join(Incarnation::from_bytes([9; 16]).to_string());
        std::fs::create_dir_all(&slot)?;
        let store = crate::FileStore::new(&slot);
        assert!(
            store
                .checkpoint_current_head("/missing-source-is-not-opened", "fixture")
                .is_err()
        );
        assert_eq!(std::fs::read_dir(&slot)?.count(), 0);
        assert!(!logical.join(".current-gates").exists());
        assert!(
            !directory
                .path()
                .join("service-data-1.4.lifecycle.lock")
                .exists()
        );
        Ok(())
    }

    #[test]
    fn stable_complete_snapshot_retains_original_request_and_refuses_unadapted_engine_change()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let namespace = Incarnation::from_bytes([3; 16]);
        let control = directory.path().join("control");
        std::fs::create_dir(&control)?;
        let mut registry = ServiceRegistry::open(&control, namespace)?;
        let project = ProjectRegistration::new(
            directory
                .path()
                .join("missing-source")
                .to_string_lossy()
                .into_owned(),
            directory
                .path()
                .join("missing-storage")
                .to_string_lossy()
                .into_owned(),
            "checkpoint-fixture".into(),
        )?;
        registry.register(&project)?;
        let (_, epoch) = registry.request_epoch()?;
        let caller = Incarnation::from_bytes([4; 16]);
        let key = RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes([5; 16]),
        };
        let job = registry.enqueue(project.binding, caller, &key, JobKind::Index)?;
        let before = registry.connection.total_changes();
        let first = snapshot(&control)?;
        let second = snapshot(&control)?;
        assert_eq!(first.digest()?, second.digest()?);
        assert_eq!(registry.connection.total_changes(), before);
        assert_eq!(first.project_bindings, vec![project.binding]);
        assert!(first.jobs.iter().any(|saved| saved.record.id == job.id));
        assert!(
            first
                .requests
                .iter()
                .any(|saved| saved.namespace == namespace
                    && saved.epoch == epoch
                    && saved.nonce == key.nonce
                    && saved.caller == caller
                    && saved.job == job.id)
        );
        assert_eq!(
            first
                .tables
                .iter()
                .find(|table| table.name == "service_jobs")
                .unwrap()
                .rows,
            Some(first.jobs.len())
        );
        let old = Sha256Digest::from_bytes([1; 32]);
        let new = Sha256Digest::from_bytes([2; 32]);
        first.require_compatible(Some(old), old)?;
        assert!(first.require_compatible(Some(old), new).is_err());
        assert!(first.require_compatible(None, new).is_err());
        assert_eq!(registry.job(job.id)?.state, JobState::Pending);
        registry.connection.execute("INSERT INTO service_events(observed_ms,project,kind) VALUES(1,?1,'checkpoint_fixture')", [project.binding.to_string()])?;
        assert_eq!(
            first.digest()?,
            snapshot(&control)?.digest()?,
            "cached telemetry and allocator state are not restart-restorable work"
        );
        registry.cancel_pending(job.id)?;
        assert_ne!(
            first.digest()?,
            snapshot(&control)?.digest()?,
            "accepted request outcome is durable checkpoint state"
        );
        registry.configure_watch(project.binding, true)?;
        for kind in [JobKind::Index, JobKind::Vectors, JobKind::ModelSetup] {
            super::super::background::set(&registry.connection, project.binding, kind, true)?;
        }
        let retained = snapshot(&control)?;
        assert!(retained.unfinished.is_empty());
        for kind in [
            "source_update",
            "semantic_update",
            "model_setup",
            "source_reconciliation",
        ] {
            assert!(
                retained
                    .retained_intents
                    .iter()
                    .any(|intent| intent.kind == kind && intent.original_engine.is_none()),
                "{kind}"
            );
        }
        retained.require_compatible(Some(old), new)?;
        assert_eq!(registry.job(job.id)?.state, JobState::Canceled);
        assert!(retained.requests.is_empty());
        let legacy = snapshot_for_schema(&control, 1)?;
        assert!(
            legacy
                .requests
                .iter()
                .any(|saved| saved.job == job.id && saved.nonce == key.nonce)
        );
        assert_eq!(legacy.jobs.len(), 1);
        assert_eq!(legacy.tables, retained.tables);
        legacy.require_compatible(Some(old), new)?;
        let mut unsupported = retained.clone();
        unsupported.retained_intent_contract = 2;
        assert!(unsupported.require_compatible(Some(old), new).is_err());
        let mut falsely_assigned = retained;
        falsely_assigned.retained_intents[0].original_engine = Some(old);
        assert!(falsely_assigned.require_compatible(Some(old), new).is_err());
        Ok(())
    }

    #[test]
    fn complete_roster_never_silently_omits_an_oversized_native_row() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let registry = ServiceRegistry::open(directory.path(), Incarnation::from_bytes([6; 16]))?;
        registry.connection.execute_batch("CREATE TABLE checkpoint_fixture(payload BLOB); INSERT INTO checkpoint_fixture VALUES(zeroblob(65537));")?;
        let error = snapshot(directory.path()).unwrap_err();
        assert!(
            error.downcast_ref::<CheckpointLimit>().is_some(),
            "{error:#}"
        );
        assert_eq!(
            registry.connection.query_row(
                "SELECT length(payload) FROM checkpoint_fixture",
                [],
                |row| row.get::<_, i64>(0)
            )?,
            65537
        );
        Ok(())
    }
}
