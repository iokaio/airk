# SPDX-License-Identifier: Apache-2.0
"""Read-only reconciliation of a retained inventory with current rescue evidence."""
import argparse
from collections import Counter
import hashlib
import json
import subprocess
from pathlib import Path
import sys
import time
import datetime
from concurrent.futures import ThreadPoolExecutor
sys.dont_write_bytecode=True
import inventory as scanner
from disposition import verify_disposition_row, parse_map_pin


def resolve(row, migrations, exclusions, tracked_current=None, committed=None):
    matches=migrations.get(row['source_sha256'],[])
    if matches:
        return {'status':'maintained_source_pin_verified','maintained_owners':matches}
    match=exclusions.get((row['path'],row['source_sha256']))
    if match:
        return {'status':match['disposition'],'evidence':match,'raw_byte_identity':match['disposition']=='generated_source_snapshot'}
    if row['disposition']=='documented_corpus_input':
        return {'status':'retained_corpus_evidence','reason':row['disposition_reason']}
    owners=(tracked_current or {}).get(row['source_sha256'],[])
    if owners:
        return {'status':'current_tracked_worktree_identity','maintained_owners':owners,
                'commit_status':'Current tracked bytes verified; staging/commit checked separately.'}
    if row.get('git_blob_oid') in (committed or set()):
        return {'status':'historical_committed_blob_identity','git_blob_oid':row['git_blob_oid'],
                'limitation':'Fresh reachable-blob proof does not establish current callers.'}
    if row.get('normalized_text_git_blob_oid') in (committed or set()):
        return {'status':'historical_normalized_text_identity','normalized_text_git_blob_oid':row['normalized_text_git_blob_oid'],
                'raw_byte_identity':False,'limitation':'Explicit CRLF normalization, not current source owner.'}
    return {'status':'unresolved','reason':'No verified maintained migration or explicit input/generated evidence.'}


def observe_tracked_owners(repo, rows, tracked_names, object_format, observe=scanner.source_hashes):
    current={};observations=[]
    for path in sorted({p for row in rows for p in row.get('tracked_worktree_matches',[])}):
        if path not in tracked_names:
            observations.append({'path':path,'status':'not_currently_tracked'});continue
        try:
            checked=scanner.checked_relative(repo,path,'')
            value=observe(checked,object_format)
            pin={'path':path,'sha256':value['source_sha256'],'bytes':value['bytes']}
            current.setdefault(value['source_sha256'],[]).append(pin)
            observations.append({**pin,'status':'current_bytes_verified'})
        except (OSError,ValueError) as error:
            observations.append({'path':path,'status':'unavailable','error':str(error)})
    return current,observations


def recheck_evidence(cache, object_format, observe=scanner.evidence_hashes):
    errors=[]
    for path,previous in cache.items():
        try:
            current=observe(Path(path),object_format)
            if current['source_sha256']!=previous['source_sha256'] or current['bytes']!=previous['bytes']:
                raise ValueError('shared evidence changed during batch')
        except (OSError,ValueError) as error:scanner.record_error(errors,path,'evidence_final_recheck',error)
    return errors


def merge_inventories(values):
    first=values[0];merged=dict(first);candidates={};errors=[];excluded=[];notes=[];roots=[]
    for value in values:
        if value.get('schema')!='airk.private.source-inventory/1':raise ValueError('wrong inventory schema')
        if any(value.get(key)!=first.get(key) for key in ['repository','object_format','head']):
            raise ValueError('inventory repository/object format/recorded HEAD differs')
        for row in value['candidates']:
            prior=candidates.get(row['path'])
            if prior is not None:
                if any(prior.get(key)!=row.get(key) for key in ['source_sha256','bytes','git_blob_oid','normalized_text_git_blob_oid']):
                    raise ValueError('same candidate path has conflicting retained identities')
                prior['tracked_worktree_matches']=sorted(set(prior.get('tracked_worktree_matches',[]))|set(row.get('tracked_worktree_matches',[])))
            else:candidates[row['path']]=dict(row)
        errors.extend(value['errors']);excluded.extend(value['excluded']);notes.extend(value['reported_prior_scan_notes']);roots.extend(value['roots'])
    merged.update(candidates=list(candidates.values()),errors=errors,excluded=excluded,reported_prior_scan_notes=notes,roots=sorted(set(roots)))
    return merged


