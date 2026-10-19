// SPDX-License-Identifier: Apache-2.0
use airk_core::{
    DocumentLocation,
    binding::ArtifactBinding,
    extraction::{ExtractionPayload, ExtractionProfile, LocalFact, LocalRegion, LocalSpan},
    identity::Sha256Digest,
    location::{ByteSpan, HostBudget, MapSegment, Mapping, RegionLocation, SourceMap},
    mapped_binding::{MappedBinding, bind_payload, syntax_id},
};
use anyhow::Result;
use std::collections::BTreeMap;

fn profile() -> ExtractionProfile {
    ExtractionProfile {
        contract_version: 2,
        adapter_id: "fixture".into(),
        adapter_version: "1".into(),
        language: "fixture".into(),
        dialect: None,
        grammar_identity: None,
        mapping_version: 1,
        chunk_version: 1,
        options: BTreeMap::new(),
        configuration_inputs: BTreeMap::new(),
    }
}
fn payload(text: &str) -> ExtractionPayload {
    ExtractionPayload {
        continuity_coverage: BTreeMap::new(),
        semantic_projection: Default::default(),
        contract_version: 2,
        extraction_method: "utf8".into(),
        parse_status: "fixture".into(),
        provenance: vec![],
        text: text.into(),
        facts: vec![],
        syntax_identities: vec![],
        chunks: vec![],
        regions: vec![],
        relationships: vec![],
        continuity: vec![],
        diagnostics: vec![],
        partial: false,
    }
}
fn verbatim(id: u32, parent: Option<u32>, local: &str, start: usize) -> LocalRegion {
    let local_span = ByteSpan {
        start: 0,
        end: local.len(),
    };
    let host = ByteSpan {
        start,
        end: start + local.len(),
    };
    LocalRegion {
        id,
        parent,
        profile: profile(),
        text: local.into(),
        source_map: SourceMap {
            version: 1,
            segments: vec![MapSegment {
                local: local_span,
                mapping: Mapping::Verbatim { host },
            }],
        },
        location: RegionLocation::Embedded {
            region_id: format!("region-{id}"),
            local_span,
            original_spans: vec![host],
        },
    }
}
fn budget() -> HostBudget {
    HostBudget::new(500_000, 10_000, 16_777_216).unwrap()
}

#[test]
fn decoded_json_surrogates_map_to_original_host_and_notebook_lines() -> Result<()> {
    let mut value = payload("\"A\\uD83D\\uDE00\\n\"");
    let spans = vec![
        ByteSpan { start: 1, end: 2 },
        ByteSpan { start: 2, end: 14 },
        ByteSpan { start: 14, end: 16 },
    ];
    value.regions.push(LocalRegion {
        id: 1,
        parent: None,
        profile: profile(),
        text: "A😀\n".into(),
        source_map: SourceMap {
            version: 1,
            segments: vec![
                MapSegment {
                    local: ByteSpan { start: 0, end: 1 },
                    mapping: Mapping::Verbatim { host: spans[0] },
                },
                MapSegment {
                    local: ByteSpan { start: 1, end: 5 },
                    mapping: Mapping::Decoded { host: spans[1] },
                },
                MapSegment {
                    local: ByteSpan { start: 5, end: 6 },
                    mapping: Mapping::Decoded { host: spans[2] },
                },
            ],
        },
        location: RegionLocation::NotebookCell {
            cell_id: Some("original-cell".into()),
            position: 1,
            local_start_line: 1,
            local_end_line: 1,
            original_spans: spans.clone(),
        },
    });
    let mut allowance = budget();
    let mapper = MappedBinding::new(&value, &profile(), &mut allowance)?;
    let (location, first, last) =
        mapper.mapped_location(1, LocalSpan { start: 1, end: 5 }, &mut allowance)?;
    assert_eq!((first, last), (1, 1));
    let serialized = serde_json::to_value(&location)?;
    assert_eq!(serialized["Mapped"]["region"]["kind"], "notebook_cell");
    assert_eq!(serialized["Mapped"]["region"]["cell_id"], "original-cell");
    assert_eq!(
        serde_json::from_value::<DocumentLocation>(serialized)?,
        location
    );
    let DocumentLocation::Mapped {
        region:
            RegionLocation::NotebookCell {
                original_spans,
                local_start_line,
                local_end_line,
                ..
            },
    } = location
    else {
        panic!("notebook location")
    };
    assert_eq!(original_spans, spans);
    assert_eq!((local_start_line, local_end_line), (1, 1));
    assert!(
        mapper
            .mapped_location(1, LocalSpan { start: 2, end: 5 }, &mut allowance)
            .is_err()
    );
    Ok(())
}

#[test]
fn nested_maps_and_region_syntax_namespaces_are_distinct() -> Result<()> {
    let mut value = payload("([x])");
    value.regions.push(verbatim(1, None, "[x]", 1));
    value.regions.push(verbatim(2, Some(1), "x", 1));
    let mut allowance = budget();
    let mapper = MappedBinding::new(&value, &profile(), &mut allowance)?;
    let (location, _, _) =
        mapper.mapped_location(2, LocalSpan { start: 0, end: 1 }, &mut allowance)?;
    let DocumentLocation::Mapped {
        region: RegionLocation::Embedded { original_spans, .. },
    } = location
    else {
        panic!("embedded location")
    };
    assert_eq!(original_spans, vec![ByteSpan { start: 2, end: 3 }]);
    let span = LocalSpan { start: 0, end: 1 };
    assert_ne!(
        syntax_id("a.md", Some(1), &span, "name"),
        syntax_id("a.md", Some(2), &span, "name")
    );
    assert_eq!(
        syntax_id("a.md", None, &span, "name"),
        "syntax:a.md:0:1:name"
    );
    Ok(())
}

