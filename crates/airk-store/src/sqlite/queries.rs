// SPDX-License-Identifier: Apache-2.0
//! Exact fact discovery and bounded relationship traversal.
use super::*;

impl FileStore {
    /// None means a legacy JSON snapshot; the kernel retains its compatibility path.
    pub fn facts(
        &self,
        root: &str,
        collection: &str,
        q: &FactQuery,
        code_only: bool,
    ) -> Result<Option<FactsResponse>> {
        self.facts_page(root, collection, q, code_only, &Default::default())
    }

    pub fn facts_page(
        &self,
        root: &str,
        collection: &str,
        q: &FactQuery,
        code_only: bool,
        options: &airk_core::discovery::DiscoveryOptions,
    ) -> Result<Option<FactsResponse>> {
        use airk_core::discovery::{self, DiscoveryKey};
        ensure!(
            !options.compact,
            "compact representation is supported by outline only"
        );
        let _lifecycle = self.lifecycle(false)?;
        if !self.directory.join("index.sqlite3").exists() {
            return Ok(None);
        }
        let conn = read_transaction(&self.directory)?;
        let tx = &conn;
        let generation = identity(tx, root, collection)?;
        let mut response = FactsResponse {
            page: Default::default(),
            outline: Vec::new(),
            indexing: conn.detail.into(),
            generation,
            publication: conn.publication.clone(),
            collection: collection.into(),
            facts: Vec::new(),
            truncated: false,
        };
        let revision = conn.publication.as_ref().map(|p| p.token.to_string());
        ensure!(
            revision.is_some() || options.cursor.is_none(),
            "legacy discovery cannot resume; reindex first"
        );
        let mut filters = q.clone();
        filters.limit = 0;
        filters.max_bytes = 0;
        let binding = discovery::binding(
            revision.as_deref().unwrap_or("legacy"),
            if code_only { "code_facts" } else { "facts" },
            (root, collection, filters),
        )?;
        let after = discovery::decode(options.cursor.as_deref(), &binding)?;
        response.page.record_bytes = Some(0);
        let name_filter = if q.name.is_none() {
            "1"
        } else if q.exact_name {
            "(name=?7 OR qualified_name=?7 OR symbol_id=?7)"
        } else {
            "instr(folded_name,?9)>0"
        };
        let projection = if crate::normalized::present(tx)? {
            crate::normalized::fact_projection(true)
        } else {
            "SELECT rowid AS discovery_id,* FROM facts".into()
        };
        // Restore the complete ordering key from its immutable row identity,
        // keeping even very large names out of continuation arguments.
        let after = after.map(|key| -> Result<DiscoveryKey> {
            let restored = tx.query_row(
                &format!("SELECT path,line,kind,name,discovery_id FROM ({projection}) WHERE discovery_id=?1 AND path=?2"),
                params![key.ordinal, key.path], |row| Ok((row.get::<_,String>(0)?, row.get::<_,i64>(1)?, row.get::<_,String>(2)?, row.get::<_,String>(3)?, row.get::<_,i64>(4)?))
            ).optional()?.context("discovery cursor position is unavailable")?;
            Ok(DiscoveryKey { path: restored.0, line: usize::try_from(restored.1)?, kind: restored.2, name: restored.3, ordinal: restored.4 })
        }).transpose()?;
        let mut last = after.clone();
        let mut stmt = tx.prepare(&format!(
            "SELECT payload,path,line,kind,name,discovery_id FROM ({projection}) WHERE
            (?1=0 OR is_code=1) AND (?2=0 OR is_document=1)
            AND (?3 IS NULL OR kind=?3) AND (?4 IS NULL OR language=?4)
            AND ((?5=1 AND path=?6) OR (?5=0 AND substr(path,1,length(?6))=?6))
            AND {name_filter}
            AND (?11 IS NULL OR (path COLLATE BINARY,line,kind COLLATE BINARY,name COLLATE BINARY,discovery_id) > (?11,?12,?13,?14,?15))
            ORDER BY path COLLATE BINARY,line,kind COLLATE BINARY,name COLLATE BINARY,discovery_id LIMIT ?10"
        ))?;
        let mut rows = stmt.query(params![
            code_only,
            q.document_only,
            q.kind,
            q.language,
            q.path_exact,
            q.path_prefix,
            q.name,
            q.exact_name,
            q.name.as_ref().map(|n| n.to_lowercase()),
            i64::try_from(q.limit + 1)?,
            after.as_ref().map(|k| &k.path),
            after.as_ref().map(|k| k.line as i64),
            after.as_ref().map(|k| &k.kind),
            after.as_ref().map(|k| &k.name),
            after.as_ref().map(|k| k.ordinal)
        ])?;
        while let Some(row) = rows.next()? {
            let payload: String = row.get(0)?;
            let hit: FactHit = serde_json::from_str(&payload)?;
            let key = DiscoveryKey {
                path: row.get(1)?,
                line: usize::try_from(row.get::<_, i64>(2)?)?,
                kind: row.get(3)?,
                name: row.get(4)?,
                ordinal: row.get(5)?,
            };
            if !discovery::append_fact(&mut response, hit, false, q.limit, q.max_bytes)? {
                break;
            }
            last = Some(key);
        }
        if response.truncated && revision.is_some() {
            response.page.cursor = last
                .as_ref()
                .map(|key| discovery::encode(&binding, key))
                .transpose()?;
        }
        Ok(Some(response))
    }

