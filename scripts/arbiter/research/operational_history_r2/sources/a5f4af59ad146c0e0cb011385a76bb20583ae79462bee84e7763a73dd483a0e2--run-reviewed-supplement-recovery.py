# SPDX-License-Identifier: Apache-2.0
import base64,json,shlex,subprocess,time
from pathlib import Path
c=json.loads(Path("artifacts/v190/neo-connection.local.json").read_text());assert c["host"]=="192.168.68.61"
payload={name:base64.b64encode(Path("scripts",name).read_bytes()).decode() for name in ["provision-static-onnx-runtime.py","static-onnx-link-closure.py","static-onnx-runtime-lock.json","adopt-neo-static-onnx-runtime.py"]}
code="""
import base64,hashlib,importlib.util,json,time,sys,traceback
from pathlib import Path
root=Path('/Users/tylerjensen/airk-builds/one-airk-static-ort-1814-1794087059')
area=root/('recovery-tools-'+str(time.time_ns()));(area/'scripts').mkdir(parents=True)
for name,data in PAYLOAD.items():(area/'scripts'/name).write_bytes(base64.b64decode(data))
s=importlib.util.spec_from_file_location('static_core',area/'scripts/provision-static-onnx-runtime.py');m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
target='aarch64-apple-darwin';record={'schema':1,'purpose':'explicit reviewed archive adoption after old owned runner failed; no build or replay','area':str(area),'source_files':{name:m.digest(area/'scripts'/name) for name in PAYLOAD},'steps':[]}
try:
 before=m.digest(root/'build-receipt.json');started=time.monotonic();result=m.load_script('static-onnx-link-closure.py').recover(root,target,m.core_api());record['steps'].append({'stage':'recover','elapsed_seconds':round(time.monotonic()-started,3),'result':'pass'})
 m.write_json(root/'static-environment-recovery.json',result)
 started=time.monotonic();verified=m.verify_build(root,target);record['steps'].append({'stage':'separate_verify','elapsed_seconds':round(time.monotonic()-started,3),'result':'pass'})
 assert before==m.digest(root/'build-receipt.json')
 recovery=m.read_json(root/'link-supplement-recovery.json');record.update({'result':'pass','original_adoption_sha256':before,'original79_unchanged':len(result['receipt']['libraries'])==79,'recovery_sha256':m.digest(root/'link-supplement-recovery.json'),'environment_sha256':m.digest(root/'static-environment-recovery.json'),'closure_archives':len(result['static_link_closure']['archives']),'command_exit_unknown':recovery['command_exit_unknown'],'process_absence':recovery['process_absence'],'qualification':recovery['qualification']})
 record['retained_files']={name:base64.b64encode((root/name).read_bytes()).decode() for name in ['link-supplement-recovery.json','static-environment-recovery.json']}
except Exception as error:
 record.update({'result':'failed','error':str(error),'traceback':traceback.format_exc()})
(area/'execution.json').write_text(json.dumps(record,indent=2));print(json.dumps(record));sys.exit(0 if record['result']=='pass' else 1)
"""
code='PAYLOAD='+repr(payload)+'\n'+code
py='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13'
p=subprocess.run(['ssh','-i',c['identity_path'],*c['ssh_options'],c['user']+'@'+c['host'],shlex.quote(py)+' -'],input=code.encode(),capture_output=True,timeout=120)
out=Path('artifacts/local/one-airk-1814/neo-static-ort/supplement-1794092444/recovery-reviewed');out.mkdir(exist_ok=False)
(out/'execution.stdout.json').write_bytes(p.stdout);(out/'execution.stderr').write_bytes(p.stderr)
try:
 v=json.loads(p.stdout)
 for name,body in v.pop('retained_files',{}).items():(out/name).write_bytes(base64.b64decode(body))
 print(json.dumps(v,indent=2))
except Exception:print(p.stdout.decode());print(p.stderr.decode())
raise SystemExit(p.returncode)
