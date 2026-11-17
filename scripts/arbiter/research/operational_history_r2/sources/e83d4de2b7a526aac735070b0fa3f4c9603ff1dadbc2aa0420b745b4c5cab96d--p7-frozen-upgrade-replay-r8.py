# SPDX-License-Identifier: Apache-2.0
"""Frozen replay through a real owned predecessor upgrade, without changing its workload."""
import argparse, hashlib, importlib.util, json, os, subprocess, tempfile, uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
def module(name,file):
 s=importlib.util.spec_from_file_location(name,ROOT/file); m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
b=module('frozen_replay','scripts/benchmark-mcp-investigation.py');c=module('native_clients','scripts/check-service-clients.py')
p=argparse.ArgumentParser();p.add_argument('--verification',choices=['off','report'],required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
a.output=a.output.resolve();a.output.mkdir(exist_ok=False)
old=(ROOT/'artifacts/v190/p6f-resume-predecessor-shared-copy').resolve();new=(ROOT/'artifacts/v190/p7-runtime-release-r8').resolve()
sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
approvals={x:sha(x/'airk-install.json') for x in (old,new)}
owner=Path(tempfile.mkdtemp(prefix='airk-p6f-e0-',dir=os.environ['AIRK_PRIVATE_CENSUS_BASE'])).resolve();(a.output/'owner-location.json').write_text(json.dumps({'root':str(owner)}));fixture=owner/'fixture';b.create_fixture(fixture);source,_=b.load_fixture(fixture);install=owner/'install';state=owner/'state';data=owner/'data';data.mkdir()
f=c.Fixture(old,owner);f.control=state/'control';records=[]
def command(runtime,*args,stdin=None):
 v=subprocess.run([str(runtime/'airk.exe'),*map(str,args)],input=stdin,capture_output=True,text=True,timeout=360,creationflags=0x08000000)
 n=len(records);records.append({'args':list(map(str,args)),'code':v.returncode});(owner/f'command-{n}.stdout').write_text(v.stdout);(owner/f'command-{n}.stderr').write_text(v.stderr);(owner/'commands.json').write_text(json.dumps(records,indent=2));assert v.returncode==0,v.stderr;return json.loads(v.stdout)
try:
 command(old,'runtime','stage','--source',old,'--install-dir',install,'--state-dir',state,'--approved-sha256',approvals[old],'--verification-basis','organization-digest');command(old,'runtime','activate','--install-dir',install,'--runtime-sha256',approvals[old])
 f.register('investigation',source,data)
 for configuration in [{'operation':'vectors','enabled':False},{'operation':'detail','level':'agent_lean'},{'operation':'usage','mode':'disabled'}]:f.configure('investigation',configuration)
 f.job('investigation','index');f.rpc({'pause':{'project':f.projects['investigation']['binding'],'paused':True}})
 command(new,'runtime','stage','--source',new,'--install-dir',install,'--state-dir',state,'--approved-sha256',approvals[new],'--verification-basis','organization-digest');command(new,'runtime','activate','--install-dir',install,'--runtime-sha256',approvals[new]);f.binary=new/'airk.exe';f.runtime=new
 inspect=command(new,'runtime','inspect','--install-dir',install);launcher=Path(inspect['launcher'])
 receipt=json.loads((f.control/'index-catalog-upgrade.json').read_bytes());assert receipt['stage']=='settled' and receipt['authority_v1']['phase']=='settled'
 (a.output/'upgrade-receipt.json').write_text(json.dumps(receipt,indent=2))
 config={'command':str(new/'airk.exe'),'args':[*f.arguments('investigation'),'mcp'],'launcher_sha256':sha(new/'airk.exe'),'expected_version':'1.8.8','route':'isolated-approved-runtime','cwd':str(new)}
 configuration=a.output/'configuration.json';configuration.write_text(json.dumps(config,indent=2))
 passed=b.run(argparse.Namespace(fixture=fixture,configuration=configuration,output=a.output/'replay',label='candidate',samples=30,concurrency=[1],timeout=30,verification=a.verification,record=None));assert passed
finally:
 try:
  health=f.rpc('health')['observation'];epoch=f.rpc('request_epoch');request={'caller':uuid.uuid4().hex,'key':{'namespace':epoch['namespace'],'epoch':epoch['epoch'],'nonce':uuid.uuid4().hex},'action':'stop','expected_instance':health['instance'],'runtime':approvals[new],'force':False}
  stop=command(new,'runtime','lifecycle','--install-dir',install,stdin=json.dumps(request));assert stop['status']=='lifecycle' and stop['record']['phase']=='complete';(a.output/'lifecycle.json').write_text(json.dumps(stop,indent=2))
 finally:f.log.close()
print(json.dumps({'passed':True,'output':str(a.output)}))
