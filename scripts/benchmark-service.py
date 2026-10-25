# SPDX-License-Identifier: Apache-2.0
"""Measure native project/client load in owned scopes, including explicit overload."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from contextlib import ExitStack
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import tempfile
import time


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


clients = module("airk_load_clients", "check-service-clients.py")
retrieval = module("airk_load_sessions", "check-retrieval-quality.py")


def latency(samples):
    ordered = sorted(samples)
    return {"samples": len(ordered), "p95_ms": ordered[math.ceil(len(ordered) * .95) - 1],
            "maximum_ms": ordered[-1]} if ordered else {"samples": 0}


def campaign(runtime, project_count, client_count, rounds):
    with tempfile.TemporaryDirectory(prefix="airk-service-load-") as temporary, ExitStack() as stack:
        owner = Path(temporary).resolve(strict=True)
        fixture = stack.enter_context(clients.Fixture(runtime, owner, job_timeout=600))
        data = owner / "data"
        data.mkdir()
        originals = {}
        source_hashes = {}
        names = [f"load-{number}" for number in range(project_count)]
        for number, name in enumerate(names):
            root = owner / name
            root.mkdir()
            for index in range(16):
                path = root / f"file-{index}.txt"
                path.write_bytes((f"projectmarker{number} Original bounded load fixture {index}.\n" * 32).encode())
                originals[path] = path.read_bytes()
                source_hashes[path] = {hashlib.sha256(originals[path]).hexdigest()}
            fixture.register(name, root, data)
            fixture.rpc({"configure_watch": {"project": fixture.projects[name]["binding"], "enabled": False}})
            fixture.configure(name, {"operation": "vectors", "enabled": False})
            fixture.configure(name, {"operation": "usage", "mode": "disabled"})
            fixture.job(name, "index")
        before_idle = fixture.rpc("resources")["observation"]["resident_workers"]
        before_pool = fixture.rpc("embedding_status")
        sessions = []
        for index in range(client_count):
            name = names[index % project_count]
            session = retrieval.Session([str(fixture.binary), *fixture.arguments(name)])
            stack.callback(session.close)
            sessions.append((session, index % project_count))
        after_idle = fixture.rpc("resources")["observation"]["resident_workers"]
        after_pool = fixture.rpc("embedding_status")
        assert after_idle <= before_idle, (before_idle, after_idle)
        assert after_pool.get("loads", 0) == before_pool.get("loads", 0) == 0

        def query(selected):
            session, number = selected
            started = time.perf_counter()
            envelope = session.request("tools/call", {"name": "airk_search", "arguments": {
                "query": f"projectmarker{number}", "mode": "lexical", "limit": 1,
                "max_bytes": 4096, "response_mode": "full"}})
            elapsed = (time.perf_counter() - started) * 1000
            if envelope.get("isError"):
                assert clients.read_retry_delay(envelope) is not None, envelope
                return {"rejected_ms": elapsed}
            value = clients.tool_value(envelope)
            assert value["collection"] == names[number] and value["hits"], value
            hit = value["hits"][0]
            source = owner / names[number] / hit["citation"]["path"]
            assert hit["citation"]["content_hash"] in source_hashes[source]
            assert f"projectmarker{number}" in hit["text"]
            return {"success_ms": elapsed}

        def wave(pool):
            replies, health = [], []
            for _ in range(rounds):
                futures = [pool.submit(query, selected) for selected in sessions]
                started = time.perf_counter()
                assert fixture.rpc("health")["status"] == "health"
                health.append((time.perf_counter() - started) * 1000)
                replies.extend(future.result() for future in futures)
            return {"queries": latency([reply["success_ms"] for reply in replies if "success_ms" in reply]),
                    "rejections": latency([reply["rejected_ms"] for reply in replies if "rejected_ms" in reply]),
                    "health_cli": latency(health)}

        with ThreadPoolExecutor(max_workers=client_count) as pool:
            idle = wave(pool)
            for path in originals:
                content = originals[path] + b"Updated original load text.\n" * 1024
                path.write_bytes(content)
                originals[path] = content
                source_hashes[path].add(hashlib.sha256(content).hexdigest())
            accepted = [fixture.enqueue(name, "index") for name in names]
            during = wave(pool)
            observed = [fixture.rpc({"job": {"id": receipt["job"]["id"]}})["job"]["state"] for receipt in accepted]
            for receipt in accepted:
                fixture.wait(receipt)
            settled = wave(pool)
            assert settled["queries"]["samples"] > 0, settled
        assert all(path.read_bytes() == content for path, content in originals.items())
        return {"projects": project_count, "clients": client_count, "rounds": rounds,
                "idle_connected_workers_before": before_idle, "idle_connected_workers_after": after_idle,
                "model_loads": after_pool.get("loads", 0), "idle": idle, "during": during,
                "source_job_states_after_wave": observed, "settled": settled,
                "source_bytes_preserved": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=5)
    args = parser.parse_args()
    if not 3 <= args.rounds <= 30:
        parser.error("rounds must be 3..30")
    runtime = clients.runtime_root(args.runtime_dir)
    report = {"schema": "airk-service-load/1", "platform": platform.platform(),
              "logical_cpus": os.cpu_count(), "machine": platform.machine(),
              "runtime_manifest_sha256": hashlib.sha256((runtime / "airk-install.json").read_bytes()).hexdigest(),
              "limitations": "Single host; lexical original fixtures; uncontrolled OS caches; health includes CLI startup; explicit read rejections counted separately; during-wave jobs may finish before all samples.",
              "cases": [], "passed": False}
    try:
        for projects in (1, 4, 10):
            for count in (1, 10, 50):
                case = campaign(runtime, projects, count, args.rounds)
                report["cases"].append(case)
                print(json.dumps({"projects": projects, "clients": count, "completed": True}), flush=True)
        report["passed"] = True
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
