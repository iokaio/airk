// SPDX-License-Identifier: Apache-2.0
//! Incremental FTS5 preparation and captured replay inside the current pager.
use super::super::lexical::{self, LexicalManifest, LexicalWork};
use super::*;

pub(super) type Updates = Vec<(String, Option<Vec<u8>>)>;
pub(super) struct Prepared {
    pub manifest: Vec<u8>,
    pub updates: Updates,
    pub work: LexicalWork,
}

fn apply(connection: &Connection, store: &FileStore, updates: &Updates) -> Result<LexicalWork> {
    let mut work = LexicalWork {
        changed_artifacts: updates.len(),
        added_documents: 0,
    };
    for (path, bytes) in updates {
        let artifact = if let Some(bytes) = bytes {
            let input: CapturedInput = serde_json::from_slice(bytes)?;
            ensure!(
                input.bytes()? == *bytes && input.artifact.path == *path,
                "FTS5 captured input binding mismatch"
            );
            input.verify(store)?;
            Some(store.load_retrieval_artifact(&input.artifact)?.artifact)
        } else {
            None
        };
        work.added_documents += lexical::replace(connection, path, artifact.as_ref())?;
    }
    if !updates.is_empty() {
        lexical::flush(connection)?;
    }
    Ok(work)
}

pub(super) fn stage(connection: &Connection, updates: &Updates) -> Result<()> {
    for (path, bytes) in updates {
        connection.execute(
            "INSERT INTO current_pending_fts(path,metadata) VALUES(?1,?2)",
            params![path, bytes],
        )?;
    }
    Ok(())
}

pub(super) fn captured(connection: &Connection, manifest: &LexicalManifest) -> Result<Updates> {
    let mut statement =
        connection.prepare("SELECT CASE WHEN length(path)<=8192 THEN path ELSE '' END,CASE WHEN metadata IS NULL OR length(metadata)<=262144 THEN metadata ELSE zeroblob(0) END FROM current_pending_fts ORDER BY path")?;
    let mut rows = statement.query([])?;
    let mut updates = Vec::new();
    let mut total = 0usize;
    while let Some(row) = rows.next()? {
        ensure!(
            updates.len() < MAX_PENDING_INPUTS,
            "FTS5 captured update count exceeded"
        );
        let path: String = row.get(0)?;
        let bytes: Option<Vec<u8>> = row.get(1)?;
        total = total
            .checked_add(path.len() + bytes.as_ref().map_or(0, Vec::len))
            .context("FTS5 captured byte overflow")?;
        ensure!(
            total <= MAX_PENDING_METADATA_BYTES as usize,
            "FTS5 captured update byte bound exceeded"
        );
        updates.push((path, bytes));
    }
    ensure!(
        lexical::updates_digest(&updates)? == manifest.updates,
        "FTS5 captured updates digest mismatch"
    );
    Ok(updates)
}

pub(super) fn replay(
    connection: &Connection,
    store: &FileStore,
    bytes: &[u8],
    expected: Sha256Digest,
    base: &lexical::Snapshot,
) -> Result<()> {
    let manifest = LexicalManifest::decode(bytes, expected)?;
    let updates = captured(connection, &manifest)?;
    let next = if updates.is_empty() {
        base.clone()
    } else {
        base.apply_tracked(connection, || apply(connection, store, &updates))?
            .0
    };
    ensure!(
        next.manifest(manifest.updates)? == manifest,
        "replayed FTS5 seal differs from prepared target"
    );
    Ok(())
}

