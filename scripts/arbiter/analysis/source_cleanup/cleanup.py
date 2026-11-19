# SPDX-License-Identifier: Apache-2.0
"""Read-only exact committed-source cleanup planning and apply preflight."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT=Path(__file__).resolve().parents[4]
SID='S-1-5-21-3678370870-3858642417-826968105-1002'
LIMIT=16<<20
EXTENSIONS={'.py','.ps1','.sh','.cmd','.bat','.js','.cjs','.mjs','.ts','.tsx','.rs','.cpp','.cc','.c','.h','.hpp','.cu','.cuh','.patch','.css'}
PREFIXES=(b'# SPDX-License-Identifier: Apache-2.0\n',b'# SPDX-License-Identifier: Apache-2.0\r\n',b'// SPDX-License-Identifier: Apache-2.0\n',b'// SPDX-License-Identifier: Apache-2.0\r\n',b'REM SPDX-License-Identifier: Apache-2.0\n',b'REM SPDX-License-Identifier: Apache-2.0\r\n')

def digest(raw):return hashlib.sha256(raw).hexdigest()
def relative(value):
    if not isinstance(value,str) or not value or '\\' in value or ':' in value or value.startswith('/') or any(x in ('','.','..') or x.endswith((' ','.')) for x in value.split('/')):raise ValueError('canonical relative path required')
    return value

def source_path(value):
    relative(value)
    if not value.startswith('artifacts/') or Path(value).suffix.lower() not in EXTENSIONS:raise ValueError('only explicit artifact source files may be selected')
    return value

def parse(raw):
    def unique(pairs):
        result={}
        for key,value in pairs:
            if key in result:raise ValueError('duplicate JSON key')
            result[key]=value
        return result
    return json.loads(raw,object_pairs_hook=unique)

def read(relative_path):
    path=ROOT/relative(relative_path)
    for item in [path,*path.parents]:
        info=item.lstat()
        if item.is_symlink() or getattr(info,'st_file_attributes',0)&0x400:raise ValueError('aliased input refused')
        if item==ROOT:break
    if not path.is_file() or path.stat().st_size>LIMIT:raise ValueError('bounded ordinary input required')
    raw=path.read_bytes()
    if len(raw)>LIMIT:raise ValueError('input exceeds bound')
    return raw

def pinned(pin):
    raw=read(pin['path'])
    if digest(raw)!=pin['sha256'] or len(raw)!=pin['bytes']:raise ValueError('pinned input changed: '+pin['path'])
    return raw

def git(*args):return subprocess.check_output(['git',*args],cwd=ROOT,stderr=subprocess.PIPE,timeout=30)
def committed(commit,path):
    relative(path)
    oid=git('rev-parse',commit+':'+path).decode().strip()
    if int(git('cat-file','-s',oid))>LIMIT:raise ValueError('committed input exceeds bound')
    raw=git('cat-file','blob',oid)
    if read(path)!=raw:raise ValueError('working file differs from raw committed replacement: '+path)
    return raw,oid

def inverse(raw,original_sha,original_bytes):
    possibilities=[('identity',b'',raw)]+[('spdx-prefix',p,raw[len(p):]) for p in PREFIXES if raw.startswith(p)]
    matches=[(kind,p,body) for kind,p,body in possibilities if digest(body)==original_sha and len(body)==original_bytes]
    if len(matches)!=1:raise ValueError('no unique exact identity/SPDX inverse')
    kind,p,body=matches[0]
    return {'kind':kind,'prefix_utf8':p.decode()},body

def valid_parents(value):
    return isinstance(value,list) and bool(value) and all(isinstance(p,dict) and p.get('owner_sid')==SID and isinstance(p.get('path'),str) and bool(p['path']) and isinstance(p.get('sddl'),str) and bool(p['sddl']) for p in value)

def candidate(row,mapdoc,replacement,observation,review):
    source=source_path(row['source'])
    if review.get('source')!=source or review.get('role')!='retired-authored-source':raise ValueError('explicit authored-source role review required')
    if review.get('status')!='clear' or review.get('active_consumers')!=[]:raise ValueError('consumer transition is incomplete')
    if not review.get('consumer_searches') or not review.get('verification_transition'):raise ValueError('consumer search and verification transition evidence required')
    matches=[x for x in mapdoc['files'] if x.get('source')==source and x.get('destination')==row['destination']]
    if len(matches)!=1:raise ValueError('unique allowlisted source-map row required')
    mapped=matches[0]
    expected=mapped.get('original_sha256',mapped.get('source_sha256'))
    if expected!=row['source_sha256']:raise ValueError('source-map original hash differs')
    if digest(replacement)!=mapped.get('maintained_sha256'):raise ValueError('committed replacement differs from map')
    for key in ('original_bytes','source_bytes','bytes'):
        if key in mapped and mapped[key]!=row['source_bytes']:raise ValueError('map-declared original length differs')
    for key in ('maintained_bytes','destination_bytes'):
        if key in mapped and mapped[key]!=len(replacement):raise ValueError('map-declared maintained length differs')
    inv,original=inverse(replacement,row['source_sha256'],row['source_bytes'])
    if observation.get('owner_sid')!=SID or observation.get('effective_sid')!=SID or observation.get('default_owner_sid')!=SID:raise ValueError('human ownership/identity mismatch')
    if observation.get('source')!=source or observation.get('sha256')!=digest(original) or observation.get('bytes')!=len(original):raise ValueError('current original differs from committed inverse')
    if observation.get('policy_passed') is not True or not observation.get('file_sddl') or not valid_parents(observation.get('parents')):raise ValueError('parent access-policy observation required')
    return {'source':source,'source_sha256':digest(original),'source_bytes':len(original),'destination':row['destination'],'inverse':inv,'observation':observation,'consumer_review':review}

def verify_custody(expected,original,proof=None):
    """Pure proof check; absent source is never silently accepted."""
    source_path(expected['source'])
    def same(value):
        return all(value.get(k)==expected[k] for k in ('source','source_sha256','source_bytes'))
    if original is not None:
        if digest(original)!=expected['source_sha256'] or len(original)!=expected['source_bytes']:raise ValueError('original custody bytes changed')
        return 'present-original'
    if proof is None:raise ValueError('missing original requires bound successful removal proof')
    result_raw=proof['result_raw'];result_pin=proof['result_pin']
    if digest(result_raw)!=result_pin['sha256'] or len(result_raw)!=result_pin['bytes']:raise ValueError('removal receipt pin differs')
    result=parse(result_raw)
    if result.get('schema')!='airk.source-cleanup-result/1' or result.get('removed') is not True or result.get('error') is not None or not same(result):raise ValueError('successful exact removal result required')
    pre_raw=proof['pre_removal_raw'];pre_pin=result['pre_removal']
    if digest(pre_raw)!=pre_pin['sha256'] or len(pre_raw)!=pre_pin['bytes']:raise ValueError('pre-removal receipt pin differs')
    pre=parse(pre_raw)
    if pre.get('schema')!='airk.source-cleanup-pre-removal/1' or pre.get('human_sid')!=SID:raise ValueError('human pre-removal proof required')
    rows=[r for r in pre['plan']['files'] if r.get('source')==expected['source']]
    if len(rows)!=1 or not same(rows[0]) or rows[0].get('eligibility')!='ready' or rows[0]['source_map']!=expected['source_map']:raise ValueError('pre-removal source/map binding differs')
    row=rows[0];raw=proof['replacement_raw'];replacement=row['replacement']
    if digest(raw)!=replacement['sha256'] or len(raw)!=replacement['bytes']:raise ValueError('committed replacement proof differs')
    oid=replacement['git_blob_oid'];algorithm='sha1' if len(oid)==40 else 'sha256' if len(oid)==64 else None
    if algorithm is None or hashlib.new(algorithm,b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()!=oid:raise ValueError('raw committed blob identity differs')
    inv,_=inverse(raw,expected['source_sha256'],expected['source_bytes'])
    if inv!=row['inverse']:raise ValueError('pre-removal inverse differs')
    observation=row['observation']
    if observation.get('source')!=expected['source'] or observation.get('sha256')!=expected['source_sha256'] or observation.get('bytes')!=expected['source_bytes'] or any(observation.get(k)!=SID for k in ('owner_sid','effective_sid','default_owner_sid')) or observation.get('policy_passed') is not True or not observation.get('file_sddl') or not valid_parents(observation.get('parents')):raise ValueError('pre-removal observed custody differs')
    return 'proved-removed-original'

def audit_original(expected,removal_pin=None):
    """Private custody audit, separate from permanent committed inverse tests."""
    pinned(expected['source_map'])
    try:original=read(source_path(expected['source']))
    except FileNotFoundError:original=None
    if original is not None:return verify_custody(expected,original)
    if removal_pin is None:return verify_custody(expected,None)
    result_raw=pinned(removal_pin);result=parse(result_raw);pre_raw=pinned(result['pre_removal']);pre=parse(pre_raw)
    rows=[r for r in pre['plan']['files'] if r.get('source')==expected['source']]
    if len(rows)!=1:raise ValueError('unique removal proof source required')
    replacement=rows[0]['replacement'];commit=replacement['commit']
    if not re.fullmatch('[0-9a-f]{40}',commit):raise ValueError('exact rescue commit required')
    git('merge-base','--is-ancestor',commit,'HEAD')
    oid=git('rev-parse',commit+':'+relative(replacement['path'])).decode().strip()
    if oid!=replacement['git_blob_oid'] or int(git('cat-file','-s',oid))>LIMIT:raise ValueError('committed cleanup replacement differs')
    raw=git('cat-file','blob',oid)
    return verify_custody(expected,None,dict(result_raw=result_raw,result_pin=removal_pin,pre_removal_raw=pre_raw,replacement_raw=raw))

def observe_many(sources):
    completed=subprocess.run(['pwsh','-NoProfile','-File',str(Path(__file__).with_name('observe.ps1')),'-BatchStdin'],input=json.dumps(sources).encode(),cwd=ROOT,stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True)
    rows=parse(completed.stdout)
    if not isinstance(rows,list) or len(rows)!=len(sources) or {r['source'] for r in rows}!=set(sources):raise ValueError('batch observation roster differs')
    return {r['source']:r for r in rows}

def revalidate_snapshot(commit,pins,head_reader,pin_reader):
    """Read-only end barrier; injected readers make drift refusals cheap to test."""
    if head_reader()!=commit:raise ValueError('HEAD changed during planning')
    for pin in pins:
        raw=pin_reader(pin)
        if digest(raw)!=pin['sha256'] or len(raw)!=pin['bytes']:raise ValueError('input changed during planning: '+pin['path'])
    if head_reader()!=commit:raise ValueError('HEAD changed during final input verification')

def build(request_pin):
    request=parse(pinned(request_pin));commit=request['rescue_commit']
    if request.get('schema')!='airk.source-cleanup-request/1' or not re.fullmatch('[0-9a-f]{40}',commit):raise ValueError('exact cleanup request and commit required')
    if git('rev-parse','HEAD').decode().strip()!=commit:raise ValueError('checkout HEAD differs from reviewed rescue commit')
    tools=[]
    for path in ['scripts/dev/assert-human-execution.ps1',*[f'scripts/arbiter/analysis/source_cleanup/{n}' for n in ('cleanup.py','observe.ps1','apply.ps1','CONTRACT.md')]]:
        raw,oid=committed(commit,path);tools.append(dict(path=path,sha256=digest(raw),bytes=len(raw),git_blob_oid=oid))
    maps={}
    for pin in request['source_maps']:
        path=relative(pin['path'])
        if not path.startswith(('scripts/','native/')) or not path.endswith(('/source-map.json','/source-provenance.json')):raise ValueError('explicit maintained source-map allowlist required')
        raw=pinned(pin);committed_raw,_=committed(commit,path)
        if raw!=committed_raw:raise ValueError('map is not exact committed input')
        if path in maps:raise ValueError('duplicate source-map allowlist')
        maps[path]=(parse(raw),pin)
    reviews=parse(pinned(request['consumer_review']))
    if reviews.get('schema')!='airk.source-cleanup-consumer-review/1':raise ValueError('consumer review schema required')
    results=[];seen=set();replacement_cache={};final_pins=[request_pin,request['consumer_review'],*request['source_maps'],*tools]
    sources=[source_path(row['source']) for row in request['files']]
    if len({source.casefold() for source in sources})!=len(sources):raise ValueError('duplicate/case-colliding source')
    observations=observe_many(sources) if sources else {}
    for row in request['files']:
        key=source_path(row['source']).casefold()
        if key in seen:raise ValueError('duplicate/case-colliding source')
        seen.add(key)
        try:
            mapdoc,mappin=maps[row['source_map']]
            destination=relative(row['destination'])
            if destination.startswith(('artifacts/','temp/','.airk/')):raise ValueError('replacement must be maintained source')
            if destination not in replacement_cache:replacement_cache[destination]=committed(commit,destination)
            replacement,oid=replacement_cache[destination]
            review_rows=[x for x in reviews['files'] if x.get('source')==row['source']]
            if len(review_rows)!=1:raise ValueError('unique consumer review row required')
            for evidence in review_rows[0].get('evidence',[]):
                pinned(evidence);final_pins.append(evidence)
            if not review_rows[0].get('evidence'):raise ValueError('pinned consumer evidence required')
            observation=observations[row['source']]
            if observation.get('error'):raise ValueError('source observation refused: '+observation['error'])
            result=candidate(row,mapdoc,replacement,observation,review_rows[0])
            final_pins.append(dict(path=destination,sha256=digest(replacement),bytes=len(replacement)))
            result.update(eligibility='ready',source_map=mappin,replacement={'path':destination,'commit':commit,'git_blob_oid':oid,'sha256':digest(replacement),'bytes':len(replacement)})
        except (ValueError,KeyError,OSError,subprocess.SubprocessError) as error:
            result={'source':row['source'],'eligibility':'deferred','reason':str(error)}
        results.append(result)
    if not results:raise ValueError('empty cleanup selection')
    revalidate_snapshot(commit,final_pins,lambda:git('rev-parse','HEAD').decode().strip(),lambda pin:read(pin['path']))
    return {'schema':'airk.source-cleanup-plan/1','mode':'dry-run','rescue_commit':commit,'human_sid':SID,'request':request_pin,'source_maps':request['source_maps'],'consumer_review':request['consumer_review'],'tools':tools,'files':results,'ready_count':sum(x['eligibility']=='ready' for x in results),'deferred_count':sum(x['eligibility']!='ready' for x in results),'no_files_changed':True}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('command',choices=['plan','verify']);p.add_argument('--input',required=True);p.add_argument('--sha256',required=True);p.add_argument('--bytes',type=int,required=True);a=p.parse_args()
    pin={'path':a.input,'sha256':a.sha256,'bytes':a.bytes}
    if a.command=='plan':result=build(pin)
    else:
        manifest=parse(pinned(pin))
        if manifest.get('schema')!='airk.source-cleanup-plan/1':raise ValueError('cleanup plan required')
        result=build(manifest['request'])
        if result!=manifest:raise ValueError('manifest no longer equals current exact dry-run proof')
        if result['deferred_count'] or not result['ready_count']:raise ValueError('apply requires a nonempty all-ready manifest')
    print(json.dumps(result,indent=2))
if __name__=='__main__':main()
