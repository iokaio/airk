# SPDX-License-Identifier: Apache-2.0
"""Retain reviewed script fixture evidence; no signing, Cargo or installation."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(__file__).resolve().parent
previous = json.loads((OUT / "tooling-fixtures-r1.json").read_bytes())
command = [sys.executable, *previous["command"][1:-1], "scripts.tests.test_split_receipts", "-v"]
paths = [Path(name) for name in previous["source_pins"]]
paths += [ROOT / "scripts/tests/test_split_receipts.py", ROOT / "scripts/signing-identity.json",
          ROOT / "docs/native-signing.md"]
def pins():
    return {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}
before = pins()
started = time.monotonic()
result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace")
elapsed = time.monotonic() - started
(OUT / "tooling-fixtures-r2.log").write_text(result.stdout + result.stderr, encoding="utf-8")
after = pins()
receipt = {"schema": previous["schema"], "command": command, "exit_code": result.returncode,
           "elapsed_seconds": elapsed, "source_pins": before,
           "drift": [name for name in before if before[name] != after[name]],
           "proof": previous["proof"], "prior_failure_receipt": "tooling-fixtures-r1.json"}
(OUT / "tooling-fixtures-r2.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"exit_code": result.returncode, "elapsed_seconds": elapsed, "drift": receipt["drift"]}))
print(result.stderr[-1600:])
raise SystemExit(result.returncode or bool(receipt["drift"]))
