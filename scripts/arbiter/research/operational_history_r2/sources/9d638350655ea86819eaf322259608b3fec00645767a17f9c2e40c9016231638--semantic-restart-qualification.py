# SPDX-License-Identifier: Apache-2.0
"""Windows original-fixture first-status and independent-client qualification."""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import time


def require(value, code):
    if not value:
        raise AssertionError(code)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_module(name, path, expected):
    require(digest(path) == expected, 'qualification_helper_changed')
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class OwnedProcess:
    """Read-only process handle, pinned before shutdown; never kills by discovered PID."""
    def __init__(self, pid, install, expected_sha):
        self.api = ctypes.WinDLL('kernel32', use_last_error=True)
        self.api.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        self.api.OpenProcess.restype = wintypes.HANDLE
        self.api.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD,
                                                       wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD)]
        self.api.QueryFullProcessImageNameW.restype = wintypes.BOOL
        self.api.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
        self.api.WaitForSingleObject.restype = wintypes.DWORD
        self.api.CloseHandle.argtypes = [wintypes.HANDLE]
        self.api.CloseHandle.restype = wintypes.BOOL
        require(type(pid) is int and pid > 0, 'invalid_owned_service_pid')
        self.handle = self.api.OpenProcess(0x100000 | 0x1000, False, pid)
        require(self.handle, 'owned_process_open_failed')
        try:
            buffer, size = ctypes.create_unicode_buffer(32768), wintypes.DWORD(32768)
            require(self.api.QueryFullProcessImageNameW(self.handle, 0, buffer, ctypes.byref(size)),
                    'owned_process_identity_unavailable')
            actual = Path(buffer.value)
            candidates = list(install.rglob('*.exe'))
            require(0 < len(candidates) <= 128, 'owned_executable_inventory_bound')
            matches = []
            for candidate in candidates:
                require(not candidate.is_symlink() and not candidate.lstat().st_file_attributes & 0x400,
                        'owned_executable_reparse')
                require(candidate.resolve(strict=True).is_relative_to(install.resolve(strict=True)),
                        'owned_executable_outside_install')
                if actual.samefile(candidate):
                    matches.append(candidate)
            require(matches and all(digest(path) == expected_sha for path in matches),
                    'service_process_not_owned_payload')
        except BaseException:
            self.close()
            raise

    def wait(self):
        require(self.api.WaitForSingleObject(self.handle, 45000) == 0, 'owned_service_drain_timeout')

    def close(self):
        if self.handle:
            self.api.CloseHandle(self.handle)
            self.handle = None


