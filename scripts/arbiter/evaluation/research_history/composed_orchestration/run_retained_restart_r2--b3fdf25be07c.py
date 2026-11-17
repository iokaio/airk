# SPDX-License-Identifier: Apache-2.0
"""One separate explicitly authorized restart phase on preserved R5 U93 state."""
import argparse,ctypes,hashlib,importlib.util,json,os,subprocess,sys,time
from pathlib import Path
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
BASE=Path(__file__).resolve().parent

def require(ok,message):
    if not ok:raise ValueError(message)
def sha(path):
    with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
def write(path,value):
    with Path(path).open('x',encoding='utf-8') as f:json.dump(value,f,indent=2);f.write('\n')
def left(cutoff):
    value=cutoff-time.monotonic()
    if value<=0:raise TimeoutError('original retained restart phase cutoff expired')
    return value

def verify_census(census):
    root=Path(census['root']).resolve(strict=True)
    expected={os.path.normcase(str(Path(r['path']).resolve(strict=True))):r for r in census['entries']}
    actual={os.path.normcase(str(p.resolve(strict=True))) for p in [root,*root.rglob('*')]}
    require(actual==set(expected),'retained state census added/removed paths')
    for name,row in expected.items():
        p=Path(row['path']);st=p.stat(follow_symlinks=False)
        require(not p.is_symlink() and not (getattr(st,'st_file_attributes',0)&0x400),'redirected retained input')
        require(p.is_dir()==row['directory'],'retained input kind changed')
        if not row['directory']:
            require(st.st_nlink==1 and st.st_size==row['length'] and sha(p)==row['sha256'],'retained file custody/bytes changed: '+str(p))
    return root


def prior_proof_values(native,stop,exited,job):
    require(stop['status']=='lifecycle' and stop['record']['phase']=='complete'
        and stop['record']['request']['action']=='stop' and stop['record']['request']['manager'] is False
        and stop['record']['request']['expected_instance']==native['previous_instance']
        and stop['record']['request']['runtime']==native['runtime_sha256'], 'original R5 native StopComplete differs')
    require(exited['pid']==native['previous_pid'] and exited['process_handle_signaled'] is True
        and exited['exit_code']==0,'original R5 actual PID exit proof absent')
    require(job['owned_descendants_settled'] is True and job['owned_job_forced_termination'] is False
        and job['owned_job_final_active_processes']==0 and job['owned_job_final_member_pids']==[]
        and job['timed_out'] is False and job['exit_code']==1,'original failed R5 Job settlement differs')


def prior_proof(card):
    value=lambda key:json.loads(Path(card['prior'][key]).read_text(encoding='utf-8'))
    require(value('child_receipt')['passed'] is False and value('owner_receipt')['passed'] is False,
        'original R5 failure was retagged')
    prior_proof_values(card['native'],value('stop'),value('exit'),value('owner_receipt')['owner'])


def validate(card):
    require(card['schema']=='airk.private-retained-r5-restart-card/1' and card['phase_seconds']==600
        and card['native_seconds']==120 and card['cleanup_seconds']==5,'unreviewed restart policy')
    require(card['scope']=='separate isolated U93 explicit restart; original R5 remains failed','restart scope differs')
    require(0<len(card['pins'])<=4096,'pin bound')
    names=[os.path.normcase(str(Path(r['path']).resolve(strict=True))) for r in card['pins']]
    require(len(names)==len(set(names)),'duplicate pin')
    for row in card['pins']:require(sha(row['path'])==row['sha256'],'pinned input changed: '+row['path'])
    prior_proof(card)
    verify_census(json.loads(Path(card['census']).read_text(encoding='utf-8')))
    return card

