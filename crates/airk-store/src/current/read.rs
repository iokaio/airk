// SPDX-License-Identifier: Apache-2.0
use super::{
    CurrentHead, CurrentScope, CurrentToken, database, decode_head, gate::Gate, graph,
    journal::CapturedInput, map,
};
use crate::FileStore;
use airk_core::graph_storage::{GraphCounts, GraphRecord, GraphRecordReader, GraphScope};
use anyhow::{Context, Result, ensure};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// The connection/read transaction drops before the gate. Providers may clone
/// the Arc for one request, but neither a kernel cache nor a sealed delta owns it.
pub struct CurrentRead {
    connection: Mutex<Connection>,
    head: CurrentHead,
    _gate: Gate,
}

impl CurrentRead {
    /// Authenticates the compact record and path. Call FileStore's verified
    /// retained-object reader before using referenced source or extraction bytes.
    pub fn artifact(&self, path: &str) -> Result<Option<airk_core::Artifact>> {
        let Some(bytes) = self.map_get(map::MapDomain::Artifact, path.as_bytes())? else {
            return Ok(None);
        };
        let captured: CapturedInput = serde_json::from_slice(&bytes)?;
        ensure!(
            captured.artifact.path == path && captured.bytes()? == bytes,
            "current artifact record binding mismatch"
        );
        Ok(Some(captured.artifact))
    }
    /// Returns authenticated current-map bytes. Referenced retained objects must
    /// still be loaded through FileStore's verified object readers.
    pub fn map_get(&self, domain: map::MapDomain, identity: &[u8]) -> Result<Option<Vec<u8>>> {
        let key = map::key(&self.head.scope, domain, identity)?;
        self.with_connection(|connection| {
            map::get(connection, &self.head.scope, self.head.artifact_root, key)
        })
    }

    /// A complete proof-checked enumeration, limited independently by visited
    /// nodes, entries and bytes. This returns no partial list on budget failure.
    pub fn map_entries(
        &self,
        limits: map::MapVisitLimits,
    ) -> Result<Vec<(airk_core::identity::Sha256Digest, Vec<u8>)>> {
        self.with_connection(|connection| {
            let mut entries = Vec::new();
            map::visit_all(
                connection,
                &self.head.scope,
                self.head.artifact_root,
                limits,
                &mut |key, value| {
                    entries.push((key, value.to_vec()));
                    Ok(())
                },
            )?;
            Ok(entries)
        })
    }
    pub fn head(&self) -> &CurrentHead {
        &self.head
    }
    pub fn token(&self) -> Result<CurrentToken> {
        self.head.token()
    }
    pub fn require_token(&self, token: &CurrentToken) -> Result<()> {
        ensure!(
            *token == self.token()?,
            "current revision expired; repeat the request against the current index"
        );
        Ok(())
    }
    pub(crate) fn with_connection<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| anyhow::anyhow!("current reader connection poisoned"))?;
        f(&connection)
    }
}

impl GraphRecordReader for CurrentRead {
    fn scope(&self) -> &GraphScope {
        &self.head.graph_head.scope
    }
    fn counts(&self) -> Result<GraphCounts> {
        Ok(self.head.graph_head.counts())
    }
    fn read(&self, id: u32) -> Result<GraphRecord> {
        self.with_connection(|connection| graph::read_record(connection, &self.head.graph_head, id))
    }
}

impl FileStore {
    pub fn current_read(&self, scope: &CurrentScope) -> Result<Arc<CurrentRead>> {
        scope.validate()?;
        ensure!(
            self.directory().file_name().and_then(|name| name.to_str())
                == Some(scope.collection.as_str()),
            "foreign current collection path"
        );
        let gate = Gate::shared(self)?;
        self.ensure_index_ready()?;
        let capacity = database::check_owner(self.directory(), scope)?;
        ensure!(
            std::fs::metadata(self.directory().join("index.sqlite3"))?.len() <= capacity,
            "current database exceeds captured capacity"
        );
        let connection = database::open_reader(&self.directory().join("index.sqlite3"))?;
        let (phase, bytes): (String, Vec<u8>) = connection
            .query_row(
                "SELECT phase,head FROM current_state WHERE id=1 AND length(head)<=65536",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .context("current index metadata missing")?;
        ensure!(
            phase == "ready" || phase == "prepared",
            "current index requires writer recovery before reads"
        );
        let head = decode_head(&bytes, scope)?;
        Ok(Arc::new(CurrentRead {
            connection: Mutex::new(connection),
            head,
            _gate: gate,
        }))
    }
}
