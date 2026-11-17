# SPDX-License-Identifier: Apache-2.0
"""Preserve reviewed AIRK operational history; never execute historical scripts.

The verified human shell must preflight output parents and audit created files.
This adapter accepts only the exact reviewed broad inventory and source pins.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT=Path(__file__).resolve().parents[4]
HERE=Path(__file__).resolve().parent
INVENTORY=ROOT/'artifacts/v190/final-plan/exec-20261116T052942Z/source-broad-delta-r1.json'
INVENTORY_SHA='ce0b4e7eb0a4755796c4cb8d325165276b749deec49f0963b0af4fec8718d5fc'
EXTENSIONS={'.py','.ps1','.sh','.cmd','.cjs','.mjs'}
UPSTREAM={'artifacts/local/one-airk-1.8.14/upstream-metadata/tools/ci_build/build.py','artifacts/local/one-airk-1.8.14/upstream-metadata/tools/ci_build/build_args.py'}

def sha(raw):return hashlib.sha256(raw).hexdigest()

def prefix(raw,suffix):
    if b'SPDX-License-Identifier: Apache-2.0' in raw.split(b'\n',1)[0]:return raw,[]
    marker='// ' if suffix in {'.cjs','.mjs'} else 'REM ' if suffix=='.cmd' else '# '
    text=marker+'SPDX-License-Identifier: Apache-2.0\n'
    return text.encode()+raw,[{'kind':'spdx-prefix','prefix':text,'reason':'License header only; recover every original body byte by stripping this prefix'}]

def inverse(raw,changes):
    for change in reversed(changes):
        if change.get('kind')!='spdx-prefix':raise ValueError('not a prefix-only owner')
        value=change['prefix'].encode('ascii')
        if value not in [b'# SPDX-License-Identifier: Apache-2.0\n',b'// SPDX-License-Identifier: Apache-2.0\n',b'REM SPDX-License-Identifier: Apache-2.0\n'] or not raw.startswith(value):raise ValueError('invalid literal license prefix')
        raw=raw[len(value):]
    return raw

def selected(row):
    return not row['path'].endswith('/service-cli/bin/airk.cmd') and row['path']!='artifacts/tmporolob5v/synthetic.py' and row['bytes']>0 and '/skills/.system/' not in row['path'] and not row['path'].startswith('artifacts/security-rmcp-2026-11-09/source/') and '/airk-native-Ya1UQi/project-' not in row['path'] and Path(row['path']).suffix in EXTENSIONS and '/globalStorage/vscode.git/' not in row['path'] and row['path'] not in UPSTREAM

def purpose(path):
    name=path.lower()
    if any(x in name for x in ['frozen-replay','benchmark','performance']):return 'Frozen workload replay or performance observation; original workload and predecessor/runtime paths remain historical'
    if any(x in name for x in ['install','upgrade','package','publish','release','delivery']):return 'Installation, upgrade, packaging or delivery preparation/probe; no current install, publication or release authority'
    if any(x in name for x in ['mac','neo','ssh','remote']):return 'Mac/remote source transfer or platform command driver; needs explicit authenticated remote identity and fresh inputs'
    if any(x in name for x in ['build','clippy','cargo','test','check','regression']):return 'Scoped build/test/check orchestration and receipt collection; not a portable fresh-run launcher'
    if any(x in name for x in ['prepare','apply','mount','normalize','rewrite','patch']):return 'Historical source/input preparation or exact-anchor adapter; never apply to current source without review'
    return 'Operational analysis, measurement, diagnostic or evidence helper; consumes the explicit historical inputs in its source'

def main():
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--write',action='store_true');a=ap.parse_args()
    data=INVENTORY.read_bytes()
    if sha(data)!=INVENTORY_SHA:raise ValueError('broad inventory changed')
    rows=json.loads(data)['lanes']['component_review'];rows=[r for r in rows if selected(r)]
    canonical={};observed={}
    names=subprocess.check_output(['git','-C',str(ROOT),'ls-files','-z']).decode().split('\0')
    for name in names:
        if not name.startswith('scripts/arbiter/') or not name.endswith('.json') or not any(x in name for x in ['source-map','source-provenance']):continue
        doc=json.loads((ROOT/name).read_bytes())
        for row in doc.get('files',[]):
            if not all(k in row for k in ['original_sha256','maintained_sha256','destination']):continue
            changes=row.get('transformations',[])
            if not isinstance(changes,list) or any(not isinstance(c,dict) or c.get('kind')!='spdx-prefix' for c in changes):continue
            target=ROOT/row['destination']
            if not target.is_file():continue
            kept=observed.setdefault(str(target),target.read_bytes())
            if sha(kept)!=row['maintained_sha256']:continue
            try:original=inverse(kept,changes)
            except ValueError:continue
            if sha(original)==row['original_sha256']:canonical.setdefault(row['original_sha256'],(target,kept,changes,name))
    result=[];copies={};new_versions={}
    for row in rows:
        source=ROOT/row['path'];raw=source.read_bytes()
        if sha(raw)!=row['source_sha256'] or len(raw)!=row['bytes']:raise ValueError('source changed: '+str(source))
        if re.search(rb'-----BEGIN (?:RSA |OPENSSH |EC )?PRIVATE KEY-----|\bgh[pousr]_[A-Za-z0-9]{30,}|\bAKIA[A-Z0-9]{16}\b',raw):raise ValueError('credential-shaped literal requires review: '+str(source))
        if row['source_sha256'] in canonical:
            dest,kept,changes,owner_map=canonical[row['source_sha256']];status='existing_prefix_only_maintained_owner'
        elif row['source_sha256'] in new_versions:
            dest,kept,changes=new_versions[row['source_sha256']];owner_map=None;status='historical_operational_source_preserved'
        else:
            dest=HERE/'sources'/(row['source_sha256']+'--'+source.name);kept,changes=prefix(raw,source.suffix);copies[dest]=kept;new_versions[row['source_sha256']]=(dest,kept,changes);owner_map=None;status='historical_operational_source_preserved'
        assert inverse(kept,changes)==raw
        result.append({'source':row['path'],'original_sha256':sha(raw),'original_bytes':len(raw),'destination':dest.relative_to(ROOT).as_posix(),'maintained_sha256':sha(kept),'maintained_bytes':len(kept),'transformations':changes,'status':status,'role':purpose(row['path']),'existing_owner_map':owner_map})
    report={'schema':'airk.source-relocation/1','inventory':{'path':INVENTORY.relative_to(ROOT).as_posix(),'sha256':INVENTORY_SHA},'scope':'Explicit standalone operational suffixes; excludes Git askpass helpers and two upstream Microsoft ORT scripts assigned separate provenance owners','historical_code_executed':False,'qualification':False,'files':result}
    if a.write:
        target=HERE/'source-map.json'
        if target.exists():raise ValueError('fresh map required')
        for p,raw in copies.items():
            if p.exists():raise ValueError('fresh copy required: '+str(p))
        for p,raw in copies.items():p.write_bytes(raw)
        target.write_bytes((json.dumps(report,indent=2)+'\n').encode())
    print(json.dumps({'paths':len(result),'new_distinct_files':len(copies),'existing_owner_paths':sum(r['existing_owner_map'] is not None for r in result),'written':a.write}))

if __name__=='__main__':main()