def main():
    started=datetime.datetime.now(datetime.timezone.utc).isoformat();clock=time.monotonic()
    source_files=[Path(__file__),Path(scanner.__file__),Path(__file__).with_name('disposition.py')]
    source_pins={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in source_files}
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('inventory',type=Path)
    p.add_argument('--disposition-map',action='append',default=[],help='Pinned external map PATH=SHA256; repeat as needed')
    p.add_argument('--hash-workers',type=int,choices=[8,16],default=8)
    p.add_argument('--additional-inventory',type=Path,action='append',default=[],help='Frozen compatible inventory; merge exact candidate paths without traversal')
    args=p.parse_args();inputs=[];values=[]
    for path in [args.inventory,*args.additional_inventory]:
        raw=path.read_bytes();value=json.loads(raw);values.append(value)
        inputs.append({'path':str(path),'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw),'candidates':len(value.get('candidates',[]))})
    old=merge_inventories(values)
    repo=Path(old['repository']);errors=[]
    committed=scanner.reachable_blobs(repo,old['head'])
    tracked_names={p.decode('utf-8').replace('\\','/') for p in scanner.git(repo,'ls-files','-z').split(b'\0') if p}
    tracked_current,tracked_observations=observe_tracked_owners(repo,old['candidates'],tracked_names,old['object_format'])
    migrations,maps=scanner.migration_index(repo,old['object_format'],set(),set(),errors)
    excluded={};documents=[];evidence_cache={};git_proof_cache={}
    def observe_evidence(path,fmt):
        key=str(path)
        if key not in evidence_cache:evidence_cache[key]=scanner.evidence_hashes(path,fmt)
        return evidence_cache[key]
    discovered={p for p in (repo/'scripts/arbiter').rglob('*disposition.json') if p.name in ['source-disposition.json','generated-disposition.json']}
    explicit_pins={}
    for supplied in args.disposition_map:
        path,digest=parse_map_pin(supplied)
        if path in explicit_pins and explicit_pins[path]!=digest:raise ValueError('conflicting external map pins')
        explicit_pins[path]=digest
    discovered.update(explicit_pins)
    for path in sorted(discovered):
        try:
            value,identity=scanner.verified_json(path,old['object_format'])
            evidence_cache[str(path)]=identity
            if path in explicit_pins and identity['source_sha256']!=explicit_pins[path]:raise ValueError('external disposition document pin differs')
            if value.get('schema')!='airk.source-disposition/1' or not isinstance(value.get('files'),list):raise ValueError('unsupported disposition map')
            document={'path':path.relative_to(repo).as_posix(),'sha256':identity['source_sha256'],'rows':0};documents.append(document)
            for row in value['files']:
                verified=verify_disposition_row(row,repo,scanner.source_hashes,scanner.checked_relative,observe_evidence,git_proof_cache)
                key=(row['source'],row['source_sha256'])
                if key in excluded and excluded[key]['disposition']!=row['disposition']:raise ValueError('conflicting disposition evidence')
                excluded[key]={**verified,'map':document['path'],'map_sha256':document['sha256']};document['rows']+=1
        except (OSError,ValueError,KeyError,TypeError,subprocess.SubprocessError) as error:scanner.record_error(errors,path,'disposition_map',error)
    rows=[]
    def recheck(row):
        try:
            now=scanner.source_hashes(scanner.checked_relative(repo,row['path'],'artifacts/'),old['object_format'])
            if now['source_sha256']!=row['source_sha256'] or now['bytes']!=row['bytes']:raise ValueError('candidate changed since inventory')
            return {'path':row['path'],'source_sha256':row['source_sha256'],'bytes':row['bytes'],**resolve(row,migrations,excluded,tracked_current,committed)},None
        except (OSError,ValueError,KeyError,TypeError,subprocess.SubprocessError) as error:
            return None,{'path':row['path'],'stage':'candidate_recheck','type':type(error).__name__,'message':str(error)}
    with ThreadPoolExecutor(max_workers=args.hash_workers) as pool:
        for row,error in pool.map(recheck,old['candidates']):
            if row is not None:rows.append(row)
            if error is not None:errors.append(error)
    errors.extend(recheck_evidence(evidence_cache,old['object_format']))
    counts=Counter(row['status'] for row in rows)
    for p in source_files:
        if hashlib.sha256(p.read_bytes()).hexdigest()!=source_pins[p.name]:
            scanner.record_error(errors,p,'verifier_source_changed',ValueError('verifier changed during reconciliation'))
    report={'schema':'airk.private.source-rescue-reconciliation/1','inventory':str(args.inventory),
            'inventory_sha256':inputs[0]['sha256'],'input_inventories':inputs,'scope':old['roots'],
            'hash_workers':args.hash_workers,'shared_evidence_start_end_rechecked':len(evidence_cache),
            'started_utc':started,'elapsed_seconds':time.monotonic()-clock,
            'source_pins':source_pins,
            'input_errors':old['errors'],'input_exclusions':old['excluded'],'prior_limitations':old['reported_prior_scan_notes'],
            'counts':dict(counts),'verified_reachable_head':old['head'],'reachable_blob_count':len(committed),
            'current_tracked_owner_observations':tracked_observations,'maps':maps,'disposition_maps':documents,'errors':errors,'rows':rows,
            'zero_unclassified_accessible_candidates':not errors and not counts['unresolved'] and len(rows)==len(old['candidates']),
            'all_artifacts_or_all_embedded_code_complete':False,
            'qualification':False,'commit_status':'Must be verified separately after authorized commit.'}
    print(json.dumps(report,indent=2,ensure_ascii=True))
    return 0 if report['zero_unclassified_accessible_candidates'] else 1


if __name__=='__main__':raise SystemExit(main())
