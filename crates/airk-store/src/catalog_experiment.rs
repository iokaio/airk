// SPDX-License-Identifier: Apache-2.0
//! Isolated O2a experiment. No active publication path calls this module.
use airk_core::{Artifact, ArtifactSource, Citation, EvidenceBasis, FactHit, RelationshipHit};
use airk_core::{
    changes::LineageEnvelope,
    identity::{RevisionToken, Sha256Digest},
    publication::InventoryV2,
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, Transaction, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::Path,
    time::Instant,
};

#[derive(Clone, Debug, Serialize)]
pub struct ChangedCatalogInput {
    pub base: RevisionToken,
    pub target: RevisionToken,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub replaced: Vec<String>,
    pub full_rebuild_reasons: Vec<&'static str>,
}
impl ChangedCatalogInput {
    pub fn between(
        old: &InventoryV2,
        next: &InventoryV2,
        base: RevisionToken,
        target: RevisionToken,
    ) -> Result<Self> {
        old.validate()?;
        next.validate()?;
        ensure!(
            target.incarnation == base.incarnation
                && target.generation > base.generation
                && old.incarnation == next.incarnation
                && old.collection == next.collection,
            "incremental input scope mismatch"
        );
        let mut result = Self {
            base,
            target,
            added: vec![],
            removed: vec![],
            replaced: vec![],
            full_rebuild_reasons: vec![],
        };
        if old.root != next.root || old.roots != next.roots {
            result.full_rebuild_reasons.push("roots");
        }
        if old.profiles != next.profiles {
            result.full_rebuild_reasons.push("profiles");
        }
        if old.extractor_version != next.extractor_version {
            result.full_rebuild_reasons.push("extractor");
        }
        if serde_json::to_vec(&old.analysis)? != serde_json::to_vec(&next.analysis)?
            || serde_json::to_vec(&old.imported_facts)? != serde_json::to_vec(&next.imported_facts)?
            || serde_json::to_vec(&old.imported_relationships)?
                != serde_json::to_vec(&next.imported_relationships)?
        {
            result.full_rebuild_reasons.push("producer_or_import");
        }
        if old.version != next.version {
            result.full_rebuild_reasons.push("schema");
        }
        for (path, file) in &next.files {
            match old.files.get(path) {
                None => result.added.push(path.clone()),
                Some(previous) if previous != file => result.replaced.push(path.clone()),
                _ => {}
            }
        }
        result.removed.extend(
            old.files
                .keys()
                .filter(|path| !next.files.contains_key(*path))
                .cloned(),
        );
        ensure!(
            result.added.len() + result.removed.len() + result.replaced.len() <= 100_000
                && serde_json::to_vec(&result)?.len() <= 16 * 1024 * 1024,
            "changed catalog input exceeds experiment bounds"
        );
        Ok(result)
    }
}
#[derive(Debug, Serialize)]
pub struct ExperimentMetrics {
    pub strategy: &'static str,
    pub input: ChangedCatalogInput,
    pub elapsed_micros: u128,
    pub copied_bytes: u64,
    pub output_bytes: u64,
    pub charged_catalog_bytes: u64,
    pub temporary_disk_bytes: u64,
    pub loaded_artifacts: usize,
}
pub struct ExperimentRequest<'a> {
    pub store: &'a crate::FileStore,
    pub base: &'a crate::blobs::Catalog,
    pub inventory: &'a InventoryV2,
    pub lineage: &'a LineageEnvelope,
    pub target: RevisionToken,
    pub source: &'a dyn ArtifactSource,
    pub reservation: &'a crate::quota::Reservation,
    pub directory: &'a Path,
    pub max_catalog_bytes: u64,
}
fn raw(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}

