// SPDX-License-Identifier: Apache-2.0
//! Saved canonical-path comparisons only. No source or collection is opened.
use super::*;
use airk_core::{
    index_catalog::{IndexId, SourceRootId, encoded_size},
    source_observations::*,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub fn source_relation(proposed: &Path, existing: &Path) -> Result<Option<SourceRelation>> {
    let contains = super::source_isolation::contains(proposed, existing)?;
    let contained = super::source_isolation::contains(existing, proposed)?;
    Ok(match (contains, contained) {
        (true, true) => Some(SourceRelation::Same),
        (true, false) => Some(SourceRelation::Contains),
        (false, true) => Some(SourceRelation::ContainedBy),
        (false, false) => None,
    })
}

fn hash_part(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
}

fn text(row: &rusqlite::Row<'_>, column: usize, maximum: usize) -> Result<String> {
    let bytes: Vec<u8> = row.get(column)?;
    ensure!(
        !bytes.is_empty() && bytes.len() <= maximum,
        "source observation metadata field exceeds bound or is missing"
    );
    Ok(String::from_utf8(bytes)?)
}
fn boolean(row: &rusqlite::Row<'_>, column: usize) -> Result<bool> {
    let value: i64 = row.get(column)?;
    ensure!(matches!(value, 0 | 1), "invalid source observation boolean");
    Ok(value == 1)
}

struct Observation<'a> {
    request: &'a SourceRelationshipsRequest,
    page: SourceRelationships,
    hash: Sha256,
    encoded_rows: usize,
    reply_reserve: usize,
    deadline: std::time::Instant,
}
#[derive(serde::Serialize)]
struct SavedRoot {
    binding: ProjectBinding,
    index_id: Option<IndexId>,
    source_root_id: Option<SourceRootId>,
    collection: String,
    root: String,
    registered: bool,
    enabled: Option<bool>,
}
impl Observation<'_> {
    fn root(&mut self, saved: SavedRoot) -> Result<()> {
        ensure!(
            std::time::Instant::now() < self.deadline,
            "source relationship metadata deadline exceeded; observation incomplete"
        );
        self.page.inspected_roots += 1;
        ensure!(
            self.page.inspected_roots <= 1024 * 17,
            "source relationship root count exceeds bound"
        );
        self.page.root_metadata_bytes = self
            .page
            .root_metadata_bytes
            .checked_add(saved.root.len())
            .context("source root metadata overflow")?;
        ensure!(
            self.page.root_metadata_bytes <= MAX_ROOT_METADATA_BYTES,
            "source relationship root metadata exceeds byte bound"
        );
        hash_part(&mut self.hash, &serde_json::to_vec(&saved)?);
        let Some(relation) =
            source_relation(Path::new(&self.request.root), Path::new(&saved.root))?
        else {
            return Ok(());
        };
        let offset = self.page.matching_roots;
        self.page.matching_roots += 1;
        let skipped = self
            .request
            .after
            .as_ref()
            .map_or(0, |cursor| cursor.offset);
        if offset < skipped || self.page.truncated {
            return Ok(());
        }
        let row = SourceRelationship {
            binding: saved.binding,
            index_id: saved.index_id,
            source_root_id: saved.source_root_id,
            collection: saved.collection,
            root: saved.root,
            relation,
            registered: saved.registered,
            enabled: saved.enabled,
        };
        let bytes = encoded_size(&row, 256 * 1024)?;
        if self.page.relationships.len() >= self.request.limit
            || self.reply_reserve + self.encoded_rows + bytes + 1 > self.request.max_bytes
        {
            ensure!(
                !self.page.relationships.is_empty(),
                "source relationship entry exceeds reply budget; increase max_bytes or use a narrower observation root"
            );
            self.page.truncated = true;
        } else {
            self.encoded_rows += bytes + 1;
            self.page.relationships.push(row);
        }
        Ok(())
    }
}

