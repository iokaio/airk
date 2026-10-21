# SPDX-License-Identifier: Apache-2.0
"""Original bounded corpus; repeatable correctness/latency/disk/context baseline."""
import argparse
import json
import os
import platform
import subprocess
import tempfile
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--files", type=int, default=256)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.files <= 4096:
        parser.error("--files must be 1..4096")
    binary = args.binary.resolve(strict=True)
    observations = []
    with tempfile.TemporaryDirectory(prefix="airk-code-benchmark-") as temporary:
        base = Path(temporary)
        root, state = base / "sources", base / "state"
        root.mkdir()
        for number in range(args.files):
            (root / f"module_{number:04}.ts").write_text(
                f"export class Service{number} {{\n"
                f"  perform{number}(input: string): string {{\n"
                "    db.query(input);\n    return input;\n  }\n}\n",
                encoding="utf-8",
            )

        def run(label, *command):
            start = time.perf_counter_ns()
            result = subprocess.run(
                [str(binary), "--root", str(root), "--data-dir", str(state),
                 "--collection", "benchmark", *command],
                capture_output=True, check=True, timeout=120,
            )
            elapsed = (time.perf_counter_ns() - start) // 1000
            payload = json.loads(result.stdout)
            observations.append({"operation": label, "elapsed_us": elapsed,
                                 "response_utf8_bytes": len(result.stdout)})
            return payload

        # Call candidates belong to Detailed; Agent Lean is the user default.
        run("configure_detail", "collections", "extraction", "detail", "detailed")
        index = run("index_initial", "index")
        assert index["indexed"] == args.files
        reused = run("index_unchanged", "index")
        assert reused["reused"] == args.files
        for label in ["cold_worker_exact", "repeat_worker_exact"]:
            result = run(label, "code-facts", "--name", "Service0::perform0", "--kind", "definition")
            assert len(result["facts"]) == 1
            assert result["facts"][0]["citation"]["path"] == "module_0000.ts"
        edges = run("incoming_call_candidates", "relationships", "db.query", "--kind", "call", "--limit", "100", "--max-bytes", "65536")
        assert len(edges["relationships"]) == min(args.files, 100)
        assert all(edge["resolution"] == "syntax_candidate" for edge in edges["relationships"])
        (root / "module_0000.ts").write_text("export function replacement() {}\n", encoding="utf-8")
        run("index_changed", "index")
        assert not run("stale_definition_check", "code-facts", "--name", "Service0::perform0")["facts"]
        (root / "module_0000.ts").unlink()
        run("index_deleted", "index")
        status = run("status", "status")
        assert status["files"] == args.files - 1
        disk = sum(file.stat().st_size for file in state.rglob("*") if file.is_file())
        original_source_bytes = sum(file.stat().st_size for file in root.iterdir())
        report = {
            "schema": "airk-code-benchmark/1", "platform": platform.platform(),
            "binary_version": subprocess.check_output([str(binary), "--version"], text=True).strip(),
            "files_initial": args.files, "files_final": status["files"],
            "index_detail": "detailed",
            "derived_logical_disk_bytes": disk, "remaining_source_bytes": original_source_bytes,
            "observations": observations, "correctness_checks_passed": True,
            "limitations": ["Each query starts a new process; repeat is OS-page-cache reuse, not a shared warm worker",
                            "Timing includes process startup; no latency acceptance threshold",
                            "No paired coding-agent tasks, measured provider tokens or billed savings",
                            "Native source-change/import/concurrent-reader coverage lives in Rust integration tests"],
            "peak_child_rss_bytes": None, "child_cpu_seconds": None,
        }
        if os.name == "posix":
            import resource
            usage = resource.getrusage(resource.RUSAGE_CHILDREN)
            report["peak_child_rss_bytes"] = usage.ru_maxrss * (1 if platform.system() == "Darwin" else 1024)
            report["child_cpu_seconds"] = usage.ru_utime + usage.ru_stime
        else:
            report["limitations"].append("CPU/RSS unavailable from the stdlib on Windows; use platform profiling")
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"Benchmark correctness passed; report: {args.output}")


if __name__ == "__main__":
    main()