    pub fn relationships(
        &self,
        root: &str,
        collection: &str,
        q: &RelationshipQuery,
    ) -> Result<RelationshipsResponse> {
        let _lifecycle = self.lifecycle(false)?;
        let conn = read_transaction(&self.directory)?;
        let tx = &conn;
        let generation = identity(tx, root, collection)?;
        let mut response = RelationshipsResponse {indexing:conn.detail.into(),generation,publication:conn.publication.clone(),collection:collection.into(),relationships:Vec::new(),truncated:false,coverage:"Selected syntax patterns and optional manifest-bound producer assertions. External targets are identifiers only; impact is incomplete.".into()};
        let mut pending = BTreeSet::from([q.symbol.clone()]);
        let mut visited = BTreeSet::new();
        let mut returned = BTreeSet::new();
        let deadline = Instant::now() + Duration::from_millis(250);
        let mut remaining = q.max_bytes;
        for _ in 0..q.hops {
            let mut next = BTreeSet::new();
            for symbol in pending {
                if !visited.insert(symbol.clone()) {
                    continue;
                }
                if visited.len() > 100 || Instant::now() > deadline {
                    response.truncated = true;
                    return Ok(response);
                }
                // Separate indexed branches prevent scanning the whole graph per hop.
                let sql = if q.direction == "incoming" {
                    "SELECT payload FROM relationships WHERE target=?1 AND (?2 IS NULL OR kind=?2) AND substr(path,1,length(?3))=?3 ORDER BY path,line,source,target LIMIT ?4"
                } else {
                    "SELECT payload FROM relationships WHERE source=?1 AND (?2 IS NULL OR kind=?2) AND substr(path,1,length(?3))=?3 ORDER BY path,line,source,target LIMIT ?4"
                };
                let mut stmt = tx.prepare(sql)?;
                let mut rows = stmt.query(params![
                    symbol,
                    q.kind,
                    q.path_prefix,
                    i64::try_from(q.limit + 1)?
                ])?;
                while let Some(row) = rows.next()? {
                    let payload: String = row.get(0)?;
                    if !returned.insert(payload.clone()) {
                        continue;
                    }
                    if response.relationships.len() == q.limit
                        || payload.len() > remaining
                        || Instant::now() > deadline
                    {
                        response.truncated = true;
                        return Ok(response);
                    }
                    let edge: RelationshipHit = serde_json::from_str(&payload)?;
                    remaining -= payload.len();
                    next.insert(if q.direction == "incoming" {
                        edge.source.clone()
                    } else {
                        edge.target.clone()
                    });
                    response.relationships.push(edge);
                }
            }
            pending = next;
            if pending.is_empty() {
                break;
            }
        }
        Ok(response)
    }
}
