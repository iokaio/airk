# SPDX-License-Identifier: Apache-2.0
"""Private original-bound capacity policy and actual Windows metadata readers."""
import ctypes,json,math,os,stat,time
from ctypes import wintypes as w
from pathlib import Path

GIB=1<<30
GRAPH=64<<20
WEIGHTS=8<<30
RESERVATION=16<<30
CONTAINMENT=32<<30
FREE=9<<30

def need(value,message):
    if not value:raise ValueError(message)

def initial_free(free):
    need(type(free) is int and free>=FREE,'Fresh diagnostic storage has less than original9GiB free')

class Capacity:
    def __init__(self,clock=time.monotonic):
        self.clock=clock;self.last_sample_time=None;self.latest=None
        self.files={};self.samples=[];self.observations=0
        self.peak_rss=self.peak_commit=0;self.last=None
    def observe(self,files,memory):
        self.observations+=1
        need(set(self.files).issubset(files),'Observed native output disappeared')
        for name,row in files.items():
            cap=GRAPH if name in ('graph','profile','body','stdout') else WEIGHTS
            need(type(row['bytes']) is int and row['bytes']>=0,'Invalid native output length')
            need(row['regular'] is True and row['reparse'] is False and row['links']==1,'Redirected/linked native output')
            previous=self.files.get(name)
            if previous:
                need(row['identity']==previous['identity'],'Native output identity replaced')
                if name in ('graph','weights'):need(row['bytes']>=previous['bytes'],'Native optimized output truncated')
            self.files[name]=dict(row)
        for name in ['peak_rss','peak_job_commit']:
            need(type(memory.get(name)) is int and memory[name]>0,'Actual peak memory unavailable: '+name)
        self.peak_rss=max(self.peak_rss,memory['peak_rss'])
        self.peak_commit=max(self.peak_commit,memory['peak_job_commit'])
        sample={'files':{k:v['bytes'] for k,v in files.items()},'memory':dict(memory)}
        self.latest=sample;now=self.clock()
        if sample!=self.last and (self.last_sample_time is None or now-self.last_sample_time>=1.0):
            need(len(self.samples)<4096,'Original bounded capacity observation population exceeded')
            self.samples.append(sample);self.last=sample;self.last_sample_time=now
        for name,row in files.items():
            cap=GRAPH if name in ('graph','profile','body','stdout') else WEIGHTS
            need(row['bytes']<=cap,'Native output exceeded original bound: '+name)
        need(self.peak_rss<=RESERVATION,'Actual native peak RSS exceeds16GiB reservation')
        need(self.peak_commit<=RESERVATION,'Actual whole-Job peak commit exceeds16GiB reservation')
    def proof(self):
        return {'observations':self.observations,'peak_process_rss_bytes':self.peak_rss,
            'peak_job_committed_bytes':self.peak_commit,'last_files':self.files,'changed_samples':self.samples,'final_observation':self.latest,
            'sampling_interval_ms':100,'detailed_snapshot_interval_ms':1000,'peak_memory_source':'OS persistent peak counters; not isolated RSS sum',
            'file_bound_scope':'live100ms/event observations plus native/final append-only optimized file checks'}

