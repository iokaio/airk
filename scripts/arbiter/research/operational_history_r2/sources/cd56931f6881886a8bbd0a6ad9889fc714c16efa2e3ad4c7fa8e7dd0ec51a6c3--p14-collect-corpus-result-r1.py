# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
from pathlib import Path

root = Path('D:/code/Github/airk')
base = root / 'artifacts/v190'
owner = Path('C:/Users/tdotn/AppData/Local/Temp/airk-p14-real-corpora-r1')
report_path = owner / 'real-corpora-r1.json'
raw = report_path.read_bytes()
report = json.loads(raw)
spec = importlib.util.spec_from_file_location('corpus', base / 'p10-worktree/scripts/benchmark-real-corpora.py')
corpus = importlib.util.module_from_spec(spec)
spec.loader.exec_module(corpus)
observed = corpus.source_identity(root)
original = report['corpora'][0]['source']
assert observed == original, 'AIRK corpus changed during campaign'
seal = json.loads((base / 'p14-runtime-final-r2-seal.json').read_text())
for name, digest in seal['files'].items():
    assert hashlib.sha256((owner / 'runtime' / name).read_bytes()).hexdigest() == digest, name
with (base / 'p14-real-corpora-r1.report.json').open('xb') as output:
    output.write(raw)
receipt = {
    'completed': report['completed'], 'code': report.get('code'),
    'observation_outcome': report['observation_outcome'],
    'report_sha256': hashlib.sha256(raw).hexdigest(),
    'airk_source_before': original, 'airk_source_after': observed,
    'airk_source_identity_preserved': True,
    'runtime_all_sealed_files_unchanged': True,
    'cleanup_failed': report['cleanup_failed'],
    'limits': ['failed campaign remains failed', 'second corpus not reached',
               'session/reopen/cancellation samples not reached',
               'no excerpt selected', 'no speedup or full semantic readiness claim'],
}
with (base / 'p14-real-corpora-collection-r1.json').open('x') as output:
    json.dump(receipt, output, indent=2)
print(json.dumps(receipt))
