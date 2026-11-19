# SPDX-License-Identifier: Apache-2.0
"""Explicit private source custody audit; permanent inverse tests do not call it."""
import argparse
import json
from pathlib import Path
import cleanup


def audit_maps(map_paths,removal_roster_pin=None):
    removals={};pins=[]
    if removal_roster_pin:
        roster=cleanup.parse(cleanup.pinned(removal_roster_pin));pins.append(removal_roster_pin)
        if roster.get('schema')!='airk.source-cleanup-removal-roster/1':raise ValueError('exact removal-roster schema required')
        for row in roster['files']:
            source=cleanup.source_path(row['source'])
            if source in removals:raise ValueError('duplicate removal source')
            removals[source]=row['result']
    results=[];seen=set()
    for path in map_paths:
        path=Path(path)
        name=path.relative_to(cleanup.ROOT).as_posix() if path.is_absolute() else path.as_posix()
        raw=cleanup.read(name);map_pin={'path':name,'sha256':cleanup.digest(raw),'bytes':len(raw)};pins.append(map_pin)
        doc=cleanup.parse(raw)
        for row in doc['files']:
            source=cleanup.source_path(row['source'])
            if source in seen:raise ValueError('duplicate audited map source')
            seen.add(source)
            expected={'source':source,'source_sha256':row['original_sha256'],'source_bytes':row['original_bytes'],'source_map':map_pin}
            try:
                try:original=cleanup.read(source)
                except FileNotFoundError:original=None
                # Present-original checks need no repeated map read or Git process.
                status=cleanup.verify_custody(expected,original) if original is not None else cleanup.audit_original(expected,removals.get(source))
                results.append({'source':source,'status':status})
            except (ValueError,OSError,KeyError) as error:results.append({'source':source,'status':'refused','error':str(error)})
    if not results:raise ValueError('empty custody selection')
    for pin in pins:cleanup.pinned(pin)
    failed=sum(row['status']=='refused' for row in results)
    return {'schema':'airk.source-custody-audit/1','inputs':pins,'files':results,'selected':len(results),'passed':len(results)-failed,'refused':failed,'no_files_changed':True}


def custody_cli(map_paths,argv):
    parser=argparse.ArgumentParser(add_help=False)
    parser.add_argument('--audit-originals',action='store_true')
    parser.add_argument('--removal-roster')
    parser.add_argument('--removal-roster-sha256')
    parser.add_argument('--removal-roster-bytes',type=int)
    args,remaining=parser.parse_known_args(argv)
    optional=[args.removal_roster,args.removal_roster_sha256,args.removal_roster_bytes]
    if not args.audit_originals:
        if any(x is not None for x in optional):raise ValueError('removal roster requires explicit custody audit')
        return False
    if remaining:raise ValueError('custody audit accepts no test selectors or unknown flags')
    if any(x is not None for x in optional) and not all(x is not None for x in optional):raise ValueError('removal roster requires exact SHA256 and byte length')
    pin={'path':args.removal_roster,'sha256':args.removal_roster_sha256,'bytes':args.removal_roster_bytes} if args.removal_roster else None
    result=audit_maps(map_paths,pin);print(json.dumps(result,indent=2))
    if result['refused']:raise SystemExit(1)
    return True
