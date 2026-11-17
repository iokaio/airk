# SPDX-License-Identifier: Apache-2.0
"""Private existing-route composed launch; no execution without Root's frozen card."""
import argparse, hashlib, importlib.util, json, os, queue, subprocess, sys, threading, time
from pathlib import Path
BASE=Path(__file__).resolve().parent
SID='S-1-5-21-3678370870-3858642417-826968105-1002'

def need(value,message):
    if not value: raise ValueError(message)
def sha(path):
    with Path(path).open('rb') as stream: return hashlib.file_digest(stream,'sha256').hexdigest()
def write(path,value):
    with Path(path).open('x',encoding='utf-8') as stream: json.dump(value,stream,indent=2);stream.write('\n')
def load(path,name):
    spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m

def remaining(cutoff,now=None):
    value=cutoff-(time.monotonic() if now is None else now)
    if value<=0: raise TimeoutError('Original whole1800s phase exhausted; no renewed allowance')
    return value

def natural(value):
    expected={'exit_code':0,'timed_out':False,'log_oversized':False,'owned_descendants_settled':True,
        'owned_job_forced_termination':False,'owned_job_final_active_processes':0,
        'owned_job_final_member_pids':[],'parent_assigned_to_job':False,'child_assigned_before_resume':True}
    return all(type(value.get(k)) is type(v) and value[k]==v for k,v in expected.items())

def card(expected):
    path=BASE/'launch-card.json';raw=path.read_bytes();need(hashlib.sha256(raw).hexdigest()==expected,'Root-reviewed card SHA differs')
    value=json.loads(raw);need(value['whole_seconds']==1800 and value['r154_seconds']==360,'Original whole and Answer phases required')
    return value,raw

def verify_source_cohort(value):
    # One pre-spawn metadata cohort: byte-equal copied corpora in different
    # directories are different source identities and must never be substituted.
    diagnostic_pin=value['r154_preparation'];scope_pin=value['source_scope_input']
    need(sha(diagnostic_pin['path'])==diagnostic_pin['sha256'],'Diagnostic preparation SHA changed')
    need(sha(scope_pin['path'])==scope_pin['sha256'],'Declared source scope SHA changed')
    diagnostic=json.loads(Path(diagnostic_pin['path']).read_bytes())
    scopes=json.loads(Path(scope_pin['path']).read_bytes())['source_roots']
    root=Path(value['source_root']);diagnostic_root=Path(diagnostic['source_root'])
    need(root.is_absolute() and root.is_dir() and root.samefile(diagnostic_root),
        'Source cohort mismatch: card and frozen diagnostic source roots differ')
    need(isinstance(scopes,list) and len(scopes)==1 and root.samefile(Path(scopes[0])),
        'Source cohort mismatch: declared scope root differs from indexed diagnostic root')
    fixture=Path(diagnostic_pin['path']).parent/'source-fixture'
    canonical=root.resolve(strict=True);copied=diagnostic['copied_source_files']
    need(len(copied)==753,'Frozen753-source copy cohort differs')
    for row in copied:
        path=Path(row['copy_path']);relative=Path(row['relative_path'])
        need(not relative.is_absolute() and '..' not in relative.parts,
            'Source cohort copy path escapes frozen fixture')
        need(path.resolve(strict=True).is_relative_to(canonical) and path.samefile(fixture/relative),
            'Source cohort mismatch: copied source belongs to another physical fixture')
    return {'source_root':str(root),'diagnostic_root':str(diagnostic_root),'declared_scope_roots':scopes,
        'copied_source_files':len(copied),'canonical_directory_identity_agrees':True}

