// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::current::{CurrentScope, map};
use airk_core::{
    graph_storage::{GraphHead, GraphScope},
    identity::Incarnation,
};

fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::from_bytes([value; 32])
}

fn head() -> CurrentHead {
    let scope = CurrentScope {
        root: "C:/source".into(),
        collection: "sample".into(),
        incarnation: Incarnation::from_bytes([1; 16]),
    };
    let graph_scope = GraphScope {
        collection: scope.collection.clone(),
        root: scope.root.clone(),
        incarnation: scope.incarnation,
        graph_instance: Incarnation::from_bytes([2; 16]),
        specification: digest(3),
        record_bytes: 16,
    };
    let mut graph_head = GraphHead::empty(graph_scope).unwrap();
    graph_head.allocated_count = 3;
    graph_head.live_nodes = 1;
    CurrentHead {
        version: 2,
        artifact_root: map::empty_root(&scope).unwrap(),
        scope,
        sequence: 1,
        lexical_sequence: 0,
        lexical_commit: digest(4),
        graph_head,
    }
}

#[test]
fn vector_binding_requires_exact_scope_id_key_and_bounded_sorted_occurrences() {
    let head = head();
    let binding = VectorBinding {
        version: 1,
        embedding_key: digest(5),
        graph_scope: head.graph_head.scope.digest(),
        graph_id: 1,
        occurrence_ids: vec![digest(7), digest(8)],
    };
    let bytes = binding.canonical_bytes(&head).unwrap();
    assert_eq!(
        VectorBinding::decode(&bytes, &head, digest(5)).unwrap(),
        binding
    );
    assert!(VectorBinding::decode(&bytes, &head, digest(6)).is_err());
    let mut wrong = binding.clone();
    wrong.graph_scope = digest(9);
    assert!(wrong.canonical_bytes(&head).is_err());
    wrong = binding.clone();
    wrong.graph_id = 3;
    assert!(wrong.canonical_bytes(&head).is_err());
    wrong = binding.clone();
    wrong.occurrence_ids.push(digest(8));
    assert!(wrong.canonical_bytes(&head).is_err());
    wrong.occurrence_ids = (0..=MAX_VECTOR_OCCURRENCES)
        .map(|n| {
            airk_core::identity::framed_digest(b"test-occurrence", &[&(n as u32).to_be_bytes()])
        })
        .collect();
    wrong.occurrence_ids.sort();
    assert!(wrong.canonical_bytes(&head).is_err());
}

#[test]
fn occurrence_pointer_rejects_foreign_id_and_unsafe_path() {
    let pointer = OccurrencePointer {
        version: 1,
        embedding_key: digest(5),
        occurrence_id: digest(6),
        path: "docs/example.md".into(),
        path_ordinal: 0,
        source_hash: digest(7),
        payload_digest: digest(8),
        object: digest(9),
    };
    let bytes = pointer.canonical_bytes().unwrap();
    assert_eq!(
        OccurrencePointer::decode(&bytes, digest(6)).unwrap(),
        pointer
    );
    assert!(OccurrencePointer::decode(&bytes, digest(10)).is_err());
    let mut wrong = pointer;
    wrong.path = "../outside".into();
    assert!(wrong.canonical_bytes().is_err());
}

#[test]
fn path_occurrences_address_only_the_exact_scope_and_source() {
    let head = head();
    let index = PathOccurrences {
        version: 1,
        path: "docs/example.md".into(),
        source_hash: digest(7),
        count: 2,
    };
    let bytes = index.canonical_bytes().unwrap();
    assert_eq!(PathOccurrences::decode(&bytes, &index.path).unwrap(), index);
    assert!(PathOccurrences::decode(&bytes, "docs/other.md").is_err());
    assert!(index.occurrence_id(&head.scope, 2).is_err());
    assert_ne!(
        index.occurrence_id(&head.scope, 0).unwrap(),
        index.occurrence_id(&head.scope, 1).unwrap()
    );
    let mut other = head.scope.clone();
    other.incarnation = Incarnation::from_bytes([8; 16]);
    assert_ne!(
        index.occurrence_id(&head.scope, 0).unwrap(),
        index.occurrence_id(&other, 0).unwrap()
    );
    let mut edited = index.clone();
    edited.source_hash = digest(9);
    assert_ne!(
        index.occurrence_id(&head.scope, 0).unwrap(),
        edited.occurrence_id(&head.scope, 0).unwrap()
    );
    edited.path = "../escape".into();
    assert!(edited.canonical_bytes().is_err());
}
