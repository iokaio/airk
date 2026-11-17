# SPDX-License-Identifier: Apache-2.0
"""Read-only replay of reviewed ORT dependency patch chains; stdout proof only."""
import argparse
import hashlib
import io
import json
import tarfile
import zipfile
import project as owner
import patch_replay

CHAINS={
 'abseil_cpp':['vcpkg-ports/abseil/'+n+'.patch' for n in ['absl_windows','absl_cuda_warnings','absl_cuda13_member_template','absl_msvc_unreachable_code']],
 'date':['patches/date/date.patch'],
 'eigen':['patches/eigen/s390x-build.patch','patches/eigen/s390x-build-werror.patch'],
 'flatbuffers':['patches/flatbuffers/flatbuffers.patch'],
 'onnx':['patches/onnx/onnx.patch'],
 'protobuf':['patches/protobuf/'+n+'.patch' for n in ['protobuf_android_log','protobuf_msvc_unreachable_code','protobuf_msvc_map_unreachable_code','protobuf_compiler_incomplete_type']],
 'pytorch_cpuinfo':['patches/cpuinfo/enable_deinit_refcounting.patch'],
}
OLD_CHAINS={'abseil_cpp':['patches/abseil/absl_windows.patch'],'flatbuffers':CHAINS['flatbuffers'],'onnx':CHAINS['onnx'],'protobuf':['patches/protobuf/protobuf_cmake.patch']}

def selected_members(raw,names,tar=False):
    result={};seen=set()
    with (tarfile.open(fileobj=io.BytesIO(raw),mode='r:gz') if tar else zipfile.ZipFile(io.BytesIO(raw))) as archive:
        members=archive.getmembers() if tar else archive.infolist()
        if len(members)>100000:raise ValueError('archive member count exceeds bound')
        for row in members:
            if (not row.isfile()) if tar else row.is_dir():continue
            name=row.name if tar else row.filename
            owner.safe_relative(name);_,sep,relative=name.partition('/')
            if not sep or relative in seen:raise ValueError('ambiguous archive member')
            seen.add(relative)
            if relative not in names:continue
            size=row.size if tar else row.file_size
            if size>owner.MAX_FILE:raise ValueError('member exceeds bound')
            with (archive.extractfile(row) if tar else archive.open(row)) as stream:body=stream.read(owner.MAX_FILE+1)
            if len(body)!=size:raise ValueError('member size differs')
            result[relative]=body
    return result

