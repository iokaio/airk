# SPDX-License-Identifier: Apache-2.0
"""Read-only ORT source/dependency provenance projection; JSON-line bundles on stdout."""
import argparse
from collections import defaultdict
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys
import tarfile
from urllib.parse import urlparse
import zipfile

ROOT = Path(__file__).resolve().parents[4]
MAX_FILE = 16 << 20
MAX_INPUT = 512 << 20
LOCK = 'scripts/static-onnx-runtime-lock.json'
LOCK_SHA = 'd9deff076321020cb15f1cadee6b2270d8098e4518aece7b4ef48b03fd998947'
SEAL = 'artifacts/advanced-split/ort130-x64-owned-r5/source-files.json'
OLD = 'artifacts/local/one-airk-1.8.14'
OLD_COMMIT = '89746dc19a0a1ae59ebf4b16df9acab8f99f3925'
ALIASES = {'eigen3':'eigen','gsl':'microsoft_gsl','nlohmann_json':'json'}
CHECKED = {}

def canonical(value):
    return (json.dumps(value,sort_keys=True,indent=2,ensure_ascii=True)+'\n').encode()

def safe_relative(value):
    if not isinstance(value,str) or '\\' in value or ':' in value or value.startswith('/') or any(p in ('','.','..') for p in value.split('/')):
        raise ValueError('noncanonical relative path')
    return value

def stamp(info):
    return info.st_dev,info.st_ino,info.st_size,info.st_mtime_ns

def input_path(relative):
    path=ROOT/safe_relative(relative)
    for ancestor in (path,*path.parents):
        info=ancestor.lstat()
        if stat.S_ISLNK(info.st_mode) or getattr(info,'st_file_attributes',0)&0x400:
            raise ValueError('reparse input refused: '+relative)
        if ancestor==ROOT:break
    return path

def file_pin(relative,expected=None):
    path=input_path(relative)
    before=path.stat()
    if before.st_size>MAX_INPUT:raise ValueError('input exceeds bound')
    sha=hashlib.sha256();count=0
    with path.open('rb') as stream:
        if stamp(os.fstat(stream.fileno()))!=stamp(before):raise ValueError('input changed before read')
        while chunk:=stream.read(1<<20):
            count+=len(chunk)
            if count>before.st_size or count>MAX_INPUT:raise ValueError('input grew beyond bound')
            sha.update(chunk)
        if count!=before.st_size:raise ValueError('input truncated during read')
        if stamp(os.fstat(stream.fileno()))!=stamp(before):raise ValueError('input changed while read')
    if stamp(path.stat())!=stamp(before):raise ValueError('input changed after read')
    pin={'path':relative,'sha256':sha.hexdigest(),'bytes':before.st_size}
    if expected is not None and pin['sha256']!=expected:raise ValueError('input SHA differs: '+relative)
    CHECKED[relative]=pin
    return pin

def unique_pairs(items):
    result={}
    for key,value in items:
        if key in result:raise ValueError('duplicate JSON key')
        result[key]=value
    return result

def authenticated_bytes(relative,expected=None):
    pin=file_pin(relative,expected)
    with input_path(relative).open('rb') as stream:raw=stream.read(MAX_INPUT+1)
    if len(raw)>MAX_INPUT or len(raw)!=pin['bytes']:raise ValueError('captured input size changed')
    if hashlib.sha256(raw).hexdigest()!=pin['sha256']:raise ValueError('captured input bytes differ')
    return raw,pin

def document(relative,expected=None):
    raw,pin=authenticated_bytes(relative,expected)
    return json.loads(raw,object_pairs_hook=unique_pairs),pin

def observed_rows(reports):
    rows={}
    for report in reports:
        if report.get('schema')!='airk.private.source-inventory/1':raise ValueError('unexpected inventory schema')
        for row in report['candidates']:
            path=safe_relative(row['path'])
            if not path.startswith('artifacts/') or not re.fullmatch('[0-9a-f]{64}',row['source_sha256']):
                raise ValueError('invalid candidate identity')
            if path in rows and any(rows[path].get(k)!=row.get(k) for k in ('source_sha256','bytes','git_blob_oid','normalized_text_git_blob_oid')):
                raise ValueError('conflicting candidate observations')
            rows[path]=row
    return list(rows.values())

def raw_match(row,expected):
    return expected is not None and row['source_sha256']==expected['sha256'] and row['bytes']==expected['bytes']

def blob_oid(raw):
    return hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()

def git_match(row,oid):
    if oid and row['git_blob_oid']==oid:return 'raw'
    if oid and row.get('normalized_text_git_blob_oid')==oid:return 'CRLF-to-LF'
    return None

