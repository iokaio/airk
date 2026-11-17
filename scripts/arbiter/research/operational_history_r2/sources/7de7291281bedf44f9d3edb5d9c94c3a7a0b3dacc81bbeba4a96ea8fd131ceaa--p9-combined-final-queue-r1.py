# SPDX-License-Identifier: Apache-2.0
import importlib.util,json,subprocess,sys,time
from pathlib import Path
root=Path.cwd();base=root/'artifacts/v190'
expected={'sha256':'224ee433248d09cbfb88f51a996464748b0adf9c552a4db011e930616a8b68bc','files':924}
out=base/'p9-combined-final-queue-r1.json';assert not out.exists()
record={'status':'waiting_owners','source':expected,'steps':[]}
def save():out.write_text(json.dumps(record,indent=2)+'\n')
save()
receipt=base/'p9-combined-audit-owners-r2.receipt.json'
deadline=time.monotonic()+3600
while not receipt.exists():
 assert time.monotonic()<deadline
 time.sleep(5)
r=json.loads(receipt.read_text());assert r['exit_code']==0 and r['source_unchanged'] and r['source_after']==expected,r
commands=json.loads((base/'p9-final-runtime-command-map-r4.json').read_text())['commands']
steps=[{'name':'workspace','log':'artifacts/v190/p9-combined-final-workspace-r1.log','command':['cargo','test','--workspace','--all-features','--locked']},*commands]
spec=importlib.util.spec_from_file_location('integrated',base/'p9-integrated-checks.py');integrated=importlib.util.module_from_spec(spec);spec.loader.exec_module(integrated)
runtime=base/'p9-runtime-final-r4';seal=None
record['status']='running';save()
for row in steps:
 if seal is not None:assert integrated.seal_runtime(runtime,expected)==seal
 print('START',row['name'],flush=True)
 result=subprocess.run([sys.executable,str(base/'p9-local-command.py'),row['log'],*row['command']],cwd=root)
 record['steps'].append({'name':row['name'],'exit_code':result.returncode,'log':row['log']});save()
 if result.returncode:record['status']='failed';save();sys.exit(result.returncode)
 receipt=json.loads((root/row['log']).with_suffix('.receipt.json').read_text());assert receipt['source_after']==expected and receipt['source_unchanged']
 if row['name']=='runtime-build':
  seal=integrated.seal_runtime(runtime,expected);(base/'p9-runtime-final-r4-seal.json').write_text(json.dumps(seal,indent=2)+'\n')
 if seal is not None:assert integrated.seal_runtime(runtime,expected)==seal
record['status']='passed';save()
