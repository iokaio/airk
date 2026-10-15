// SPDX-License-Identifier: Apache-2.0
//! Original matching qualification. Local pairs are not durable handle claims.
use airk_core::{
    DocumentLocation,
    changes::ChangeDimension as D,
    continuity::{
        ContinuityBudget, ContinuityDescriptor, ContinuityFamily as Family,
        ContinuitySubject as Subject,
    },
    extraction::{ExtractionPayload, ExtractionProfile, LocalSpan, LocalSyntaxId},
    identity::{Sha256Digest, framed_digest},
};
use airk_ingest::continuity::{documents, matching::reconcile};
use anyhow::Result;
use std::collections::BTreeMap;

fn digest(text: &str) -> Sha256Digest {
    framed_digest(b"original-matching-fixture/1", &[text.as_bytes()])
}
fn symbol(id: u32) -> Subject {
    Subject::Syntax(LocalSyntaxId(id))
}
fn descriptor(
    subject: Subject,
    family: Family,
    category: &str,
    name: Option<&str>,
    body: &str,
) -> ContinuityDescriptor {
    ContinuityDescriptor {
        algorithm_version: 1,
        subject,
        owner: None,
        region: None,
        family,
        category: category.into(),
        name: name.map(digest),
        explicit_identity: None,
        profile: digest("profile-v1"),
        provenance: digest("native-original-v1"),
        dimensions: BTreeMap::from([(
            if family == Family::Code {
                D::CodeBody
            } else {
                D::Statement
            },
            digest(body),
        )]),
        unsupported_dimensions: vec![],
        span: LocalSpan {
            start: 0,
            end: body.len(),
        },
        location: DocumentLocation::Lines { start: 1, end: 1 },
        complete: true,
        body_informative: family == Family::Code && body != "{}",
    }
}
fn code(id: u32, name: &str, signature: &str, body: &str) -> ContinuityDescriptor {
    let mut item = descriptor(symbol(id), Family::Code, "function", Some(name), body);
    item.dimensions.insert(D::CodeSignature, digest(signature));
    item
}
fn document(id: u32, body: &str) -> ContinuityDescriptor {
    descriptor(
        Subject::Unit(id),
        Family::Documents,
        "paragraph",
        None,
        body,
    )
}

#[test]
fn unique_qualified_name_preserves_body_edit_and_marks_no_unrelated_pair() {
    let old = code(
        0,
        "Module::calculate",
        "calculate(i32)",
        "return input + 1;",
    );
    let new = code(
        8,
        "Module::calculate",
        "calculate(i32)",
        "return input + 2;",
    );
    let unrelated = code(9, "Module::other", "other()", "return 77;");
    let result = reconcile(&[old], &[new, unrelated]);
    assert_eq!(result.pairs, BTreeMap::from([(symbol(8), symbol(0))]));
}

#[test]
fn overload_insertion_and_reordering_use_unique_signatures() {
    let old = [
        code(0, "parse", "parse(int)", "return value + 1;"),
        code(1, "parse", "parse(string)", "return value.size();"),
    ];
    let new = [
        code(5, "parse", "parse(bool)", "return value ? 2 : 3;"),
        code(6, "parse", "parse(string)", "return value.size() + 1;"),
        code(7, "parse", "parse(int)", "return value + 2;"),
    ];
    let result = reconcile(&old, &new);
    assert_eq!(
        result.pairs,
        BTreeMap::from([(symbol(6), symbol(1)), (symbol(7), symbol(0))])
    );
    assert!(!result.pairs.contains_key(&symbol(5)));
}

#[test]
fn overload_body_disambiguation_preserves_distinct_informative_implementations() {
    let old = [
        code(0, "transform", "transform(T)", "return input.left + 11;"),
        code(1, "transform", "transform(T)", "return input.right + 19;"),
    ];
    let new = [
        code(8, "transform", "transform(U)", "return input.right + 19;"),
        code(9, "transform", "transform(U)", "return input.left + 11;"),
    ];
    assert_eq!(
        reconcile(&old, &new).pairs,
        BTreeMap::from([(symbol(8), symbol(1)), (symbol(9), symbol(0))])
    );
}

