# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
from pathlib import Path

root = Path.cwd()
out = root / 'artifacts/performance-1.8.2-20261101'
spec = importlib.util.spec_from_file_location('bench', root / 'scripts/benchmark-real-corpora.py')
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)
reports = {v: json.loads((out / f'corpus-{v}.json').read_bytes()) for v in ('1.7.6', '1.8.2')}
result = {'raw_reports_preserved': True, 'corpora': []}
for alias, path in [('airk', root), ('bookbuilder', Path('D:/code/Github/bookbuilder'))]:
    before = [next(c['source'] for c in r['corpora'] if c['alias'] == alias) for r in reports.values()]
    after = bench.source_identity(path)
    result['corpora'].append({'alias': alias, 'before': before[0], 'after': after,
        'both_runtime_inputs_match': before[0] == before[1], 'unchanged': all(b == after for b in before)})
assert all(c['both_runtime_inputs_match'] and c['unchanged'] for c in result['corpora'])
result['source_identity_preserved'] = True
(out / 'source-verification.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