def explicit_start(fixture,cutoff,record):
    result=fixture.service_run('runtime','start','--install-dir',fixture.install,
        check=False,timeout=min(120,left(cutoff)))
    record('raw-explicit-start',{'stdout':result.stdout,'stderr':result.stderr,'exit_code':result.returncode})
    require(result.returncode==0,'native explicit Start refused; original raw result retained')
    observed=fixture.rpc('health',deadline=cutoff)
    record('actual-health-response',observed)
    require(isinstance(observed,dict) and isinstance(observed.get('observation'),dict),'actual Health unavailable; never synthesize')
    return observed['observation']

def cleanup(fixture,health,settled,attempted,stop_cutoff,cutoff,record,report):
    if fixture is None or health is None:
        report['cleanup_decision']='no actual current Health; native outcome unobserved'
        return stop_cutoff
    if settled:
        report['cleanup_decision']='native StopComplete and original PID exit already observed'
        return stop_cutoff
    if attempted:
        report['cleanup_decision']='Stop already submitted; never repeat uncertain request'
        return stop_cutoff
    if health.get('user')!=SID:
        report['cleanup_failure']={'type':'ValueError','message':'cleanup cannot adopt another user'}
        return stop_cutoff
    stop_cutoff=min(cutoff,time.monotonic()+120) if stop_cutoff is None else stop_cutoff
    fixture.stop_cutoff=stop_cutoff
    try:
        fixture.close();record('cleanup-native-stop',fixture.managed_stop)
        report['actual_native_stop']=fixture.managed_stop
    except BaseException as error:report['cleanup_failure']={'type':type(error).__name__,'message':str(error)}
    return stop_cutoff

