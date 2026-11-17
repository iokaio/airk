# SPDX-License-Identifier: Apache-2.0
"""Owned, bounded native helper lifecycle checks; not installed qualification."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

repo = Path(__file__).resolve().parents[2]
runtime = repo / "artifacts/v190/p6f-parity-runtime-r1"
binary = repo / "artifacts/v190/p6f-check-target/debug/deps/airk_cli-eec3e8c1ba56441a.exe"
environment = os.environ.copy()
environment["AIRK_GUARDED_PAIR_MAIN"] = str(runtime / "airk.exe")
prefix = "embedding_pool_process::shared::tests::native::"
cases = [
    prefix + "guarded_real_pair_starts_and_records_actual_native_completion",
    prefix + "abrupt_service_exit_recovers_real_guarded_helper_before_predecessor_proof",
]
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
report = {"schema": 1, "scope": "actual Windows guarded helper lifecycle component",
          "installed_qualified": False, "model_work_qualified": False,
          "runtime_manifest_sha256": sha(runtime / "airk-runtime.json"),
          "binary_sha256": sha(binary), "results": []}
for n, case in enumerate(cases, 1):
    listed = subprocess.run([str(binary), "--list", case], env=environment,
                            capture_output=True, text=True, timeout=15, check=True)
    assert listed.stdout.count(": test") == 1 and case + ": test" in listed.stdout
    log = repo / f"artifacts/v190/p6f-windows-guarded-pair-case-{n}.log"
    with log.open("w", encoding="utf-8") as output:
        try:
            result = subprocess.run([str(binary), "--exact", case, "--ignored", "--nocapture", "--test-threads=1"],
                                    env=environment, stdout=output, stderr=subprocess.STDOUT, timeout=180)
            status = {"case": case, "returncode": result.returncode}
        except subprocess.TimeoutExpired:
            status = {"case": case, "timeout": True}
    status.update(log=log.name, log_sha256=sha(log))
    report["results"].append(status)
report["passed"] = all(item.get("returncode") == 0 for item in report["results"])
target = repo / "artifacts/v190/p6f-windows-guarded-pair-report.json"
target.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
print(json.dumps(report))
sys.exit(0 if report["passed"] else 1)
