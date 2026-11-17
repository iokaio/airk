# SPDX-License-Identifier: Apache-2.0
"""Record the primary conditional non-adoption decision; no source mutation."""
import hashlib
import importlib.util
import json
from pathlib import Path

base = Path(__file__).resolve().parent
work = base / 'p10-worktree'
def read(path):
    return json.loads(path.read_text(encoding='utf-8'))
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
acceptance = read(base / 'p12-local-acceptance-commit-r1.json')
assert acceptance['phase'] == 'P12' and acceptance['status'] == 'accepted'
assert acceptance['dco_verified'] and acceptance['root_clean']
lexical = work / 'crates/airk-store/src/current/lexical.rs'
assert sha(lexical) == 'e8f50e0275e31de2e9422d415ef3e51e397b27160feae52ca5b7c19eda325eed'
spec = importlib.util.spec_from_file_location('p13_decision_builder', work / 'scripts/build-service-runtime.py')
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)
assert builder.source_identity() == acceptance['source']
recommendation = read(base / 'p13-phase-closure-recommendation-r1.json')
record = {'phase': 'P13', 'decision': 'retain_current', 'production_changed': False,
          'source': acceptance['source'], 'p12_commit': acceptance['commit'],
          'p13a': 'Do not adopt the inert path-affinity lexical candidate.',
          'p13b': 'Do not enable an unqualified diversity change.',
          'reason': 'Required matched quality/resource and complete native disclosure/guard evidence is unqualified. The approved conditional path retains current behavior; this is not a measured quality failure or a superiority claim.',
          'candidate_applied': False, 'candidate_executed': False,
          'unmet_gates': recommendation['gates_not_met'],
          'review': {'recommendation_sha256': sha(base / 'p13-phase-closure-recommendation-r1.json'),
                     'source_label_spot_check_sha256': sha(base / 'p13-independent-label-spot-check-r1.json'),
                     'current_lexical_sha256': sha(lexical)},
          'remaining': 'Mandatory P14 qualification, deleted-source cleanup, final local test installer and last PR60 assessment remain required.'}
with (base / 'p13-decision-r1.json').open('x', encoding='utf-8') as output:
    output.write(json.dumps(record, indent=2) + '\n')
print(json.dumps({'phase': 'P13', 'decision': record['decision'], 'production_changed': False}))
