# SPDX-License-Identifier: Apache-2.0
"""Capture only an explicitly reviewed pinned roster as inert source templates."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import subprocess

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[3]
HELPER=HERE.parent/'operational_history_r2/preserve.py'
spec=importlib.util.spec_from_file_location('operational_prefix_owner',HELPER)
owner=importlib.util.module_from_spec(spec);spec.loader.exec_module(owner)


def sha(raw):return hashlib.sha256(raw).hexdigest()


def encoded(raw,suffix):
    if suffix=='.css':
        if raw.splitlines()[0]!=b'/* SPDX-License-Identifier: Apache-2.0 */':raise ValueError('reviewed CSS header required')
        return raw,[]
    return owner.prefix(raw,'.cjs')


def checked(relative):
    if not relative or '\\' in relative or ':' in relative or any(x in ('','.','..') for x in relative.split('/')):raise ValueError('canonical relative path required')
    path=ROOT/relative
    for p in [path,*path.parents]:
        info=p.lstat()
        if p.is_symlink() or getattr(info,'st_file_attributes',0)&0x400:raise ValueError('source alias refused')
        if p==ROOT:break
    return path


def prepare(selection):
    copies={};versions={};result=[]
    listing=subprocess.check_output(['git','rev-list','--objects','--all'],cwd=ROOT)
    all_refs={line.split(b' ',1)[0].decode() for line in listing.splitlines()}
    head_listing=subprocess.check_output(['git','rev-list','--objects','HEAD'],cwd=ROOT)
    head={line.split(b' ',1)[0].decode() for line in head_listing.splitlines()}
    for row in selection['files']:
        source=checked(row['source']);raw=source.read_bytes()
        if sha(raw)!=row['source_sha256'] or len(raw)!=row['source_bytes']:raise ValueError('source changed: '+row['source'])
        if source.suffix not in {'.rs','.ts','.tsx','.css'}:raise ValueError('unreviewed source language')
        if re.search(rb'-----BEGIN (?:RSA |OPENSSH |EC )?PRIVATE KEY-----|\bgh[pousr]_[A-Za-z0-9]{30,}|\bAKIA[A-Z0-9]{16}\b',raw):raise ValueError('credential-shaped literal requires review')
        oid=hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()
        if oid in head:raise ValueError('current branch already owns exact source; review deduplication')
        if row['source_sha256'] not in versions:
            kept,changes=encoded(raw,source.suffix)
            destination=HERE/'sources'/(row['source_sha256']+'--'+source.name+'.in')
            copies[destination]=kept;versions[row['source_sha256']]=(destination,kept,changes)
        destination,kept,changes=versions[row['source_sha256']]
        if owner.inverse(kept,changes)!=raw:raise ValueError('source inverse differs')
        result.append({'source':row['source'],'original_sha256':sha(raw),'original_bytes':len(raw),'destination':destination.relative_to(ROOT).as_posix(),'maintained_sha256':sha(kept),'maintained_bytes':len(kept),'transformations':changes,'status':'historical_unmounted_implementation_variant','role':row['role'],'family':row['family'],'local_archive_ref_blob':oid if oid in all_refs else None,'delivery':'Inert source copy in this branch; no runtime mount or history merge'})
    return copies,result


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--selection-sha',required=True);parser.add_argument('--write',action='store_true');args=parser.parse_args()
    raw=(HERE/'selection.json').read_bytes()
    if sha(raw)!=args.selection_sha:raise ValueError('reviewed selection pin changed')
    selection=json.loads(raw)
    copies,rows=prepare(selection)
    report={'schema':'airk.source-relocation/1','selection':{'path':(HERE/'selection.json').relative_to(ROOT).as_posix(),'sha256':sha(raw)},'prefix_helper':{'path':HELPER.relative_to(ROOT).as_posix(),'sha256':sha(HELPER.read_bytes())},'historical_code_executed':False,'qualification':False,'files':rows}
    if args.write:
        if (HERE/'source-map.json').exists() or any(p.exists() for p in copies):raise ValueError('fresh destinations required')
        for path,body in copies.items():path.write_bytes(body)
        (HERE/'source-map.json').write_bytes((json.dumps(report,indent=2)+'\n').encode())
    print(json.dumps({'paths':len(rows),'unique_files':len(copies),'bytes':sum(map(len,copies.values())),'written':args.write}))

if __name__=='__main__':main()
