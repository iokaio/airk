# SPDX-License-Identifier: Apache-2.0
"""Owned real installer/upgrade fixture. No SQL, synthetic activation, or publication."""
import hashlib, importlib.util, json, os, subprocess, time, uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
s=importlib.util.spec_from_file_location('clients', ROOT/'scripts/check-service-clients.py'); c=importlib.util.module_from_spec(s); s.loader.exec_module(c)
old=(ROOT/'artifacts/v190/p6f-resume-predecessor-shared-copy').resolve(strict=True)
new=(ROOT/'artifacts/v190/p6f-resume-activation-shared-copy-r2').resolve(strict=True)
owner=ROOT/'artifacts/v190/p6f-resume-legacy-upgrade-fixture-r3'
assert not owner.exists(); owner.mkdir()
install=owner/'install'; state=owner/'state'; source=owner/'source'; data=owner/'data'
source.mkdir(); data.mkdir(); (source/'fixture.ts').write_text('export function LegacyOriginalEvidence() { return 42; }\n',encoding='utf-8')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
approvals={str(p):sha(p/'airk-install.json') for p in (old,new)}
records=[]; f=c.Fixture(old,owner); f.control=state/'control'
def command(runtime,*args,stdin=None):
 n=len(records)+1; result=subprocess.run([str(runtime/'airk.exe'),*map(str,args)],input=stdin,capture_output=True,text=True,encoding='utf-8',timeout=360,creationflags=0x08000000 if os.name=='nt' else 0)
 records.append({'args':list(map(str,args)),'returncode':result.returncode})
 (owner/f'command-{n}.stdout').write_text(result.stdout,encoding='utf-8'); (owner/f'command-{n}.stderr').write_text(result.stderr,encoding='utf-8'); (owner/'commands.json').write_text(json.dumps(records,indent=2),encoding='utf-8')
 assert result.returncode==0, (args,result.stderr[-4096:]); return json.loads(result.stdout)
def stop(runtime):
 health=f.rpc('health')['observation']; epoch=f.rpc('request_epoch'); request={'caller':uuid.uuid4().hex,'key':{'namespace':epoch['namespace'],'epoch':epoch['epoch'],'nonce':uuid.uuid4().hex},'action':'stop','expected_instance':health['instance'],'runtime':approvals[str(runtime)],'force':False}
 value=command(runtime,'runtime','lifecycle','--install-dir',install,stdin=json.dumps(request)); assert value['status']=='lifecycle' and value['record']['phase']=='complete'; return value
try:
 command(old,'runtime','stage','--source',old,'--install-dir',install,'--state-dir',state,'--approved-sha256',approvals[str(old)],'--verification-basis','organization-digest')
 command(old,'runtime','activate','--install-dir',install,'--runtime-sha256',approvals[str(old)])
 f.register('legacy',source,data); f.job('legacy','index'); before=f.tool('legacy','airk_symbols',{'name':'LegacyOriginalEvidence','compact':False,'response_mode':'full'})
 assert 'LegacyOriginalEvidence' in json.dumps(before)
 f.rpc({'pause':{'project':f.projects['legacy']['binding'],'paused':True}})
 original={p.relative_to(source).as_posix():sha(p) for p in source.rglob('*') if p.is_file()}
 original_inspect=command(old,'runtime','inspect','--install-dir',install)
 config=owner/'original-mcp.json'; args=['--service-install-dir',str(install),*f.arguments('legacy')[2:],'mcp']
 entry={'command':original_inspect['launcher'],'args':args,'env':{'AIRK_CLIENT':'vscode'}}
 config.write_text(json.dumps({'servers':{'legacy-original':entry}},indent=2)+'\n',encoding='utf-8'); config_hash=sha(config)
 command(new,'runtime','stage','--source',new,'--install-dir',install,'--state-dir',state,'--approved-sha256',approvals[str(new)],'--verification-basis','organization-digest')
 command(new,'runtime','activate','--install-dir',install,'--runtime-sha256',approvals[str(new)])
 activation=json.loads((install/'activation-result.json').read_bytes())
 f.binary=new/'airk.exe'; f.runtime=new
 health=f.rpc('health'); inspect=command(new,'runtime','inspect','--install-dir',install)
 receipt=json.loads((f.control/'index-catalog-upgrade.json').read_bytes()); assert receipt['stage']=='settled' and receipt['authority_v1']['phase']=='settled'
 assert config_hash==sha(config) and original=={p.relative_to(source).as_posix():sha(p) for p in source.rglob('*') if p.is_file()}
 evidence=[]
 for name,value in [('activation',activation),('inspect',inspect),('post-health',health),('upgrade-receipt',receipt)]:
  p=owner/(name+'.json'); p.write_text(json.dumps(value,indent=2)+'\n',encoding='utf-8'); evidence.append({'path':str(p),'sha256':sha(p)})
 runtime=Path(inspect['executable']).parent; pair=json.loads((runtime/'airk-runtime.json').read_bytes()); launcher=Path(inspect['launcher']); launcher_receipt=install/'launcher.json'
 binding={'kind':'installation','command_path':str(launcher),'command_sha256':sha(launcher),'installation_launcher':{'path':str(launcher),'sha256':sha(launcher),'receipt':{'path':str(launcher_receipt),'sha256':sha(launcher_receipt)}}}
 report={'schema':1,'fixture_kind':'managed_native_catalog_upgrade','fixture_root':str(owner),'install_dir':str(install),'control_dir':str(f.control),'data_root':str(data),'source_root':str(source),'candidate':{'install_manifest_sha256':approvals[str(new)],'runtime_dir':str(runtime),'engine_sha256':pair['airk_sha256'],'helper_sha256':pair['embed_sha256']},'legacy_config':{'kind':'vscode_servers','binding_kind':'stable_launcher','path':str(config),'sha256':config_hash,'entry_key':'legacy-original','command':entry['command'],'args':args,'env':entry['env'],'cwd':str(source),'launcher_binding':binding},'source_hashes_before':original,'source_hashes_after':original,'probe':{'symbol':'LegacyOriginalEvidence','path':'fixture.ts','content_sha256':original['fixture.ts']},'native':{'activation_receipt':activation,'authority_v1_readiness':receipt['authority_v1'],'post_health':health,'inspect':inspect,'evidence_files':evidence}}
 (owner/'fixture.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8'); print(json.dumps({'passed':True,'fixture':str(owner/'fixture.json')}),flush=True)
finally:
 try:
  if (f.control/'service-1.4.sqlite3').is_file():
   current=new if (install/'selected.json').is_file() and json.loads((install/'selected.json').read_bytes())['active']==approvals[str(new)] else old
   f.binary=current/'airk.exe'; f.runtime=current
   stopped=stop(current); (owner/'stop.json').write_text(json.dumps(stopped,indent=2),encoding='utf-8')
 except Exception as e:
  (owner/'cleanup-error.txt').write_text(str(e),encoding='utf-8'); raise
 f.log.close()
