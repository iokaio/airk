// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    DocumentLocation, EvidenceBasis,
    extraction::{
        EXTRACTION_CONTRACT_VERSION, LocalCodeEvidence, LocalFact, LocalRelationship, LocalSpan,
        LocalSyntaxIdentity, SemanticProjection,
    },
};

fn fixture() -> ExtractionPayload {
    let fact = |id, kind: &str, syntax, owner: Option<u32>| LocalFact {
        id,
        owner: None,
        kind: kind.into(),
        name: kind.into(),
        span: LocalSpan { start: 10, end: 18 },
        syntax_kind: "method_definition".into(),
        start_line: 1,
        end_line: 1,
        location: DocumentLocation::Lines { start: 1, end: 1 },
        document: None,
        code: Some(LocalCodeEvidence {
            fingerprints: None,
            syntax_id: LocalSyntaxId(syntax),
            owner_id: owner.map(LocalSyntaxId),
            qualified_name: "Ledger::run".into(),
            scope: vec!["Ledger".into()],
            signature: None,
            span: None,
            signature_span: None,
            parameters: None,
            return_type: None,
            modifiers: vec![],
            documentation: None,
            target: None,
            basis: EvidenceBasis::Syntax,
            producer: "original-detail-fixture".into(),
            producer_version: "1".into(),
            coverage: "written nodes".into(),
            details_truncated: false,
        }),
        region: None,
        attributes: BTreeMap::new(),
    };
    ExtractionPayload {
        contract_version: EXTRACTION_CONTRACT_VERSION,
        extraction_method: "utf8".into(),
        parse_status: "complete".into(),
        provenance: vec![],
        text: "class A { run() {} }".into(),
        // Fact 90 deliberately shares a number with the *owner syntax node*.
        // The two retained records describe the same method syntax node.
        facts: vec![
            fact(12, "call", 7, None),
            fact(90, "definition", 42, Some(90)),
            fact(91, "test_declaration", 42, Some(90)),
        ],
        syntax_identities: vec![
            LocalSyntaxIdentity {
                id: LocalSyntaxId(42),
                span: LocalSpan { start: 10, end: 18 },
                syntax_kind: "method_definition".into(),
                region: None,
            },
            LocalSyntaxIdentity {
                id: LocalSyntaxId(7),
                span: LocalSpan { start: 10, end: 18 },
                syntax_kind: "call_expression".into(),
                region: None,
            },
            LocalSyntaxIdentity {
                id: LocalSyntaxId(90),
                span: LocalSpan { start: 0, end: 19 },
                syntax_kind: "class_declaration".into(),
                region: None,
            },
        ],
        chunks: vec![],
        semantic_projection: SemanticProjection::default(),
        regions: vec![],
        relationships: vec![
            LocalRelationship {
                source: 91,
                target: LocalRelationshipTarget::Local { id: 90 },
                kind: "test_target".into(),
                basis: EvidenceBasis::StructuralRule,
                producer: "original-detail-fixture".into(),
                producer_version: "1".into(),
                span: LocalSpan { start: 10, end: 18 },
            },
            LocalRelationship {
                source: 12,
                target: LocalRelationshipTarget::Written {
                    spelling: "run".into(),
                },
                kind: "call".into(),
                basis: EvidenceBasis::Syntax,
                producer: "original-detail-fixture".into(),
                producer_version: "1".into(),
                span: LocalSpan { start: 10, end: 18 },
            },
        ],
        continuity: vec![],
        continuity_coverage: BTreeMap::new(),
        diagnostics: vec![],
        partial: false,
    }
}

#[test]
fn selection_preserves_shared_nodes_nonemitted_owners_and_fact_edges() -> Result<()> {
    let mut payload = fixture();
    payload.facts[2].owner = Some(90);
    select(&mut payload, IndexDetail::Balanced)?;
    assert_eq!(payload.facts.len(), 3);
    assert_eq!(payload.facts[0].kind, "call");
    assert_eq!(payload.facts[2].owner, Some(1));
    assert_eq!(payload.relationships[0].source, 2);
    assert!(matches!(
        payload.relationships[0].target,
        LocalRelationshipTarget::Local { id: 1 }
    ));
    assert_eq!(payload.relationships[1].kind, "call");
    assert_eq!(payload.relationships[1].source, 0);
    assert_eq!(payload.syntax_identities.len(), 3);
    assert_eq!(
        payload.syntax_identities[0].syntax_kind,
        "class_declaration"
    );
    for code in payload.facts[1..]
        .iter()
        .filter_map(|fact| fact.code.as_ref())
    {
        assert_eq!(code.syntax_id, LocalSyntaxId(2));
        assert_eq!(code.owner_id, Some(LocalSyntaxId(0)));
    }
    let once = serde_json::to_vec(&payload)?;
    select(&mut payload, IndexDetail::Balanced)?;
    assert_eq!(serde_json::to_vec(&payload)?, once);
    select(&mut payload, IndexDetail::Essential)?;
    assert!(
        payload.facts.is_empty()
            && payload.syntax_identities.is_empty()
            && payload.relationships.is_empty()
    );
    assert_eq!(payload.text, "class A { run() {} }");
    Ok(())
}

#[test]
fn selected_ids_do_not_depend_on_initial_syntax_ordinals() -> Result<()> {
    let mut complete = fixture();
    let mut early = fixture();
    for node in &mut early.syntax_identities {
        node.id.0 += 100;
    }
    for code in early.facts.iter_mut().filter_map(|fact| fact.code.as_mut()) {
        code.syntax_id.0 += 100;
        if let Some(owner_id) = &mut code.owner_id {
            owner_id.0 += 100;
        }
    }
    select(&mut complete, IndexDetail::Balanced)?;
    select(&mut early, IndexDetail::Balanced)?;
    assert_eq!(serde_json::to_vec(&complete)?, serde_json::to_vec(&early)?);
    Ok(())
}

#[test]
fn equal_local_spans_in_different_regions_remain_distinct() -> Result<()> {
    let mut payload = fixture();
    payload.facts.retain(|fact| fact.kind != "call");
    payload.relationships.clear();
    payload
        .syntax_identities
        .retain(|node| node.id == LocalSyntaxId(42));
    let mut second = payload.syntax_identities[0].clone();
    payload.syntax_identities[0].region = Some(2);
    second.id = LocalSyntaxId(9);
    second.region = Some(1);
    payload.syntax_identities.push(second);
    for (fact, (region, id)) in payload.facts.iter_mut().zip([(2, 42), (1, 9)]) {
        fact.region = Some(region);
        let code = fact.code.as_mut().unwrap();
        code.syntax_id = LocalSyntaxId(id);
        code.owner_id = None;
    }
    select(&mut payload, IndexDetail::Detailed)?;
    assert_eq!(payload.syntax_identities[0].region, Some(1));
    assert_eq!(
        payload.facts[0].code.as_ref().unwrap().syntax_id,
        LocalSyntaxId(1)
    );
    assert_eq!(
        payload.facts[1].code.as_ref().unwrap().syntax_id,
        LocalSyntaxId(0)
    );
    Ok(())
}

#[test]
fn missing_owner_identity_fails_instead_of_erasing_the_reference() {
    let mut payload = fixture();
    payload
        .syntax_identities
        .retain(|node| node.id != LocalSyntaxId(90));
    assert!(select(&mut payload, IndexDetail::Balanced).is_err());
}