#[test]
fn informative_rename_matches_but_empty_body_is_not_identity() {
    let old = [
        code(
            0,
            "before",
            "before()",
            "return original.distinctive_value + 17;",
        ),
        code(1, "empty_before", "empty_before()", "{}"),
    ];
    let new = [
        code(
            8,
            "after",
            "after()",
            "return original.distinctive_value + 17;",
        ),
        code(9, "empty_after", "empty_after()", "{}"),
    ];
    assert_eq!(
        reconcile(&old, &new).pairs,
        BTreeMap::from([(symbol(8), symbol(0))])
    );
}

#[test]
fn matched_owners_prevent_cross_scope_pairing() {
    let owner_a = descriptor(symbol(0), Family::Code, "class", Some("A"), "class A");
    let owner_b = descriptor(symbol(1), Family::Code, "class", Some("B"), "class B");
    let mut child = code(2, "run", "run()", "return 19;");
    child.owner = Some(owner_a.subject);
    let mut a = owner_a.clone();
    a.subject = symbol(8);
    let mut b = owner_b.clone();
    b.subject = symbol(9);
    let mut moved = child.clone();
    moved.subject = symbol(10);
    moved.owner = Some(b.subject);
    let result = reconcile(&[owner_a, owner_b, child], &[moved, b, a]);
    assert_eq!(
        result.pairs,
        BTreeMap::from([(symbol(8), symbol(0)), (symbol(9), symbol(1))])
    );
}

#[test]
fn profile_provenance_and_incomplete_evidence_do_not_establish_continuity() {
    let old = code(0, "same", "same()", "return exact + 7;");
    for changed in [0, 1, 2] {
        let mut new = old.clone();
        new.subject = symbol(9);
        match changed {
            0 => new.profile = digest("other-profile"),
            1 => new.provenance = digest("different-producer"),
            _ => new.complete = false,
        }
        assert!(
            reconcile(std::slice::from_ref(&old), &[new])
                .pairs
                .is_empty()
        );
    }
}

#[test]
fn duplicate_requirement_ids_cannot_be_promoted_by_distinct_statements() {
    let mut a = document(0, "REQ-42 must preserve original A.");
    a.category = "requirement".into();
    a.explicit_identity = Some(digest("REQ-42"));
    let mut b = a.clone();
    b.subject = Subject::Unit(1);
    b.dimensions
        .insert(D::Statement, digest("REQ-42 must preserve original B."));
    let mut new_a = a.clone();
    new_a.subject = Subject::Unit(8);
    let mut new_b = b.clone();
    new_b.subject = Subject::Unit(9);
    let result = reconcile(&[a.clone(), b], &[new_b, new_a.clone()]);
    assert!(result.pairs.is_empty());
    assert_eq!(result.ambiguous.len(), 2);
    let duplicate_on_new_only = reconcile(
        &[a],
        &[
            new_a.clone(),
            ContinuityDescriptor {
                subject: Subject::Unit(10),
                ..new_a
            },
        ],
    );
    assert!(duplicate_on_new_only.pairs.is_empty());
}

#[test]
fn explicit_ids_are_unique_within_document_category() {
    let mut requirement = document(0, "REQ-42 must retain evidence.");
    requirement.category = "requirement".into();
    requirement.explicit_identity = Some(digest("REQ-42"));
    let mut acceptance = document(1, "REQ-42 acceptance evidence is written here.");
    acceptance.category = "acceptance".into();
    acceptance.explicit_identity = requirement.explicit_identity;
    let mut new_requirement = requirement.clone();
    new_requirement.subject = Subject::Unit(8);
    new_requirement
        .dimensions
        .insert(D::Statement, digest("edited requirement"));
    let mut new_acceptance = acceptance.clone();
    new_acceptance.subject = Subject::Unit(9);
    new_acceptance
        .dimensions
        .insert(D::Statement, digest("edited acceptance"));
    assert_eq!(
        reconcile(
            &[requirement, acceptance],
            &[new_acceptance, new_requirement]
        )
        .pairs,
        BTreeMap::from([
            (Subject::Unit(8), Subject::Unit(0)),
            (Subject::Unit(9), Subject::Unit(1))
        ])
    );
}

