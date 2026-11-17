# SPDX-License-Identifier: Apache-2.0
"""One reviewed same-human native primitive process; no execution without --execute-reviewed.
The immutable old image is evidence only. A new genuine source/image packet with
exact reviewed operation-observer changes is mandatory. No Cargo, model download,
source mutation, retry or cleanup deletion is performed here.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import sys
import time
sys.dont_write_bytecode=True
BASE=Path(__file__).resolve().parent

def digest(path):
    with Path(path).open('rb') as source:return hashlib.file_digest(source,'sha256').hexdigest()

def pin(path, expected):
    path=Path(path).resolve(strict=True)
    if not path.is_file() or digest(path)!=expected:raise ValueError('frozen native input changed: '+str(path))
    for parent in [path,*path.parents]:
        metadata=parent.lstat()
        if parent.is_symlink() or getattr(metadata,'st_file_attributes',0)&0x400:raise ValueError('input path redirected')
    return path

def load(path,name):
    spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

def plan_input(args, full=False):
    path=pin(args.plan,args.plan_sha256);plan=json.loads(path.read_bytes())
    if plan.get('schema')!='airk.private.software-v2-owned-native-plan/1':raise ValueError('wrong execution plan')
    if plan['mode'] not in ('resource','numeric'):raise ValueError('unreviewed native operation mode')
    for source,expected in plan['input_seals'].items():
        path=Path(source)
        if full or path.stat().st_size<=16*1024*1024:pin(source,expected)
    request=json.loads(pin(plan['native_input']['path'],plan['native_input']['sha256']).read_bytes())
    if request['policy']!=plan['policy'] or request['policy_digest']!=plan['policy_digest']:raise ValueError('actual native policy changed')
    if (plan['limits']['per_operation_ms']!=request['policy']['deadline_ms']
        or plan['limits']['log_bytes']!=64*1024*1024 or plan['limits']['operation_journal_bytes']!=131072):
        raise ValueError('observer bound differs from original plan')
    if plan['mode']=='numeric' and request['numeric_fixture'] is None:raise ValueError('independent numeric fixture unavailable')
    if plan['mode']=='resource' and request['numeric_fixture'] is not None:raise ValueError('resource-only plan unexpectedly changed')
    return plan,request

def check_combined_source_bridge(combined,old_group,new_group,prior,current):
    changes={name:current.get(name) for name in set(prior)|set(current) if prior.get(name)!=current.get(name)}
    if (combined.get('schema')!='airk.private.reviewed-native-source-bridge/1'
        or combined.get('old_native_group')!=old_group
        or combined.get('new_native_group')!=new_group):
        raise ValueError('Root-reviewed combined source bridge has different actual native groups')
    approved=combined['changes']
    if set(approved)!=set(changes):
        raise ValueError('combined source bridge omits or invents an actual per-path delta')
    for name,after in changes.items():
        row=approved[name]
        if (row.get('before_sha256')!=prior.get(name) or row.get('after_sha256')!=after
            or not isinstance(row.get('reviewed_scope'),str) or not row['reviewed_scope'].strip()):
            raise ValueError('actual source delta differs from its explicit reviewed scope: '+name)

def new_image(args,plan):
    packet_path=pin(args.image_packet,args.image_packet_sha256);packet=json.loads(packet_path.read_bytes())
    if packet['schema']!='airk.preserved-native-test-images/1':raise ValueError('genuine preserved native image packet required')
    rows=[row for row in packet['images'] if row['binary_id']=='airk-inference']
    if len(rows)!=1:raise ValueError('exact preserved Inference image required')
    image=rows[0];pin(image['path'],image['sha256'])
    if image['sha256']==plan['old_image_evidence']['image']['sha256']:
        raise ValueError('old immutable image has no original-operation journal; execution refused')
    build_pin=packet['actual_build_receipt'];build=json.loads(pin(build_pin['path'],build_pin['sha256']).read_bytes())
    old_pin=plan['old_native_build'];old=json.loads(pin(old_pin['path'],old_pin['sha256']).read_bytes())
    if build.get('passed') is not True or build.get('inputs_unchanged') is not True:
        raise ValueError('actual new build/source inventory failed or changed')
    prior=old['groups']['native']['files'];current=build['groups']['native']['files']
    changes={name:current.get(name) for name in set(prior)|set(current) if prior.get(name)!=current.get(name)}
    required=plan['required_observer_source_changes']
    if any(current.get(name)!=expected for name,expected in required.items()):
        raise ValueError('new source lacks the exact reviewed observer proposal')
    bridge_path=pin(args.source_bridge,args.source_bridge_sha256)
    combined=json.loads(bridge_path.read_bytes())
    check_combined_source_bridge(combined,old['groups']['native']['sha256'],build['groups']['native']['sha256'],prior,current)
    repository=Path(plan['repository_root']).resolve()
    for row in plan['unchanged_scoring_source_pins']:
        relative=Path(row['path']).resolve().relative_to(repository).as_posix()
        if current.get(relative)!=row['sha256']:
            raise ValueError('original NN scoring/profile/constructor/math source changed: '+relative)
    if packet['native_group']!=build['groups']['native']['sha256']:
        raise ValueError('new image/build native source group differs')
    if (image['preserved_file_owner_sid']!=plan['human_sid']
        or image['original_file_owner_sid']!=plan['human_sid']):raise ValueError('foreign native artifact custody')
    return image,{'old_native_group':old['groups']['native']['sha256'],
        'new_native_group':build['groups']['native']['sha256'],'exact_source_changes':changes,
        'new_build_receipt':build_pin,'new_image_packet':{'path':str(packet_path),'sha256':args.image_packet_sha256},
        'original_NN0_metadata_and_reference_source_pins_preserved':True,
        'combined_source_bridge':{'path':str(bridge_path),'sha256':args.source_bridge_sha256},
        'scope':'explicit Root-reviewed combined per-path bridge; unchanged scoring-source pins and new native metadata/promoter provenance remain strict'}

def write(path,value):
    with Path(path).open('x',encoding='utf-8') as output:json.dump(value,output,indent=2);output.write('\n')

def run(args,plan,request,started):
    deadline=started+plan['limits']['whole_process_seconds']
    plan,request=plan_input(args,full=True)
    image,bridge=new_image(args,plan)
    source_manifest=json.loads(pin(request['source_manifest']['path'],request['source_manifest']['sha256']).read_bytes())
    if source_manifest['native_build_receipt']!=bridge['new_build_receipt']:
        raise ValueError('native NN0 source manifest must be regenerated against the exact NEW image build receipt; old source proof is never transferred')
    sys.path.insert(0,str(BASE/'owned'))
    parent=load(BASE/'owned/reference_parent_no_site_r2.py','reviewed_native_parent')
    ownership=load(BASE/'owned/reference_owned.py','reviewed_native_job')
    if parent.worker.windows_sid()!=plan['human_sid']:raise ValueError('verified logged-in human SID required')
    from native_operations_watchdog import WindowsWatch
    output=Path(args.output).absolute()
    if output.exists() or output.parent.resolve()!=Path(plan['output_parent']).resolve():raise ValueError('fresh immediate owned run directory required')
    output.mkdir()
    temp=output/'temp';temp.mkdir()
    raw=output/'native-result.json';journal=raw.with_suffix('.operations.jsonl');log_path=output/'stdout-stderr.log'
    argv=[image['path'],'--ignored','--exact',plan['command_selector'],'--nocapture','--test-threads=1']
    environment=dict(os.environ);environment.update(plan['environment'])
    environment.update({'TEMP':str(temp),'TMP':str(temp),'AIRK_SOFTWARE_V2_NATIVE_INPUT':plan['native_input']['path'],
        'AIRK_SOFTWARE_V2_NATIVE_INPUT_SHA256':plan['native_input']['sha256'],
        'AIRK_SOFTWARE_V2_NATIVE_OUTPUT':str(raw)})
    receipt={'schema':'airk.private.owned-software-v2-native-execution/1','complete':False,'measurement_passed':False,
        'evaluation':True,'qualification_granted':False,'plan':{'path':str(Path(args.plan).resolve()),'sha256':args.plan_sha256},
        'mode':plan['mode'],'model':plan['model'],'profile':plan['profile'],'command':argv,'limits':plan['limits'],
        'cpu_fit':plan['cpu_fit'],'reservation_bytes':plan['reservation_bytes'],'source_bridge':bridge,
        'original_input_seals':plan['input_seals'],'native_image':image,'execution_sid':plan['human_sid'],
        'whole_deadline_not_renewed':True,'whole_phase_deadline_not_refreshed':True,
        'whole_phase_expired':False,'no_automatic_retry':True,'other_processes_touched':False,
        'failure':None,'post_input_drift':[],
        'input_seals':dict(plan['input_seals']) | {image['path']:image['sha256']},
        'committed_memory_passed':False,
        'coexistence_proven':False,'scope':plan['scope']}
    write(output/'started.json',receipt)
    watch=None
    def before_resume(api,job,info,child_deadline):
        nonlocal watch
        proof=parent.suspended_authentication(api,info,deadline,child_deadline,plan['human_sid'],image,ownership.image_proof)
        identity={'input_sha256':plan['native_input']['sha256'],'profile':plan['profile'],
            'process_id':int(info.pid),'native_binary_sha256':image['sha256']}
        watch=WindowsWatch(journal,log_path,identity,plan['native_operation_roster'],
            plan['policy']['deadline_ms'],plan['mode']=='numeric')
        return proof
    def after_resume(api,job,info,child_deadline):
        if child_deadline>deadline:raise ValueError('child original process deadline extended')
        return watch.run(api,info,deadline)
    try:
        with (output/'stdin.bin').open('xb'):pass
        remaining=deadline-time.monotonic()
        if remaining<=0:raise TimeoutError('original native preflight deadline exhausted')
        with (output/'stdin.bin').open('rb') as initial,log_path.open('xb') as log:
            owned=ownership.run_windows_owned_training(argv,Path(plan['repository_root']),log,remaining,
                environment,initial,plan['limits']['host_committed_bytes'],before_resume=before_resume,
                after_resume=after_resume,absolute_deadline=deadline)
        receipt['owned']=owned
        receipt['operation_watch']=owned['before_GO']['operation_watch']
        if not parent.settled(owned):raise ValueError('native direct child did not naturally settle with zero exit and empty actual Job')
        text=log_path.read_text(encoding='utf-8',errors='replace')
        if not re.search(r'(?m)^running 1 test\s*$',text):raise ValueError('actual native selector did not select exactly one test')
        body=json.loads(raw.read_bytes());receipt['native_body']=body
        if (body.get('process_id')!=owned['owned_leader_pid'] or body.get('native_binary_sha256')!=image['sha256']
            or body.get('input_sha256')!=plan['native_input']['sha256'] or body.get('profile')!=plan['profile']
            or body.get('execution_user')!=plan['human_sid'] or body.get('source_manifest')!=request['source_manifest']
            or body.get('profile_source_sha256')!=request['profile']['sha256']
            or body.get('resource_policy')!=plan['policy'] or body.get('resource_policy_digest')!=plan['policy_digest']
            or body.get('cpu_fit')!=plan['cpu_fit'] or body.get('complete') is not True
            or body.get('measurement_identity')!='native-original-operation-start-to-post-end-retention/1'):
            raise ValueError('native result/process/input/policy/measurement identity incomplete')
        if plan['mode']=='resource' and (body.get('candidate_resource_passed') is not True
            or body['resource']['complete'] is not True
            or body['resource']['attempted_samples']!=plan['policy']['required_warm_samples']):
            raise ValueError('native original full resource series incomplete')
        receipt['native_body_pin']={'path':str(raw),'sha256':digest(raw),'bytes':raw.stat().st_size}
        receipt['operation_journal_pin']={'path':str(journal),'sha256':digest(journal),'bytes':journal.stat().st_size}
        receipt['committed_memory_passed']=(isinstance(owned.get('peak_host_committed_bytes'),int)
            and not isinstance(owned['peak_host_committed_bytes'],bool)
            and 0<owned['peak_host_committed_bytes']<=plan['reservation_bytes'])
        if not receipt['committed_memory_passed']:
            raise ValueError('actual owned committed-memory exceeds native profile reservation; RSS pass cannot waive it')
        receipt['complete']=True;receipt['measurement_passed']=True
    except BaseException as error:
        receipt['failure']={'type':type(error).__name__,'message':str(error)}
        if hasattr(error,'operation_watch'):receipt['operation_watch']=error.operation_watch
        if hasattr(error,'owned_cleanup'):receipt['owned_cleanup']=error.owned_cleanup
        for label,path in [('native_body_pin',raw),('operation_journal_pin',journal)]:
            if path.is_file():receipt[label]={'path':str(path),'bytes':path.stat().st_size,'sha256':digest(path)}
    finally:
        if watch is not None:
            try:watch.close()
            except BaseException as error:
                receipt['watch_handle_close_failure']={'type':type(error).__name__,'message':str(error)}
                receipt['complete']=False;receipt['measurement_passed']=False
                if receipt['failure'] is None:receipt['failure']={'type':'WatchHandleCloseRefusal','message':str(error)}
        for path,expected in receipt['input_seals'].items():
            try:
                if digest(path)!=expected:receipt['post_input_drift'].append(path)
            except OSError:receipt['post_input_drift'].append(path)
        receipt['whole_phase_expired']=time.monotonic()>deadline
        if receipt['post_input_drift'] or receipt['whole_phase_expired']:
            receipt['complete']=False;receipt['measurement_passed']=False
            if receipt['failure'] is None:receipt['failure']={'type':'FinalObservationRefusal','message':'original input/whole-phase observation failed'}
        if log_path.is_file():
            receipt['stdout']={'path':str(log_path),'sha256':digest(log_path),'bytes':log_path.stat().st_size}
        receipt['elapsed_seconds']=time.monotonic()-started
        write(output/'execution.json',receipt)
    print(json.dumps({'output':str(output),'measurement_passed':receipt['measurement_passed'],
        'qualification_granted':False,'failure':receipt['failure']}))
    return 0 if receipt['measurement_passed'] else 1

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan',required=True);parser.add_argument('--plan-sha256',required=True)
    parser.add_argument('--execute-reviewed',action='store_true')
    parser.add_argument('--image-packet');parser.add_argument('--image-packet-sha256');parser.add_argument('--output')
    parser.add_argument('--source-bridge');parser.add_argument('--source-bridge-sha256')
    args=parser.parse_args()
    started=time.monotonic()
    plan,request=plan_input(args)
    if not args.execute_reviewed:
        print(json.dumps({'plan_metadata_verified':True,'large_asset_hashes_deferred':True,'mode':plan['mode'],'model':plan['model'],
            'native_or_model_execution':False,'new_observer_image_required':True}));return 0
    if not args.image_packet or not args.image_packet_sha256 or not args.output or not args.source_bridge or not args.source_bridge_sha256:
        raise ValueError('actual reviewed image, combined source bridge pins and fresh output are mandatory')
    return run(args,plan,request,started)

if __name__=='__main__':sys.exit(main())
