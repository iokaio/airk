# SPDX-License-Identifier: Apache-2.0
import hashlib
import json
import math
from pathlib import Path
import statistics

candidate_path = Path('artifacts/v190/p6d-source-fairness-release.json')
candidate = json.loads(candidate_path.read_text(encoding='utf-8'))
control = json.loads(Path('artifacts/v190/p6d-p6c-backlog-control.json').read_text(encoding='utf-8'))
assert candidate['status'] == control['status'] == 'passed'
assert control['mode'] == 'load-only-baseline' and control['source_priority_assertions'] is False
assert control['runtime_native_source'] == {'sha256': '28f6f23ab1860db27b21c6cef252525b5f7c668cd6e4aeb037049221fe55ef86', 'files': 789}
a, b = candidate['matched_load'], control['matched_load']
assert a['corpus_sha256'] == b['corpus_sha256'] == control['source_snapshot_sha256']
assert a['corpus_files'] == b['corpus_files'] == 387
assert b['contract']['candidate_receipt_sha256'] == hashlib.sha256(candidate_path.read_bytes()).hexdigest()
assert candidate['cleanup_failures'] == control['cleanup_failures'] == []
assert a['backlog_resources_before']['observation']['configuration'] == b['resources_before']['observation']['configuration']
metrics = {}
for name, cohort in [('candidate', a['backlog']), ('p6c_control', b['backlog'])]:
    assert cohort['mode'] == 'lexical' and cohort['verification'] == 'off'
    assert cohort['limit'] == 8 and cohort['max_bytes'] == 8192
    values, metadata = cohort['samples_ms'], cohort['read_metadata']
    assert len(values) == len(metadata) == 30
    for item in metadata:
        ready = item['semantic_readiness']
        assert ready['state'] in ['partial', 'pending'] and ready['completed_artifacts'] < ready['selected_artifacts'] == 387
    metrics[name] = {'median_ms': statistics.median(values), 'p95_ms': sorted(values)[math.ceil(.95 * len(values)) - 1],
                     'completed_artifacts_range': [min(x['semantic_readiness']['completed_artifacts'] for x in metadata), max(x['semantic_readiness']['completed_artifacts'] for x in metadata)]}
gates = []
for key, relative, floor in [('median_ms', .10, 25), ('p95_ms', .15, 50)]:
    expected = metrics['p6c_control'][key]
    bound = expected + max(expected * relative, floor)
    gates.append({'metric': key, 'candidate': metrics['candidate'][key], 'control': expected,
                  'bound': bound, 'passed': metrics['candidate'][key] <= bound})
result = {'passed': all(g['passed'] for g in gates), 'limits_changed': False,
          'samples_per_runtime': 30, 'corpus_sha256': a['corpus_sha256'], 'metrics': metrics, 'gates': gates}
Path('artifacts/v190/p6d-backlog-control-gates.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
print(json.dumps(result))
raise SystemExit(0 if result['passed'] else 1)
