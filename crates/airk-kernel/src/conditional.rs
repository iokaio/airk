// SPDX-License-Identifier: Apache-2.0
//! Conditional representations are built from one leased catalog only.
use crate::{FileInfo, Kernel};
use airk_core::{
    ArtifactSource, Citation, FactHit, FactsResponse, SearchResponse,
    evidence::{ConditionalRead, EvidenceReadOptions, EvidenceReadStatus, ReadEvidence},
    identity::framed_digest,
    validate_budget,
};
use airk_store::blobs::Catalog;
use anyhow::{Context, Result, ensure};
use serde::Serialize;

impl Kernel {
    fn conditional<T: Serialize>(
        &self,
        catalog: &Catalog,
        mut read: ReadEvidence,
        options: &EvidenceReadOptions,
        selection: impl Serialize,
        payload: T,
    ) -> Result<ConditionalRead<T>> {
        if catalog.receipt.is_none() {
            return Ok(ConditionalRead {
                read,
                content: Some(payload),
            });
        }
        let path = read
            .resolved_path
            .as_deref()
            .context("resolved path missing")?;
        let file = catalog.file(path)?.context("resolved file missing")?;
        let mut representation = serde_json::to_value(&payload)?;
        if let Some(fields) = representation.as_object_mut() {
            fields.remove("publication");
            fields.remove("generation");
        }
        let validator = framed_digest(
            b"read-representation/1",
            &[
                file.state.payload_digest.as_bytes(),
                file.state.evidence_digest.as_bytes(),
                file.state.profile_id.as_bytes(),
                file.state.root.as_bytes(),
                file.state.path.as_bytes(),
                &serde_json::to_vec(&selection)?,
                &serde_json::to_vec(&representation)?,
            ],
        );
        read.validator = Some(validator);
        let content = if options.if_evidence == Some(validator) {
            read.status = EvidenceReadStatus::NotModified;
            None
        } else {
            Some(payload)
        };
        Ok(ConditionalRead { read, content })
    }

    pub fn file_info_conditional(
        &self,
        path: &str,
        options: &EvidenceReadOptions,
    ) -> Result<ConditionalRead<FileInfo>> {
        let catalog = self.catalog()?;
        let read = self.resolve_evidence_path(&catalog, path, options)?;
        if read.status != EvidenceReadStatus::Content {
            return Ok(ConditionalRead {
                read,
                content: None,
            });
        }
        let resolved = read
            .resolved_path
            .as_deref()
            .context("resolved path missing")?;
        let payload = self.file_info_in(&catalog, resolved)?;
        self.conditional(&catalog, read, options, "file_info/1", payload)
    }

    pub fn read_file_conditional(
        &self,
        path: &str,
        start: usize,
        end: usize,
        max_bytes: usize,
        options: &EvidenceReadOptions,
    ) -> Result<ConditionalRead<SearchResponse>> {
        validate_budget(1, max_bytes)?;
        ensure!(start > 0 && end >= start, "invalid line range");
        let catalog = self.catalog()?;
        let read = self.resolve_evidence_path(&catalog, path, options)?;
        if read.status != EvidenceReadStatus::Content {
            return Ok(ConditionalRead {
                read,
                content: None,
            });
        }
        let resolved = read
            .resolved_path
            .as_deref()
            .context("resolved path missing")?;
        let artifact = catalog.load_artifact(resolved)?;
        let payload = Self::read_snapshot(
            Self::catalog_envelope(&catalog),
            &catalog.snapshot,
            &artifact,
            resolved,
            start,
            end,
            max_bytes,
        )?;
        self.conditional(
            &catalog,
            read,
            options,
            ("read_file/1", start, end, max_bytes),
            payload,
        )
    }

    pub fn context_conditional(
        &self,
        path: &str,
        line: usize,
        before: usize,
        after: usize,
        max_bytes: usize,
        options: &EvidenceReadOptions,
    ) -> Result<ConditionalRead<SearchResponse>> {
        validate_budget(1, max_bytes)?;
        ensure!(
            before <= 100 && after <= 100,
            "context radius must be 0..100"
        );
        ensure!(line > 0, "line is outside indexed source");
        let catalog = self.catalog()?;
        let read = self.resolve_evidence_path(&catalog, path, options)?;
        if read.status != EvidenceReadStatus::Content {
            return Ok(ConditionalRead {
                read,
                content: None,
            });
        }
        let resolved = read
            .resolved_path
            .as_deref()
            .context("resolved path missing")?;
        let artifact = catalog.load_artifact(resolved)?;
        let lines = artifact.chunks.last().map_or(0, |chunk| chunk.end_line);
        ensure!(line <= lines, "line is outside indexed source");
        let payload = Self::read_snapshot(
            Self::catalog_envelope(&catalog),
            &catalog.snapshot,
            &artifact,
            resolved,
            line.saturating_sub(before).max(1),
            line.saturating_add(after).min(lines),
            max_bytes,
        )?;
        self.conditional(
            &catalog,
            read,
            options,
            ("context/1", line, before, after, max_bytes),
            payload,
        )
    }

