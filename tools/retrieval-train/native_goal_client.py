# SPDX-License-Identifier: Apache-2.0
"""Retain actual canonical CLI calls for the isolated public-fixture evaluator.

Arguments are native-owned JSON instances, never a parallel definition of Core
request types. This adapter owns no model, catalog, registration or setup policy.
"""
import hashlib
import json
import os
import queue
import shutil
import threading
from goal_sources import ResidentBinding
from pathlib import Path
import subprocess
import time


def digest(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()


def require_normal_session_close(outcome):
    """A successful reply cannot conceal an aborted native/helper process."""
    if (not isinstance(outcome,dict) or outcome.get('kind')!='closed'
        or outcome.get('helper_exit_observed') is not True or type(outcome.get('helper_exit_code')) is not int or outcome.get('helper_exit_code')!=0
        or outcome.get('child_exit_observed') is not True or type(outcome.get('child_exit_code')) is not int or outcome.get('child_exit_code')!=0
        or outcome.get('child_signal_code') is not None or outcome.get('poisoned') is not None
        or type(outcome.get('launch_count')) is not int or outcome.get('launch_count')!=1):
        raise ValueError('normal native session/helper exit was not observed')


def windows_process_identity(pid):
    """Observe the actual fixture process, including PID-reuse protection."""
    import ctypes
    from ctypes import wintypes
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD)]
    kernel.GetProcessTimes.argtypes = [wintypes.HANDLE, *([ctypes.POINTER(ctypes.c_ulonglong)] * 4)]
    kernel.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    handle = kernel.OpenProcess(0x1000 | 0x100000, False, pid)
    if not handle:raise OSError(ctypes.get_last_error(), 'fixture process observation refused')
    try:
        if kernel.WaitForSingleObject(handle, 0) != 258:raise ValueError('actual fixture process has exited')
        size = wintypes.DWORD(32768);path = ctypes.create_unicode_buffer(size.value)
        if not kernel.QueryFullProcessImageNameW(handle, 0, path, ctypes.byref(size)):raise OSError(ctypes.get_last_error(), 'fixture executable observation failed')
        times = [ctypes.c_ulonglong() for _ in range(4)]
        if not kernel.GetProcessTimes(handle, *(ctypes.byref(value) for value in times)):raise OSError(ctypes.get_last_error(), 'fixture process creation observation failed')
        return {'process_id': pid, 'executable': path.value, 'executable_sha256': digest(path.value), 'creation_filetime_100ns': times[0].value}
    finally:kernel.CloseHandle(handle)


def tool_value(envelope):
    if not isinstance(envelope,dict) or not isinstance(envelope.get('content'),list) or len(envelope['content'])!=1:
        raise ValueError('canonical CLI text envelope differs')
    content=envelope['content'][0]
    if content.get('type')!='text' or not isinstance(content.get('text'),str):raise ValueError('canonical CLI content differs')
    value=json.loads(content['text'])
    if not isinstance(value,dict):raise ValueError('canonical document is not an object')
    return value


