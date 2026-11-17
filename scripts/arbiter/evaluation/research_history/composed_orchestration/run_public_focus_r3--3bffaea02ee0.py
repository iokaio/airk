# SPDX-License-Identifier: Apache-2.0
"""Isolated U93 public-control Focus/MiniLM fixture; current typed producer.

This does not qualify the modern I4 installed journey.

Root must retain the actual same-user process/descendant owner around this script.
No current public Context download authority or Large-finalize proof is inferred.
"""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time

FOCUS='04821edaed45ac10aa5b0b7b2c0b7b28bf5d04a06c8b623cfe6d762f77d5e23b'
SELECTOR='service_runtime::public_arbiter_journey_tests::actual_public_arbiter_control'
SYNTHETIC='service_runtime::evaluation_fixture_tests::emit_private_selected_synthetic_fixture_software_v2'
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
HELPERS=('check-service-clients.py','check-runtime-upgrade.py','build-service-runtime.py',
         'split_receipts.py','standalone-installer-contract.py','check-retrieval-quality.py',
         'reviewed-source-fixture.py','retrieval_judgments.py','combined_evidence.py',
         'http_fixture.py','owned-runtime-processes.py')

def digest(path):
    with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()

def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);value=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value);return value

def retain(path,value):
    with path.open('x',encoding='utf-8') as f:json.dump(value,f,indent=2);f.write('\n')

def verify_preparation(expected_sha256):
    packet_path=Path(__file__).with_name('public-focus-preparation-r3.json')
    if digest(packet_path)!=expected_sha256:raise ValueError('reviewed private preparation packet differs')
    packet=json.loads(packet_path.read_text(encoding='utf-8'))
    for pin in packet['files']+packet['helper_source_pins']:
        if digest(pin['path'])!=pin['sha256']:raise ValueError('reviewed private source input changed: '+pin['path'])
    return packet

def remaining(deadline):
    value=deadline-time.monotonic()
    if value<=0:raise TimeoutError('original native journey observation cutoff expired')
    return value

class ProcessExit:
    """Observe the PID from our actual managed activation; never request terminate rights."""
    def __init__(self,pid):
        self.api=ctypes.WinDLL('kernel32',use_last_error=True)
        self.api.OpenProcess.argtypes=[wintypes.DWORD,wintypes.BOOL,wintypes.DWORD]
        self.api.OpenProcess.restype=wintypes.HANDLE
        self.api.WaitForSingleObject.argtypes=[wintypes.HANDLE,wintypes.DWORD]
        self.api.WaitForSingleObject.restype=wintypes.DWORD
        self.api.GetExitCodeProcess.argtypes=[wintypes.HANDLE,ctypes.POINTER(wintypes.DWORD)]
        self.api.CloseHandle.argtypes=[wintypes.HANDLE]
        self.pid=pid;self.exit_observation=None
        self.handle=self.api.OpenProcess(0x100000|0x1000,False,pid)
        if not self.handle:raise ctypes.WinError(ctypes.get_last_error())
        if self.api.WaitForSingleObject(self.handle,0)!=258:
            self.close();raise RuntimeError('managed activation process exited before handle observation')
    def wait(self,deadline):
        if self.exit_observation is not None:
            if self.exit_observation['exit_code']!=0:raise RuntimeError('actual native service exited unsuccessfully')
            return dict(self.exit_observation)
        code=self.api.WaitForSingleObject(self.handle,max(1,min(0xfffffffe,int(remaining(deadline)*1000))))
        if code!=0:raise TimeoutError('actual original service process exit unobserved')
        value=wintypes.DWORD()
        if not self.api.GetExitCodeProcess(self.handle,ctypes.byref(value)):raise ctypes.WinError(ctypes.get_last_error())
        self.exit_observation={'pid':self.pid,'process_handle_signaled':True,'exit_code':value.value}
        if value.value!=0:raise RuntimeError('actual native service exited unsuccessfully: '+str(value.value))
        return dict(self.exit_observation)
    def close(self):
        if self.handle:self.api.CloseHandle(self.handle);self.handle=None

