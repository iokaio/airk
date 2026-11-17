# SPDX-License-Identifier: Apache-2.0
"""One explicit fresh R154 diagnostic under the maintained same-human multiprocess owner."""
import hashlib,importlib.util,json,os,sys,time
from pathlib import Path
BASE=Path('D:\\code\\Github\\airk\\artifacts\\advanced-split\\models\\actual-composed-service-r2\\r154-fresh-attribution-preparation-r3')

def load(path,name):
    spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module

def natural(owned):
    expected={'exit_code':0,'timed_out':False,'log_oversized':False,
        'owned_descendants_settled':True,'owned_job_forced_termination':False,
        'owned_job_final_active_processes':0,'owned_job_final_member_pids':[],
        'parent_assigned_to_job':False,'child_assigned_before_resume':True}
    return all(type(owned.get(k)) is type(v) and owned.get(k)==v for k,v in expected.items())

def ownership_settled(value):
    expected={'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,
        'owned_job_forced_termination':False,'owned_job_final_active_processes':0,
        'owned_job_final_member_pids':[],'parent_assigned_to_job':False,'child_assigned_before_resume':True}
    return (type(value.get('exit_code')) is int
        and all(type(value.get(k)) is type(v) and value[k]==v for k,v in expected.items()))

def ownership_outcome(value):
    if not ownership_settled(value):return 'unsettled_or_unknown'
    return 'naturally_settled_success' if value['exit_code']==0 else 'naturally_settled_nonzero'

def retain_child_report(root,report):
    path=root/'worker/diagnostic.json'
    with path.open('rb') as stream:raw=stream.read(8*1024*1024+1)
    if len(raw)>8*1024*1024:raise ValueError('Worker diagnostic exceeds8MiB')
    child=json.loads(raw)
    report['diagnostic']={'path':str(path),'sha256':hashlib.sha256(raw).hexdigest(),'full_result':child}
    return child

def observe_child_shutdown(child,report,handle,cutoff):
    proof=child.get('fresh_service_process_settlement',{})
    if (child.get('native_service_shutdown',{}).get('full_response',{}).get('status')=='stopping'
        and proof.get('exit_observed') is True and type(proof.get('exit_code')) is int and proof['exit_code']==0
        and proof.get('handle_closed') is True
        and proof.get('process_created_filetime')==report['original_service_before']['process_created_filetime']):
        # The child proof only permits an observation of OUR retained HANDLE.
        # It never substitutes its exit claim for the observer's actual wait.
        report['original_service_after']=handle.observe_original_exit(cutoff)

