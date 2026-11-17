# SPDX-License-Identifier: Apache-2.0
import base64,json,shlex,subprocess,time
from pathlib import Path
c=json.loads(Path("artifacts/v190/neo-connection.local.json").read_text());assert c["host"]=="192.168.68.61"
payload={name:base64.b64encode(Path("scripts",name).read_bytes()).decode() for name in ["provision-static-onnx-runtime.py","static-onnx-runtime-lock.json"]}
code="""
import base64,hashlib,importlib.util,json,os,subprocess,sys,time
from pathlib import Path
root=Path('/Users/tylerjensen/airk-builds/one-airk-static-ort-1814-1794087059')
probe=root/('runner-probe-'+str(time.time_ns()));(probe/'scripts').mkdir(parents=True)
for name,data in PAYLOAD.items():(probe/'scripts'/name).write_bytes(base64.b64decode(data))
s=importlib.util.spec_from_file_location('owner',probe/'scripts/provision-static-onnx-runtime.py');m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
results={'schema':1,'probe':str(probe),'purpose':'harmless corrected owned Unix runner, no compiler/Cargo/AIRK','base_sha256':m.digest(probe/'scripts/provision-static-onnx-runtime.py'),'cases':{}}
for name,argv,timeout in [('true',['/usr/bin/true'],10),('descendant',[sys.executable,'-c',"import subprocess,sys,time;subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)']);time.sleep(.2)"],10),('timeout',[sys.executable,'-c',"import subprocess,sys,time;subprocess.Popen([sys.executable,'-c','import time;time.sleep(30)']);time.sleep(30)"],.2)]:
 results['cases'][name]=m.run_owned(argv,probe,probe/(name+'.log'),timeout)
results['checks_passed']=all(r['owned_descendants_settled'] for r in results['cases'].values()) and results['cases']['true']['exit_code']==0 and results['cases']['descendant']['exit_code']==0 and results['cases']['timeout']['timed_out']
files=['link-supplement-attempt.json','logs/link-supplement.log','build-receipt.json','b2/Release/_deps/re2-build/libre2.a']
results['recovery_inputs']=[{'path':name,'bytes':(root/name).stat().st_size,'sha256':m.digest(root/name)} for name in files]
old=json.loads((root/'build-receipt.json').read_bytes());results['original79_unchanged']=len(old['libraries'])==79 and all((m.verify_file(root/r['path'],r) is None) for r in old['libraries'])
p=subprocess.run(['/bin/ps','-axo','pid,pgid,uid,state,command'],capture_output=True,text=True,timeout=10);assert p.returncode==0 and len(p.stdout)<4194304
results['campaign_processes']=[line for line in p.stdout.splitlines() if str(root) in line and int(line.split()[0])!=os.getpid()]
results['owned_root_processes_absent']=not results['campaign_processes']
(probe/'result.json').write_text(json.dumps(results,indent=2));print(json.dumps(results,indent=2));sys.exit(0 if results['checks_passed'] and results['original79_unchanged'] and results['owned_root_processes_absent'] else 1)
"""
code='PAYLOAD='+repr(payload)+'\n'+code
py='/Users/tylerjensen/airk-builds/timing-1.6.4.PutU1P/python/cpython-3.13.18-macos-aarch64-none/bin/python3.13'
cmd=shlex.quote(py)+' -'
p=subprocess.run(['ssh','-i',c['identity_path'],*c['ssh_options'],c['user']+'@'+c['host'],cmd],input=code.encode(),capture_output=True,timeout=50)
out=Path('artifacts/local/one-airk-1814/neo-static-ort/supplement-1794092444');(out/'corrected-runner-probe.json').write_bytes(p.stdout);(out/'corrected-runner-probe.stderr').write_bytes(p.stderr)
print(p.stdout.decode());print(p.stderr.decode());raise SystemExit(p.returncode)
