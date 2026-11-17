# SPDX-License-Identifier: Apache-2.0
"""Read-only supplemental plan, or externally guarded absent-output apply."""
import ast
import base64
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

BASE=Path(__file__).resolve().parent
ROOT=BASE.parents[4]
PROBE='artifacts/local/one-airk-1814/neo-audit-token-probe-1794106224/probe.json'
METHOD='single-python-b64decode-literal/1'
PATTERN=r"b64decode\('([A-Za-z0-9+/=]+)'\)"

def sha(raw):
    return hashlib.sha256(raw).hexdigest()

def decode(field):
    literals=re.findall(PATTERN,field)
    if len(literals)!=1 or field.count('b64decode(')!=1: raise ValueError('Expected exactly one base64 literal')
    body=base64.b64decode(literals[0],validate=True)
    body.decode('utf-8',errors='strict')
    return body

def pointer(doc,location):
    for part in location[1:].split('/'):
        part=part.replace('~1','/').replace('~0','~')
        doc=doc[int(part)] if isinstance(doc,list) else doc[part]
    return doc

def plan():
    owners={}
    tracked=subprocess.run(['git','ls-files','-z'],cwd=ROOT,capture_output=True,check=True).stdout.decode().split('\0')
    for name in tracked:
        p=ROOT/name
        if not name or p.suffix not in {'.py','.patch','.diff','.in'} or not p.is_file():continue
        body=p.read_bytes();owners.setdefault(sha(body),(name,''))
        first,sep,rest=body.partition(b'\n')
        if sep and b'SPDX-License-Identifier: Apache-2.0' in first:owners.setdefault(sha(rest),(name,(first+sep).decode()))
    maps={name:{'schema':schema,'files':[]} for name,schema in [('source-provenance.json','airk.embedded-source-migration/1'),('encoded-source-provenance.json','airk.encoded-source-migration/1')]}
    inputs=[]
    for revision in ('r2','r3'):
        name='artifacts/v190/p11b-composed-p11a-'+revision+'/overlap-resolutions.json'
        doc=json.loads((ROOT/name).read_bytes())
        for i,row in enumerate(doc['rejected_hunks']):inputs.append((name,'/rejected_hunks/'+str(i)+'/text','patch',row['text'].encode()))
    raw=(ROOT/PROBE).read_bytes();field=json.loads(raw)['stderr'];body=decode(field);ast.parse(body)
    inputs.append((PROBE,'/stderr','python',body))
    for name,location,language,body in inputs:
        raw=(ROOT/name).read_bytes();digest=sha(body);prefix=''
        if digest in owners:
            dest,prefix=owners[digest];disposition='existing-current-maintained-owner'
        else:
            prefix='# SPDX-License-Identifier: Apache-2.0\n' if language=='python' else ''
            dest=(BASE/(('neo-audit-token-probe--' if language=='python' else 'rejected-hunk--')+digest[:12]+('.py' if language=='python' else '.patch'))).relative_to(ROOT).as_posix()
            disposition='extracted-source'
        maintained=prefix.encode()+body
        row=dict(container=name,container_sha256=sha(raw),container_bytes=len(raw),json_pointer=location,decoded_sha256=digest,decoded_bytes=len(body),destination=dest,prefix=prefix,maintained_sha256=sha(maintained),maintained_bytes=len(maintained),language=language,disposition=disposition)
        if language=='python':
            row.update(encoded_field_sha256=sha(field.encode()),encoded_field_bytes=len(field.encode()),decode_method=METHOD)
            maps['encoded-source-provenance.json']['files'].append(row)
        else:maps['source-provenance.json']['files'].append(row)
    return maps

def apply(maps):
    expected={'source-provenance.json':'airk.embedded-source-migration/1','encoded-source-provenance.json':'airk.encoded-source-migration/1'}
    if set(maps)!=set(expected):raise ValueError('Unexpected map roster')
    pending={}
    for name,doc in maps.items():
        if doc.get('schema')!=expected[name] or (BASE/name).exists():raise ValueError('Unexpected schema or existing provenance')
        for row in doc['files']:
            if row['disposition'] not in {'extracted-source','existing-current-maintained-owner'}:raise ValueError('Unknown disposition')
            raw=(ROOT/row['container']).read_bytes()
            if sha(raw)!=row['container_sha256'] or len(raw)!=row['container_bytes']:raise ValueError('Container drift')
            field=pointer(json.loads(raw),row['json_pointer'])
            if name=='encoded-source-provenance.json':
                if row.get('decode_method')!=METHOD or sha(field.encode())!=row['encoded_field_sha256'] or len(field.encode())!=row['encoded_field_bytes']:raise ValueError('Encoded field drift')
                body=decode(field)
            else:body=field.encode()
            if sha(body)!=row['decoded_sha256'] or len(body)!=row['decoded_bytes']:raise ValueError('Decoded drift')
            payload=row['prefix'].encode()+body;target=(ROOT/row['destination']).resolve()
            if sha(payload)!=row['maintained_sha256'] or len(payload)!=row['maintained_bytes']:raise ValueError('Maintained drift')
            if row['disposition']=='existing-current-maintained-owner':
                if target.read_bytes()!=payload:raise ValueError('Current owner drift')
            else:
                if target.parent!=BASE or target.exists():raise ValueError('Unsafe output')
                if target in pending and pending[target]!=payload:raise ValueError('Conflicting alias')
                pending[target]=payload
    # Every input validated before first mutation. Caller guards all output parents.
    for target,payload in pending.items():target.write_bytes(payload)
    for name,doc in maps.items():(BASE/name).write_text(json.dumps(doc,indent=2)+'\n',encoding='utf-8')

if __name__=='__main__':
    if sys.argv[1:]==['apply']:apply(json.load(sys.stdin))
    elif not sys.argv[1:]:print(json.dumps(plan()))
    else:raise SystemExit('usage: rescue.py [apply]')
