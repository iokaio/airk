// SPDX-License-Identifier: Apache-2.0
//! Shared host orchestration: original bytes, bounded children and exact maps.
use crate::adapters::{legacy::convert_native_legacy, registry::AdapterRegistry};
use airk_core::{
    Artifact, Fact,
    extraction::{ExtractionPayload, ExtractionProfile, LocalRegion, LocalSyntaxId},
    location::{ByteSpan, HostBudget, MapSegment, Mapping, RegionLocation, SourceMap},
};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};

pub(crate) fn is_host(id: &str) -> bool {
    matches!(
        id,
        "html"
            | "vue"
            | "svelte"
            | "astro"
            | "mdx"
            | "notebook"
            | "razor"
            | "rmd"
            | "haskell-literate"
    )
}

fn artifact(
    path: &str,
    text: &str,
    profile: &ExtractionProfile,
    facts: Vec<Fact>,
    partial: bool,
) -> Artifact {
    Artifact {
        path: path.into(),
        content_hash: format!("{:x}", Sha256::digest(text.as_bytes())),
        bytes: text.len(),
        chunks: crate::chunk(text),
        language: profile.language.clone(),
        extraction_method: "utf8".into(),
        parse_status: if partial { "partial-syntax" } else { "parsed" }.into(),
        facts,
        retained: None,
    }
}

