# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

root = Path(__file__).resolve().parents[3]
base = Path(__file__).resolve().parent
prior = json.loads((base / "reconcile-debug-client-load-r1.json").read_text())
arguments = prior["arguments"].copy()
arguments[arguments.index("--output") + 1] = "artifacts/local/one-airk-1.8.14/reconcile-debug-client-load-result-r2.json"
runtime = root / arguments[arguments.index("--runtime-dir") + 1]
inventory = json.loads((runtime / "airk-install.json").read_text())
sha = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()


def verify():
    for name, expected in prior["source_files"].items():
        if sha(root / name) != expected:
            raise RuntimeError("Recorded native source input changed: " + name)
    for name, expected in inventory["files"].items():
        if sha(runtime / name) != expected:
            raise RuntimeError("Recorded native payload changed: " + name)


verify()
private = json.loads((base / "user-private-fixture-base.json").read_text())["path"]
environment = dict(os.environ, TEMP=private, TMP=private, AIRK_PRIVATE_CENSUS_BASE=private)
started = time.monotonic()
with (base / "reconcile-debug-client-load-r2.log").open("xb") as log:
    result = subprocess.run(arguments, cwd=root, env=environment, stdout=log, stderr=subprocess.STDOUT)
row = {"arguments": arguments, "exit_code": result.returncode, "elapsed_seconds": time.monotonic() - started,
       "native_reuse": "original sealed Windows debug r24; no compilation", "runtime_manifest_sha256": sha(runtime / "airk-install.json"),
       "sealed_files_checked": len(inventory["files"]), "native_source_files_checked": len(prior["source_files"]),
       "harness_sources": {name: sha(root / name) for name in ["scripts/benchmark-service.py", "scripts/http_fixture.py", "scripts/tests/test_benchmark_service_http.py"]}}
try:
    verify()
    row["native_source_and_payload_unchanged_after_run"] = True
finally:
    with (base / "reconcile-debug-client-load-r2.json").open("x") as output:
        json.dump(row, output, indent=2)
        output.write("\n")
print(json.dumps(row))
raise SystemExit(result.returncode)
