# SPDX-License-Identifier: Apache-2.0
"""Exercise explicit content-free usage inputs through the built public CLI."""
import argparse
from contextlib import ExitStack
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
import os
import queue
from pathlib import Path
import subprocess
import tempfile
import threading
import time
import uuid

spec = importlib.util.spec_from_file_location("airk_usage_fixture", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    binary = parser.parse_args().binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="airk-usage-inputs-") as temporary, ExitStack() as stack:
        base = Path(temporary).resolve(strict=True)
        root = base / "sources"
        root.mkdir()
        (root / "fixture.txt").write_text("Original bounded usage fixture.\n", encoding="utf-8")
        state = base / "state"
        state.mkdir()
        fixture = clients.Fixture(binary.parent, base)
        assert fixture.binary == binary
        fixture.environment.pop("AIRK_USAGE_MODE", None)
        stack.enter_context(fixture)
        fixture.register("usage-fixture", root, state)
        fixture.configure("usage-fixture", {"operation": "vectors", "enabled": False})
        fixture.configure("usage-fixture", {"operation": "usage", "mode": "aggregates_only"})
        environment = fixture.environment

        def run(*command, fails=False):
            if command[0] not in ("index", "usage"):
                assert not fails
                return fixture.query("usage-fixture", *command, retry=False, admission_only=True)
            result = fixture.run(*fixture.arguments("usage-fixture"), *command, check=False)
            if result.returncode:
                assert fails, result.stderr
                return None
            value = json.loads(result.stdout)
            if "job" in value:
                job = fixture.wait(value, "failed" if fails else "succeeded")
                if fails:
                    return None
                if command[0] == "index":
                    return fixture.published_status("usage-fixture")
                value = fixture.rpc({"configuration_result": {"id": job["id"]}})["result"]
            else:
                assert not fails, (command, value)
            return value

        def write_json(name, value):
            path = base / name
            path.write_text(json.dumps(value), encoding="utf-8")
            return path

        indexed = run("index")
        # Each measured query is submitted once; only explicitly unsubmitted admission may wait.
        # Hold only this fixture's actual native writer advisory file; release before mutations.
        with fixture.hold_publication("usage-fixture"):
            request = run("search", "--mode", "lexical", "fixture")
            native_calls = run("usage", "report")["report"]["calls"]
            with ThreadPoolExecutor(max_workers=4) as workers:
                replies = list(workers.map(lambda _: run("search", "--mode", "lexical", "fixture"), range(40)))
            assert all(reply["hits"] for reply in replies)
            assert len({reply["request_id"] for reply in replies}) == 40
            native_calls += 40
            concurrent = run("usage", "report")
            assert concurrent["report"]["calls"] == native_calls, "concurrent CLI observations were lost"
            assert concurrent["lifetime"]["calls"] == native_calls
            comparisons = concurrent["report"]["comparisons"]["full_returned_sources"]
            assert comparisons["compared_calls"] == 41
            assert concurrent["report"]["eligible_evidence_calls"] == 41
            assert concurrent["report"]["missing_baseline_calls"] == 0
            assert concurrent["report"]["baseline_unavailable"] == {}
            assert int(comparisons["returned_utf8_bytes"]) > 0
            assert int(comparisons["token_estimate"]["nominal_reduction"]) < 0, "Tiny complete-file results must preserve expansion from response overhead"
            # Exercise the actual stdio adapter, including a conditional response
            # with no evidence payload. No fake provider/token measurements.
            with (base / "mcp-errors.log").open("w", encoding="utf-8") as errors:
                child = subprocess.Popen([str(binary), *fixture.arguments("usage-fixture"), "mcp"], stdin=subprocess.PIPE,
                                         stdout=subprocess.PIPE, stderr=errors, text=True, encoding="utf-8",
                                         env=dict(environment, AIRK_CLIENT="codex"))
                responses = queue.Queue()
                reader = threading.Thread(target=lambda: [responses.put(line) for line in child.stdout], daemon=True)
                reader.start()

                def rpc(identifier, method, params):
                    child.stdin.write(json.dumps({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params}) + "\n")
                    child.stdin.flush()
                    deadline = time.monotonic() + 30
                    while True:
                        response = json.loads(responses.get(timeout=clients.remaining(deadline, 30)))
                        if response.get("id") == identifier:
                            assert "error" not in response, response
                            return response["result"]

                try:
                    rpc(1, "initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                          "clientInfo": {"name": "codex", "version": "airk-native-fixture-1"}})
                    child.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
                    child.stdin.flush()
                    arguments = {"path": "fixture.txt", "start_line": 1, "end_line": 1, "max_bytes": 200, "response_mode": "full"}
                    reply = rpc(2, "tools/call", {"name": "airk_read_file", "arguments": arguments})
                    assert not reply.get("isError"), reply
                    read = json.loads(reply["content"][0]["text"])
                    reply = rpc(3, "tools/call", {"name": "airk_read_file", "arguments": {**arguments, "if_evidence": read["read"]["validator"]}})
                    assert not reply.get("isError"), reply
                    assert json.loads(reply["content"][0]["text"])["read"]["status"] == "not_modified"
                    reply = rpc(4, "tools/call", {"name": "airk_search", "arguments": {"query": "fixture", "mode": "lexical"}})
                    assert not reply.get("isError"), reply
                finally:
                    child.stdin.close()
                    try:
                        child.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait(timeout=10)
                    reader.join(timeout=5)
                    child.stdout.close()
            native_calls += 3
            mcp_report = run("usage", "report")["report"]
            assert mcp_report["calls"] == native_calls
            assert mcp_report["eligible_evidence_calls"] == 43
            assert mcp_report["comparisons"]["full_returned_sources"]["compared_calls"] == 43
            assert mcp_report["missing_baseline_calls"] == 0
            assert mcp_report["clients"]["codex"] >= 3
        measurement = write_json("measurement.json", {
            "schema_version": 1, "event_id": uuid.uuid4().hex,
            "unix_ms": int(time.time() * 1000), "client": "codex", "content_kind": "code",
            "provenance": "client_reported",
            "context": {
                "baseline_kind": "observed_alternative", "generation": indexed["generation"],
                "representation": "logical_result_json", "baseline_utf8_bytes": 80,
                "returned_utf8_bytes": 400, "tokens": None,
            },
            "session": {
                "model_revision_sha256": "a" * 64, "requests": 2,
                "input_tokens": 120, "cached_input_tokens": None, "output_tokens": 12,
                "elapsed_ms": 1000, "tool_round_trips": None,
            },
            "paired_task": None,
            "airk_request_ids": [request["request_id"]],
        })
        preview = run("usage", "import", measurement, "--preview")
        assert preview["recorded"] is False
        assert run("usage", "report")["supplemental"] == []
        assert run("usage", "import", measurement)["recorded"] is True
        assert run("usage", "import", measurement)["duplicate"] is True
        tokenizer = write_json("tokenizer.json", {
            "schema": 1, "model_revision_sha256": "a" * 64,
            "merges": [["68", "65"], ["6865", "6c"], ["68656c", "6c"], ["68656c6c", "6f"]],
        })
        baseline = write_json("baseline.json", "hellohello")
        returned = write_json("returned.json", "hello")
        calibration = ["usage", "calibrate", "--tokenizer", tokenizer, "--baseline", baseline,
                       "--returned", returned, "--generation", indexed["generation"], "--content-kind", "code"]
        assert run(*calibration)["summary"]["token_reduction"] == "1"
        assert run(*calibration, "--record")["recording"]["recorded"] is True
        prices = write_json("prices.json", {
            "model": "fictional-model", "currency": "USD", "rate_date": "2026-10-14",
            "input_price_per_million": "2.50", "cached_input_treatment": "unknown",
            "output_price_per_million": "10.00",
            "cached_input_price_per_million": None, "context_assumes_uncached": True,
        })
        report = run("usage", "report", "--price-scenario", prices)
        assert report["report"]["calls"] == native_calls
        assert report["report"]["provider_token_usage_observed"] is False
        assert report["report"]["estimated_bill_savings"] is None
        assert report["health"]["supplemental_duplicate_observations"] == 1
        assert report["lifetime"]["calls"] == native_calls
        assert sum(s["linked_airk_requests"] for b in report["supplemental"] for s in b["summaries"]) == 1
        scenario = report["price_scenario"]
        observed = next(row for row in scenario["context_equivalents"]
                        if row.get("stratum", {}).get("provenance") == "client_reported")
        assert observed["nominal_input_cost_equivalent"] == "-0.000200000000"
        assert scenario["session_input_equivalents"][0]["unpriced_input_tokens_unknown_cache_split"] == "120"
        assert scenario["session_input_equivalents"][0]["uncached_input_equivalent"] is None
        assert scenario["session_input_equivalents"][0]["output_equivalent"] == "0.000120000000"
        ledger = state / "collections-1.4" / "usage-fixture" / "usage.sqlite3"
        assert b"hellohello" not in ledger.read_bytes()
        run("usage", "configure", "--mode", "disabled")
        before = hashlib.sha256(ledger.read_bytes()).digest()
        run("usage", "import", measurement, fails=True)
        run(*calibration, "--record", fails=True)
        assert run(*calibration)["recording"] is None
        assert hashlib.sha256(ledger.read_bytes()).digest() == before
        run("usage", "clear")
        assert run("usage", "report")["supplemental"] == []
        assert run("usage", "report")["lifetime"] is None
        assert (root / "fixture.txt").read_text(encoding="utf-8") == "Original bounded usage fixture.\n"
    print("Usage input CLI smoke passed: concurrent process accounting, separate imports, replay protection, exact local byte-BPE counts, signed price scenarios, unknown cache splits, disabled no-writes and clear.")


if __name__ == "__main__":
    main()
