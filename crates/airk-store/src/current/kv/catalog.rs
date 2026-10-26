// SPDX-License-Identifier: Apache-2.0
//! Existing catalog authentication over redb typed tables; hash bytes are unchanged.
use super::tables;
use crate::current::{CurrentScope, map};
use airk_core::identity::Sha256Digest;
use anyhow::{Result, ensure};
use redb::ReadableTable;

/// Catalog afterimages predicted from one exact publication base.
pub struct Plan<'a> {
    plan: map::MapPlan<'a>,
    head: &'a crate::current::CurrentHead,
    changes: std::collections::BTreeMap<Sha256Digest, map::MapChange>,
}
pub struct ArtifactRequest<'a> {
    pub store: &'a crate::FileStore,
    pub target: &'a crate::current::CurrentHead,
    pub graph: &'a dyn airk_core::graph_storage::PreparedGraphDelta,
    pub update: crate::current::ArtifactVectorChanges<'a>,
    pub model: &'a Option<airk_core::model_objects::ModelObjectSet>,
    pub metadata: &'a [crate::current::metadata::FileMetadataRef],
    pub resources: &'a [crate::current::resources::ResourceChange],
}
impl<'a> Plan<'a> {
    /// Replace only authenticated physical source/payload encodings. All logical
    /// evidence and posting memberships retain their previous identity.
    pub fn repack(
        base: &'a super::Snapshot,
        head: &'a crate::current::CurrentHead,
        store: &crate::FileStore,
        inputs: &[crate::current::CapturedInput],
    ) -> Result<Self> {
        use anyhow::Context;
        ensure!(
            !inputs.is_empty()
                && inputs.len() <= 32
                && inputs
                    .windows(2)
                    .all(|pair| pair[0].artifact.path < pair[1].artifact.path),
            "invalid repack batch"
        );
        let mut plan = Self::new(base, head)?;
        for input in inputs {
            let prior: crate::current::CapturedInput = serde_json::from_slice(
                &plan
                    .get(map::MapDomain::Artifact, input.artifact.path.as_bytes())?
                    .context("repack source missing")?,
            )?;
            ensure!(
                input.postings_version == prior.postings_version,
                "repack changed posting format"
            );
            let mut comparison = input.artifact.clone();
            let previous = prior
                .artifact
                .retained
                .as_ref()
                .context("repack prior reference missing")?;
            let next = comparison
                .retained
                .as_mut()
                .context("repack next reference missing")?;
            next.source_encoding = previous.source_encoding.clone();
            next.payload_encoding = previous.payload_encoding.clone();
            ensure!(
                serde_json::to_vec(&comparison)? == serde_json::to_vec(&prior.artifact)?,
                "repack changed logical evidence"
            );
            input.verify(store)?;
            plan.put(
                map::MapDomain::Artifact,
                input.artifact.path.as_bytes(),
                Some(&input.bytes()?),
            )?;
        }
        for change in crate::current::recent::prepare(
            &head.scope,
            &mut plan.plan,
            head,
            crate::current::recent::Observation::empty(),
            crate::current::recent::unix_seconds(),
        )? {
            plan.changes.insert(change.key(), change);
        }
        Ok(plan)
    }
    /// Reuse the owning crate's artifact/selector/occurrence rules against a
    /// pinned redb base. Retained payloads are verified through FileStore.
    pub fn artifacts(
        base: &'a super::Snapshot,
        head: &'a crate::current::CurrentHead,
        request: ArtifactRequest<'a>,
    ) -> Result<(Self, Vec<crate::current::CapturedInput>)> {
        ensure!(base.head()? == *head, "catalog planning base mismatch");
        ensure!(
            request.store.collection_id()? == head.scope.collection,
            "foreign artifact planning store"
        );
        crate::current::journal::verify_model(request.store, request.model)?;
        let planned = crate::current::planning::artifacts(crate::current::planning::Context {
            storage: base,
            store: request.store,
            scope: &head.scope,
            base: head,
            target: request.target,
            graph_delta: request.graph,
            update: request.update,
            model: request.model,
            metadata: request.metadata,
            resources: request.resources,
        })?;
        for input in &planned.inputs {
            input.verify(request.store)?;
        }
        Ok((
            Self {
                plan: planned.plan,
                head,
                changes: planned.changes.into_iter().map(|c| (c.key(), c)).collect(),
            },
            planned.inputs,
        ))
    }
    pub fn new(base: &'a super::Snapshot, head: &'a crate::current::CurrentHead) -> Result<Self> {
        ensure!(base.head()? == *head, "catalog planning base mismatch");
        Ok(Self {
            plan: map::MapPlan::new(base, &head.scope, head.artifact_root)?,
            head,
            changes: Default::default(),
        })
    }
    pub fn get(&self, domain: map::MapDomain, identity: &[u8]) -> Result<Option<Vec<u8>>> {
        self.plan.get(map::key(&self.head.scope, domain, identity)?)
    }
    pub fn put(
        &mut self,
        domain: map::MapDomain,
        identity: &[u8],
        value: Option<&[u8]>,
    ) -> Result<()> {
        let change = map::MapChange::new(
            &self.head.scope,
            domain,
            identity,
            value.map(<[u8]>::to_vec),
        )?;
        self.plan.put(change.key(), change.value())?;
        self.changes.insert(change.key(), change);
        Ok(())
    }
    pub fn root(&self) -> Result<Sha256Digest> {
        self.plan.root()
    }
    pub fn base(&self) -> Result<crate::current::CurrentToken> {
        self.head.token()
    }
    pub(crate) fn native_intent(
        &self,
        fields: crate::current::journal::NativeIntent<'_>,
    ) -> Result<crate::current::PendingTransaction> {
        ensure!(
            fields.expected == self.base()? && fields.target.artifact_root == self.root()?,
            "native catalog intention differs from plan"
        );
        crate::current::journal::native_intent(fields, self.changes.values())
    }
    pub fn visit(
        &self,
        visitor: &mut dyn FnMut(&super::journal::Operation) -> Result<()>,
    ) -> Result<()> {
        self.plan.visit_kv(visitor)
    }
}