def child(card_path,expected,cutoff):
    require(SID in subprocess.check_output([r'C:\Windows\System32\whoami.exe','/user','/fo','csv','/nh'],text=True),'child human identity differs')
    require(sha(card_path)==expected,'card differs')
    card=validate(json.loads(Path(card_path).read_text(encoding='utf-8')))
    require(0<left(cutoff)<=600,'original phase cutoff required')
    output=Path(card['child_output']);owner=Path(card['controller_owner'])
    require(not output.exists() and not owner.exists(),'fresh controller outputs required')
    output.mkdir();owner.mkdir()
    report={'schema':'airk.private-retained-r5-restart-run/1','passed':False,'original_R5_whole_journey_passed':False,
        'release_qualified':False,'scope':card['scope'],'steps':[],'focus_work_repeated':False}
    def record(label,value):
        write(output/(f'{len(report["steps"]):03d}-'+label+'.json'),value);report['steps'].append(label)
    began=time.monotonic();fixture=None;health=None;handle=None;settled=False;attempted=False;stop_cutoff=None
    r5=Path(card['r5_recipe']);R=load('retained_r5_helpers',r5/'run_public_focus_r5.py')
    A=load('retained_assertions',r5/'public_focus_evidence_r5.py')
    E=load('retained_evidence',r5/'public-focus-r5-helper-source/scripts/combined_evidence.py')
    Q=load('retained_oracle',r5/'public-focus-r5-helper-source/scripts/check-retrieval-quality.py')
    clients=load('retained_native_clients',r5/'public-focus-r5-helper-source/scripts/check-service-clients.py')
    class RetainedControl(clients.Fixture):
        stop_cutoff=None
        def execute(self,command,*,input=None,check=True,timeout=None):
            bound=min(cutoff,self.stop_cutoff if self.stop_cutoff is not None else cutoff)
            return super().execute(command,input=input,check=check,timeout=min(timeout or 120,left(bound)))
        def rpc(self,method,*,deadline=None):
            result=self.run('service','rpc','--runtime-dir',self.control,'--stdin',input=json.dumps(method),
                check=False,timeout=min(120,left(min(cutoff,deadline if deadline is not None else cutoff))))
            record('raw-native-rpc',{'method':method,'stdout':result.stdout,'stderr':result.stderr,'exit_code':result.returncode})
            require(result.returncode==0,'native RPC refused');return json.loads(result.stdout)
    try:
        # Fresh owner constructs only logs/private Git environment. Never calls
        # Fixture.start/stage/activation or substitutes any persisted state.
        fixture=RetainedControl(Path(card['runtime']),owner,preserve_pair_metadata=True)
        inspection=fixture.service_run('runtime','inspect','--install-dir',card['retained_install'],check=False,timeout=min(120,left(cutoff)))
        record('raw-retained-inspection',{'stdout':inspection.stdout,'stderr':inspection.stderr,'exit_code':inspection.returncode})
        require(inspection.returncode==0,'retained native inspection refused')
        view=json.loads(inspection.stdout)
        require(view['selection']['active']==card['native']['runtime_sha256'] and view['payload_verified'] is True
            and Q.location_key(view['control_dir'])==Q.location_key(card['retained_control'])
            and Q.location_key(view['executable'])==Q.location_key(card['retained_main'])
            and view['registry_namespace']==card['native']['namespace'] and view['lifecycle']['explicitly_stopped'] is True,
            'retained native owner/selection/control/stop intent differs')
        require(sha(view['executable'])==card['native']['airk_sha256'],'actual selected image differs')
        # Explicit, reviewed attachment after native Inspect: only in-memory
        # controller paths; native admission still owns all state mutation.
        fixture.install=Path(card['retained_install']);fixture.control=Path(card['retained_control'])
        startup_cutoff=min(cutoff,time.monotonic()+120)
        health=explicit_start(fixture,startup_cutoff,record)
        fixture.managed_health=health;fixture.managed_started=True
        record('actual-start',health)
        handle=R.ProcessExit(health['process_id'])
        A.require_health_source(health,SID,card['native']['source_sha256'])
        require(health['executable_sha256']==card['native']['airk_sha256']
            and health['runtime_sha256']==card['native']['runtime_sha256']
            and health['version']=='1.8.207' and health['instance']!=card['native']['previous_instance']
            and health['process_id']!=card['native']['previous_pid'],'same-image new native lease/instance absent')
        fixture.managed_health=health;fixture.managed_started=True
        status=R.capture_status(lambda:fixture.run('--service-runtime-dir',fixture.control,'arbiter','status',check=False,
            timeout=min(120,left(cutoff))),record)
        require(status['configuration']['focus']['enabled'] is False and R.profile_row(status)['availability']=='missing_assets',
            'persisted disabled/removed state absent')
        oracles=json.loads(Path(card['oracles']).read_text(encoding='utf-8'))
        for name in ('a','b'):
            oracle=oracles[name];oracle['sources']={(r['index_id'],r['source_root_id'],r['source_relative_path']):
                {k:v for k,v in r.items() if k not in ('index_id','source_root_id','source_relative_path')} for r in oracle['sources']}
            index=oracle['selection']['index_id'];ready=json.loads(Path(card['ready'][name]).read_text(encoding='utf-8'))
            for kind in ('diagnostics','search'):
                request=Path(card['requests'][name]).read_bytes() if kind=='search' else b'{"response_mode":"full"}'
                result=fixture.run('--service-runtime-dir',fixture.control,'tools','call',
                    'search' if kind=='search' else 'airk_diagnostics','--index',index,'--args',request.decode('utf-8'),
                    check=False,timeout=min(120,left(cutoff)))
                record('raw-'+kind+'-'+name,{'stdout':result.stdout,'stderr':result.stderr,'exit_code':result.returncode,'request_arguments':json.loads(request)})
                require(result.returncode==0,'native ordinary MiniLM read refused')
                observed=A.observe(json.loads(result.stdout),index,oracle,E,Q,kind=kind,request_arguments=json.loads(request),
                    expected_publications=ready['publication_identity'],expected_scope=ready['member_body']['scope_observation'])
                record('validated-'+kind+'-'+name,observed)
                if kind=='search':
                    A.require_hybrid(observed);require(not A.applied_focus(observed),'removed Focus unexpectedly applied')
        require(all(sha(r['path'])==r['sha256'] for r in card['read_only_sources']),'original source/request bytes changed')
        stop_cutoff=min(cutoff,time.monotonic()+120);fixture.stop_cutoff=stop_cutoff;attempted=True
        fixture.close();record('actual-native-stop',fixture.managed_stop)
        report['actual_native_stop']=fixture.managed_stop
        exited=handle.wait(stop_cutoff);record('actual-original-process-exit',exited)
        report['actual_original_process_exit']=exited;settled=True
        report['source_files_unchanged']=True;report['actual_new_instance_same_image_lease_reacquisition']=True
        report['ordinary_minilm_reads']=2;report['passed']=True
    except BaseException as error:
        report['failure']={'type':type(error).__name__,'message':str(error)};raise
    finally:
        stop_cutoff=cleanup(fixture,health,settled,attempted,stop_cutoff,cutoff,record,report)
        if handle is not None:
            report['cleanup_process_exit']=R.retain_original_exit(handle,stop_cutoff if stop_cutoff is not None else cutoff);handle.close()
        if fixture is not None:fixture.log.close()
        report['elapsed_seconds']=time.monotonic()-began;write(output/'receipt.json',report)


