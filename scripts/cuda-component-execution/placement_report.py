# SPDX-License-Identifier: Apache-2.0
"""Summarize a complete, reviewed CUDA graph diagnostic without launching a model."""

import argparse
from collections import Counter, defaultdict
import hashlib
import json
from math import prod
from pathlib import Path
import runpy


OWNER = Path(__file__).resolve().parents[1] / "cuda-component-execution.py"
CPU = "CPUExecutionProvider"
# ONNX TensorProto.DataType from the collector's pinned ONNX source.
INTEGER_OR_BOOL = {2, 3, 4, 5, 6, 7, 9, 12, 13, 21, 22, 25, 26}
TENSOR_TYPES = range(1, 27)
TYPE_CLASSES = (
    "integer_or_bool_tensor", "other_tensor", "unknown_type",
    "unknown_tensor_element", "non_tensor",
)
NODE_INPUT_ROLES = (0, 1)
NODE_OUTPUT_ROLE = 2
CPU_SHAPE_ELEMENT_BYTES = {7: 8, 9: 1}  # INT64 and BOOL representations in the pinned ORT core.


def require(condition, message):
    if not condition:
        raise ValueError(message)


def decode(raw):
    def unique_fields(pairs):
        fields = {}
        for key, value in pairs:
            require(key not in fields, "duplicate JSON field: " + key)
            fields[key] = value
        return fields

    def invalid_constant(value):
        raise ValueError("nonfinite JSON constant: " + value)

    return json.loads(raw, object_pairs_hook=unique_fields, parse_constant=invalid_constant)


def read_checked(path, sha256, maximum):
    require(path.is_absolute() and path.resolve(strict=True) == path and path.is_file(),
            "absolute unaliased evidence file required")
    for ancestor in (path, *path.parents):
        metadata = ancestor.lstat()
        require(not ancestor.is_symlink()
                and not getattr(metadata, "st_file_attributes", 0) & 0x400,
                "reparse evidence path refused")
    with path.open("rb") as stream:
        raw = stream.read(maximum + 1)
    require(0 < len(raw) <= maximum and hashlib.sha256(raw).hexdigest() == sha256,
            "reviewed evidence bytes differ: " + str(path))
    return {"path": str(path), "bytes": len(raw), "sha256": sha256}, decode(raw)


def graph_result(receipt_path, receipt_sha256):
    receipt_pin, receipt = read_checked(receipt_path, receipt_sha256, 16 << 20)
    require(isinstance(receipt, dict)
            and receipt.get("schema") == "airk.private.owned-cuda-graph-metadata-diagnostic/1"
            and all(receipt.get(key) is True for key in
                    ("complete", "diagnostic_only", "graph_metadata_observed",
                     "whole_phase_deadline_not_refreshed"))
            and all(receipt.get(key) is False for key in
                    ("qualification", "product_admission", "component_pass", "model_pass",
                     "whole_phase_expired"))
            and receipt.get("failure", "missing") is None
            and all(receipt.get(key) == [] for key in
                    ("post_input_drift", "final_input_errors", "final_proof_errors"))
            and receipt.get("final_guard_errors") == {}
            and receipt.get("final_proof_success") == ["native_result", "stdout"]
            and type(receipt.get("selected_logs_complete")) is bool,
            "complete owned graph execution receipt required")
    native_pin = receipt["native_result"]
    native_path = receipt_path.with_name("native-result.json")
    require(isinstance(native_pin, dict) and set(native_pin) == {"path", "bytes", "sha256"}
            and Path(native_pin["path"]) == native_path, "native result must be beside its receipt")
    actual_pin, native = read_checked(native_path, native_pin["sha256"], 16 << 20)
    require(actual_pin == native_pin, "retained native result file pin differs")
    owned = receipt["owned"]
    custody = receipt["graph_custody"]
    require(type(owned.get("exit_code")) is int and owned["exit_code"] == 0
            and owned.get("owned_descendants_settled") is True
            and owned.get("owned_job_forced_termination") is False
            and owned.get("timed_out") is False
            and type(owned.get("owned_job_final_active_processes")) is int
            and owned["owned_job_final_active_processes"] == 0
            and type(custody.get("opened_handles")) is int
            and type(custody.get("closed_handles")) is int
            and 0 < custody["opened_handles"] == custody["closed_handles"] == len(custody["boundary"])
            and custody.get("close_errors") == [],
            "settled process and input custody required")
    require(isinstance(native, dict)
            and native.get("schema") == "airk.private.cuda-graph-metadata-diagnostic/1"
            and native.get("diagnostic_complete") is True
            and native.get("outcome") == "graph_metadata_observed"
            and native.get("forward_dispatched") is False
            and native.get("selected_logs_complete") is receipt["selected_logs_complete"]
            and type(native.get("pid")) is int and 0 < native["pid"] == owned.get("owned_leader_pid")
            and native.get("execution_user") == native.get("default_owner") == receipt["execution_user_sid"],
            "native graph observation scope or owned process identity differs")
    placement = native["placement"]
    require(placement.get("scope") == "onnxruntime-final-session-state-ep-placement"
            and placement.get("complete") is True and placement.get("shim_status") == 0
            and placement.get("ort_error_code") == 0
            and placement.get("reported_nodes") == len(placement["nodes"]),
            "complete final placement observation required")
    validator = runpy.run_path(str(OWNER))["validate_graph_metadata"]
    validator(native["graph_metadata"], placement)
    return receipt_pin, native_pin, native


