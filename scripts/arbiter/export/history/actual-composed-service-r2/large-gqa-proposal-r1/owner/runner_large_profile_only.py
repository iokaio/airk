# SPDX-License-Identifier: Apache-2.0
"""Private ONE Large8192 profiling-only /2 diagnostic; Root sole native launcher."""
import argparse,ctypes,hashlib,importlib.util,json,math,os,re,shutil,sys,time
from pathlib import Path
sys.dont_write_bytecode=True
BASE=Path(__file__).resolve().parent
OUTPUT_PARENT=BASE.parents[1]
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
SELECTOR='inference_engine::pool::arbiter::qualification::software_v2::attention_diagnostic::software_v2_large_attention_profiled_8192_diagnostic'
from capacity import initial_free,RESERVATION,CONTAINMENT,GRAPH,WEIGHTS,FREE
from watch import make_watch

def need(value,message):
    if not value:raise ValueError(message)
def digest(path):
    with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def unaliased(value):
    text=os.fspath(value);need(Path(text).is_absolute() and not any(v in ('.','..') for v in re.split(r'[\\/]',text)),'Absolute unaliased path required')
    path=Path(text)
    for item in (path,*path.parents):
        m=item.lstat();need(not item.is_symlink() and not getattr(m,'st_file_attributes',0)&0x400,'Redirected path refused')
    return path.resolve(strict=True)
def pin(value):
    path=unaliased(value['path']);need(path.is_file() and digest(path)==value['sha256'],'Frozen input changed: '+str(path))
    if 'bytes' in value:need(path.stat().st_size==value['bytes'],'Frozen input length changed')
    return path
