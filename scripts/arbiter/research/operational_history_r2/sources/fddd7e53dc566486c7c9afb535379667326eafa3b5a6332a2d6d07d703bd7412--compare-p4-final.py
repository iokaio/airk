# SPDX-License-Identifier: Apache-2.0
import json
from pathlib import Path
root=Path('artifacts/v190')
base=json.loads(Path('docs/research/baseline-v190.json').read_text())
candidate=json.loads((root/'p4-final-off-mcp/replay/report.json').read_text())
refs={
 'frozen_archived':base['mcp_cohorts'],
 'preceding_archived':json.loads((root/'p2b-qualified-mcp/replay/report.json').read_text())['cohorts'],
 'frozen_runtime_current':json.loads((root/'p4-diagnostic-frozen-mcp/replay/report.json').read_text())['cohorts'],
 'preceding_runtime_current':json.loads((root/'p4-diagnostic-p2b-mcp/replay/report.json').read_text())['cohorts'],
}
gates=[]
for name, cohorts in refs.items():
 for observed in candidate['cohorts']:
  reference=next(c for c in cohorts if (c['connection'],c['concurrency'])==(observed['connection'],observed['concurrency']))
  for metric in ['task_ms','task_with_connection_ms']:
   for quantile in ['median','p95']:
    v=observed[metric][quantile]; r=reference[metric][quantile]
    allowance=max(base['frozen_gates'][quantile+'_allowance_ms'],r*base['frozen_gates'][quantile+'_allowance_relative'])
    gates.append(dict(reference=name,connection=observed['connection'],metric=metric,quantile=quantile,observed=v,bound=r+allowance,passed=v<=r+allowance))
report={'reference_rule':'Archived and remeasured exact immutable payloads retained separately; unchanged frozen allowances','historical_all_passed':all(g['passed'] for g in gates if g['reference'].endswith('_archived')),'matched_control_all_passed':all(g['passed'] for g in gates if g['reference'].endswith('_current')),'gates':gates}
(root/'p4-final-gates.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k!='gates'}))
for g in gates:
 if not g['passed']: print(json.dumps(g))
