# SPDX-License-Identifier: Apache-2.0
"""Measure bounded saved-source checks on disposable original files through MCP.

Uses an exact staged runtime pair and its own service; never installs AIRK or
changes the normal project's sources. Logical bytes and end-to-end latency are
observed; physical disk I/O, process peak memory and other platforms are not.
"""
import argparse
import hashlib
import importlib.util
import json
import os
import re
from pathlib import Path
import tempfile
import time


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run(args):
    clients = load("verification_clients", "check-service-clients.py")
    bench = load("verification_replay", "benchmark-mcp-investigation.py")
    builder = load("verification_builder", "build-service-runtime.py")
    runtime = clients.runtime_root(args.runtime_dir)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    runtime_receipt = json.loads((runtime / "airk-runtime.json").read_text(encoding="utf-8"))
    build = json.loads((runtime / "airk-build.json").read_text(encoding="utf-8"))
    source_before = builder.source_identity()
    if source_before != build["source"]:
        raise ValueError("staged_runtime_source_identity_mismatch")
    binary = runtime / ("airk.exe" if os.name == "nt" else "airk")
    if not binary.is_file():
        binary = runtime / "AIRK.app/Contents/MacOS/airk"
    helper = binary.with_name("airk-embed.exe" if os.name == "nt" else "airk-embed")
    if builder.digest(binary) != build["runtime"]["airk_sha256"] or builder.digest(helper) != build["runtime"]["embed_sha256"]:
        raise ValueError("staged_runtime_payload_identity_mismatch")
    report = {"schema": "airk.source-verification-measurement/1", "version": runtime_receipt["version"],
              "runtime_build": build, "native_source_before": source_before,
              "measurement_script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "launcher_sha256": builder.digest(binary),
              "samples_per_case_and_mode": args.samples, "cases": [], "checks": {},
              "limitations": ["Warm filesystem caches; no cache flush or physical disk I/O measurement.",
                              "Logical source bytes exclude ignore-policy and index metadata reads.",
                              "No whole-process peak memory, installed editor or cross-platform qualification."]}
    report["stage"] = "fixture_setup"
    try:
        with tempfile.TemporaryDirectory(prefix="airk-verification-owned-", dir=os.environ.get("AIRK_PRIVATE_CENSUS_BASE")) as temporary:
            owner = Path(temporary).resolve()
            root, state = owner / "source", owner / "state"
            root.mkdir()
            state.mkdir()
            contents = {"small.txt": b"Saved verification measurement.\n",
                        "medium.txt": b"Saved verification measurement.\n" * 2048,
                        "large.txt": b"Saved verification measurement.\n" * 270600}
            for number in range(8):
                contents[f"tiny-{number}.txt"] = f"VerificationMarker original source {number}.\n".encode()
            for name, content in contents.items():
                (root / name).write_bytes(content)
            with clients.Fixture(runtime, owner) as service:
                service.register("verification", root, state)
                service.rpc({"configure_watch": {"project": service.projects["verification"]["binding"], "enabled": False}})
                service.configure("verification", {"operation": "vectors", "enabled": False})
                service.configure("verification", {"operation": "detail", "level": "agent_lean"})
                service.configure("verification", {"operation": "usage", "mode": "disabled"})
                report["stage"] = "fixture_index"
                service.job("verification", "index")
                service.published_status("verification", time.monotonic() + 90)
                configuration = {"command": str(service.binary), "args": [*service.arguments("verification"), "mcp"],
                                 "expected_version": runtime_receipt["version"], "env": {"AIRK_MCP_RESPONSE_MODE": "full"}}
                with bench.Session(configuration, 30) as session:
                    publication = None
                    report["stage"] = "input_catalogs"
                    # Exercise real SDK discovery and the canonical native
                    # Workbench catalog; compare effective input contracts,
                    # excluding only explanatory schema annotations.
                    catalog, _ = session.request("tools/list", {})
                    def capability_read(until):
                        reply, _ = session.request("tools/call", {"name": "capabilities", "arguments": {"response_mode": "full"}}, until)
                        (output / "capability-catalog-reply.json").write_text(json.dumps(reply, indent=2) + "\n", encoding="utf-8")
                        return reply
                    capability, retries = clients.read_envelope(capability_read, time.monotonic() + 15, attempts=241)
                    report["catalog_typed_read_retries"] = retries
                    (output / "capability-catalog-reply.json").write_text(json.dumps(capability, indent=2) + "\n", encoding="utf-8")
                    assert not capability.get("isError"), "capability_catalog_unavailable"
                    capability = bench.body(capability)
                    operations = capability["workbench"]["operations"]
                    controls = ("verification", "verification_max_sources", "verification_max_bytes", "verification_deadline_ms")
                    cli_catalog = json.loads(service.run(*service.arguments("verification"), "tools", "list", "--json").stdout)
                    cli_tools = {tool["name"].removeprefix("airk_"): tool for tool in cli_catalog["tools"]}
                    expected_native = {tool.removeprefix("airk_") for tool in clients.TOOLS}
                    expected_operations = expected_native - {"parse"}
                    assert len(cli_catalog["tools"]) == len(expected_native) and set(cli_tools) == expected_native, "native_cli_tool_inventory_changed"
                    assert set(operations) == expected_operations, "canonical_operation_inventory_changed"
                    public_names = [tool["name"] for tool in catalog["tools"]]
                    assert len(public_names) == len(clients.MCP_TOOLS) and set(public_names) == clients.MCP_TOOLS, "public_tool_inventory_changed"
                    compared = 0
                    cli_compared = 0
                    for tool in catalog["tools"]:
                        name = tool["name"].removeprefix("airk_")
                        effective = lambda schema: {key: value for key, value in schema.items() if key not in ("description", "title")} if schema else None
                        if name in cli_tools:
                            for field in controls:
                                cli = cli_tools[name]["inputSchema"]
                                native = tool["inputSchema"]
                                assert effective(cli.get("properties", {}).get(field)) == effective(native.get("properties", {}).get(field)), "cli_mcp_verification_input_mismatch"
                                assert (field in cli.get("required", [])) == (field in native.get("required", [])), "cli_mcp_verification_required_mismatch"
                                cli_compared += 1
                        if name not in operations:
                            continue
                        for field in controls:
                            native = tool["inputSchema"].get("properties", {}).get(field)
                            workbench = operations[name].get("properties", {}).get(field)
                            assert effective(native) == effective(workbench), "verification_input_catalog_mismatch"
                            assert (field in tool["inputSchema"].get("required", [])) == (field in operations[name].get("required", [])), "verification_required_input_mismatch"
                            compared += 1
                    assert compared == len(expected_operations) * len(controls), "canonical_operation_field_comparisons_incomplete"
                    assert cli_compared == len(expected_native) * len(controls), "native_cli_tool_field_comparisons_incomplete"
                    report["checks"]["native_mcp_workbench_verification_input_catalog_parity"] = True
                    report["input_catalog_field_comparisons"] = {"mcp_workbench": compared, "cli_mcp": cli_compared}
                    report["stage"] = "measurements"

                    def call(tool, arguments, mode=None):
                        nonlocal publication
                        arguments = {**arguments, "response_mode": "full"}
                        if mode is not None:
                            arguments["verification"] = mode
                        envelope, timing = session.tool((tool, arguments))
                        value = bench.body(envelope)
                        token = value.get("publication", {}).get("token")
                        if token is not None:
                            if publication is not None and token != publication:
                                raise AssertionError("fixture_publication_changed")
                            publication = token
                        return envelope, value, timing

                    _, before_status, _ = call("status", {})
                    work_fields = ("state", "requested_watermark", "completed_watermark", "running_watermark", "source_job", "last_success_unix_ms")
                    # A new frontend can initially expose a pending management
                    # observation even after the fixture's index job completed.
                    # Settle that observation before comparing source-work state.
                    settle_deadline = time.monotonic() + 20
                    report["source_work_warmup_observations"] = 1
                    while before_status.get("freshness", {}).get("state") != "idle":
                        if time.monotonic() >= settle_deadline:
                            raise AssertionError("source_work_observation_did_not_settle")
                        time.sleep(.05)
                        _, before_status, _ = call("status", {})
                        report["source_work_warmup_observations"] += 1
                    before_work = {key: before_status.get("freshness", {}).get(key) for key in work_fields}
                    report["source_work_before"] = before_work
                    cases = [(name, "file_info", {"path": name}, [name]) for name in ("small.txt", "medium.txt", "large.txt")]
                    cases.append(("eight-sources", "search", {"query": "VerificationMarker", "mode": "lexical", "limit": 8},
                                  [f"tiny-{number}.txt" for number in range(8)]))
                    for name, tool, arguments, paths in cases:
                        expected_bytes = sum(len(contents[path]) for path in paths)
                        expected_hashes = {path: hashlib.sha256(contents[path]).hexdigest() for path in paths}
                        row = {"id": name, "source_count": len(paths), "logical_source_bytes": expected_bytes, "modes": {}}
                        report["cases"].append(row)
                        measurements = {"off": [], "default_report": []}
                        for sample in range(args.samples + 1):
                            # Alternate order; the first pair is a disclosed warmup.
                            modes = ("off", "default_report") if sample % 2 == 0 else ("default_report", "off")
                            for mode in modes:
                                envelope, value, timing = call(tool, arguments, "off" if mode == "off" else None)
                                assert not envelope.get("isError"), "measurement_read_failed"
                                receipt = value.get("source_verification")
                                if mode == "off":
                                    assert receipt is None, "off_added_verification"
                                else:
                                    assert receipt["mode"] == "report" and receipt["status"] == "verified", "default_report_not_verified"
                                    assert receipt["counts"]["bytes_read"] == expected_bytes, "logical_bytes_disagree"
                                    assert receipt["counts"]["matched_sources"] == len(paths), "source_count_disagrees"
                                    assert all(source["state"] == "matched" and source["observed_hash"] == source["expected_hash"] == expected_hashes[source["path"]]
                                               for source in receipt["sources"]), "source_hash_disagrees"
                                    if sample == 0:
                                        (output / f"{name}-example-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
                                measurement = {"elapsed_ms": timing["elapsed_ms"], "retries": timing["retries"],
                                               "response_bytes": len(bench.canonical(envelope)),
                                               "receipt_bytes": len(bench.canonical(receipt)) if receipt else 0}
                                if sample == 0:
                                    row["modes"][mode] = {"warmup": measurement}
                                else:
                                    measurements[mode].append(measurement)
                        for mode, samples in measurements.items():
                            row["modes"][mode].update(samples=samples, elapsed_ms=bench.distribution([sample["elapsed_ms"] for sample in samples]))
                    _, limited, _ = call("search", {"query": "VerificationMarker", "mode": "lexical", "limit": 8, "verification_max_sources": 1})
                    assert limited["source_verification"]["status"] == "partial"
                    assert limited["source_verification"]["counts"]["matched_sources"] == 1
                    report["checks"]["count_limit_does_not_certify_subset"] = True
                    _, limited, _ = call("file_info", {"path": "small.txt", "verification_max_bytes": 1})
                    assert limited["source_verification"]["counts"]["bytes_read"] == 0
                    assert limited["source_verification"]["status"] == "unverified"
                    report["checks"]["byte_limit_does_not_hash_oversized_file"] = True
                    (root / "small.txt").write_bytes(b"Changed saved bytes.\n")
                    failed, value, _ = call("file_info", {"path": "small.txt"}, "require")
                    assert failed.get("isError") is True and value["source_verification"]["sources"][0]["state"] == "mismatch"
                    assert value["error"]["retryable"] is False
                    report["checks"]["require_mismatch_preserves_indexed_evidence"] = value["content_hash"] == hashlib.sha256(contents["small.txt"]).hexdigest()
                    (root / "small.txt").write_bytes(contents["small.txt"])
                    _, after_status, _ = call("status", {})
                    after_work = {key: after_status.get("freshness", {}).get(key) for key in work_fields}
                    report["source_work_after"] = after_work
                    assert before_work == after_work, "verification_changed_source_work"
                    report["checks"]["source_work_identity_and_watermarks_unchanged"] = True
                report["frontend_cleanup"] = session.cleanup
            report["owned_service_drained"] = True
            report["source_unchanged"] = all((root / name).read_bytes() == content for name, content in contents.items())
        report["status"] = "complete"
    except (OSError, ValueError, RuntimeError, TimeoutError, AssertionError, KeyError, TypeError) as error:
        report.update(status="failed", failure_class=type(error).__name__)
        if error.args and isinstance(error.args[0], str) and re.fullmatch(r"[a-z_]{1,80}", error.args[0]):
            report["failure_code"] = error.args[0]
    report["native_source_after"] = builder.source_identity()
    report["native_source_unchanged"] = report["native_source_after"] == source_before
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return (report["status"] == "complete" and report.get("source_unchanged") and report["native_source_unchanged"]
            and all(report["checks"].values()) and report.get("frontend_cleanup", {}).get("graceful_eof", False))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=30)
    args = parser.parse_args()
    if not 1 <= args.samples <= 1000:
        parser.error("samples must be 1..1000")
    raise SystemExit(0 if run(args) else 1)
