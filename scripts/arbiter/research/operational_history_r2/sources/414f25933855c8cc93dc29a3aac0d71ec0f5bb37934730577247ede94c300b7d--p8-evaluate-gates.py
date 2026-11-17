# SPDX-License-Identifier: Apache-2.0
"""Compare P8 to the unchanged frozen bounds and preceding P7 checkpoint."""
import importlib.util,json,hashlib
from pathlib import Path
base=Path('artifacts/v190')
spec=importlib.util.spec_from_file_location('gates',base/'p5-evaluate-gates.py');owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)
off,off_cohorts,off_counts=owner.measurement('p8-off-mcp','off')
report,report_cohorts,report_counts=owner.measurement('p8-report-mcp','report')
frozen=owner.read('docs/research/baseline-v190.json');gates=[]
gates+=owner.compare(off_cohorts,{c['connection']:c for c in frozen['mcp_cohorts'] if c['concurrency']==1},'frozen_archived')
for mode,candidate in [('off',off_cohorts),('report',report_cohorts)]:
 for label,name in [('p5_archived',f'p5-188-{mode}-mcp'),('p5_recorded_control',f'p6f-final-control-{mode}-mcp'),('preceding_p7',f'p7-r8-{mode}-mcp')]:
  prior=owner.read(base/name/'replay/report.json')
  gates+=owner.compare(candidate,{c['connection']:c for c in prior['cohorts']},label+'_'+mode)
assert len(gates)==84
result={'schema':'airk.legacy-phase-gates/1','phase':'P8','passed':all(g['passed'] for g in gates),'limits_changed':False,'baseline_sha256':hashlib.sha256(Path('docs/research/baseline-v190.json').read_bytes()).hexdigest(),'limits':'median +max(10%,25ms); p95 +max(15%,50ms)','observations':{'off':off_counts,'report':report_counts},'gates':gates}
(base/'p8-gates.json').write_text(json.dumps(result,indent=2))
print(json.dumps({'passed':result['passed'],'passed_gates':sum(g['passed'] for g in gates),'total_gates':len(gates)}))
raise SystemExit(0 if result['passed'] else 1)
