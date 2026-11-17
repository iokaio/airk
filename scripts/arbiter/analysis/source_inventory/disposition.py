# SPDX-License-Identifier: Apache-2.0
"""Emit exhaustive rescue routing, not migration proof; stdout only."""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path


def lane(row):
    path = row['path']
    if row['disposition'] == 'documented_corpus_input':
        return 'corpus_evidence'
    if row['classification'] != 'remaining_implementation_candidate':
        return 'existing_identity_evidence'
    if any(s in path for s in ['/backend-research/', '/reference-code/', '/public-human-label-admission-r1/', '/selected-model-rights/', '/research-attribution']):
        return 'artifact_code_audit'
    if row['disposition'] == 'repository_snapshot_or_proposal_review':
        return 'component_review'
    if Path(path).suffix.lower() in {'.rs','.c','.cc','.cpp','.h','.hpp','.ts','.tsx','.js','.mjs','.cjs','.cs','.def'}:
        return 'fp16_readiness'
    if '/models/' in path and any(s in path.lower() for s in ['gqa', 'attention', 'training', 'lora', 'minilm', 'qwen', 'encoder', 'parent-bag', 'calibration', 'export', 'mixed-r']):
        return 'focus_review'
    if '/v190/final-plan/' in path:
        return 'component_review'
    return 'graph_emitter'


def disposition(inventory, source_sha256):
    lanes = defaultdict(list)
    for row in inventory['candidates']:
        assigned = lane(row)
        lanes[assigned].append({'path':row['path'], 'source_sha256':row['source_sha256'],
                               'bytes':row['bytes'], 'classification':row['classification'],
                               'disposition':row['disposition'], 'reason':row['disposition_reason'],
                               'status':'evidence_present_requires_commit_review' if assigned in ['existing_identity_evidence','corpus_evidence'] else 'assigned_pending_verified_disposition'})
    return {'schema':'airk.private.source-rescue-routing/1','input_sha256':source_sha256,
            'scope':inventory['roots'], 'input_complete':inventory['complete_within_declared_scope'],
            'migration_complete':False, 'routing_is_not_migration_proof':True,
            'counts':{name:{'paths':len(rows),'unique_sha256':len({r['source_sha256'] for r in rows})} for name,rows in sorted(lanes.items())},
            'lanes':dict(sorted(lanes.items()))}