def main():
    started=time.monotonic()
    sys.path.insert(0,str(BASE/'helpers'));sys.path.insert(0,str(BASE/'owned'))
    diagnostic=load(BASE/'r154_diagnostic.py','r154_owned_diagnostic')
    args=diagnostic.parser().parse_args()
    frozen=(BASE/'preparation.json').read_bytes()
    packet=diagnostic.preparation(args.preparation_sha256,frozen)
    if not args.execute_new_diagnostic:
        return diagnostic.main()
    diagnostic.require(args.phase_seconds==360 and args.owned_started is None and args.owned_deadline is None,
        'One fresh original360s owner cutoff required; caller cannot set descendant clock')
    cutoff=started+args.phase_seconds
    import reference_worker,reference_owned,reference_parent_no_site_r2,custody,windows_job_8g
    diagnostic.require(reference_worker.windows_sid()==diagnostic.SID,'Same verified human parent required')
    ready_path=diagnostic.pinned(args.fixture_ready,args.fixture_ready_sha256)
    ready=json.loads(ready_path.read_bytes())
    diagnostic.require(ready.get('schema')=='airk.private-arbiter-fixture/1' and ready.get('evaluation') is True
        and ready.get('release_qualified') is False,'Actual isolated private fixture required')
    diagnostic.require(ready['health']['executable_sha256']==args.service_executable_sha256,'Actual native fixture image differs')
    root=Path(args.output).absolute()
    diagnostic.require(not root.exists() and root.parent.resolve().is_relative_to(ready_path.parent.resolve()),
        'Fresh output must stay in original owner-private fixture work')
    diagnostic.require(not root.resolve().is_relative_to(Path(packet['source_root']).resolve()),'Output cannot modify frozen source')
    diagnostic.require(time.monotonic()<cutoff,'Original phase expired during input preflight')
    root.mkdir();(root/'temp').mkdir()
    handle=None;report={'schema':'airk.private-r154-owned-diagnostic/1','classification':packet['classification'],
        'complete':False,'qualification_granted':False,'original_call_retried':False,
        'original_started_monotonic':started,'original_deadline_monotonic':cutoff,
        'original_preparation_sha256':args.preparation_sha256,'other_processes_touched':False,
        'no_automatic_retry':True,'failure':None}
    python={'path':str(Path(sys.executable).resolve()),'sha256':diagnostic.digest(sys.executable)}
    # Child command derives from the same native-facing parser; no native DTOs.
    argv=[python['path'],'-S','-B',str(BASE/'r154_diagnostic.py'),*sys.argv[1:]]
    position=argv.index('--output');argv[position+1]=str(root/'worker')
    argv.extend(['--owned-started',repr(started),'--owned-deadline',repr(cutoff)])
    report['command']=argv;report['python']=python
    def before_resume(api,job,info,child_deadline):
        return reference_parent_no_site_r2.suspended_authentication(api,info,cutoff,child_deadline,
            diagnostic.SID,python,reference_owned.image_proof)
    try:
        handle=custody.OriginalProcess(ready['health']['process_id'],
            {'path':args.service_executable_path,'sha256':args.service_executable_sha256},
            diagnostic.SID,reference_worker.windows_sid)
        report['original_service_before']=dict(handle.proof)
        env=dict(os.environ);env.update({'TEMP':str(root/'temp'),'TMP':str(root/'temp')})
        remaining=cutoff-time.monotonic();diagnostic.require(remaining>0,'Original prelaunch cutoff expired')
        with (root/'stdout-stderr.log').open('xb') as log:
            owned=windows_job_8g.run_windows_owned(argv,BASE,log,remaining,env,
                absolute_deadline=cutoff,before_resume=before_resume)
        report['owned']=owned
        report['owned_outcome']=ownership_outcome(owned)
        diagnostic.require(ownership_settled(owned),'Actual diagnostic subprocess/descendant ownership unsettled or unknown')
        child=retain_child_report(root,report)
        observe_child_shutdown(child,report,handle,cutoff)
        diagnostic.require(natural(owned),'Actual diagnostic naturally settled with nonzero exit '+str(owned['exit_code'])+'; original child failure retained')
        diagnostic.require(child.get('complete') is True and child.get('canonical_call_attempts')==1 and child.get('fresh_call_validated') is True
            and child.get('prepared_sources_and_inputs_unchanged') is True
            and child.get('original_phase_expired') is False
            and child.get('native_service_shutdown',{}).get('full_response',{}).get('status')=='stopping',
            'Full fresh one-Answer/input/native-shutdown capture absent or refused')
        if 'original_service_after' not in report:report['original_service_after']=handle.observe_original_exit(cutoff)
        diagnostic.require(child.get('fresh_service_process_settlement',{}).get('process_created_filetime')
            ==report['original_service_before']['process_created_filetime'],
            'Worker original service process identity differs from outer retained HANDLE')
        report['complete']=True
    except BaseException as error:
        report['failure']={'type':type(error).__name__,'message':str(error)}
    finally:
        if handle is not None and handle.handle is not None:
            closed=handle.api.CloseHandle(handle.handle);handle.handle=None
            report['unsettled_service_observer_handle_closed']=bool(closed)
            report['original_service_after']={**handle.proof,'handle_closed':bool(closed)}
            if not closed and report['failure'] is None:report['failure']={'type':'HandleCloseRefusal','message':'Observer handle close failed'}
        # Worker owns all753-source/SDK/post-shutdown hashes under the actual Job.
        # This final small retained output hash also precedes the sole final clock.
        log_path=root/'stdout-stderr.log'
        if log_path.is_file():report['stdout']={'path':str(log_path),'sha256':diagnostic.digest(log_path),'bytes':log_path.stat().st_size}
        observed=time.monotonic();report['elapsed_seconds']=observed-started
        report['original_phase_expired']=observed>=cutoff
        if report['original_phase_expired'] or report['failure'] is not None:report['complete']=False
        if report['original_phase_expired'] and report['failure'] is None:report['failure']={'type':'OriginalPhaseExpired','message':'Final retained observations exceeded original360s cutoff'}
        diagnostic.write(root/'owned-execution.json',report)
    print(json.dumps({'output':str(root),'complete':report['complete'],'qualification_granted':False,'failure':report['failure']}))
    return 0 if report['complete'] else 1

if __name__=='__main__':sys.exit(main())