def value_type(value):
    if value["type_state"] == "unsupported_non_tensor":
        return "non_tensor"
    if value["type_state"] == "unknown":
        return "unknown_type"
    element = value["element_type"]
    if element in INTEGER_OR_BOOL:
        return "integer_or_bool_tensor"
    if element in TENSOR_TYPES:
        return "other_tensor"
    return "unknown_tensor_element"


def counts_by_type(values, value_ids):
    counts = Counter(value_type(values[value]) for value in value_ids)
    return {key: counts[key] for key in TYPE_CLASSES}


def static_shape_summary(values, shapes, value_ids):
    ranks = Counter(values[value]["rank"] for value in value_ids)
    unknown_rank = ranks.pop(None, 0)
    sizes = []
    max_dimension = 0
    for value_id in value_ids:
        value = values[value_id]
        width = CPU_SHAPE_ELEMENT_BYTES.get(value["element_type"])
        shape = shapes[value_id]
        if (value["type_state"] == "tensor" and width is not None
                and value["rank"] == len(shape) and all(axis is not None for axis in shape)):
            sizes.append(width * prod(shape))
            max_dimension = max(max_dimension, max(shape, default=0))
    return {
        "rank_counts": dict(sorted(ranks.items())),
        "unknown_rank": unknown_rank,
        "fixed_int64_bool_values": len(sizes),
        "other_or_dynamic_values": len(value_ids) - len(sizes),
        "max_fixed_dim": max_dimension,
        "max_fixed_payload_bytes": max(sizes, default=0),
        "sum_fixed_payload_bytes": sum(sizes),
    }


def summarize_graph(graph, rows):
    nodes = {row["node"]: row for row in rows if row["kind"] == "node"}
    values = {row["value"]: row for row in rows if row["kind"] == "value"}
    providers = Counter(row["ep"] for row in nodes.values())
    cpu_nodes = {node for node, row in nodes.items() if row["ep"] == CPU}
    shapes = defaultdict(list)
    for row in rows:
        if row["kind"] == "dimension":
            shapes[row["value"]].append(row["number"])
    cpu_values = defaultdict(set)
    absent_cpu_slots = Counter()
    output_providers = {}
    input_providers = defaultdict(set)
    for row in rows:
        if row["kind"] != "edge" or row["node"] is None:
            continue
        node, value, role = row["node"], row["value"], row["role"]
        if value is None:
            if node in cpu_nodes:
                absent_cpu_slots[node] += 1
            continue
        provider = nodes[node]["ep"]
        if node in cpu_nodes:
            cpu_values[node].add(value)
        if role == NODE_OUTPUT_ROLE:
            output_providers[value] = provider
        elif role in NODE_INPUT_ROLES:
            input_providers[value].add(provider)

    directions = Counter()
    cross_provider_values = set()
    shared_without_producer = Counter()
    for value, consumers in input_providers.items():
        producer = output_providers.get(value)
        if producer is None:
            if len(consumers) > 1:
                shared_without_producer[values[value]["provenance"]] += 1
            continue
        destinations = consumers - {producer}
        if destinations:
            cross_provider_values.add(value)
        directions.update((producer, destination) for destination in destinations)

    incident = {value for values_at_node in cpu_values.values() for value in values_at_node}
    return {
        **{key: graph[key] for key in
           ("graph", "parent_graph", "parent_node", "attribute", "outer_bindings")},
        "node_count": len(nodes),
        "provider_node_counts": dict(sorted(providers.items())),
        "cpu_incident_value_counts": counts_by_type(values, incident),
        "cpu_incident_static_shapes": static_shape_summary(values, shapes, incident),
        "cpu_nodes": [
            {"node": node, "domain": nodes[node]["domain"], "op": nodes[node]["op"],
             "version": nodes[node]["version"],
             "incident_value_counts": counts_by_type(values, cpu_values[node]),
             "absent_edge_slots": absent_cpu_slots[node]}
            for node in sorted(cpu_nodes)
        ],
        "cross_provider_value_count": len(cross_provider_values),
        "cross_provider_static_shapes": static_shape_summary(values, shapes, cross_provider_values),
        "cross_provider_value_counts_by_direction": [
            {"producer_provider": producer, "consumer_provider": consumer, "count": count}
            for (producer, consumer), count in sorted(directions.items())
        ],
        "multi_provider_inputs_without_local_producer": dict(sorted(shared_without_producer.items())),
    }


def summarize(metadata):
    sections = defaultdict(list)
    for row in metadata["rows"]:
        sections[row["graph"]].append(row)
    graphs = [summarize_graph(rows[0], rows) for rows in sections.values()]
    providers = Counter()
    operators = Counter()
    for graph in graphs:
        providers.update(graph["provider_node_counts"])
        operators.update((node["domain"], node["op"], node["version"]) for node in graph["cpu_nodes"])
    return {
        "node_count": sum(providers.values()),
        "provider_node_counts": dict(sorted(providers.items())),
        "cpu_operator_node_counts": [
            {"domain": domain, "op": op, "version": version, "count": count}
            for (domain, op, version), count in sorted(operators.items())
        ],
        "cpu_incident_value_counts": {
            kind: sum(graph["cpu_incident_value_counts"][kind] for graph in graphs)
            for kind in TYPE_CLASSES
        },
        "cross_provider_value_count": sum(graph["cross_provider_value_count"] for graph in graphs),
        "graphs": graphs,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--graph-receipt", type=Path, required=True)
    parser.add_argument("--graph-receipt-sha256", required=True)
    args = parser.parse_args()
    receipt, result, native = graph_result(args.graph_receipt, args.graph_receipt_sha256)
    report = {
        "graph_receipt": receipt,
        "native_result": result,
        "selected_logs_complete": native["selected_logs_complete"],
        **summarize(native["graph_metadata"]),
    }
    print(json.dumps(report, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
