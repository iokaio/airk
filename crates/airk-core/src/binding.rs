// SPDX-License-Identifier: Apache-2.0
//! Pure binding of verified path-free extraction to one current source location.
use crate::extraction::{ExtractionPayload, ExtractionProfile, LocalSyntaxId};
use crate::identity::Sha256Digest;
use crate::{Artifact, Chunk, CodeEvidence, Fact};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;

pub struct ArtifactBinding<'a> {
    pub path: &'a str,
    pub source_hash: Sha256Digest,
    pub source_bytes: usize,
}

/// Caller verifies the source/profile/payload association before binding.
/// This compatibility binder supports the existing unembedded artifact shape.
pub fn bind_payload(
    payload: &ExtractionPayload,
    profile: &ExtractionProfile,
    binding: ArtifactBinding<'_>,
) -> Result<Artifact> {
    ensure!(
        !binding.path.is_empty()
            && binding.path.len() <= 4096
            && !binding.path.starts_with('/')
            && !binding.path.contains(['\\', ':', '\0'])
            && binding
                .path
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."),
        "invalid artifact binding path"
    );
    ensure!(
        binding.source_bytes <= 16_777_216,
        "artifact source size exceeds ceiling"
    );
    profile.identity()?;
    payload.validate_with_text_limit(if payload.extraction_method == "utf8" {
        16_777_216
    } else {
        4_194_304
    })?;
    ensure!(
        payload.regions.is_empty()
            && payload.relationships.is_empty()
            && payload.continuity.is_empty(),
        "legacy artifact binding cannot represent regions, explicit relationships or continuity"
    );
    ensure!(
        payload
            .facts
            .iter()
            .all(|fact| fact.owner.is_none() && fact.attributes.is_empty()),
        "legacy artifact binding cannot represent generic fact owners or attributes"
    );
    let identities: BTreeMap<LocalSyntaxId, String> = payload
        .syntax_identities
        .iter()
        .map(|identity| {
            (
                identity.id,
                format!(
                    "syntax:{}:{}:{}:{}",
                    binding.path, identity.span.start, identity.span.end, identity.syntax_kind
                ),
            )
        })
        .collect();
    let facts = payload
        .facts
        .iter()
        .map(|fact| Fact {
            name: fact.name.clone(),
            kind: fact.kind.clone(),
            syntax_kind: fact.syntax_kind.clone(),
            start_line: fact.start_line,
            end_line: fact.end_line,
            location: fact.location.clone(),
            document: fact.document.clone(),
            code: fact.code.as_ref().map(|code| CodeEvidence {
                symbol_id: identities[&code.syntax_id].clone(),
                owner_id: code.owner_id.map(|id| identities[&id].clone()),
                qualified_name: code.qualified_name.clone(),
                scope: code.scope.clone(),
                signature: code.signature.clone(),
                span: code.span.clone(),
                signature_span: code.signature_span.clone(),
                parameters: code.parameters.clone(),
                return_type: code.return_type.clone(),
                modifiers: code.modifiers.clone(),
                documentation: code.documentation.clone(),
                target: code.target.clone(),
                basis: code.basis.clone(),
                producer: code.producer.clone(),
                producer_version: code.producer_version.clone(),
                coverage: code.coverage.clone(),
                details_truncated: code.details_truncated,
            }),
        })
        .collect();
    let chunks = payload
        .chunks
        .iter()
        .map(|chunk| Chunk {
            start_line: chunk.start_line,
            end_line: chunk.end_line,
            text: payload.text[chunk.span.start..chunk.span.end].into(),
            location: chunk.location.clone(),
        })
        .collect();
    Ok(Artifact {
        path: binding.path.into(),
        content_hash: binding.source_hash.to_string(),
        bytes: binding.source_bytes,
        chunks,
        language: profile.language.clone(),
        extraction_method: payload.extraction_method.clone(),
        parse_status: payload.parse_status.clone(),
        facts,
        retained: None,
    })
}
