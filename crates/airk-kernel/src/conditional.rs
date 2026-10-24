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
        if options.revision.is_some() {
            return self.historical_info(path, options);
        }
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
        if options.revision.is_some() {
            return self.historical_file(path, start, end, max_bytes, options);
        }
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
        let artifact = self.source_line_artifact(&catalog, artifact)?;
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
        if options.revision.is_some() {
            return self.historical_context(path, line, before, after, max_bytes, options);
        }
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
        let artifact = self.source_line_artifact(&catalog, artifact)?;
        let lines = artifact
            .chunks
            .iter()
            .map(|chunk| chunk.end_line)
            .max()
            .unwrap_or(0);
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
        self.outline_conditional_page(path, limit, max_bytes, options, &Default::default())
    }

    pub fn outline_conditional_page(
        &self,
        path: &str,
        limit: usize,
        max_bytes: usize,
        options: &EvidenceReadOptions,
        page: &airk_core::discovery::DiscoveryOptions,
    ) -> Result<ConditionalRead<FactsResponse>> {
        validate_budget(limit, max_bytes)?;
        if options.revision.is_some() {
            return self.historical_outline(path, limit, max_bytes, options, page);
        }
        let catalog = self.catalog()?;
        let publication = Self::catalog_envelope(&catalog);
        ensure!(
            publication.is_some() || page.cursor.is_none(),
            "legacy discovery cannot resume; reindex first"
        );
        let binding = airk_core::discovery::binding(
            &publication
                .as_ref()
                .map(|p| p.token.to_string())
                .unwrap_or_else(|| "legacy".into()),
            "outline",
            (
                path,
                page.compact,
                options.follow_moves,
                options.artifact_id,
                options.known_hash,
            ),
        )?;
        airk_core::discovery::decode(page.cursor.as_deref(), &binding)?;
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
        let continuity = catalog
            .inventory
            .as_ref()
            .and_then(|inventory| inventory.files.get(resolved))
            .map_or(&[][..], |file| file.continuity.as_slice());
        let continuity = airk_core::continuity::FactKeyIndex::new(continuity);
        let mut facts: Vec<_> = artifact
            .facts
            .iter()
            .enumerate()
            .map(|(ordinal, fact)| FactHit {
                candidate: fact.code.as_ref().is_some_and(|code| code.is_call_candidate(&fact.kind)),
                continuity_key: continuity.key(ordinal, fact),
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
                    .cloned()
                    .map(|mut fact| {
                        fact.continuity_key = None;
                        fact
                    }),
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
            minimal: None,
            page: Default::default(),
            outline: Vec::new(),
            indexing: catalog
                .inventory
                .as_ref()
                .and_then(|i| i.index_detail)
                .unwrap_or_default()
                .into(),
            publication: Self::catalog_envelope(&catalog),
            generation: catalog.snapshot.generation,
            collection: catalog.snapshot.collection.clone(),
            facts: vec![],
            truncated: false,
        };
        airk_core::discovery::paginate_facts(
            &mut payload,
            facts,
            page,
            &binding,
            limit,
            max_bytes,
        )?;
        if publication.is_none() {
            payload.page.cursor = None;
        }
        self.conditional(
            &catalog,
            read,
            options,
            ("outline/2", limit, max_bytes, page),
            payload,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::changes::{ChangeCause, ChangesRequest, PublicationReason};

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
        let mut snapshot = kernel.store.catalog()?.unwrap().snapshot;
        let artifact = kernel.store.load_retained(&snapshot.artifacts["one.rs"])?;
        let mut profile = airk_ingest::adapters::legacy::legacy_profile(&artifact)?;
        let mut payload = kernel
            .store
            .load_extraction_payload(&snapshot.artifacts["one.rs"])?;
        profile.adapter_version = "original-test-profile/2".into();
        for descriptor in &mut payload.continuity {
            descriptor.profile = profile.identity()?;
        }
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
