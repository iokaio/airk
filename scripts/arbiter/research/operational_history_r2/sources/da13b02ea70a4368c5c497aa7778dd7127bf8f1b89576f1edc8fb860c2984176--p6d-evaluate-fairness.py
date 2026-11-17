# SPDX-License-Identifier: Apache-2.0
import argparse
import json
import math
from pathlib import Path
import statistics

parser = argparse.ArgumentParser()
parser.add_argument('--receipt', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
data = json.loads(args.receipt.read_text(encoding='utf-8'))
assert data['status'] == 'passed'
assert data['source_bytes_preserved'] is True and data['cleanup_failures'] == []
assert len(data['waves']) == 2
gates = []
for wave in data['waves']:
    assert wave['order']['new_vector_overtakers'] == 0
    gates.append({'gate': wave['label'] + '_source_overhead', 'observed_seconds': wave['source_overhead_seconds'],
                  'bound_seconds': 10, 'passed': wave['source_overhead_seconds'] <= 10})
matched = data['matched_load']
assert matched['source_bytes_unchanged'] is True
metrics = {}
for mode in ['backlog', 'quiet']:
    values = matched[mode]['samples_ms']
    observations = matched[mode]['read_metadata']
    assert len(values) == len(observations) == 30
    for item in observations:
        ready = item['semantic_readiness']
        assert (ready['state'] == 'ready') if mode == 'quiet' else (
            ready['state'] in ['pending', 'partial'] and ready['completed_artifacts'] < ready['selected_artifacts'])
    metrics[mode] = {'median_ms': statistics.median(values), 'p95_ms': sorted(values)[math.ceil(.95 * len(values)) - 1]}
result = {'passed': all(item['passed'] for item in gates), 'limits_changed': False,
          'corpus_sha256': matched['corpus_sha256'], 'samples_per_mode': 30,
          'metrics': {'backlog': metrics['backlog'], 'semantic_ready_stage': metrics['quiet']},
          'interpretation': 'Candidate load-effect observations only. Ready-stage cohort is not proven idle; matched prior-phase backlog regression comparison remains separate.',
          'gates': gates}
args.output.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
print(json.dumps(result))
raise SystemExit(0 if result['passed'] else 1)