    pub fn outline_conditional(
        &self,
        path: &str,
        limit: usize,
        max_bytes: usize,
        options: &EvidenceReadOptions,
    ) -> Result<ConditionalRead<FactsResponse>> {
        validate_budget(limit, max_bytes)?;
        let catalog = self.catalog()?;
        let read = self.resolve_evidence_path(&catalog, path, options)?;
        if read.status != EvidenceReadStatus::Content {
            return Ok(ConditionalRead {
                read,
                content: None,
            });
        }
        let resolved = read
            .resolved_path
            .as_deref()
            .context("resolved path missing")?;
        let artifact = catalog.load_artifact(resolved)?;
        let mut facts: Vec<_> = artifact
            .facts
            .iter()
            .map(|fact| FactHit {
                name: fact.name.clone(),
                kind: fact.kind.clone(),
                syntax_kind: fact.syntax_kind.clone(),
                language: artifact.language.clone(),
                document: fact.document.clone(),
                code: fact.code.clone(),
                citation: Citation {
                    path: resolved.into(),
                    content_hash: artifact.content_hash.clone(),
                    start_line: fact.start_line,
                    end_line: fact.end_line,
                    location: fact.location.clone(),
                },
            })
            .chain(
                catalog
                    .snapshot
                    .imported_facts
                    .iter()
                    .filter(|fact| fact.citation.path == resolved)
                    .cloned(),
            )
            .collect();
        facts.sort_by(|a, b| {
            (a.citation.start_line, &a.kind, &a.name).cmp(&(
                b.citation.start_line,
                &b.kind,
                &b.name,
            ))
        });
        let mut payload = FactsResponse {
            publication: Self::catalog_envelope(&catalog),
            generation: catalog.snapshot.generation,
            collection: catalog.snapshot.collection.clone(),
            facts: vec![],
            truncated: false,
        };
        let mut remaining = max_bytes;
        for fact in facts {
            let bytes = fact.name.len()
                + fact.citation.path.len()
                + fact
                    .document
                    .as_ref()
                    .map(serde_json::to_vec)
                    .transpose()?
                    .map_or(0, |v| v.len())
                + fact
                    .code
                    .as_ref()
                    .map(serde_json::to_vec)
                    .transpose()?
                    .map_or(0, |v| v.len());
            if payload.facts.len() == limit || bytes > remaining {
                payload.truncated = true;
                break;
            }
            remaining -= bytes;
            payload.facts.push(fact);
        }
        self.conditional(
            &catalog,
            read,
            options,
            ("outline/1", limit, max_bytes),
            payload,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        SnapshotStore,
        changes::{ChangeCause, ChangesRequest, PublicationReason},
    };

    #[test]
    fn validators_survive_unrelated_publications_but_bind_profile_and_selection() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        let source = b"fn fictional_entry() {\n    let answer = 42;\n}\n";
        std::fs::write(root.path().join("one.rs"), source)?;
        std::fs::write(root.path().join("other.txt"), "Before unrelated edit.\n")?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.index()?;
        let empty = EvidenceReadOptions::default();
        let first = kernel.read_file_conditional("one.rs", 1, 1, 4096, &empty)?;
        let options = EvidenceReadOptions {
            if_evidence: first.read.validator,
            ..empty.clone()
        };
        std::fs::write(root.path().join("other.txt"), "After unrelated edit.\n")?;
        kernel.index()?;
        let reused = kernel.read_file_conditional("one.rs", 1, 1, 4096, &options)?;
        assert_eq!(reused.read.status, EvidenceReadStatus::NotModified);
        assert!(
            reused.read.publication.as_ref().unwrap().generation
                > first.read.publication.as_ref().unwrap().generation
        );
        assert!(reused.content.is_none());
        assert_eq!(
            kernel
                .read_file_conditional("one.rs", 1, 2, 4096, &options)?
                .read
                .status,
            EvidenceReadStatus::Content
        );
        kernel.reindex()?;
        assert_eq!(
            kernel
                .read_file_conditional("one.rs", 1, 1, 4096, &options)?
                .read
                .status,
            EvidenceReadStatus::NotModified
        );
        let baseline = kernel.catalog()?.receipt.as_ref().unwrap().token;
        let _writer = kernel.store.lock()?;
        let mut snapshot = kernel.store.load()?.unwrap();
        let artifact = kernel.store.load_retained(&snapshot.artifacts["one.rs"])?;
        let mut profile = airk_ingest::adapters::legacy::legacy_profile(&artifact)?;
        let payload =
            airk_ingest::adapters::legacy::convert_native_legacy(&artifact, source, &profile)?;
        profile.adapter_version = "original-test-profile/2".into();
        let reservation = kernel.publication_reservation()?;
        let compact = kernel.store.retain_payload_with_budget(
            &artifact,
            &payload,
            &profile,
            Some(source),
            Some(&reservation),
        )?;
        snapshot.artifacts.insert("one.rs".into(), compact);
        kernel.publish_v2(
            &_writer,
            &snapshot,
            reservation,
            PublicationReason::PolicyChange,
        )?;
        let changed = kernel.read_file_conditional(
            "one.rs",
            1,
            1,
            4096,
            &EvidenceReadOptions {
                known_hash: Some(artifact.content_hash.parse()?),
                ..options
            },
        )?;
        assert_eq!(changed.read.status, EvidenceReadStatus::Content);
        assert_ne!(changed.read.validator, first.read.validator);
        assert_eq!(
            changed.content.unwrap().hits[0].text,
            first.content.unwrap().hits[0].text
        );
        let changes = kernel.changes(&ChangesRequest {
            since: Some(baseline),
            max_bytes: 65536,
            ..Default::default()
        })?;
        assert!(
            changes
                .changes
                .iter()
                .any(|change| change.causes.contains(&ChangeCause::ProfileChanged))
        );
        Ok(())
    }
}
