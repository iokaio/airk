# SPDX-License-Identifier: Apache-2.0
"""Serialize corrected-source qualification after both existing campaigns settle."""
import importlib.util, json, os, subprocess, sys, time
from pathlib import Path
root=Path(__file__).resolve().parents[2]
base=root/'artifacts/v190'
expected={'sha256':'689e2f5e844cba5cb533dd46755d36b0199ac5689f306cd3a5b28a66704badb2','files':922}
out=base/'p9-final-qualification-r1.json'
assert not out.exists()
record={'status':'waiting','source':expected,'steps':[]}
def save():
    pending=out.with_suffix('.pending')
    pending.write_text(json.dumps(record,indent=2)+'\n');pending.replace(out)
save()
deadline=time.monotonic()+7200
for name in ['p9-final-runtime-build-r2.receipt.json','p9-workspace-final-r2.receipt.json']:
    receipt=base/name
    while not receipt.exists():
        assert time.monotonic()<deadline,'Prior qualification did not settle within two hours'
        time.sleep(5)
    result=json.loads(receipt.read_text())
    assert result['exit_code']==0 and result['source_unchanged'] and result['source_after']==expected,result
spec=importlib.util.spec_from_file_location('p9_integrated',base/'p9-integrated-checks.py')
integrated=importlib.util.module_from_spec(spec);spec.loader.exec_module(integrated)
runtime=base/'p9-runtime-final-r2'
seal=integrated.seal_runtime(runtime,expected)
(base/'p9-runtime-final-r2-seal.json').write_text(json.dumps(seal,indent=2)+'\n')
inputs=json.loads((base/'p9-clean-inputs.local.json').read_text(encoding='utf-8-sig'))['inputs']
steps=[
 ('fmt',['cargo','fmt','--all','--check']),
 ('clippy',['cargo','clippy','--workspace','--all-targets','--all-features','--locked','--','-D','warnings']),
 ('clients',[sys.executable,'scripts/check-service-clients.py','--runtime-dir',str(runtime),'--files','16','--model','--report',str(base/'p9-final-clients-r1.json')]),
 ('publication-faults',[sys.executable,'scripts/check-publication-faults.py','--binary',str(runtime/'airk.exe'),'--attempts','6','--output',str(base/'p9-final-publication-faults-r1.json')]),
 ('off',[sys.executable,str(base/'p9-frozen-replay.py'),'--runtime-dir',str(runtime),'--verification','off','--output',str(base/'p9-final-off-mcp')]),
 ('report',[sys.executable,str(base/'p9-frozen-replay.py'),'--runtime-dir',str(runtime),'--verification','report','--output',str(base/'p9-final-report-mcp')]),
 ('latency-gates',[sys.executable,str(base/'p9-evaluate-gates.py'),'--candidate-off',str(base/'p9-final-off-mcp/replay/report.json'),'--candidate-report',str(base/'p9-final-report-mcp/replay/report.json'),'--output',str(base/'p9-final-gates-r1.json')]),
 ('relevance',[sys.executable,'scripts/check-retrieval-quality.py','--binary',str(runtime/'airk.exe'),'--output',str(base/'p9-final-relevance-r1.json'),'--trials','3','--detail','agent_lean','--corpus','scripts/fixtures/retrieval-v190.json','--model-dir',str(Path(inputs['local-vector-model'])/'minilm-l6-v2-u8-v1')]),
 ('quality-comparison',[sys.executable,str(base/'p9-compare-quality.py'),'--candidate',str(base/'p9-final-relevance-r1.json'),'--output',str(base/'p9-final-quality-comparison-r1.json')]),
 ('ci-local',['pwsh','-NoProfile','-File','scripts/ci-local.ps1','-SkipBuild','-RuntimeDir',str(runtime),'-Cores','4','-Repeat','1']),
]
record['status']='running';save()
for name,command in steps:
    assert integrated.seal_runtime(runtime,expected)==seal,'Frozen runtime changed'
    log=f'artifacts/v190/p9-final-{name}-queue-r1.log'
    print('START',name,flush=True)
    result=subprocess.run([sys.executable,str(base/'p9-local-command.py'),log,*command],cwd=root)
    record['steps'].append({'name':name,'exit_code':result.returncode,'log':log});save()
    if result.returncode:
        record['status']='failed';save();sys.exit(result.returncode)
record['status']='passed';save()
