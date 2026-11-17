# SPDX-License-Identifier: Apache-2.0
"""Read-only reconciliation of a retained inventory with current rescue evidence."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import sys
import time
import datetime
sys.dont_write_bytecode=True
import inventory as scanner
from disposition import verify_disposition_row


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


def main():
    started=datetime.datetime.now(datetime.timezone.utc).isoformat();clock=time.monotonic()
    source_files=[Path(__file__),Path(scanner.__file__),Path(__file__).with_name('disposition.py')]
    source_pins={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in source_files}
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('inventory',type=Path)
    p.add_argument('--disposition-map',type=Path,action='append',default=[])
    args=p.parse_args();raw=args.inventory.read_bytes();old=json.loads(raw)
    if old.get('schema')!='airk.private.source-inventory/1':raise ValueError('wrong inventory schema')
    repo=Path(old['repository']);errors=[]
    committed=scanner.reachable_blobs(repo,old['head'])
    tracked_names={p.decode('utf-8').replace('\\','/') for p in scanner.git(repo,'ls-files','-z').split(b'\0') if p}
    tracked_current,tracked_observations=observe_tracked_owners(repo,old['candidates'],tracked_names,old['object_format'])
    migrations,maps=scanner.migration_index(repo,old['object_format'],set(),set(),errors)
    excluded={};documents=[]
    discovered={p for p in (repo/'scripts/arbiter').rglob('*disposition.json') if p.name in ['source-disposition.json','generated-disposition.json']}
    discovered.update(p.absolute() for p in args.disposition_map)
    for path in sorted(discovered):
        try:
            value,identity=scanner.verified_json(path,old['object_format'])
            if value.get('schema')!='airk.source-disposition/1' or not isinstance(value.get('files'),list):raise ValueError('unsupported disposition map')
            document={'path':path.relative_to(repo).as_posix(),'sha256':identity['source_sha256'],'rows':0};documents.append(document)
            for row in value['files']:
                verified=verify_disposition_row(row,repo,scanner.source_hashes,scanner.checked_relative)
                key=(row['source'],row['source_sha256'])
                if key in excluded and excluded[key]['disposition']!=row['disposition']:raise ValueError('conflicting disposition evidence')
                excluded[key]={**verified,'map':document['path'],'map_sha256':document['sha256']};document['rows']+=1
        except (OSError,ValueError,KeyError,TypeError) as error:scanner.record_error(errors,path,'disposition_map',error)
    rows=[]
    for row in old['candidates']:
        try:
            now=scanner.source_hashes(scanner.checked_relative(repo,row['path'],'artifacts/'),old['object_format'])
            if now['source_sha256']!=row['source_sha256'] or now['bytes']!=row['bytes']:raise ValueError('candidate changed since inventory')
            rows.append({'path':row['path'],'source_sha256':row['source_sha256'],'bytes':row['bytes'],**resolve(row,migrations,excluded,tracked_current,committed)})
        except (OSError,ValueError,KeyError,TypeError) as error:scanner.record_error(errors,row['path'],'candidate_recheck',error)
    counts=Counter(row['status'] for row in rows)
    for p in source_files:
        if hashlib.sha256(p.read_bytes()).hexdigest()!=source_pins[p.name]:
            scanner.record_error(errors,p,'verifier_source_changed',ValueError('verifier changed during reconciliation'))
    report={'schema':'airk.private.source-rescue-reconciliation/1','inventory':str(args.inventory),
            'inventory_sha256':hashlib.sha256(raw).hexdigest(),'scope':old['roots'],
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