pub fn prepare(request: ExperimentRequest<'_>) -> Result<ExperimentMetrics> {
    let started = Instant::now();
    let old = request
        .base
        .inventory
        .as_ref()
        .context("incremental experiment requires v2 base")?;
    let receipt = request
        .base
        .receipt
        .as_ref()
        .context("incremental experiment requires pinned receipt")?;
    ensure!(
        request.store.receipt(&receipt.token)?.as_ref() == Some(receipt),
        "base belongs to another collection"
    );
    ensure!(
        raw(&old.canonical_bytes()?) == receipt.inventory_digest,
        "base inventory differs from pinned receipt"
    );
    let input =
        ChangedCatalogInput::between(old, request.inventory, receipt.token, request.target)?;
    let bytes = request.inventory.canonical_bytes()?;
    request.lineage.validate()?;
    let lineage_bytes = serde_json::to_vec(request.lineage)?;
    ensure!(
        request.target.incarnation == request.inventory.incarnation
            && request.target.generation == request.inventory.generation
            && request.target.revision
                == airk_core::identity::revision_digest(
                    request.inventory.retrieval_specification.as_bytes(),
                    &bytes
                ),
        "target token mismatch"
    );
    ensure!(
        raw(&lineage_bytes) == request.inventory.lineage_digest
            && request.lineage.incarnation == request.inventory.incarnation
            && request.lineage.observation.generation == request.target.generation
            && request.lineage.observation.parent == Some(receipt.token),
        "target lineage mismatch"
    );
    let pending = 131_072u64
        .checked_add(lineage_bytes.len() as u64)
        .context("experiment metadata allowance overflow")?;
    let maximum = request.max_catalog_bytes.saturating_sub(pending) / 4096 * 4096;
    ensure!(
        (16384..=128 * 1024 * 1024).contains(&maximum),
        "prototype catalog cap outside supported bounds"
    );
    if !input.full_rebuild_reasons.is_empty() {
        request.store.prepare_catalog_v2(
            request.directory,
            request.inventory,
            request.lineage,
            &request.target,
            request.source,
            Some(request.reservation),
            request.max_catalog_bytes,
        )?;
        return Ok(ExperimentMetrics {
            strategy: "full_fallback",
            input,
            elapsed_micros: started.elapsed().as_micros(),
            copied_bytes: 0,
            output_bytes: fs::metadata(request.directory.join("catalog.sqlite"))?.len(),
            charged_catalog_bytes: maximum,
            temporary_disk_bytes: 0,
            loaded_artifacts: request.inventory.files.len(),
        });
    }
    // Authenticate the immutable source file against the exact pinned receipt,
    // then hash while copying. Never open the active database for writing.
    let base_directory = request
        .store
        .directory()
        .join("generations")
        .join(receipt.token.revision.to_string());
    let manifest: serde_json::Value = serde_json::from_slice(&crate::blobs::read_verified(
        &base_directory.join("manifest.json"),
        &receipt.manifest_digest.to_string(),
        131072,
    )?)?;
    let component = &manifest["components"]["catalog.sqlite"];
    let expected_bytes = component["bytes"]
        .as_u64()
        .context("base catalog length missing")?;
    let expected_hash: Sha256Digest = component["sha256"]
        .as_str()
        .context("base catalog digest missing")?
        .parse()?;
    let source = base_directory.join("catalog.sqlite");
    ensure!(
        !source.symlink_metadata()?.file_type().is_symlink() && expected_bytes <= maximum,
        "invalid or oversized base catalog"
    );
    request.reservation.charge(maximum - expected_bytes)?;
    let destination = request.directory.join("catalog.sqlite");
    let mut output = request.reservation.create_file(&destination)?;
    let mut source = fs::File::open(source)?;
    ensure!(
        source.metadata()?.len() == expected_bytes,
        "base catalog length changed"
    );
    let mut buffer = [0u8; 65536];
    let mut copied = 0u64;
    let mut hash = Sha256::new();
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        copied += count as u64;
        ensure!(copied <= expected_bytes, "base catalog grew");
        hash.update(&buffer[..count]);
        output.write_all(&buffer[..count])?;
    }
    ensure!(
        copied == expected_bytes
            && Sha256Digest::from_bytes(hash.finalize().into()) == expected_hash,
        "base catalog copy checksum mismatch"
    );
    output.sync_all()?;
    drop(output);
    let mut connection =
        Connection::open_with_flags(&destination, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    connection.execute_batch(&format!("PRAGMA journal_mode=OFF; PRAGMA temp_store=MEMORY; PRAGMA cache_size=-16384; PRAGMA cache_spill=ON; PRAGMA max_page_count={};",maximum/4096))?;
    let accepted: i64 = connection.pragma_query_value(None, "max_page_count", |row| row.get(0))?;
    ensure!(
        u64::try_from(accepted)? == maximum / 4096,
        "SQLite refused experiment page cap"
    );
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    ensure!(
        version == crate::IMMUTABLE_CATALOG_VERSION,
        "unsupported base catalog schema"
    );
    let stored: String =
        connection.query_row("SELECT token FROM v2_catalog WHERE id=1", [], |row| {
            row.get(0)
        })?;
    ensure!(
        stored == receipt.token.to_string(),
        "base catalog token changed"
    );
    connection.authorizer(Some(
        |context: rusqlite::hooks::AuthContext<'_>| match context.action {
            rusqlite::hooks::AuthAction::Attach { .. }
            | rusqlite::hooks::AuthAction::Detach { .. }
            | rusqlite::hooks::AuthAction::Pragma {
                pragma_value: Some(_),
                ..
            } => rusqlite::hooks::Authorization::Deny,
            _ => rusqlite::hooks::Authorization::Allow,
        },
    ))?;
    let tx = connection.transaction()?;
    let changed: BTreeSet<_> = input
        .added
        .iter()
        .chain(&input.removed)
        .chain(&input.replaced)
        .collect();
    for path in &changed {
        if crate::normalized::present(&tx)? {
            for table in ["fact_rows", "edge_rows"] {
                tx.execute(&format!("DELETE FROM {table} WHERE file_id IN (SELECT id FROM evidence_files WHERE path=?1)"), [path])?;
            }
        } else {
            for table in ["facts", "relationships"] {
                tx.execute(&format!("DELETE FROM {table} WHERE path=?1"), [path])?;
            }
        }
        for table in ["files", "continuity_records", "usage_sources"] {
            tx.execute(&format!("DELETE FROM {table} WHERE path=?1"), [path])?;
        }
    }
    let mut loaded = 0;
    for path in input.added.iter().chain(&input.replaced) {
        let file = &request.inventory.files[path];
        tx.execute(
            "INSERT INTO files VALUES(?1,?2,?3,?4,?5)",
            params![
                path,
                file.state.artifact_id.to_string(),
                file.state.source_hash.to_string(),
                file.state.profile_id.to_string(),
                serde_json::to_string(file)?
            ],
        )?;
        for record in &file.continuity {
            tx.execute(
                "INSERT INTO continuity_records VALUES(?1,?2,?3)",
                params![
                    serde_json::to_string(&record.key)?,
                    path,
                    serde_json::to_string(record)?
                ],
            )?;
        }
        let artifact = request.source.load_artifact(path)?;
        loaded += 1;
        let size = airk_core::usage::SourceContextSize::measure(&artifact)?;
        tx.execute(
            "INSERT INTO usage_sources VALUES(?1,?2,?3)",
            params![path, artifact.content_hash, serde_json::to_string(&size)?],
        )?;
        insert_artifact(&tx, &artifact, file)?;
        // Imported rows share the same path namespace but are independent of native rows.
        for fact in request
            .inventory
            .imported_facts
            .iter()
            .filter(|fact| fact.citation.path == *path)
        {
            let mut fact = fact.clone();
            fact.continuity_key = None;
            insert_fact(&tx, &fact, true)?;
        }
        for edge in request
            .inventory
            .imported_relationships
            .iter()
            .filter(|edge| edge.citation.path == *path)
        {
            insert_edge(&tx, edge, true)?;
        }
    }
    // Removing a native file does not erase independently imported evidence.
    for path in &input.removed {
        for fact in request
            .inventory
            .imported_facts
            .iter()
            .filter(|fact| fact.citation.path == *path)
        {
            let mut fact = fact.clone();
            fact.continuity_key = None;
            insert_fact(&tx, &fact, true)?;
        }
        for edge in request
            .inventory
            .imported_relationships
            .iter()
            .filter(|edge| edge.citation.path == *path)
        {
            insert_edge(&tx, edge, true)?;
        }
    }
    replace_metadata(&tx, &request, &bytes)?;
    tx.commit()?;
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    ensure!(
        integrity == "ok",
        "private incremental catalog integrity failure"
    );
    connection.close().map_err(|(_, error)| error)?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&destination)?;
    ensure!(
        file.metadata()?.len() <= maximum,
        "private catalog exceeded charged cap"
    );
    file.sync_all()?;
    let mut output = request
        .reservation
        .create_file(request.directory.join("lineage.json"))?;
    output.write_all(&lineage_bytes)?;
    output.sync_all()?;
    drop(output);
    ensure!(
        !request.directory.join("catalog.sqlite-journal").exists()
            && !request.directory.join("catalog.sqlite-wal").exists()
            && !request.directory.join("catalog.sqlite-shm").exists(),
        "unexpected private SQLite sidecar"
    );
    let output_bytes = file.metadata()?.len();
    Ok(ExperimentMetrics {
        strategy: "private_copy_update",
        input,
        elapsed_micros: started.elapsed().as_micros(),
        copied_bytes: copied,
        output_bytes,
        charged_catalog_bytes: maximum,
        temporary_disk_bytes: 0,
        loaded_artifacts: loaded,
    })
}

