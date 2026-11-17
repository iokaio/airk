# SPDX-License-Identifier: Apache-2.0
"""Private local reproduction: existing sources, isolated service and data only."""
import importlib.util
import json
from pathlib import Path
import sys
import time

repository = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("native_fixture", repository / "scripts/check-service-clients.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
owner = Path(__file__).parent / (sys.argv[1] if len(sys.argv) > 1 else "original-159")
owner.mkdir(exist_ok=False)
(owner / "data").mkdir()
source = Path(r"D:\code\Github\bookbuilder").resolve(strict=True)
runtime = repository / "artifacts/local/management-1.5.9-20261028/runtime-release"
started = time.monotonic()
with module.Fixture(runtime, owner, command_timeout=120, job_timeout=3600) as fixture:
    fixture.register("bookbuilder-repro", source, owner / "data")
    (owner / "registration.json").write_text(json.dumps(fixture.projects, indent=2), encoding="utf-8")
    print("Isolated service ready; original source is read-only.", flush=True)
    receipt = fixture.enqueue("bookbuilder-repro", "index")
    (owner / "request.json").write_text(json.dumps(receipt, indent=2), encoding="utf-8")
    deadline = time.monotonic() + 3600
    while time.monotonic() < deadline:
        job = fixture.rpc({"job": {"id": receipt["job"]["id"]}})["job"]
        page = fixture.rpc({"console": {"after": None, "revision": None, "limit": 32}})
        (owner / "latest-console.json").write_text(json.dumps(page, indent=2), encoding="utf-8")
        (owner / "job.json").write_text(json.dumps(job, indent=2), encoding="utf-8")
        print(json.dumps({"elapsed_s": round(time.monotonic()-started), "state": job["state"], "outcome": job.get("outcome")}), flush=True)
        if job["state"] in ("failed", "succeeded", "canceled"):
            break
        time.sleep(5)
    else:
        raise RuntimeError("Reproduction observation budget exhausted")
    logs = fixture.run("service", "logs", "--runtime-dir", fixture.control, "--all", "--format", "json", check=False)
    (owner / "service-logs.json").write_text(logs.stdout, encoding="utf-8")
    if job["state"] == "succeeded":
        status = fixture.published_status("bookbuilder-repro")
        (owner / "status.json").write_text(json.dumps(status, indent=2), encoding="utf-8")
        print(json.dumps({"files": status.get("files"), "chunks": status.get("chunks"), "publication": status.get("publication")}), flush=True)
print("Owned diagnostic service stopped.", flush=True)
