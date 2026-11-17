# SPDX-License-Identifier: Apache-2.0
"""Read-only exact JavaScript reproduction analysis; JSON stdout, no emission."""
import argparse
import hashlib
import json
import math
from pathlib import Path, PureWindowsPath
import subprocess
import shutil
import time

ROOT = Path(__file__).resolve().parents[4]
PRIOR = ROOT/'scripts/arbiter/research/source_rescue_r1/generated-disposition.json'
COMPILER = ROOT/'packages/vscode/node_modules/typescript/lib/typescript.js'
CONFIG = ROOT/'packages/vscode/tsconfig.json'


def check(deadline):
    if type(deadline) not in (int,float) or not math.isfinite(deadline) or time.monotonic() >= deadline:
        raise ValueError('original analysis deadline expired')


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def exact(raw, digest, size):
    if type(size) is not int or len(raw) != size or sha(raw) != digest:
        raise ValueError('source bytes differ from authenticated inventory')
    return raw


def command(argv, deadline, data=None):
    check(deadline)
    value = subprocess.check_output(argv,input=data,cwd=ROOT,timeout=deadline-time.monotonic())
    check(deadline)
    return value


def read(path, deadline):
    check(deadline)
    chunks=[]
    with path.open('rb') as stream:
        while chunk:=stream.read(1<<20):
            chunks.append(chunk);check(deadline)
    check(deadline)
    return b''.join(chunks)


def unchanged(path, original, deadline):
    if read(path,deadline)!=original:
        raise ValueError('authenticated input drift: '+str(path))


def json_snapshot(path, deadline):
    raw=read(path,deadline)
    return raw,json.loads(raw)


def pin(path, deadline):
    raw=read(path,deadline)
    return {'path':str(path.relative_to(ROOT)).replace('\\','/'),'bytes':len(raw),'sha256':sha(raw)}


def relative_name(value):
    if not isinstance(value,str) or not value or PureWindowsPath(value).is_absolute() or PureWindowsPath(value).drive or '..' in PureWindowsPath(value).parts:
        raise ValueError('repository-relative source path required')
    return value


def repository_path(value):
    path=ROOT/relative_name(value)
    if not path.resolve(strict=True).is_relative_to(ROOT):
        raise ValueError('source escaped repository')
    for part in [path,*path.parents]:
        if part.is_symlink() or getattr(part.lstat(),'st_file_attributes',0)&0x400:
            raise ValueError('source reparse path refused')
        if part==ROOT:break
    return path


def emits_javascript(path):
    return str(path).endswith('.ts') and not str(path).endswith('.d.ts')


def select(rows):
    chosen=[r for r in rows if Path(r['path']).suffix=='.js']
    if len({r['path'] for r in chosen})!=len(chosen):
        raise ValueError('duplicate selected source path')
    return chosen


NODE = r"""
const fs=require('fs'),path=require('path'),crypto=require('crypto');
const request=JSON.parse(fs.readFileSync(0,'utf8'));
const ts=require(path.resolve(request.compiler)),out=[];
const opts=ts.convertCompilerOptionsFromJson(request.options,'packages/vscode').options;
for(const row of request.rows){
 let result;
 try {result=ts.transpileModule(row.text,{compilerOptions:opts,fileName:row.path}).outputText;}
 catch(error){throw Error('TypeScript candidate '+row.path+' SHA256 '+row.key+': '+String(error));}
 const raw=Buffer.from(result);out.push({key:row.key,bytes:raw.length,sha256:crypto.createHash('sha256').update(raw).digest('hex')});
}
process.stdout.write(JSON.stringify(out));
"""