def outer(card_path,expected):
    require(os.name=='nt','Windows only')
    require(SID in subprocess.check_output([r'C:\Windows\System32\whoami.exe','/user','/fo','csv','/nh'],text=True),'wrong human identity')
    began=time.monotonic();cutoff=began+600
    require(sha(card_path)==expected,'card differs');card=validate(json.loads(Path(card_path).read_text(encoding='utf-8')))
    output=Path(card['outer_output']);require(not output.exists(),'fresh outer output required');output.mkdir()
    report={'schema':'airk.private-retained-r5-restart-owner/1','passed':False,'original_R5_whole_journey_passed':False,
        'scope':card['scope'],'execution_card':{'path':str(card_path),'sha256':expected}}
    try:
        owner=load('retained_multichild_owner',card['owner_source'])
        argv=[card['python'],'-B',str(Path(__file__).resolve()),'--card',str(card_path),'--card-sha256',expected,
            '--owned-child','--whole-cutoff',str(cutoff)]
        result=owner.run_owned(argv,card['repository'],output/'owned-child.log',left(cutoff),{**os.environ,**card['environment']})
        report['owner']=result
        result_path=Path(card['child_output'])/'receipt.json';receipt=json.loads(result_path.read_text(encoding='utf-8')) if result_path.is_file() else None
        report['child_receipt_observation']=receipt
        R=load('retained_owner_acceptance',Path(card['r5_recipe'])/'run_owned_public_focus_r5.py')
        require(R.natural_owner(result),'owned restart child/Job did not naturally settle')
        require(receipt is not None and receipt['passed'] is True and receipt['focus_work_repeated'] is False
            and receipt['actual_original_process_exit']['process_handle_signaled'] is True
            and receipt['actual_original_process_exit']['exit_code']==0
            and receipt['actual_native_stop']['record']['phase']=='complete','actual native restart/Stop/exit proof absent')
        report['passed']=True;report['child_receipt_sha256']=sha(result_path)
    except BaseException as error:
        report['failure']={'type':type(error).__name__,'message':str(error)};raise
    finally:
        report['elapsed_seconds']=time.monotonic()-began;write(output/'receipt.json',report)

if __name__=='__main__':
    ap=argparse.ArgumentParser();ap.add_argument('--card',type=Path,required=True);ap.add_argument('--card-sha256',required=True)
    ap.add_argument('--owned-child',action='store_true');ap.add_argument('--whole-cutoff',type=float)
    ap.add_argument('--consent-actual-retained-r5-restart',action='store_true')
    args=ap.parse_args()
    if args.owned_child:
        require(args.whole_cutoff is not None,'parent original cutoff required');child(args.card,args.card_sha256,args.whole_cutoff)
    else:
        require(args.consent_actual_retained_r5_restart,'explicit restart consent required');outer(args.card,args.card_sha256)
