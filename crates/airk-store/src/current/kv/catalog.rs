// SPDX-License-Identifier: Apache-2.0
//! Existing catalog authentication over redb typed tables; hash bytes are unchanged.
use super::tables;
use crate::current::{CurrentScope, map};
use airk_core::identity::Sha256Digest;
use anyhow::{Result, ensure};
use redb::ReadableTable;

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
        let sql = rusqlite::Connection::open_in_memory()?;
        map::create(&sql)?;
        let file = tempfile::NamedTempFile::new()?;
        let database = Database::open(file.reopen()?, 32 << 20, true)?;
        let old = database.snapshot()?;
        let empty = map::empty_root(&scope)?;
        let mut root = empty;
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
                let sql_root = map::put(&sql, &scope, root, key, value)?;
                root = put(&transaction, &scope, root, key, value)?;
                assert_eq!(root, sql_root);
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
                assert_eq!(map::get(&sql, &scope, root, key)?.as_deref(), Some(value));
                count += 1;
                Ok(())
            },
        )?;
        assert!(count > 0);
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
