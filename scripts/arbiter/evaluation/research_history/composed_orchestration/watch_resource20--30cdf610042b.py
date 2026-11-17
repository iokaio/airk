# SPDX-License-Identifier: Apache-2.0
"""Add live capacity observations to the unchanged native operation journal owner."""
import ctypes,math,time
from ctypes import wintypes as w
from pathlib import Path
from capacity_resource20 import Capacity,WindowsRead

def make_watch(base,journal,stdout,identity,roster,deadline_ms,root):
    class Watch(base.WindowsWatch):
        def __init__(self):
            super().__init__(journal,stdout,identity,roster,deadline_ms,True)
            self.capacity=Capacity();self.reader=WindowsRead();self.process=self.job=None
        def inspect(self):
            super().inspect()
            if self.process is not None:
                files={}
                names={'graph':'compact-gqa-optimized.onnx','weights':'compact-gqa-optimized.weights.bin',
                    'body':'native-result.json','stdout':'stdout-stderr.log'}
                for key,name in names.items():
                    value=self.reader.file(Path(root)/name)
                    if value is not None:files[key]=value
                profiles=sorted(Path(root).glob('compact-gqa-profile*.json'))
                if len(profiles)>1:raise base.ObservationFailure('More than one original profiling output')
                for path in profiles:
                    value=self.reader.file(path)
                    if value is not None:files['profile']=value
                self.capacity.observe(files,self.reader.memory(self.process,self.job))
        def run(self,api,job,info,whole_deadline):
            self.process,self.job=info.process,job;started=time.monotonic()
            try:
                handles=(w.HANDLE*2)(self.change,info.process)
                while True:
                    self.inspect();now=self.now();self.state.check_expiry(now)
                    remaining=whole_deadline-time.monotonic()
                    if remaining<=0:raise base.ObservationFailure('Original3600s resource phase expired')
                    # A bounded WaitForMultipleObjects timeout is a live capacity
                    # observation tick, not a sleep, retry or renewed operation.
                    seconds=min(remaining,.1)
                    if self.state.expiry() is not None:seconds=min(seconds,max(0,(self.state.expiry()-now)/1e9))
                    waited=self.api.WaitForMultipleObjects(2,handles,False,max(1,math.ceil(seconds*1000)))
                    if waited==0:
                        if not self.api.FindNextChangeNotification(self.change):raise ctypes.WinError(ctypes.get_last_error())
                    elif waited==1:
                        self.inspect();self.state.check_expiry(self.now())
                        code=w.DWORD()
                        if not api.GetExitCodeProcess(info.process,ctypes.byref(code)):raise ctypes.WinError(ctypes.get_last_error())
                        if code.value==0 and (self.state.position!=len(self.state.roster) or self.state.active is None or self.state.active['phase']!='returned'):
                            raise base.ObservationFailure('Original22operation population incomplete')
                        return {'worker_pid':int(info.pid),'operation_watch':{**self.state.proof(),
                            'actual_process_exit_observed':True,'exit_code':int(code.value),'whole_deadline_not_renewed':True},
                            'capacity_watch':self.capacity.proof()},[]
                    elif waited!=258:raise ctypes.WinError(ctypes.get_last_error())
            except BaseException as error:
                error.operation_watch={**self.state.proof(),'failure':str(error),'elapsed_seconds':time.monotonic()-started,
                    'whole_deadline_not_renewed':True}
                error.capacity_watch=self.capacity.proof();raise
    return Watch()
