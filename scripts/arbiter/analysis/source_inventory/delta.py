# SPDX-License-Identifier: Apache-2.0
"""Route frozen broad inventory deltas; assignment never means source disposition."""
import argparse
from collections import Counter,defaultdict
import hashlib
import json
from pathlib import Path


def broad_lane(row):
    path=row['path'];suffix=Path(path).suffix.lower()
    if suffix in {'.patch','.diff'}:return 'fp16_readiness'
    if '/copies/' in path or '/tools/' in path and ('/artifacts/tools/' in '/'+path or 'ort130' in path):return 'artifact_code_audit'
    if any(p in path for p in ['ort130-x64','/one-airk-1.8.14/b/','/one-airk-1.8.14/onnxruntime-source/']):return 'platform_inventory'
    if '/models/' in path:
        if any(p in path for p in ['/backend-research/','/reference-code/','/publisher/','/dataset-research/']):return 'artifact_code_audit'
        if suffix in {'.cu','.cuh','.proto','.toml','.yaml','.yml','.html','.css','.scss','.vue','.svelte'} or any(p in path.lower() for p in ['training','lora','minilm','qwen','gqa','attention','export','encoder','parent-bag']):return 'focus_review'
        return 'graph_emitter'
    return 'component_review'


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline',required=True,type=Path)
    parser.add_argument('--inventory',required=True,type=Path,action='append')
    args=parser.parse_args();baseline_raw=args.baseline.read_bytes();baseline=json.loads(baseline_raw)
    known={(r['path'],r['source_sha256']) for r in baseline['candidates']}
    rows={};inputs=[]
    for path in args.inventory:
        raw=path.read_bytes();value=json.loads(raw)
        if value.get('schema')!='airk.private.source-inventory/1':raise ValueError('unsupported inventory')
        inputs.append({'path':str(path),'sha256':hashlib.sha256(raw).hexdigest(),'roots':value['roots'],
                       'errors':value['errors'],'excluded_count':len(value['excluded'])})
        for row in value['candidates']:
            key=(row['path'],row['source_sha256'])
            if key in known or row['classification']!='remaining_implementation_candidate':continue
            if key not in rows:rows[key]={**row,'input_reports':[]}
            rows[key]['input_reports'].append(str(path))
    lanes=defaultdict(list)
    for _,row in sorted(rows.items()):lanes[broad_lane(row)].append(row)
    print(json.dumps({'schema':'airk.private.broad-source-delta/1','baseline':{'path':str(args.baseline),'sha256':hashlib.sha256(baseline_raw).hexdigest()},
        'inputs':inputs,'counts':{name:{'paths':len(items),'unique_sha256':len({r['source_sha256'] for r in items})} for name,items in lanes.items()},
        'assignment_is_not_disposition':True,'lanes':dict(lanes),'author_rescue_complete':False},indent=2,ensure_ascii=True))


if __name__=='__main__':main()
