# SPDX-License-Identifier: Apache-2.0
"""Private Windows watcher for native Rust-emitted operation observations.
No native process is launched by this module. A Root-owned launcher supplies the
actual suspended-assigned direct-child process/job handles and frozen metadata.
"""
import ctypes
from ctypes import wintypes as w
import json
import math
import os
from pathlib import Path
import time

class ObservationFailure(ValueError):
    pass

class Operations:
    def __init__(self, identity, roster, deadline_ms, numeric=False):
        self.identity, self.roster = identity, roster
        self.deadline_ns = deadline_ms * 1000000
        self.numeric = numeric
        self.raw = b''
        self.sequence = self.position = 0
        self.active = None
        self.resource_digest = None
        self.events = []

    def expiry(self):
        return self.active['expires_monotonic_ns'] if self.active is not None else None

    def check_expiry(self, now):
        expires = self.expiry()
        if expires is not None and now >= expires:
            raise ObservationFailure('original native operation expiry reached; no next-start/process-exit observation')

    def feed(self, raw, now):
        if len(raw) > 131072 or not raw.startswith(self.raw):
            raise ObservationFailure('native operation journal bound/prefix changed')
        complete = raw[:raw.rfind(b'\n')+1] if b'\n' in raw else b''
        lines = complete.splitlines()
        for line in lines[self.sequence:]:
            event = json.loads(line)
            fields = {'schema','sequence','identity','kind','ordinal','operation_input_digest','phase','clock',
                'started_monotonic_ns','expires_monotonic_ns','observed_monotonic_ns',
                'native_call_elapsed_us','native_success'}
            if set(event) != fields or event['schema'] != 'airk.arbiter.native-operation/1':
                raise ObservationFailure('unreviewed native operation event shape')
            if event['identity'] != self.identity or event['clock'] != 'windows_performance_counter':
                raise ObservationFailure('native operation PID/image/input/profile/clock differs')
            for key in ['sequence','ordinal','started_monotonic_ns','expires_monotonic_ns','observed_monotonic_ns']:
                if type(event[key]) is not int or event[key] < 0:
                    raise ObservationFailure('invalid native unsigned operation field: ' + key)
            if event['sequence'] != self.sequence+1:
                raise ObservationFailure('native operation sequence duplicate/gap')
            if event['expires_monotonic_ns'] != event['started_monotonic_ns'] + self.deadline_ns:
                raise ObservationFailure('native operation did not retain the original policy allowance')
            if event['observed_monotonic_ns'] > now:
                raise ObservationFailure('native operation clock is ahead of the observing host counter')
            phase = event['phase']
            if phase == 'started':
                if self.active is not None:
                    self.check_expiry(now)
                    if self.active['phase'] != 'returned' or self.active['native_success'] is not True:
                        raise ObservationFailure('native operation overlapped or continued after a known native error')
                    if event['started_monotonic_ns'] < self.active['observed_monotonic_ns']:
                        raise ObservationFailure('native operation clock moved backwards')
                if self.position >= len(self.roster):
                    raise ObservationFailure('extra native operation after frozen population')
                expected = self.roster[self.position]
                if (event['kind'],event['ordinal']) != (expected['kind'],expected['ordinal']):
                    raise ObservationFailure('native operation kind/ordinal differs from frozen input')
                if (event['native_success'] is not None or event['native_call_elapsed_us'] is not None
                    or event['observed_monotonic_ns'] != event['started_monotonic_ns']):
                    raise ObservationFailure('prospective start cannot claim a native result')
                actual = event['operation_input_digest']
                if expected['kind'] == 'load':
                    if actual is not None: raise ObservationFailure('load has unexpected compiled-input identity')
                elif self.numeric:
                    if actual != expected['input_digest']: raise ObservationFailure('numeric tensor input digest changed')
                else:
                    if not isinstance(actual,str) or len(actual)!=64 or any(c not in '0123456789abcdef' for c in actual):
                        raise ObservationFailure('invalid native resource compiled input digest')
                    if self.resource_digest is None: self.resource_digest = actual
                    if actual != self.resource_digest: raise ObservationFailure('resource compiled input changed between calls')
                self.active = event.copy()
                self.check_expiry(now)
            elif phase in ('returned','not_dispatched'):
                if self.active is None or self.active['phase'] != 'started':
                    raise ObservationFailure('native end lacks exactly one matching original start')
                for key in ['kind','ordinal','operation_input_digest','started_monotonic_ns','expires_monotonic_ns']:
                    if event[key] != self.active[key]: raise ObservationFailure('native end renewed/crossed original operation')
                if event['observed_monotonic_ns'] < event['started_monotonic_ns']:
                    raise ObservationFailure('native operation return clock moved backwards')
                if phase == 'not_dispatched':
                    if (event['native_success'] is not None or event['native_call_elapsed_us'] is not None
                        or event['observed_monotonic_ns'] < event['expires_monotonic_ns']):
                        raise ObservationFailure('invalid native pre-dispatch refusal')
                    raise ObservationFailure('native original operation expired before dispatch')
                if (type(event['native_success']) is not bool or type(event['native_call_elapsed_us']) is not int
                    or event['native_call_elapsed_us'] < 0
                    or event['native_call_elapsed_us'] > (event['observed_monotonic_ns']-event['started_monotonic_ns'])//1000):
                    raise ObservationFailure('invalid native returned component observation')
                self.check_expiry(now)
                if event['observed_monotonic_ns'] > event['expires_monotonic_ns']:
                    raise ObservationFailure('actual native operation returned after original expiry')
                self.active = event.copy()
                self.position += 1
                # Deliberately retain the SAME original expiry. Returned bytes
                # can become visible before their fsync completes. Only a next
                # authenticated start or actual process exit ends this fence.
            else:
                raise ObservationFailure('unknown native operation event phase')
            self.events.append(event)
            self.sequence += 1
        self.raw = raw

    def proof(self):
        return {'observed_events':self.events,'completed_return_markers':self.position,
            'frozen_operations':len(self.roster),'active_original_operation':self.active,
            'journal_bytes':len(self.raw),'prospective_starts_are_not_NN_counts':True,
            'returned_marker_alone_clears_expiry':False}

class WindowsWatch:
    def __init__(self, journal, log, identity, roster, deadline_ms, numeric=False):
        if os.name != 'nt': raise ValueError('External operation watcher is qualified for Windows only')
        self.journal, self.log = Path(journal), Path(log)
        self.state = Operations(identity,roster,deadline_ms,numeric)
        self.api = ctypes.WinDLL('kernel32',use_last_error=True)
        declarations = {
            'FindFirstChangeNotificationW':([w.LPCWSTR,w.BOOL,w.DWORD],w.HANDLE),
            'FindNextChangeNotification':([w.HANDLE],w.BOOL),
            'FindCloseChangeNotification':([w.HANDLE],w.BOOL),
            'WaitForMultipleObjects':([w.DWORD,ctypes.POINTER(w.HANDLE),w.BOOL,w.DWORD],w.DWORD),
            'QueryPerformanceCounter':([ctypes.POINTER(ctypes.c_longlong)],w.BOOL),
            'QueryPerformanceFrequency':([ctypes.POINTER(ctypes.c_longlong)],w.BOOL)}
        for name,(arguments,result) in declarations.items():
            getattr(self.api,name).argtypes=arguments;getattr(self.api,name).restype=result
        self.change = self.api.FindFirstChangeNotificationW(str(self.journal.parent),False,0x1|0x8|0x10)
        if self.change in (None,ctypes.c_void_p(-1).value): raise ctypes.WinError(ctypes.get_last_error())

    def now(self):
        ticks,frequency=ctypes.c_longlong(),ctypes.c_longlong()
        if (not self.api.QueryPerformanceCounter(ctypes.byref(ticks))
            or not self.api.QueryPerformanceFrequency(ctypes.byref(frequency))
            or ticks.value<0 or frequency.value<=0): raise ValueError('native Windows QPC observation unavailable')
        return ticks.value*1000000000//frequency.value

    def inspect(self):
        if self.log.exists() and self.log.stat().st_size>64*1024*1024:
            raise ObservationFailure('owned native log exceeded original64MiB bound')
        if self.journal.exists():
            metadata=self.journal.lstat()
            if self.journal.is_symlink() or getattr(metadata,'st_file_attributes',0)&0x400:
                raise ObservationFailure('native operation journal redirected')
            self.state.feed(self.journal.read_bytes(),self.now())

    def run(self, api, info, whole_deadline):
        started=time.monotonic()
        try:
            handles=(w.HANDLE*2)(self.change,info.process)
            while True:
                self.inspect()
                now=self.now();self.state.check_expiry(now)
                remaining=whole_deadline-time.monotonic()
                if remaining<=0: raise ObservationFailure('original whole-process allowance expired')
                wait_seconds=remaining
                if self.state.expiry() is not None:
                    wait_seconds=min(wait_seconds,max(0,(self.state.expiry()-now)/1000000000))
                waited=self.api.WaitForMultipleObjects(2,handles,False,max(1,math.ceil(wait_seconds*1000)))
                if waited==0:
                    if not self.api.FindNextChangeNotification(self.change): raise ctypes.WinError(ctypes.get_last_error())
                elif waited==1:
                    self.inspect();self.state.check_expiry(self.now())
                    code=w.DWORD()
                    if not api.GetExitCodeProcess(info.process,ctypes.byref(code)): raise ctypes.WinError(ctypes.get_last_error())
                    if code.value==0 and (self.state.position!=len(self.state.roster)
                        or self.state.active is None or self.state.active['phase']!='returned'):
                        raise ObservationFailure('zero-exit native child omitted frozen full operation population')
                    return {'worker_pid':int(info.pid),'operation_watch':{**self.state.proof(),
                        'actual_process_exit_observed':True,'exit_code':int(code.value),
                        'whole_deadline_not_renewed':True,'elapsed_seconds':time.monotonic()-started}},[]
                elif waited!=258:
                    raise ctypes.WinError(ctypes.get_last_error())
        except BaseException as error:
            # Raise while the reviewed owner still holds the actual Job/PID
            # handles. Its existing finally performs and observes owned cleanup;
            # never return a false natural/forced-cleanup success here.
            error.operation_watch={**self.state.proof(),'failure_type':type(error).__name__,
                'failure':str(error),'whole_deadline_not_renewed':True,'elapsed_seconds':time.monotonic()-started}
            raise

    def close(self):
        if self.change is not None:
            if not self.api.FindCloseChangeNotification(self.change): raise ctypes.WinError(ctypes.get_last_error())
            self.change=None
