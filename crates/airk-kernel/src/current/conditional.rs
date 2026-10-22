// SPDX-License-Identifier: Apache-2.0
//! Exact current conditional reads. Superseded tokens never fall back to bytes.
use super::runtime::{envelope, revision};
use crate::{FileInfo, Kernel};
use airk_core::{
    FactQuery, FactsResponse, SearchResponse,
    changes::ContinuityBasis,
    evidence::{ConditionalRead, EvidenceReadOptions, EvidenceReadStatus, ReadEvidence},
    identity::framed_digest,
};
use airk_store::current::CurrentRead;
use anyhow::{Context, Result, ensure};
use serde::Serialize;

impl Kernel {
    fn current_resolve(
        &self,
        read: &CurrentRead,
        path: &str,
        options: &EvidenceReadOptions,
    ) -> Result<ReadEvidence> {
        ensure!(
            path.len() <= 8192 && !path.contains('\0'),
            "invalid indexed path"
        );
        let token = revision(read.token()?);
        ensure!(
            options.revision.is_none_or(|expected| expected == token),
            "current revision expired; repeat the request against the current index"
        );
        let mut resolved = read.artifact(path)?.map(|_| path.to_owned());
        let mut basis = ContinuityBasis::SamePathObservation;
        if let Some(id) = options.artifact_id {
            let owned = read.artifact_path(&self.store, id)?;
            if owned.as_deref() != Some(path) {
                resolved = None;
                if options.follow_moves
                    && let Some(candidate) = owned
                {
                    let file = read
                        .file_metadata(&self.store, &candidate)?
                        .context("current artifact metadata missing")?;
                    if options.known_hash == Some(file.state.source_hash) {
                        resolved = Some(candidate);
                        basis = ContinuityBasis::UniqueCompatibleContent;
                    }
                }
            }
        }
        if let (Some(selected), Some(expected)) = (resolved.as_deref(), options.known_hash) {
            let artifact = read
                .artifact(selected)?
                .context("current artifact missing")?;
            ensure!(
                artifact.content_hash == expected.to_string(),
                "known_hash does not match current indexed source; refresh the citation"
            );
        }
        let present = resolved.is_some();
        let publication = envelope(read)?;
        let state = read
            .collection_state()?
            .context("current configuration missing")?;
        let source = airk_store::roots::source_location(
            &self.root_id(),
            &state.configuration.roots,
            resolved.as_deref().unwrap_or(path),
        )?;
        Ok(ReadEvidence {
            publication: Some(publication.clone()),
            current_publication: Some(publication),
            historical: false,
            original_root: Some(source.root),
            requested_path: path.into(),
            resolved_path: resolved,
            status: if present {
                EvidenceReadStatus::Content
            } else {
                EvidenceReadStatus::Unknown
            },
            validator: None,
            basis: if present {
                basis
            } else {
                ContinuityBasis::Unknown
            },
            reason: (!present).then(|| "path is absent from the current index".into()),
        })
    }

    fn current_conditional<T: Serialize>(
        &self,
        pinned: &CurrentRead,
        mut read: ReadEvidence,
        options: &EvidenceReadOptions,
        selection: impl Serialize,
        payload: T,
    ) -> Result<ConditionalRead<T>> {
        let path = read
            .resolved_path
            .as_deref()
            .context("resolved path missing")?;
        let artifact = pinned.artifact(path)?.context("current artifact missing")?;
        let retained = artifact
            .retained
            .as_ref()
            .context("current extraction missing")?;
        let mut representation = serde_json::to_value(&payload)?;
        if let Some(fields) = representation.as_object_mut() {
            fields.remove("publication");
            fields.remove("generation");
        }
        let validator = framed_digest(
            b"current-read-representation/1",
            &[
                pinned.head().scope.incarnation.as_bytes(),
                path.as_bytes(),
                artifact.content_hash.as_bytes(),
                retained.object.as_bytes(),
                read.original_root.as_deref().unwrap_or_default().as_bytes(),
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
        let pinned = self.current_reader()?;
        let read = self.current_resolve(&pinned, path, options)?;
        if read.resolved_path.is_none() {
            return Ok(ConditionalRead {
                read,
                content: None,
            });
        }
        let path = read
            .resolved_path
            .clone()
            .context("resolved path missing")?;
        let payload = self.current_file_info(&pinned, &path)?;
        self.current_conditional(&pinned, read, options, "file_info/1", payload)
    }

    pub fn read_file_conditional(
        &self,
        path: &str,
        start: usize,
        end: usize,
        max_bytes: usize,
        options: &EvidenceReadOptions,
    ) -> Result<ConditionalRead<SearchResponse>> {
        airk_core::validate_budget(1, max_bytes)?;
        ensure!(start > 0 && end >= start, "invalid line range");
        let pinned = self.current_reader()?;
        let read = self.current_resolve(&pinned, path, options)?;
        if read.resolved_path.is_none() {
            return Ok(ConditionalRead {
                read,
                content: None,
            });
        }
        let path = read
            .resolved_path
            .clone()
            .context("resolved path missing")?;
        let artifact = self.current_artifact(&pinned, &path, true)?;
        let mut metadata = self.empty_snapshot();
        metadata.generation = pinned.head().sequence;
        let payload = Self::read_snapshot(
            Some(envelope(&pinned)?),
            &metadata,
            &artifact,
            &path,
            start,
            end,
            max_bytes,
        )?;
        self.current_conditional(
            &pinned,
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
        airk_core::validate_budget(1, max_bytes)?;
        ensure!(
            before <= 100 && after <= 100 && line > 0,
            "invalid context bounds"
        );
        let pinned = self.current_reader()?;
        let read = self.current_resolve(&pinned, path, options)?;
        if read.resolved_path.is_none() {
            return Ok(ConditionalRead {
                read,
                content: None,
            });
        }
        let path = read
            .resolved_path
            .clone()
            .context("resolved path missing")?;
        let artifact = self.current_artifact(&pinned, &path, true)?;
        let lines = artifact
            .chunks
            .iter()
            .map(|c| c.end_line)
            .max()
            .unwrap_or(0);
        ensure!(line <= lines, "line is outside indexed source");
        let mut metadata = self.empty_snapshot();
        metadata.generation = pinned.head().sequence;
        let payload = Self::read_snapshot(
            Some(envelope(&pinned)?),
            &metadata,
            &artifact,
            &path,
            line.saturating_sub(before).max(1),
            line.saturating_add(after).min(lines),
            max_bytes,
        )?;
        self.current_conditional(
            &pinned,
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
        airk_core::validate_budget(limit, max_bytes)?;
        let pinned = self.current_reader()?;
        let read = self.current_resolve(&pinned, path, options)?;
        if read.resolved_path.is_none() {
            return Ok(ConditionalRead {
                read,
                content: None,
            });
        }
        let path = read
            .resolved_path
            .clone()
            .context("resolved path missing")?;
        let payload = self.current_facts(
            &pinned,
            &FactQuery {
                path_prefix: path,
                name: None,
                kind: None,
                language: None,
                exact_name: false,
                limit,
                max_bytes,
                path_exact: true,
                document_only: false,
            },
            false,
            page,
        )?;
        self.current_conditional(
            &pinned,
            read,
            options,
            ("outline/1", limit, max_bytes, page),
            payload,
        )
    }
}