impl CurrentWriter<'_> {
    pub(super) fn lexical_snapshot(&self) -> Result<std::sync::Arc<lexical::Snapshot>> {
        let head = self.head()?;
        if let Some((commit, snapshot)) = &*self.lexical_base.borrow()
            && *commit == head.lexical_commit
        {
            return Ok(snapshot.clone());
        }
        let manifest =
            LexicalManifest::decode(&self.committed_lexical_manifest()?, head.lexical_commit)?;
        let snapshot = std::sync::Arc::new(lexical::Snapshot::capture(self.connection()?)?);
        ensure!(
            snapshot.manifest(manifest.updates)? == manifest,
            "current FTS5 physical index digest mismatch"
        );
        *self.lexical_base.borrow_mut() = Some((head.lexical_commit, snapshot.clone()));
        Ok(snapshot)
    }
    pub fn verify_current_lexical(&self) -> Result<()> {
        self.lexical_snapshot().map(|_| ())
    }
    pub fn verify_pending_fts(&self, pending: &PendingTransaction) -> Result<()> {
        if let Some(bytes) = self.pending_lexical_manifest(pending)? {
            let manifest = LexicalManifest::decode(&bytes, pending.target.lexical_commit)?;
            captured(self.connection()?, &manifest)?;
        }
        self.verify_current_lexical()
    }
    pub(super) fn prepare_fts(
        &mut self,
        expected: &CurrentToken,
        changes: &[ArtifactChange],
    ) -> Result<Prepared> {
        ensure!(
            self.phase()? == TransactionPhase::Ready
                && self.pending()?.is_none()
                && self.head()?.token()? == *expected,
            "FTS5 preparation base expired"
        );
        let updates: Updates = changes
            .iter()
            .map(|change| match change {
                ArtifactChange::Upsert(input) => {
                    Ok((input.artifact.path.clone(), Some(input.bytes()?)))
                }
                ArtifactChange::Delete(path) => Ok((path.clone(), None)),
            })
            .collect::<Result<_>>()?;
        let digest = lexical::updates_digest(&updates)?;
        let mut manifest = LexicalManifest::decode(
            &self.committed_lexical_manifest()?,
            self.head()?.lexical_commit,
        )?;
        manifest.updates = digest;
        if changes.is_empty() {
            return Ok(Prepared {
                manifest: manifest.canonical_bytes()?,
                updates,
                work: LexicalWork::default(),
            });
        }
        let base = self.lexical_snapshot()?;
        let mut growth = 1u64 << 20;
        for change in changes {
            if let ArtifactChange::Upsert(input) = change {
                // Retained extraction byte count is authenticated captured metadata.
                // Do not deserialize the artifact merely to estimate admission.
                let retained = input
                    .artifact
                    .retained
                    .as_ref()
                    .context("FTS5 retained input missing")?;
                let estimate = (retained.object_bytes as u64)
                    .checked_mul(16)
                    .and_then(|n| n.checked_add(8192))
                    .context("FTS5 admission estimate overflow")?;
                growth = growth
                    .checked_add(estimate)
                    .context("FTS5 admission estimate overflow")?;
            }
        }
        let used = fs::metadata(self.store.directory().join("current.sqlite3"))?.len();
        let needed = used.checked_add(growth).context("FTS5 capacity overflow")?;
        if needed > self.capacity {
            self.grow_capacity(
                needed
                    .checked_next_power_of_two()
                    .context("FTS5 capacity overflow")?,
            )?;
        }
        self.release_orphan_grants()?;
        let id = framed_digest(
            b"current-fts5-prepare/1",
            &[&serde_json::to_vec(expected)?, digest.as_bytes()],
        );
        let scope = self.grant_scope(id);
        let allowance = database::allowance(
            self.connection
                .as_mut()
                .context("current database missing")?,
            self.capacity,
        )?;
        let grant = self.quota.reserve_current(self.writer, scope, allowance)?;
        crate::capacity::require_growth(self.quota.root(), allowance)?;
        let transaction = self
            .connection
            .as_mut()
            .context("current database missing")?
            .transaction()?;
        let (next, work) =
            base.apply_tracked(&transaction, || apply(&transaction, &self.store, &updates))?;
        let manifest = next.manifest(digest)?.canonical_bytes()?;
        // The base remains the only published lexical state. Replay applies the
        // same captured inputs in the atomic graph/map/head commit, then checks
        // the sealed shadow-row digest before committing. No index copy/history.
        transaction.rollback()?;
        grant.release(self.writer)?;
        Ok(Prepared {
            manifest,
            updates,
            work,
        })
    }
}
