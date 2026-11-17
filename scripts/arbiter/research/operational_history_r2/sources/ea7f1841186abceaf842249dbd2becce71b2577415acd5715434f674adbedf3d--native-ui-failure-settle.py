# SPDX-License-Identifier: Apache-2.0
import hashlib,json,subprocess,uuid
from pathlib import Path
run=Path(__file__).resolve().parent;source=run/'source';evidence=run/'evidence';failed=json.loads((evidence/'native-ui-result.json').read_text());assert failed['status']=='failed'
owned=Path(failed['editor_run']).resolve(strict=True);assert owned.is_relative_to(source/'artifacts/editor-qualification') and owned.name=='native-installed-WYOvHW'
install=owned/'same/native-service';control=install/'control';runtime=source/'artifacts/neo/signed-notarized-r2/runtime';binary=runtime/'AIRK.app/Contents/MacOS/airk';expected=hashlib.sha256((runtime/'airk-install.json').read_bytes()).hexdigest();rows=[]
def call(name,args,input=None,timeout=45):
 command=[str(binary),*map(str,args)];p=subprocess.run(command,input=input,capture_output=True,text=True,timeout=timeout);row={'name':name,'command':command,'exit_code':p.returncode,'stdout':p.stdout[-65536:],'stderr':p.stderr[-8192:]};rows.append(row);return p
inspection=call('inspect-exact-owned-install',['runtime','inspect','--install-dir',install]);assert inspection.returncode==0;observed=json.loads(inspection.stdout)
assert Path(observed['control_dir']).resolve()==control.resolve() and observed['approval']['manifest_sha256']==expected
view=json.loads((owned/'same/a-ui-done-9.json').read_text());assert observed['registry_namespace']==view['namespace']
health=call('health-exact-owned-control',['service','health','--runtime-dir',control])
if health.returncode==0:
 selection={'key':{'namespace':view['namespace'],'epoch':0,'nonce':uuid.uuid4().hex},'expected_catalog_revision':view['revision'],'index_id':'idx_065143763944ea69568087a21bf7d743','change':{'operation':'trim_path','source_root_id':'src_76d5078c277013b108bb7129b1f01a04','kind':'exact','path':'hidden.ts'}}
 call('read-only-source-mutation-draft',['service','rpc','--runtime-dir',control,'--stdin'],input=json.dumps({'catalog_source_mutation_draft':{'selection':selection}}))
 call('canonical-shutdown-exact-owned-control',['service','shutdown','--runtime-dir',control])
else:rows.append({'name':'canonical-shutdown-not-needed','reason':'Private owned service is already unavailable; no start or replay attempted'})
after=call('post-cleanup-health',['service','health','--runtime-dir',control])
report={'schema':'airk.neo-failed-editor-owned-settlement/1','original_scenario_status':'failed; unchanged','owned_install':str(install),'control':str(control),'verified_runtime_sha256':expected,'verified_namespace':observed['registry_namespace'],'checks':rows,'service_health_unavailable_after':after.returncode!=0,'profiles_and_uninstall_receipts_preserved':True,'target_absent':not (run/'target').exists() and not (source/'target').exists()}
(evidence/'native-ui-failure-settlement.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