def verify(value):
    verify_source_cohort(value)  # Must refuse before every Popen, including outer-owner creation.
    for path,expected in value['pins'].items(): need(sha(path)==expected,'Immutable input changed: '+path)
    staging=json.loads(Path(value['staging_receipt']['path']).read_bytes())
    guard=load(Path(value['staging_guard']), 'r154_copy_custody_guard')
    need(staging['complete'] is True and len(staging['copies'])==23,'Actual independent staging receipt required')
    for row in staging['copies']:
        path=Path(row['destination']);before=row['destination_observation'];current=path.lstat()
        need(not current.st_file_attributes&0x400 and current.st_nlink==1
            and current.st_ino==before['file_id'] and current.st_size==before['bytes']
            and current.st_mtime_ns==before['mtime_ns'],'Actual staged asset preimage changed: '+str(path))
        need(guard.security(path)=={key:before[key] for key in ('owner_sid','sddl')},'Actual staged source owner/SDDL changed')
    return staging

class NativeRoute:
    def __init__(self,value,root,cutoff):
        self.value=value;self.root=root;self.cutoff=cutoff;self.counter=0;self.service=None;self.handle=None
        self.ready=None;self.controller=None;self.controller_lines=queue.Queue();self.delegated_stop=False
        self.report={'schema':'airk.private-r154-composed-route/1','complete':False,'qualification_granted':False,
            'first_failure':None,'service_stop_delegated_to_R154':False,'stages':[],'whole_deadline_monotonic':cutoff}
    def retain(self):
        path=self.root/'route-progress.json'
        with path.open('w',encoding='utf-8') as stream:json.dump(self.report,stream,indent=2);stream.flush()
    def call(self,argv,label,input=None,timeout=None,env=None):
        self.counter+=1;directory=self.root/'commands'/f'{self.counter:05d}-{label}';directory.mkdir()
        budget=min(remaining(self.cutoff),timeout if timeout is not None else 120)
        command=[str(x) for x in argv];record={'argv':command,'original_whole_cutoff':self.cutoff,'timeout':budget}
        write(directory/'before.json',record);started=time.monotonic();error=None
        try:
            result=subprocess.run(command,input=input.encode() if isinstance(input,str) else input,
                capture_output=True,timeout=budget,check=False,env=env)
            out,err,code=result.stdout,result.stderr,result.returncode
        except subprocess.TimeoutExpired as caught:
            out,err,code=caught.stdout or b'',caught.stderr or b'',None;error=caught
        (directory/'stdout.bin').write_bytes(out);(directory/'stderr.bin').write_bytes(err)
        write(directory/'after.json',{'returncode':code,'timed_out':error is not None,'elapsed_seconds':time.monotonic()-started,
            'stdout_sha256':hashlib.sha256(out).hexdigest(),'stderr_sha256':hashlib.sha256(err).hexdigest()})
        remaining(self.cutoff)
        if error is not None:raise error
        need(code==0,'Actual native command refused; full bytes retained: '+str(directory))
        return subprocess.CompletedProcess(command,code,out.decode(),err.decode())
    def cli(self,*argv,**kwargs):return self.call([self.value['airkctl'],*argv],kwargs.pop('label','cli'),**kwargs)
    def alive(self):need(self.service.poll() is None,'Original Service exited before required observation')
    def wait_event(self,watch,cutoff):
        self.alive();api=watch.api
        from ctypes import wintypes as w
        import ctypes
        api.WaitForMultipleObjects.argtypes=[w.DWORD,ctypes.POINTER(w.HANDLE),w.BOOL,w.DWORD]
        api.WaitForMultipleObjects.restype=w.DWORD
        handles=(w.HANDLE*2)(watch.handle,self.handle.handle)
        result=api.WaitForMultipleObjects(2,handles,False,max(1,int(remaining(min(cutoff,self.cutoff))*1000)))
        if result==0:
            need(api.FindNextChangeNotification(watch.handle),'Actual directory observer rearm failed')
        elif result==1:raise RuntimeError('Original Service exited during observed startup')
        elif result==258:raise TimeoutError('Original startup/event observation cutoff expired')
        else:raise ctypes.WinError(ctypes.get_last_error())
    def start(self,custody,sid_reader,changes):
        env=dict(os.environ);env.update(self.value['service_environment'])
        image=self.value['service_image'];argv=[image['path'],'--exact',self.value['fixture_selector'],'--ignored','--nocapture','--test-threads=1']
        self.service_log=(self.root/'service-stdout-stderr.log').open('xb')
        # This child belongs to the maintained outer uncapped Job, never the nested8GiB observer Job.
        watch=changes(Path(self.value['stage_root']))
        startup_cutoff=min(self.cutoff,time.monotonic()+900)
        try:
            self.service=subprocess.Popen(argv,stdin=subprocess.DEVNULL,stdout=self.service_log,stderr=subprocess.STDOUT,
                env=env,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
            self.report['service_spawn']={'argv':argv,'pid':self.service.pid,'startup_cutoff_monotonic':startup_cutoff}
            self.retain()
            self.handle=custody.OriginalProcess(self.service.pid,image,SID,sid_reader)
            self.report['original_service_before']=dict(self.handle.proof);self.retain()
            ready_path=Path(self.value['work'])/'ready.json'
            while not ready_path.exists():self.wait_event(watch,startup_cutoff)
            self.observe_ready(startup_cutoff)
        finally:watch.close()
    def observe_ready(self,cutoff):
        from private_record_reader import read_observed
        path=Path(self.value['work'])/'ready.json'
        raw,proof=read_observed(path,min(cutoff,self.cutoff))
        candidate=json.loads(raw);self.report['ready_read_observation']=proof
        self.report['observed_ready_candidate']=candidate
        need(candidate.get('schema')=='airk.private-arbiter-fixture/1' and candidate.get('evaluation') is True
            and candidate.get('release_qualified') is False,'Actual private Ready required')
        health=candidate.get('health',{});image=self.value['service_image']
        need(health.get('process_id')==self.service.pid and health.get('executable_sha256')==image['sha256']
            and health.get('user')==SID and health.get('instance'),'Actual Ready differs from retained original PID/SID/image')
        need(self.handle.proof['execution_user_sid']==SID and self.handle.proof['image']['sha256']==image['sha256'],
            'Original retained process image/SID proof differs')
        need(candidate.get('inputs_sha256')==self.value['evaluation_inputs']['sha256'],'Actual Ready uses different staged profiles')
        control=Path(candidate['control_root'])
        need(control.samefile(Path(self.value['work'])/'control'),'Actual Ready names a different control directory')
        # Only a fully authenticated actual record becomes cleanup/control authority.
        self.ready=candidate;self.control=control;self.report['full_ready']=candidate;self.retain()
    def setup(self,clients):
        route=self
        class Attached(clients.Fixture):
            @property
            def catalog_authority(self):return True
            def run(self,*args,input=None,check=True,timeout=None):
                need(check is True,'Only successful native setup commands permitted')
                return route.cli(*args,input=input,timeout=timeout,label='setup')
            def wait(self,receipt,expected='succeeded'):
                # Same exercised FocusR5 observation loop; only read-only Job observations repeat.
                # Every real CLI completion is bounded by the ONE original route cutoff; no sleep/retry mutation.
                while True:
                    job=self.rpc({'job':{'id':receipt['job']['id']}},deadline=route.cutoff)['job']
                    if job['state'] in ('succeeded','failed','canceled'):
                        need(job['state']==expected,'Native maintenance job failed: '+json.dumps(job));return job
                    remaining(route.cutoff)
        fixture=Attached.__new__(Attached);fixture.control=self.control;fixture.projects={};fixture.scopes={}
        data=Path(self.value['index_data']);data.mkdir(exist_ok=False)
        fixture.register('r154',Path(self.value['source_root']),data)
        write(self.root/'native-registration-and-selection.json',{'registration':fixture.projects['r154'],'selection':fixture.scopes['r154']})
        for command in ('index','setup-model','index'):
            result=fixture.job('r154',command)
            write(self.root/('native-job-'+command+('-final' if (self.root/('native-job-'+command+'.json')).exists() else '')+'.json'),result)
        self.report['native_index_setup_complete']=True;self.retain()
    def start_controller(self):
        value=self.value;packet={'whole_phase_seconds':1800,'resource_policy_input':value['resource_policy_input'],
            'session_helper':value['session_helper'],'session_library':value['session_library'],
            'client_dependencies':value['client_dependencies'],'pins':value['controller_pins']}
        packet_path=self.root/'controller-packet.json';write(packet_path,packet)
        self.controller_error=(self.root/'controller-stderr.bin').open('xb')
        self.controller=subprocess.Popen([value['node']['path'],value['session_controller'],str(packet_path)],
            stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.controller_error,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
        def read():
            with (self.root/'controller-stdout.jsonl').open('xb') as log:
                for line in iter(self.controller.stdout.readline,b''):
                    log.write(line);log.flush()
                    try:self.controller_lines.put(json.loads(line))
                    except BaseException as error:self.controller_lines.put(error)
            self.controller_lines.put(None)
        threading.Thread(target=read,daemon=True).start();self.controller_id=0
    def controller_request(self,request):
        self.controller_id+=1;identifier=str(self.controller_id);request={**request,'control_id':identifier}
        write(self.root/f'controller-request-{identifier}.json',request)
        self.controller.stdin.write((json.dumps(request)+'\n').encode());self.controller.stdin.flush()
        result=self.controller_lines.get(timeout=remaining(self.cutoff))
        if isinstance(result,BaseException):raise result
        write(self.root/f'controller-response-{identifier}.json',result)
        need(isinstance(result,dict) and result.get('id')==identifier and result.get('ok') is True,'Existing native event controller refused')
        return result
    def warm(self,stage,manifest,composed):
        directory=self.root/('warm-'+stage);directory.mkdir();v=self.value
        environment=dict(os.environ);environment.update(v['service_environment']);environment.update({
            'AIRK_ARBITER_EVALUATION_SCOPES':v['source_scope_input']['path'],
            'AIRK_ARBITER_EVALUATION_SCOPES_SHA256':v['source_scope_input']['sha256'],
            'AIRK_ARBITER_EVALUATION_CONTROL_PREFIX':'r154'+stage,'AIRK_ARBITER_EVALUATION_STAGE':stage,
            'AIRK_ARBITER_EVALUATION_PROFILE':manifest,'AIRK_ARBITER_EVALUATION_ENABLED':'true'})
        self.call([v['service_image']['path'],'--exact',v['emitter_selector'],'--ignored','--nocapture','--test-threads=1'],
            'emit-'+stage,env=environment)
        status_argv=[v['airkctl'],'--service-runtime-dir',str(self.control),'arbiter','status']
        config_argv=[v['airkctl'],'--service-runtime-dir',str(self.control),'arbiter','configure','--file',str(Path(v['work'])/('r154'+stage+'.configure.json'))]
        rows=[];starts={}
        for ordinal,argv in enumerate((status_argv,config_argv)):
            starts[ordinal]=time.monotonic();result=self.call(argv,'warm-'+stage+'-'+str(ordinal))
            raw=result.stdout.encode();err=result.stderr.encode();(directory/f'{ordinal:03d}-stdout.bin').write_bytes(raw);(directory/f'{ordinal:03d}-stderr.bin').write_bytes(err)
            rows.append({'request':{'kind':'native_command','argv':argv},'result':{'stdout_sha256':hashlib.sha256(raw).hexdigest(),'stderr_sha256':hashlib.sha256(err).hexdigest()}})
        watch={'kind':'controller','warm_configuration_observation_step':1,'warm_deadline_seconds':1800,
            'request':{'kind':'watch','path':str(Path(v['work'])/'warm-completions.jsonl'),'timeout_ms':1800000,
                'match':{'observer_id':self.ready['warm_observer_id'],'stage':stage,'profile_manifest':manifest},'after_sequence':-1}}
        bounded=composed.controller_request(watch,2,rows,starts,None,lambda:remaining(self.cutoff))
        response=self.controller_request(bounded);rows.append({'request':watch,'result':response})
        step={'stage':stage,'argv':status_argv,'configuration_observation_step':1,'prior_idle_observation_step':0,
            'completion_watch_observation_step':2,'warm_observer_id':self.ready['warm_observer_id'],
            'profile_manifest':manifest,'deadline_seconds':1800}
        result=composed.await_ready(step,rows,starts,directory,3,[v['airkctl']],lambda:remaining(self.cutoff))
        self.report['stages'].append({'stage':stage,'manifest':manifest,'ready_observation':result});self.retain()
    def finish_controller(self):
        if self.controller is None:return
        value=self.controller_request({'kind':'finish'});need(value['value'].get('closed') is True,'Existing controller closure absent')
        self.controller.stdin.close();need(self.controller.wait(timeout=remaining(self.cutoff))==0,'Existing controller process exit refused')
        self.controller_error.close();self.controller=None;self.report['warm_controller_naturally_closed']=True
    def diagnostic(self):
        v=self.value;work=Path(v['work']);need(remaining(self.cutoff)>=360,'Original route cannot admit unchanged360s observer phase')
        ready=work/'ready.json';binding=work/'r154context.bindings.json';template=work/'context-request.json'
        args=[v['python']['path'],'-S','-B',v['r154_launcher'],'--execute-new-diagnostic',
            '--preparation-sha256',v['r154_preparation']['sha256'],'--repository-root',v['repository_root'],
            '--runtime-pair',v['client_pair'],'--runtime-build-sha256',v['client_pair_build']['sha256'],
            '--fixture-ready',str(ready),'--fixture-ready-sha256',sha(ready),
            '--service-executable-path',v['service_image']['path'],'--service-executable-sha256',v['service_image']['sha256'],
            '--index-bindings',str(binding),'--index-bindings-sha256',sha(binding),
            '--resource-policy-input',v['resource_policy_input']['path'],'--resource-policy-input-sha256',v['resource_policy_input']['sha256'],
            '--request-template-sha256',sha(template),'--native-budget-cases',v['native_budget_cases']['path'],
            '--native-budget-cases-sha256',v['native_budget_cases']['sha256'],
            '--context-manifest',v['context_manifest'],'--focus-manifest',v['focus_manifest'],
            '--sdk-manifest',v['sdk_handoff']['path'],'--sdk-manifest-sha256',v['sdk_handoff']['sha256'],
            '--phase-seconds','360','--output',str(work/'r154-fresh-attribution-r4')]
        # After handoff the unchanged observer owns exactly one canonical Shutdown, including failures.
        self.delegated_stop=True;self.report['service_stop_delegated_to_R154']=True;self.retain()
        self.call(args,'R154-owned-observer',timeout=remaining(self.cutoff))
        path=work/'r154-fresh-attribution-r4/owned-execution.json';result=json.loads(path.read_bytes())
        self.report['r154_owned_execution']={'path':str(path),'sha256':sha(path),'full_result':result}
        need(result.get('complete') is True,'Original360s observer phase failed; no retry')
    def cleanup(self):
        if self.service is not None and self.handle is not None:
            owned_stop_cutoff=min(self.cutoff,time.monotonic()+120)
            if not self.delegated_stop and self.service.poll() is None and self.ready is None:
                # Recover only genuine actual Ready under the existing cleanup observation bound.
                # Never guess ControlRoot, synthesize Health or adopt a replacement process.
                self.observe_ready(owned_stop_cutoff)
                self.report['cleanup_recovered_actual_ready']=True;self.retain()
            if not self.delegated_stop and self.service.poll() is None and self.ready is not None:
                cutoff=owned_stop_cutoff
                health=json.loads(self.cli('--service-runtime-dir',self.control,'service','health',label='cleanup-health',timeout=remaining(cutoff)).stdout)
                self.report['full_cleanup_health']=health;self.retain()
                observed=health.get('observation',{})
                need(health.get('status')=='health' and all(observed.get(k)==self.ready['health'].get(k)
                    for k in ('instance','process_id','executable_sha256','user','version','protocol')),'Original cleanup Health binding refused')
                result=self.cli('--service-runtime-dir',self.control,'service','shutdown',label='canonical-shutdown',timeout=remaining(cutoff))
                stop=json.loads(result.stdout);self.report['native_shutdown']=stop;self.retain()
                need(stop.get('status')=='stopping','Actual original-instance private Shutdown refused; no retry')
                self.report['original_service_after']=self.handle.observe_original_exit(cutoff)
            else:self.report['original_service_after']=self.handle.observe_original_exit(self.cutoff)
            need(self.service.wait(timeout=remaining(self.cutoff))==0,'Actual original Service process return code refused')
            self.service._handle.Close();self.report['popen_original_handle_closed']=True
            work=Path(self.value['work']);closed=work/'closed.json';value=json.loads(closed.read_bytes())
            self.report['native_closed']={'path':str(closed),'sha256':sha(closed),'full_result':value}
            need(self.ready is not None and value.get('schema')=='airk.private-arbiter-fixture-closed/1'
                and value.get('evaluation') is True and value.get('release_qualified') is False
                and value.get('instance')==self.ready['health']['instance'] and Path(value['control_root'])==self.control
                and value.get('shutdown_completed') is True and value.get('service_lease_reacquired') is True,'Actual native lease/closed proof refused')
            self.report['native_journals']={}
            for name in ('composed-observer','warm-completions'):
                summary=work/(name+'.closed.json');raw=work/(name+'.jsonl');body=json.loads(summary.read_bytes())
                self.report['native_journals'][name]={'summary':{'path':str(summary),'sha256':sha(summary)},'raw':{'path':str(raw),'sha256':sha(raw),'bytes':raw.stat().st_size}}
                need(body.get('complete') is True and body.get('dropped')==0,'Actual native journal closure incomplete')
            self.report['service_process_wait_exit0']=True
        if self.controller is not None:
            self.controller.stdin.close();need(self.controller.wait(timeout=remaining(self.cutoff))==0,'Existing controller did not exit naturally')
            self.controller_error.close();self.controller=None
        if hasattr(self,'service_log'):self.service_log.flush();self.service_log.close()

def worker(value,started,cutoff):
    need(cutoff==started+1800,'Original inherited1800s absolute clock required')
    sys.path.insert(0,str(BASE/'shared'));sys.path.insert(0,value['owned_helpers'])
    import custody,reference_worker
    job=custody.current_job();need(job['flags']&0x2000 and not job['flags']&0x200 and job['job_memory']==0 and job['active_limit']==0,
        'Actual maintained uncapped outerJob required; Service must not enter8GiB observer Job')
    need(reference_worker.windows_sid()==SID,'Current actual worker SID differs')
    root=Path(value['output'])/'route';root.mkdir();(root/'commands').mkdir();route=NativeRoute(value,root,cutoff)
    route.report.update(original_started_monotonic=started,outer_job_policy=job)
    try:
        verify(value);remaining(cutoff)
        clients=load(BASE/'shared/check-service-clients.py','r154_attached_clients')
        changes=load(BASE/'shared/check-arbiter-lifecycle.py','r154_native_changes').Changes
        composed=load(BASE/'shared/run_composed.py','r154_existing_composed')
        route.start(custody,reference_worker.windows_sid,changes);route.setup(clients);route.start_controller()
        route.warm('focus',value['focus_manifest'],composed);route.warm('context',value['context_manifest'],composed)
        route.finish_controller();route.diagnostic();route.report['complete']=True
    except BaseException as error:
        route.report['first_failure']={'type':type(error).__name__,'message':str(error)}
        route.retain()  # First failure is durable BEFORE any possibly blocking cleanup observation.
    finally:
        try:route.cleanup()
        except BaseException as error:
            route.report['cleanup_failure']={'type':type(error).__name__,'message':str(error)}
            if route.report['first_failure'] is None:route.report['first_failure']=route.report['cleanup_failure']
        if route.handle is not None and route.handle.handle is not None:
            closed=route.handle.api.CloseHandle(route.handle.handle);route.handle.handle=None
            route.report['unsettled_original_observer_handle_closed']=bool(closed)
            route.report['original_service_partial_proof']=dict(route.handle.proof)
            if not closed and route.report['first_failure'] is None:
                route.report['first_failure']={'type':'HandleCloseFailure','message':'Original observer handle close failed'}
        try:verify(value);route.report['immutable_inputs_postverified']=True
        except BaseException as error:
            route.report['postverification_failure']=str(error)
            if route.report['first_failure'] is None:route.report['first_failure']={'type':type(error).__name__,'message':str(error)}
        observed=time.monotonic();route.report['elapsed_seconds']=observed-started;route.report['original_phase_expired']=observed>=cutoff
        route.report['complete']=route.report['complete'] and route.report['first_failure'] is None and observed<cutoff
        route.retain();write(root/'receipt.json',route.report)
    return 0 if route.report['complete'] else 1

def main():
    started=time.monotonic();parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--card-sha256',required=True);parser.add_argument('--execute-route',action='store_true')
    parser.add_argument('--worker',action='store_true');parser.add_argument('--started',type=float);parser.add_argument('--deadline',type=float)
    args=parser.parse_args();value,raw=card(args.card_sha256)
    if args.worker:
        need(args.execute_route and args.started is not None and args.deadline is not None,'Original owned worker arguments required')
        return worker(value,args.started,args.deadline)
    need(args.started is None and args.deadline is None,'Caller cannot supply/reset original outer clock')
    verify(value)
    if not args.execute_route:print(json.dumps({'prepared_only':True,'staged_assets':23,'native_execution':False}));return 0
    cutoff=started+1800;remaining(cutoff)
    sys.path.insert(0,value['owned_helpers']);sys.path.insert(0,str(BASE/'owned'))
    import reference_worker,reference_parent_no_site_r2,reference_owned,outer_job
    need(reference_worker.windows_sid()==SID,'Actual parent human SID differs')
    root=Path(value['output']);need(not root.exists(),'Fresh output required');root.mkdir()
    argv=[value['python']['path'],'-S','-B',str(Path(__file__).resolve()),'--card-sha256',args.card_sha256,
        '--execute-route','--worker','--started',repr(started),'--deadline',repr(cutoff)]
    def before_resume(api,job,info,child_deadline):
        return reference_parent_no_site_r2.suspended_authentication(api,info,cutoff,child_deadline,SID,
            value['python'],reference_owned.image_proof)
    report={'schema':'airk.private-r154-composed-owned/1','complete':False,'qualification_granted':False,
        'card_sha256':args.card_sha256,'original_started_monotonic':started,'original_deadline_monotonic':cutoff,'argv':argv}
    try:
        env={key:item for key,item in os.environ.items() if not key.startswith('AIRK_')}
        env.update(value['environment'])
        with (root/'stdout-stderr.log').open('xb') as log:
            result=outer_job.run_windows_owned(argv,BASE,log,remaining(cutoff),env,absolute_deadline=cutoff,before_resume=before_resume)
        report['owned']=result;need(natural(result),'Actual route Job did not naturally settle; native cleanup remains separate')
        child=root/'route/receipt.json';body=json.loads(child.read_bytes());report['route_receipt']={'path':str(child),'sha256':sha(child)}
        need(body.get('complete') is True and body.get('immutable_inputs_postverified') is True,'Composed route did not complete')
        report['complete']=True
    except BaseException as error:report['failure']={'type':type(error).__name__,'message':str(error)}
    finally:
        try:verify(value);need((BASE/'launch-card.json').read_bytes()==raw,'Frozen launch card changed');report['immutable_inputs_postverified']=True
        except BaseException as error:
            report['postverification_failure']=str(error);report['complete']=False
        observed=time.monotonic();report['elapsed_seconds']=observed-started;report['original_phase_expired']=observed>=cutoff
        if observed>=cutoff:report['complete']=False
        write(root/'receipt.json',report)
    print(json.dumps({'output':str(root),'complete':report['complete'],'failure':report.get('failure')}));return 0 if report['complete'] else 1

if __name__=='__main__':sys.exit(main())