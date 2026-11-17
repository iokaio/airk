# SPDX-License-Identifier: Apache-2.0
"""Synthetic-only suite under actual existing 256MiB policy; no publisher data."""
import hashlib
import importlib.util
import json
from pathlib import Path
import runpy
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
assert sys.flags.isolated
spec = importlib.util.spec_from_file_location('synthetic_budget_adapter', HERE / 'adapter.py')
subject = importlib.util.module_from_spec(spec)
spec.loader.exec_module(subject)
protocol = json.loads((HERE.with_name('a1-fresh-final-materializer-r4') / 'protocol.json').read_text())
entry = protocol['inputs']['audit_source']
raw = (ROOT / entry['path']).read_bytes()
assert hashlib.sha256(raw).hexdigest() == entry['sha256']
budget = subject.import_bytes('synthetic_256m_budget', ROOT / entry['path'], raw)
budget.JOB_STATE = budget.enforce_budget()
sys.argv = [str(HERE / 'test_adapter.py')]
try:
    runpy.run_path(sys.argv[0], run_name='__main__')
finally:
    print(json.dumps({'event': 'synthetic_suite_memory',
                      'hard_memory_limit_bytes': 268435456,
                      'peak_committed_process_bytes': budget.peak_memory(),
                      'current_committed_process_bytes': subject.current_committed_bytes(),
                      'publisher_metadata_raw_or_gold_opened': False}), flush=True)
