// SPDX-License-Identifier: Apache-2.0
//! Exact-revision, authenticated compact source measurements for usage.
use super::*;

impl FileStore {
    /// Resolve bounded counters from exactly the response's committed revision.
    /// The lease protects old generations while their small rows are inspected.
    pub fn usage_source_sizes(
        &self,
        root: &str,
        collection: &str,
        token: &airk_core::identity::RevisionToken,
        sources: &BTreeSet<(&str, &str)>,
    ) -> Result<crate::UsageSourceSizes> {
        Ok(self
            .usage_source_sizes_measured(root, collection, token, sources)?
            .0)
    }

    /// Identical bounded lookup with per-call verification costs for diagnostic
    /// fixtures. No timing-derived cache or cross-request trust is introduced.
    pub fn usage_source_sizes_measured(
        &self,
        root: &str,
        collection: &str,
        token: &airk_core::identity::RevisionToken,
        sources: &BTreeSet<(&str, &str)>,
    ) -> Result<(crate::UsageSourceSizes, crate::UsageReadMetrics)> {
        let started = Instant::now();
        let mut metrics = crate::UsageReadMetrics::default();
        let result =
            self.usage_source_sizes_inner(root, collection, token, sources, &mut metrics)?;
        metrics.lookup_us = (started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64)
            .saturating_sub(metrics.verification_us);
        Ok((result, metrics))
    }

    fn usage_source_sizes_inner(
        &self,
        root: &str,
        collection: &str,
        token: &airk_core::identity::RevisionToken,
        sources: &BTreeSet<(&str, &str)>,
        metrics: &mut crate::UsageReadMetrics,
    ) -> Result<crate::UsageSourceSizes> {
        use crate::UsageSourceSizes;
        use airk_core::usage::{BaselineUnavailable as Missing, SourceContextSize};
        ensure!(
            sources.len() <= 100
                && sources
                    .iter()
                    .all(|(p, h)| p.len() <= 4096 && crate::blobs::valid_digest(h)),
            "usage source bounds exceeded"
        );
        let _lifecycle = self.lifecycle(false)?;
        let Some(header) = self.persistent_revision()? else {
            return Ok(UsageSourceSizes::Unavailable(Missing::RevisionUnavailable));
        };
        ensure!(
            header.root == root && header.collection == collection,
            "usage collection binding differs"
        );
        let Some(current) = header.token else {
            return Ok(UsageSourceSizes::Legacy);
        };
        token.validate_against(&current)?;
        validate_retained_token(self.directory(), token)?;
        let Some(receipt) = self.receipt(token)? else {
            return Ok(UsageSourceSizes::Unavailable(Missing::RevisionUnavailable));
        };
        if retirement_proof(self.directory(), token)?.is_some() {
            return Ok(UsageSourceSizes::Unavailable(Missing::RevisionUnavailable));
        }
        let lease = match std::fs::File::open(crate::blobs::reader_lease_path(
            self.directory(),
            &token.revision.to_string(),
        )?) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(UsageSourceSizes::Unavailable(Missing::RevisionUnavailable));
            }
            Err(error) => return Err(error.into()),
        };
        lease.lock_shared()?;
        let directory = self
            .directory()
            .join("generations")
            .join(token.revision.to_string());
        // Historical text-only packages have no full catalog; the caller may use
        // its bounded legacy path, but must never substitute current evidence.
        if !directory.try_exists()? {
            return Ok(UsageSourceSizes::Unavailable(Missing::RevisionUnavailable));
        }
        // Baselines consume only catalog counters. Authenticate their exact
        // receipt-bound bytes, not unrelated lexical/vector/model payloads that
        // retrieval already authenticates independently. The component set is
        // still checked so SQLite sidecars cannot overlay the catalog.
        crate::blobs::verify_usage_catalog(self.directory(), &receipt, metrics)?;
        let conn = open_immutable_catalog(&directory.join("catalog.sqlite"))?;
        validate_v2_catalog(&conn, token, &receipt.inventory_digest)?;
        ensure!(
            identity(&conn, root, collection)? == token.generation,
            "usage catalog revision differs"
        );
        if !table_exists(&conn, "usage_sources")? {
            return Ok(UsageSourceSizes::Legacy);
        }
        let deadline = Instant::now() + Duration::from_millis(250);
        conn.progress_handler(1000, Some(move || Instant::now() > deadline))?;
        let mut sizes = Vec::with_capacity(sources.len());
        for (path, hash) in sources {
            let row: Option<(String, Option<String>)> = conn.query_row(
                "SELECT source_hash,CASE WHEN length(CAST(payload AS BLOB)) <= 1024 THEN payload END FROM usage_sources WHERE path=?1",
                [path], |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
            let Some((stored_hash, payload)) = row else {
                return Ok(UsageSourceSizes::Unavailable(Missing::SourceUnavailable));
            };
            if stored_hash != *hash {
                return Ok(UsageSourceSizes::Unavailable(Missing::SourceMismatch));
            }
            let size: SourceContextSize =
                serde_json::from_str(&payload.context("usage source measurement exceeds bound")?)?;
            ensure!(
                size.source_hash == stored_hash,
                "usage source identity differs"
            );
            sizes.push(size);
        }
        Ok(UsageSourceSizes::Available(sizes))
    }
}