class WindowsRead:
    def __init__(self):
        need(os.name=='nt','Qualified Windows-only capacity observer')
        self.api=ctypes.WinDLL('kernel32',use_last_error=True)
        self.psapi=ctypes.WinDLL('psapi',use_last_error=True)
        class Time(ctypes.Structure):_fields_=[('low',w.DWORD),('high',w.DWORD)]
        class FileInfo(ctypes.Structure):
            _fields_=[('attributes',w.DWORD),('created',Time),('accessed',Time),('written',Time),
                ('volume',w.DWORD),('size_hi',w.DWORD),('size_lo',w.DWORD),('links',w.DWORD),
                ('index_hi',w.DWORD),('index_lo',w.DWORD)]
        class Memory(ctypes.Structure):
            _fields_=[('cb',w.DWORD),('faults',w.DWORD)]+[(n,ctypes.c_size_t) for n in
                ['peak_working','working','quota_peak_paged','quota_paged','quota_peak_nonpaged','quota_nonpaged','pagefile','peak_pagefile','private']]
        class Limits(ctypes.Structure):
            _fields_=[('per_process',ctypes.c_longlong),('per_job',ctypes.c_longlong),('flags',w.DWORD),
                ('minimum',ctypes.c_size_t),('maximum',ctypes.c_size_t),('active',w.DWORD),
                ('affinity',ctypes.c_size_t),('priority',w.DWORD),('scheduling',w.DWORD)]
        class IO(ctypes.Structure):_fields_=[(n,ctypes.c_ulonglong) for n in ['read_ops','write_ops','other_ops','read_bytes','write_bytes','other_bytes']]
        class Job(ctypes.Structure):
            _fields_=[('basic',Limits),('io',IO),('process_memory',ctypes.c_size_t),('job_memory',ctypes.c_size_t),
                ('peak_process',ctypes.c_size_t),('peak_job',ctypes.c_size_t)]
        self.FileInfo,self.Memory,self.Job=FileInfo,Memory,Job
        self.api.CreateFileW.argtypes=[w.LPCWSTR,w.DWORD,w.DWORD,ctypes.c_void_p,w.DWORD,w.DWORD,w.HANDLE];self.api.CreateFileW.restype=w.HANDLE
        self.api.GetFileInformationByHandle.argtypes=[w.HANDLE,ctypes.POINTER(FileInfo)];self.api.GetFileInformationByHandle.restype=w.BOOL
        self.api.CloseHandle.argtypes=[w.HANDLE];self.api.CloseHandle.restype=w.BOOL
        self.api.QueryInformationJobObject.argtypes=[w.HANDLE,ctypes.c_int,ctypes.c_void_p,w.DWORD,ctypes.c_void_p];self.api.QueryInformationJobObject.restype=w.BOOL
        self.psapi.GetProcessMemoryInfo.argtypes=[w.HANDLE,ctypes.POINTER(Memory),w.DWORD];self.psapi.GetProcessMemoryInfo.restype=w.BOOL
    def file(self,path):
        path=Path(path)
        try:metadata=path.lstat()
        except FileNotFoundError:return None
        need(not path.is_symlink() and not getattr(metadata,'st_file_attributes',0)&0x400,'Output path redirected before metadata observation')
        handle=self.api.CreateFileW(str(path),0x80,7,None,3,0x200000,None)
        if handle in (None,ctypes.c_void_p(-1).value):raise ctypes.WinError(ctypes.get_last_error())
        try:
            value=self.FileInfo()
            if not self.api.GetFileInformationByHandle(handle,ctypes.byref(value)):raise ctypes.WinError(ctypes.get_last_error())
            return {'bytes':(int(value.size_hi)<<32)|int(value.size_lo),'identity':[int(value.volume),int(value.index_hi),int(value.index_lo)],
                'links':int(value.links),'regular':not bool(value.attributes&0x10),'reparse':bool(value.attributes&0x400)}
        finally:
            if not self.api.CloseHandle(handle):raise ctypes.WinError(ctypes.get_last_error())
    def memory(self,process,job):
        value=self.Memory();value.cb=ctypes.sizeof(value)
        if not self.psapi.GetProcessMemoryInfo(process,ctypes.byref(value),ctypes.sizeof(value)):raise ctypes.WinError(ctypes.get_last_error())
        limits=self.Job()
        if not self.api.QueryInformationJobObject(job,9,ctypes.byref(limits),ctypes.sizeof(limits),None):raise ctypes.WinError(ctypes.get_last_error())
        need(limits.process_memory==limits.job_memory==CONTAINMENT and limits.basic.flags&0x300==0x300,
            'Actual original32GiB process/Job containment differs')
        return {'peak_rss':int(value.peak_working),'current_rss':int(value.working),
            'peak_process_commit':int(value.peak_pagefile),'current_private_commit':int(value.private),
            'peak_job_commit':int(limits.peak_job)}
