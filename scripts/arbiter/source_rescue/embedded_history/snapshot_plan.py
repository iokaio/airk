# SPDX-License-Identifier: Apache-2.0
"""Plan explicitly reviewed historical source snapshots; never execute them."""
import hashlib
import json
import re
import subprocess
from pathlib import Path
from extract import ROOT, BASE, sha, current_owners

E = 'artifacts/v190/final-plan/exec-20261116T052942Z/'
SELECTION = {
 E+'tools-parity-cleanup-20261116T090112Z/removed-helper-blocks.json': ('rust-fragment', None),
 E+'neo-current-diagnostics-monitor-r1/native-name-open-diagnostic-proposal-r1/preimages.json': ('rust-template', None),
 'artifacts/advanced-split/r04-review-boundary-before.json': ('typescript-template', ['files']),
 'artifacts/advanced-split/r19-baseline-r2.json': ('typescript-template', ['original','bodies','helpers','modules','replacements']),
 'artifacts/advanced-split/r19-baseline.json': ('typescript-template', ['original','bodies','helpers','modules','replacements']),
 'artifacts/advanced-split/r19-before.json': ('typescript-template', ['original','bodies','helpers','modules','replacements']),
 'artifacts/advanced-split/r19-review-boundary-before.json': ('typescript-template', ['files','modules']),
 'artifacts/advanced-split/receipts/workbench-projectless-decoder-investigation/proposed-sources.json': ('typescript-template', None),
}

def plan():
    owners=current_owners(); seen={}; records=[]
    reachable=set(line.split(' ',1)[0] for line in subprocess.run(['git','rev-list','--objects','--all'],cwd=ROOT,capture_output=True,text=True,check=True).stdout.splitlines())
    inventory=ROOT/E/'source-embedded-all-artifacts-r1.json'
    pins={str(Path(x['path']).relative_to(ROOT)).replace('\\','/'):x['sha256'] for x in json.loads(inventory.read_bytes())['files'] if x.get('sha256')}
    for name,(language,groups) in SELECTION.items():
        raw=(ROOT/name).read_bytes()
        if sha(raw)!=pins[name]: raise ValueError('Container drift: '+name)
        doc=json.loads(raw); entries=[]
        if groups:
            for group in groups:
                for key,value in doc[group].items():
                    loc='/'+group+'/'+key.replace('~','~0').replace('/','~1')
                    if group=='replacements':
                        entries.extend((loc+'/'+str(i)+'/text',item['text']) for i,item in enumerate(value) if item['text'])
                    elif key!='activityNames':
                        entries.append((loc,value))
        else:
            entries=[('/'+key.replace('~','~0').replace('/','~1'),value) for key,value in doc.items() if not key.endswith('.md')]
        for pointer,body in entries:
            if not isinstance(body,str): raise ValueError('Non-string reviewed field')
            payload=body.encode(); digest=sha(payload); prefix=''
            if digest in owners:
                dest,prefix=owners[digest]; maintained=(ROOT/dest).read_bytes(); disposition='existing-current-maintained-owner'
            else:
                if digest not in seen:
                    stem=re.sub('[^A-Za-z0-9_.-]','_',Path(name).stem+'-'+pointer.rsplit('/',1)[-1])[:100]
                    ext='.rs.in' if language.startswith('rust') else '.ts.in'
                    dest=(BASE/'snapshots'/(stem+'--'+digest[:12]+ext)).relative_to(ROOT).as_posix()
                    if not payload.splitlines() or b'SPDX-License-Identifier: Apache-2.0' not in payload.splitlines()[0]:
                        prefix='// SPDX-License-Identifier: Apache-2.0'+ ('\r\n' if b'\r\n' in payload else '\n')
                    seen[digest]=(dest,prefix)
                dest,prefix=seen[digest];maintained=prefix.encode()+payload;disposition='extracted-source'
            blob=hashlib.sha1(b'blob '+str(len(payload)).encode()+b'\0'+payload).hexdigest()
            records.append(dict(container=name,container_sha256=sha(raw),container_bytes=len(raw),json_pointer=pointer,decoded_sha256=digest,decoded_bytes=len(payload),destination=dest,maintained_sha256=sha(maintained),maintained_bytes=len(maintained),prefix=prefix,language=language,disposition=disposition,exact_git_blob=blob,git_blob_reachable_at_plan=blob in reachable))
    return dict(schema='airk.embedded-source-migration/1',inventory=dict(path=inventory.relative_to(ROOT).as_posix(),sha256=sha(inventory.read_bytes())),files=records,exclusions=[],inspection_gaps={'complete_source_coverage':False,'scope':'Eight explicitly reviewed AIRK source snapshot containers only; docs/split/contracts.md prose excluded from proposed-sources selection; other indicators remain separately classified.'})

def apply(value):
    # External same-human shell must preflight all parents and audit outputs.
    from extract import pointer
    if value.get('schema')!='airk.embedded-source-migration/1': raise ValueError('Unexpected source map schema')
    if (BASE/'snapshots/source-provenance.json').exists(): raise ValueError('Never overwrite source provenance')
    if any(row.get('disposition') not in {'extracted-source','existing-current-maintained-owner'} for row in value['files']): raise ValueError('Unknown source disposition')
    seen=set()
    for row in value['files']:
        raw=(ROOT/row['container']).read_bytes()
        if sha(raw)!=row['container_sha256']: raise ValueError('Container drift')
        body=pointer(json.loads(raw),'$'+row['json_pointer']).encode()
        if sha(body)!=row['decoded_sha256']: raise ValueError('Decoded drift')
        target=(ROOT/row['destination']).resolve()
        payload=row['prefix'].encode()+body
        if len(raw)!=row['container_bytes'] or len(body)!=row['decoded_bytes']: raise ValueError('Source length drift')
        if sha(payload)!=row['maintained_sha256'] or len(payload)!=row['maintained_bytes']: raise ValueError('Target drift')
        if row['disposition']!='extracted-source':
            if target.read_bytes()!=payload: raise ValueError('Owner drift')
        elif target not in seen:
            if target.parent!=BASE/'snapshots' or target.exists(): raise ValueError('Unsafe output')
            target.write_bytes(payload)
            seen.add(target)
    (BASE/'snapshots/source-provenance.json').write_text(json.dumps(value,indent=2)+'\n',encoding='utf-8')
    print(len(value['files']),len(seen))

if __name__=='__main__':
    import sys
    if sys.argv[1:]==['apply']:
        apply(json.load(sys.stdin))
    elif not sys.argv[1:]:
        print(json.dumps(plan()))
    else:
        raise SystemExit('usage: snapshot_plan.py [apply]')
