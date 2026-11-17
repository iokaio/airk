# SPDX-License-Identifier: Apache-2.0
# Original temporary fixture smoke; does not target the real corpora.
from pathlib import Path
import json, subprocess, sys
meta=json.loads(Path(__file__).with_name('f01c-original-smoke.json').read_text())
command=[sys.executable, 'scripts/benchmark-real-corpora.py', '--runtime-dir', meta['runtime'], '--runtime-sha256', meta['runtime_sha256'], '--airk',meta['airk'],'--bookbuilder',meta['bookbuilder'],'--state-parent',meta['state_parent'],'--report',meta['report'],'--semantic','--excerpt-path','README.md','--samples','1']
raise SystemExit(subprocess.run(command).returncode)
