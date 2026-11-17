# SPDX-License-Identifier: Apache-2.0
"""Read-only disposition planner/verifier for the reviewed native/client rescue."""
from pathlib import Path
import argparse, hashlib, json, subprocess
ROOT = Path(__file__).resolve().parents[5]
DEST = Path(__file__).resolve().parent
ROUTING = ROOT / 'artifacts/v190/final-plan/exec-20261116T052942Z/source-rescue-routing-r2.json'
E = 'artifacts/v190/final-plan/exec-20261116T052942Z/'
EXTERNAL = {
 E+'compiler-provenance-r1/official-payload/{app}/Examples/MyDll/C#/MyDll.cs': 'upstream Inno Setup compiler payload example; not authored campaign implementation',
 E+'compiler-provenance-r1/official-payload/{app}/Examples/MyDll/C#/Properties/AssemblyInfo.cs': 'upstream Inno Setup compiler payload example; not authored campaign implementation',
 E+'compiler-provenance-r1/official-payload/{app}/Examples/MyDll/C/MyDll.c': 'upstream Inno Setup compiler payload example; not authored campaign implementation',
 E+'compiler-provenance-r1/official-payload/{app}/Examples/MyDll/C/MyDll.def': 'upstream Inno Setup compiler payload example; not authored campaign implementation',
 E+'gpu-runtime-strategy-proposal-r1/gpu-provision-owner-proposal-r11/powershell-7.6.10-Process.cs': 'Microsoft PowerShell 7.6.10 upstream implementation inspected as evidence; not authored campaign source',
}
def sha(raw):
    return hashlib.sha256(raw).hexdigest()
def checked(path, expected, size=None):
    path=ROOT/path
    if path.is_symlink() or not path.resolve().is_relative_to(ROOT):
        raise ValueError('source escaped repository: '+str(path))
    raw=path.read_bytes()
    if len(raw)>16*1024*1024 or sha(raw)!=expected or (size is not None and len(raw)!=size):
        raise ValueError('source identity differs: '+str(path))
    return raw
def plan():
    raw=ROUTING.read_bytes()
    rows=json.loads(raw)['lanes']['fp16_readiness']
    if len(rows)!=190 or len({r['source_sha256'] for r in rows})!=137:
        raise ValueError('reviewed routing scope changed')
    inventory=ROOT/(E+'gpu-runtime-strategy-proposal-r1/source-change-inventory-r2.json')
    baseline=json.loads(inventory.read_bytes())
    external=dict(EXTERNAL)
    for r in baseline['files']:
        p=E+'gpu-runtime-strategy-proposal-r1/before/'+r['path']
        checked(p,r['before_sha256'])
        external[p]='unchanged ONNX Runtime '+baseline['version']+' upstream '+baseline['upstream']+'; exact before_sha256 in source-change-inventory-r2.json'
    names=subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard','scripts','crates','packages','tools'],cwd=ROOT).decode().split('\0')
    wanted={r['source_sha256'] for r in rows};ext={Path(r['path']).suffix for r in rows};live={}
    for n in names:
        p=ROOT/n
        if n and p.suffix in ext and p.is_file() and not p.resolve().is_relative_to(DEST):
            h=sha(p.read_bytes())
            if h in wanted:live.setdefault(h,[]).append(n)
    objects={line.partition(' ')[0]:line.partition(' ')[2] for line in subprocess.check_output(['git','rev-list','--objects','HEAD'],cwd=ROOT).decode().splitlines()}
    head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT).decode().strip()
    out=[];copies={}
    for r in rows:
        data=checked(r['path'],r['source_sha256'],r['bytes'])
        oid=hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
        row={'source':r['path'],'sha256':r['source_sha256'],'bytes':len(data),'original_classification':r['classification']}
        if r['path'] in external:
            row.update(disposition='upstream_reference_not_authored',reason=external[r['path']])
        elif r['source_sha256'] in live:
            row.update(disposition='exact_maintained_source',owners=sorted(live[r['source_sha256']]),commit_status='worktree source; root commit pending')
        elif oid in objects:
            row.update(disposition='exact_reachable_git_blob',git_blob=oid,reachable_from_commit=head,git_owner_path=objects[oid])
        else:
            h=r['source_sha256'];leaf=Path(r['path']).name
            if leaf.endswith('.rs') or (Path(leaf).suffix in {'.cjs','.mjs'} and not data.startswith(b'// SPDX-License-Identifier: Apache-2.0')):leaf+='.in'
            destination=copies.setdefault(h,'sources/'+h[:16]+'--'+leaf)
            row.update(disposition='preserved_proposal_source',destination=destination,transformation='path only; Rust .rs.in template is not mounted/compiled',status='historical draft; no new execution, integration or qualification claim')
        out.append(row)
    return {'schema':'airk.native-client-source-rescue/1','routing':str(ROUTING.relative_to(ROOT)).replace('\\','/'),'routing_sha256':sha(raw),'repository_head':head,'upstream_baseline_inventory':{'path':str(inventory.relative_to(ROOT)).replace('\\','/'),'sha256':sha(inventory.read_bytes())},'source_count':len(out),'unique_source_count':len(wanted),'preserved_unique_count':len(copies),'files':out}
def verify():
    m=json.loads((DEST/'rescue-map.json').read_bytes())
    for row in m['files']:
        if row['disposition']=='preserved_proposal_source':
            raw=(DEST/row['destination']).read_bytes()
            if sha(raw)!=row['sha256'] or len(raw)!=row['bytes']:raise ValueError('rescued source drift')
        elif row['disposition']=='exact_maintained_source':
            for owner in row['owners']:checked(owner,row['sha256'],row['bytes'])
        elif row['disposition']=='exact_reachable_git_blob':
            raw=subprocess.check_output(['git','cat-file','blob',row['git_blob']],cwd=ROOT)
            if sha(raw)!=row['sha256']:raise ValueError('Git identity differs')
        elif row['disposition']!='upstream_reference_not_authored':
            raise ValueError('unknown disposition')
    print(json.dumps({'verified_rows':len(m['files']),'preserved_unique':m['preserved_unique_count'],'qualification':False}))
if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--plan',action='store_true');a=p.parse_args()
    if a.plan:print(json.dumps(plan(),indent=2))
    else:verify()
