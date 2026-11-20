# SPDX-License-Identifier: Apache-2.0
"""Measure Windows group-zero CPU utilization and select distinct physical cores."""

import ctypes
import json
import os
import struct
import time
from ctypes import wintypes as w


def measure():
    if os.name != "nt":
        raise OSError("Windows CPU set information required")
    k = ctypes.WinDLL("kernel32",use_last_error=True)
    query = k.GetSystemCpuSetInformation
    query.argtypes = [w.LPVOID,w.ULONG,ctypes.POINTER(w.ULONG),w.HANDLE,w.ULONG]
    need = w.ULONG()
    query(None,0,ctypes.byref(need),None,0)
    buf=ctypes.create_string_buffer(need.value)
    if not query(buf,need,ctypes.byref(need),None,0):
        raise ctypes.WinError(ctypes.get_last_error())
    raw=buf.raw
    pos=0
    cores={}
    while pos < need.value:
        size,kind=struct.unpack_from("<II",raw,pos)
        if size < 20 or pos+size > need.value:
            raise ValueError("invalid CPU set row")
        if kind==0:
            group,logical,core=struct.unpack_from("<HBB",raw,pos+12)
            if group!=0:
                raise ValueError("only a single Windows processor group is supported")
            cores.setdefault(core,[]).append(logical)
        pos+=size
    count=sum(map(len,cores.values()))
    if sorted(n for siblings in cores.values() for n in siblings)!=list(range(count)) or count>64:
        raise ValueError("incomplete group-zero topology")
    ntdll=ctypes.WinDLL("ntdll")
    nt=ntdll.NtQuerySystemInformation
    nt.argtypes=[w.ULONG,w.LPVOID,w.ULONG,ctypes.POINTER(w.ULONG)]
    def times():
        out=ctypes.create_string_buffer(count*48)
        length=w.ULONG()
        if nt(8,out,len(out),ctypes.byref(length))!=0 or length.value!=len(out):
            raise OSError("processor performance information unavailable")
        return [struct.unpack_from("<qqq",out,i*48) for i in range(count)]
    # Measure a utilization interval. Preserve a record of all candidate cores.
    first=times()
    start=time.monotonic()
    time.sleep(1.25)
    second=times()
    excluded={2,48,50,56,57,58,60,62}
    ranks=[]
    for core,siblings in cores.items():
        if any(i in excluded for i in siblings):
            continue
        idle=total=0
        for i in siblings:
            a,b,c=(second[i][j]-first[i][j] for j in range(3))
            idle+=a
            total+=b+c
        if total<=0 or not 0<=idle<=total:
            raise ValueError("invalid measured CPU interval")
        ranks.append({"core":core,"siblings":siblings,"busy_fraction":(total-idle)/total})
    ranks.sort(key=lambda x:(x["busy_fraction"],x["core"]))
    if len(ranks)<2:
        raise ValueError("two distinct quiet cores required")
    chosen=[r["siblings"][0] for r in ranks[:2]]
    return {"logical_cpus":chosen,"mask":hex(sum(1<<v for v in chosen)), "selected":ranks[:2],
            "sample_seconds":time.monotonic()-start,"excluded_recent_workers":sorted(excluded),"candidates":ranks}


if __name__=="__main__":
    print(json.dumps(measure(),allow_nan=False))