#[test]
fn hundreds_of_facts_share_one_region_admission_and_invalid_maps_fail() -> Result<()> {
    let mut value = payload("<x>");
    value.regions.push(verbatim(1, None, "x", 1));
    value.chunks.push(airk_core::extraction::LocalChunk {
        span: LocalSpan { start: 0, end: 1 },
        start_line: 1,
        end_line: 1,
        location: DocumentLocation::Lines { start: 1, end: 1 },
        region: Some(1),
    });
    for id in 0..300 {
        value.facts.push(LocalFact {
            id,
            owner: None,
            kind: "key".into(),
            name: format!("fact-{id}"),
            span: LocalSpan { start: 0, end: 1 },
            syntax_kind: "key".into(),
            start_line: 1,
            end_line: 1,
            location: DocumentLocation::Lines { start: 1, end: 1 },
            document: None,
            code: None,
            region: Some(1),
            attributes: BTreeMap::new(),
        });
    }
    let binding = || ArtifactBinding {
        path: "original.md",
        source_hash: Sha256Digest::from_bytes([3; 32]),
        source_bytes: 3,
    };
    let result = bind_payload(&value, &profile(), binding(), &mut budget())?;
    assert_eq!(result.facts.len(), 300);
    assert_eq!(
        result.chunks[0].text, "x",
        "chunk reads decoded region, not host prefix"
    );
    value.regions[0].source_map.segments[0].mapping = Mapping::Unmapped;
    assert!(bind_payload(&value, &profile(), binding(), &mut budget()).is_err());
    Ok(())
}

#[test]
fn unembedded_binding_serialization_stays_identical() -> Result<()> {
    let value = payload("original\n");
    let profile = profile();
    let binding = || ArtifactBinding {
        path: "original.txt",
        source_hash: Sha256Digest::from_bytes([4; 32]),
        source_bytes: value.text.len(),
    };
    let old = airk_core::binding::bind_payload(&value, &profile, binding())?;
    let new = bind_payload(&value, &profile, binding(), &mut budget())?;
    assert_eq!(serde_json::to_value(old)?, serde_json::to_value(new)?);
    Ok(())
}

#[test]
fn mapped_code_span_is_explicitly_local_while_citation_verifies_original_host() -> Result<()> {
    use airk_core::extraction::{LocalCodeEvidence, LocalSyntaxId, LocalSyntaxIdentity};
    let host = "<!-- original 🛰 -->\r\n<script>let beacon = 7;</script>\r\n";
    let body = "let beacon = 7;";
    let start = host.find(body).unwrap();
    let mut value = payload(host);
    value.regions.push(verbatim(7, None, body, start));
    let local_span = LocalSpan {
        start: 0,
        end: body.len(),
    };
    value.syntax_identities.push(LocalSyntaxIdentity {
        id: LocalSyntaxId(1),
        span: local_span.clone(),
        syntax_kind: "variable_declaration".into(),
        region: Some(7),
    });
    value.facts.push(LocalFact {
        id: 1,
        owner: None,
        kind: "variable".into(),
        name: "beacon".into(),
        span: local_span,
        syntax_kind: "variable_declaration".into(),
        start_line: 1,
        end_line: 1,
        location: DocumentLocation::Lines { start: 1, end: 1 },
        document: None,
        code: Some(LocalCodeEvidence {
            fingerprints: None,
            syntax_id: LocalSyntaxId(1),
            owner_id: None,
            qualified_name: "beacon".into(),
            scope: vec![],
            signature: Some(body.into()),
            span: Some(airk_core::CodeSpan {
                start_byte: 0,
                end_byte: body.len(),
            }),
            signature_span: Some(airk_core::CodeSpan {
                start_byte: 0,
                end_byte: body.len(),
            }),
            parameters: None,
            return_type: None,
            modifiers: vec![],
            documentation: None,
            target: None,
            basis: airk_core::EvidenceBasis::Syntax,
            producer: "original-fixture".into(),
            producer_version: "1".into(),
            coverage: "fixture".into(),
            details_truncated: false,
        }),
        region: Some(7),
        attributes: BTreeMap::new(),
    });
    let bound = bind_payload(
        &value,
        &profile(),
        ArtifactBinding {
            path: "original.html",
            source_hash: Sha256Digest::from_bytes([8; 32]),
            source_bytes: host.len(),
        },
        &mut budget(),
    )?;
    let fact = &bound.facts[0];
    assert_eq!((fact.start_line, fact.end_line), (2, 2));
    let code = fact.code.as_ref().unwrap();
    let span = code.span.as_ref().unwrap();
    assert_eq!(&body[span.start_byte..span.end_byte], body);
    assert_ne!(span.start_byte, start);
    let DocumentLocation::Mapped {
        region:
            RegionLocation::Embedded {
                local_span,
                original_spans,
                ..
            },
    } = &fact.location
    else {
        panic!("mapped locator required")
    };
    assert_eq!(local_span.start, span.start_byte);
    assert_eq!(
        original_spans,
        &vec![ByteSpan {
            start,
            end: start + body.len()
        }]
    );
    assert_eq!(&host[original_spans[0].start..original_spans[0].end], body);
    let serialized = serde_json::to_value(fact)?;
    assert_eq!(
        serialized["location"]["Mapped"]["region"]["kind"],
        "embedded"
    );
    assert_eq!(serialized["code"]["span"]["start_byte"], 0);
    assert_eq!(
        serialized["location"]["Mapped"]["region"]["original_spans"][0]["start"],
        start
    );
    Ok(())
}
