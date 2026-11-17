# SPDX-License-Identifier: Apache-2.0
"""Same maintained120s/8GiB owned NN0 metadata/template launcher; Root executes."""
from pathlib import Path
import argparse,hashlib,json,os,sys,time
R=Path(r'D:\code\Github\airk');O=R/'artifacts/advanced-split/models/actual-composed-service-r2/large-gqa-proposal-r1'
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
SELECTORS={'metadata':'arbiter::runtime::qualification::regeneration::emit_current_policy_profiles_from_verified_original_assets','template':'inference_engine::pool::arbiter::qualification::software_v2::emit_software_v2_native_numeric_template'}
def sha(p):
    with Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def pin(p):return {'path':str(p),'bytes':Path(p).stat().st_size,'sha256':sha(p)}
def checked(v):
    p=Path(v['path']);assert p.is_absolute() and p.is_file() and not p.is_symlink() and sha(p)==v['sha256'];return p
def write(p,v):
    with p.open('x',encoding='utf-8',newline='\n') as f:json.dump(v,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def main():
    started=time.monotonic();deadline=started+120
    a=argparse.ArgumentParser(description=__doc__)
    a.add_argument('--staging',required=True);a.add_argument('--staging-sha256',required=True);a.add_argument('--kind',choices=['metadata','template'],required=True);a.add_argument('--input-sha256');a.add_argument('--execute-reviewed',action='store_true');x=a.parse_args()
    staging_pin={'path':x.staging,'sha256':x.staging_sha256};staging=json.loads(checked(staging_pin).read_bytes())
    root=Path(x.staging).parent;assert root==O/'large-native-metadata-r1'
    request=json.loads(checked(staging['request']).read_bytes());assert request['schema']=='airk.private.context-metadata-regeneration/2' and 'large_graph' in request and 'compact_graph' not in request
    role='retrieval_image' if x.kind=='metadata' else 'inference_image';image=staging[role];checked(image)
    assert image['original_file_owner_sid']==image['preserved_file_owner_sid']==SID
    build=json.loads(checked(staging['actual_build_receipt']).read_bytes());assert build['passed'] is True and build['groups']['native']['sha256']==staging['native_group']
    command=staging[x.kind+'_command'];assert command==[image['path'],'--exact',SELECTORS[x.kind],'--ignored','--nocapture']
    env=dict(os.environ)
    if x.kind=='metadata':env.update(staging['metadata_environment']);input_pin=staging['request'];result=Path(request['output'])/'metadata-regeneration-report.private.json'
    else:
        assert x.input_sha256 is not None and len(x.input_sha256)==64
        template_env=staging['template_environment_after_actual_metadata'];input_pin={'path':template_env['AIRK_SOFTWARE_V2_NATIVE_INPUT'],'sha256':x.input_sha256};checked(input_pin)
        env.update(template_env);env['AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256']=x.input_sha256;result=Path(template_env['AIRK_SOFTWARE_V2_NATIVE_OUTPUT'])
    graph_plan_path=O/'converter/graph-plan-large-r1.json';assert sha(graph_plan_path)=='354c022a8ccb34266243865f55d717c021c0a2f1c777e6884668345ac478581c'
    graph_plan=json.loads(graph_plan_path.read_bytes());seals=dict(graph_plan['input_seals'])
    for value in [staging_pin,staging['request'],staging['actual_build_receipt'],staging['image_inventory'],image,input_pin,*request['source_files'],*request['large_graph'].values()]:
        checked(value);seals[value['path']]=value['sha256']
    seals[str(Path(__file__).resolve())]=sha(Path(__file__).resolve());seals[str(graph_plan_path)]=sha(graph_plan_path)
    sys.path.insert(0,graph_plan['owner_root']);import reference_worker,reference_owned,reference_parent_no_site_r2,windows_job_8g
    assert sys.flags.no_site==1 and sys.dont_write_bytecode and reference_worker.windows_sid()==SID
    runtime={'runtime_manifest':graph_plan['runtime_manifest'],'input_seals':seals};reference_worker.verify_loaded_runtime(runtime)
    output=root/(x.kind+'-owned-emission-r1');assert not output.exists() and not result.exists()
    if not x.execute_reviewed:print(json.dumps({'preflight_passed':True,'command':command,'neural_entry_dispatched':False}));return 0
    output.mkdir();(output/'temp').mkdir();env.update({'TEMP':str(output/'temp'),'TMP':str(output/'temp'),'TMPDIR':str(output/'temp'),'PYTHONPYCACHEPREFIX':str(output/'never-created-pycache')})
    log=output/'stdout.log';receipt={'schema':'airk.private.large-native-nn0-owned/1','kind':x.kind,'complete':False,'failure':None,'post_input_drift':[],'command':command,'input':input_pin,'image':image,'staging':staging_pin,'whole_seconds':120,'Job_bytes':8<<30,'neural_dispatch_scope':'Reviewed exact metadata/template selector has no scorer/forward entry; no runtime dispatch counter is claimed.'}
    write(output/'started.json',receipt)
    def before(api,job,info,child_deadline):return reference_parent_no_site_r2.suspended_authentication(api,info,deadline,child_deadline,SID,image,reference_owned.image_proof)
    try:
        remaining=deadline-time.monotonic();assert remaining>0
        with log.open('xb') as stream:actual=windows_job_8g.run_windows_owned(command,output,stream,remaining,env,absolute_deadline=deadline,before_resume=before)
        receipt['owned']=actual
        expected={'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,'owned_job_forced_termination':False,'owned_job_final_active_processes':0,'owned_job_final_member_pids':[],'parent_assigned_to_job':False,'child_assigned_before_resume':True}
        assert all(type(actual.get(k)) is type(v) and actual.get(k)==v for k,v in expected.items())
        assert result.is_file() and result.stat().st_size<=64<<20
        body=json.loads(result.read_bytes())
        if x.kind=='metadata':assert body['complete'] is True and len(body['profiles'])==1 and body['profiles'][0]['model']=='arbiter-context-qwen3-4b' and body['task_qualified'] is False and body['release_qualified'] is False
        else:assert body['model']=='arbiter-context-qwen3-4b' and len(body['rows'])==3 and all(row['reference_same_artifact'] is None and row['reference_original_fp32'] is None for row in body['rows'])
        receipt['native_output']=pin(result);receipt['complete']=True
    except BaseException as error:
        receipt['failure']={'type':type(error).__name__,'message':str(error)}
        if hasattr(error,'owned_cleanup'):receipt['owned_cleanup']=error.owned_cleanup
    finally:
        for path,value in seals.items():
            try:
                if sha(path)!=value:receipt['post_input_drift'].append(path)
            except OSError:receipt['post_input_drift'].append(path)
        try:reference_worker.verify_loaded_runtime(runtime)
        except BaseException as error:receipt['post_input_drift'].append(str(error))
        for name,path in [('native_output',result),('stdout',log)]:
            try:
                if path.is_file():
                    assert path.stat().st_size<=64<<20
                    receipt[name]=pin(path)
            except BaseException as error:
                receipt.setdefault('final_observation_errors',[]).append({'type':type(error).__name__,'message':str(error)})
                if receipt['failure'] is None:receipt['failure']={'type':type(error).__name__,'message':str(error)}
        now=time.monotonic();receipt['elapsed_seconds']=now-started;receipt['original_phase_expired']=now>=deadline
        if receipt['failure'] is not None or receipt['post_input_drift'] or now>=deadline:receipt['complete']=False
        write(output/'execution.json',receipt)
    print(json.dumps({'complete':receipt['complete'],'failure':receipt['failure'],'receipt':str(output/'execution.json')}));return 0 if receipt['complete'] else 1
if __name__=='__main__':raise SystemExit(main())