impl ServiceRegistry {
    /// Explicit administrative or already scoped metadata read. Scoped adapters
    /// establish the selection before calling; request IDs alone grant no access.
    /// A bounded complete metadata pass authenticates pagination without caching
    /// another index or opening unselected source/index files.
    pub fn source_relationships(
        &self,
        request: &SourceRelationshipsRequest,
    ) -> Result<SourceRelationships> {
        request.validate()?;
        let tx = self.connection.unchecked_transaction()?;
        let version: u32 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        ensure!(
            matches!(version, 11..=13),
            "source observations require registry schema 11, 12 or 13; use the native installer to upgrade"
        );
        let namespace: String = tx.query_row(
            "SELECT incarnation FROM service_meta WHERE id=1",
            [],
            |row| row.get(0),
        )?;
        let namespace: Incarnation = namespace.parse()?;
        let selected = request.bindings.as_ref().map(|bindings| {
            bindings
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        });
        let mut hash = Sha256::new();
        hash_part(&mut hash, b"airk/source-relationships/1");
        hash_part(&mut hash, &version.to_be_bytes());
        hash_part(&mut hash, namespace.to_string().as_bytes());
        hash_part(&mut hash, request.root.as_bytes());
        hash_part(&mut hash, &serde_json::to_vec(&selected)?);
        let mut observation = Observation {
            request,
            page: SourceRelationships {
                schema: 1,
                namespace,
                observed_unix_ms: u64::try_from(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_millis(),
                )?,
                root: request.root.clone(),
                basis: if matches!(version, 12 | 13) {
                    RootObservationBasis::CatalogSources
                } else {
                    RootObservationBasis::LegacyOwnerAndWatchCache
                },
                selected_bindings: selected.clone(),
                relationships: vec![],
                matching_roots: 0,
                inspected_roots: 0,
                root_metadata_bytes: 0,
                cached_alias_metadata_bytes: 0,
                unknown_assignment_indexes: 0,
                snapshot: Sha256Digest::from_bytes([0; 32]),
                truncated: false,
                after: None,
            },
            hash,
            encoded_rows: 0,
            reply_reserve: 0,
            deadline: std::time::Instant::now() + Duration::from_secs(2),
        };
        // Include the actual escaped root/selection envelope, plus room for the
        // final cursor and bounded counter widths. A fixed path-size reserve
        // would undercount legal POSIX names containing JSON escapes.
        observation.reply_reserve = encoded_size(&observation.page, MAX_RELATIONSHIP_BYTES)? + 512;
        let selection_column = if matches!(version, 12 | 13) {
            "i.legacy_binding"
        } else {
            "p.binding"
        };
        let filter = selected
            .as_ref()
            .map(|bindings| {
                format!(
                    " WHERE {selection_column} IN ({})",
                    vec!["?"; bindings.len()].join(",")
                )
            })
            .unwrap_or_default();
        let parameters = selected
            .as_ref()
            .map(|bindings| bindings.iter().map(ToString::to_string).collect::<Vec<_>>())
            .unwrap_or_default();
        let mut seen = BTreeSet::new();
        if matches!(version, 12 | 13) {
            let selected_predicate = selected
                .as_ref()
                .map(|bindings| {
                    format!(
                        "p.binding IN ({}) AND ",
                        vec!["?"; bindings.len()].join(",")
                    )
                })
                .unwrap_or_default();
            let unmapped = format!(
                "SELECT EXISTS(SELECT 1 FROM projects p WHERE {selected_predicate} NOT EXISTS(SELECT 1 FROM index_definitions i WHERE i.legacy_binding=p.binding))"
            );
            let missing: bool =
                tx.query_row(&unmapped, rusqlite::params_from_iter(&parameters), |row| {
                    row.get(0)
                })?;
            ensure!(
                !missing,
                "catalog legacy index mappings are incomplete; inspect the owned upgrade or repair"
            );
            let sql = format!(
                "SELECT substr(CAST(i.legacy_binding AS BLOB),1,65),substr(CAST(i.id AS BLOB),1,37),substr(CAST(i.collection AS BLOB),1,129),coalesce(p.registered,0)=1 AND i.tombstone=0,substr(CAST(i.source_import_state AS BLOB),1,33),substr(CAST(s.id AS BLOB),1,37),substr(CAST(s.canonical_path AS BLOB),1,32769),s.enabled FROM index_definitions i LEFT JOIN projects p ON p.binding=i.active_registration LEFT JOIN index_sources s ON s.index_id=i.id{filter} ORDER BY i.legacy_binding,s.id LIMIT 17409"
            );
            let mut statement = tx.prepare(&sql)?;
            let mut rows = statement.query(rusqlite::params_from_iter(&parameters))?;
            let mut previous = None;
            while let Some(row) = rows.next()? {
                let binding = text(row, 0, 64)?;
                let binding: ProjectBinding = binding.parse()?;
                seen.insert(binding);
                let state = text(row, 4, 32)?;
                ensure!(
                    matches!(state.as_str(), "complete" | "pending_metadata"),
                    "unknown source assignment state"
                );
                if previous != Some(binding) {
                    ensure!(
                        seen.len() <= MAX_PROJECTS,
                        "source relationship index count exceeds bound"
                    );
                    if state != "complete" {
                        observation.page.unknown_assignment_indexes += 1;
                    }
                    hash_part(&mut observation.hash, state.as_bytes());
                    previous = Some(binding);
                }
                let source = text(row, 5, 36)?;
                let root = text(row, 6, 32768)?;
                let index = text(row, 1, 36)?;
                let collection = text(row, 2, 128)?;
                let registered = boolean(row, 3)?;
                let enabled = boolean(row, 7)?;
                observation.root(SavedRoot {
                    binding,
                    index_id: Some(index.parse()?),
                    source_root_id: Some(source.parse()?),
                    collection,
                    root,
                    registered,
                    enabled: Some(enabled),
                })?;
            }
        } else {
            let sql = format!(
                "SELECT substr(p.payload,1,20481),p.registered,substr(coalesce(w.roots,x'5b5d'),1,98305),substr(CAST(p.binding AS BLOB),1,65) FROM projects p LEFT JOIN service_watch w ON w.project=p.binding{filter} ORDER BY p.binding LIMIT 1025"
            );
            let mut statement = tx.prepare(&sql)?;
            let mut rows = statement.query(rusqlite::params_from_iter(&parameters))?;
            while let Some(row) = rows.next()? {
                let payload: Vec<u8> = row.get(0)?;
                let existing = super::decode(&payload, 0, false)?.registration;
                ensure!(
                    text(row, 3, 64)? == existing.binding.to_string(),
                    "crossed source observation binding"
                );
                ensure!(
                    seen.len() < MAX_PROJECTS,
                    "source relationship index count exceeds bound"
                );
                seen.insert(existing.binding);
                let roots: Vec<u8> = row.get(2)?;
                observation.page.cached_alias_metadata_bytes = observation
                    .page
                    .cached_alias_metadata_bytes
                    .checked_add(roots.len())
                    .context("source alias metadata overflow")?;
                ensure!(
                    observation.page.cached_alias_metadata_bytes <= MAX_ROOT_METADATA_BYTES,
                    "source relationship cached root metadata exceeds byte bound"
                );
                let roots = super::watch::roots(&roots)?;
                let roots = std::iter::once(existing.root)
                    .chain(roots)
                    .collect::<BTreeSet<_>>();
                observation.page.unknown_assignment_indexes += 1;
                let registered = boolean(row, 1)?;
                for root in roots {
                    observation.root(SavedRoot {
                        binding: existing.binding,
                        index_id: None,
                        source_root_id: None,
                        collection: existing.collection.clone(),
                        root,
                        registered,
                        enabled: None,
                    })?;
                }
            }
        }
        if let Some(selected) = selected {
            ensure!(
                selected.iter().all(|binding| seen.contains(binding)),
                "source observation selected an unknown binding"
            );
        }
        let snapshot = Sha256Digest::from_bytes(observation.hash.finalize().into());
        if let Some(cursor) = &request.after {
            ensure!(
                cursor.namespace == namespace
                    && cursor.snapshot == snapshot
                    && cursor.offset <= observation.page.matching_roots,
                "source relationship continuation expired or changed scope"
            );
        }
        observation.page.snapshot = snapshot;
        if observation.page.truncated {
            observation.page.after = Some(RelationshipCursor {
                namespace,
                snapshot,
                offset: request.after.as_ref().map_or(0, |cursor| cursor.offset)
                    + observation.page.relationships.len(),
            });
        }
        encoded_size(&observation.page, request.max_bytes)?;
        Ok(observation.page)
    }
}

#[cfg(test)]
mod tests;