pub(crate) fn extract(
    path: &str,
    bytes: &[u8],
    profile: &ExtractionProfile,
) -> Result<ExtractionPayload> {
    let text = std::str::from_utf8(bytes)?;
    ensure!(
        bytes.len() <= crate::MAX_FILE_BYTES && !bytes.contains(&0),
        "invalid composite source"
    );
    let registry = AdapterRegistry::baseline();
    registry.validate_effective_profile(path, profile)?;
    let mut budget = HostBudget::new(500_000, 10_000, 16_777_216)?;
    let mut exclusions = Vec::new();
    let (mut regions, mut facts, diagnostics, partial) = match profile.adapter_id.as_str() {
        "haskell-literate" => {
            let found = crate::languages_b_functional::literate::discover(path, text, &mut budget)?;
            exclusions = found.exclusions;
            let regions = found
                .regions
                .into_iter()
                .enumerate()
                .map(|(index, child)| {
                    let local_span = ByteSpan {
                        start: 0,
                        end: child.text.len(),
                    };
                    let original_spans = child
                        .source_map
                        .original_spans(local_span, &child.text, text)?
                        .context("literate region must map to original source")?;
                    Ok(LocalRegion {
                        id: index as u32,
                        parent: None,
                        profile: registry.get("haskell").unwrap().profile.clone(),
                        text: child.text,
                        source_map: child.source_map,
                        location: RegionLocation::Embedded {
                            region_id: format!("{}:{index}", child.kind),
                            local_span,
                            original_spans,
                        },
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            (regions, vec![], found.diagnostics, found.partial)
        }
        "notebook" => {
            let found = crate::notebook::discover(path, text, &mut budget)?;
            (found.regions, found.facts, found.diagnostics, found.partial)
        }
        "razor" => {
            let found = crate::razor::discover(path, text, &mut budget)?;
            (found.regions, found.facts, found.diagnostics, found.partial)
        }
        id => {
            let found = match id {
                "rmd" => {
                    let found =
                        crate::languages_b_r_perl_groovy::rmd::discover(path, text, &mut budget)?;
                    exclusions = found.exclusions;
                    found.host
                }
                "html" => crate::host_html::discover(path, text, &mut budget)?,
                "vue" | "svelte" => crate::host_components::discover(path, text, id, &mut budget)?,
                "astro" | "mdx" => {
                    let found = crate::host_prose::discover(path, text, id, &mut budget)?;
                    exclusions = found.exclusions;
                    found.host
                }
                _ => anyhow::bail!("unsupported composite host"),
            };
            let mut regions = Vec::new();
            for (index, child) in found.regions.into_iter().enumerate() {
                let local = &text[child.span.start..child.span.end];
                if local.is_empty() {
                    continue;
                }
                let local_span = ByteSpan {
                    start: 0,
                    end: local.len(),
                };
                regions.push(LocalRegion {
                    id: index as u32,
                    parent: None,
                    profile: registry
                        .get(child.adapter_id)
                        .context("unregistered child")?
                        .profile
                        .clone(),
                    text: local.into(),
                    source_map: SourceMap {
                        version: 1,
                        segments: vec![MapSegment {
                            local: local_span,
                            mapping: Mapping::Verbatim { host: child.span },
                        }],
                    },
                    location: RegionLocation::Embedded {
                        region_id: format!("{}:{index}", child.kind),
                        local_span,
                        original_spans: vec![child.span],
                    },
                });
            }
            (regions, found.facts, found.diagnostics, found.partial)
        }
    };
    let mut prose_text = None;
    if matches!(
        profile.adapter_id.as_str(),
        "mdx" | "rmd" | "haskell-literate"
    ) {
        // Preserve offsets while suppressing syntax islands from the prose pass.
        let mut masked = bytes.to_vec();
        for span in &exclusions {
            span.validate(text)?;
            for byte in &mut masked[span.start..span.end] {
                if !matches!(*byte, b'\r' | b'\n') {
                    *byte = b' ';
                }
            }
        }
        budget.charge(masked.len(), 0, 0, 0)?;
        let prose = crate::documents::extract_selected(
            path,
            &masked,
            &registry.get("markdown").unwrap().profile,
        )?;
        budget.charge(0, prose.facts.len(), 0, 0)?;
        facts.extend(prose.facts);
        prose_text = Some(String::from_utf8(masked)?);
    }
    let raw = artifact(path, text, profile, facts, partial);
    let mut payload = convert_native_legacy(&raw, bytes, profile)?;
    payload.contract_version = 2;
    payload.partial = partial;
    payload.diagnostics = diagnostics;
    payload.regions.clear();
    for region in regions.drain(..) {
        let child_id = region.profile.adapter_id.as_str();
        // Specialized adapters without shared parser walks cannot reset the host allowance.
        if !matches!(
            child_id,
            "markdown"
                | "r"
                | "haskell"
                | "rust"
                | "python"
                | "javascript"
                | "typescript"
                | "tsx"
                | "go"
                | "java"
                | "c"
                | "cpp"
                | "csharp"
                | "bash"
                | "powershell"
                | "lua"
                | "swift"
                | "kotlin"
                | "json"
                | "yaml"
                | "toml"
                | "css"
                | "scss"
        ) {
            payload.partial = true;
            if payload.diagnostics.len() < 256 {
                payload.diagnostics.push(format!(
                    "Child {child_id} has no qualified shared-budget adapter"
                ));
            }
            continue;
        }
        let child = if child_id == "markdown" {
            budget.charge(region.text.len(), 0, 0, 0)?;
            let extracted =
                crate::documents::extract_selected(path, region.text.as_bytes(), &region.profile)?;
            budget.charge(0, extracted.facts.len(), 0, 0)?;
            artifact(path, &region.text, &region.profile, extracted.facts, false)
        } else {
            let analysis = crate::syntax::analyze_selected_budgeted(
                path,
                &region.text,
                child_id,
                &mut budget,
            )?;
            artifact(
                path,
                &region.text,
                &region.profile,
                analysis.facts,
                analysis.status == "partial-syntax",
            )
        };
        let mut child_payload =
            convert_native_legacy(&child, region.text.as_bytes(), &region.profile)?;
        child_payload.chunks = crate::chunking::partition(&child_payload, &[])?;
        payload.partial |= child_payload.partial || child.parse_status == "partial-syntax";
        let fact_offset = u32::try_from(payload.facts.len())?;
        let syntax_offset = u32::try_from(payload.syntax_identities.len())?;
        for mut identity in child_payload.syntax_identities {
            identity.id = LocalSyntaxId(
                identity
                    .id
                    .0
                    .checked_add(syntax_offset)
                    .context("syntax ID overflow")?,
            );
            identity.region = Some(region.id);
            payload.syntax_identities.push(identity);
        }
        for mut fact in child_payload.facts {
            fact.id = fact
                .id
                .checked_add(fact_offset)
                .context("fact ID overflow")?;
            fact.region = Some(region.id);
            if let Some(code) = &mut fact.code {
                code.syntax_id.0 += syntax_offset;
                if let Some(owner) = &mut code.owner_id {
                    owner.0 += syntax_offset;
                }
            }
            payload.facts.push(fact);
        }
        for mut chunk in child_payload.chunks {
            chunk.region = Some(region.id);
            payload.chunks.push(chunk);
        }
        payload.regions.push(region);
    }
    crate::continuity::prepare_with_prose_text(&mut payload, profile, prose_text.as_deref())?;
    if profile.adapter_id == "notebook" {
        // The original JSON is a mapping domain, never a retrieval chunk.
        payload.chunks.retain(|chunk| chunk.region.is_some());
    }
    if payload.partial {
        payload.parse_status = "partial-syntax".into();
    }
    payload.validate()?;
    Ok(payload)
}
