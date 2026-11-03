// SPDX-License-Identifier: Apache-2.0
//! Print only compiled registry DDL from an empty disposable namespace.
//! No user registry, collection or source is read.
use airk_core::identity::Incarnation;
use airk_store::service::registry::ServiceRegistry;
use anyhow::Result;
use rusqlite::Connection;

fn main() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let registry = ServiceRegistry::open(directory.path(), Incarnation::from_bytes([1; 16]))?;
    drop(registry);
    let connection = Connection::open(directory.path().join("service-1.4.sqlite3"))?;
    let mut query = connection.prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY type,name")?;
    let rows = query
        .query_map([], |row| {
            Ok(serde_json::json!({
                "kind":row.get::<_,String>(0)?,"name":row.get::<_,String>(1)?,
                "table":row.get::<_,String>(2)?,"sql":row.get::<_,String>(3)?
            }))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "schema":"airk.registry-ddl/1","version":connection.pragma_query_value(None,"user_version",|row|row.get::<_,i64>(0))?,"objects":rows
        }))?
    );
    Ok(())
}
