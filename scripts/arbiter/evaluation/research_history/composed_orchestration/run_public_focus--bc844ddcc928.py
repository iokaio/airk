# SPDX-License-Identifier: Apache-2.0
"""Opt-in public Focus/MiniLM native journey; exact current typed control producer.

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
         'split_receipts.py','standalone-installer-contract.py')

def digest(path):
    with Path(path).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()

def load(name,path):
    spec=importlib.util.spec_from_file_location(name,path);value=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value);return value

def retain(path,value):
    with path.open('x',encoding='utf-8') as f:json.dump(value,f,indent=2);f.write('\n')

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
        self.handle=self.api.OpenProcess(0x100000|0x1000,False,pid)
        if not self.handle:raise ctypes.WinError(ctypes.get_last_error())
        if self.api.WaitForSingleObject(self.handle,0)!=258:
            self.close();raise RuntimeError('managed activation process exited before handle observation')
    def wait(self,deadline):
        code=self.api.WaitForSingleObject(self.handle,max(1,min(0xfffffffe,int(remaining(deadline)*1000))))
        if code!=0:raise TimeoutError('actual original service process exit unobserved')
        value=wintypes.DWORD()
        if not self.api.GetExitCodeProcess(self.handle,ctypes.byref(value)):raise ctypes.WinError(ctypes.get_last_error())
        if value.value!=0:raise RuntimeError('actual native service exited unsuccessfully: '+str(value.value))
        return {'process_handle_signaled':True,'exit_code':value.value}
    def close(self):
        if self.handle:self.api.CloseHandle(self.handle);self.handle=None

def profile_row(status):
    rows=[row for row in status['focus']['profiles'] if row.get('profile')==FOCUS]
    if len(rows)!=1:raise ValueError('running product does not advertise exact selected Focus')
    return rows[0]

def applied_focus(value):
    focus=value.get('focus')
    return isinstance(focus,dict) and focus.get('kind')=='receipt' and focus.get('receipt',{}).get('applied') is True

def run(args):
    if os.name!='nt':raise ValueError('this actual journey qualifies Windows only')
    actual_sid=subprocess.check_output([r'C:\Windows\System32\whoami.exe','/user','/fo','csv','/nh'],text=True)
    if SID not in actual_sid:raise ValueError('current execution SID differs from verified human')
    repo=args.repository.resolve(strict=True)
    output=args.output.resolve()
    if not output.is_relative_to(repo/'artifacts'):
        raise ValueError('resolved private evidence leaves authorized artifacts')
    started=time.monotonic();cutoff=started+args.whole_phase_seconds
    output.mkdir(exist_ok=False)
    fixture=None;handles=[];children=[];report={'schema':'airk.public-focus-native-journey/1',
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
            data=(repo/'scripts'/name).read_bytes();(frozen/name).write_bytes(data)
            helper_pins[name]=hashlib.sha256(data).hexdigest()
        record('helper-inputs',helper_pins)
        clients=load('current_public_focus_clients',frozen/'check-service-clients.py')
        class ObservedFixture(clients.Fixture):
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
        fixture=ObservedFixture(runtime,args.fixture,command_timeout=120,job_timeout=180)
        if not fixture.managed_installation or fixture.compatibility['topology']!='split_pair':
            raise ValueError('actual matched pair managed installation required')
        fixture.start();handles.append(ProcessExit(fixture.managed_health['process_id']))
        if fixture.managed_health['user']!=SID or fixture.managed_health.get('build',{}).get('source_sha256')!=args.source_sha256:
            raise ValueError('actual service human/build source differs')
        first_health=dict(fixture.managed_health)
        record('actual-start',first_health)
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
        def tool(name,argument_name):
            args_file=sources[name][0]/argument_name
            result=fixture.run('--service-runtime-dir',fixture.control,'tools','call','search',
                '--index',fixture.scopes['focus-'+name]['index_id'],'--args-file',args_file,
                check=False,timeout=min(120,remaining(cutoff)))
            envelope=json.loads(result.stdout)
            if result.returncode or envelope.get('isError'):raise RuntimeError('actual canonical native read failed')
            return clients.tool_value(envelope)
        # Publication and semantic readiness are actual metadata observations,
        # not repeated scoring requests. Keep one original setup cutoff per index.
        for name in sources:
            semantic_cutoff=min(cutoff,time.monotonic()+180)
            while True:
                result=fixture.run('--service-runtime-dir',fixture.control,'tools','call','airk_diagnostics',
                    '--index',fixture.scopes['focus-'+name]['index_id'],'--args','{"response_mode":"full"}',
                    check=False,timeout=remaining(semantic_cutoff))
                envelope=json.loads(result.stdout)
                if result.returncode or envelope.get('isError'):
                    raise RuntimeError('actual publication/semantic metadata observation refused')
                observed=clients.tool_value(envelope);remaining(semantic_cutoff)
                readiness=clients.semantic_readiness(observed)
                if readiness['state']=='failed':raise RuntimeError('normal MiniLM semantic work failed')
                if readiness['state']=='ready':
                    record('semantic-ready-'+name,observed);break
        baselines={name:tool(name,'search-hybrid.json') for name in sources}
        for value in baselines.values():
            if not value.get('hits') or value.get('execution',{}).get('served_mode')!='hybrid' or value['execution'].get('incomplete'):
                raise ValueError('normal bundled MiniLM hybrid execution was not proved')
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
        if not applied_focus(enabled) or applied_focus(excluded):raise ValueError('actual Focus application/enrollment isolation unproved')
        receipt=enabled['focus']['receipt']
        if (receipt['identity']['profile']!=FOCUS or receipt['scored_count']<=0
                or receipt.get('observed_input_tokens',0)<=0):raise ValueError('actual native Focus work absent')
        record('focus-applied',enabled);record('unenrolled-minilm',excluded)
        report['native_nn_calls_claimed']=True
        control('disable',index=a,source=source)
        disabled=tool('a','search-hybrid.json')
        if applied_focus(disabled) or disabled.get('execution',{}).get('served_mode')!='hybrid':
            raise ValueError('normal MiniLM behavior after disable absent')
        record('disabled-minilm',disabled)
        control('unenroll',index=a,source=source)
        removal_cutoff=min(cutoff,time.monotonic()+120)
        control('remove')
        removed=settle(lambda s:profile_row(s)['availability']=='missing_assets',removal_cutoff)
        record('removed',removed)
        if any(digest(Path(p))!=expected for p,expected in {**originals,**import_originals}.items()):raise ValueError('original source/import bytes changed')
        first_cutoff=min(cutoff,time.monotonic()+120);fixture.close()
        record('first-native-stop',fixture.managed_stop);record('first-process-exit',handles[-1].wait(first_cutoff))
        # Reacquire the real native lease through ordinary installed Ensure.
        fixture.managed_stop=None;fixture.managed_health=None
        fixture.service_run('runtime','ensure','--install-dir',fixture.install,timeout=remaining(cutoff))
        fixture.managed_health=fixture.rpc('health')['observation']
        if any(fixture.managed_health[key]!=first_health[key] for key in ('runtime_sha256','executable_sha256','user','version')) or fixture.managed_health['instance']==first_health['instance']:
            raise ValueError('restart failed to bind same artifact/new actual service instance')
        handles.append(ProcessExit(fixture.managed_health['process_id']))
        record('actual-restart-health',fixture.managed_health)
        restarted=status()
        if restarted['configuration']['focus']['enabled'] or profile_row(restarted)['availability']!='missing_assets':
            raise ValueError('persisted disabled/removed state did not survive native lease reacquisition')
        record('restarted-state',restarted)
        final_cutoff=min(cutoff,time.monotonic()+120);fixture.close()
        record('final-native-stop',fixture.managed_stop);record('final-process-exit',handles[-1].wait(final_cutoff))
        report['source_files_unchanged']=True
        report['import_source_files_unchanged']=True
        report['passed']=True
    except BaseException as error:
        report['failure']={'type':type(error).__name__,'message':str(error)}
        raise
    finally:
        if fixture is not None and fixture.managed_started and fixture.managed_stop is None:
            try:fixture.close()
            except BaseException as error:report['cleanup_failure']={'type':type(error).__name__,'message':str(error)}
        report['actual_controller_children']=[{'pid':p.pid,'exit_code':p.poll()} for p in children]
        report['actual_native_stop']=getattr(fixture,'managed_stop',None)
        report['elapsed_seconds']=time.monotonic()-started
        for handle in handles:handle.close()
        retain(output/'receipt.json',report)

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('runtime-dir','repository','service-test-exe','fixture','focus-assets','output'):
        parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--service-test-sha256',required=True)
    parser.add_argument('--source-sha256',required=True)
    parser.add_argument('--whole-phase-seconds',type=int,default=1800)
    parser.add_argument('--consent-focus-native-journey',action='store_true',required=True)
    options=parser.parse_args()
    if not 1<=options.whole_phase_seconds<=3600:parser.error('bounded original journey phase required')
    run(options)
