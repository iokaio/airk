// SPDX-License-Identifier: Apache-2.0
//! One captured publication for catalog, graph and lexical state. The owning
//! worker additionally validates retained objects and holds its durable grants.
use super::{
    Database, Snapshot, catalog,
    directory::Budget,
    journal::Prepared,
    lexical::{Index, Manifest, QueryWork},
    lexical_plan, tables,
};
use crate::current::{CurrentHead, CurrentScope, graph, map};
use airk_core::{
    SearchQuery,
    graph_storage::{GraphHead, GraphScope, PreparedGraphDelta},
};
use anyhow::{Context, Result, ensure};
use redb::{ReadableTable, ReadableTableMetadata};

pub struct Verification {
    pub catalog: map::MapVisitLimits,
    pub graph_records: u64,
    pub lexical: Budget,
}
impl Default for Verification {
    fn default() -> Self {
        Self {
            catalog: map::MapVisitLimits::default(),
            graph_records: 1_000_000,
            lexical: Budget::publication(),
        }
    }
}
/// Structural verification is separate from FileStore's ownership and retained
/// object verification. Both must succeed before a production head is installed.
pub fn verify(
    transaction: &redb::WriteTransaction,
    head: &CurrentHead,
    limits: &mut Verification,
) -> Result<()> {
    head.validate()?;
    map::visit_all(
        transaction,
        &head.scope,
        head.artifact_root,
        limits.catalog,
        &mut |_, _| Ok(()),
    )?;
    graph::kv::verify_write(transaction, &head.graph_head, limits.graph_records)?;
    let meta = transaction.open_table(tables::META)?;
    let bytes = meta.get(2)?.context("lexical manifest missing")?;
    let manifest = Manifest::decode(bytes.value(), head.lexical_commit, &head.scope)?;
    drop(bytes);
    drop(meta);
    Index::new(&head.scope)?.verify(transaction, &manifest, &mut limits.lexical)
}
fn target(
    base: &Snapshot,
    catalog: &catalog::Plan<'_>,
    graph: &dyn PreparedGraphDelta,
    lexical: Option<&lexical_plan::Sealed>,
) -> Result<CurrentHead> {
    let head = base.head()?;
    let expected = head.token()?;
    ensure!(
        catalog.base()? == expected && graph.descriptor().base == head.graph_head,
        "publication components have different bases"
    );
    let mut target = head.clone();
    target.sequence = target
        .sequence
        .checked_add(1)
        .context("publication sequence exhausted")?;
    target.artifact_root = catalog.root()?;
    target.graph_head = graph.descriptor().target.clone();
    if let Some(lexical) = lexical {
        let bytes = lexical.manifest().canonical_bytes()?;
        target.lexical_commit = lexical.manifest().digest()?;
        Manifest::decode(&bytes, target.lexical_commit, &target.scope)?;
        target.lexical_sequence = target.sequence;
    }
    Ok(target)
}
pub struct Capture<'a> {
    pub inputs: &'a dyn crate::current::CapturedInputs,
    pub model: Option<airk_core::model_objects::ModelObjectSet>,
    pub capacity: u64,
}
impl Database {
    /// Only a newly admitted empty collection can acquire its first head.
    pub fn initialize_components(
        &self,
        scope: CurrentScope,
        graph_scope: GraphScope,
    ) -> Result<CurrentHead> {
        let manifest = Manifest::empty(&scope)?;
        let head = CurrentHead {
            version: 2,
            sequence: 0,
            lexical_sequence: 0,
            artifact_root: map::empty_root(&scope)?,
            lexical_commit: manifest.digest()?,
            graph_head: GraphHead::empty(graph_scope)?,
            scope,
        };
        head.validate()?;
        let transaction = self.write()?;
        {
            let mut meta = transaction.open_table(tables::META)?;
            ensure!(
                meta.len()? == 1
                    && meta
                        .get(3)?
                        .is_some_and(|v| v.value() == super::FORMAT.to_be_bytes()),
                "collection initialization requires only its format marker"
            );
            ensure!(
                transaction.open_table(tables::MAP_BUCKET)?.is_empty()?
                    && transaction.open_table(tables::MAP_NODE)?.is_empty()?
                    && transaction.open_table(tables::GRAPH_RECORD)?.is_empty()?
                    && transaction.open_table(tables::GRAPH_NODE)?.is_empty()?
                    && transaction.open_table(tables::LEX_DIRECTORY)?.is_empty()?
                    && transaction.open_table(tables::LEX_BLOCK)?.is_empty()?
                    && transaction.open_table(tables::LEX_DOCUMENT)?.is_empty()?
                    && transaction.open_table(tables::PENDING)?.is_empty()?,
                "collection contains orphan state"
            );
            meta.insert(0, head.canonical_bytes()?.as_slice())?;
            meta.insert(1, b"ready".as_slice())?;
            meta.insert(2, manifest.canonical_bytes()?.as_slice())?;
        }
        transaction.commit()?;
        Ok(head)
    }