def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--delta',type=Path,required=True)
    ap.add_argument('--delta-sha256',required=True)
    ap.add_argument('--seconds',type=float,default=300)
    args=ap.parse_args()
    if not math.isfinite(args.seconds) or not 0<args.seconds<=600:
        raise ValueError('finite analysis bound in (0,600] required')
    start=time.monotonic();deadline=start+args.seconds
    owner=pin(Path(__file__),deadline)
    delta_raw=read(args.delta,deadline)
    if sha(delta_raw)!=args.delta_sha256:raise ValueError('delta identity differs')
    rows=select(json.loads(delta_raw)['lanes']['component_review'])
    if command(['git','rev-parse','--show-object-format'],deadline).decode().strip()!='sha1':
        raise ValueError('analysis currently requires explicit SHA1 Git object format')
    node_name=shutil.which('node')
    if node_name is None:raise ValueError('Node executable unavailable')
    node_path=Path(node_name).resolve(strict=True)
    node_raw=read(node_path,deadline)
    node={'path':str(node_path),'sha256':sha(node_raw),'bytes':len(node_raw)}
    node_version=command([str(node_path),'--version'],deadline).decode().strip()
    head=command(['git','rev-parse','HEAD'],deadline).decode().strip()
    groups={}
    for row in rows:
        exact(read(repository_path(row['path']),deadline),row['source_sha256'],row['bytes'])
        group=groups.setdefault(row['source_sha256'],{'sha256':row['source_sha256'],'bytes':row['bytes'],'paths':[]})
        if group['bytes']!=row['bytes']:raise ValueError('inconsistent duplicate size')
        group['paths'].append(row['path'])
    compiler=pin(COMPILER,deadline)
    config_raw,config_value=json_snapshot(CONFIG,deadline)
    config={'path':CONFIG.relative_to(ROOT).as_posix(),'sha256':sha(config_raw),'bytes':len(config_raw)}
    options=config_value['compilerOptions']
    prior_raw,prior=json_snapshot(PRIOR,deadline)
    prior_pin={'path':PRIOR.relative_to(ROOT).as_posix(),'sha256':sha(prior_raw),'bytes':len(prior_raw)}
    checked={};proofs={}
    def verify(p):
        path=repository_path(p['path'])
        if str(path) not in checked:checked[str(path)]=read(path,deadline)
        return exact(checked[str(path)],p['sha256'],p['bytes'])
    for row in prior['files']:
        digest=row['source_sha256']
        if digest not in groups:continue
        verify({'path':row['source'],'bytes':row['source_bytes'],'sha256':digest})
        g=row['generated'];verify(g['compiler']);verify(g['source_version'])
        for evidence in row['evidence']:verify(evidence)
        if g['reproduced_js_sha256']!=digest:raise ValueError('prior output identity differs')
        proofs.setdefault(digest,{'kind':'retained_exact_generation_proof','document':prior_pin,'row_source':row['source'],'generated':g,'evidence':row['evidence']})
    candidates={}
    for p in sorted((ROOT/'packages/vscode/src').rglob('*.ts')):
        data=read(p,deadline);key=sha(data);candidates.setdefault(key,{'key':key,'path':p.relative_to(ROOT).as_posix(),'text':data.decode('utf8'),'source':{'path':p.relative_to(ROOT).as_posix(),'sha256':key,'bytes':len(data)}})
    lines=command(['git','rev-list','--objects',head,'--','packages/vscode/src'],deadline).decode().splitlines()
    objects={line.partition(' ')[0]:line.partition(' ')[2] for line in lines if line.endswith('.ts')}
    # cat-file emits reachable blob bodies, never executes them.
    output=command(['git','cat-file','--batch'],deadline,(''.join(oid+'\n' for oid in objects)).encode())
    offset=0
    for oid,path in objects.items():
        end=output.index(b'\n',offset);header=output[offset:end].decode().split();offset=end+1
        if len(header)!=3 or header[0]!=oid or header[1]!='blob':raise ValueError('reachable TS object is not a blob')
        size=int(header[2]);data=output[offset:offset+size];offset+=size+1
        if hashlib.sha1(b'blob '+str(size).encode()+b'\0'+data).hexdigest()!=oid:raise ValueError('Git blob identity differs')
        key=sha(data)
        candidates.setdefault(key,{'key':key,'path':path,'text':data.decode('utf8'),'source':{'git_head':head,'git_blob_oid':oid,'git_path_label':path,'sha256':key,'bytes':size}})
    declarations=[{'path':row['path'],'source':row['source'],'status':'nonemitting_typescript_declaration'} for row in candidates.values() if not emits_javascript(row['path'])]
    values=[row for row in candidates.values() if emits_javascript(row['path'])]
    for offset in range(0,len(values),96):
        batch=values[offset:offset+96]
        emitted=json.loads(command([str(node_path),'-e',NODE],deadline,json.dumps({'compiler':str(COMPILER),'options':options,'rows':batch}).encode()))
        lookup={r['key']:r for r in batch}
        for value in emitted:
            digest=value['sha256']
            if digest in groups and digest not in proofs and value['bytes']==groups[digest]['bytes']:
                proofs[digest]={'kind':'exact_in_memory_transpile','source':lookup[value['key']]['source'],'compiler':compiler,'config':config,'options':options,'method':'TypeScript transpileModule; exact raw UTF-8 bytes; no historical build or typecheck claim'}
    for proof in proofs.values():
        if proof['kind']=='exact_in_memory_transpile' and 'path' in proof['source']:
            source=proof['source'];exact(read(repository_path(source['path']),deadline),source['sha256'],source['bytes'])
    if pin(COMPILER,deadline)!=compiler or pin(CONFIG,deadline)!=config:raise ValueError('compiler/config drift')
    unchanged(args.delta,delta_raw,deadline)
    unchanged(PRIOR,prior_raw,deadline)
    for path,original in checked.items():
        unchanged(Path(path),original,deadline)
    unchanged(node_path,node_raw,deadline)
    if command(['git','rev-parse','HEAD'],deadline).decode().strip()!=head:raise ValueError('HEAD changed during analysis')
    for digest,group in groups.items():
        group['status']='proved_exact_generated_replica' if digest in proofs else 'unresolved'
        group['proof']=proofs.get(digest)
    if pin(Path(__file__),deadline)!=owner:raise ValueError('analyzer source drift')
    check(deadline)
    report={'schema':'airk.private.javascript-provenance-analysis/1','delta':{'path':str(args.delta),'sha256':args.delta_sha256},'head':head,'git_object_format':'sha1','node':node,'node_version':node_version,'owner':owner,'compiler':compiler,'config':config,'qualification':False,'historical_javascript_executed':False,'filesystem_emission':False,'selected_paths':len(rows),'unique_hashes':len(groups),'candidate_typescript_bodies':len(candidates),'emitting_typescript_bodies':len(values),'nonemitting_declarations':declarations,'proved_paths':sum(len(g['paths']) for h,g in groups.items() if h in proofs),'proved_hashes':len(proofs),'elapsed_seconds':time.monotonic()-start,'groups':list(groups.values())}
    print(json.dumps(report,indent=2))


if __name__=='__main__':
    main()
