# SPDX-License-Identifier: Apache-2.0
"""Additive model+allFP32sidecar seal; no quantization or model execution."""
import argparse
import hashlib
import json
from pathlib import Path

HERE=Path(__file__).resolve().parent
STATIC=HERE.parent/'qwen3-reranker06-static-export-r1'
BASE=STATIC/'execution-seal-export-r1.json'
BASE_SHA='1286e3eafaee69bd52f91733cc26cb9d1e7641f89eee602d6721a0481358aeba'
def digest(path):
    with Path(path).open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def row(path):
    path=Path(path).resolve();return dict(path=str(path),sha256=digest(path),bytes=path.stat().st_size)
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--release',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if args.output.exists():raise ValueError('Fresh output seal required')
    if digest(BASE)!=BASE_SHA:raise ValueError('Actual reviewed CPUexport seal differs')
    seal=json.loads(BASE.read_bytes());original=list(seal['files']);release=json.loads(args.release.read_bytes())
    if release['action']!='focus-static512-weight8' or release['limits']!=seal['limits']:raise ValueError('Separate exact priorCPUcaps/rootrelease required')
    for name,expected in release['source_sha256'].items():
        if Path(name).name!=name or digest(HERE/name)!=expected:raise ValueError('Released weight8 source differs')
    seal['status']='ROOT_RELEASED_FOCUS_WEIGHT8_NUMERIC_ONLY' if release['status']=='ROOT_RELEASED' else 'FOCUS_WEIGHT8_PENDING_ROOT_REVIEW'
    cache=str(HERE/'disabled-pycache-weight8-r1');seal['environment']['PYTHONPYCACHEPREFIX']=cache
    seal['launch_log_dir']=str(HERE/'owned-weight8-r1');seal['focus_weight8_output']=str(HERE/'weight8-r1')
    seal['command']=[seal['file_roles']['venv_launcher']['path'],'-B','-X','pycache_prefix='+cache,str(HERE/'owned_weight8.py'),
        '--execution-seal',str(args.output.resolve()),'--execution-seal-sha256','<SEAL_SHA256>',
        '--primary-release',str(args.release.resolve()),'--primary-release-sha256',digest(args.release)]
    prior_path=STATIC/'export-r1/receipt.json';prior=json.loads(prior_path.read_bytes())
    if prior.get('complete') is not True or len(prior['rows'])!=12:raise ValueError('Complete actual CPU512numericstage required')
    paths=[HERE/name for name in release['source_sha256']]+[args.release,BASE,HERE/'weight8-freeze.json',prior_path,STATIC/'owned-export-r1/execution-receipt.json']
    for asset in prior['assets']:
        if Path(asset['name']).name!=asset['name']:raise ValueError('FP32closure asset is not local')
        path=STATIC/'export-r1'/asset['name']
        if path.stat().st_size!=asset['bytes'] or digest(path)!=asset['sha256']:raise ValueError('Actual complete FP32graph/refbytes differ')
        paths.append(path)
    indexed={item['path'].lower():item for item in original}
    for path in paths:
        item=row(path);old=indexed.get(item['path'].lower())
        if old and old!=item:raise ValueError('Refuse replacing inherited pin')
        if not old:seal['files'].append(item);indexed[item['path'].lower()]=item
    if seal['files'][:len(original)]!=original:raise ValueError('Inherited source/runtime pins changed')
    seal['focus_weight8_amendment']={'original_seal_sha256':BASE_SHA,'original_rows':len(original),'no_pin_removed_or_replaced':True,'scope':'Explicit weight8 artifact and12numericcases; no profile enrollment, native/runtime/resource-tier/quality qualification'}
    args.output.write_text(json.dumps(seal,indent=2)+'\n',encoding='utf8')
    print(json.dumps({'seal':str(args.output),'sha256':digest(args.output),'original_rows':len(original),'added_rows':len(seal['files'])-len(original),'root_released':release['status']=='ROOT_RELEASED'}))
if __name__=='__main__':main()
