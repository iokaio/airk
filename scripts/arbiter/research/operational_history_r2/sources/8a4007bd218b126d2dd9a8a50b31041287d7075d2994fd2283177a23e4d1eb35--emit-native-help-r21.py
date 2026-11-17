# SPDX-License-Identifier: Apache-2.0
import hashlib,json,os,types
from pathlib import Path
root=Path.cwd();out=root/'artifacts/advanced-split/receipts/native-help-r21';out.mkdir(exist_ok=False)
helper=root/'scripts/provision-static-onnx-runtime.py';raw=helper.read_bytes();assert hashlib.sha256(raw).hexdigest()=='bdbdf773716af7184da59ee17b9ec718f657874cfe15efea8462fbbae27fc59a'
owner=types.ModuleType('retained_help_owner');owner.__file__=str(helper);exec(compile(raw,str(helper),'exec'),owner.__dict__)
binary=root/'artifacts/v190/p10-target/debug/deps/airk_mcp-6566b8fea9bb6a35.exe'
with binary.open('rb') as f:binary_sha=hashlib.file_digest(f,'sha256').hexdigest()
rows=[]
for name,variable,selector in [('content','AIRK_WRITE_HELP_CONTENT','help::content_cases::help_content_manifest_matches_native_owners'),('pilot','AIRK_WRITE_UI_PILOT_HELP','ui_pilot_cases::ui_pilot_help_native_cases')]:
 os.environ[variable]='1';argv=[str(binary),'--exact',selector,'--nocapture','--test-threads=1'];result=owner.run_owned(argv,root,out/(name+'.log'),60);del os.environ[variable]
 rows.append({'name':name,'owned':result});(out/'execution.json').write_text(json.dumps({'binary_sha256':binary_sha,'rows':rows},indent=2)+'\n')
 assert owner.successful(result) and result['owned_job_forced_termination'] is False,result
with binary.open('rb') as f:assert binary_sha==hashlib.file_digest(f,'sha256').hexdigest()
print(json.dumps({'binary_sha256':binary_sha,'rows':rows}))