class StopCutoffs:
    """One existing120s observer per actual service instance, never renewed."""
    def __init__(self,whole_cutoff,clock=time.monotonic):
        self.whole_cutoff=whole_cutoff;self.clock=clock;self.values={}
    def capture(self,instance):
        if instance not in self.values:
            self.values[instance]=min(self.whole_cutoff,self.clock()+120)
        return self.values[instance]

def retain_original_exit(handle,deadline):
    """Retain actual failure signals too; unknown never becomes a successful exit."""
    if handle.exit_observation is None:
        try:handle.wait(deadline)
        except BaseException as error:
            return {'pid':handle.pid,'observation':handle.exit_observation,
                    'failure':{'type':type(error).__name__,'message':str(error)}}
    return {'pid':handle.pid,'observation':handle.exit_observation}

def profile_row(status):
    rows=[row for row in status['focus']['profiles'] if row.get('profile')==FOCUS]
    if len(rows)!=1:raise ValueError('running product does not advertise exact selected Focus')
    return rows[0]

def run(args):
    if os.name!='nt':raise ValueError('this isolated actual public-control fixture qualifies Windows only')
    actual_sid=subprocess.check_output([r'C:\Windows\System32\whoami.exe','/user','/fo','csv','/nh'],text=True)
    if SID not in actual_sid:raise ValueError('current execution SID differs from verified human')
    repo=args.repository.resolve(strict=True)
    output=args.output.resolve()
    if not output.is_relative_to(repo/'artifacts'):
        raise ValueError('resolved private evidence leaves authorized artifacts')
    preparation=verify_preparation(args.preparation_sha256)
    started=time.monotonic();cutoff=started+args.whole_phase_seconds
    output.mkdir(exist_ok=False)
    fixture=None;handles=[];children=[];stop_limits=StopCutoffs(cutoff);stop_attempts=set();report={'schema':'airk.public-focus-native-journey/3',
        'scope':'isolated U93 managed public-control fixture; modern I4 installed journey unqualified',
        'passed':False,'release_qualified':False,'large_public_finalize_exercised':False,
        'current_public_context_available':False,'native_nn_calls_claimed':False,'steps':[]}
    def record(label,value):
        retain(output/(f'{len(report["steps"]):03d}-{label}.json'),value)
        report['steps'].append({'label':label,'elapsed_seconds':time.monotonic()-started})
    def child(argv,env,deadline,observe=None):
        process=subprocess.Popen(argv,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=env,
                                 creationflags=subprocess.CREATE_NO_WINDOW)
        children.append(process)
        try:
            if observe is not None:
                while process.poll()is None:
                    remaining(deadline);observe(deadline)
            stdout,stderr=process.communicate(timeout=remaining(deadline))
        except subprocess.TimeoutExpired:
            # Root's original descendant owner remains responsible; no kill,
            # retry or successful-cleanup claim is manufactured here.
            raise TimeoutError('owned controller outcome remains unknown; retain fixture/owner')
        record('native-child',{'argv':list(map(str,argv)),'exit_code':process.returncode,
            'stdout':stdout.decode('utf-8','replace'),'stderr':stderr.decode('utf-8','replace')})
        if process.returncode:raise RuntimeError('native child refused; inspect retained stdout/stderr')
    try:
        if os.name!='nt':raise ValueError('this actual process observer qualifies Windows only')
        runtime=args.runtime_dir.resolve(strict=True);repo=args.repository.resolve(strict=True)
        service_test=args.service_test_exe.resolve(strict=True)
        if digest(service_test)!=args.service_test_sha256:raise ValueError('current native controller bytes differ')
        if not output.is_relative_to(repo/'artifacts'):raise ValueError('protected private evidence must stay in artifacts')
        if args.fixture.exists():raise ValueError('fresh protected owning fixture required')
        args.fixture.mkdir()
        frozen=output/'frozen-source'/'scripts';frozen.mkdir(parents=True)
        helper_pins={}
        for name in HELPERS:
            data=(Path(__file__).with_name('public-focus-r3-helper-source')/'scripts'/name).read_bytes();(frozen/name).write_bytes(data)
            helper_pins[name]=hashlib.sha256(data).hexdigest()
            expected=next(pin['sha256'] for pin in preparation['helper_source_pins'] if Path(pin['path']).name==name)
            if helper_pins[name]!=expected:raise ValueError('helper changed during source freeze: '+name)
        private_assertions=Path(__file__).with_name('public_focus_evidence_r3.py').read_bytes()
        (frozen/'public_focus_evidence_r3.py').write_bytes(private_assertions)
        helper_pins['public_focus_evidence_r3.py']=hashlib.sha256(private_assertions).hexdigest()
        record('helper-inputs',{'sha256':helper_pins,'preparation_packet_sha256':args.preparation_sha256})
        evidence=load('public_focus_combined',frozen/'combined_evidence.py')
        quality=load('public_focus_source_oracle',frozen/'check-retrieval-quality.py')
        assertions=load('public_focus_assertions',frozen/'public_focus_evidence_r3.py')
        clients=load('current_public_focus_clients',frozen/'check-service-clients.py')
        class ObservedFixture(clients.Fixture):
            stop_cutoff=None
            def execute(self,command,*,input=None,check=True,timeout=None):
                until=min(cutoff,self.stop_cutoff if self.stop_cutoff is not None else cutoff)
                bounded=min(timeout if timeout is not None else self.command_timeout,remaining(until))
                return super().execute(command,input=input,check=check,timeout=bounded)
            def rpc(self,method,*,deadline=None):
                until=min(deadline if deadline is not None else cutoff,cutoff)
                result=self.run('service','rpc','--runtime-dir',self.control,'--stdin',
                    input=json.dumps(method),timeout=remaining(until))
                return json.loads(result.stdout)
            def wait(self,receipt,expected='succeeded'):
                until=min(cutoff,time.monotonic()+self.job_timeout)
                while True:
                    job=self.rpc({'job':{'id':receipt['job']['id']}},deadline=until)['job']
                    if job['state'] in ('succeeded','failed','canceled'):
                        if job['state']!=expected:raise RuntimeError('native maintenance job failed: '+json.dumps(job))
                        return job
                    remaining(until)
        fixture=ObservedFixture(runtime,args.fixture,command_timeout=120,job_timeout=180,preserve_pair_metadata=True)
        if not fixture.managed_installation or fixture.compatibility['topology']!='split_pair':
            raise ValueError('isolated U93 managed matched pair required')
        fixture.start()
        first_health=dict(fixture.managed_health)
        record('actual-start',first_health)
        handles.append(ProcessExit(first_health['process_id']))
        assertions.require_health_source(first_health,SID,args.source_sha256)
        def begin_stop():
            instance=(fixture.managed_health or {}).get('instance','startup-unbound')
            fixture.stop_cutoff=stop_limits.capture(instance)
            return instance,fixture.stop_cutoff
        ordinal=0
        def status(until=None):
            return json.loads(fixture.run('--service-runtime-dir',fixture.control,'arbiter','status',
                                         timeout=remaining(min(cutoff,until if until is not None else cutoff))).stdout)
        overlap=[]
        def cache_progress(until):
            before=status(until);health=fixture.rpc('health',deadline=until)['observation'];after=status(until)
            if any(health[key]!=fixture.managed_health[key] for key in ('instance','process_id','executable_sha256','user')):
                raise ValueError('actual control-progress Health identity changed')
            observed=(profile_row(before)['availability']=='downloading'
                      and profile_row(after)['availability']=='downloading')
            if observed:overlap.append({'before':before,'actual_health_between':health,'after':after})
        def control(operation,*,index=None,source=None,extra=None,observe=None):
            nonlocal ordinal
            target=output/'typed-controls'/f'{ordinal:03d}-{operation}';target.mkdir(parents=True);ordinal+=1
            health=fixture.managed_health
            env={**os.environ,'PYTHONDONTWRITEBYTECODE':'1'}
            values={'USER_SID':SID,'PID':str(health['process_id']),'INSTANCE':health['instance'],
                'CONTROL':str(fixture.control),'RUNTIME':str(runtime),'SERVICE_SHA256':digest(runtime/'airk.exe'),
                'CLI_SHA256':digest(runtime/'airkctl.exe'),'INSTALL_SHA256':digest(runtime/'airk-install.json'),
                'SOURCE_SHA256':args.source_sha256,'OUTPUT':str(target),'OPERATION':operation,'PROFILE':FOCUS}
            if index:values['INDEX']=index
            if source:values['SOURCE_ROOT']=str(source)
            values.update(extra or {})
            env.update({'AIRK_PUBLIC_ARBITER_'+key:value for key,value in values.items()})
            child([str(service_test),'--ignored','--exact',SELECTOR,'--nocapture','--test-threads=1'],env,min(cutoff,time.monotonic()+120),observe=observe)
            value=json.loads((target/'receipt.json').read_text(encoding='utf-8'))
            if value.get('passed')is not True:raise RuntimeError('native typed control lacked accepted proof')
            return value
        initial=status()
        if initial.get('initialization')is not None:control('initialize');initial=status()
        if initial['configuration']['focus']['enabled'] or initial['configuration']['context']['enabled']:
            raise ValueError('fresh optional stages unexpectedly enabled')
        record('initial-admission',initial)
        if profile_row(initial)['availability']!='missing_assets':raise ValueError('fresh Focus admission did not expose missing assets')
        # The source dirs and full read argument files come from current Rust.
        data=args.fixture/'data';data.mkdir()
        sources={}
        for name in ('a','b'):
            source_output=output/('synthetic-'+name)
            env={**os.environ,'AIRK_SELECTED_SYNTHETIC_OUTPUT':str(source_output)}
            child([str(service_test),'--ignored','--exact',SYNTHETIC,'--nocapture','--test-threads=1'],env,min(cutoff,time.monotonic()+120))
            manifest=json.loads((source_output/'fixture.private.json').read_text(encoding='utf-8'))
            source=Path(manifest['source_root']);sources[name]=(source_output,source)
            fixture.register('focus-'+name,source,data)
            fixture.job('focus-'+name,'index')
            fixture.job('focus-'+name,'setup-model')
            fixture.job('focus-'+name,'index')
        originals={str(p):digest(p) for _,source in sources.values() for p in source.iterdir() if p.is_file()}
        oracles={name:quality.freeze_source_oracle(fixture,'focus-'+name,[(source,'')])
                 for name,(_,source) in sources.items()}
        if not all(oracles.values()):raise ValueError('native catalog source closure required')
        record('pre-read-source-oracles',{name:quality.oracle_receipt(value) for name,value in oracles.items()})
        publications={}
        def canonical_read(name,kind,arguments,until):
            index=fixture.scopes['focus-'+name]['index_id']
            result=fixture.run('--service-runtime-dir',fixture.control,'tools','call',
                'search' if kind=='search' else 'airk_diagnostics', '--index',index,*arguments,
                check=False,timeout=min(120,remaining(until)))
            # Retain exact native bytes even when parsing/admission subsequently refuses.
            record('raw-'+kind+'-'+name,{'stdout':result.stdout,'stderr':result.stderr,'exit_code':result.returncode})
            envelope=json.loads(result.stdout)
            if result.returncode:raise RuntimeError('actual native canonical read refused')
            observation=assertions.observe(envelope,index,oracles[name],evidence,quality,kind=kind,
                                            expected_publications=publications.get(name))
            record('validated-'+kind+'-'+name,observation)
            return observation
        def tool(name,argument_name):
            return canonical_read(name,'search',['--args-file',sources[name][0]/argument_name],cutoff)
        # Publication and semantic readiness are actual metadata observations,
        # not repeated scoring requests. Keep one original setup cutoff per index.
        for name in sources:
            semantic_cutoff=min(cutoff,time.monotonic()+180)
            while True:
                observed=canonical_read(name,'diagnostics',['--args','{"response_mode":"full"}'],semantic_cutoff)
                remaining(semantic_cutoff)
                readiness=clients.semantic_readiness(observed['member_body'])
                if readiness['state']=='failed':raise RuntimeError('normal MiniLM semantic work failed')
                if readiness['state']=='ready':
                    publications[name]=observed['publication_members']
                    record('semantic-ready-'+name,observed);break
        baselines={name:tool(name,'search-hybrid.json') for name in sources}
        for observation in baselines.values():
            assertions.require_hybrid(observation)
            if assertions.applied_focus(observation):raise ValueError('unexpected pre-enable native Focus')
        record('minilm-baselines',baselines)
        imported_source=args.focus_assets.resolve(strict=True)
        import_originals={str(p):digest(p) for p in imported_source.iterdir() if p.is_file()}
        import_cutoff=min(cutoff,time.monotonic()+1800)
        control('import',extra={'IMPORT_DIRECTORY':str(imported_source)})
        def settle(ready,until,observe=None):
            while True:
                remaining(until)
                if observe is not None:observe(until)
                observed=status(until);remaining(until);row=profile_row(observed)
                if row['availability'] in ('failed','unqualified','unsupported_system','constrained'):
                    raise RuntimeError('actual native lifecycle refused: '+row['reason'])
                if ready(observed):return observed
                remaining(until)
        cached=settle(lambda s:profile_row(s)['availability']=='requested' and s['focus']['running']==0 and s['focus']['queued']==0,import_cutoff)
        record('cached-import',cached)
        # Existing exact cached native Provision verifies all files and returns
        # before HTTP. It is not proof of old Large transaction.finalize behavior.
        verification_cutoff=min(cutoff,time.monotonic()+1800)
        control('provision',extra={'DOWNLOAD_CONSENT':'true'},observe=cache_progress)
        record('cached-provision',settle(lambda s:profile_row(s)['availability']=='requested',verification_cutoff,observe=cache_progress))
        record('actual-cached-verification-control-overlap',{'observed':bool(overlap),'samples':overlap,
            'scope':'Focus cached pin hashes only; not old Large transaction.finalize path'})
        report['cached_verification_control_overlap_observed']=bool(overlap)
        a=fixture.scopes['focus-a']['index_id'];source=sources['a'][1]
        warm_cutoff=min(cutoff,time.monotonic()+1800)
        control('enable',index=a,source=source)
        record('focus-ready',settle(lambda s:s['focus']['availability']=='ready' and s['focus']['active_profile']==FOCUS,warm_cutoff))
        control('stale-configure',index=a,source=source)
        enabled=tool('a','search-hybrid.json');excluded=tool('b','search-hybrid.json')
        if not assertions.applied_focus(enabled) or assertions.applied_focus(excluded):raise ValueError('actual Focus application/enrollment isolation unproved')
        assertions.require_hybrid(enabled);assertions.require_hybrid(excluded)
        assertions.require_applied_focus(enabled,a,FOCUS)
        record('focus-applied',enabled);record('unenrolled-minilm',excluded)
        report['native_nn_calls_claimed']=True
        control('disable',index=a,source=source)
        disabled=tool('a','search-hybrid.json')
        assertions.require_hybrid(disabled)
        if assertions.applied_focus(disabled):
            raise ValueError('normal MiniLM behavior after disable absent')
        record('disabled-minilm',disabled)
        control('unenroll',index=a,source=source)
        removal_cutoff=min(cutoff,time.monotonic()+120)
        control('remove')
        removed=settle(lambda s:profile_row(s)['availability']=='missing_assets',removal_cutoff)
        record('removed',removed)
        if any(digest(Path(p))!=expected for p,expected in {**originals,**import_originals}.items()):raise ValueError('original source/import bytes changed')
        stop_key,first_cutoff=begin_stop();stop_attempts.add(stop_key);fixture.close()
        record('first-native-stop',fixture.managed_stop);record('first-process-exit',handles[-1].wait(first_cutoff))
        # Reacquire the real native lease through ordinary U93 managed Ensure.
        fixture.managed_stop=None;fixture.managed_health=None;fixture.stop_cutoff=None
        fixture.service_run('runtime','ensure','--install-dir',fixture.install,timeout=remaining(cutoff))
        fixture.managed_health=fixture.rpc('health')['observation']
        record('actual-restart-health',fixture.managed_health)
        handles.append(ProcessExit(fixture.managed_health['process_id']))
        assertions.require_health_source(fixture.managed_health,SID,args.source_sha256)
        if any(fixture.managed_health[key]!=first_health[key] for key in ('runtime_sha256','executable_sha256','user','version')) or fixture.managed_health['instance']==first_health['instance']:
            raise ValueError('restart failed to bind same artifact/new actual service instance')
        restarted=status()
        if restarted['configuration']['focus']['enabled'] or profile_row(restarted)['availability']!='missing_assets':
            raise ValueError('persisted disabled/removed state did not survive native lease reacquisition')
        record('restarted-state',restarted)
        stop_key,final_cutoff=begin_stop();stop_attempts.add(stop_key);fixture.close()
        record('final-native-stop',fixture.managed_stop);record('final-process-exit',handles[-1].wait(final_cutoff))
        report['source_files_unchanged']=True
        report['import_source_files_unchanged']=True
        report['passed']=True
    except BaseException as error:
        report['failure']={'type':type(error).__name__,'message':str(error)}
        raise
    finally:
        if fixture is not None and fixture.managed_started:
            instance=(fixture.managed_health or {}).get('instance','startup-unbound')
            cleanup_cutoff=stop_limits.capture(instance)
            fixture.stop_cutoff=cleanup_cutoff
            if fixture.managed_stop is None and instance not in stop_attempts:
                stop_attempts.add(instance)
                try:fixture.close()
                except BaseException as error:report['cleanup_failure']={'type':type(error).__name__,'message':str(error)}
            # Never repeat an uncertain Stop or renew its original cutoff.
            report['cleanup_process_exits']=[retain_original_exit(handle,cleanup_cutoff) for handle in handles]
        report['stop_observation_cutoffs']=[{'instance':key,'absolute_monotonic':value}
                                          for key,value in stop_limits.values.items()]
        report['actual_controller_children']=[{'pid':p.pid,'exit_code':p.poll()} for p in children]
        report['actual_native_stop']=getattr(fixture,'managed_stop',None)
        report['elapsed_seconds']=time.monotonic()-started
        for handle in handles:handle.close()
        retain(output/'receipt.json',report)

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('runtime-dir','repository','service-test-exe','fixture','focus-assets','output'):
        parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--preparation-sha256',required=True)
    parser.add_argument('--service-test-sha256',required=True)
    parser.add_argument('--source-sha256',required=True)
    parser.add_argument('--whole-phase-seconds',type=int,default=1800)
    parser.add_argument('--consent-focus-native-journey',action='store_true',required=True)
    options=parser.parse_args()
    if not 1<=options.whole_phase_seconds<=3600:parser.error('bounded original journey phase required')
    run(options)
