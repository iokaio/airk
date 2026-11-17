# SPDX-License-Identifier: Apache-2.0
"""Prepared actual Windows-only API smoke; no model/native/Cargo/ORT. Explicit review required."""
import argparse,ctypes,json,os,re,sys,time
from pathlib import Path
from runner import BASE,SID,need,source_modules,load,unaliased,pin,write,digest,final_observation
from capacity import Capacity,WindowsRead,CONTAINMENT,RESERVATION
from security import observe

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--output',required=True);p.add_argument('--execute-reviewed',action='store_true');a=p.parse_args()
    need(a.execute_reviewed,'Explicit reviewed OS-only child execution required')
    started=time.monotonic();deadline=started+30;card=source_modules()
    sys.path.insert(0,card['reference_root'])
    worker=load(card['source_pins']['sid_reader']['path'],'capacity_control_sid')
    parent=load(card['source_pins']['reference_parent']['path'],'capacity_control_parent')
    owner=load(card['source_pins']['reference_owner']['path'],'capacity_control_owner')
    seals={r['path']:r['sha256'] for r in card['source_pins'].values()}
    runtime={'runtime_manifest':card['runtime_manifest'],'input_seals':seals}
    worker.verify_loaded_runtime(runtime);need(worker.windows_sid()==SID,'Wrong human execution identity')
    root=Path(a.output)
    need(root.is_absolute() and root.parent.resolve(strict=True)==BASE.parent and re.fullmatch(r'compact-native-capacity-control-r[0-9]+',root.name) and not root.exists(),'Fresh approved private OS-control root required')
    need(observe(root.parent)==card['approved_output_parent_security'],'Output-parent owner/DACL changed')
    root.mkdir();raw=root/'actual-owned-file.bin';log=root/'stdout.log';stdin=root/'stdin.bin'
    with stdin.open('xb'):pass
    argv=[str(pin(card['parent_python'])),'-S','-B','-c',"from pathlib import Path;import sys;Path(sys.argv[1]).write_bytes(b'owned capacity API control')",str(raw)]
    reader=WindowsRead();capacity=Capacity();exit_memory_observed=False
    receipt={'schema':'airk.private.compact-native-capacity-os-control/1','complete':False,'failure':None,'post_input_drift':[],
        'preparation_sha256':digest(BASE/'preparation.json'),'command':argv,'child_model_or_native_runs':0,'outer_seconds':30,'full_model_limits_changed':False,
        'containment_bytes':CONTAINMENT,'reservation_bytes':RESERVATION}
    def sample(info,job):
        files={};v=reader.file(raw)
        if v is not None:files['body']=v
        capacity.observe(files,reader.memory(info.process,job))
    def before(api,job,info,child_deadline):
        proof=parent.suspended_authentication(api,info,deadline,child_deadline,SID,card['parent_python'],owner.image_proof,sid_reader=worker.windows_sid)
        sample(info,job);return proof
    def after(api,job,info,child_deadline):
        nonlocal exit_memory_observed
        while True:
            sample(info,job);remaining=deadline-time.monotonic();need(remaining>0,'Original30s OS-only control expired')
            result=api.WaitForSingleObject(info.process,max(1,min(100,int(remaining*1000))))
            if result==0:
                sample(info,job);exit_memory_observed=True
                return {'actual_capacity_control':capacity.proof(),'post_exit_memory_observed':True},[]
            if result!=258:raise ctypes.WinError(ctypes.get_last_error())
    try:
        with stdin.open('rb') as inp,log.open('xb') as out:
            actual=owner.run_windows_owned_training(argv,root,out,deadline-time.monotonic(),dict(os.environ),inp,CONTAINMENT,
                before_resume=before,after_resume=after,absolute_deadline=deadline)
        receipt['owned']=actual;need(parent.settled(actual),'Original single child/Job/handle did not settle naturally')
        need(exit_memory_observed and 0<actual['peak_host_committed_bytes']<=RESERVATION,'Actual exit Job peak unavailable/outside reservation')
        need(raw.read_bytes()==b'owned capacity API control','Actual child-owned file differs')
        receipt['actual_owned_file']={'path':str(raw),'bytes':raw.stat().st_size,'sha256':digest(raw)};receipt['complete']=True
    except BaseException as error:
        receipt['failure']={'type':type(error).__name__,'message':str(error)}
        if hasattr(error,'owned_cleanup'):receipt['owned_cleanup']=error.owned_cleanup
    finally:
        receipt['actual_capacity']=capacity.proof()
        try:worker.verify_loaded_runtime(runtime)
        except BaseException as error:receipt['post_input_drift'].append(str(error))
        if digest(BASE/'preparation.json')!=receipt['preparation_sha256']:receipt['post_input_drift'].append('source preparation card')
        for row in card['source_pins'].values():
            try:pin(row)
            except BaseException:receipt['post_input_drift'].append(row['path'])
        final_observation(receipt,started,deadline,log)
        write(root/'execution.json',receipt)
    print(json.dumps({'complete':receipt['complete'],'failure':receipt['failure'],'output':str(root)}))
    return 0 if receipt['complete'] else 1
if __name__=='__main__':sys.exit(main())
