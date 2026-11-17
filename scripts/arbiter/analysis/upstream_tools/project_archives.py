# SPDX-License-Identifier: Apache-2.0
"""Read-only exact archive/member projection; emits metadata, never vendor source."""
import argparse
import hashlib
import json
import io
from pathlib import Path, PurePosixPath
import re
import stat
import tarfile
import zipfile

REPOSITORY = Path(__file__).resolve().parents[4]
SOURCE_LIMIT = 16 << 20
ARCHIVE_LIMIT = 1 << 30

def strict_json(raw):
    def unique(pairs):
        result={}
        for key,value in pairs:
            if key in result:raise ValueError('duplicate JSON key')
            result[key]=value
        return result
    return json.loads(raw,object_pairs_hook=unique)

def safe_relative(value):
    if not isinstance(value, str) or not value or '\\' in value or ':' in value or value.startswith('/') or any(p in ('', '.', '..') for p in value.split('/')):
        raise ValueError('canonical relative path required')
    return value

def checked(root, relative):
    path = root / safe_relative(relative)
    for part in [path, *path.parents]:
        info = part.lstat()
        if part.is_symlink() or getattr(info, 'st_file_attributes', 0) & 0x400:
            raise ValueError('aliased source/evidence refused')
        if part == root:break
    return path

def stamp(info):return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns)

def measure(path, limit=SOURCE_LIMIT):
    before=path.stat()
    if not path.is_file() or before.st_size > limit:raise ValueError('regular bounded input required')
    digest=hashlib.sha256();count=0
    with path.open('rb') as stream:
        while block:=stream.read(1 << 20):
            count+=len(block)
            if count>limit or count>before.st_size:raise ValueError('input grew during read')
            digest.update(block)
    if count!=before.st_size or stamp(before)!=stamp(path.stat()):raise ValueError('input changed during read')
    return dict(sha256=digest.hexdigest(),bytes=count)

def consumed(path, limit=SOURCE_LIMIT):
    before=path.stat()
    if not path.is_file() or before.st_size>limit:raise ValueError('regular bounded input required')
    with path.open('rb') as stream:data=stream.read(limit+1)
    if len(data)>limit or len(data)!=before.st_size or stamp(before)!=stamp(path.stat()):raise ValueError('input changed during consumed read')
    return data,dict(sha256=hashlib.sha256(data).hexdigest(),bytes=len(data))

def pinned_bytes(root,row,limit=SOURCE_LIMIT):
    data,actual=consumed(checked(root,row['path']),limit)
    if any(actual[k]!=row[k] for k in ('sha256','bytes')):raise ValueError('consumed evidence pin changed: '+row['path'])
    return data

def pinned(root, row, limit=SOURCE_LIMIT):
    path=checked(root,row['path']);actual=measure(path,limit)
    if any(actual[k]!=row[k] for k in ('sha256','bytes')):raise ValueError('exact evidence pin changed: '+row['path'])
    return path

def member_for(source, spec):
    safe_relative(source)
    for root in spec['roots']:
        if source.startswith(root+'/'):
            return spec['archive_prefix']+source[len(root)+1:]
    prefix=spec.get('askpass_member_prefix')
    if prefix and re.search(r'/User/globalStorage/vscode\.git/askpass/[0-9a-f]+/(askpass|askpass-empty|ssh-askpass|ssh-askpass-empty)\.sh$',source):
        return prefix+PurePosixPath(source).name
    if prefix and re.search(r'/User/globalStorage/vscode\.git/askpass/[0-9a-f]+/askpass-main\.js$',source):
        return prefix+'askpass-main.js'
    return None

def archive_roster(archive, kind):
    entries={};folded=set()
    for info in (archive.infolist() if kind=='zip' else archive.getmembers()):
        name=info.filename if kind=='zip' else info.name
        name=name.rstrip('/')
        safe_relative(name)
        if name.casefold() in folded:raise ValueError('duplicate or case-colliding archive member')
        folded.add(name.casefold())
        directory=info.is_dir() if kind=='zip' else info.isdir()
        mode=(info.external_attr >> 16) if kind=='zip' else info.mode
        if (kind=='zip' and stat.S_ISLNK(mode)) or (kind!='zip' and not (info.isfile() or directory)):
            raise ValueError('archive alias/special member refused')
        if not directory:entries[name]=info
    return entries

def member_digest(archive, kind, info):
    size=info.file_size if kind=='zip' else info.size
    if size>SOURCE_LIMIT:raise ValueError('candidate archive member exceeds source cap')
    stream=archive.open(info) if kind=='zip' else archive.extractfile(info)
    digest=hashlib.sha256();count=0
    with stream:
        while block:=stream.read(1 << 20):
            count+=len(block)
            if count>size or count>SOURCE_LIMIT:raise ValueError('archive member exceeds declared size')
            digest.update(block)
    if count!=size:raise ValueError('truncated archive member')
    return dict(sha256=digest.hexdigest(),bytes=count)

