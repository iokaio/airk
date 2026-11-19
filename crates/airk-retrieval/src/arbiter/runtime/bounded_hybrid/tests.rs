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
    validate_cpu_values(&rows, COMPACT_CPU_PAYLOAD_BYTES)?;

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
        assert!(validate_cpu_values(&rows, COMPACT_CPU_PAYLOAD_BYTES).is_err());
    }
    let mut rows = rows;
    let Value { element_type, .. } = &mut rows[3] else {
        unreachable!()
    };
    *element_type = Some(1);
    assert!(validate_cpu_values(&rows, COMPACT_CPU_PAYLOAD_BYTES).is_err());
    Ok(())
}

#[test]
fn compact_answer_uses_original_operation_and_fixture_expiries() -> Result<()> {
    let load_end = Instant::now();
    let now = load_end + Duration::from_secs(1);
    let expiry = now + Duration::from_secs(120);
    let whole_expiry = expiry + Duration::from_secs(1);
    let limit = Duration::from_secs(120);
    require_answer_deadline(now, expiry, whole_expiry, limit)?;
    for (deadline, overall) in [
        (now, whole_expiry),
        (load_end, whole_expiry),
        (expiry + Duration::from_nanos(1), whole_expiry),
        (expiry, expiry - Duration::from_nanos(1)),
    ] {
        assert!(require_answer_deadline(now, deadline, overall, limit).is_err());
    }
    Ok(())
}

#[test]
fn compact_resource_operations_keep_independent_deadlines_and_exact_width() -> Result<()> {
    let load_expiry = Instant::now() + Duration::from_secs(120);
    let now = load_expiry + Duration::from_secs(1);
    let operation_expiry = now + Duration::from_secs(120);
    let overall_expiry = now + Duration::from_secs(3600);
    let mut input = CompiledInput {
        prompt: String::new(),
        input_ids: vec![1; 8192],
        attention_mask: vec![1; 8192],
        position_ids: (0..8192).collect(),
        decision_position: 8191,
        candidate_positions: (0..8).collect(),
        option_mask: vec![true; 8],
        input_digest: Sha256Digest::from_bytes([1; 32]),
    };
    require_resource_operation(&input, now, operation_expiry, overall_expiry)?;
    assert!(require_resource_operation(&input, now, now, overall_expiry).is_err());
    assert!(
        require_resource_operation(
            &input,
            now,
            operation_expiry + Duration::from_nanos(1),
            overall_expiry,
        )
        .is_err()
    );
    assert!(
        require_resource_operation(
            &input,
            now,
            operation_expiry,
            operation_expiry - Duration::from_nanos(1),
        )
        .is_err()
    );
    input.attention_mask[0] = 0;
    assert!(require_resource_operation(&input, now, operation_expiry, overall_expiry).is_err());
    input.attention_mask[0] = 1;
    input.option_mask[0] = false;
    assert!(require_resource_operation(&input, now, operation_expiry, overall_expiry).is_err());
    input.option_mask[0] = true;
    input.input_ids.truncate(1024);
    assert!(require_resource_operation(&input, now, operation_expiry, overall_expiry).is_err());
    Ok(())
}

#[test]
fn large_payload_budget_is_separate_from_compact() -> Result<()> {
    let mut rows = sample();
    rows.pop();
    for value in 2..=1000 {
        rows.extend([
            GraphMetadataRow::Value {
                graph: 0,
                value,
                name: format!("control-{value}"),
                element_type: Some(7),
                rank: Some(1),
                type_state: "tensor".into(),
                provenance: "node_output".into(),
                producer: Some(0),
            },
            GraphMetadataRow::Dimension {
                graph: 0,
                value,
                index: 0,
                number: Some(5),
                symbol: None,
            },
            GraphMetadataRow::Edge {
                graph: 0,
                node: Some(0),
                value: Some(value),
                role: 2,
                slot: value - 1,
            },
        ]);
    }
    rows.push(GraphMetadataRow::End { graph: 0 });
    assert!(validate_cpu_values(&rows, COMPACT_CPU_PAYLOAD_BYTES).is_err());
    validate_cpu_values(&rows, LARGE_CPU_PAYLOAD_BYTES)?;
    Ok(())
}
