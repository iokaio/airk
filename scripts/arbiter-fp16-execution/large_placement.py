# SPDX-License-Identifier: Apache-2.0
"""Measure fixed Large CPU control payloads from complete native final graph rows."""
from collections import Counter


def cpu_controls(graph):
    rows = graph["rows"]
    roots = [row for row in rows if row["kind"] == "graph"]
    if len(roots) != 1 or roots[0]["graph"] != 0 or roots[0]["parent_graph"] is not None:
        raise ValueError("one Large root graph required")
    nodes = {row["node"]: row for row in rows if row["kind"] == "node"}
    providers = Counter(row["ep"] for row in nodes.values())
    if set(providers) != {"CPUExecutionProvider", "CUDAExecutionProvider"}:
        raise ValueError("observed CPU and CUDA providers required")
    cpu = {i for i, row in nodes.items() if row["ep"] == "CPUExecutionProvider"}
    edges = [row for row in rows if row["kind"] == "edge" and row["node"] in cpu]
    if any(row["role"] == 1 for row in edges):
        raise ValueError("CPU node has an implicit input")
    ids = {row["value"] for row in edges if row["value"] is not None}
    values = {row["value"]: row for row in rows if row["kind"] == "value"}
    dims = {(row["value"], row["index"]): row for row in rows if row["kind"] == "dimension"}
    widths = {6: 4, 7: 8, 9: 1}
    types, shapes = Counter(), Counter()
    payloads = []
    longest = 0
    for key in ids:
        value = values[key]
        rank, dtype = value["rank"], value["element_type"]
        if value["type_state"] != "tensor" or dtype not in widths or rank not in (0, 1):
            raise ValueError("CPU value outside fixed INT32, INT64 or BOOL scalar/vector scope")
        count = 1
        if rank == 1:
            axis = dims[(key, 0)]
            count = axis["number"]
            if type(count) is not int or count < 0 or axis["symbol"] is not None:
                raise ValueError("unbounded CPU control vector")
            longest = max(longest, count)
        types[dtype] += 1
        shapes[rank] += 1
        payloads.append(widths[dtype] * count)
    if not payloads:
        raise ValueError("CPU tensor evidence missing")
    return {"provider_nodes": dict(providers), "cpu_value_types": dict(types), "cpu_value_ranks": dict(shapes),
            "unique_cpu_values": len(ids), "maximum_vector_elements": longest,
            "maximum_value_bytes": max(payloads), "summed_unique_payload_bytes": sum(payloads)}