def prepare_restart_eligibility(fixture, identity):
    """Prepare a healthy saved service before stopping it; never reset its circuit."""
    path = fixture.install / f'startup-{identity}.json'
    times = []
    if path.exists():
        require(not path.is_symlink() and not path.lstat().st_file_attributes & 0x400
                and path.resolve(strict=True).parent.samefile(fixture.install), 'unsafe_startup_receipt')
        with path.open('rb') as stream:
            raw = stream.read(4097)
        require(len(raw) <= 4096, 'oversized_startup_receipt')
        value = json.loads(raw)
        require(isinstance(value, dict) and set(value) == {'attempts_unix_ms'}, 'invalid_startup_receipt')
        times = value['attempts_unix_ms']
        require(isinstance(times, list) and len(times) <= 3
                and all(type(t) is int and 0 <= t < 2**64 for t in times), 'invalid_startup_attempts')
    now = time.time_ns() // 1000000
    live = [t for t in times if max(0, now - t) < 120000]
    require(len(live) < 3, 'fresh_fixture_startup_circuit_open')
    eligible_at = live[-1] + (1 << len(live)) * 1000 if live else now
    require(eligible_at - now <= 8000, 'fixture_startup_clock_not_ready')
    began, deadline = time.monotonic(), time.monotonic() + 10
    while time.time_ns() // 1000000 < eligible_at:
        require(time.monotonic() < deadline, 'fixture_startup_eligibility_deadline')
        time.sleep(min(0.25, max(0, eligible_at - time.time_ns() // 1000000) / 1000))
    return {'prior_attempt_count': len(live), 'waited_ms': round((time.monotonic() - began) * 1000, 3),
            'before_controlled_shutdown': True, 'reset': False}


def arguments_for(fixture, source, data, binding, pinned):
    args = ['--service-install-dir', fixture.install, '--root', source]
    if pinned:
        args.extend(['--service-project', binding, '--data-dir', data, '--collection', 'healthy'])
    return args




def observe(session, label, mode, decode):
    started=time.perf_counter()
    row={'label':label,'mode':mode,'outcome':'transport_failure','attempts':1}
    try:
        envelope=session.tool('search',{'query':'healthyfixturebeacon reference','mode':mode,'response_mode':'full'},deadline=time.monotonic()+30)
        row['outcome']='response' # complete frame, even if its envelope is malformed
        if not isinstance(envelope,dict):
            row['envelope_schema']='non_object'
            raise ValueError('invalid_complete_response_envelope')
        row['envelope_schema']='object'
        row['is_error']=envelope.get('isError',False)
        # Exact envelope is already retained by the private Session transcript.
        # Aggregate metadata contains no source excerpts/query/error text.
        value=decode(envelope)
        row['response_sha256']=hashlib.sha256(json.dumps(envelope,sort_keys=True,ensure_ascii=True).encode()).hexdigest()
        if not isinstance(value,dict):
            row['response_schema']='non_object'
        else:
            publication=value.get('publication')
            token=publication.get('token') if isinstance(publication,dict) else None
            encoded=json.dumps(token,sort_keys=True,ensure_ascii=True).encode()
            token_bounded=len(encoded)<=16384
            collection=value.get('collection')
            row['response_schema']='object'
            row['response_metadata']={
                'collection':collection if isinstance(collection,str) and len(collection)<=128 else None,
                'publication_token':token if token_bounded else None,
                'publication_token_omitted':not token_bounded,
                'hits_count':len(value['hits']) if isinstance(value.get('hits'),list) else None,
            }
    except Exception as error:
        row.update(failure_category='complete_response_decode_failed' if row['outcome']=='response' else 'first_semantic_transport_failed',exception_type=type(error).__name__[:64])
    row['elapsed_ms']=round((time.perf_counter()-started)*1000,3)
    return row

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime',type=Path,required=True)
    parser.add_argument('--approved-sha256',required=True)
    parser.add_argument('--task-root',type=Path,required=True)
    parser.add_argument('--protected-root',type=Path,action='append',required=True)
    parser.add_argument('--report-tag',default='semantic-restart')
    args=parser.parse_args()
    require(os.name=='nt','windows_only')
    task,runtime=args.task_root.resolve(strict=True),args.runtime.resolve(strict=True)
    protected=[p.resolve(strict=True) for p in args.protected_root]
    require(all(not task.is_relative_to(p) and not p.is_relative_to(task) for p in protected),'protected_overlap')
    for directory in (task/'fixtures',task/'reports',task/'scripts'):
        require(directory.is_dir() and not directory.is_symlink() and not directory.lstat().st_file_attributes&0x400 and directory.resolve(strict=True).is_relative_to(task),'unsafe_task_directory')
    require(digest(runtime/'airk-install.json')==args.approved_sha256,'unapproved_pair')
    scripts=task/'scripts'
    clients=load_module('semantic_clients',scripts/'check-service-clients.py','a4588c7202f3ba3aee27057bf0aa9e15dfeb627815ee61e3c8df249491d6984b')
    scope=load_module('semantic_scope',scripts/'check-source-scope-original-identity.py','02d6c9573972cb1b21aa67813c29d1effde48bd7237c6374b8b36e7a542e82ae')
    require(digest(scripts/'check-service-pool.py')=='6a35e24cb66497b722c6c252d23d7c6203dd6402be2eba25bdc935cd5102ae5f','pool_helper_changed')
    require(1<=len(args.report_tag)<=64 and all(c.isascii() and (c.isalnum() or c in '-_') for c in args.report_tag),'invalid_report_tag')
    reports=task/'reports'/args.report_tag;reports.mkdir()
    result={'schema':1,'completed':False,'cleanup':False,'runtime_manifest_sha256':args.approved_sha256,'first_observations':[],'cohort':[],'saved_indexes':[],'limits':['Windows functional fixture; uncontrolled OS caches','helper may already be ready: cold success does not prove wait path','no helper authority/handshake/inference counters measured','one query permit; cohort sequential; no submitted read retry']}
    fixtures=[];owners=[];inventories=[];sessions=[];stage='setup'
    class Installed(clients.Fixture):
        def __init__(self,owner):
            super().__init__(runtime,owner)
            self.install,self.state=owner/'install',owner/'state'
            self.control,self.staged=self.state/'control',False
        def activate(self):
            self.run('runtime','stage','--source',runtime,'--install-dir',self.install,'--state-dir',self.state,'--approved-sha256',args.approved_sha256,'--verification-basis','organization-digest')
            self.staged=True
            self.run('runtime','activate','--install-dir',self.install,'--runtime-sha256',args.approved_sha256)
        def close(self):
            try:
                if self.staged:self.run('runtime','uninstall','--install-dir',self.install)
            finally:self.log.close()
    definitions=[('source-vector',False,'vector'),('pinned-hybrid',True,'hybrid'),('source-hybrid',False,'hybrid'),('pinned-vector',True,'vector')]
    def qualified(row,publication):
        value=row.get('response_metadata',{})
        return (row['outcome']=='response' and not row.get('is_error',True)
            and row.get('response_schema')=='object' and value.get('collection')=='healthy'
            and type(value.get('hits_count')) is int and value['hits_count']>0
            and not value.get('publication_token_omitted',True)
            and value.get('publication_token')==publication)
    try:
        for label,pinned,mode in definitions:
            stage='prepare_'+label
            owner=Path(tempfile.mkdtemp(prefix='semantic-restart-',dir=task/'fixtures')).resolve(strict=True)
            owners.append(owner)
            require(not owner.is_relative_to(runtime) and not runtime.is_relative_to(owner) and all(not owner.is_relative_to(p) and not p.is_relative_to(owner) for p in protected),'owner_overlap')
            source,data=owner/'source',owner/'data';source.mkdir();data.mkdir()
            (source/'README.md').write_text('Original healthyfixturebeacon reference.\n',encoding='utf-8')
            (source/'entry.rs').write_text('fn healthyfixturebeacon() {}\n',encoding='utf-8')
            inventories.append((source,scope.source_inventory([source])))
            fixture=Installed(owner);fixtures.append(fixture);fixture.activate()
            fixture.register('healthy',source,data);binding=fixture.projects['healthy']['binding']
            fixture.rpc({'configure_watch':{'project':binding,'enabled':False}})
            require(fixture.rpc({'watch':{'project':binding}})['observation']['enabled'] is False,'watch_not_disabled')
            fixture.configure('healthy',{'operation':'vectors','enabled':True})
            fixture.job('healthy','setup-model');fixture.job('healthy','index')
            fixture.wait(fixture.enqueue('healthy','vectors'))
            prepared=fixture.tool('healthy','airk_status',{'response_mode':'full'})
            require(prepared.get('index_state')=='available' and prepared.get('files')==2 and clients.semantic_readiness(prepared)['state']=='ready','saved_semantics_not_ready')
            publication=prepared['publication']['token']
            result['saved_indexes'].append({'label':label,'publication':publication,'files':prepared['files'],'chunks':prepared.get('chunks'),'startup_preparation':prepare_restart_eligibility(fixture,args.approved_sha256)})
            health=fixture.rpc('health')['observation']
            require(health['runtime_sha256']==args.approved_sha256,'foreign_health')
            process=OwnedProcess(health['process_id'],fixture.install,health['executable_sha256'])
            try:
                require(fixture.rpc('shutdown').get('status')=='stopping','shutdown_not_acknowledged');process.wait()
            finally:process.close()
            # No metadata/status/diagnostics/model/evidence/wait after drain;
            # only static initialize/projects before exact first semantic request.
            stage='cold_'+label
            session=scope.Session(fixture,arguments_for(fixture,source,data,binding,pinned),label,reports)
            sessions.append(session);session.start()
            static=scope.body(session.request('tools/call',{'name':'projects','arguments':{}}))
            session.expected_project=static['default_project']
            row=observe(session,label,mode,scope.body)
            result['first_observations'].append(row) # retain first outcome BEFORE validation
            row.update(frontend_pid=session.child.pid,prior_service_drained=True)
            row['qualified']=qualified(row,publication)
            session.close();sessions.remove(session)
        cold_passed=all(row['qualified'] for row in result['first_observations'])
        result['cold_outcome']='qualified' if cold_passed else 'failed'
        if cold_passed:
            stage='separate_sequential_cohort'
            for label,pinned,mode in definitions:
                session=scope.Session(fixture,arguments_for(fixture,source,data,binding,pinned),'cohort-'+label,reports)
                sessions.append(session);session.start()
            require(len({s.child.pid for s in sessions})==4,'frontends_not_independent')
            worker_before=fixture.rpc({'worker':{'project':binding}})
            require(isinstance(worker_before.get('identity'),dict) and worker_before['identity'].get('project')==binding,'cohort_worker_missing')
            result['cohort_worker_before']=worker_before['identity']
            for session,(label,_,mode) in zip(sessions,definitions):
                row=observe(session,label,mode,scope.body)
                result['cohort'].append(row) # retain first outcome BEFORE validation
                row['qualified']=qualified(row,publication)
            result['cohort_worker_after']=fixture.rpc({'worker':{'project':binding}}).get('identity')
            require(result['cohort_worker_after']==result['cohort_worker_before'],'cohort_worker_changed')
            require(all(row['qualified'] for row in result['cohort']),'cohort_failed')
        for source,inventory in inventories:require(scope.source_inventory([source])==inventory,'source_inventory_changed')
        result['source_inventories_preserved']=True
        require(cold_passed,'one_or_more_cold_first_failures')
        result['completed']=True
    except Exception:
        result['failed_stage']=stage;raise
    finally:
        primary=sys.exc_info()[1];errors=[]
        for session in reversed(sessions):
            try:session.close()
            except Exception:errors.append('session_cleanup_failed')
        # Verify source bytes/names before any fixture directory may be removed,
        # including setup/query/schema/transport failures. A mismatch is preserved.
        inventory_checks=[]
        for source,inventory in inventories:
            try:
                matched=scope.source_inventory([source])==inventory
                inventory_checks.append({'ordinal':len(inventory_checks),'matched':matched})
                if not matched:errors.append('source_inventory_changed')
            except Exception:
                inventory_checks.append({'ordinal':len(inventory_checks),'matched':False,'observation_failed':True})
                errors.append('source_inventory_observation_failed')
        result['final_source_inventories']=inventory_checks
        result['source_inventories_preserved']=all(check['matched'] for check in inventory_checks)
        for fixture in reversed(fixtures):
            try:fixture.close()
            except Exception:errors.append('installation_cleanup_failed')
        # Cleanup is also source-read-only: verify again after uninstall/drain,
        # before removing any directory, keeping both independent observations.
        post_checks=[]
        for source,inventory in inventories:
            try:
                matched=scope.source_inventory([source])==inventory
                post_checks.append({'ordinal':len(post_checks),'matched':matched})
                if not matched:errors.append('post_uninstall_source_inventory_changed')
            except Exception:
                post_checks.append({'ordinal':len(post_checks),'matched':False,'observation_failed':True})
                errors.append('post_uninstall_source_inventory_observation_failed')
        result['post_uninstall_source_inventories']=post_checks
        result['source_inventories_preserved']=(result['source_inventories_preserved']
            and all(check['matched'] for check in post_checks))
        if not errors:
            for owner in reversed(owners):
                try:
                    parent=(task/'fixtures').resolve(strict=True)
                    require(owner.parent==parent and owner.name.startswith('semantic-restart-') and owner.resolve(strict=True).is_relative_to(parent) and not owner.is_symlink() and not owner.lstat().st_file_attributes&0x400,'unsafe_cleanup')
                    shutil.rmtree(owner)
                except Exception:errors.append('filesystem_cleanup_failed')
        result['cleanup']=not errors;result['cleanup_errors']=errors
        result['completed']=result['completed'] and result['cleanup']
        with (reports/'report.json').open('x',encoding='utf-8') as stream:json.dump(result,stream,indent=2);stream.write('\n')
        print(json.dumps(result,indent=2),flush=True)
        if errors:
            if primary is not None:primary.add_note('owned cleanup also failed')
            else:raise AssertionError('owned_cleanup_failed')

if __name__=='__main__':main()
