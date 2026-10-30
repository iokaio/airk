# SPDX-License-Identifier: Apache-2.0
"""Acquire pinned public inputs and extract only native AIRK vocabulary.

Clones, vocabulary, source indexes and logs stay in ignored target/parser-train.
No source code, index state or expanded corpus is copied into public artifacts.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import shutil
from retention import retain

ROOT=Path(__file__).resolve().parents[2]
WORK=ROOT/'target/parser-train'
LOCK=Path(__file__).with_name('repositories.lock.json')

def git(*args,cwd=None):
    result=subprocess.run(['git',*map(str,args)],cwd=cwd,check=True,capture_output=True,text=True,encoding='utf-8',timeout=300)
    return result.stdout.strip()

def licenses(directory,revision):
    result=[]
    for file in sorted(directory.iterdir()):
        if file.is_file() and file.name.upper().startswith(('LICENSE','LICENCE','COPYING')):
            content=file.read_bytes()
            if len(content)>1000000:raise ValueError('Oversized license input')
            text=content.decode('utf-8',errors='replace')
            label=('Apache-2.0' if 'Apache License' in text and 'Version 2.0' in text else
                   'MIT' if 'Permission is hereby granted' in text else
                   'BSD-3-Clause' if 'Redistribution and use in source and binary forms' in text and ('Neither the name' in text or 'Neither name' in text) else
                   'BSD-2-Clause' if 'Redistribution and use in source and binary forms' in text else None)
            if not label:raise ValueError(f'License requires manual review: {directory.name}/{file.name}')
            result.append({'path':file.name,'sha256':hashlib.sha256(content).hexdigest(),'detected_license':label,'revision':revision})
    if not result:raise ValueError(f'No root license found for {directory.name}')
    return result

def acquire():
    manifest=json.loads(Path(__file__).with_name('repositories.json').read_text(encoding='utf-8'))
    prior=json.loads(LOCK.read_text(encoding='utf-8')) if LOCK.exists() else {'repositories':[]}
    pins={row['repository']:row for row in prior['repositories']}
    rows=[]
    for item in manifest['repositories']:
        repository=item['repository']
        if not re.fullmatch(r'[A-Za-z0-9_-]+/[A-Za-z0-9_.-]+',repository):raise ValueError('Invalid repository identifier')
        directory=WORK/'repos'/repository.replace('/','--')
        directory.parent.mkdir(parents=True,exist_ok=True)
        if not directory.exists():
            print(f'Cloning metadata: {repository}',flush=True)
            git('clone','--filter=blob:none','--depth=1','--no-checkout','https://github.com/'+repository+'.git',directory)
        revision=pins.get(repository,{}).get('revision') or git('-C',directory,'rev-parse','HEAD')
        if not re.fullmatch(r'[a-f0-9]{40}',revision):raise ValueError('Invalid pinned revision')
        if git('-C',directory,'rev-parse','HEAD')!=revision:
            if git('-C',directory,'status','--porcelain'):raise ValueError('Existing training checkout has edits; it was preserved')
            git('-C',directory,'fetch','--depth=1','origin',revision)
        git('-C',directory,'sparse-checkout','init','--cone')
        git('-C',directory,'sparse-checkout','set',*item['paths'])
        git('-C',directory,'checkout','--detach',revision)
        license_files=licenses(directory,revision)
        row={**item,'revision':revision,'license_files':license_files}
        rows.append(row)
        (WORK/'acquisition-progress.json').write_text(json.dumps({'schema':1,'repositories':rows},indent=2)+'\n',encoding='utf-8')
        print(f'Pinned {repository}@{revision[:12]}: '+', '.join(f['detected_license'] for f in license_files),flush=True)
    LOCK.write_text(json.dumps({'schema':1,'repositories':rows},indent=2)+'\n',encoding='utf-8')

def sampled_source(source,slug):
    """A fixed, recorded native-extraction sample; never claim whole-repository coverage."""
    extensions={'.rs','.py','.ts','.tsx','.go','.cs','.cpp','.cc','.h','.java','.md'}
    files=[]
    for file in source.rglob('*'):
        relative=file.relative_to(source)
        if '.git' in relative.parts or file.suffix.lower() not in extensions or not file.is_file() or file.is_symlink():continue
        if not file.resolve().is_relative_to(source.resolve()) or file.stat().st_size>131072:continue
        files.append((hashlib.sha256(relative.as_posix().encode()).hexdigest(),file,relative))
    files.sort(key=lambda row:row[0])
    selected=[];total=0;per_extension={}
    for _,file,relative in files:
        extension=file.suffix.lower();cap=32 if extension=='.md' else 48
        if per_extension.get(extension,0)>=cap or total+file.stat().st_size>2097152:continue
        selected.append((file,relative));total+=file.stat().st_size;per_extension[extension]=per_extension.get(extension,0)+1
        if len(selected)>=64:break
    records=[{'path':relative.as_posix(),'sha256':hashlib.sha256(file.read_bytes()).hexdigest()} for file,relative in selected]
    identity=hashlib.sha256(json.dumps(records,sort_keys=True).encode()).hexdigest()
    directory=WORK/'corpora'/(slug+'-sample-'+identity[:12])
    if not directory.exists():
        directory.mkdir(parents=True)
        for file,relative in selected:
            target=directory/relative;target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(file,target)
        (directory.parent/(directory.name+'.manifest.json')).write_text(json.dumps({'schema':1,'sample_sha256':identity,'source_files':records,'source_bytes':total,'scope':'deterministic bounded sample, not complete repository coverage'},indent=2)+'\n',encoding='utf-8')
    retain(directory.parent/(directory.name+'.manifest.json'))
    return directory,identity

def extract(binary,sample=False):
    manifest=json.loads(LOCK.read_text(encoding='utf-8'))
    for item in manifest['repositories']:
        slug=item['repository'].replace('/','--')+'-'+item['revision'][:12]
        source=WORK/'repos'/item['repository'].replace('/','--')
        if sample:
            source,identity=sampled_source(source,slug)
            slug+='-sample-'+identity[:12]
        output=WORK/'vocabulary'/(slug+'.json')
        if output.exists():
            retain(output)
            print(f'Reusing retained vocabulary: {slug}',flush=True);continue
        output.parent.mkdir(parents=True,exist_ok=True)
        logs=WORK/'logs';logs.mkdir(parents=True,exist_ok=True)
        print(f'Native extraction: {slug}',flush=True)
        with (logs/(slug+'.log')).open('w',encoding='utf-8') as log:
            result=subprocess.run([str(binary),'--root',str(source),'--data-dir',str(WORK/'indexes'/slug),'dump-vocab','--output',str(output)],cwd=ROOT,stdout=log,stderr=subprocess.STDOUT,timeout=600)
        if result.returncode:raise RuntimeError(f'Native extraction failed; inspect {slug}.log')
        data=json.loads(output.read_text(encoding='utf-8'))
        if not data['paths']:raise ValueError(f'No indexed vocabulary in {slug}')
        retain(output)
        print(f'Exported {len(data["symbols"])} symbols, {len(data["headings"])} headings, {len(data["paths"])} paths',flush=True)

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--acquire',action='store_true');parser.add_argument('--extract',type=Path);parser.add_argument('--sample',action='store_true')
    args=parser.parse_args();WORK.mkdir(parents=True,exist_ok=True)
    if args.acquire:acquire()
    if args.extract:extract(args.extract.resolve(),args.sample)
