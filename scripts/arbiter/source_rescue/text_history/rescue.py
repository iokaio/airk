# SPDX-License-Identifier: Apache-2.0
"""Plan/apply reviewed authored programs stored in text-suffixed artifacts."""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

BASE=Path(__file__).resolve().parent
ROOT=BASE.parents[3]
REQUEST='artifacts/v190/final-plan/exec-20261116T052942Z/embedded-container-proof-requests-r1.json'
REQUEST_SHA='8add84112e2755ff56b92cb7bccd6a24e3ef2fa82f364467bf2fe77103da1f3d'
MARKERS=('mac-cli-await-','pr56-diff','singleton-sdk','context.rs.txt','context-fp32.rs','materialized-source.txt','development.rs.proposed','native_parent_bag.rs','private_control.rs.fragment','legacy-frame-reproduction.rs','typed-floor-helper.rs','benchmark-config-test.txt','MAC-SOURCE-SYNC.md')

def sha(raw):return hashlib.sha256(raw).hexdigest()

def role(name):
    if 'MAC-SOURCE-SYNC.md' in name:return 'shell-transfer-runbook','.md','', 'Authored source-only macOS transfer runbook with three preserved Python/shell code fences; not an operational authorization.'
    if 'pr56-diff' in name:return 'patch','.patch','', 'Historical PR56 complete source review diff; never auto-apply.'
    if 'apply-source.review.txt' in name:return 'shell-template','.sh.in','# ', 'Reviewed macOS public-source transfer template with sealed payload omitted; not directly runnable.'
    if 'benchmark-config-test' in name:return 'python-test-fragment','.py.in','# ', 'Owned benchmark configuration regression test body for exact scope/control configuration.'
    if 'singleton-sdk' in name:return 'rust-test-fragment','.rs.in','// ', 'Focus singleton allowance and direct-custody SDK regression test.'
    if 'context-verification-headroom' in name:return 'rust-fragment','.rs.in','// ', 'Source-verification metadata capacity calculation and legacy-frame reproduction.'
    if 'qwen3-challengers' in name:return 'rust-template','.rs.in','// ', 'Context candidate admission/native execution and FP32 storage proposal.'
    return 'rust-template','.rs.in','// ', 'MiniLM parent-bag native candidate admission, private control, and development proposal.'

def plan():
    raw=(ROOT/REQUEST).read_bytes()
    if sha(raw)!=REQUEST_SHA:raise ValueError('Reviewed request drift')
    selected=[r for r in json.loads(raw)['files'] if r['path'].endswith(('.txt','.md')) and any(k in r['path'] for k in MARKERS)]
    if len(selected)!=27:raise ValueError('Reviewed source roster drift')
    owners={}
    paths=subprocess.run(['git','ls-files','-z'],cwd=ROOT,capture_output=True,check=True).stdout.decode().split('\0')
    for name in paths:
        p=ROOT/name
        if not name or p.suffix not in {'.py','.rs','.in','.sh','.ps1','.patch','.diff','.txt','.md'} or not p.is_file():continue
        body=p.read_bytes();owners.setdefault(sha(body),(name,''));first,sep,rest=body.partition(b'\n')
        if sep and b'SPDX-License-Identifier: Apache-2.0' in first:owners.setdefault(sha(rest),(name,(first+sep).decode()))
    reachable={r.split(' ',1)[0] for r in subprocess.run(['git','rev-list','--objects','--all'],cwd=ROOT,capture_output=True,text=True,check=True).stdout.splitlines()}
    seen={};rows=[]
    for source in selected:
        name=source['path'];body=(ROOT/name).read_bytes();digest=sha(body)
        if digest!=source['sha256'] or len(body)!=source['bytes']:raise ValueError('Source drift')
        language,extension,marker,description=role(name);prefix=''
        if digest in owners:
            dest,prefix=owners[digest];disposition='existing-current-maintained-owner'
        else:
            if digest not in seen:
                stem=re.sub('[^A-Za-z0-9_.-]','_',Path(name).parent.name+'-'+Path(name).stem)[:110]
                dest=(BASE/(stem+'--'+digest[:12]+extension)).relative_to(ROOT).as_posix()
                if marker and not body.splitlines()[0].startswith((marker+'SPDX-License-Identifier: Apache-2.0').encode()):prefix=marker+'SPDX-License-Identifier: Apache-2.0\n'
                seen[digest]=(dest,prefix)
            dest,prefix=seen[digest];disposition='spdx-prefix-historical-source' if prefix else 'byte-identical-historical-source'
        maintained=prefix.encode()+body;blob=hashlib.sha1(b'blob '+str(len(body)).encode()+b'\0'+body).hexdigest()
        rows.append(dict(source=name,destination=dest,original_sha256=digest,bytes=len(body),maintained_sha256=sha(maintained),maintained_bytes=len(maintained),disposition=disposition,changes=[{'kind':'spdx-prefix','prefix':prefix}] if prefix else [],code_role={'language':language,'description':description},exact_git_blob=blob,git_blob_reachable_at_plan=blob in reachable))
    return {'schema':'airk.source-relocation/1','scope':'27reviewed authored text-suffixed programs/fragments only; source bodies preserved, no historical execution.','request':{'path':REQUEST,'sha256':REQUEST_SHA},'files':rows}

def apply(doc):
    if doc.get('schema')!='airk.source-relocation/1' or (BASE/'source-provenance.json').exists():raise ValueError('Unexpected schema or existing provenance')
    pending={}
    for row in doc['files']:
        body=(ROOT/row['source']).read_bytes()
        if sha(body)!=row['original_sha256'] or len(body)!=row['bytes']:raise ValueError('Source drift')
        changes=row['changes']
        if any(x['kind']!='spdx-prefix' for x in changes) or len(changes)>1:raise ValueError('Unexpected transformation')
        prefix=changes[0]['prefix'] if changes else '';payload=prefix.encode()+body
        if sha(payload)!=row['maintained_sha256'] or len(payload)!=row['maintained_bytes']:raise ValueError('Maintained drift')
        target=(ROOT/row['destination']).resolve()
        if row['disposition']=='existing-current-maintained-owner':
            if target.read_bytes()!=payload:raise ValueError('Current owner drift')
        elif row['disposition'] in {'spdx-prefix-historical-source','byte-identical-historical-source'}:
            if target.parent!=BASE or target.exists():raise ValueError('Unsafe output')
            if target in pending and pending[target]!=payload:raise ValueError('Alias conflict')
            pending[target]=payload
        else:raise ValueError('Unexpected source disposition')
    # All inputs checked before writing. External caller guards/audits outputs.
    for target,body in pending.items():target.write_bytes(body)
    (BASE/'source-provenance.json').write_text(json.dumps(doc,indent=2)+'\n',encoding='utf-8')

if __name__=='__main__':
    if sys.argv[1:]==['apply']:apply(json.load(sys.stdin))
    elif not sys.argv[1:]:print(json.dumps(plan()))
    else:raise SystemExit('usage: rescue.py [apply]')
