# SPDX-License-Identifier: Apache-2.0
"""Reviewed30s non-neural graph creation owner. No execution without explicit flag."""
import argparse,hashlib,json,os,sys,time
from pathlib import Path
BASE=Path(__file__).resolve().parent
from gate_protocol import fresh_child_cwd

def need(ok,message):
    if not ok:raise ValueError(message)
def digest(path):
    with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def retain(path,value):
    with path.open('x',encoding='utf-8') as f:json.dump(value,f,indent=2);f.write('\n');f.flush();os.fsync(f.fileno())
def main():
    started=time.monotonic();p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--plan',required=True);p.add_argument('--plan-sha256',required=True);p.add_argument('--execute-reviewed',action='store_true');a=p.parse_args()
    need(digest(a.plan)==a.plan_sha256,'Exact externally reviewed graph plan required');plan=json.loads(Path(a.plan).read_bytes())
    need(plan['whole_seconds']==30 and plan['graph_only'] is True,'Original frozen graph-only30s cap required');deadline=started+30
    for path,expected in plan['input_seals'].items():need(digest(path)==expected,'Frozen graph producer input changed: '+path)
    if not a.execute_reviewed:print(json.dumps({'plan_verified':True,'graph_created':False,'model_forward':False}));return 0
    sys.path.insert(0,plan['owner_root']);import reference_worker,reference_owned,reference_parent_no_site_r2,windows_job_8g
    need(reference_worker.windows_sid()==plan['human_sid'],'Verified same-human owner required')
    root=Path(plan['output_root']);need(not root.exists() and root.parent.resolve()==BASE.parent.resolve(),'Fresh protected graph output directory required')
    root.mkdir();(root/'temp').mkdir()
    argv=[plan['python']['path'],'-S','-B',str(BASE/'transformer.py'),'--source',plan['source']['path'],
        '--source-sha256',plan['source']['sha256'],'--python-site',plan['python_site'],'--output',str(root/'model.onnx'),
        '--cpu-shape-source',plan['cpu_shape_source']['path'],'--cpu-shape-source-sha256',plan['cpu_shape_source']['sha256'],
        '--execution-plan',str(Path(a.plan).resolve()),'--execution-plan-sha256',a.plan_sha256,'--execute-graph-only']
    env=dict(os.environ);env.update({'TEMP':str(root/'temp'),'TMP':str(root/'temp'),'PYTHONPYCACHEPREFIX':str(root/'fresh-unwritten-pycache')})
    report={'schema':'airk.private.large-graph-owner/1','complete':False,'graph_only':True,'model_forward':False,
        'command':argv,'original_started':started,'original_deadline':deadline,'no_automatic_retry':True,'failure':None}
    def before_resume(api,job,info,child_deadline):
        return reference_parent_no_site_r2.suspended_authentication(api,info,deadline,child_deadline,
            plan['human_sid'],plan['python'],reference_owned.image_proof)
    try:
        remaining=deadline-time.monotonic();need(remaining>0,'Original graph phase expired before launch')
        with (root/'stdout.log').open('xb') as log:
            owned=windows_job_8g.run_windows_owned(argv,fresh_child_cwd(root,BASE),log,remaining,env,absolute_deadline=deadline,before_resume=before_resume)
        report['owned']=owned
        expected={'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,
            'owned_job_forced_termination':False,'owned_job_final_active_processes':0,'owned_job_final_member_pids':[],
            'parent_assigned_to_job':False,'child_assigned_before_resume':True}
        need(all(type(owned.get(k)) is type(v) and owned.get(k)==v for k,v in expected.items()),'Actual graph worker/Job did not naturally settle')
        graph=root/'model.onnx';proof_path=graph.with_suffix('.proof.json');proof=json.loads(proof_path.read_bytes())
        need(digest(graph)==proof['new_graph_sha256'] and proof['custom_shape_gate_pending'] is False,'Actual graph/strict shape proof absent')
        report['graph']={'path':str(graph),'sha256':digest(graph),'bytes':graph.stat().st_size}
        report['shape_proof']={'path':str(proof_path),'sha256':digest(proof_path)};report['complete']=True
    except BaseException as error:report['failure']={'type':type(error).__name__,'message':str(error)}
    finally:
        drift=[]
        for path,expected in plan['input_seals'].items():
            try:
                if digest(path)!=expected:drift.append(path)
            except OSError:drift.append(path)
        report['post_input_drift']=drift
        log=root/'stdout.log'
        if log.is_file():report['stdout']={'path':str(log),'sha256':digest(log),'bytes':log.stat().st_size}
        now=time.monotonic();report['elapsed_seconds']=now-started;report['original_phase_expired']=now>=deadline
        if drift or now>=deadline or report['failure'] is not None:report['complete']=False
        retain(root/'owned-execution.json',report)
    print(json.dumps({'output':str(root),'complete':report['complete'],'model_forward':False,'failure':report['failure']}))
    return 0 if report['complete'] else 1
if __name__=='__main__':sys.exit(main())
