# SPDX-License-Identifier: Apache-2.0
"""Same maintained120s/8GiB owned Compact original-roster launcher; Root executes."""
from pathlib import Path
import argparse,hashlib,json,os,sys,time
R=Path(r'D:\code\Github\airk');O=R/'artifacts/advanced-split/models/actual-composed-service-r2/compact-gqa-six-reference-preparation-r1'
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
SELECTOR='inference_engine::pool::arbiter::qualification::software_v2::original_sources::emit_software_v2_original_reference_assets'
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
    a.add_argument('--staging',required=True);a.add_argument('--staging-sha256',required=True);a.add_argument('--execute-reviewed',action='store_true');x=a.parse_args()
    staging_pin={'path':x.staging,'sha256':x.staging_sha256};staging=json.loads(checked(staging_pin).read_bytes())
    root=Path(x.staging).parent;assert root==O/'roster-stage'
    input_pin=staging['reference_native_input'];request=json.loads(checked(input_pin).read_bytes())
    assert request['schema']=='airk.arbiter.resource-run/2' and request['numeric_fixture'] is None
    profile_pin=request['profile'];profile=json.loads(checked(profile_pin).read_bytes());assert profile['model']=='arbiter-context-qwen3-17b'
    manifest_pin=request['source_manifest'];manifest=json.loads(checked(manifest_pin).read_bytes())
    image=staging['native_image'];checked(image)
    assert image['original_file_owner_sid']==image['preserved_file_owner_sid']==SID
    inventory=json.loads(checked(staging['image_inventory']).read_bytes())
    assert next(v for v in inventory['images'] if v['binary_id']=='airk-inference')==image
    assert inventory['actual_build_receipt']==manifest['native_build_receipt']
    build=json.loads(checked(manifest['native_build_receipt']).read_bytes());assert build['passed'] is True and build['groups']['native']['sha256']==inventory['native_group']
    command=staging['command'];assert command==[image['path'],'--exact',SELECTOR,'--ignored','--nocapture']
    env=dict(os.environ);env.update(staging['environment']);result=Path(staging['native_roster_output'])
    assert env['AIRK_SOFTWARE_V2_NATIVE_INPUT']==input_pin['path'] and env['AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256']==input_pin['sha256']
    assert env['AIRK_SOFTWARE_V2_NATIVE_OUTPUT']==str(result) and result==root/'original-roster.json'
    original_pin={'path':env['AIRK_SOFTWARE_V2_ORIGINAL_INPUT'],'sha256':env['AIRK_SOFTWARE_V2_ORIGINAL_INPUT_SHA256']}
    assert original_pin['sha256']=='488ef2cde44d83abf7d2f167ad2c5af486da2bb7449c5047818fc4add5fbd8c0'
    original=json.loads(checked(original_pin).read_bytes())
    graph_plan_path=O.parent/'compact-attention-proposal-r5/graph-plan.json';assert sha(graph_plan_path)=='e7afc5c0889794addd4f310840d74cc62ab61665164078906d95c5e13e7d84ee'
    graph_plan=json.loads(graph_plan_path.read_bytes());seals=dict(graph_plan['input_seals'])
    # Only consumed metadata/source/publisher files are sealed here. Native
    # ONNX weights are not read by the original-roster selector; complete model
    # asset custody belongs to the later reference/native measurement owners.
    for value in [staging_pin,input_pin,profile_pin,manifest_pin,manifest['native_build_receipt'],staging['image_inventory'],image,original_pin,*manifest['source_files'],*original['files']]:
        checked(value);seals[value['path']]=value['sha256']
    seals[str(Path(__file__).resolve())]=sha(Path(__file__).resolve());seals[str(graph_plan_path)]=sha(graph_plan_path)
    sys.path.insert(0,graph_plan['owner_root']);import reference_worker,reference_owned,reference_parent_no_site_r2,windows_job_8g
    assert sys.flags.no_site==1 and sys.dont_write_bytecode and reference_worker.windows_sid()==SID
    runtime={'runtime_manifest':graph_plan['runtime_manifest'],'input_seals':seals};reference_worker.verify_loaded_runtime(runtime)
    output=root/'roster-owned-emission-r1';assert not output.exists() and not result.exists()
    if not x.execute_reviewed:print(json.dumps({'preflight_passed':True,'command':command,'neural_entry_dispatched':False}));return 0
    output.mkdir();(output/'temp').mkdir();env.update({'TEMP':str(output/'temp'),'TMP':str(output/'temp'),'TMPDIR':str(output/'temp'),'PYTHONPYCACHEPREFIX':str(output/'never-created-pycache')})
    log=output/'stdout.log';receipt={'schema':'airk.private.compact-original-roster-nn0-owned/1','kind':'original_roster','complete':False,'failure':None,'post_input_drift':[],'command':command,'input':input_pin,'image':image,'staging':staging_pin,'whole_seconds':120,'Job_bytes':8<<30,'neural_dispatch_scope':'Reviewed exact original-roster selector has no scorer/forward entry; no runtime dispatch counter is claimed.'}
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
        assert body['schema']=='airk.arbiter.original-numeric-source-assets/1' and body['model']=='arbiter-context-qwen3-17b'
        assert body['native_input_sha256']==input_pin['sha256'] and body['original_source_input']==original_pin
        assert body['native_airk_nn_invocations']==0 and body['qualification'] is False
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
