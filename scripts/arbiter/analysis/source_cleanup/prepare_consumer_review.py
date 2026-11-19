# SPDX-License-Identifier: Apache-2.0
"""Read-only finite source-cleanup consumer evidence; never marks rows apply-ready."""
import argparse
import json
from pathlib import Path
import subprocess
import cleanup

SCOPE=('scripts','crates','packages','.github','docs')
TEXT_SUFFIXES={'.py','.ps1','.sh','.cmd','.bat','.rs','.js','.cjs','.mjs','.ts','.tsx','.json','.toml','.yaml','.yml','.md','.txt','.in','.cpp','.c','.cc','.h','.hpp','.css','.html','.cmake'}

def argument_pin(value):
    path,sha=value.rsplit('=',1);raw=cleanup.read(path)
    if cleanup.digest(raw)!=sha:raise ValueError('explicit input SHA differs')
    return {'path':path,'sha256':sha,'bytes':len(raw)}

def prepare(family,map_pins,custody_pin):
    cleanup.relative(family)
    if not family.startswith('artifacts/'):raise ValueError('artifact family required')
    custody=cleanup.parse(cleanup.pinned(custody_pin))
    if custody.get('schema')!='airk.source-custody-audit/1' or custody.get('refused')!=0:raise ValueError('successful strict original custody evidence required')
    observed={r['source']:r['status'] for r in custody['files']}
    candidates=[];seen=set()
    for pin in map_pins:
        if pin not in custody['inputs']:raise ValueError('custody proof did not authenticate selected map')
        doc=cleanup.parse(cleanup.pinned(pin))
        for row in doc['files']:
            if not row['source'].startswith(family+'/'):continue
            source=cleanup.source_path(row['source'])
            if source.casefold() in seen:raise ValueError('duplicate or case-colliding candidate')
            seen.add(source.casefold())
            if row.get('status') not in {'historical_operational_source_preserved','historical_unmounted_implementation_variant'}:raise ValueError('candidate lacks reviewed authored-history role')
            if observed.get(source)!='present-original':raise ValueError('candidate lacks actual original custody proof')
            kept=cleanup.read(row['destination'])
            if cleanup.digest(kept)!=row['maintained_sha256'] or len(kept)!=row['maintained_bytes']:raise ValueError('maintained bytes changed')
            inverse,_=cleanup.inverse(kept,row['original_sha256'],row['original_bytes'])
            candidates.append({'source':source,'source_sha256':row['original_sha256'],'source_bytes':row['original_bytes'],'destination':row['destination'],'maintained_sha256':row['maintained_sha256'],'maintained_bytes':row['maintained_bytes'],'source_map':pin['path'],'authored_role':row['status'],'inverse':inverse,'review_status':'pending-consumer-review'})
    if not candidates:raise ValueError('empty finite source selection')
    command=['rg','--files',*SCOPE]
    process=subprocess.run(command,cwd=cleanup.ROOT,stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True)
    names=process.stdout.decode('utf-8').splitlines()
    names += [name for name in ('Cargo.toml','Cargo.lock','build.rs','CMakeLists.txt','Makefile') if (cleanup.ROOT/name).is_file()]
    names=sorted(set(name.replace('\\','/') for name in names))
    term=family.rsplit('/',1)[1].lower();matches=[];searched=[];gaps=[];not_text=0
    for name in names:
        if Path(name).suffix.lower() not in TEXT_SUFFIXES and Path(name).name not in {'Cargo.lock','CMakeLists.txt','Makefile'}:not_text+=1;continue
        try:
            raw=cleanup.read(name);text=raw.decode('utf-16') if raw.startswith((b'\xff\xfe',b'\xfe\xff')) else raw.decode('utf-8',errors='surrogateescape')
            file_pin={'path':name,'sha256':cleanup.digest(raw),'bytes':len(raw)};searched.append(file_pin)
            normalized=text.replace('\\\\','/').replace('\\','/').lower()
            if term not in normalized:continue
            lines=[i for i,line in enumerate(normalized.splitlines(),1) if term in line]
            exact=[row['source'] for row in candidates if row['source'].lower() in normalized]
            matches.append({'file':file_pin,'term_line_numbers':lines,'family_root_mentioned':family.lower() in normalized,'exact_candidates_mentioned':exact,'disposition':'requires-role-review-no-automatic-exclusion'})
        except (OSError,ValueError,UnicodeError) as error:gaps.append({'path':name,'error':str(error)})
    for pin in [*map_pins,custody_pin]:cleanup.pinned(pin)
    for row in candidates:
        if cleanup.digest(cleanup.read(row['destination']))!=row['maintained_sha256']:raise ValueError('replacement changed during review preparation')
    return {'schema':'airk.source-cleanup-consumer-preparation/1','family':family,'maps':map_pins,'custody':custody_pin,'helper':{'path':Path(__file__).relative_to(cleanup.ROOT).as_posix(),'sha256':cleanup.digest(Path(__file__).read_bytes())},'candidates':candidates,'search':{'command':command,'roots':list(SCOPE),'case_insensitive_family_term':term,'searched_files':searched,'non_text_files_outside_text_search':not_text,'matches':matches,'errors':gaps},'counts':{'candidates':len(candidates),'original_bytes':sum(r['source_bytes'] for r in candidates),'searched_files':len(searched),'matching_files':len(matches),'errors':len(gaps)},'not_cleanup_authorization':True,'requires_committed_blob_verification':True,'requires_dynamic_consumer_review':True,'no_files_changed':True}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--family',required=True);parser.add_argument('--map',action='append',required=True);parser.add_argument('--custody',required=True);args=parser.parse_args()
    result=prepare(args.family,[argument_pin(x) for x in args.map],argument_pin(args.custody));print(json.dumps(result,indent=2))
    if result['search']['errors']:raise SystemExit(1)
