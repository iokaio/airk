# SPDX-License-Identifier: Apache-2.0
"""Wait boundedly for the original producer, then run the reviewed adoption queue."""
import json
from pathlib import Path
import subprocess
import sys
import time

base = Path(__file__).resolve().parent
receipt = base / 'p10-final-release-build-r1.receipt.json'
deadline = time.monotonic() + 1800
assert not (base / 'p10-final-qualification-r3.json').exists()
print('Waiting for the original release producer receipt; no build is launched here.', flush=True)
while not receipt.exists():
    if time.monotonic() >= deadline:
        raise TimeoutError('Original release producer receipt unavailable after 30 minutes')
    time.sleep(5)
observed = json.loads(receipt.read_text(encoding='utf-8'))
assert observed['actual_exit_code'] == observed['exit_code'] == 0, observed
assert observed['source_unchanged'] and observed['source_before'] == observed['source_after']
assert observed['source_after'] == {'sha256': 'f7d6a19ddfbe939b8085c57b5aee7c62ae29a3a34e1e73c50625f272d3a7e60d', 'files': 945}
assert not (base / 'p10-final-qualification-owner.lock').exists()
print('Original producer succeeded; adopting its sealed runtime without rebuilding.', flush=True)
sys.exit(subprocess.run([sys.executable, str(base / 'p10-final-queue-r3.py')], cwd=base.parents[1]).returncode)
