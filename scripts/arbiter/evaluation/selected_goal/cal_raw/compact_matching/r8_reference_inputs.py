# SPDX-License-Identifier: Apache-2.0
"""Select exact completed R8 Compact inputs for the existing independent CPU owner."""

import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from r7_inputs import (AUDIT, CAMPAIGN, GRAPH, IDENTITY, SERVICE, canonical,
                       checked, parent, read, reference, require)
from r8_reuse import R8, R8_NATIVE
from r8_capture import LIFETIME_SHAS, TERMINAL_SHAS, verify_terminal


def selected(projection_ref, compiler_ref, graph_ref, first, count):
    require(type(first) is int and type(count) is int and 0 <= first < 100
            and 1 <= count <= 8 and first + count <= 100
            and first // 8 == (first + count - 1) // 8, "one original R8 phase interval required")
    projection_pin = canonical(projection_ref, R8)
    compiler_pin = canonical(compiler_ref, R8_NATIVE)
    graph_pin = canonical(graph_ref, GRAPH)
    projection, compiler, graph = (read(pin) for pin in (projection_pin, compiler_pin, graph_pin))
    require(projection["original_requests"] == 100 and projection["service_sha256"] == SERVICE
            and projection["launch_audit"]["sha256"] == AUDIT
            and compiler["projection_sha256"] == projection_pin["sha256"]
            and compiler["reference_preflight"] == projection["reference_preflight"]
            and compiler["passed"] is True and compiler["NN_invocations"] == 0
            and parent.settled(compiler["job"])
            and compiler["image"]["sha256"] == graph["image"]["sha256"]
            and all(graph["result"][key] == IDENTITY[key] for key in
                    ("model_sha256", "weight_sha256", "typed_graph_sha256"))
            and graph["runtime_manifest"]["sha256"] == IDENTITY["runtime_manifest_sha256"],
            "wrong original R8 compiler or exact protected candidate")
    verify_terminal(projection["plan"], LIFETIME_SHAS, TERMINAL_SHAS)
    certificate = read(projection["reference_preflight"])
    model = projection["models"]["compact"]
    require(compiler["inputs"]["compact"] == {key: model[key] for key in ("path", "sha256")}
            and certificate["complete"] is True
            and certificate["lifetime_plan_sha256"] == projection["plan_sha256"],
            "wrong R8 Compact inputs or independent certificate")
    rows = read(model)["rows"]
    require([row["ordinal"] for row in rows] == list(range(100))
            and [row["id"] for row in rows] == [row["id"] for row in certificate["rows"]],
            "incomplete original R8 Compact roster")
    for row in rows[first:first + count]:
        captured = certificate["rows"][row["ordinal"]]
        require(row["status"] == "eligible" and captured["native_capture_status"] == "native_request_captured",
                "closed R8 projection differs from the expected eligible population")
        pin, = row["captures"]
        observation = read(pin)
        request = observation["request"]
        require(observation["Context_NN_dispatch"] is False and request == captured["request"]
                and request["roster"] == row["roster"]
                and request["remaining_input_tokens"] == row["remaining_input_tokens"]
                and min(request["remaining_ms"], observation["remaining_ns_at_observation"] // 1_000_000)
                == row["remaining_ms"], "R8 request custody or original allowance differs")
    checked(graph["runtime_manifest"])
    return rows[first:first + count], {
        "mode": "r8_original", "projection": projection_pin, "compiler": compiler_pin, "graph": graph_pin,
        "plan_sha256": projection["plan_sha256"], "launch_audit_sha256": AUDIT, **IDENTITY,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--first", type=int, required=True)
    parser.add_argument("--count", type=int, required=True)
    args = parser.parse_args()
    require(sys.platform == "win32" and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
            "use checked Windows Python -I -B -S")
    rows, provenance = selected(*(reference(*item) for item in (R8, R8_NATIVE, GRAPH)), args.first, args.count)
    print(json.dumps({"ordinals": [row["ordinal"] for row in rows], "mode": provenance["mode"],
                      "NN_invocations": 0}, allow_nan=False))


if __name__ == "__main__":
    main()
