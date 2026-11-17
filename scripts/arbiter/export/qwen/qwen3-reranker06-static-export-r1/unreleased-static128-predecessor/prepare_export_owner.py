# SPDX-License-Identifier: Apache-2.0
"""Additive separate export seal preparation; no execution or resource release."""
import argparse
import hashlib
import json
from pathlib import Path

HERE=Path(__file__).resolve().parent
BASE=HERE.parent/'qwen3-reranker06-experiment-r1/execution-seal-reference-r1.json'
BASE_SHA='64b6c4af233452badd0bd6b66da20722b2f799657aede3bd382d1237b4c244fc'
def digest(path):
    with Path(path).open('rb') as source:return hashlib.file_digest(source,'sha256').hexdigest()
def row(path):
    path=Path(path).resolve();return dict(path=str(path),sha256=digest(path),bytes=path.stat().st_size)
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--release',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if args.output.exists():raise ValueError('Fresh exact seal output required')
    if digest(BASE)!=BASE_SHA:raise ValueError('Released actual reference seal differs')
    seal=json.loads(BASE.read_bytes());original=list(seal['files']);release=json.loads(args.release.read_bytes())
    if release['action']!='focus-static-fp32-export':raise ValueError('Distinct explicit export release required')
    for name,expected in release['source_sha256'].items():
        if Path(name).name!=name or digest(HERE/name)!=expected:raise ValueError('Released static export source differs')
    seal['status']='ROOT_RELEASED_FOCUS_STATIC_EXPORT_ONLY' if release['status']=='ROOT_RELEASED' else 'FOCUS_STATIC_EXPORT_PENDING_ROOT_REVIEW'
    seal['limits']=release['limits']
    if seal['limits']!={'wall_seconds':600,'host_committed_bytes':16*1024**3,'cuda_allocator_bytes':0,'minimum_gpu_free_mib':0,'owned_process_slots':2,'minimum_host_free_bytes':24*1024**3}:
        raise ValueError('Export resources differ from separate finite proposal')
    cache=str(HERE/'disabled-pycache-export-r1')
    seal['environment'].update(CUDA_VISIBLE_DEVICES='',PYTHONPYCACHEPREFIX=cache,OMP_NUM_THREADS='1',MKL_NUM_THREADS='1')
    seal['launch_log_dir']=str(HERE/'owned-export-r1');seal['focus_export_output']=str(HERE/'export-r1')
    seal['command']=[seal['file_roles']['venv_launcher']['path'],'-B','-X','pycache_prefix='+cache,str(HERE/'owned_export.py'),
        '--execution-seal',str(args.output.resolve()),'--execution-seal-sha256','<SEAL_SHA256>',
        '--primary-release',str(args.release.resolve()),'--primary-release-sha256',digest(args.release)]
    paths=[HERE/name for name in release['source_sha256']]+[args.release,BASE,HERE/'export-freeze.json']
    paths += [HERE.parent/'qwen3-reranker06-experiment-r1/reference-r1'/name for name in ['receipt.json','cases.npz']]
    paths += [HERE.parent/'qwen3-reranker06-experiment-r1/owned-reference-r1/execution-receipt.json']
    indexed={item['path'].lower():item for item in original}
    for path in paths:
        item=row(path);old=indexed.get(item['path'].lower())
        if old and old!=item:raise ValueError('Refuse replacing inherited input pin')
        if not old:seal['files'].append(item);indexed[item['path'].lower()]=item
    if seal['files'][:len(original)]!=original:raise ValueError('Inherited reference/runtime pin changed')
    seal['focus_static_export_amendment']={'original_seal_sha256':BASE_SHA,'original_rows':len(original),'no_pin_removed_or_replaced':True,'separate_cpu_caps_not_reference_hardware_qualification':True}
    args.output.write_text(json.dumps(seal,indent=2)+'\n',encoding='utf8')
    print(json.dumps({'seal':str(args.output),'sha256':digest(args.output),'original_rows':len(original),'added_rows':len(seal['files'])-len(original),'root_released':release['status']=='ROOT_RELEASED'}))
if __name__=='__main__':main()