def load(path,name):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
def write(path,value):
    with Path(path).open('x',encoding='utf-8',newline='\n') as f:json.dump(value,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def final_observation(receipt,started,deadline,log,*,clock=time.monotonic,hash_file=digest):
    if log.is_file():receipt['stdout']={'path':str(log),'bytes':log.stat().st_size,'sha256':hash_file(log)}
    now=clock();receipt['elapsed_seconds']=now-started;receipt['original_phase_expired']=now>=deadline
    if receipt['failure'] is not None or receipt['post_input_drift'] or now>=deadline:receipt['complete']=False
    if now>=deadline and receipt['failure'] is None:receipt['failure']={'type':'OriginalPhaseExpired','message':'Final observations exceeded original360s cutoff'}
def source_modules():
    card=json.loads((BASE/'preparation.json').read_bytes())
    need(sys.flags.no_site==1 and sys.dont_write_bytecode and sys.pycache_prefix is not None,'Use frozen Python -S -B -X fresh pycache_prefix')
    cache=Path(sys.pycache_prefix)
    need(cache.is_absolute() and cache.parent.resolve()==BASE and cache.name=='never-created-runtime-cache' and not cache.exists(),'Fresh never-written source-bound pycache prefix required')
    for value in card['source_pins'].values():pin(value)
    need(unaliased(sys.executable).samefile(pin(card['parent_python'])),'Actual parent Python image differs')
    return card
def capacity_smoke(body,card,parent):
    need(body.get('schema')=='airk.private.compact-native-capacity-os-control/1' and body.get('complete') is True,'Actual cheap OS-capacity control absent/refused')
    need(body.get('preparation_sha256')==card['capacity_api_preparation']['sha256'] and body.get('containment_bytes')==32<<30 and body.get('reservation_bytes')==16<<30,'OS control source/limits differ')
    actual=body.get('owned',{});need(parent.settled(actual),'OS-capacity child/Job/handles did not settle naturally')
    need(actual.get('suspended_assignment',{}).get('execution_user_sid')==SID and actual.get('suspended_assignment',{}).get('image',{}).get('sha256')==card['parent_python']['sha256'],'OS control actual child SID/image differs')
    need(actual.get('before_GO',{}).get('post_exit_memory_observed') is True,'Actual retained-handle exit memory API remains unproven')
    for key in ['peak_process_rss_bytes','peak_job_committed_bytes']:
        value=body.get('actual_capacity',{}).get(key);need(type(value) is int and 0<value<=16<<30,'Original32/16 API smoke actual persistent peaks unavailable')
def frozen_plan(args,started):
    path=pin({'path':args.plan,'sha256':args.plan_sha256});plan=json.loads(path.read_bytes())
    need(plan['schema']=='airk.private.large-profiled-native-plan/1' and plan['human_sid']==SID,'Exact private plan/human identity required')
    expected={'outer_seconds':360,'native_operation_ms':120000,'resident_rss_bytes':RESERVATION,
        'resident_committed_bytes':RESERVATION,'job_containment_bytes':CONTAINMENT,'optimized_graph_bytes':GRAPH,
        'optimized_weights_bytes':WEIGHTS,'minimum_fresh_free_bytes':FREE,'sample_interval_ms':100}
    need(plan['limits']==expected,'Original reviewed limits cannot change')
    for source,sha in plan['input_seals'].items():pin({'path':source,'sha256':sha})
    card=source_modules()
    need(plan['input_seals'].get(str(Path(__file__).resolve()))==digest(Path(__file__).resolve()),'Profiling-only variant omitted/changed in actual frozen input closure')
    need(plan['input_seals'].get(str(BASE/'preparation.json'))==digest(BASE/'preparation.json'),'Owner card itself omitted/changed in frozen input closure')
    need(plan['runtime_manifest']==card['runtime_manifest'],'Frozen Python runtime manifest differs')
    for value in card['source_pins'].values():need(plan['input_seals'].get(value['path'])==value['sha256'],'Owner source omitted from frozen input closure')
    for value in [plan['native_input'],plan['image_packet'],plan['source_bridge'],plan['old_build'],plan['structural_proof'],plan['optimized_tiny_review'],plan['capacity_os_control']]:
        need(plan['input_seals'].get(value['path'])==value['sha256'],'Actual gate/input omitted from seals');pin(value)
    request=json.loads(pin(plan['native_input']).read_bytes());profile=json.loads(pin(request['profile']).read_bytes())
    need(profile['model']=='arbiter-context-qwen3-4b' and profile['manifest']==plan['profile'] and profile['memory_reservation_bytes']==RESERVATION and profile['max_tokens']==8192,'Exact new Large profile required')
    need(profile['cpu_fit']==plan['cpu_fit'] and profile['cpu_fit']=={'minimum_physical_cores':16,'intra_threads':16,'inter_threads':1,'sequential':True,'spinning':False},'Original Large CPU16 fit differs')
    need(profile.get('qualification') is None and not profile.get('qualified_targets') and not profile.get('downloads'),'Diagnostic must remain unqualified/private')
    need(request['policy']['deadline_ms']==plan['limits']['native_operation_ms'] and request['policy']['shared_input_tokens']==8192,'Rust native policy differs')
    need(request['policy']==plan['native_policy'] and request['policy_digest']==plan['native_policy_digest'],'Full Rust policy/digest mismatch')
    for role in ['profile','source_manifest','numeric_fixture']:
        value=request[role];need(plan['input_seals'].get(value['path'])==value['sha256'],'Native input pin omitted: '+role);pin(value)
    for asset in profile['assets']:
        path=request['files'][asset['name']];need(plan['input_seals'].get(path)==asset['sha256'],'Native asset omitted from seals')
        need(Path(path).stat().st_size==asset['bytes'],'Native asset length differs')
    weights=[a for a in profile['assets'] if a['role']=='external_weights']
    need(len(weights)==1 and weights[0]['bytes']==16089835520 and weights[0]['sha256']=='9da676d87747052f709c48d71f072508685c853e012330aa47de5a73e792f81c','Exact original Large external weights required')
    fixture=json.loads(pin(request['numeric_fixture']).read_bytes());need(len(fixture['rows'])==3,'Original three-row numeric fixture roster changed')
    need(all(row.get('reference_same_artifact') is None for row in fixture['rows']),'Old same-artifact references cannot be relabeled as new graph evidence')
    row=fixture['rows'][2];need(len(row['input']['input_ids'])==8192 and row['input']['attention_mask']==[1]*8192 and row['input']['position_ids']==list(range(8192)) and len(row['input']['candidate_positions'])==8 and row['input']['option_mask']==[True]*8,'Original complete8192/N8 input differs')
    need(row['input']['input_digest']==plan['numeric_input_digest'] and row.get('reference_original_fp32') is not None,'Original publisher reference/input unavailable')
    packet=json.loads(pin(plan['image_packet']).read_bytes());images=[r for r in packet['images'] if r['binary_id']=='airk-inference'];need(len(images)==1,'One preserved actual Inference image required');image=images[0];pin(image)
    need(image['preserved_file_owner_sid']==image['original_file_owner_sid']==SID,'Foreign native artifact refused')
    build=json.loads(pin(packet['actual_build_receipt']).read_bytes());old=json.loads(pin(plan['old_build']).read_bytes());bridge=json.loads(pin(plan['source_bridge']).read_bytes())
    need(build['passed'] is True and build['inputs_unchanged'] is True and packet['native_group']==build['groups']['native']['sha256'],'Actual new build failed/changed')
    old_launcher=load(card['source_pins']['previous_launcher']['path'],'compact_original_bridge_owner')
    old_launcher.check_combined_source_bridge(bridge,old['groups']['native']['sha256'],build['groups']['native']['sha256'],old['groups']['native']['files'],build['groups']['native']['files'])
    for name,sha in plan['required_native_sources'].items():need(build['groups']['native']['files'].get(name)==sha,'New image lacks exact reviewed diagnostic source: '+name)
    proof=json.loads(pin(plan['structural_proof']).read_bytes())
    graph_asset=next(a for a in profile['assets'] if a['name']=='model.onnx')
    need(proof['gqa_nodes']==36 and proof['source_sha256']=='f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94' and proof['original_initializers']==398 and proof['original_external_initializers']==326 and proof['original_scores_output_preserved_verbatim'] is True and proof['new_graph_sha256']==graph_asset['sha256'] and proof['original_initializers_and_external_ranges_preserved_verbatim'] is True and proof['all6inputs_scores_only'] is True and proof['custom_shape_gate_pending'] is False,'Actual new graph structural gate absent')
    need(graph_asset['sha256']!='f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94','Original failed graph cannot be retried as new candidate')
    sys.path.insert(0,card['reference_root']);sys.path.insert(0,card['operation_root'])
    worker=load(card['source_pins']['sid_reader']['path'],'compact_sid_reader')
    need(worker.windows_sid()==SID,'Actual execution account differs from logged-in human')
    worker.verify_loaded_runtime(plan)
    parent=load(card['source_pins']['reference_parent']['path'],'compact_reference_parent')
    capacity_smoke(json.loads(pin(plan['capacity_os_control']).read_bytes()),card,parent)
    need(len(plan['tiny_gates'])==2,'Both original tiny kernel populations required')
    for gate,tokens in zip(plan['tiny_gates'],[17,8192]):
        body=json.loads(pin(gate['body']).read_bytes());owner=json.loads(pin(gate['owner']).read_bytes())
        actual=owner.get('owned',owner.get('actual_owner_result'))
        need(owner['complete'] is True and parent.settled(actual),'Tiny kernel original process/Job/handle completion absent')
        command=owner.get('command',[]); exact=command.index('--exact') if '--exact' in command else -1
        expected_selector='inference_engine::pool::arbiter::qualification::software_v2::attention_diagnostic::software_v2_synthetic_large_gqa_allocation_t'+str(tokens)
        need(exact>=0 and exact+1<len(command) and command[exact+1]==expected_selector,'Actual Large tiny selector required; Compact tiny custody cannot transfer')
        need(actual['suspended_assignment']['image']['sha256']==image['sha256'] and actual['suspended_assignment']['execution_user_sid']==SID,'Tiny gate image/SID differs')
        need(body['tokens']==tokens and body['graph_outputs']==['context'] and body['model_weights_loaded'] is False and 0<=body['maximum_error']<=.0001,'Actual tiny kernel value/ABI proof absent')
        for key,field in [('profiling','profiling_path'),('optimized','optimized_graph_path')]:
            need(pin(gate[key]).samefile(unaliased(body[field])),'Actual tiny output pin differs')
        for value in gate.values():need(plan['input_seals'].get(value['path'])==value['sha256'],'Tiny gate byte proof omitted')
    reviewed=json.loads(pin(plan['optimized_tiny_review']).read_bytes())
    need(reviewed['schema']=='airk.private.reviewed-gqa-native-tiny-gates/1' and reviewed['complete'] is True and reviewed['native_image_sha256']==image['sha256'] and reviewed['flash_environment_value']=='0','Actual Root-reviewed tiny optimized-attribute gate absent')
    need(reviewed['tokens']==[17,8192] and reviewed['all_internal_allocated_outputs']==3 and reviewed['optimized_graphs_sha256']==[r['optimized']['sha256'] for r in plan['tiny_gates']],'Tiny optimized/allocated output population changed')
    output=Path(plan['output_root']);need(output.is_absolute() and output.parent.resolve(strict=True)==OUTPUT_PARENT.resolve() and re.fullmatch(r'large-native-profiled-execution-r[0-9]+',output.name) and not output.exists(),'Fresh approved private output root required')
    from security import observe
    actual_security=observe(output.parent)
    need(actual_security==card['approved_output_parent_security'] and actual_security['owner_sid']==SID,'Approved human output-parent owner/DACL changed')
    initial_free(shutil.disk_usage(output.parent).free)
    need(time.monotonic()<started+360,'Original outer phase expired during preflight')
    return plan,request,image,card,worker,parent
def profiling_only_result(body):
    save=body.get('optimized_save')
    need(isinstance(save,dict) and save.get('enabled') is False,'Profiling-only result must explicitly disable optimized saving')
    for value in [body,save]:
        need('optimized_graph_path' not in value and 'external_path' not in value,'Profiling-only result claims an optimized artifact')
def no_optimized_outputs(root,observed_files=()):
    need(not {'graph','weights'}.intersection(observed_files),'Optimized artifact observed during profiling-only load')
    for name in ['large-gqa-optimized.onnx','large-gqa-optimized.weights.bin']:
        try:(Path(root)/name).lstat()
        except FileNotFoundError:continue
        raise ValueError('Profiling-only optimized output must remain absent: '+name)
def native_result(body,plan,request,owned):
    need(body.get('kind')=='large_attention_diagnostic' and body.get('complete') is True and body.get('profiling_enabled') is True and body.get('diagnostic_measurement_identity')=='large-fp32-gqa-profiled/2','Diagnostic result missing/refused')
    profiling_only_result(body)
    for key,value in [('model','arbiter-context-qwen3-4b'),('process_id',owned['owned_leader_pid']),('native_binary_sha256',owned['suspended_assignment']['image']['sha256']),('input_sha256',plan['native_input']['sha256']),('profile',plan['profile']),('execution_user',SID),('source_manifest',request['source_manifest']),('profile_source_sha256',request['profile']['sha256']),('resource_policy',request['policy']),('resource_policy_digest',request['policy_digest']),('cpu_fit',plan['cpu_fit'])]:
        need(body.get(key)==value,'Native diagnostic identity differs: '+key)
    need(body.get('measurement_identity')=='native-original-operation-start-to-post-end-retention/1' and body.get('release_qualified') is False and body.get('qualification_granted') is False,'Diagnostic cannot inherit qualification or another timing boundary')
    rows=body.get('rows');need(isinstance(rows,list) and len(rows)==1 and rows[0]['input_digest']==plan['numeric_input_digest'] and rows[0]['tokens']==8192 and rows[0]['options']==8,'Exactly ONE original8192 diagnostic row required')
    scores=rows[0]['raw_scores'];need(len(scores)==8 and all(type(v) in (int,float) and math.isfinite(v) for v in scores),'Full native8score array unavailable')
    comparison=body['original_fp32_comparison'];need(type(comparison['raw']) in (float,int) and 0<=comparison['raw']<=.001 and type(comparison['mass']) in (float,int) and 0<=comparison['mass']<=.05 and comparison['argmax'] is True,'Original numerical gate refused')
def run(args):
    started=time.monotonic();deadline=started+360
    plan,request,image,card,worker,parent=frozen_plan(args,started)
    if not args.execute_reviewed:
        print(json.dumps({'complete_plan_verified':True,'model_executed':False,'outer_phase_seconds':360}));return 0
    ownership=load(card['source_pins']['reference_owner']['path'],'compact_owned_native')
    base_watch=load(card['source_pins']['operations']['path'],'compact_original_operation_watch')
    output=Path(plan['output_root']);output.mkdir();(output/'temp').mkdir()
    raw=output/'native-result.json';journal=raw.with_suffix('.operations.jsonl');stdout=output/'stdout-stderr.log'
    argv=[str(unaliased(image['path'])),'--ignored','--exact',SELECTOR,'--nocapture']
    environment=dict(os.environ);environment.update({'TEMP':str(output/'temp'),'TMP':str(output/'temp'),'TMPDIR':str(output/'temp'),
        'ORT_GQA_DISABLE_FLASH_ATTENTION':'0','AIRK_SOFTWARE_V2_NATIVE_INPUT':plan['native_input']['path'],
        'AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256':plan['native_input']['sha256'],'AIRK_SOFTWARE_V2_NATIVE_OUTPUT':str(raw)})
    receipt={'schema':'airk.private.large-profiled-owned-execution/2','diagnostic_measurement_identity':'large-fp32-gqa-profiled/2','optimized_model_saving':False,'complete':False,'model_qualified':False,'human_quality_claim':False,
        'plan_sha256':args.plan_sha256,'command':argv,'limits':plan['limits'],'failure':None,'post_input_drift':[],
        'original_phase_deadline':deadline,'selected_original_numeric_ordinals':[3],'original_numeric_fixture_rows':3,
        'not_run_by_design':[1,2],'no_warmup':True,'no_automatic_retry':True,'no_other_processes_touched':True}
    write(output/'started.json',receipt);no_optimized_outputs(output);watch=None
    def before_resume(api,job,info,child_deadline):
        nonlocal watch
        proof=parent.suspended_authentication(api,info,deadline,child_deadline,SID,image,ownership.image_proof,sid_reader=worker.windows_sid)
        initial_free(shutil.disk_usage(output).free)
        identity={'input_sha256':plan['native_input']['sha256'],'profile':plan['profile'],'process_id':int(info.pid),'native_binary_sha256':image['sha256']}
        roster=[{'kind':'load','ordinal':0},{'kind':'numeric','ordinal':3,'input_digest':plan['numeric_input_digest']}]
        watch=make_watch(base_watch,journal,stdout,identity,roster,request['policy']['deadline_ms'],output)
        watch.process,watch.job=info.process,job;watch.inspect()
        return proof
    def after_resume(api,job,info,child_deadline):
        need(child_deadline<=deadline,'Original outer deadline extended')
        return watch.run(api,job,info,deadline)
    try:
        stdin=output/'stdin.bin'
        with stdin.open('xb'):pass
        with stdin.open('rb') as input_file,stdout.open('xb') as log:
            remaining=deadline-time.monotonic();need(remaining>0,'Original360s phase expired before launch')
            owned=ownership.run_windows_owned_training(argv,output,log,remaining,environment,input_file,CONTAINMENT,
                before_resume=before_resume,after_resume=after_resume,absolute_deadline=deadline)
        receipt['owned']=owned;receipt['operation_watch']=owned['before_GO']['operation_watch'];receipt['capacity_watch']=owned['before_GO']['capacity_watch']
        need(parent.settled(owned),'Actual original child/handle/Job was not natural single-child completion')
        need(0<owned['peak_host_committed_bytes']<=RESERVATION,'Actual final OS Job peak commit exceeded40GiB reservation')
        need(re.search(r'(?m)^running 1 test\s*$',stdout.read_text(encoding='utf-8',errors='replace')) is not None,'Exactly one native selector required')
        body=json.loads(raw.read_bytes());receipt['native_body']=body;native_result(body,plan,request,owned)
        no_optimized_outputs(output,watch.capacity.files)
        for name in ['native-result.json','native-result.operations.jsonl']:
            path=output/name;receipt.setdefault('retained_outputs',[]).append({'path':str(path),'bytes':path.stat().st_size,'sha256':digest(path)})
        path=unaliased(body['profiling_path']);need(path.parent.samefile(output) and path.stat().st_size<=GRAPH,'Native profile outside bounded output');receipt['retained_outputs'].append({'path':str(path),'bytes':path.stat().st_size,'sha256':digest(path)})
        receipt['complete']=True
    except BaseException as error:
        receipt['failure']={'type':type(error).__name__,'message':str(error)}
        for key in ['owned_cleanup','operation_watch','capacity_watch']:
            if hasattr(error,key):receipt[key]=getattr(error,key)
        if watch is not None:receipt['capacity_watch']=watch.capacity.proof()
        for path in [raw,journal,*sorted(output.glob('large-gqa-profile*.json'))]:
            if path.is_file() and path.stat().st_size<=GRAPH:receipt.setdefault('partial_outputs',[]).append({'path':str(path),'bytes':path.stat().st_size,'sha256':digest(path)})
    finally:
        if watch is not None:
            try:watch.close();receipt['watch_change_handle_closed']=watch.change is None
            except BaseException as error:
                receipt['complete']=False;receipt['watch_close_error']=str(error)
                if receipt['failure'] is None:receipt['failure']={'type':'WatchCloseRefusal','message':str(error)}
        for path,sha in plan['input_seals'].items():
            try:
                if digest(path)!=sha:receipt['post_input_drift'].append(path)
            except OSError:receipt['post_input_drift'].append(path)
        try:worker.verify_loaded_runtime(plan)
        except BaseException as error:
            receipt['post_input_drift'].append('loaded Python runtime: '+str(error))
        final_observation(receipt,started,deadline,stdout)
        write(output/'execution.json',receipt)
    print(json.dumps({'complete':receipt['complete'],'output':str(output),'failure':receipt['failure'],'model_qualified':False}))
    return 0 if receipt['complete'] else 1
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--source-check',action='store_true');p.add_argument('--plan');p.add_argument('--plan-sha256');p.add_argument('--execute-reviewed',action='store_true');args=p.parse_args()
    if args.source_check:
        need(not args.execute_reviewed and args.plan is None,'Source-only check cannot execute')
        source_modules();print(json.dumps({'source_pins_verified':True,'model_or_child_executed':False}));return 0
    need(args.plan is not None and args.plan_sha256 is not None,'Actual externally reviewed complete plan and SHA required')
    return run(args)
if __name__=='__main__':sys.exit(main())