fn insert_artifact(
    tx: &Transaction<'_>,
    artifact: &Artifact,
    file: &airk_core::publication::InventoryFile,
) -> Result<()> {
    let keys = airk_core::continuity::FactKeyIndex::new(&file.continuity);
    for (ordinal, fact) in artifact.facts.iter().enumerate() {
        insert_fact(
            tx,
            &FactHit {
                continuity_key: keys.key(ordinal, fact),
                name: fact.name.clone(),
                kind: fact.kind.clone(),
                syntax_kind: fact.syntax_kind.clone(),
                language: artifact.language.clone(),
                document: fact.document.clone(),
                code: fact.code.clone(),
                citation: Citation {
                    path: artifact.path.clone(),
                    content_hash: artifact.content_hash.clone(),
                    start_line: fact.start_line,
                    end_line: fact.end_line,
                    location: fact.location.clone(),
                },
            },
            false,
        )?;
    }
    Ok(())
}
fn insert_fact(tx: &Transaction<'_>, hit: &FactHit, imported: bool) -> Result<()> {
    let code = hit.code.as_ref();
    if crate::normalized::present(tx)? {
        crate::normalized::Writer::new(tx).fact(hit, imported)?;
    } else {
        tx.execute(
            "INSERT INTO facts VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                hit.citation.path,
                hit.name,
                hit.name.to_lowercase(),
                code.map_or("", |code| code.qualified_name.as_str()),
                code.map_or("", |code| code.symbol_id.as_str()),
                hit.kind,
                hit.language,
                code.is_some(),
                code.is_none() && !matches!(hit.kind.as_str(), "definition" | "import"),
                i64::try_from(hit.citation.start_line)?,
                serde_json::to_string(hit)?
            ],
        )?;
    }
    if let Some(code) = code {
        if let Some(owner) = &code.owner_id {
            insert_edge(
                tx,
                &RelationshipHit {
                    source: owner.clone(),
                    target: code.symbol_id.clone(),
                    kind: "contains".into(),
                    resolution: "syntax_candidate".into(),
                    basis: EvidenceBasis::Syntax,
                    producer: "airk-syntax-containment".into(),
                    producer_version: "native-facts/4".into(),
                    citation: hit.citation.clone(),
                },
                imported,
            )?;
        }
        if let Some(target) = &code.target {
            insert_edge(
                tx,
                &RelationshipHit {
                    source: code
                        .owner_id
                        .clone()
                        .unwrap_or_else(|| format!("file:{}", hit.citation.path)),
                    target: target.clone(),
                    kind: hit.kind.clone(),
                    resolution: "syntax_candidate".into(),
                    basis: code.basis.clone(),
                    producer: code.producer.clone(),
                    producer_version: code.producer_version.clone(),
                    citation: hit.citation.clone(),
                },
                imported,
            )?;
        }
    }
    Ok(())
}
fn insert_edge(tx: &Transaction<'_>, edge: &RelationshipHit, imported: bool) -> Result<()> {
    if crate::normalized::present(tx)? {
        return crate::normalized::Writer::new(tx).edge(edge, imported);
    }
    tx.execute(
        "INSERT INTO relationships VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            edge.source,
            edge.target,
            edge.kind,
            edge.citation.path,
            i64::try_from(edge.citation.start_line)?,
            serde_json::to_string(edge)?
        ],
    )?;
    Ok(())
}
fn replace_metadata(
    tx: &Transaction<'_>,
    request: &ExperimentRequest<'_>,
    inventory_bytes: &[u8],
) -> Result<()> {
    let snapshot = request.inventory.projection()?;
    let header = crate::PersistentRevision {
        token: None,
        manifest_sha256: None,
        storage_format: snapshot.storage_format,
        roots: snapshot.roots.clone(),
        inventory_sha256: None,
        revision: request.target.revision.to_string(),
        root: snapshot.root.clone(),
        collection: snapshot.collection.clone(),
        generation: snapshot.generation,
        files: snapshot.artifacts.len(),
        chunks: snapshot.artifacts.values().map(Artifact::chunk_count).sum(),
        source_bytes: snapshot
            .artifacts
            .values()
            .map(|artifact| artifact.bytes)
            .sum(),
        extractor_version: snapshot.extractor_version.clone(),
        semantic_producer: snapshot
            .analysis
            .as_ref()
            .map(|analysis| analysis.producer.clone()),
        imported_facts: snapshot.imported_facts.len(),
        imported_relationships: snapshot.imported_relationships.len(),
    };
    tx.execute("UPDATE revision SET root=?1,collection=?2,generation=?3,schema_version=?4,payload=?5 WHERE id=1",params![snapshot.root,snapshot.collection,i64::try_from(snapshot.generation)?,snapshot.schema_version,serde_json::to_string(&snapshot)?])?;
    tx.execute(
        "UPDATE index_components SET payload=?1 WHERE id=1",
        [serde_json::to_string(&header)?],
    )?;
    tx.execute(
        "UPDATE v2_catalog SET token=?1,inventory_digest=?2,lineage_digest=?3 WHERE id=1",
        params![
            request.target.to_string(),
            raw(inventory_bytes).to_string(),
            request.inventory.lineage_digest.to_string()
        ],
    )?;
    tx.execute_batch(
        "DELETE FROM change_events; DELETE FROM revision_headers; DELETE FROM path_resolutions;",
    )?;
    for event in &request.lineage.events {
        tx.execute(
            "INSERT INTO change_events VALUES(?1,?2,?3)",
            params![
                i64::try_from(event.generation)?,
                i64::try_from(event.event_id)?,
                serde_json::to_string(event)?
            ],
        )?;
    }
    for header in &request.lineage.ancestry {
        tx.execute(
            "INSERT INTO revision_headers VALUES(?1,?2,?3)",
            params![
                i64::try_from(header.token.generation)?,
                header.token.to_string(),
                serde_json::to_string(header)?
            ],
        )?;
    }
    for resolver in &request.lineage.resolvers {
        tx.execute(
            "INSERT INTO path_resolutions VALUES(?1,?2,?3)",
            params![
                resolver.artifact_id.to_string(),
                resolver.from.path,
                serde_json::to_string(resolver)?
            ],
        )?;
    }
    Ok(())
}
