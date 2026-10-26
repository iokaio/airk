# SPDX-License-Identifier: Apache-2.0
"""Measure real brokered code queries and source publications on an original bounded corpus."""
import argparse
import importlib.util
import json
from pathlib import Path
import platform
import tempfile
import time

spec = importlib.util.spec_from_file_location("airk_service_fixture", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True, help="Main executable in the verified runtime pair")
    parser.add_argument("--files", type=int, default=64)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 2 <= args.files <= 2048:
        parser.error("files must be 2..2048")
    binary = args.binary.resolve(strict=True)
    observations = []
    with tempfile.TemporaryDirectory(prefix="airk-native-code-") as temporary:
        base = Path(temporary).resolve(strict=True)
        root, state = base / "sources", base / "state"
        root.mkdir()
        state.mkdir()
        for number in range(args.files):
            (root / f"module_{number:04}.ts").write_text(
                f"export class Service{number} {{\n  perform{number}(input: string): string {{\n"
                "    db.query(input);\n    return input;\n  }\n}\n", encoding="utf-8")
        fixture = clients.Fixture(binary.parent, base)
        assert fixture.binary == binary
        def measured(label, operation):
            started = time.perf_counter_ns()
            value = operation()
            observations.append({"operation": label, "elapsed_us": (time.perf_counter_ns() - started) // 1000,
                                 "response_utf8_bytes": len(json.dumps(value).encode())})
            return value
        def tool(label, name, arguments=None):
            return measured(label, lambda: fixture.published_status("benchmark") if name == "airk_status" else fixture.tool("benchmark", name, {"response_mode": "full", **(arguments or {})}))
        try:
            fixture.start()
            fixture.register("benchmark", root, state)
            fixture.configure("benchmark", {"operation": "vectors", "enabled": False})
            fixture.configure("benchmark", {"operation": "detail", "level": "detailed"})
            measured("index_initial", lambda: fixture.job("benchmark", "index"))
            initial = tool("status_initial", "airk_status")
            assert initial["files"] == args.files
            measured("index_unchanged", lambda: fixture.job("benchmark", "index"))
            assert tool("status_unchanged", "airk_status")["publication"] == initial["publication"]
            for label in ("first_direct_query", "repeated_direct_query"):
                result = tool(label, "airk_code_facts", {"name": "Service0::perform0", "kind": "definition", "compact": False})
                assert len(result["items"]) == 1 and result["items"][0]["citation"]["path"] == "module_0000.ts"
            edges = tool("incoming_call_candidates", "airk_relationships", {"symbol": "db.query", "kind": "call", "limit": 100, "max_bytes": 65536})
            assert len(edges["items"]) == min(args.files, 100)
            assert all(edge["resolution"] == "syntax_candidate" for edge in edges["items"])
            (root / "module_0000.ts").write_text("export function replacement() {}\n", encoding="utf-8")
            measured("index_changed", lambda: fixture.job("benchmark", "index"))
            assert not tool("stale_definition_check", "airk_code_facts", {"name": "Service0::perform0", "compact": False})["items"]
            (root / "module_0000.ts").unlink()
            measured("index_deleted", lambda: fixture.job("benchmark", "index"))
            status = tool("status_final", "airk_status")
            assert status["files"] == args.files - 1
            report = {"schema": "airk-code-benchmark/2", "platform": platform.platform(),
                      "version": json.loads((fixture.runtime / "airk-runtime.json").read_text())["version"],
                      "runtime_manifest_sha256": clients.hashlib.sha256((fixture.runtime / "airk-install.json").read_bytes()).hexdigest(),
                      "files_initial": args.files, "files_final": status["files"], "index_detail": "detailed",
                      "derived_logical_disk_bytes": sum(file.stat().st_size for file in state.rglob("*") if file.is_file()),
                      "observations": observations, "correctness_checks_passed": True,
                      "limitations": ["Timings include new CLI brokerage and bounded publication/admission observation waits; the project worker remains shared.",
                                      "Source job completion is separate from semantic readiness; this corpus opts out of inference.",
                                      "No physical-I/O, installed-editor, provider-token or billed-savings claim."]}
        finally:
            fixture.close()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"Native code benchmark correctness passed; report: {args.output}")


if __name__ == "__main__":
    main()
