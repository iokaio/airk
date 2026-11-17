# SPDX-License-Identifier: Apache-2.0
"""Actual same-human Win32 notification probe; no AIRK/native model process."""
from pathlib import Path
import ctypes,hashlib,importlib.util,json,os,threading,time,traceback
from ctypes import wintypes as w
BASE=Path(__file__).resolve().parent
OLD=BASE.parent/'r154-composed-launch-preparation-r1'
def load(path,name):
    spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
route=load(OLD/'launch_route.py','original_r154_route')
changes=load(OLD/'shared/check-arbiter-lifecycle.py','original_changes').Changes
api=ctypes.WinDLL('kernel32',use_last_error=True)
api.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];api.OpenProcess.restype=w.HANDLE
api.CloseHandle.argtypes=[w.HANDLE];api.CloseHandle.restype=w.BOOL
rows=[]
for ordinal in range(3):
    root=BASE/('shared-subtree-'+str(ordinal));root.mkdir();ready=root/'work'/'nested'/'ready.json'
    held=api.OpenProcess(0x100000,False,os.getpid());assert held
    watcher=changes(root);subject=route.NativeRoute({},root,time.monotonic()+5)
    class Alive:
        def poll(self):return None
    class Held:handle=held
    subject.service=Alive();subject.handle=Held();start=threading.Event();errors=[]
    payload=json.dumps({'observation':'atomic complete record','padding':'x'*2333}).encode()
    def publish():
        try:
            assert start.wait(5);ready.parent.mkdir(parents=True)
            temporary=ready.with_name('temporary')
            with temporary.open('xb') as stream:stream.write(payload);stream.flush();os.fsync(stream.fileno())
            os.replace(temporary,ready)
        except BaseException as error:errors.append(repr(error))
    thread=threading.Thread(target=publish);thread.start();started=time.monotonic();start.set();wakes=0
    record={'case':ordinal,'recursive':True,'original_frozen_driver_sha256':hashlib.sha256((OLD/'launch_route.py').read_bytes()).hexdigest()}
    try:
        while not ready.exists():subject.wait_event(watcher,started+5);wakes+=1
        api.CreateFileW.argtypes=[w.LPCWSTR,w.DWORD,w.DWORD,ctypes.c_void_p,w.DWORD,w.DWORD,w.HANDLE]
        api.CreateFileW.restype=w.HANDLE
        original=api.CreateFileW(str(ready),0x80000000,3,None,3,0x00200000,None)
        ordinary_error=ctypes.get_last_error() if original==w.HANDLE(-1).value else None
        if original!=w.HANDLE(-1).value:api.CloseHandle(original)
        shared=api.CreateFileW(str(ready),0x80000000,7,None,3,0x00200000,None)
        shared_error=ctypes.get_last_error() if shared==w.HANDLE(-1).value else None
        record.update(ordinary_share_error=ordinary_error,delete_share_error=shared_error)
        assert shared!=w.HANDLE(-1).value,(ordinary_error,shared_error)
        import msvcrt
        with os.fdopen(msvcrt.open_osfhandle(shared,os.O_RDONLY|os.O_BINARY),'rb') as reader:
            info=os.fstat(reader.fileno());raw=reader.read(8193)
        record['links']=info.st_nlink;assert info.st_nlink==1
        assert raw==payload;assert not errors
        record.update(passed=True,wakes=wakes,bytes=len(raw),elapsed_seconds=time.monotonic()-started)
    except BaseException as error:record.update(passed=False,wakes=wakes,error=repr(error),traceback=traceback.format_exc())
    finally:
        thread.join(timeout=5);watcher.close();assert api.CloseHandle(held)
        record['writer_thread_settled']=not thread.is_alive();record['writer_errors']=errors;rows.append(record)
report={'schema':'airk.private-r154-original-win32-wait-probe/1','actual_native_service_or_model':False,'cases':rows,'passed':all(row['passed'] and row['writer_thread_settled'] for row in rows)}
(BASE/'delete-shared-result.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
raise SystemExit(0 if report['passed'] else 1)