/// Reuse the same transaction's typed table handles for a complete proof walk.
pub(crate) struct MapTables<'a, B, N> {
    pub buckets: &'a B,
    pub nodes: &'a N,
}
impl<B: ReadableTable<u16, &'static [u8]>, N: ReadableTable<u32, &'static [u8]>> map::MapStorage
    for MapTables<'_, B, N>
{
    fn bucket_bytes(&self, prefix: u16) -> Result<Option<Vec<u8>>> {
        self.buckets
            .get(prefix)?
            .map(|value| {
                ensure!(
                    value.value().len() <= map::MAX_BUCKET_BYTES,
                    "oversized redb map bucket"
                );
                Ok(value.value().to_vec())
            })
            .transpose()
    }
    fn node_bytes(&self, height: u8, index: u16) -> Result<Option<Vec<u8>>> {
        let key = (u32::from(height) << 16) | u32::from(index);
        self.nodes
            .get(key)?
            .map(|value| {
                ensure!(value.value().len() == 32, "invalid redb map node length");
                Ok(value.value().to_vec())
            })
            .transpose()
    }
}
impl map::MapStorage for super::Snapshot {
    fn bucket_bytes(&self, prefix: u16) -> Result<Option<Vec<u8>>> {
        MapTables {
            buckets: &self.map_buckets,
            nodes: &self.map_nodes,
        }
        .bucket_bytes(prefix)
    }
    fn node_bytes(&self, height: u8, index: u16) -> Result<Option<Vec<u8>>> {
        MapTables {
            buckets: &self.map_buckets,
            nodes: &self.map_nodes,
        }
        .node_bytes(height, index)
    }
}
impl crate::current::graph::GraphStorage for super::Snapshot {
    fn graph_record(
        &self,
        scope: &airk_core::graph_storage::GraphScope,
        id: u32,
    ) -> Result<Option<airk_core::graph_storage::GraphRecord>> {
        crate::current::graph::kv::ReadTables {
            records: &self.graph_records,
            nodes: &self.graph_nodes,
        }
        .graph_record(scope, id)
    }
    fn graph_node(&self, height: u8, index: u32) -> Result<Option<Sha256Digest>> {
        crate::current::graph::kv::ReadTables {
            records: &self.graph_records,
            nodes: &self.graph_nodes,
        }
        .graph_node(height, index)
    }
}
macro_rules! map_storage {
    ($transaction:ty) => {
        impl map::MapStorage for $transaction {
            fn bucket_bytes(&self, prefix: u16) -> Result<Option<Vec<u8>>> {
                self.open_table(tables::MAP_BUCKET)?
                    .get(prefix)?
                    .map(|v| {
                        ensure!(
                            v.value().len() <= map::MAX_BUCKET_BYTES,
                            "oversized redb map bucket"
                        );
                        Ok(v.value().to_vec())
                    })
                    .transpose()
            }
            fn node_bytes(&self, height: u8, index: u16) -> Result<Option<Vec<u8>>> {
                let key = (u32::from(height) << 16) | u32::from(index);
                self.open_table(tables::MAP_NODE)?
                    .get(key)?
                    .map(|v| {
                        ensure!(v.value().len() == 32, "invalid redb map node length");
                        Ok(v.value().to_vec())
                    })
                    .transpose()
            }
        }
    };
}
map_storage!(redb::ReadTransaction);
map_storage!(redb::WriteTransaction);

