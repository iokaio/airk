# SPDX-License-Identifier: Apache-2.0
"""Pinned Arrow synthetic-only import/projection under the unchanged 256MiB cap."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import threading

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
spec = importlib.util.spec_from_file_location('synthetic_memory_subject', HERE / 'adapter.py')
subject = importlib.util.module_from_spec(spec)
spec.loader.exec_module(subject)
protocol = json.loads((HERE.with_name('a1-fresh-final-materializer-r3') / 'protocol.json').read_text())
entry = protocol['inputs']['audit_source']
raw = (ROOT / entry['path']).read_bytes()
assert hashlib.sha256(raw).hexdigest() == entry['sha256']
budget = subject.import_bytes('synthetic_only_budget', ROOT / entry['path'], raw)
budget.JOB_STATE = budget.enforce_budget()
subject.MEMORY_OBSERVERS = (budget.peak_memory, None)
def expire():
    if not budget.done.wait(30):
        os._exit(124)
threading.Thread(target=expire, daemon=True).start()
report = {'schema': 'airk.private-synthetic-arrow-memory-control/1',
          'no_actual_metadata_raw_questions_evidence_or_gold_opened': True,
          'hard_memory_limit_bytes': 268435456, 'hard_wall_seconds': 30}
try:
    subject.STAGE = 'synthetic_arrow_import'
    report['before_arrow'] = subject.safe_memory_counters()
    import pyarrow as arrow
    import pyarrow.dataset
    subject.bind_arrow_diagnostics(arrow)
    subject.MEMORY_OBSERVERS = (budget.peak_memory, arrow.total_allocated_bytes)
    arrow.set_cpu_count(1)
    arrow.set_io_thread_count(1)
    report['after_arrow_dataset_import'] = subject.safe_memory_counters()
    subject.STAGE = 'synthetic_arrow_array'
    array = arrow.array(['SYNTHETIC ONLY'] * 32)
    report['after_array_creation'] = subject.safe_memory_counters()
    subject.STAGE = 'synthetic_arrow_to_pylist'
    assert array.to_pylist() == ['SYNTHETIC ONLY'] * 32
    report['after_to_pylist'] = subject.safe_memory_counters()
    report['complete'] = True
except BaseException as error:
    report['complete'] = False
    report['safe_failure'] = subject.safe_failure(error)
finally:
    budget.done.set()
with (HERE / 'synthetic-arrow-memory-control.json').open('x', encoding='utf-8', newline='\n') as stream:
    json.dump(report, stream, indent=2)
    stream.write('\n')
print(json.dumps(report))
sys.exit(0 if report['complete'] else 1)
