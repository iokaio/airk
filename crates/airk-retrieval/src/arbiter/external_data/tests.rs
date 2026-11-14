// SPDX-License-Identifier: Apache-2.0
use super::*;

fn integer(mut value: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    while value >= 128 {
        bytes.push(value as u8 | 128);
        value >>= 7;
    }
    bytes.push(value as u8);
    bytes
}
fn field(number: u64, bytes: &[u8]) -> Vec<u8> {
    [
        integer(number * 8 + 2),
        integer(bytes.len() as u64),
        bytes.to_vec(),
    ]
    .concat()
}
fn tensor(name: &str, offset: &str, length: &str) -> Vec<u8> {
    let mut bytes = vec![112, 1]; // TensorProto data_location = EXTERNAL.
    for (key, value) in [("location", name), ("offset", offset), ("length", length)] {
        bytes.extend(field(
            13,
            &[field(1, key.as_bytes()), field(2, value.as_bytes())].concat(),
        ));
    }
    bytes
}
fn model(tensor: &[u8]) -> Vec<u8> {
    field(7, &field(5, tensor))
}
fn files() -> BTreeMap<String, u64> {
    BTreeMap::from([("weights.data".into(), 64)])
}

#[test]
fn full_verified_external_ranges_are_admitted_without_filesystem_reads() -> Result<()> {
    assert_eq!(
        validate(&model(&tensor("weights.data", "16", "32")), &files())?,
        BTreeSet::from(["weights.data".into()])
    );
    Ok(())
}
#[test]
fn external_paths_and_unregistered_files_refuse_before_native_loading() {
    for name in [
        "../before",
        "/private/file",
        "D:\\private",
        "dir/file",
        "https://source",
        "unknown.data",
    ] {
        assert!(
            validate(&model(&tensor(name, "0", "32")), &files()).is_err(),
            "{name}"
        );
    }
}
#[test]
fn overflow_outside_ranges_and_ambiguous_raw_tensor_data_refuse() {
    for (offset, length) in [
        ("64", "1"),
        ("18446744073709551615", "2"),
        ("-1", "1"),
        ("0", "0"),
    ] {
        assert!(validate(&model(&tensor("weights.data", offset, length)), &files()).is_err());
    }
    let mut data = tensor("weights.data", "0", "32");
    data.extend(field(9, b"raw"));
    assert!(validate(&model(&data), &files()).is_err());
}
#[test]
fn nested_graph_and_sparse_tensor_references_get_the_same_admission() {
    let bad = tensor("../private", "0", "1");
    // Model.graph -> Graph.node -> Node.attribute -> Attribute.graph -> initializer.
    let nested = field(7, &field(1, &field(5, &field(6, &field(5, &bad)))));
    assert!(validate(&nested, &files()).is_err());
    let sparse = field(7, &field(15, &field(1, &bad)));
    assert!(validate(&sparse, &files()).is_err());
}
#[test]
fn duplicate_properties_unused_assets_and_unreviewed_graph_roots_refuse() {
    let mut data = tensor("weights.data", "0", "32");
    data.extend(field(
        13,
        &[field(1, b"location"), field(2, b"weights.data")].concat(),
    ));
    assert!(validate(&model(&data), &files()).is_err());
    assert!(validate(&field(7, b""), &files()).is_err());
    for tag in [20, 25] {
        assert!(validate(&field(tag, b""), &BTreeMap::new()).is_err());
    }
}
#[test]
fn truncated_overflowing_and_group_wire_fields_are_refused() {
    for bytes in [vec![58, 127], vec![255; 11], vec![59], vec![0]] {
        assert!(validate(&bytes, &BTreeMap::new()).is_err());
    }
}

#[test]
fn initializer_above_two_gib_has_a_metadata_only_ortvalue_plan() -> Result<()> {
    let length = 2_542_796_800u64;
    let mut large = tensor("weights.data", "0", &length.to_string());
    large.extend([integer(8), integer(248320), integer(8), integer(2560)].concat());
    large.extend([integer(16), integer(1)].concat());
    large.extend(field(8, b"text.embed_tokens.weight"));
    let graph = model(&large);
    let files = BTreeMap::from([("weights.data".into(), length)]);
    let plan = large_initializers(&graph, &files)?;
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].name, "text.embed_tokens.weight");
    assert_eq!(plan[0].shape, vec![248320, 2560]);
    assert_eq!(plan[0].length, length);
    assert_eq!(plan[0].element, 1);
    let mut wrong = large.clone();
    wrong.extend([integer(8), integer(2)].concat());
    assert!(large_initializers(&model(&wrong), &files).is_err());
    Ok(())
}

#[test]
fn even_small_safe_named_external_data_in_nested_graphs_refuses_before_any_native_path_fallback() {
    let small = tensor("weights.data", "0", "4");
    let nested = field(7, &field(1, &field(5, &field(6, &field(5, &small)))));
    assert!(validate(&nested, &files()).is_err());
    assert!(large_initializers(&nested, &files()).is_err());
    assert!(validate(&model(&small), &files()).is_ok());
}
