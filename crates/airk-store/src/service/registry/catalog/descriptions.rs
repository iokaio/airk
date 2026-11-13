// SPDX-License-Identifier: Apache-2.0
//! U105 index and index-set descriptions: a lazily installed layer over any
//! authority-1 catalog layout. Only the first description write installs it,
//! inside its catalog transaction; reads, previews and clears never do, so an
//! untouched installation keeps its exact `airk-state/1` identity. The layer
//! changes no authority or policy marker: a predecessor refuses the extra
//! physical shape ("unsupported registry table ... shape"), failing closed.
use super::*;

const SCHEMA: &str = include_str!("../../../service_catalog_descriptions_v1.sql");
const TABLE: &str = "catalog_descriptions";

/// The described catalog entity kind, persisted as `index` or `project`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DescriptionKind {
    Index,
    Project,
}
impl DescriptionKind {
    fn column(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Project => "project",
        }
    }
}

/// Exactly the compiled objects the layer DDL adds; never database-supplied.
fn layer() -> Result<&'static Vec<SchemaObject>> {
    static LAYER: OnceLock<Result<Vec<SchemaObject>, String>> = OnceLock::new();
    LAYER
        .get_or_init(|| {
            (|| -> Result<_> {
                let db = Connection::open_in_memory()?;
                db.execute_batch(LEGACY_SCHEMA)?;
                db.execute_batch(super::SCHEMA)?;
                capacity_triggers(&db)?;
                write_barriers(&db)?;
                db.execute_batch(AUTHORITY_V1_SCHEMA)?;
                let before = schema_objects(&db)?;
                db.execute_batch(SCHEMA)?;
                Ok(schema_objects(&db)?
                    .into_iter()
                    .filter(|object| !before.contains(object))
                    .collect::<Vec<_>>())
            })()
            .map_err(|error| format!("{error:#}"))
        })
        .as_ref()
        .map_err(|error| anyhow::anyhow!("compiled description schema is invalid: {error}"))
}

/// The base objects when the complete layer is present; `None` when absent.
/// A partial or altered layer is an unsupported shape.
pub(super) fn strip(objects: &[SchemaObject]) -> Result<Option<Vec<SchemaObject>>> {
    if !objects
        .iter()
        .any(|object| object.kind == "table" && object.name == TABLE)
    {
        return Ok(None);
    }
    let layer = layer()?;
    ensure!(
        layer.iter().all(|object| objects.contains(object)),
        "unsupported registry table, index, foreign-key or trigger shape"
    );
    Ok(Some(
        objects
            .iter()
            .filter(|object| !layer.contains(object))
            .cloned()
            .collect(),
    ))
}

fn installed(db: &Connection) -> Result<bool> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='catalog_descriptions')",
        [],
        |row| row.get(0),
    )?)
}

/// Install the layer inside the caller's catalog transaction. The base
/// layout and its markers are unchanged.
pub(super) fn apply_layout(db: &Connection) -> Result<()> {
    ensure!(
        !db.is_autocommit(),
        "description layout requires an explicit transaction"
    );
    let layout = validate_catalog_layout(db)?;
    ensure!(
        layout != CatalogLayout::Schema12Authority0,
        "catalog descriptions require index authority one"
    );
    if !installed(db)? {
        db.execute_batch(SCHEMA)?;
    }
    ensure!(
        installed(db)? && validate_catalog_layout(db)? == layout,
        "description layout differs"
    );
    Ok(())
}

/// The stored description; `None` without reading when the layer is absent.
pub(super) fn load(db: &Connection, kind: DescriptionKind, id: &str) -> Result<Option<String>> {
    if !installed(db)? {
        return Ok(None);
    }
    let value: Option<Vec<u8>> = db
        .query_row(
            "SELECT substr(CAST(description AS BLOB),1,1025) FROM catalog_descriptions WHERE kind=?1 AND id=?2",
            rusqlite::params![kind.column(), id],
            |row| row.get(0),
        )
        .optional()?;
    value
        .map(|bytes| {
            ensure!(
                bytes.len() <= airk_core::index_catalog::MAX_DESCRIPTION_BYTES,
                "catalog description exceeds byte bound"
            );
            Ok(String::from_utf8(bytes)?)
        })
        .transpose()
}

/// Set (`Some`, installing the layer when needed) or clear (`None`, never
/// installing it) one description. The caller has fenced the definition.
pub(super) fn store(
    db: &Connection,
    kind: DescriptionKind,
    id: &str,
    description: Option<&str>,
) -> Result<()> {
    airk_core::index_catalog::description(description)?;
    match description {
        Some(text) => {
            apply_layout(db)?;
            db.execute(
                "INSERT INTO catalog_descriptions(kind,id,description) VALUES(?1,?2,?3) ON CONFLICT(kind,id) DO UPDATE SET description=excluded.description",
                rusqlite::params![kind.column(), id, text],
            )?;
        }
        None => {
            if installed(db)? {
                db.execute(
                    "DELETE FROM catalog_descriptions WHERE kind=?1 AND id=?2",
                    rusqlite::params![kind.column(), id],
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
