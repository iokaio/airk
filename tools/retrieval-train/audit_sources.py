# SPDX-License-Identifier: Apache-2.0
"""Read-only closed-roster source seals with explicit observed UTC timing."""
import argparse
from datetime import datetime,timezone
import hashlib
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[2]


def audit_pass(root,manifest):
    expected={row['file']:row for row in manifest['files']};actual={}
    for path in (root/'sources').rglob('*'):
        if path.is_symlink() or path.is_junction():raise ValueError('source link: '+str(path))
        if path.is_file():actual[path.relative_to(root).as_posix()]=path
    if set(actual)!=set(expected):raise ValueError('source roster differs: '+str({'extra':sorted(set(actual)-set(expected)),'missing':sorted(set(expected)-set(actual))}))
    rows=[]
    for name,path in sorted(actual.items()):
        before=path.stat()
        with path.open('rb') as stream:observed=hashlib.file_digest(stream,'sha256').hexdigest()
        after=path.stat();wanted=expected[name]
        if before.st_size!=after.st_size or after.st_size!=wanted['bytes'] or before.st_mtime_ns!=after.st_mtime_ns or observed!=wanted['sha256']:raise ValueError('source bytes changed: '+name)
        rows.append({'file':name,'bytes':after.st_size,'sha256':observed})
    return rows


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--fixture',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--phase',choices=('current','pre-run','post-run'),default='current')
    args=parser.parse_args();output=args.output.resolve();root=args.fixture.resolve()
    if output.exists() or not output.is_relative_to(ROOT/'artifacts'):parser.error('fresh private output file required')
    source=Path(__file__).read_bytes();raw=(root/'manifest.private.json').read_bytes();sha=lambda value:hashlib.sha256(value).hexdigest()
    if sha(raw)!='2627e54241d8e961cc03c005c02f1655be364462ada16bbafdc0434cf4192a60':raise ValueError('unreviewed native source fixture manifest')
    manifest=json.loads(raw);now=lambda:datetime.now(timezone.utc).isoformat();started=now();passes=[]
    for index in range(2):
        beginning=now();rows=audit_pass(root,manifest)
        passes.append({'pass':index+1,'started_at_utc':beginning,'finished_at_utc':now(),'files':rows,'roster_and_hashes_sha256':sha(json.dumps(rows,sort_keys=True,separators=(',',':')).encode())})
    if passes[0]['roster_and_hashes_sha256']!=passes[1]['roster_and_hashes_sha256'] or Path(__file__).read_bytes()!=source or (root/'manifest.private.json').read_bytes()!=raw:raise ValueError('source/auditor/manifest drift')
    output.parent.mkdir(parents=True,exist_ok=True)
    report={'schema':'airk.private-source-tree-audit/1','requested_phase':args.phase,'timing_scope':'Actual UTC observations below; phase label does not invent timing relative to another process. Compare with retained native campaign timestamps.',
        'started_at_utc':started,'finished_at_utc':now(),'manifest_sha256':sha(raw),'runner_sha256':sha(source),'declared_files':len(manifest['files']),'actual_files':len(passes[0]['files']),'extra_files':[],'missing_files':[],'links':[],'two_current_passes_identical':True,'passes':passes,'scope':'Read-only closed roster and every original file hash; no source rewrites, model scores or qualification claim'}
    output.write_text(json.dumps(report,indent=2)+'\n');output.with_suffix('.runner.py').write_bytes(source)
    print(json.dumps({'files':report['actual_files'],'sha256':sha(output.read_bytes()),'roster_and_hashes_sha256':passes[0]['roster_and_hashes_sha256'],'started_at_utc':started,'finished_at_utc':report['finished_at_utc']}))


if __name__=='__main__':main()
