# SPDX-License-Identifier: Apache-2.0
"""Check shared admission, bounded watching and MCP isolation in four private projects."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import time


spec = importlib.util.spec_from_file_location("service_clients", Path(__file__).with_name("check-service-clients.py"))
clients = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clients)


def qualify(runtime, files):
    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix="airk-four-projects-") as temporary:
        owner = Path(temporary).resolve()
        (owner / "data").mkdir()
        with clients.Fixture(runtime, owner) as fixture:
            fixture.job_timeout = 600
            names = [f"project-{number}" for number in range(4)]
            markers = ["auroraprivatemarker", "birchprivatemarker", "cedarprivatemarker", "deltaprivatemarker"]
            originals = {}
            for number, name in enumerate(names):
                source = owner / name
                source.mkdir()
                for directory in range(600):
                    (source / f"directory-{directory}").mkdir()
                for index in range(files):
                    file = source / f"directory-{index % 600}" / f"fixture-{index}.py"
                    file.write_text(f"# polaronlymarker {markers[number]}\ndef project_{number}_fixture_{index}():\n    return {number}\n", encoding="utf-8")
                    originals[file] = hashlib.sha256(file.read_bytes()).hexdigest()
                fixture.register(name, source, owner / "data")
            before = fixture.rpc("health")
            receipts = [fixture.enqueue(name, "index") for name in names]
            samples = []
            deadline = time.monotonic() + 600
            while True:
                sample = fixture.rpc("resources", deadline=deadline)["observation"]
                samples.append(sample)
                assert sample["memory_reserved_bytes"] <= sample["memory_limit_bytes"], sample
                assert sample["background_io_reserved_bytes_per_second"] <= 256 << 20, sample
                states = [fixture.rpc({"job": {"id": receipt["job"]["id"]}}, deadline=deadline)["job"] for receipt in receipts]
                assert not any(job["state"] in ("failed", "canceled") for job in states), states
                if all(job["state"] == "succeeded" for job in states):
                    break
                assert time.monotonic() < deadline, states
                time.sleep(0.2)
            with ThreadPoolExecutor(max_workers=4) as pool:
                # Every simultaneous session must receive all sixteen tools.
                list(pool.map(fixture.stdio, names))
            statuses = [fixture.published_status(name, deadline) for name in names]
            for number, (name, status) in enumerate(zip(names, statuses)):
                assert status["files"] == files and status["collection"] == name, status
                foreign = fixture.tool(name, "airk_search", {"query": markers[(number + 1) % 4], "mode": "lexical"})
                assert not foreign["hits"], foreign
                fixture.rpc({"configure_watch": {"project": fixture.projects[name]["binding"], "enabled": True}})
            deadline = time.monotonic() + 120
            while True:
                statuses = [fixture.published_status(name, deadline) for name in names]
                watches = [status["work"]["watch"] for status in statuses]
                if all(watch["backend"] == "native" and watch["native_roots"] == 1 and watch["failure"] is None for watch in watches):
                    break
                assert time.monotonic() < deadline, watches
                time.sleep(0.5)
            after = fixture.rpc("health")
            assert before["observation"]["instance"] == after["observation"]["instance"], (before, after)
            assert all(hashlib.sha256(file.read_bytes()).hexdigest() == expected for file, expected in originals.items())
            return {"schema": 1, "version": json.loads((runtime / "airk-runtime.json").read_text())["version"],
                    "projects": 4, "files_per_project": files, "directories_per_project": 601,
                    "mcp_tools_per_session": len(clients.TOOLS), "same_service_instance": True,
                    "collection_isolation": True, "native_watch_roots": [watch["native_roots"] for watch in watches],
                    "peak_running_jobs_observed": max(sample["running_jobs"] for sample in samples),
                    "peak_reserved_bytes": max(sample["memory_reserved_bytes"] for sample in samples),
                    "resource_configuration": samples[0]["configuration"], "source_bytes_preserved": True,
                    "elapsed_seconds": round(time.monotonic() - started, 3)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--runtime-dir", type=Path, required=True)
    parser.add_argument("--files", type=int, default=64)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if not 1 <= args.files <= 2048:
        parser.error("files must be 1..2048")
    report = qualify(args.runtime_dir.resolve(strict=True), args.files)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