def project(index_path,index_sha):
    code_pins=[owner.file_pin('scripts/arbiter/analysis/upstream_ort/'+name) for name in ['project_patches.py','patch_replay.py','project.py']]
    index,index_pin=owner.document(index_path,index_sha)
    if index.get('schema')!='airk.private.upstream-projection-index/1':raise ValueError('wrong index schema')
    pins={r['path'].replace('\\','/'):r for r in index['files']}
    if len(pins)!=len(index['files']):raise ValueError('duplicate index paths')
    def indexed(suffix):
        matching=[p for p in pins if p.endswith(suffix)]
        if len(matching)!=1:raise ValueError('ambiguous indexed evidence')
        pin=pins[matching[0]];path=owner.Path(pin['path']).relative_to(owner.ROOT).as_posix() if owner.Path(pin['path']).is_absolute() else pin['path']
        return owner.document(path,pin['sha256'])
    summary,summary_pin=indexed('/summary/summary.json');tree,tree_pin=indexed('/legacy-source/upstream-tree.json')
    if summary.get('schema')!='airk.private.ort-projection-summary/1' or tree.get('schema')!='airk.upstream-git-tree/1' or tree.get('commit')!=owner.OLD_COMMIT:raise ValueError('wrong summary/tree identity')
    lock,lock_pin=owner.document(owner.LOCK,owner.LOCK_SHA);seal,seal_pin=owner.document(owner.SEAL,lock['source']['inventory_sha256']);sealed={r['path']:r for r in seal}
    old_build,old_build_pin=owner.document(owner.OLD+'/windows-static-build-result.json')
    if old_build['upstream_revision']!=owner.OLD_COMMIT:raise ValueError('legacy revision differs')
    deps,deps_pin=owner.document(owner.OLD+'/cpu-dependencies.json',old_build['dependency_manifest_sha256'])
    if deps.get('schema')!=1 or deps.get('upstream_commit')!=owner.OLD_COMMIT:raise ValueError('wrong legacy dependency identity')
    grouped={}
    for row in summary['unmatched']:
        if 'dependency' not in row:raise ValueError('unreviewed unmatched source family')
        source=row['path'];member=source.split('/b/Release/_deps/',1)[1].split('/',1)[1];legacy=source.startswith(owner.OLD+'/')
        grouped.setdefault((legacy,row['dependency']),[]).append((source,member))
    results=[]
    for (legacy,name),rows in grouped.items():
        declared=next(r for r in (deps if legacy else lock)['dependencies'] if r['name']==name)
        archive=(owner.OLD+'/dependency-mirror/'+declared['mirror_relative_path']) if legacy else 'artifacts/advanced-split/ort130-x64-owned-r5/downloads/'+name+'.zip'
        raw,archive_pin=owner.authenticated_bytes(archive,declared['sha256'])
        if len(raw)!=declared['bytes']:raise ValueError('archive size differs')
        original=selected_members(raw,{member for _,member in rows},declared['url'].endswith('.tar.gz'))
        patchlist=[];chain_errors=[]
        for relative in (OLD_CHAINS if legacy else CHAINS)[name]:
            rel='cmake/'+relative;base=owner.OLD+'/onnxruntime-source/' if legacy else 'artifacts/advanced-split/ort130-x64-owned-r5/source/'
            body,pin=owner.authenticated_bytes(base+rel)
            if legacy:
                normalized=body.replace(b'\r\n',b'\n');oid=hashlib.sha1(b'blob '+str(len(normalized)).encode()+b'\0'+normalized).hexdigest()
                if tree['files'].get(rel)!=oid:raise ValueError('legacy patch not exact upstream Git member')
            elif sealed.get(rel,{}).get('sha256')!=pin['sha256']:raise ValueError('current patch not sealed upstream member')
            try:parsed=patch_replay.sections(body)
            except ValueError as exc:
                parsed={};chain_errors.append({'patch':pin,'error':str(exc)})
            patchlist.append((pin,parsed))
        for source,member in rows:
            actual,actual_pin=owner.authenticated_bytes(source);before=original.get(member);applied=[];error=None;reproduced=before
            if chain_errors:error='declared chain contains unsupported patch syntax'
            elif before is None:error='archive member absent'
            else:
                try:
                    for pin,sections in patchlist:
                        if member in sections:
                            reproduced=patch_replay.apply(reproduced,sections[member]);applied.append(pin)
                except ValueError as exc:error=str(exc)
            expected=hashlib.sha256(reproduced).hexdigest() if reproduced is not None else None
            results.append({'source':source,'source_sha256':actual_pin['sha256'],'source_bytes':actual_pin['bytes'],'dependency':name,'legacy':legacy,'member':member,'archive':archive_pin,'original_member_sha256':hashlib.sha256(before).hexdigest() if before is not None else None,'ordered_patches':applied,'declared_chain':[p for p,_ in patchlist],'reproduced_sha256':expected,'exact':error is None and reproduced==actual,'error':error,'chain_errors':chain_errors})
    initial=dict(owner.CHECKED)
    for path,pin in initial.items():
        if owner.file_pin(path)!=pin:raise ValueError('evidence changed')
    return {'schema':'airk.private.ort-patch-replay/1','code':code_pins,'method':'Chosen ordered upstream patch derivation; not proof of historical command execution','index':index_pin,'summary':summary_pin,'source_evidence':[lock_pin,seal_pin,old_build_pin,deps_pin,tree_pin],'files':results,'exact':sum(r['exact'] for r in results),'unresolved':sum(not r['exact'] for r in results),'native_execution':False,'vendor_files_written':False}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--index',required=True);parser.add_argument('--index-sha256',required=True);args=parser.parse_args()
    print(json.dumps(project(args.index,args.index_sha256),indent=2))
