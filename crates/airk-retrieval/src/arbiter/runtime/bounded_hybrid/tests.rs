// SPDX-License-Identifier: Apache-2.0
use super::*;

fn sample() -> Vec<GraphMetadataRow> {
    use GraphMetadataRow::*;
    vec![
        Graph {
            graph: 0,
            parent_graph: None,
            parent_node: None,
            attribute: String::new(),
            outer_bindings: "not_applicable".into(),
        },
        Node {
            graph: 0,
            node: 0,
            name: String::new(),
            domain: String::new(),
            op: "Shape".into(),
            ep: CPU.into(),
            version: 1,
        },
        Node {
            graph: 0,
            node: 1,
            name: String::new(),
            domain: String::new(),
            op: "Other".into(),
            ep: CUDA.into(),
            version: 1,
        },
        Value {
            graph: 0,
            value: 1,
            name: "control".into(),
            element_type: Some(7),
            rank: Some(1),
            type_state: "tensor".into(),
            provenance: "node_output".into(),
            producer: Some(0),
        },
        Dimension {
            graph: 0,
            value: 1,
            index: 0,
            number: Some(5),
            symbol: None,
        },
        Edge {
            graph: 0,
            node: Some(0),
            value: Some(1),
            role: 2,
            slot: 0,
        },
        Edge {
            graph: 0,
            node: Some(1),
            value: Some(1),
            role: 0,
            slot: 0,
        },
        End { graph: 0 },
    ]
}

#[test]
fn bounded_hybrid_cpu_value_gate_refuses_unprofiled_or_non_control_tensors() -> Result<()> {
    use GraphMetadataRow::*;
    let rows = sample();
    validate_cpu_values(&rows)?;

    let rejects = [
        (
            4,
            Dimension {
                graph: 0,
                value: 1,
                index: 0,
                number: Some(6),
                symbol: None,
            },
        ),
        (
            4,
            Dimension {
                graph: 0,
                value: 1,
                index: 0,
                number: None,
                symbol: Some("dynamic".into()),
            },
        ),
        (
            5,
            Edge {
                graph: 0,
                node: Some(0),
                value: Some(1),
                role: 1,
                slot: 0,
            },
        ),
        (
            5,
            Edge {
                graph: 0,
                node: Some(0),
                value: Some(2),
                role: 2,
                slot: 0,
            },
        ),
    ];
    for (index, replacement) in rejects {
        let mut rows = rows.clone();
        rows[index] = replacement;
        assert!(validate_cpu_values(&rows).is_err());
    }
    let mut rows = rows;
    let Value { element_type, .. } = &mut rows[3] else {
        unreachable!()
    };
    *element_type = Some(1);
    assert!(validate_cpu_values(&rows).is_err());
    Ok(())
}
