# SPDX-License-Identifier: Apache-2.0
"""One owner-schema drift observation; expected failure retains native capture."""
import os
from pathlib import Path
import subprocess
import sys
root = Path.cwd()
env = {key: value for key, value in os.environ.items() if not key.upper().startswith('GIT_')}
env.update(CARGO_BUILD_JOBS='8', CARGO_INCREMENTAL='1', CARGO_NET_OFFLINE='true')
capture = root / 'artifacts/advanced-split/receipts/native-context-r10/native-schema'
capture.mkdir()
env.update(TEMP=str(capture), TMP=str(capture))
command = [sys.executable, 'scripts/dev/receipt_runner.py', '--groups', 'native,harness,fixtures',
           '--out', 'artifacts/advanced-split/receipts/native-context-r10', '--target-dir',
           'artifacts/v190/p10-target', 'schema-drift-observation', 'C:/Users/tdotn/.cargo/bin/cargo.exe',
           'nextest', 'run', '--profile', 'ci', '-p', 'airk-core', '--lib', '--all-features',
           '--locked', '--timings', '-E', 'test(arbiter::schema::tests::schema_matches_reviewed_capture)']
raise SystemExit(subprocess.run(command, cwd=root, env=env).returncode)