pub fn get(
    transaction: &redb::ReadTransaction,
    scope: &CurrentScope,
    root: Sha256Digest,
    key: Sha256Digest,
) -> Result<Option<Vec<u8>>> {
    map::get(transaction, scope, root, key)
}
pub fn put(
    transaction: &redb::WriteTransaction,
    scope: &CurrentScope,
    root: Sha256Digest,
    key: Sha256Digest,
    value: Option<&[u8]>,
) -> Result<Sha256Digest> {
    let mut plan = map::MapPlan::new(transaction, scope, root)?;
    plan.put(key, value)?;
    plan.write_kv(transaction)
}
pub fn visit_all(
    transaction: &redb::ReadTransaction,
    scope: &CurrentScope,
    root: Sha256Digest,
    limits: map::MapVisitLimits,
    visitor: &mut dyn FnMut(Sha256Digest, &[u8]) -> Result<()>,
) -> Result<()> {
    map::visit_all(transaction, scope, root, limits, visitor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::current::kv::Database;
    use airk_core::identity::Incarnation;

    #[test]
    fn generated_mutations_match_sqlite_roots_and_preserve_snapshot() -> Result<()> {
        let scope = CurrentScope {
            root: "fixture/catalog".into(),
            collection: "catalog".into(),
            incarnation: Incarnation::from_bytes([9; 16]),
        };
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fixture {
            version: u32,
            empty: Sha256Digest,
            roots: Vec<Sha256Digest>,
        }
        let fixture: Fixture = serde_json::from_slice(include_bytes!(
            "../../../tests/fixtures/catalog-map-roots.json"
        ))?;
        assert_eq!(fixture.version, 1);
        assert_eq!(fixture.roots.len(), 192);
        let mut reference = std::collections::BTreeMap::new();
        let file = tempfile::NamedTempFile::new()?;
        let database = Database::open(file.reopen()?, 32 << 20, true)?;
        let old = database.snapshot()?;
        let empty = map::empty_root(&scope)?;
        let mut root = empty;
        assert_eq!(empty, fixture.empty);
        for batch in 0..8 {
            let transaction = database.write()?;
            for item in 0..24 {
                let number = (item * 17 + batch * 13) % 53;
                let key = map::key(
                    &scope,
                    map::MapDomain::Artifact,
                    format!("file-{number}").as_bytes(),
                )?;
                let value = format!("batch-{batch}-{item}").into_bytes();
                let value = (item % 5 != 0).then_some(value.as_slice());
                root = put(&transaction, &scope, root, key, value)?;
                assert_eq!(root, fixture.roots[batch * 24 + item]);
                if let Some(value) = value {
                    reference.insert(key, value.to_vec());
                } else {
                    reference.remove(&key);
                }
            }
            transaction.commit()?;
        }
        let mut count = 0;
        visit_all(
            database.snapshot()?.transaction(),
            &scope,
            root,
            map::MapVisitLimits::default(),
            &mut |key, value| {
                assert_eq!(reference.get(&key).map(Vec::as_slice), Some(value));
                count += 1;
                Ok(())
            },
        )?;
        assert!(count > 0);
        assert_eq!(count, reference.len());
        visit_all(
            old.transaction(),
            &scope,
            empty,
            map::MapVisitLimits::default(),
            &mut |_, _| panic!("old snapshot changed"),
        )?;
        let transaction = database.write()?;
        transaction.open_table(tables::MAP_BUCKET)?.pop_first()?;
        transaction.commit()?;
        assert!(
            visit_all(
                database.snapshot()?.transaction(),
                &scope,
                root,
                map::MapVisitLimits::default(),
                &mut |_, _| Ok(())
            )
            .is_err()
        );
        Ok(())
    }
}