def upstream_from_url(url):
    if url=='https://github.com/protocolbuffers/protobuf/releases/download/v33.6/protobuf-33.6.tar.gz':
        return 'https://github.com/protocolbuffers/protobuf','v33.6'
    parsed=urlparse(url);parts=parsed.path.strip('/').split('/')
    if parsed.scheme!='https' or parsed.netloc!='github.com' or len(parts)<4 or parts[2]!='archive':
        raise ValueError('unsupported pinned upstream URL')
    revision='/'.join(parts[3:6]) if parts[3]=='refs' else parts[3]
    if revision.endswith('.zip'):revision=revision[:-4]
    return 'https://github.com/'+parts[0]+'/'+parts[1],revision

def validate_bundle(bundle):
    if any(len(text.encode())>MAX_FILE for text in bundle['files'].values()):raise ValueError('group output exceeds16MiB; split required')
    return bundle

def bundle_files(group,proof,rows,destination,extra_files=None):
    proof_raw=canonical(proof)
    proof_path=destination+'/'+group+'/projection.json'
    proof_pin={'path':proof_path,'sha256':hashlib.sha256(proof_raw).hexdigest(),'bytes':len(proof_raw)}
    dispositions=[]
    for row in rows:
        item=dict(row)
        item['evidence']=[*item['evidence'],proof_pin]
        item['projection']={'path':proof_path,'sha256':proof_pin['sha256'],'binding_source':item['source']}
        dispositions.append(item)
    mapping={'schema':'airk.source-disposition/1','scope':'Selected ORT upstream inputs; no runtime qualification or whole-directory exclusion.','files':dispositions}
    outputs={'projection.json':proof_raw.decode(),'source-disposition.json':canonical(mapping).decode(),**(extra_files or {})}
    return validate_bundle({'schema':'airk.private.projection-bundle/1','group':group,'destination':destination,'files':outputs})

def disposition(row,upstream,representation,evidence,reason):
    return {'source':row['path'],'source_sha256':row['source_sha256'],'source_bytes':row['bytes'],
            'disposition':'upstream_build_input','reason':reason,'evidence':evidence,
            'upstream':upstream,'representation':representation}

def archive_members(raw,requested,tar_gzip=False):
    archive=tarfile.open(fileobj=io.BytesIO(raw),mode='r:gz') if tar_gzip else zipfile.ZipFile(io.BytesIO(raw))
    result={};seen=set()
    with archive:
        members=archive.getmembers() if tar_gzip else archive.infolist()
        if len(members)>100000:raise ValueError('archive member count exceeds bound')
        for info in members:
            if (not info.isfile()) if tar_gzip else info.is_dir():continue
            name=info.name if tar_gzip else info.filename
            safe_relative(name)
            _,sep,relative=name.partition('/')
            if not sep or relative in seen:raise ValueError('ambiguous archive member')
            seen.add(relative)
            if relative not in requested:continue
            size=info.size if tar_gzip else info.file_size
            if size>MAX_FILE:continue
            stream=archive.extractfile(info) if tar_gzip else archive.open(info)
            with stream:body=stream.read(MAX_FILE+1)
            if len(body)!=size:raise ValueError('partial/oversized archive member')
            result[relative]={'sha256':hashlib.sha256(body).hexdigest(),'bytes':len(body)}
    return result

