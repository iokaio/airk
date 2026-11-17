# SPDX-License-Identifier: Apache-2.0
"""Read-only narrowing of frozen non-script candidates, not disposition by name."""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import subprocess
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'source_inventory'))
import inventory as owner

ROOT=Path(__file__).resolve().parents[4]
EXTENSIONS={'.rs','.ts','.tsx','.css','.html','.htm'}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--delta',required=True,type=Path);a=p.parse_args()
    raw=a.delta.read_bytes();value=json.loads(raw)
    if value['schema']!='airk.private.broad-source-delta/1':raise ValueError('wrong frozen input schema')
    rows=[r for r in value['lanes']['component_review'] if Path(r['path']).suffix.lower() in EXTENSIONS]
    head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT).decode().strip()
    refs=subprocess.check_output(['git','for-each-ref','--format=%(refname) %(objectname)'],cwd=ROOT).decode().splitlines()
    blobs=owner.reachable_blobs(ROOT,'--all');head_blobs=owner.reachable_blobs(ROOT,head);errors=[]
    migrations,maps=owner.migration_index(ROOT,'sha1',set(),blobs,errors)
    def observe(row):
        try:
            actual=owner.source_hashes(owner.checked_relative(ROOT,row['path'],''),'sha1')
            if actual['source_sha256']!=row['source_sha256'] or actual['bytes']!=row['bytes']:raise ValueError('frozen input changed')
            if actual['source_sha256'] in migrations:return {**row,'review_status':'maintained_migration','owners':migrations[actual['source_sha256']]}
            if actual['git_blob_oid'] in blobs:return {**row,'review_status':'reachable_git_blob','head_reachable':actual['git_blob_oid'] in head_blobs,'actual':actual}
            if actual.get('normalized_text_git_blob_oid') in blobs:return {**row,'review_status':'reachable_normalized_git_blob','head_reachable':actual['normalized_text_git_blob_oid'] in head_blobs,'actual':actual}
            return {**row,'review_status':'unresolved','actual':actual}
        except (OSError,ValueError) as error:return {**row,'review_status':'error','error':str(error)}
    with ThreadPoolExecutor(max_workers=8) as pool:results=list(pool.map(observe,rows))
    print(json.dumps({'schema':'airk.private.non-script-review/1','input':{'path':a.delta.as_posix(),'sha256':hashlib.sha256(raw).hexdigest()},'head':head,'retained_refs':refs,'history_limitation':'Local refs are not necessarily delivered with this branch; no exact commit:path owner is inferred.','source_pins':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [Path(__file__).resolve(),Path(owner.__file__).resolve()]},'counts':dict(Counter(r['review_status'] for r in results)),'maps':maps,'errors':errors,'rows':results,'qualification':False},separators=(',',':')))
    if errors or any(r['review_status']=='error' for r in results):raise SystemExit(1)

if __name__=='__main__':main()