def project(delta_path, catalog_path, root=REPOSITORY):
    delta_raw,_=consumed(delta_path,256 << 20);catalog_raw,catalog_pin=consumed(catalog_path)
    delta=strict_json(delta_raw);catalog=strict_json(catalog_raw)
    if catalog.get('schema')!='airk.upstream-tool-archives/1':raise ValueError('unsupported catalog')
    candidates={}
    for lane in delta['lanes'].values():
        for row in lane:
            if row['path'] in candidates and candidates[row['path']]!=row:raise ValueError('conflicting candidate identities')
            candidates[row['path']]=row
    code=Path(__file__).resolve()
    common=[dict(path=catalog_path.relative_to(root).as_posix(),**catalog_pin),dict(path=code.relative_to(root).as_posix(),**measure(code))]
    result=dict(schema='airk.private.upstream-tool-projection/1',inputs=[dict(path=delta_path.relative_to(root).as_posix(),sha256=hashlib.sha256(delta_raw).hexdigest(),bytes=len(delta_raw)),common[0]],archives=[],files=[],errors=[],not_selected_count=0,no_files_created=True,no_network=True,no_vendor_code_executed=True)
    selected=set()
    for spec in catalog['archives']:
        rows=[(row,member_for(path,spec)) for path,row in candidates.items() if member_for(path,spec) is not None]
        if not rows:continue
        try:
            authority=strict_json(pinned_bytes(root,spec['authority']))
            for key in spec['authority_selector']:authority=authority[key]
            if authority!=spec['archive']['sha256']:raise ValueError('archive pin differs from retained authority')
            path=checked(root,spec['archive']['path'])
            archive_raw=pinned_bytes(root,spec['archive'],ARCHIVE_LIMIT)
            archive_stream=io.BytesIO(archive_raw)
            archive_handle=zipfile.ZipFile(archive_stream,'r') if spec['format']=='zip' else tarfile.open(fileobj=archive_stream,mode='r:*')
            pending=[]
            with archive_handle as archive:
                roster=archive_roster(archive,spec['format']);cache={};passed=0
                for row,member in rows:
                    selected.add(row['path'])
                    try:
                        if member not in roster:raise ValueError('candidate absent from original archive')
                        if member not in cache:cache[member]=member_digest(archive,spec['format'],roster[member])
                        actual=measure(checked(root,row['path']))
                        if actual!={'sha256':row['source_sha256'],'bytes':row['bytes']}:raise ValueError('candidate differs from frozen inventory')
                        if actual!=cache[member]:raise ValueError('candidate differs from exact archived member')
                        pending.append(dict(source=row['path'],source_sha256=actual['sha256'],source_bytes=actual['bytes'],disposition='upstream_build_input',reason='Exact current source bytes equal a member of the original archive authenticated by retained upstream metadata/lock; no source copied or executed.',representation='raw',evidence=[*common,spec['authority']],upstream=dict(repository=spec['repository'],revision=spec['revision'],path=member,source_sha256=actual['sha256'],archive=spec['archive']),projection=dict(archive_member=member,archive_member_sha256=cache[member]['sha256'],archive_member_bytes=cache[member]['bytes'])))
                        passed+=1
                    except (OSError,ValueError,KeyError) as error:result['errors'].append(dict(source=row['path'],archive=spec['name'],error=str(error)))
            if measure(path,ARCHIVE_LIMIT)!={k:spec['archive'][k] for k in ('sha256','bytes')}:raise ValueError('archive changed during projection')
            result['files'].extend(pending)
            result['archives'].append(dict(name=spec['name'],archive=spec['archive'],authority=spec['authority'],selected=len(rows),verified=passed,exact_member_equality=True))
            del archive_raw,archive_stream,archive_handle
        except (OSError,ValueError,KeyError,zipfile.BadZipFile,tarfile.TarError) as error:
            selected.update(row['path'] for row,_ in rows)
            result['errors'].append(dict(archive=spec['name'],selected=len(rows),error=str(error)))
    result['not_selected_count']=len(candidates)-len(selected)
    result['selected_count']=len(selected);result['verified_count']=len(result['files']);result['complete_within_selected_scope']=not result['errors'] and len(selected)==len(result['files'])
    return result

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--delta',type=Path,required=True);p.add_argument('--catalog',type=Path,default=Path(__file__).with_name('archive-catalog.json'));a=p.parse_args()
    result=project(a.delta.resolve(),a.catalog.resolve());print(json.dumps(result,separators=(',',':')))
    if result['errors']:raise SystemExit(1)

if __name__=='__main__':main()
