# SPDX-License-Identifier: Apache-2.0
import importlib.util,json,subprocess,sys
from pathlib import Path
root=Path.cwd();base=root/'artifacts/v190'
expected={'sha256':'422249902bd8863abd8bcd938c0f5b8096535b00932ecaacabf3a9b4a038bcb7','files':924}
for name in ['p9-minimal-feature-clippy-r2.receipt.json','p9-feature-gates-all-clippy-r1.receipt.json']:
 r=json.loads((base/name).read_text());assert r['exit_code']==0 and r['source_unchanged'] and r['source_after']==expected
out=base/'p9-combined-final-queue-r2.json';assert not out.exists()
record={'status':'running','source':expected,'steps':[],'full_workspace_evidence':'p9-combined-final-workspace-r1.receipt.json source224ee433; subsequent change is exactly three cfg guards in remote_tokenization. All-feature production/test code remains unchanged.'}
def save():out.write_text(json.dumps(record,indent=2)+'\n')
save()
commands=json.loads((base/'p9-final-runtime-command-map-r5.json').read_text())['commands']
steps=[
 {'name':'fmt','log':'artifacts/v190/p9-feature-gates-fmt-r1.log','command':['cargo','fmt','--all','--check']},
 {'name':'minimal-tokenizer','log':'artifacts/v190/p9-feature-gates-minimal-tokenizer-r1.log','command':['cargo','test','-p','airk-kernel','--no-default-features','--lib','--locked','shared_calibration','--','--test-threads=1']},
 {'name':'helper-tokenizer','log':'artifacts/v190/p9-feature-gates-helper-tokenizer-r1.log','command':['cargo','test','-p','airk-kernel','--all-features','--lib','--locked','shared_calibration','--','--test-threads=1']},
 *commands]
spec=importlib.util.spec_from_file_location('integrated',base/'p9-integrated-checks.py');integrated=importlib.util.module_from_spec(spec);spec.loader.exec_module(integrated)
runtime=base/'p9-runtime-final-r5';seal=None
for row in steps:
 if seal is not None:assert integrated.seal_runtime(runtime,expected)==seal
 print('START',row['name'],flush=True)
 result=subprocess.run([sys.executable,str(base/'p9-local-command.py'),row['log'],*row['command']],cwd=root)
 record['steps'].append({'name':row['name'],'exit_code':result.returncode,'log':row['log']});save()
 if result.returncode:record['status']='failed';save();sys.exit(result.returncode)
 r=json.loads((root/row['log']).with_suffix('.receipt.json').read_text());assert r['source_after']==expected and r['source_unchanged']
 if row['name']=='runtime-build':
  seal=integrated.seal_runtime(runtime,expected);(base/'p9-runtime-final-r5-seal.json').write_text(json.dumps(seal,indent=2)+'\n')
 if seal is not None:assert integrated.seal_runtime(runtime,expected)==seal
record['status']='passed';save()