def verify_disposition_row(row, repo, observe, checked_path):
    """Verify metadata pins only; never infer model or execution qualification."""
    def pin(path, digest, size=None, prefix=''):
        if not isinstance(digest,str) or len(digest)!=64 or any(c not in '0123456789abcdef' for c in digest):
            raise ValueError('invalid disposition digest')
        actual=observe(checked_path(repo,path,prefix),'sha1')
        if actual['source_sha256']!=digest:
            raise ValueError('disposition source/evidence bytes changed')
        if size is not None and (type(size) is not int or size!=actual['bytes']):
            raise ValueError('disposition byte count differs')
        return {'path':path,'sha256':digest,'bytes':actual['bytes']}
    if type(row.get('source_bytes')) is not int or row['source_bytes']<0:
        raise ValueError('explicit source byte count required')
    source=pin(row['source'],row['source_sha256'],row['source_bytes'],'artifacts/')
    if not isinstance(row.get('reason'),str) or not row['reason'].strip():
        raise ValueError('disposition reason required')
    evidence=row.get('evidence')
    if not isinstance(evidence,list) or not evidence:
        raise ValueError('explicit disposition evidence required')
    proofs=[pin(e['path'],e['sha256'],e.get('bytes')) for e in evidence]
    kind=row['disposition']
    if kind=='upstream_research_input':
        upstream=row.get('upstream',{})
        if any(not isinstance(upstream.get(k),str) or not upstream[k].strip() for k in ['repository','revision','path']):
            raise ValueError('unknown upstream provenance remains unresolved')
    elif kind=='generated_fixture':
        generated=row.get('generated',{});generator=generated['generator']
        prefix=next((p for p in ['scripts/','crates/','packages/','tools/'] if isinstance(generator.get('path'),str) and generator['path'].startswith(p)),None)
        if prefix is None:raise ValueError('fixture generator outside maintained source roots')
        pin(generator['path'],generator['sha256'],generator['bytes'],prefix)
        if generated.get('reproduced_sha256')!=source['sha256'] or not isinstance(generated.get('method'),str) or not generated['method']:
            raise ValueError('fixture exact reproduction evidence required')
    elif kind=='generated_from_maintained_typescript':
        generated=row.get('generated',{})
        compiler=generated['compiler'];version=generated['source_version']
        pin(compiler['path'],compiler['sha256'],compiler['bytes'],'packages/')
        version_pin=pin(version['path'],version['sha256'],version['bytes'],'artifacts/')
        if not isinstance(generated.get('options'),dict) or not isinstance(generated.get('method'),str) or not generated['method']:
            raise ValueError('compiler options and method required')
        if generated.get('reproduced_js_sha256')!=source['sha256']:
            raise ValueError('reproduced JavaScript digest differs')
        if version.get('maintained_path'):
            current=pin(version['maintained_path'],version['sha256'],version['bytes'],'packages/')
        elif version.get('git_commit') and version.get('git_path'):
            import re,subprocess
            commit=version['git_commit'];git_path=version['git_path']
            if re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}',commit) is None or not isinstance(git_path,str) or ':' in git_path or '\\' in git_path or any(p in ('','..','.') for p in git_path.split('/')):
                raise ValueError('invalid Git source locator')
            raw=subprocess.check_output(['git','-C',str(repo),'show',commit+':'+git_path])
            retained=checked_path(repo,version['path'],'artifacts/').read_bytes()
            if hashlib.sha256(raw).hexdigest()!=version_pin['sha256']:
                if version.get('normalization') != 'CRLF to LF only' or retained.replace(b'\r\n',b'\n')!=raw:
                    raise ValueError('Git source differs without exact declared CRLF normalization')
        else:
            raise ValueError('generated TypeScript lacks verified maintained/Git owner')
    elif kind=='generated_source_snapshot':
        owner=row.get('maintained',{})
        prefix=next((p for p in ['scripts/','crates/','packages/','tools/'] if isinstance(owner.get('path'),str) and owner['path'].startswith(p)),None)
        if prefix is None:
            raise ValueError('generated owner outside maintained source roots')
        target=pin(owner['path'],owner['sha256'],owner['bytes'],prefix)
        if target['sha256']!=source['sha256']:
            raise ValueError('generated snapshot must match maintained bytes exactly')
    else:
        raise ValueError('unknown disposition subtype')
    return {**row,'verified_source':source,'verified_evidence':proofs,'qualification':False}


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('inventory',type=Path)
    p.add_argument('--disposition-map',type=Path,action='append',default=[])
    args=p.parse_args()
    raw=args.inventory.read_bytes()
    value=json.loads(raw)
    if value.get('schema')!='airk.private.source-inventory/1':
        raise ValueError('unexpected inventory schema')
    report=disposition(value,hashlib.sha256(raw).hexdigest())
    if args.disposition_map:
        import inventory as scanner
        repo=Path(value['repository'])
        documents=[];errors=[]
        for path in args.disposition_map:
            try:
                document,identity=scanner.verified_json(path,'sha1')
                if document.get('schema')!='airk.source-disposition/1' or not isinstance(document.get('files'),list):
                    raise ValueError('unsupported disposition document')
                verified=[verify_disposition_row(row,repo,scanner.source_hashes,scanner.checked_relative) for row in document['files']]
                documents.append({'path':str(path),'sha256':identity['source_sha256'],'verified_rows':verified})
            except (OSError,ValueError,KeyError,TypeError) as error:
                errors.append({'path':str(path),'type':type(error).__name__,'message':str(error)})
        report['verified_disposition_maps']=documents
        report['disposition_errors']=errors
    print(json.dumps(report,indent=2,ensure_ascii=True))
    if report.get('disposition_errors'):
        raise SystemExit(1)


if __name__=='__main__':
    main()
