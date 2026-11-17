# SPDX-License-Identifier: Apache-2.0
"""Full seal verification only. This file never imports Torch or launches training."""
from pathlib import Path
import json
import sys
import time

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[3]
sys.path.insert(0, str(REPO / 'tools/retrieval-train'))
import training_execution

started = time.monotonic()
value = training_execution.verify_seal(HERE / 'training-execution-seal.json', sys.argv[1])
result = {'schema': 'airk.private-training-seal-verification/1', 'seal_sha256': sys.argv[1],
          'verified_files': len(value['files']), 'verified_bytes': sum(f['bytes'] for f in value['files']),
          'runtime_trees_closed': True, 'elapsed_seconds': time.monotonic() - started,
          'torch_not_imported': 'torch' not in sys.modules, 'training_launched': False}
(HERE / 'seal-verification.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
print(json.dumps(result))
