# SPDX-License-Identifier: Apache-2.0
"""Actual retained predecessor upgrade and unchanged original native MCP configuration; no SQL or synthetic activation."""
import hashlib,importlib.util,json,os,subprocess,tempfile,uuid
from pathlib import Path
RUN=Path(__file__).resolve().parent;ROOT=RUN/'source';OLD=Path('/Users/tylerjensen/airk-builds/p6a-20261103.jBR4Jh/source-r2/artifacts/p6e-runtime');NEW=ROOT/'artifacts/neo-full-runtime'
def load(name,file):
 spec=importlib.util.spec_from_file_location(name,ROOT/file);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
c=load('native_clients','scripts/check-service-clients.py');b=load('native_replay','scripts/benchmark-mcp-investigation.py')
def main(runtime):
 for name in ['airk','airk.exe','AIRK.app/Contents/MacOS/airk']:
  if (runtime/name).is_file():return runtime/name
 raise AssertionError('Actual native main absent')
sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
for runtime in [OLD,NEW]:
 inventory=json.loads((runtime/'airk-install.json').read_bytes())
 assert inventory['version']=='1.8.8' and inventory['os']=='macos' and inventory['arch']=='aarch64'
 for name,digest in inventory['files'].items():assert (runtime/name).is_file() and sha(runtime/name)==digest,(runtime,name)
owner=Path(tempfile.mkdtemp(prefix='airk-p6-full-upgrade-',dir=os.environ['AIRK_PRIVATE_CENSUS_BASE'])).resolve();(RUN/'upgrade-owner.json').write_text(json.dumps({'owner':str(owner)}));install=owner/'install';state=owner/'state';source=owner/'source';data=owner/'data';source.mkdir();data.mkdir();(source/'fixture.ts').write_text('export function NeoLegacyEvidence() { return 42; }\n')
approvals={runtime:sha(runtime/'airk-install.json') for runtime in [OLD,NEW]};fixture=c.Fixture(OLD,owner);fixture.control=state/'control';records=[]
def command(runtime,*args,stdin=None):
 result=subprocess.run([str(main(runtime)),*map(str,args)],input=stdin,text=True,capture_output=True,timeout=360);n=len(records);(owner/f'command-{n}.stdout').write_text(result.stdout);(owner/f'command-{n}.stderr').write_text(result.stderr);records.append({'args':list(map(str,args)),'code':result.returncode});(owner/'commands.json').write_text(json.dumps(records,indent=2));assert result.returncode==0,result.stderr;return json.loads(result.stdout)
selected=OLD
try:
 command(OLD,'runtime','stage','--source',OLD,'--install-dir',install,'--state-dir',state,'--approved-sha256',approvals[OLD],'--verification-basis','organization-digest');command(OLD,'runtime','activate','--install-dir',install,'--runtime-sha256',approvals[OLD])
 fixture.register('legacy',source,data);fixture.configure('legacy',{'operation':'vectors','enabled':False});fixture.job('legacy','index');fixture.rpc({'pause':{'project':fixture.projects['legacy']['binding'],'paused':True}})
 inspection=command(OLD,'runtime','inspect','--install-dir',install);config={'command':inspection['launcher'],'args':[*fixture.arguments('legacy')[2:],'mcp'],'expected_version':'1.8.8','cwd':str(source)};configfile=owner/'original-mcp.json';configfile.write_text(json.dumps(config,indent=2));config_hash=sha(configfile);source_hash=sha(source/'fixture.ts')
 def read():
  with b.Session(config,90) as session:
   envelope,_=session.tool(('symbols',{'name':'NeoLegacyEvidence','compact':False,'response_mode':'full'}));assert not envelope.get('isError'),envelope;return json.loads(envelope['content'][0]['text'])
 before=read();assert len(before['items'])==1 and before['items'][0]['citation']['content_hash']==source_hash
 command(NEW,'runtime','stage','--source',NEW,'--install-dir',install,'--state-dir',state,'--approved-sha256',approvals[NEW],'--verification-basis','organization-digest');command(NEW,'runtime','activate','--install-dir',install,'--runtime-sha256',approvals[NEW]);selected=NEW;fixture.binary=main(NEW);fixture.runtime=NEW
 receipt=json.loads((fixture.control/'index-catalog-upgrade.json').read_bytes());assert receipt['stage']=='settled' and receipt['authority_v1']['phase']=='settled';after=read();assert after['scope_observation']['provenance']=='imported_legacy_snapshot';assert len(after['items'])==1 and after['items'][0]['citation']['content_hash']==source_hash;assert sha(configfile)==config_hash and sha(source/'fixture.ts')==source_hash
 report={'passed':True,'predecessor_install_manifest_sha256':approvals[OLD],'candidate_install_manifest_sha256':approvals[NEW],'original_config_sha256':config_hash,'source_hash':source_hash,'source_preserved':True,'configuration_preserved':True,'native_upgrade_receipt':receipt,'before':before,'after':after};(RUN/'actual-upgrade.json').write_text(json.dumps(report,indent=2));print(json.dumps({'passed':True,'actual_upgrade':True,'source_preserved':True,'configuration_preserved':True}),flush=True)
finally:
 try:
  fixture.binary=main(selected);fixture.runtime=selected;health=fixture.rpc('health')['observation'];epoch=fixture.rpc('request_epoch');request={'caller':uuid.uuid4().hex,'key':{'namespace':epoch['namespace'],'epoch':epoch['epoch'],'nonce':uuid.uuid4().hex},'action':'stop','expected_instance':health['instance'],'runtime':approvals[selected],'force':False};stop=command(selected,'runtime','lifecycle','--install-dir',install,stdin=json.dumps(request));assert stop['status']=='lifecycle' and stop['record']['phase']=='complete';(RUN/'actual-upgrade-stop.json').write_text(json.dumps(stop,indent=2))
 finally:fixture.log.close()