    /// Attach the initial authenticated collection policy to the sequence-zero
    /// bootstrap head. The configuration and new catalog root become durable
    /// atomically; a retry accepts the same policy only.
    pub fn initialize_collection(
        &self,
        store: &crate::FileStore,
        configuration: &crate::current::collection::CollectionConfiguration,
    ) -> Result<CurrentHead> {
        configuration.validate()?;
        if let Some(model) = &configuration.model {
            store.validate_model_reference(model, 64u64 << 30)?;
        }
        let transaction = self.write()?;
        let metadata = transaction.open_table(tables::META)?;
        let head_bytes = metadata.get(0)?.context("redb bootstrap head missing")?;
        let bytes = head_bytes.value().to_vec();
        drop(head_bytes);
        drop(metadata);
        let mut head: CurrentHead = serde_json::from_slice(&bytes)?;
        ensure!(
            head.canonical_bytes()? == bytes && head.sequence <= 1,
            "native collection initialization requires the exact bootstrap head"
        );
        ensure!(
            head.sequence != 0 || head.artifact_root == map::empty_root(&head.scope)?,
            "native collection bootstrap catalog is not empty"
        );
        let mut plan = map::MapPlan::new(&transaction, &head.scope, head.artifact_root)?;
        let collection = crate::current::collection::CollectionPlan::load(
            &plan,
            &head.scope,
            Some(configuration),
        )?;
        collection.finish(&mut plan, &head.scope)?;
        let _posting_changes = crate::current::postings::PostingChanges::new(true)
            .finish(&mut plan, &head.scope)?
            .collect::<Vec<_>>();
        let root = plan.root()?;
        if root != head.artifact_root {
            ensure!(
                transaction.open_table(tables::PENDING)?.is_empty()?,
                "native initialization has pending publication rows"
            );
            ensure!(
                transaction
                    .open_table(tables::META)?
                    .get(1)?
                    .is_some_and(|v| v.value() == b"ready"),
                "native initialization is not ready"
            );
            plan.write_kv(&transaction)?;
            ensure!(
                root == plan.root()?,
                "native initialization catalog root changed"
            );
            head.artifact_root = plan.root()?;
            head.sequence = 1;
            transaction
                .open_table(tables::META)?
                .insert(0, head.canonical_bytes()?.as_slice())?;
        }
        transaction.commit()?;
        Ok(head)
    }
    pub fn prepare_native_components(
        &self,
        base: &Snapshot,
        catalog: &catalog::Plan<'_>,
        graph: &dyn PreparedGraphDelta,
        lexical: Option<&lexical_plan::Sealed>,
        capture: Capture<'_>,
    ) -> Result<Prepared> {
        let target = target(base, catalog, graph, lexical)?;
        let expected = base.head()?.token()?;
        let intent = catalog.native_intent(crate::current::journal::NativeIntent {
            expected,
            target: target.clone(),
            graph: graph.descriptor().clone(),
            inputs: capture.inputs,
            model: capture.model,
            capacity: capture.capacity,
            lexical_manifest_bytes: lexical
                .map(|v| v.manifest().canonical_bytes().map(|b| b.len() as u64))
                .transpose()?
                .unwrap_or(0),
        })?;
        self.prepare_intent_stream(expected, target, Some(intent), |visitor| {
            catalog.visit(visitor)?;
            graph::kv::visit_operations(base.transaction(), graph, visitor)?;
            if let Some(lexical) = lexical {
                lexical.visit(visitor)?;
            }
            capture.inputs.visit(&mut |input| {
                visitor(&super::journal::Operation {
                    table: super::journal::Table::CapturedInput,
                    key: input.artifact.path.as_bytes().to_vec(),
                    value: Some(input.bytes()?),
                })
            })
        })
    }
    /// Components are streamed in the closed journal's table/key order. A stale
    /// base, poisoned component or callback failure leaves Ready unchanged.
    pub fn prepare_components(
        &self,
        base: &Snapshot,
        catalog: &catalog::Plan<'_>,
        graph: &dyn PreparedGraphDelta,
        lexical: Option<&lexical_plan::Sealed>,
    ) -> Result<Prepared> {
        let target = target(base, catalog, graph, lexical)?;
        let expected = base.head()?.token()?;
        self.prepare_stream(expected, target, |visitor| {
            catalog.visit(visitor)?;
            graph::kv::visit_operations(base.transaction(), graph, visitor)?;
            if let Some(lexical) = lexical {
                lexical.visit(visitor)?;
            }
            Ok(())
        })
    }
}
impl Snapshot {
    pub fn lexical_manifest(&self) -> Result<Manifest> {
        let head = self.head()?;
        let meta = self.transaction().open_table(tables::META)?;
        let bytes = meta.get(2)?.context("lexical manifest missing")?;
        Manifest::decode(bytes.value(), head.lexical_commit, &head.scope)
    }
    pub fn lexical_search(
        &self,
        query: &SearchQuery,
        work: &mut QueryWork<'_>,
    ) -> Result<crate::current::lexical::CurrentLexicalCandidates> {
        let head = self.head()?;
        Index::new(&head.scope)?.search(self.transaction(), &self.lexical_manifest()?, query, work)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        current::graph::{GraphOverlay, GraphOverlayLimits},
        quota::Quota,
    };
    use airk_core::{
        Artifact, Chunk, DocumentLocation,
        graph_storage::{GraphRecord, GraphRecordOverlay, GraphRecordReader},
        identity::{Incarnation, Sha256Digest},
    };
    use std::sync::{Arc, atomic::AtomicBool};
    struct InterruptedDelta<'a>(&'a dyn PreparedGraphDelta);
    impl PreparedGraphDelta for InterruptedDelta<'_> {
        fn descriptor(&self) -> &airk_core::graph_storage::GraphDeltaDescriptor {
            self.0.descriptor()
        }
        fn visit_records(
            &self,
            visitor: &mut dyn FnMut(u32, &GraphRecord) -> Result<()>,
        ) -> Result<()> {
            self.0.visit_records(&mut |id, record| {
                visitor(id, record)?;
                anyhow::bail!("injected capture interruption")
            })
        }
        fn visit_nodes(
            &self,
            visitor: &mut dyn FnMut(airk_core::graph_storage::GraphMerkleNode) -> Result<()>,
        ) -> Result<()> {
            self.0.visit_nodes(visitor)
        }
    }
    fn artifact(text: &str) -> Artifact {
        Artifact {
            path: "fixture.rs".into(),
            content_hash: Sha256Digest::from_bytes([7; 32]).to_string(),
            bytes: text.len(),
            chunks: vec![Chunk {
                text: text.into(),
                start_line: 1,
                end_line: 1,
                location: DocumentLocation::Lines { start: 1, end: 1 },
            }],
            language: "rust".into(),
            extraction_method: "fixture".into(),
            parse_status: "ok".into(),
            facts: vec![],
            retained: None,
        }
    }
    fn record(id: u32) -> GraphRecord {
        GraphRecord {
            embedding_key: (id != 0).then(|| Sha256Digest::from_bytes([id as u8; 32])),
            live: id != 0,
            bytes: vec![id as u8; 32],
        }
    }
    fn count(snapshot: &Snapshot, text: &str) -> Result<usize> {
        let cancelled = AtomicBool::new(false);
        Ok(snapshot
            .lexical_search(
                &SearchQuery {
                    text: text.into(),
                    path_prefix: None,
                    limit: 10,
                    max_bytes: 65536,
                },
                &mut QueryWork::new(&cancelled),
            )?
            .values
            .len())
    }
    #[test]
    fn catalog_graph_and_lexical_publish_one_head_and_repack_removes_old_graph_rows() -> Result<()>
    {
        let file = tempfile::NamedTempFile::new()?;
        let db = Database::open(file.reopen()?, 32 << 20, true)?;
        let scope = CurrentScope {
            root: "fixture/publication".into(),
            collection: "publication".into(),
            incarnation: Incarnation::from_bytes([1; 16]),
        };
        let graph_scope = GraphScope {
            root: scope.root.clone(),
            collection: scope.collection.clone(),
            incarnation: scope.incarnation,
            graph_instance: Incarnation::from_bytes([2; 16]),
            specification: Sha256Digest::from_bytes([3; 32]),
            record_bytes: 32,
        };
        let mut head = db.initialize_components(scope, graph_scope)?;
        let root = tempfile::tempdir()?;
        let quota = Quota::open(root.path())?;
        let limits = GraphOverlayLimits {
            maximum_bytes: 1 << 20,
            maximum_dirty_records: 100,
        };
        for (step, text) in ["first value", "second replacement"]
            .into_iter()
            .enumerate()
        {
            let base = Arc::new(db.snapshot()?);
            let reservation = quota.reserve("publication", 16 << 20)?;
            let overlay = if step == 0 {
                GraphOverlay::empty(&reservation, head.graph_head.scope.clone(), limits)?
            } else {
                GraphOverlay::from_redb(&reservation, base.clone(), limits)?
            };
            if step == 0 {
                overlay.append(0, record(0))?;
                overlay.append(1, record(1))?;
            } else {
                let mut changed = overlay.read(1)?;
                changed.bytes = vec![22; 32];
                overlay.put(1, changed)?;
                overlay.set_live(1, false)?;
                overlay.append(2, record(2))?;
            }
            let graph = overlay.seal()?;
            let mut catalog = catalog::Plan::new(&base, &head)?;
            catalog.put(map::MapDomain::Metadata, b"fixture", Some(text.as_bytes()))?;
            let stage = reservation.staging_directory()?.join("lexical.airk");
            let mut lexical = lexical_plan::Plan::new(&base, &reservation, &stage, 8 << 20)?;
            lexical.replace(
                "fixture.rs",
                Some(&artifact(text)),
                &mut Budget::publication(),
            )?;
            let lexical = lexical.seal(&mut Budget::publication())?;
            if step == 0 {
                assert!(
                    db.prepare_components(
                        &base,
                        &catalog,
                        &InterruptedDelta(&graph),
                        Some(&lexical)
                    )
                    .is_err()
                );
                assert!(db.pending_publication()?.is_none());
                assert_eq!(db.snapshot()?.head()?, head);
            }
            let pending = db.prepare_components(&base, &catalog, &graph, Some(&lexical))?;
            assert_eq!(db.snapshot()?.head()?, head);
            db.mark_applying(&pending)?;
            assert_eq!(db.snapshot()?.head()?, head);
            drop(catalog);
            drop(graph);
            drop(lexical);
            drop(reservation);
            assert!(!stage.exists());
            if step == 0 {
                assert!(
                    db.apply(&pending, |tx, target| {
                        tx.open_table(tables::GRAPH_NODE)?.remove(32u64 << 32)?;
                        verify(tx, target, &mut Verification::default())
                    })
                    .is_err()
                );
                assert_eq!(db.snapshot()?.head()?, head);
                assert_eq!(db.pending_publication()?, Some(pending.clone()));
            }
            let token = db.apply(&pending, |tx, target| {
                verify(tx, target, &mut Verification::default())
            })?;
            let latest = db.snapshot()?;
            let next = latest.head()?;
            assert_eq!(next.token()?, token);
            assert_eq!(count(&latest, text.split_whitespace().next().unwrap())?, 1);
            assert_eq!(
                catalog::get(
                    latest.transaction(),
                    &next.scope,
                    next.artifact_root,
                    map::key(&next.scope, map::MapDomain::Metadata, b"fixture")?
                )?
                .as_deref(),
                Some(text.as_bytes())
            );
            assert_eq!(base.head()?, head);
            assert_eq!(count(&base, "second")?, 0);
            if step == 1 {
                assert!(graph::kv::read(base.transaction(), &head.graph_head, 1)?.live);
                assert!(!graph::kv::read(latest.transaction(), &next.graph_head, 1)?.live);
                assert_eq!(
                    graph::kv::read(latest.transaction(), &next.graph_head, 2)?.bytes,
                    vec![2; 32]
                );
            }
            head = next;
        }
        let base = db.snapshot()?;
        let reservation = quota.reserve("publication", 4 << 20)?;
        let mut scope = head.graph_head.scope.clone();
        scope.graph_instance = Incarnation::from_bytes([9; 16]);
        let overlay = GraphOverlay::empty(&reservation, scope, limits)?;
        overlay.append(0, record(0))?;
        overlay.append(1, record(1))?;
        let graph = overlay.seal()?.replacing(head.graph_head.clone())?;
        let catalog = catalog::Plan::new(&base, &head)?;
        let pending = db.prepare_components(&base, &catalog, &graph, None)?;
        db.mark_applying(&pending)?;
        db.apply(&pending, |tx, target| {
            verify(tx, target, &mut Verification::default())
        })?;
        let latest = db.snapshot()?;
        let next = latest.head()?;
        assert_eq!(next.graph_head.allocated_count, 2);
        assert_eq!(
            latest
                .transaction()
                .open_table(tables::GRAPH_RECORD)?
                .len()?,
            2
        );
        assert_eq!(next.lexical_commit, head.lexical_commit);
        assert_eq!(count(&latest, "second")?, 1);
        assert!(graph::kv::read(latest.transaction(), &next.graph_head, 2).is_err());
        assert!(graph::kv::read(base.transaction(), &head.graph_head, 2)?.live);
        Ok(())
    }
}