def project(primary,extra,destination,containers=None):
    reports=[];observations=[]
    for path,sha in (primary,extra):
        report,pin=document(path,sha);reports.append(report);observations.append(pin)
    rows=observed_rows(reports)
    container_paths=set()
    if containers:
        request,request_pin=document(*containers)
        if request.get('schema')!='airk.embedded-container-proof-requests/1':raise ValueError('wrong container request schema')
        observations.append(request_pin)
        selected={row['path']:row for row in rows}
        for item in request['files']:
            path=safe_relative(item['path'])
            if not ((path.startswith('artifacts/advanced-split/ort130-x64') and ('/source/' in path or '/b/Release/_deps/' in path)) or path.startswith((OLD+'/onnxruntime-source/',OLD+'/upstream-metadata/',OLD+'/b/Release/_deps/'))):continue
            pin=file_pin(path,item['sha256'])
            if pin['bytes']!=item['bytes'] or pin['bytes']>MAX_FILE:raise ValueError('container size differs')
            with input_path(path).open('rb') as stream:raw=stream.read(MAX_FILE+1)
            if len(raw)!=pin['bytes'] or hashlib.sha256(raw).hexdigest()!=pin['sha256']:raise ValueError('container changed')
            row={'path':path,'source_sha256':pin['sha256'],'bytes':len(raw),'git_blob_oid':blob_oid(raw)}
            if b'\r\n' in raw and b'\0' not in raw:
                try:raw.decode('utf-8')
                except UnicodeError:pass
                else:row['normalized_text_git_blob_oid']=blob_oid(raw.replace(b'\r\n',b'\n'))
            if path in selected and any(selected[path].get(k)!=row.get(k) for k in ('source_sha256','bytes','git_blob_oid','normalized_text_git_blob_oid')):raise ValueError('container conflicts with source inventory')
            selected[path]=row;container_paths.add(path)
        rows=list(selected.values())
    lock,lock_pin=document(LOCK,LOCK_SHA)
    seal,seal_pin=document(SEAL,lock['source']['inventory_sha256'])
    if lock['ort_version']!='1.30.0' or len(seal)!=lock['source']['files']:raise ValueError('wrong current ORT identity')
    sealed={row['path']:row for row in seal}
    if len(sealed)!=len(seal):raise ValueError('duplicate sealed upstream member')
    old_build,old_build_pin=document(OLD+'/windows-static-build-result.json')
    if old_build['upstream_revision']!=OLD_COMMIT or old_build['ort_version']!='1.22.1':raise ValueError('wrong legacy ORT identity')
    old_deps,old_deps_pin=document(OLD+'/cpu-dependencies.json',old_build['dependency_manifest_sha256'])
    if old_deps['upstream_commit']!=OLD_COMMIT:raise ValueError('legacy dependencies have wrong upstream')
    tree_bytes=subprocess.run(['git','-C',str(input_path(OLD+'/onnxruntime-source')),'ls-tree','-r','-z',OLD_COMMIT],check=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30).stdout
    if len(tree_bytes)>MAX_FILE:raise ValueError('Git tree exceeds metadata bound')
    tree={}
    for item in tree_bytes.split(b'\0'):
        if not item:continue
        header,name=item.split(b'\t',1);mode,kind,oid=header.split()
        if kind==b'blob':tree[name.decode('utf-8')]=oid.decode('ascii')
    tree_proof={'schema':'airk.upstream-git-tree/1','commit':OLD_COMMIT,'command':['git','ls-tree','-r','-z',OLD_COMMIT],
                'repository_path':OLD+'/onnxruntime-source','tree_listing_sha256':hashlib.sha256(tree_bytes).hexdigest(),'files':tree}
    tree_raw=canonical(tree_proof)
    tree_pin={'path':destination+'/legacy-source/upstream-tree.json','sha256':hashlib.sha256(tree_raw).hexdigest(),'bytes':len(tree_raw)}
    groups=defaultdict(list);bindings=defaultdict(list);unmatched=[];claimed=set()
    def add(group,row,upstream,representation,evidence,method):
        groups[group].append(disposition(row,upstream,representation,evidence,'Exact pinned upstream input representation; no AIRK-authored modifications inferred.'))
        bindings[group].append({'source':row['path'],'source_sha256':row['source_sha256'],'source_bytes':row['bytes'],'upstream':upstream,'representation':representation,'method':method})
        claimed.add(row['path'])
    for row in rows:
        parts=row['path'].split('/')
        if len(parts)>4 and parts[:2]==['artifacts','advanced-split'] and re.fullmatch('ort130-x64(?:-owned-r[1-5])?',parts[2]) and parts[3]=='source':
            rel='/'.join(parts[4:]);expected=sealed.get(rel)
            if raw_match(row,expected):
                add(parts[2]+'-source',row,{'repository':'https://github.com/microsoft/onnxruntime','revision':lock['upstream_commit'],'path':rel,'source_sha256':expected['sha256']},'raw',[lock_pin,seal_pin],'sealed upstream source inventory exact path/SHA/bytes')
            else:unmatched.append({'path':row['path'],'reason':'current upstream sealed member mismatch'})
        elif row['path'].startswith((OLD+'/onnxruntime-source/',OLD+'/upstream-metadata/')):
            prefix=OLD+('/onnxruntime-source/' if '/onnxruntime-source/' in row['path'] else '/upstream-metadata/')
            rel=row['path'][len(prefix):];representation=git_match(row,tree.get(rel))
            if representation:
                identity={'repository':'https://github.com/microsoft/onnxruntime','revision':OLD_COMMIT,'path':rel,'git_blob_oid':tree[rel]}
                if representation=='raw':identity['source_sha256']=row['source_sha256']
                add('legacy-source',row,identity,representation,[old_build_pin,tree_pin],'exact retained upstream Git tree; optional documented CRLF-only checkout')
            else:unmatched.append({'path':row['path'],'reason':'legacy upstream Git member mismatch'})
    # Extract no archives: read only matching ZIP members of independently pinned versions.
    dependency_rows=defaultdict(list)
    current_roots={'artifacts/advanced-split/'+name for name in ('ort130-x64','ort130-x64-owned-r3','ort130-x64-owned-r5')}
    for row in rows:
        marker='/b/Release/_deps/'
        if marker not in row['path']:continue
        base,tail=row['path'].split(marker,1)
        if base not in current_roots and base!=OLD:continue
        folder,sep,member=tail.partition('/')
        if not sep or not folder.endswith('-src'):continue
        name=ALIASES.get(folder[:-4],folder[:-4])
        dependency_rows[(base==OLD,name)].append((base,member,row))
    for (legacy,name),selected in sorted(dependency_rows.items()):
        owner=old_deps if legacy else lock
        declared=next((d for d in owner['dependencies'] if d['name']==name),None)
        if declared is None:
            unmatched.extend({'path':r['path'],'reason':'dependency not declared'} for _,_,r in selected);continue
        archive=(OLD+'/dependency-mirror/'+safe_relative(declared['mirror_relative_path'])) if legacy else 'artifacts/advanced-split/ort130-x64-owned-r5/downloads/'+name+'.zip'
        archive_raw,archive_pin=authenticated_bytes(archive,declared['sha256'])
        if archive_pin['bytes']!=declared['bytes']:raise ValueError('dependency archive size mismatch')
        repository,revision=upstream_from_url(declared['url'])
        tar_gzip=declared['url']=='https://github.com/protocolbuffers/protobuf/releases/download/v33.6/protobuf-33.6.tar.gz'
        cache=archive_members(archive_raw,{member for _,member,_ in selected},tar_gzip)
        for base,member,row in selected:
                expected=cache.get(member)
                if raw_match(row,expected):
                    group=('legacy' if legacy else base.rsplit('/',1)[1])+'-dependencies'
                    add(group,row,{'repository':repository,'revision':revision,'path':member,'source_sha256':expected['sha256']},'raw',[old_deps_pin if legacy else lock_pin,archive_pin],'exact SHA/size-pinned archive member; no extraction')
                else:unmatched.append({'path':row['path'],'reason':'dependency member missing/changed/generated','dependency':name})
    # Authenticate every consumed input again before publishing any output bundle.
    initial=dict(CHECKED)
    for path,pin in initial.items():
        if file_pin(path)!=pin:raise ValueError('evidence changed before output')
    bundles=[{'schema':'airk.private.projection-bundle/1','group':'legacy-source','destination':destination,'files':{'upstream-tree.json':tree_raw.decode()}}]
    for group in sorted(groups):
        for offset in range(0,len(groups[group]),2000):
            leaf=group+'-'+str(offset//2000+1).zfill(3)
            proof={'schema':'airk.upstream-source-projection/1','source_observations':observations,
                   'live_source_rehash_required_at_reconciliation':True,'bindings':bindings[group][offset:offset+2000]}
            bundles.append(bundle_files(leaf,proof,groups[group][offset:offset+2000],destination))
    covered_ort=[r for r in rows if r['path'].startswith('artifacts/advanced-split/ort130-x64') or r['path'].startswith(OLD+'/b/') or r['path'].startswith(OLD+'/onnxruntime-source/')]
    unmatched_paths={row['path'] for row in unmatched}
    summary={'schema':'airk.private.ort-projection-summary/1','matched':len(claimed),'groups':{g:len(v) for g,v in groups.items()},
             'unmatched':unmatched,'other_ort_candidates':[{'path':r['path'],'source_sha256':r['source_sha256'],'bytes':r['bytes']} for r in covered_ort if r['path'] not in claimed and r['path'] not in unmatched_paths],
             'whole_directory_exclusions_proven':False,'qualification':False,'embedded_containers_selected':len(container_paths),'embedded_containers_matched':len(container_paths&claimed)}
    bundles.append({'schema':'airk.private.projection-bundle/1','group':'summary','destination':destination,'files':{'summary.json':canonical(summary).decode()}})
    return [validate_bundle(bundle) for bundle in bundles]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('primary','primary-sha256','extra','extra-sha256','destination'):parser.add_argument('--'+name,required=True)
    parser.add_argument('--containers')
    parser.add_argument('--containers-sha256')
    args=parser.parse_args()
    if bool(args.containers)!=bool(args.containers_sha256):raise ValueError('container path and SHA required together')
    destination=safe_relative(args.destination)
    if not destination.startswith('artifacts/'):raise ValueError('private output destination must be artifacts-relative')
    bundles=project((safe_relative(args.primary),args.primary_sha256),(safe_relative(args.extra),args.extra_sha256),destination,(safe_relative(args.containers),args.containers_sha256) if args.containers else None)
    for bundle in bundles:print(json.dumps(bundle,ensure_ascii=True,separators=(',',':')))
if __name__=='__main__':main()
