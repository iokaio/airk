# SPDX-License-Identifier: Apache-2.0
"""Private phase boundaries. No native DTOs, retries, sleeps or model calls."""
import hashlib,json,os,stat,time
from pathlib import Path
SIZES=[8]*12+[4]
def need(value,message):
    if not value:raise ValueError(message)
def pin(path):
    path=Path(path).resolve(strict=True)
    with path.open('rb') as stream:digest=hashlib.file_digest(stream,'sha256').hexdigest()
    return {'path':str(path),'sha256':digest,'bytes':path.stat().st_size}
def verified_json(reference):
    actual=pin(reference['path'])
    need(actual['sha256']==reference['sha256'] and ('bytes' not in reference or actual['bytes']==reference['bytes']),'pinned input changed')
    return json.loads(Path(actual['path']).read_bytes())
def merge_seals(*sources):
    result={}
    for source in sources:
        for path,digest in source.items():
            need(path not in result or result[path]==digest,'conflicting input seal identities')
            result[path]=digest
    return result
def partitions(ids):
    need(len(ids)==100 and len(set(ids))==100,'exact100 unique IDs required')
    result=[];start=0
    for index,size in enumerate(SIZES):
        result.append({'chunk_index':index,'question_start':start,'question_end':start+size,'questions':ids[start:start+size]});start+=size
    return result
def require_absent(path):
    need(not os.path.lexists(path),'recorded absent build input exists or aliases')
def check_inputs(release):
    for path,digest in release['input_pins'].items():need(pin(path)['sha256']==digest,'capture source/artifact input changed')
    for path,value in release.get('absent_build_inputs',{}).items():
        need(value=='missing','unknown absence sentinel');require_absent(path)
def global_deadline(reference):
    bound=verified_json(reference)
    start=bound['original_start_unix_ms'];end=bound['absolute_deadline_unix_ms']
    need(type(start) is int and type(end) is int and start>0 and 0<end-start<=10_800_000,'original fixture10800s bound required')
    evidence=verified_json(bound['original_start_evidence'])
    observed=evidence[bound['original_start_field']]
    need(int(observed*1000)==start,'original retained fixture start evidence differs')
    return end
class PhaseBoundary:
    def __init__(self,packet,started_monotonic,clock=time.monotonic,wall=time.time):
        self.packet=packet;self.started=started_monotonic;self.clock=clock;self.wall=wall
        self.global_deadline=global_deadline(packet['fixture_bound'])
        self.original_global_monotonic=self.clock()+max(0,self.global_deadline/1000-self.wall())
        inherited=os.environ.get('AIRK_CAL_ORIGINAL_GLOBAL_MONOTONIC_DEADLINE_NS')
        if inherited is not None:self.original_global_monotonic=min(self.original_global_monotonic,int(inherited)/1e9)
        need(packet['owner_seconds']==1800,'original phase1800s ceiling required')
    def remaining(self):return min(1800-(self.clock()-self.started),self.global_deadline/1000-self.wall(),self.original_global_monotonic-self.clock())
    def verify_global(self):need(self.remaining()>0,'original phase/absolute fixture deadline expired')
    def require_budget(self,seconds):need(self.remaining()>seconds,'original remaining phase/global budget cannot admit call plus closure')
    def stop_requested(self):
        path=Path(self.packet['stop_file']);ready=Path(verified_json(self.packet['release'])['paths']['fixture_ready'])
        need(path.parent.samefile(ready.parent),'stop marker must remain in genuine owner-protected fixture metadata root')
        if not os.path.lexists(path):return False
        info=path.lstat()
        need(stat.S_ISREG(info.st_mode) and info.st_nlink==1 and not (getattr(info,'st_file_attributes',0)&0x400),'stop marker alias/refusal')
        return True
