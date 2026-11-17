# SPDX-License-Identifier: Apache-2.0
"""Summarize complete Neo observations without relabeling historical failures."""
from pathlib import Path
import hashlib
import json
import re

base = Path(__file__).resolve().parent
run = base / 'upgrade-1.8.9-full-final-node-r2'
read = lambda name: json.loads((run / name).read_text(encoding='utf-8'))
qualification, cleanup, reuse = read('qualification.json'), read('cleanup.json'), read('runtime-reuse.json')
assert qualification['status'] == 'passed' and qualification['error'] is None
assert all(step['exit_code'] == 0 for step in qualification['steps'])
assert cleanup['group_absent'] and cleanup['cargo_clean_exit_code'] == 0 and cleanup['target_removed']
assert reuse['original_compiled_source']['sha256'] == 'acc3d1df74d5ee575f59f3f247b5c585bbf81f5ac40b3001df8e02e86f5c6d99'
assert reuse['current_test_source']['sha256'] == 'f200722c2bf21be2c443384f7daf415667b4fafe5c52d5245dd9b69deba2f50a'
assert reuse['complete_artifact_seal_files'] == 678 and reuse['unchanged_native_inputs'] == 974

def node_counts(name):
    text = (run / name).read_text(encoding='utf-8')
    result = {key:int(re.search('ℹ ' + key + r' (\d+)', text)[1]) for key in ('tests','pass','fail','skipped','cancelled')}
    result['node_duration_ms'] = float(re.search(r'ℹ duration_ms ([\d.]+)', text)[1])
    return result, text

node, node_log = node_counts('extension-tests.log')
assert {key:node[key] for key in ('tests','pass','fail','skipped','cancelled')} == {'tests':863,'pass':849,'fail':0,'skipped':14,'cancelled':0}
helper, _ = node_counts('editor-helper-tests.log')
assert {key:helper[key] for key in ('tests','pass','fail','skipped','cancelled')} == {'tests':14,'pass':14,'fail':0,'skipped':0,'cancelled':0}
python_log = (run / 'python-tests.log').read_text(encoding='utf-8')
python_total = int(re.search(r'Ran (\d+) tests in', python_log)[1])
python_skips = [line for line in python_log.splitlines() if "... skipped '" in line]
assert python_total == 482 and len(python_skips) == 6 and 'OK (skipped=6)' in python_log
node_skips = [line for line in node_log.splitlines() if line.startswith('﹣')]
assert len(node_skips) == 14

counts = {'status':'passed_final_TS_and_tooling','node':node,'editor_helper':helper,
          'python':{'total':python_total,'pass':python_total-len(python_skips),'skip':len(python_skips),'fail':0},
          'steps':qualification['steps'],'node_skip_lines':node_skips,'python_skip_lines':python_skips,
          'node_skip_categories':{'Windows-only':12,'opt-in F09 campaign not enabled':1,'retained old-helper pair not provided':1},
          'source_and_original_runtime_verified_after_tests':True,'runtime_reuse':reuse,'cleanup':cleanup}
(run / 'suite-counts.json').write_text(json.dumps(counts,indent=2)+'\n',encoding='utf-8')
report_path = base / 'neo-validation-summary.json'
report = json.loads(report_path.read_text(encoding='utf-8'))
report['status'] = 'qualification_complete_with_historical_failures_and_scoped_corrections_retained'
report['final_TS_and_tooling'] = counts
report['final_snapshot'] = read('preparation-review.json')
report['native_skip_reason_inventory'] = str(base / 'upgrade-1.8.9-full-r5/skip-reasons.json')
report['corrective_skip_reason_inventory'] = str(base / 'upgrade-1.8.9-full-corrective-r1/skip-reasons.json')
report['historical_harness_issue'] = 'Superseded Node r1 staged whitespace command required unavailable historical Git child trees; standard command passed and final r2 standard command passed. Original staged failure preserved.'
report_path.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'node':node,'editor_helper':helper,'python':counts['python'],'cleanup':'passed','summary':str(report_path)}))
