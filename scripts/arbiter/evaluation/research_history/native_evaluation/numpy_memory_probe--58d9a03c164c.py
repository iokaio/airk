# SPDX-License-Identifier: Apache-2.0
"""No scoring: reproduce NumPy/bootstrap memory after six DEV captures."""
from pathlib import Path
import importlib.util
import json
import os
import time
ROOT=Path(__file__).resolve().parents[4]
helper=ROOT/'artifacts/advanced-split/models/a1-next-experiment-blind-metadata-r5/audit.py'
spec=importlib.util.spec_from_file_location('bounded',helper);bounded=importlib.util.module_from_spec(spec);spec.loader.exec_module(bounded)
bounded.START=time.monotonic();bounded.JOB_STATE=bounded.enforce_budget()
paths=sorted((ROOT/'artifacts/advanced-split/models/native-dev64-baseline-r10-r1').glob('*/capture.json'))
captures=[json.loads(p.read_bytes()) for p in paths]
before=bounded.peak_memory()
import numpy as np
print(json.dumps({'captures':len(captures),'records':sum(len(c['records']) for c in captures),'before_numpy_peak':before,'after_numpy_peak':bounded.peak_memory(),'numpy_version':np.__version__,'environment':{k:os.environ.get(k) for k in ('OPENBLAS_NUM_THREADS','OMP_NUM_THREADS','MKL_NUM_THREADS')},'no_scoring':True}),flush=True)