class PinnedSession:
    """Existing generated Node I2 client; opaque native W4 selection, no DTO mirror."""
    def __init__(self,client,timeout):
        root=Path(__file__).resolve().parents[2]
        helper=Path(__file__).with_name('native_eval_session.cjs')
        self.lines=queue.Queue(maxsize=4);self.closed=False
        node=shutil.which('node')
        if not node:raise ValueError('pinned Node client runtime is unavailable')
        directory=client.output/'native-session';directory.mkdir()
        config={'runtime_pair':str(client.pair),'control_root':str(client.control),
                'fixture_ready':str(client.fixture_ready),'ready_sha256':digest(client.fixture_ready),
                'index_bindings':str(client.binding.path),'bindings_sha256':digest(client.binding.path),
                'index_id':client.index_id,'transcript_dir':str(directory/'i2-transcript')}
        for name,key in [('session.js','session_module_sha256'),('runner.js','runner_module_sha256'),('adminChunks.js','admin_chunks_sha256')]:
            config[key]=digest(root/'packages/vscode/out/airkctl'/name)
        path=directory/'config.json';path.write_text(json.dumps(config,indent=2)+'\n',encoding='utf-8')
        self.config=config;self.log=directory/'control-stdout.jsonl'
        self.stderr=(directory/'control-stderr.bin').open('wb')
        self.process=subprocess.Popen([node,str(helper),str(path)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.stderr,
                                      creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0))
        def reader():
            with self.log.open('wb') as out:
                while True:
                    line=self.process.stdout.readline(32*1024*1024+1)
                    if not line:self.lines.put(None);return
                    out.write(line);out.flush()
                    if len(line)>32*1024*1024:self.lines.put(ValueError('private session control result exceeds bound'));return
                    try:value=json.loads(line)
                    except (ValueError,UnicodeDecodeError) as error:value=error
                    self.lines.put(value)
        threading.Thread(target=reader,name='native-goal-session-output',daemon=True).start()
        self.ready=self.message(timeout)
        if self.ready.get('kind')!='ready' or self.ready.get('hello',{}).get('serviceInstance')!=client.service_identity['actual_native_health']['instance']:
            raise ValueError('native persistent session did not bind original fixture instance')

    def message(self,timeout):
        try:value=self.lines.get(timeout=max(0.001,timeout))
        except queue.Empty:raise TimeoutError('original native session deadline expired') from None
        if value is None:raise ValueError('native persistent session closed')
        if isinstance(value,Exception):raise ValueError('native session control result is invalid') from value
        if not isinstance(value,dict) or value.get('kind')=='failed':raise ValueError('native persistent session refused: '+str(value))
        return value

    def invoke(self,identifier,tool,args_file,timeout):
        if timeout<0.001:raise TimeoutError('original deadline cannot admit a one-millisecond native request')
        request={'id':identifier,'tool':tool,'args_file':str(args_file),'deadline_ms':max(1,int(timeout*1000))}
        self.process.stdin.write((json.dumps(request)+'\n').encode());self.process.stdin.flush()
        result=self.message(timeout)
        if result.get('kind')!='invoked' or result.get('id')!=identifier:raise ValueError('native persistent invocation identity differs')
        return result

    def close(self):
        if self.closed:return
        self.closed=True
        try:
            self.process.stdin.write(b'{"kind":"close"}\n');self.process.stdin.flush()
            outcome=self.message(20)
            if outcome.get('kind')!='closed':raise ValueError('native session close was not observed')
            self.process.wait(timeout=20)
            outcome['helper_exit_code']=self.process.returncode
            outcome['helper_exit_observed']=True
            return outcome
        finally:
            self.stderr.flush()
        # No kill or fabricated successful cleanup on timeout.


