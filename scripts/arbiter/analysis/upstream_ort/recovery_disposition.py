# SPDX-License-Identifier: Apache-2.0
"""Read-only private disposition adapter for reviewed observed-source recovery."""
import argparse
import json
import project as owner

def adapt(path,sha):
    manifest,pin=owner.document(path,sha)
    if manifest.get('schema')!='airk.observed-source-recovery/1':raise ValueError('wrong recovery manifest')
    required=['scripts/arbiter/analysis/upstream_ort/recovery.py','scripts/arbiter/analysis/upstream_ort/project_patches.py','scripts/arbiter/analysis/upstream_ort/project.py']
    known={r['path']:r for r in manifest['code']}
    evidence=[pin]
    for path in required:
        expected=known[path];actual=owner.file_pin(path,expected['sha256'])
        if actual!=expected:raise ValueError('recovery helper pin differs')
        evidence.append(actual)
    evidence.append(owner.file_pin('scripts/arbiter/analysis/upstream_ort/patch_replay.py'))
    evidence.append(owner.file_pin('scripts/arbiter/analysis/upstream_ort/recovery_disposition.py'))
    rows=[];seen=set()
    for source in manifest['files']:
        if source['source'] in seen:raise ValueError('duplicate recovery source')
        seen.add(source['source'])
        row={k:source[k] for k in ['source','source_sha256','source_bytes','base','recovery']}
        row.update(disposition='observed_source_recovery',reason='Exact observed dependency delta retained as authenticated reversible byte edits; no AIRK authorship or historical execution claim.',evidence=evidence)
        rows.append(row)
    for path,pin in dict(owner.CHECKED).items():
        if owner.file_pin(path)!=pin:raise ValueError('adapter evidence changed')
    return {'schema':'airk.source-disposition/1','files':rows}

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--manifest',required=True);p.add_argument('--manifest-sha256',required=True);a=p.parse_args();print(json.dumps(adapt(a.manifest,a.manifest_sha256),indent=2))
