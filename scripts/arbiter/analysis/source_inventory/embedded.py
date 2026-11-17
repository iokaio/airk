# SPDX-License-Identifier: Apache-2.0
"""Read-only supplemental source indicators in artifact JSON/text; no code execution."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import time

LIMIT = 2 << 20
SUFFIXES = frozenset(('.json', '.txt', '.text', '.md'))
NAME = re.compile(r'(source|proposal|patch|generated|implementation|script|command|plan)', re.I)
CODE = re.compile(r'(?m)^(?:[ \t]*(?:def |class |async def |import |from \S+ import |function |param\(|#include\s*[<"]|pub(?:\([^)]*\))? (?:fn|struct|enum) |fn \w+\(|use \w+::)|diff --git |--- a/|\+\+\+ b/)')
KEY = re.compile(r'(?:^|_)(?:code|script|source_body|source_text|contents|patch|command_text)(?:$|_)', re.I)

def indicators(value, location='$', depth=0):
    if depth > 80:
        raise ValueError('JSON nesting exceeds supplemental inspection bound')
    found = []
    if isinstance(value, str):
        if '\n' in value and CODE.search(value):
            found.append({'location':location, 'reason':'multiline source syntax indicator', 'text_sha256':hashlib.sha256(value.encode()).hexdigest(), 'characters':len(value)})
    elif isinstance(value, dict):
        for key, item in value.items():
            here = location + '/' + str(key).replace('~','~0').replace('/','~1')
            if isinstance(item,str) and KEY.search(str(key)) and '\n' in item:
                found.append({'location':here,'reason':'multiline source-bearing key','text_sha256':hashlib.sha256(item.encode()).hexdigest(),'characters':len(item)})
            found.extend(indicators(item,here,depth+1))
    elif isinstance(value,list):
        for i,item in enumerate(value):
            found.extend(indicators(item,location+'/'+str(i),depth+1))
    return found

def identity(s):
    return (s.st_dev,s.st_ino,s.st_size,s.st_mtime_ns)

def inspect(path):
    before=path.lstat()
    with path.open('rb') as stream:
        if identity(os.fstat(stream.fileno())) != identity(before):
            raise ValueError('source changed before open')
        raw=stream.read(LIMIT+1)
        if identity(os.fstat(stream.fileno())) != identity(before):
            raise ValueError('source changed during read')
    if identity(path.lstat()) != identity(before):
        raise ValueError('source changed after read')
    if len(raw)>LIMIT:
        return {'status':'size_bound_uninspected','bytes':before.st_size}
    sha=hashlib.sha256(raw).hexdigest()
    try:
        text=raw.decode('utf-8-sig')
    except UnicodeError:
        return {'status':'non_utf8_uninspected','bytes':len(raw),'sha256':sha}
    if path.suffix.lower()=='.json':
        try:
            value=json.loads(text)
        except (ValueError,RecursionError) as error:
            return {'status':'invalid_json_uninspected','bytes':len(raw),'sha256':sha,'error_type':type(error).__name__}
    else:
        value=text
    hits=indicators(value)
    return {'status':'source_indicators' if hits else 'no_selected_indicator','bytes':len(raw),'sha256':sha,'indicators':hits}

def scan(root):
    root=Path(os.path.abspath(root)); start=time.monotonic()
    pending=[root]; rows=[]; errors=[]; excluded=[]; counts={'directories':0,'files':0,'selected':0}
    while pending:
        folder=pending.pop()
        try:
            with os.scandir(folder) as it: entries=sorted(it,key=lambda e:e.name)
            counts['directories']+=1
        except OSError as error:
            errors.append({'path':str(folder),'operation':'scandir','type':type(error).__name__,'message':str(error)})
            continue
        for entry in entries:
            path=folder/entry.name
            try:
                info=entry.stat(follow_symlinks=False)
                if stat.S_ISLNK(info.st_mode) or getattr(info,'st_file_attributes',0)&0x400:
                    excluded.append({'path':str(path),'reason':'reparse/symlink never followed'})
                elif stat.S_ISDIR(info.st_mode):
                    if entry.name in ('.git','.hg','.svn'):
                        excluded.append({'path':str(path),'reason':'VCS object/administrative directory; not source working tree'})
                    else:
                        pending.append(path)
                elif stat.S_ISREG(info.st_mode):
                    counts['files']+=1
                    if path.suffix.lower() not in SUFFIXES: continue
                    counts['selected']+=1
                    row={'path':str(path),'filename_source_indicator':bool(NAME.search(path.name)),**inspect(path)}
                    rows.append(row)
            except (OSError,ValueError,RecursionError) as error:
                errors.append({'path':str(path),'operation':'inspect','type':type(error).__name__,'message':str(error)})
    return {'schema':'airk.private.embedded-source-indicators/1','root':str(root),'elapsed_seconds':time.monotonic()-start,
            'complete_source_coverage':False,'limitations':['Heuristic source indicators are review candidates, not code ownership proof.','JSON/text only; non-UTF8, oversize and inaccessible inputs remain explicit gaps.','No candidate executed; no model/session/weights loaded.','No private source text is copied into this report.','No target/vendor/fixture/environment directory name pruning; only VCS administrative and reparse entries excluded.'],
            'counts':counts,'errors':errors,'excluded':excluded,'files':rows}

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root',type=Path,required=True)
    args=parser.parse_args()
    result=scan(args.root)
    json.dump(result,__import__('sys').stdout,indent=2)
    print()
    return 1 if result['errors'] else 0

if __name__=='__main__':
    raise SystemExit(main())