class NativeGoalClient:
    """Read-only, one invocation per call; never retries or invents readiness."""
    def __init__(self,runtime_pair,control_root,index_id,output,fixture_ready=None,process_observer=None,*,index_bindings=None,session_factory=None):
        self.pair=Path(runtime_pair).resolve();self.control=Path(control_root).resolve();self.index_id=index_id
        self.output=Path(output).resolve()
        root=Path(__file__).resolve().parents[2]/'artifacts'
        if self.output.exists() or not self.output.is_relative_to(root):raise ValueError('fresh private native call directory required')
        self.binding=ResidentBinding(index_bindings,index_id=index_id) if index_bindings is not None else None
        if fixture_ready and self.binding is None:raise ValueError('resident evaluation requires original native index bindings')
        if self.binding:
            for row in self.binding.receipt['bindings']:
                source=Path(row['source_root']).resolve()
                if self.output.is_relative_to(source) or any(parent.exists() and parent.samefile(source) for parent in self.output.parents):
                    raise ValueError('native evaluation metadata must stay outside frozen source roots')
        self.metadata_bytes=(self.pair/'airk-runtime.json').read_bytes();metadata=json.loads(self.metadata_bytes)
        self.images={name:self.pair/(name+'.exe') for name in ('airk','airkctl')}
        self.pins={name:digest(path) for name,path in self.images.items()}
        if any(self.pins[name]!=metadata[name+'_sha256'] for name in self.images):raise ValueError('matched runtime image hash differs')
        self.source_bytes=Path(__file__).read_bytes();self.output.mkdir(parents=True)
        (self.output/'airk-runtime-as-consumed.json').write_bytes(self.metadata_bytes)
        (self.output/'adapter.py').write_bytes(self.source_bytes)
        self.fixture_ready=Path(fixture_ready).resolve() if fixture_ready else None
        self.fixture_seals={};self.service_identity=None;self.process_observer=process_observer or windows_process_identity
        if self.fixture_ready:
            raw=self.fixture_ready.read_bytes();ready=json.loads(raw)
            if ready.get('schema')!='airk.private-arbiter-fixture/1' or ready.get('evaluation') is not True or ready.get('release_qualified') is not False or not Path(ready['control_root']).samefile(self.control):raise ValueError('unreviewed native evaluation fixture Ready')
            health=ready['health'];observed=self.process_observer(health['process_id'])
            if observed['process_id']!=health['process_id'] or observed['executable_sha256']!=health['executable_sha256'] or not health.get('instance') or not health.get('catalog_clock',{}).get('namespace'):raise ValueError('actual fixture process differs from native Ready health')
            self.fixture_seals[self.fixture_ready]=raw
            input_path=self.fixture_ready.parent/'inputs.json';input_raw=input_path.read_bytes()
            if hashlib.sha256(input_raw).hexdigest()!=ready['inputs_sha256'] or len(ready['profiles'])!=4:raise ValueError('native fixture input/profile closure differs')
            self.fixture_seals[input_path]=input_raw
            for row in ready['profiles']:
                name=row['snapshot']
                if Path(name).name!=name:raise ValueError('foreign native fixture profile snapshot')
                path=self.fixture_ready.parent/name;content=path.read_bytes()
                if hashlib.sha256(content).hexdigest()!=row['profile_sha256']:raise ValueError('native fixture profile snapshot identity differs')
                self.fixture_seals[path]=content
            self.service_identity={'ready_sha256':hashlib.sha256(raw).hexdigest(),'actual_native_health':health,'actual_process_observation':observed,'running_service_is_matched_pair_airk':False,'scope':'Actual native test-binary fixture process; matched pair pins below identify the client distribution, not the running service executable.'}
            (self.output/'actual-service-ready.json').write_bytes(raw)
        if self.fixture_ready:
            if self.binding is None:raise ValueError('resident evaluation requires original native index bindings')
            if (self.binding.receipt['instance']!=health['instance'] or self.binding.namespace!=health['catalog_clock']['namespace']
                or not Path(self.binding.receipt['control_root']).samefile(self.control)):
                raise ValueError('original native binding service instance/namespace differs')
        self.adapter_seals={}
        for path in [Path(__file__).with_name('goal_sources.py'),Path(__file__).with_name('native_eval_session.cjs')]:self.adapter_seals[path]=path.read_bytes()
        self.session=None;self.session_factory=session_factory or PinnedSession
        self.sequence=0

    def verify(self):
        if Path(__file__).read_bytes()!=self.source_bytes or (self.pair/'airk-runtime.json').read_bytes()!=self.metadata_bytes:
            raise ValueError('native goal adapter/runtime metadata changed')
        if any(digest(path)!=self.pins[name] for name,path in self.images.items()):raise ValueError('matched native images changed')
        if any(path.read_bytes()!=raw for path,raw in self.fixture_seals.items()):raise ValueError('native fixture Ready/profile drift')
        if self.binding:self.binding.verify()
        if any(path.read_bytes()!=raw for path,raw in self.adapter_seals.items()):raise ValueError('native evaluation adapter changed')
        if self.service_identity and self.process_observer(self.service_identity['actual_native_health']['process_id'])!=self.service_identity['actual_process_observation']:raise ValueError('actual fixture process changed or PID reused')

    def observe_health(self,directory,phase,timeout):
        command=[str(self.images['airkctl']),'--service-runtime-dir',str(self.control),'service','health']
        started=time.monotonic()
        process=subprocess.run(command,capture_output=True,timeout=timeout,check=False)
        elapsed=time.monotonic()-started
        (directory/(phase+'-stdout.bin')).write_bytes(process.stdout)
        (directory/(phase+'-stderr.bin')).write_bytes(process.stderr)
        if process.returncode!=0:raise ValueError('actual fixture health observation failed')
        response=json.loads(process.stdout)
        actual=response.get('observation') if isinstance(response,dict) else None
        expected=self.service_identity['actual_native_health']
        if (not isinstance(actual,dict) or not isinstance(actual.get('catalog_clock'),dict)
            or response.get('status')!='health' or any(actual.get(key)!=expected.get(key) for key in ('instance','process_id','executable_sha256','version','protocol','user'))
            or actual['catalog_clock'].get('namespace')!=expected['catalog_clock']['namespace']):
            raise ValueError('actual fixture instance/namespace/identity changed')
        result={'command':command,'elapsed_seconds':elapsed,'full_control_response':response,
                'stdout_sha256':hashlib.sha256(process.stdout).hexdigest(),'stderr_sha256':hashlib.sha256(process.stderr).hexdigest(),
                'scope':'Native management observation, not a canonical evidence read. Counted separately; elapsed time charged within this invocation deadline.'}
        (directory/(phase+'.json')).write_text(json.dumps(result,indent=2)+'\n')
        return result

    def call(self,tool,arguments,remaining_canonical_seconds):
        adapter_started=time.monotonic()
        if tool not in ('search','read_file','context','answer'):raise ValueError('evaluator may invoke only approved canonical read tools')
        if not isinstance(arguments,dict) or not 0<remaining_canonical_seconds<=30:raise ValueError('invalid native arguments/deadline')
        self.verify();self.sequence+=1;directory=self.output/f'call-{self.sequence:04d}';directory.mkdir()
        args_bytes=(json.dumps(arguments,ensure_ascii=False,indent=2)+'\n').encode();args_path=directory/'arguments.json';args_path.write_bytes(args_bytes)
        command=[str(self.images['airkctl']),'--service-runtime-dir',str(self.control),'tools','call',tool,'--index',self.index_id,'--args-file',str(args_path)]
        overall_rpc_start=time.monotonic();health=[]
        if self.service_identity:health.append(self.observe_health(directory,'health-before',remaining_canonical_seconds))
        remaining=remaining_canonical_seconds-(time.monotonic()-overall_rpc_start)
        if remaining<=0:raise ValueError('identity observations exhausted native invocation deadline')
        start=time.monotonic();timed_out=False;identity_error=None;session_exchange=None
        try:
            if self.service_identity:
                if self.session is None:
                    self.session=self.session_factory(self,remaining)
                remaining=remaining_canonical_seconds-(time.monotonic()-overall_rpc_start)
                if remaining<=0:raise TimeoutError('original native invocation budget exhausted before dispatch')
                session_exchange=self.session.invoke(str(self.sequence),tool,args_path,remaining)
                result=session_exchange['result'];stdout=result['stdout'].encode();stderr=result['stderr'].encode();code=result['exitCode']
                command=session_exchange['argv']
                if session_exchange['arguments_sha256']!=hashlib.sha256(args_bytes).hexdigest():raise ValueError('native session arguments changed')
            else:
                process=subprocess.run(command,capture_output=True,timeout=remaining,check=False)
                stdout,stderr,code=process.stdout,process.stderr,process.returncode
        except (subprocess.TimeoutExpired,TimeoutError) as error:
            stdout,stderr,code=getattr(error,'stdout',None) or b'',getattr(error,'stderr',None) or b'',None;timed_out=True
        except (ValueError,OSError,KeyError,TypeError) as error:
            if isinstance(session_exchange,dict) and isinstance(session_exchange.get('result'),dict):
                result=session_exchange['result'];stdout=result.get('stdout','').encode();stderr=result.get('stderr','').encode();code=result.get('exitCode')
            else:stdout,stderr,code=b'',b'',None
            identity_error=str(error)
        read_elapsed=time.monotonic()-start
        (directory/'stdout.bin').write_bytes(stdout);(directory/'stderr.bin').write_bytes(stderr)
        parsed=None;parse_error=None;envelope=None
        try:
            envelope=json.loads(stdout);parsed=tool_value(envelope)
        except (UnicodeDecodeError,ValueError,TypeError,KeyError) as error:parse_error=str(error)
        if self.service_identity and code==0 and isinstance(parsed,dict) and not envelope.get('isError'):
            try:self.binding.validate(parsed)
            except (ValueError,KeyError,TypeError) as error:identity_error=str(error)
        remaining=remaining_canonical_seconds-(time.monotonic()-overall_rpc_start)
        if self.service_identity and not timed_out:
            if remaining<=0:timed_out=True
            else:
                try:health.append(self.observe_health(directory,'health-after',remaining))
                except (ValueError,OSError,subprocess.TimeoutExpired) as error:identity_error=str(error)
        elapsed=time.monotonic()-overall_rpc_start
        receipt={'schema':'airk.development-native-goal-call/1','command':command,'tool':tool,'index_id':self.index_id,
            'arguments_sha256':hashlib.sha256(args_bytes).hexdigest(),'runtime_image_pins':self.pins,
            'actual_service_identity':self.service_identity,
            'native_binding_sha256':digest(self.binding.path) if self.binding else None,'session_exchange':session_exchange,
            'identity_error':identity_error,'validated':not identity_error and not parse_error and not timed_out and code==0,
            'elapsed_seconds':elapsed,'deadline_seconds':remaining_canonical_seconds,'timed_out':timed_out,'returncode':code,
            'canonical_read_process_seconds':read_elapsed,'identity_management_cli_invocations':len(health),'actual_health_observations':health,
            'stdout_sha256':hashlib.sha256(stdout).hexdigest(),'stderr_sha256':hashlib.sha256(stderr).hexdigest(),
            'stdout_bytes':len(stdout),'stderr_bytes':len(stderr),
            'envelope':envelope,'full_canonical_value':parsed,'parse_error':parse_error,
            'scope':'Actual canonical read through original-instance W4/I2 in resident fixture mode (otherwise one-shot); all full result fields retained. No automatic retry, no normalized subset, no quality/readiness claim. Caller must charge actual inner work/bytes and authenticate original source/citation ranges before grading.'}
        try:
            self.verify()
            if args_path.read_bytes()!=args_bytes:raise ValueError('native goal call arguments changed')
        except (ValueError,OSError) as error:
            receipt['identity_error']=str(error);receipt['validated']=False
        receipt['adapter_preprocessing_seconds']=max(0.,time.monotonic()-adapter_started-elapsed)
        (directory/'receipt.json').write_text(json.dumps(receipt,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
        return receipt

    def close(self):
        if self.session is not None:return self.session.close()
        return {'session_started':False,'native_child_launched':False}
