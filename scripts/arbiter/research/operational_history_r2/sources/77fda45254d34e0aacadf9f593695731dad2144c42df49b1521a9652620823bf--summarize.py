# SPDX-License-Identifier: Apache-2.0
import hashlib
import importlib.util
import json
import math
from pathlib import Path

out = Path(__file__).resolve().parent
root = out.parents[1]
spec = importlib.util.spec_from_file_location('bench', root / 'scripts/benchmark-real-corpora.py')
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)

def quantiles(values):
    values = sorted(values)
    return {'n': len(values), 'p50_ms': values[math.ceil(len(values) * .5) - 1] if values else None,
            'p95_ms': values[math.ceil(len(values) * .95) - 1] if values else None}

summary = {}
for version in ('1.7.6', '1.8.2'):
    path = out / f'corpus-{version}.json'
    if not path.exists():
        continue
    raw = path.read_bytes()
    report = json.loads(raw)
    row = summary[version] = {key: report.get(key) for key in (
        'completed', 'code', 'observation_outcome', 'cleanup_failed', 'cleanup_detail',
        'source_identity_preserved', 'runtime', 'environment', 'policy')}
    row['raw_report_sha256'] = hashlib.sha256(raw).hexdigest()
    row['corpora'] = []
    for corpus in report.get('corpora', []):
        initial = corpus.get('initial_index', {})
        c = {key: corpus.get(key) for key in ('alias', 'source', 'before_index', 'semantic_identity',
            'cancellation_after_healthy_reopen', 'unchanged_reconcile')}
        c['initial'] = {key: initial.get(key) for key in ('elapsed_ms', 'first_usable_ms',
            'published', 'job_state', 'status_observations')}
        c['initial']['probes'] = [{key: p.get(key) for key in ('kind', 'observed',
            'usable_nonempty', 'success_empty', 'typed_no_content', 'native_error',
            'transport_protocol_failure', 'budget_exhausted', 'first_usable_ms')}
            for p in initial.get('probe_summary', {}).get('probes', [])]
        c['cohorts'] = {}
        for cohort, sessions in [('running', corpus.get('sessions', [])),
                                 ('reopen', corpus.get('reopen', {}).get('sessions', []))]:
            operations = {'initialize': quantiles([s['initialize_ms'] for s in sessions
                if isinstance(s.get('initialize_ms'), (int, float))])}
            for name in ('first_status', 'warm_status'):
                observations = [s[name] for s in sessions if s.get(name)]
                operations[name] = {**quantiles([o['elapsed_ms'] for o in observations]),
                    'failures': sum(o.get('is_error', False) or o.get('index_state') != 'available'
                                    for o in observations), 'missing': 30 - len(observations)}
            for kind in ('inventory', 'lexical', 'excerpt', 'vector', 'hybrid'):
                observations = [o for s in sessions for o in s.get('reads', []) if o['kind'] == kind]
                usable = [o for o in observations if bench.probe_outcome(o) == 'usable_nonempty']
                operations[kind] = {**quantiles([o['elapsed_ms'] for o in usable]),
                    'observed': len(observations), 'nonusable': len(observations) - len(usable),
                    'missing': 30 - len(observations)}
            c['cohorts'][cohort] = operations
        row['corpora'].append(c)
(out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
