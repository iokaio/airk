# SPDX-License-Identifier: Apache-2.0
"""Exercise explicit content-free usage inputs through the built public CLI."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    binary = parser.parse_args().binary.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="airk-usage-inputs-") as temporary:
        base = Path(temporary)
        root = base / "sources"
        root.mkdir()
        (root / "fixture.txt").write_text("Original bounded usage fixture.\n", encoding="utf-8")
        state = base / "state"
        environment = dict(os.environ)
        environment.pop("AIRK_USAGE_MODE", None)

        def run(*command, fails=False):
            result = subprocess.run(
                [str(binary), "--root", str(root), "--data-dir", str(state),
                 "--collection", "usage-fixture", *map(str, command)],
                capture_output=True, timeout=120, env=environment,
            )
            if fails:
                assert result.returncode != 0, result.stdout
                return None
            assert result.returncode == 0, result.stderr.decode(errors="replace")
            return json.loads(result.stdout)

        def write_json(name, value):
            path = base / name
            path.write_text(json.dumps(value), encoding="utf-8")
            return path

        indexed = run("index")
        request = run("search", "fixture")
        native_calls = run("usage", "report")["report"]["calls"]
        with ThreadPoolExecutor(max_workers=4) as workers:
            replies = list(workers.map(lambda _: run("search", "fixture"), range(40)))
        assert all(reply["hits"] for reply in replies)
        assert len({reply["request_id"] for reply in replies}) == 40
        native_calls += 40
        concurrent = run("usage", "report")
        assert concurrent["report"]["calls"] == native_calls, "concurrent CLI observations were lost"
        assert concurrent["lifetime"]["calls"] == native_calls
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
        ledger = state / "collections" / "usage-fixture" / "usage.sqlite3"
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
    print("Usage input CLI smoke passed: concurrent process accounting, separate imports, replay protection, exact local byte-BPE counts, signed price scenarios, unknown cache splits, disabled no-writes and clear.")


if __name__ == "__main__":
    main()
