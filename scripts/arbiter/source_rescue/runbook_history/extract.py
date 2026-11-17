# SPDX-License-Identifier: Apache-2.0
"""Reconstruct three reviewed authored runbook/diagnostic sources, without execution."""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
BASE=Path(__file__).resolve().parent
ROOT=BASE.parents[3]
INPUTS={
 'artifacts/readiness-1817/final-windows-plan.md':('windows-qualification-plan.md','Pinned editor verification and Windows qualification/package command runbook'),
 'artifacts/advanced-split/local-delivery-preparation-r1/NATIVE-HELP.md':('native-help-runbook.md','Reviewed native Help generation, environment import, emitter and consumer command chain'),
 'artifacts/local/one-airk-1.8.14/noncli-ignore-boundary-investigation.md':('windows-handle-probe.py.in','Only the Windows CreateFileW diagnostic fragment; surrounding investigation/report not copied'),
}

def sha(raw):return hashlib.sha256(raw).hexdigest()
def plan():
    scope=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z/embedded-bounded-coverage-r2.json';raw=scope.read_bytes()
    if sha(raw)!='e6b776a10990b71cd68adccd8bafbe9c2ab8235bcc7ff45d70a580ca8d98f666':raise ValueError('Reviewed source scope drift')
    pins={r['path']:r for r in json.loads(raw)['document_text_scope']['files']};rows=[]
    reachable={x.split(' ',1)[0] for x in subprocess.run(['git','rev-list','--objects','--all'],cwd=ROOT,capture_output=True,text=True,check=True).stdout.splitlines()}
    for name,(filename,description) in INPUTS.items():
        original=(ROOT/name).read_bytes();pin=pins[name]
        if sha(original)!=pin['sha256'] or len(original)!=pin['bytes']:raise ValueError('Original drift')
        body=original;changes=[]
        if filename.endswith('.py.in'):
            blocks=list(re.finditer(rb'(?m)^```python\r?\n(.*?)^```',original,re.S))
            if len(blocks)!=1:raise ValueError('Expected exact one Python diagnostic fence')
            match=blocks[0];body=match.group(1)
            changes.append({'kind':'byte-range-extraction','offset':match.start(1),'bytes':len(body),'extracted_sha256':sha(body)})
            prefix='# SPDX-License-Identifier: Apache-2.0\n';changes.append({'kind':'spdx-prefix','prefix':prefix});maintained=prefix.encode()+body
        else:maintained=body
        blob=hashlib.sha1(b'blob '+str(len(body)).encode()+b'\0'+body).hexdigest()
        rows.append(dict(source=name,destination=(BASE/filename).relative_to(ROOT).as_posix(),original_sha256=sha(original),bytes=len(original),maintained_sha256=sha(maintained),maintained_bytes=len(maintained),changes=changes,disposition='reviewed-runbook-source' if not changes else 'reviewed-diagnostic-byte-range',code_role={'description':description},exact_extracted_git_blob=blob,extracted_git_blob_reachable=blob in reachable))
    return {'schema':'airk.source-relocation/1','scope':'Only three reviewed substantial authored source bodies from finite830document followup. Ordinary reports/prose and publisher examples remain unarchived.','files':rows}

def payload(row):
    raw=(ROOT/row['source']).read_bytes()
    if sha(raw)!=row['original_sha256'] or len(raw)!=row['bytes']:raise ValueError('Original drift')
    result=raw
    for change in row['changes']:
        if change['kind']=='byte-range-extraction':
            start=change['offset'];end=start+change['bytes']
            if type(start)is not int or type(change['bytes'])is not int or start<0 or end>len(result):raise ValueError('Invalid byte range')
            result=result[start:end]
            if sha(result)!=change['extracted_sha256']:raise ValueError('Extracted drift')
        elif change['kind']=='spdx-prefix':result=change['prefix'].encode()+result
        else:raise ValueError('Unknown transformation')
    if sha(result)!=row['maintained_sha256'] or len(result)!=row['maintained_bytes']:raise ValueError('Maintained drift')
    return result

def apply(doc):
    if doc.get('schema')!='airk.source-relocation/1' or len(doc['files'])!=3 or (BASE/'source-provenance.json').exists():raise ValueError('Unexpected roster/schema or existing provenance')
    sources=[row['source'] for row in doc['files']]
    if len(set(sources))!=len(INPUTS) or set(sources)!=set(INPUTS):raise ValueError('Unexpected source roster')
    for row in doc['files']:
        expected=(BASE/INPUTS[row['source']][0]).relative_to(ROOT).as_posix()
        if row['destination']!=expected:raise ValueError('Unexpected destination roster')
    pending={}
    for row in doc['files']:
        target=(ROOT/row['destination']).resolve()
        if target.parent!=BASE or target.exists() or target in pending:raise ValueError('Unsafe output')
        pending[target]=payload(row)
    for target,body in pending.items():
        with target.open('xb') as stream:stream.write(body)
    with (BASE/'source-provenance.json').open('x',encoding='utf-8') as stream:stream.write(json.dumps(doc,indent=2)+'\n')
if __name__=='__main__':
    if sys.argv[1:]==['apply']:apply(json.load(sys.stdin))
    elif not sys.argv[1:]:print(json.dumps(plan()))
    else:raise SystemExit('usage: extract.py [apply]')