#[test]
fn repeated_anonymous_prose_is_ambiguous_and_edited_prose_is_unmatched() {
    let old = [
        document(0, "Repeated original paragraph."),
        document(1, "Repeated original paragraph."),
    ];
    let new = [
        document(8, "Repeated original paragraph."),
        document(9, "Repeated original paragraph."),
        document(10, "A newly edited paragraph."),
    ];
    let result = reconcile(&old, &new);
    assert!(result.pairs.is_empty());
    assert!(
        result.ambiguous.contains(&Subject::Unit(8))
            && result.ambiguous.contains(&Subject::Unit(9))
    );
    assert!(!result.ambiguous.contains(&Subject::Unit(10)));
}

fn prose(text: &str) -> Result<Vec<ContinuityDescriptor>> {
    let profile = ExtractionProfile {
        contract_version: 1,
        adapter_id: "markdown".into(),
        adapter_version: "original/1".into(),
        language: "markdown".into(),
        dialect: None,
        grammar_identity: None,
        mapping_version: 1,
        chunk_version: 1,
        options: BTreeMap::new(),
        configuration_inputs: BTreeMap::new(),
    };
    let payload = ExtractionPayload {
        contract_version: 1,
        extraction_method: "original/1".into(),
        parse_status: "text".into(),
        provenance: vec![],
        text: text.into(),
        facts: vec![],
        syntax_identities: vec![],
        chunks: vec![],
        regions: vec![],
        relationships: vec![],
        continuity: vec![],
        continuity_coverage: BTreeMap::new(),
        diagnostics: vec![],
        partial: false,
    };
    Ok(documents::describe(&payload, &profile, &mut ContinuityBudget::default())?.descriptors)
}

#[test]
fn actual_prose_descriptors_match_paragraph_shift_through_unchanged_section() -> Result<()> {
    let old = prose("# Original section\n\nPreserve this café paragraph.\n")?;
    let new = prose(
        "# Original section\n\nInserted original paragraph.\n\nPreserve this café paragraph.\n",
    )?;
    let result = reconcile(&old, &new);
    assert_eq!(result.pairs.len(), 2);
    let paragraph = old
        .iter()
        .find(|item| item.category == "paragraph")
        .unwrap();
    let matched = new
        .iter()
        .find(|item| result.pairs.get(&item.subject) == Some(&paragraph.subject))
        .unwrap();
    assert_ne!(paragraph.location, matched.location);
    assert_eq!(
        paragraph.dimensions[&D::Statement],
        matched.dimensions[&D::Statement]
    );
    Ok(())
}

#[test]
fn region_evidence_is_required_and_local_ordinal_never_promotes_a_match() {
    let region = descriptor(
        Subject::Region(1),
        Family::Documents,
        "embedded_region",
        None,
        "original embedded region",
    );
    let mut child = code(0, "execute", "execute()", "return 17;");
    child.region = Some(1);
    let mut moved_region = region.clone();
    moved_region.subject = Subject::Region(7);
    let mut moved_child = child.clone();
    moved_child.subject = symbol(8);
    moved_child.region = Some(7);
    let result = reconcile(
        &[region.clone(), child.clone()],
        &[moved_child, moved_region],
    );
    assert_eq!(
        result.pairs,
        BTreeMap::from([
            (Subject::Region(7), Subject::Region(1)),
            (symbol(8), symbol(0))
        ])
    );
    let mut replacement = region.clone();
    replacement
        .dimensions
        .insert(D::Statement, digest("entirely different region"));
    let mut unchanged_ordinal_child = child.clone();
    unchanged_ordinal_child.subject = symbol(9);
    assert!(
        reconcile(
            &[region, child.clone()],
            &[replacement, unchanged_ordinal_child]
        )
        .pairs
        .is_empty()
    );
    let mut orphan = child.clone();
    orphan.subject = symbol(10);
    assert!(reconcile(&[child], &[orphan]).pairs.is_empty());
}

#[test]
fn unmatched_new_units_never_pair_by_position_or_local_id() {
    let old = document(0, "Completely different prior paragraph.");
    let new = document(0, "Unrelated new paragraph at identical local position.");
    assert!(reconcile(&[old], &[new]).pairs.is_empty());
}
